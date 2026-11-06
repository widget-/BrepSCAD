//! Classic (0.3.2) boolean pipeline -- the self-contained default backend.
//!
//! This subtree grafts the proven upstream boolean assembly from the published
//! 0.3.2 crate. It is compiled only when the marching SSI is the active backend
//! (default features); a build with an external SSI backend never touches it
//! and runs the current `integrate` pipeline unchanged. The public entry points in
//! `transversal::integrate` cfg-dispatch `and`/`or`/`difference`/
//! `symmetric_difference` here.
//!
//! Graft-boundary adaptations vs. the 0.3.2 source (all documented inline):
//! - The Alternative intersection-curve arm is a raw
//!   `IntersectionCurve<PolylineCurve<Point3>, S, S>` (0.3.2 wrapped it with
//!   parameter polylines the pipeline never consumed).
//! - `altshell_to_shell` converts that arm back to `C` via
//!   `SurfaceCurve::with_boundaries(..).into()` (the conversion the current
//!   `ShapeOpsCurve` bound provides), not `IntersectionCurve::new(..).into()`.
//! - Failures map onto the current `ShapeOpsError` (`EmptyOutputShell` for a
//!   `None` pipeline result, `InvalidOutputShell` for a rejected solid), since
//!   the current error enum has no single `Internal` variant.

mod coplanar;
mod divide_face;
mod faces_classification;
mod intersection_curve;
mod loops_store;

use super::integrate::{ShapeOpsCurve, ShapeOpsError, ShapeOpsSurface};
use crate::alternative::Alternative;
use crate::diagnostics;
use monstertruck_geometry::prelude::*;
use monstertruck_meshing::prelude::*;
use monstertruck_topology::*;
use rustc_hash::FxHashMap as HashMap;
use std::iter;

type ClassicResult<T> = std::result::Result<T, ShapeOpsError>;

type AltCurve<C, S> = Alternative<C, IntersectionCurve<PolylineCurve<Point3>, S, S>>;
type AltCurveShell<C, S> = Shell<Point3, AltCurve<C, S>, S>;

/// Convert an Alternative-curve shell back to a target-curve shell, approximating
/// each intersection-curve arm with a polyline B-spline wrapped in a
/// `SurfaceCurve` (uses the `From<SurfaceCurve<BsplineCurve<Point3>, ..>>`
/// conversion the current `ShapeOpsCurve` bound guarantees).
fn altshell_to_shell<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    altshell: &AltCurveShell<C, S>,
) -> Option<Shell<Point3, C, S>> {
    altshell.try_mapped(
        |p| Some(*p),
        |c| match c {
            Alternative::FirstType(c) => Some(c.clone()),
            Alternative::SecondType(ic) => {
                let surface0 = ic.surface0().clone();
                let surface1 = ic.surface1().clone();
                let mut points = ic.leader().as_slice().to_vec();
                points.dedup_by(|lhs, rhs| (*lhs).near(&*rhs));
                if points.len() < 2 {
                    // A degenerate intersection curve - a single point - carries no
                    // geometry, but returning `None` here discards the whole pair
                    // and every face in it. Pass a zero-length curve through
                    // instead and let the collapsed-edge pass remove it.
                    let Some(point) = points.first().copied() else {
                        return None;
                    };
                    points = vec![point, point];
                    if std::env::var_os("BREPSCAD_DEBUG_TRACE").is_some() {
                        eprintln!("TRACE altshell_to_shell: degenerate intersection curve kept");
                    }
                }
                let denominator = (points.len() - 1) as f64;
                let knot_vec = KnotVector::from(
                    iter::once(0.0)
                        .chain((0..points.len()).map(|index| index as f64 / denominator))
                        .chain(iter::once(1.0))
                        .collect::<Vec<_>>(),
                );
                let bspline = BsplineCurve::new(knot_vec, points);
                let boundary0: Option<ParameterCurve<BoundaryCurve2D, S>> = None;
                let boundary1: Option<ParameterCurve<BoundaryCurve2D, S>> = None;
                Some(
                    SurfaceCurve::with_boundaries(
                        surface0, surface1, bspline, boundary0, boundary1,
                    )
                    .into(),
                )
            }
        },
        |s| Some(s.clone()),
    )
}

/// Split one pair of shells into `[and_shell, or_shell]` (0.3.2 verbatim): build
/// intersection loops, divide faces, classify each divided face as `and`/`or`,
/// then ray-cast the remaining `unknown` faces against the other shell.
/// A point to follow through the pipeline, from `BREPSCAD_DEBUG_TRACE=x,y,z`.
///
/// Aggregates - face counts, volumes, boundary totals - cannot say *which* face
/// vanished. Following one point can: each stage reports how many faces' bounds
/// cover it, so the stage where coverage drops is the stage that lost it.
fn trace_point() -> Option<Point3> {
    let value = std::env::var("BREPSCAD_DEBUG_TRACE").ok()?;
    let parts: Vec<f64> = value
        .split(',')
        .filter_map(|part| part.trim().parse().ok())
        .collect();
    match parts.len() == 3 {
        true => Some(Point3::new(parts[0], parts[1], parts[2])),
        false => None,
    }
}

/// Whether a face's boundary bounds contain `point`.
fn face_covers<C, S>(face: &Face<Point3, C, S>, point: Point3) -> bool {
    let margin = 1.0e-6;
    let mut min = Point3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut max = Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for wire in face.boundaries() {
        for vertex in wire.vertex_iter() {
            let p = vertex.point();
            min = Point3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
            max = Point3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
        }
    }
    point.x >= min.x - margin
        && point.x <= max.x + margin
        && point.y >= min.y - margin
        && point.y <= max.y + margin
        && point.z >= min.z - margin
        && point.z <= max.z + margin
}

/// Reports how many of `shell`'s faces have bounds covering `point`.
fn trace_shell<C, S>(label: &str, shell: &Shell<Point3, C, S>, point: Point3) {
    let (mut total, mut hits) = (0usize, 0usize);
    for face in shell.face_iter() {
        total += 1;
        if face_covers(face, point) {
            hits += 1;
        }
    }
    eprintln!("TRACE {label}: {hits} of {total} faces cover {point:?}");
}

/// Reports how many of a finished solid's faces cover `point`.
///
/// Each operation keeps only one half of the pair it builds, so a face present in
/// both halves can still be discarded. This is the check that says which.
fn trace_solid<C, S>(label: &str, solid: &Solid<Point3, C, S>, point: Point3) {
    let (mut total, mut hits) = (0usize, 0usize);
    for shell in solid.boundaries() {
        for face in shell.face_iter() {
            total += 1;
            if face_covers(face, point) {
                hits += 1;
            }
        }
    }
    eprintln!("TRACE {label}: {hits} of {total} faces cover {point:?}");
}

fn process_one_pair_of_shells<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    shell0: &Shell<Point3, C, S>,
    shell1: &Shell<Point3, C, S>,
    tol: f64,
) -> Option<[Shell<Point3, C, S>; 2]> {
    if tol <= 0.0 {
        return None;
    }
    // PATCH(BrepSCAD): opt-in stage timing for the classic boolean pipeline.
    let mut timer = diagnostics::StageTimer::new(diagnostics::boolean_timing_enabled());
    if timer.enabled() {
        eprintln!(
            "[boolean] pair: faces={}x{} tol={tol}",
            shell0.face_iter().count(),
            shell1.face_iter().count()
        );
    }
    let imprinted = coplanar::imprint(shell0, shell1, tol);
    timer.mark("imprint");
    let (shell0, shell1) = match &imprinted {
        Some(found) => (&found.shell0, &found.shell1),
        None => (shell0, shell1),
    };
    let flat = imprinted.is_some();
    let planar = |shell: &Shell<Point3, C, S>| {
        shell.face_iter().all(|face| {
            matches!(
                face.surface().try_into_analytic_surface_kind(),
                Some(AnalyticSurfaceKind::Plane(_))
            )
        })
    };
    let planar_pair = planar(shell0) && planar(shell1);
    timer.mark("planar-check");
    let trace = trace_point();
    if let Some(point) = trace {
        trace_shell("input/shell0", shell0, point);
        trace_shell("input/shell1", shell1, point);
    }
    let poly_shell0 = shell0.triangulation(tol);
    let poly_shell1 = shell1.triangulation(tol);
    timer.mark("triangulate");
    let altshell0: AltCurveShell<C, S> =
        shell0.mapped(|x| *x, |c| Alternative::FirstType(c.clone()), Clone::clone);
    let altshell1: AltCurveShell<C, S> =
        shell1.mapped(|x| *x, |c| Alternative::FirstType(c.clone()), Clone::clone);
    timer.mark("map-shells");
    let stores = loops_store::create_loops_stores(
        &altshell0,
        &poly_shell0,
        &altshell1,
        &poly_shell1,
        flat.then_some(tol),
        tol,
    );
    if stores.is_none() && trace.is_some() {
        eprintln!("TRACE failed at create_loops_stores (tol {tol})");
    }
    let loops_store::LoopsStoreQuadruple {
        geom_loops_store0: loops_store0,
        geom_loops_store1: loops_store1,
        ..
    } = stores?;
    timer.mark("loops-stores");
    let divided0 = divide_face::divide_faces(&altshell0, &loops_store0, tol);
    let divided1 = divide_face::divide_faces(&altshell1, &loops_store1, tol);
    if trace.is_some() && (divided0.is_none() || divided1.is_none()) {
        eprintln!(
            "TRACE failed at divide_faces (shell0 {}, shell1 {}, tol {tol})",
            divided0.is_some(),
            divided1.is_some()
        );
    }
    let mut cls0 = divided0?;
    let mut cls1 = divided1?;
    timer.mark("divide-faces");
    let alt_ids = |shell: &Shell<Point3, C, S>,
                   alt: &AltCurveShell<C, S>,
                   marks: &HashMap<FaceId<S>, coplanar::Overlap>| {
        shell
            .face_iter()
            .zip(alt.face_iter())
            .filter_map(|(face, mapped)| marks.get(&face.id()).map(|m| (mapped.id(), *m)))
            .collect::<HashMap<_, _>>()
    };
    let (taken0, taken1) = match &imprinted {
        Some(found) => (
            cls0.take(&alt_ids(shell0, &altshell0, &found.overlap0)),
            cls1.take(&alt_ids(shell1, &altshell1, &found.overlap1)),
        ),
        None => (Vec::new(), Vec::new()),
    };
    cls0.integrate_by_component();
    cls1.integrate_by_component();
    timer.mark("integrate-components");
    // PATCH(BrepSCAD): keep every face's mesh, not only for coplanar pairs. The
    // triangulation already exists, and without it the classification below has
    // no interior point to sample.
    let meshes: HashMap<FaceId<S>, PolygonMesh> = altshell0
        .face_iter()
        .zip(poly_shell0.face_iter())
        .chain(altshell1.face_iter().zip(poly_shell1.face_iter()))
        .filter_map(|(face, poly)| Some((face.id(), poly.surface()?)))
        .collect();
    timer.mark("collect-meshes");
    // Classification samples several points on a face rather than one. A face
    // that straddles the other solid's surface has no single correct answer, and
    // a lone probe - especially a vertex, which lies on the neighbouring faces
    // too - decides it arbitrarily. Spread the samples over the face and let the
    // vote decide.
    let probes = |face: &Face<Point3, AltCurve<C, S>, S>| -> Vec<Point3> {
        let mut points: Vec<Point3> = Vec::new();
        // Not gated on `flat`: a non-coplanar pair needs an interior probe just
        // as much.
        if let Some(point) = coplanar::inner_point(face, tol) {
            points.push(point);
        }
        if let Some(mesh) = meshes.get(&face.id()) {
            let positions = mesh.positions();
            let mut triangles: Vec<[Point3; 3]> = mesh
                .faces()
                .triangle_iter()
                .map(|triangle| triangle.map(|vertex| positions[vertex.pos]))
                .collect();
            let area = |t: &[Point3; 3]| (t[1] - t[0]).cross(t[2] - t[0]).magnitude();
            triangles.sort_by(|left, right| area(right).total_cmp(&area(left)));
            for triangle in triangles.iter().take(4) {
                let [a, b, c] = *triangle;
                points.push(Point3::from_vec(
                    (a.to_vec() + b.to_vec() + c.to_vec()) / 3.0,
                ));
            }
        }
        if points.is_empty() {
            points.push(face.boundaries()[0].vertex_iter().next().unwrap().point());
        }
        let mut distinct: Vec<Point3> = Vec::new();
        for point in points {
            if !distinct.iter().any(|known| known.distance(point) <= tol) {
                distinct.push(point);
            }
        }
        distinct
    };
    // PATCH(BrepSCAD): a negated (difference) shell is inverted even when the
    // pair is not coplanar. Without the shift, the subject's outer faces are
    // classified as outside and the engine returns the tool instead of the cut.
    let inverted = |poly: &Shell<Point3, PolylineCurve<Point3>, Option<PolygonMesh>>| -> isize {
        isize::from(poly.to_polygon().volume() < 0.0)
    };
    let (shift0, shift1) = (inverted(&poly_shell0), inverted(&poly_shell1));
    let [mut and0, mut or0, unknown0] = cls0.and_or_unknown();
    unknown0.into_iter().try_for_each(|face| {
        let points = probes(&face);
        let rays = if flat { 5 } else { 1 };
        let (mut inside, mut total) = (0, 0);
        for pt in points.iter() {
            for k in 0..rays {
                let dir = hash::take_one_unit(
                    *pt + Vector3::new(k as f64 * 0.37, k as f64 * 0.11, k as f64 * 0.23),
                );
                let count = poly_shell1.iter().try_fold(0, |count, face| {
                    let poly = face.surface()?;
                    let crossed = poly.signed_crossing_faces(*pt, dir);
                    Some(
                        count
                            + if face.orientation() {
                                crossed
                            } else {
                                -crossed
                            },
                    )
                })?;
                total += 1;
                if count + shift1 >= 1 {
                    inside += 1;
                }
            }
        }
        // Provenance: the point the classification probed and the verdict it
        // reached. A face that ends up on the wrong side is explained here.
        #[cfg(feature = "provenance")]
        {
            use monstertruck_core::provenance;
            let key = face.provenance_key();
            provenance::label(key, "face");
            provenance::record(
                key,
                "classify_shell0",
                format!(
                    "points={} inside={} votes={inside}/{total}",
                    points.len(),
                    inside * 2 >= total
                ),
                &[],
            );
        }
        // A tie means the face straddles the other solid's surface, so keep it:
        // dropping is what loses material.
        if inside * 2 >= total {
            and0.push(face);
        } else {
            or0.push(face);
        }
        Some(())
    })?;
    let [mut and1, mut or1, unknown1] = cls1.and_or_unknown();
    unknown1.into_iter().try_for_each(|face| {
        let points = probes(&face);
        let rays = if flat { 5 } else { 1 };
        let (mut inside, mut total) = (0, 0);
        for pt in points.iter() {
            for k in 0..rays {
                let dir = hash::take_one_unit(
                    *pt + Vector3::new(k as f64 * 0.37, k as f64 * 0.11, k as f64 * 0.23),
                );
                let count = poly_shell0.iter().try_fold(0, |count, face| {
                    let poly = face.surface()?;
                    let crossed = poly.signed_crossing_faces(*pt, dir);
                    Some(
                        count
                            + if face.orientation() {
                                crossed
                            } else {
                                -crossed
                            },
                    )
                })?;
                total += 1;
                if count + shift0 >= 1 {
                    inside += 1;
                }
            }
        }
        // Provenance: the point the classification probed and the verdict it
        // reached. A face that ends up on the wrong side is explained here.
        #[cfg(feature = "provenance")]
        {
            use monstertruck_core::provenance;
            let key = face.provenance_key();
            provenance::label(key, "face");
            provenance::record(
                key,
                "classify_shell1",
                format!(
                    "points={} inside={} votes={inside}/{total}",
                    points.len(),
                    inside * 2 >= total
                ),
                &[],
            );
        }
        if inside * 2 >= total {
            and1.push(face);
        } else {
            or1.push(face);
        }
        Some(())
    })?;
    timer.mark("classify-unknown");
    for (face, mark) in taken0 {
        if mark == coplanar::Overlap::Same {
            or0.push(face);
        }
    }
    for (face, mark) in taken1 {
        if mark == coplanar::Overlap::Same {
            and1.push(face);
        }
    }
    and0.append(&mut and1);
    or0.append(&mut or1);

    let and_shell = altshell_to_shell(&and0);
    let or_shell = altshell_to_shell(&or0);
    if trace.is_some() && (and_shell.is_none() || or_shell.is_none()) {
        eprintln!(
            "TRACE failed at altshell_to_shell (and {}, or {})",
            and_shell.is_some(),
            or_shell.is_some()
        );
    }
    let (and, or) = (and_shell?, or_shell?);
    timer.mark("assemble");
    if let Some(point) = trace {
        trace_shell("assembled/and", &and, point);
        trace_shell("assembled/or", &or, point);
    }
    let result = match flat {
        // PATCH(BrepSCAD): a half that cannot be sewn is kept unsewn rather than
        // failing the whole pair. `sew` only merges coincident vertices and
        // edges; returning `None` threw away a half that was otherwise fine, and
        // with it the boolean. The `planar_pair` arm below already did this.
        true => Some([
            coplanar::sew(&and, tol).unwrap_or(and),
            coplanar::sew(&or, tol).unwrap_or(or),
        ]),
        // PATCH(BrepSCAD): an all-planar non-coplanar pair can still leave
        // split edges; run the general sew for those.
        false if planar_pair => Some([
            coplanar::sew(&and, tol).unwrap_or(and),
            coplanar::sew(&or, tol).unwrap_or(or),
        ]),
        false => Some([and, or]),
    };
    timer.mark("sew");
    if let Some(point) = trace {
        match &result {
            Some([sewn_and, sewn_or]) => {
                trace_shell("sewn/and", sewn_and, point);
                trace_shell("sewn/or", sewn_or, point);
            }
            None => eprintln!("TRACE sewn: the pair produced no shells"),
        }
    }
    result
}

fn finalize<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    operation: &'static str,
    shell: Shell<Point3, C, S>,
) -> ClassicResult<Solid<Point3, C, S>> {
    let boundaries = shell.connected_components();
    // PATCH(BrepSCAD): keep open boolean-derived shells; the adapter retries
    // and the tessellator renders the surviving faces.
    let _ = operation;
    Ok(Solid::new_unchecked(boundaries))
}

/// AND operation between two solids (classic backend).
pub(crate) fn and<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    solid0: &Solid<Point3, C, S>,
    solid1: &Solid<Point3, C, S>,
    tol: f64,
) -> ClassicResult<Solid<Point3, C, S>> {
    let operation = "and";
    let pair = |a: &Shell<Point3, C, S>, b: &Shell<Point3, C, S>| {
        process_one_pair_of_shells(a, b, tol).ok_or(ShapeOpsError::EmptyOutputShell { operation })
    };
    let mut iter0 = solid0.boundaries().iter();
    let mut iter1 = solid1.boundaries().iter();
    let shell0 = iter0.next().unwrap();
    let shell1 = iter1.next().unwrap();
    let [mut and_shell, _] = match pair(shell0, shell1) {
        Ok(shells) => shells,
        Err(_) => [
            coplanar::planar_and(shell0, shell1, tol)
                .ok_or(ShapeOpsError::EmptyOutputShell { operation })?,
            Shell::new(),
        ],
    };
    for shell in iter0 {
        let [res, _] = pair(&and_shell, shell)?;
        and_shell = res;
    }
    for shell in iter1 {
        let [res, _] = pair(&and_shell, shell)?;
        and_shell = res;
    }
    let solid = finalize(operation, and_shell)?;
    if let Some(point) = trace_point() {
        trace_solid("and-result", &solid, point);
    }
    Ok(solid)
}

/// OR operation between two solids (classic backend).
pub(crate) fn or<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    solid0: &Solid<Point3, C, S>,
    solid1: &Solid<Point3, C, S>,
    tol: f64,
) -> ClassicResult<Solid<Point3, C, S>> {
    let operation = "or";
    let pair = |a: &Shell<Point3, C, S>, b: &Shell<Point3, C, S>| {
        process_one_pair_of_shells(a, b, tol).ok_or(ShapeOpsError::EmptyOutputShell { operation })
    };
    let mut iter0 = solid0.boundaries().iter();
    let mut iter1 = solid1.boundaries().iter();
    let shell0 = iter0.next().unwrap();
    let shell1 = iter1.next().unwrap();
    let [_, mut or_shell] = match pair(shell0, shell1) {
        Ok(shells) => shells,
        Err(_) => [
            Shell::new(),
            coplanar::planar_or(shell0, shell1, tol)
                .ok_or(ShapeOpsError::EmptyOutputShell { operation })?,
        ],
    };
    for shell in iter0 {
        let [_, res] = pair(&or_shell, shell)?;
        or_shell = res;
    }
    for shell in iter1 {
        let [_, res] = pair(&or_shell, shell)?;
        or_shell = res;
    }
    let solid = finalize(operation, or_shell)?;
    if let Some(point) = trace_point() {
        trace_solid("or-result", &solid, point);
    }
    Ok(solid)
}

/// Difference: the region inside `solid0` but outside `solid1` (classic backend).
pub(crate) fn difference<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    solid0: &Solid<Point3, C, S>,
    solid1: &Solid<Point3, C, S>,
    tol: f64,
) -> ClassicResult<Solid<Point3, C, S>> {
    let mut neg = solid1.clone();
    neg.not();
    and(solid0, &neg, tol)
}

/// Symmetric difference (XOR): the region inside exactly one solid (classic backend).
pub(crate) fn symmetric_difference<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    solid0: &Solid<Point3, C, S>,
    solid1: &Solid<Point3, C, S>,
    tol: f64,
) -> ClassicResult<Solid<Point3, C, S>> {
    let d0 = difference(solid0, solid1, tol)?;
    let d1 = difference(solid1, solid0, tol)?;
    or(&d0, &d1, tol)
}
