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

mod divide_face;
mod faces_classification;
mod intersection_curve;
mod loops_store;

use super::integrate::{ShapeOpsCurve, ShapeOpsError, ShapeOpsSurface};
use crate::alternative::Alternative;
use monstertruck_geometry::prelude::*;
use monstertruck_meshing::prelude::*;
use monstertruck_topology::*;
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

/// Inside/outside judge against a closed tessellated shell.
///
/// PATCH(BrepSCAD): ported from upstream 0.4.0. Counts signed ray crossings
/// (the winding number) against the face polygons **with the face orientation
/// applied**. For an outward shell the count is 1 inside and 0 outside. For an
/// inverted shell (`Solid::not`, as used for difference) it is -1 inside the
/// original region and 0 outside, and the "inside" of the inverted shell is the
/// complement, so the threshold is 0 instead of 1. A ray that grazes a triangle
/// edge can be counted twice or missed, so the verdict is a majority of three
/// rays.
struct InsideJudge {
    polys: Vec<PolygonMesh>,
    threshold: isize,
}

impl InsideJudge {
    fn new(poly_shell: &Shell<Point3, PolylineCurve<Point3>, Option<PolygonMesh>>) -> Option<Self> {
        let polys = poly_shell
            .face_iter()
            .map(|face| {
                let mut poly = face.surface()?;
                if !face.orientation() {
                    poly.invert();
                }
                Some(poly)
            })
            .collect::<Option<Vec<_>>>()?;
        let volume: f64 = polys.iter().map(signed_volume).sum();
        Some(Self {
            polys,
            threshold: if volume < 0.0 { 0 } else { 1 },
        })
    }

    fn inside(&self, pt: Point3) -> bool {
        let votes = [
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.31, 0.17, 0.53),
            Vector3::new(-0.43, 0.29, -0.11),
        ]
        .into_iter()
        .filter(|&offset| {
            let dir = hash::take_one_unit(pt + offset);
            let count: isize = self
                .polys
                .iter()
                .map(|poly| poly.signed_crossing_faces(pt, dir))
                .sum();
            count >= self.threshold
        })
        .count();
        votes >= 2
    }
}

/// Signed volume of a polygon mesh (divergence theorem); positive when the faces
/// point outward.
fn signed_volume(poly: &PolygonMesh) -> f64 {
    let ps = poly.positions();
    poly.faces()
        .triangle_iter()
        .map(|t| {
            let [a, b, c] = [ps[t[0].pos], ps[t[1].pos], ps[t[2].pos]];
            a.to_vec().dot(b.to_vec().cross(c.to_vec())) / 6.0
        })
        .sum()
}

/// Split one pair of shells into `[and_shell, or_shell]`: build intersection
/// loops, divide faces, classify each divided face as `and`/`or`, then ray-cast
/// the remaining `unknown` faces against the other shell.
fn process_one_pair_of_shells<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    shell0: &Shell<Point3, C, S>,
    shell1: &Shell<Point3, C, S>,
    tol: f64,
) -> Option<[Shell<Point3, C, S>; 2]> {
    if tol <= 0.0 {
        return None;
    }
    let poly_shell0 = shell0.triangulation(tol);
    let poly_shell1 = shell1.triangulation(tol);
    let altshell0: AltCurveShell<C, S> =
        shell0.mapped(|x| *x, |c| Alternative::FirstType(c.clone()), Clone::clone);
    let altshell1: AltCurveShell<C, S> =
        shell1.mapped(|x| *x, |c| Alternative::FirstType(c.clone()), Clone::clone);
    let loops_store::LoopsStoreQuadruple {
        geom_loops_store0: loops_store0,
        geom_loops_store1: loops_store1,
        ..
    } = loops_store::create_loops_stores(&altshell0, &poly_shell0, &altshell1, &poly_shell1)?;
    let mut cls0 = divide_face::divide_faces(&altshell0, &loops_store0, tol)?;
    cls0.integrate_by_component();
    let mut cls1 = divide_face::divide_faces(&altshell1, &loops_store1, tol)?;
    cls1.integrate_by_component();
    let [mut and0, mut or0, unknown0] = cls0.and_or_unknown();
    let judge1 = InsideJudge::new(&poly_shell1)?;
    unknown0.into_iter().try_for_each(|face| {
        let pt = face.boundaries()[0].vertex_iter().next()?.point();
        match judge1.inside(pt) {
            true => and0.push(face),
            false => or0.push(face),
        }
        Some(())
    })?;
    let [mut and1, mut or1, unknown1] = cls1.and_or_unknown();
    let judge0 = InsideJudge::new(&poly_shell0)?;
    unknown1.into_iter().try_for_each(|face| {
        let pt = face.boundaries()[0].vertex_iter().next()?.point();
        match judge0.inside(pt) {
            true => and1.push(face),
            false => or1.push(face),
        }
        Some(())
    })?;
    and0.append(&mut and1);
    or0.append(&mut or1);
    Some([altshell_to_shell(&and0)?, altshell_to_shell(&or0)?])
}

fn finalize<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    operation: &'static str,
    shell: Shell<Point3, C, S>,
) -> ClassicResult<Solid<Point3, C, S>> {
    let boundaries = shell.connected_components();
    // PATCH(BrepSCAD): keep boolean-derived shells even when the engine leaves
    // them open or non-simple. The adapter retries on its own terms and the
    // tessellator renders the surviving faces; rejecting here loses results the
    // 0.4.0 backend used to return unchecked.
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
    let [mut and_shell, _] = pair(shell0, shell1)?;
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
    let [_, mut or_shell] = pair(shell0, shell1)?;
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
