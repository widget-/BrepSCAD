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
                    return None;
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
    let poly_shell0 = shell0.triangulation(tol);
    let poly_shell1 = shell1.triangulation(tol);
    timer.mark("triangulate");
    let altshell0: AltCurveShell<C, S> =
        shell0.mapped(|x| *x, |c| Alternative::FirstType(c.clone()), Clone::clone);
    let altshell1: AltCurveShell<C, S> =
        shell1.mapped(|x| *x, |c| Alternative::FirstType(c.clone()), Clone::clone);
    timer.mark("map-shells");
    let loops_store::LoopsStoreQuadruple {
        geom_loops_store0: loops_store0,
        geom_loops_store1: loops_store1,
        ..
    } = loops_store::create_loops_stores(
        &altshell0,
        &poly_shell0,
        &altshell1,
        &poly_shell1,
        flat.then_some(tol),
        tol,
    )?;
    timer.mark("loops-stores");
    let mut cls0 = divide_face::divide_faces(&altshell0, &loops_store0, tol)?;
    let mut cls1 = divide_face::divide_faces(&altshell1, &loops_store1, tol)?;
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

    let (and, or) = (altshell_to_shell(&and0)?, altshell_to_shell(&or0)?);
    timer.mark("assemble");
    let result = match flat {
        true => Some([coplanar::sew(&and, tol)?, coplanar::sew(&or, tol)?]),
        // PATCH(BrepSCAD): an all-planar non-coplanar pair can still leave
        // split edges; run the general sew for those.
        false if planar_pair => Some([
            coplanar::sew(&and, tol).unwrap_or(and),
            coplanar::sew(&or, tol).unwrap_or(or),
        ]),
        false => Some([and, or]),
    };
    timer.mark("sew");
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
    finalize(operation, and_shell)
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
    finalize(operation, or_shell)
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
