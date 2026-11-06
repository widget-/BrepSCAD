//! Classic (0.3.2) marching intersection-curve backend.
//!
//! Extracts the mesh-vs-mesh interference segments for a surface pair, chains
//! them into polylines, and re-samples each polyline onto both surfaces to
//! produce a cleaned [`IntersectionCurve`] leader. Ported verbatim from the
//! published 0.3.2 crate's `transversal::intersection_curve`, except that the
//! parameter-space polylines the 0.3.2 wrapper carried are dropped here: the
//! classic loops-store consumes only the 3D leader.

use monstertruck_core::cgmath64::*;
use monstertruck_geometry::prelude::*;
use monstertruck_meshing::prelude::*;

use crate::transversal::polyline_construction::construct_polylines;

type Polyline = PolylineCurve<Point3>;

/// Re-sample a raw interference polyline onto both surfaces, returning a
/// cleaned polyline intersection curve. Mirrors 0.3.2
/// Longest distance a chord may stray from the exact intersection, relative to
/// the curve's own extent.
///
/// The trim on a planar face is a polygon through the curve's samples, while
/// the surface of revolution it cuts is exact. An unrefined chord therefore
/// sits inside the exact surface by the chord's sagitta, and the wall bulges
/// past the planar trim by that amount - a visible sliver along the rim. With
/// the interference chords at a modelling tolerance, that was several pixels on
/// the built-in sample.
const CHORD_TOLERANCE: f64 = 1.0e-4;
/// Bisection depth for the refinement. Four splits make a chord 1/16 of the
/// raw interference segment, which is below `CHORD_TOLERANCE` for the
/// curvature the primitives in this backend produce.
const REFINE_DEPTH: usize = 4;

/// Insert points on the exact intersection wherever a chord strays further
/// than `tolerance` from it.
///
/// `search_triple` at a fractional index interpolates the raw leader and then
/// projects the result onto both surfaces, so the inserted point lies on the
/// exact curve. Recursion stops as soon as the chord is close enough, which
/// leaves straight intersections (plane-plane) untouched.
fn refine_chords<S>(
    curve: &IntersectionCurve<Polyline, S, S>,
    (t0, p0): (f64, Point3),
    (t1, p1): (f64, Point3),
    tolerance: f64,
    depth: usize,
    out: &mut Polyline,
) where
    S: ParametricSurface3D
        + Clone
        + SearchParameter<SurfaceParameter, Point = Point3>
        + SearchNearestParameter<SurfaceParameter, Point = Point3>,
{
    if depth == 0 {
        return;
    }
    let tm = (t0 + t1) * 0.5;
    let Some((pm, _, _)) = curve.search_triple(tm, 100) else {
        return;
    };
    if pm.distance(p0.midpoint(p1)) <= tolerance {
        return;
    }
    refine_chords(curve, (t0, p0), (tm, pm), tolerance, depth - 1, out);
    out.push(pm);
    refine_chords(curve, (tm, pm), (t1, p1), tolerance, depth - 1, out);
}

/// `IntersectionCurveWithParameters::try_new`, keeping only the 3D leader.
fn build_intersection_curve<S>(
    surface0: S,
    surface1: S,
    poly: Polyline,
) -> Option<IntersectionCurve<Polyline, S, S>>
where
    S: ParametricSurface3D
        + Clone
        + SearchParameter<SurfaceParameter, Point = Point3>
        + SearchNearestParameter<SurfaceParameter, Point = Point3>,
{
    let ic = IntersectionCurve::new(surface0.clone(), surface1.clone(), poly);
    let raw = ic.leader().clone();
    let len = raw.len();
    if len < 2 {
        return None;
    }
    let recover_triple = |point: Point3| {
        ic.search_nearest_point(point, None, None, 100).or_else(|| {
            let p0 = ic
                .surface0()
                .search_parameter(point, None, 100)
                .or_else(|| ic.surface0().search_nearest_parameter(point, None, 100))?;
            let p1 = ic
                .surface1()
                .search_parameter(point, None, 100)
                .or_else(|| ic.surface1().search_nearest_parameter(point, None, 100))?;
            let q0 = ic.surface0().evaluate(p0.0, p0.1);
            let q1 = ic.surface1().evaluate(p1.0, p1.1);
            Some((q0.midpoint(q1), p0.into(), p1.into()))
        })
    };
    let mut samples: Vec<(f64, Point3)> = Vec::with_capacity(len);
    for i in 0..len - 1 {
        let (q, _, _) = ic
            .search_triple(i as f64, 100)
            .or_else(|| recover_triple(raw[i]))?;
        samples.push((i as f64, q));
    }
    let closed = raw[0].near(&raw[len - 1]);
    let last = if closed {
        samples[0].1
    } else {
        ic.search_triple((len - 1) as f64, 100)
            .or_else(|| recover_triple(raw[len - 1]))?
            .0
    };
    samples.push(((len - 1) as f64, last));

    let mut min = Point3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut max = Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for (_, point) in &samples {
        min = Point3::new(min.x.min(point.x), min.y.min(point.y), min.z.min(point.z));
        max = Point3::new(max.x.max(point.x), max.y.max(point.y), max.z.max(point.z));
    }
    let extent = (max - min).magnitude();
    let tolerance = if extent.is_finite() && extent > 0.0 {
        CHORD_TOLERANCE * extent
    } else {
        0.0
    };

    let mut polyline = PolylineCurve(Vec::new());
    polyline.push(samples[0].1);
    for pair in samples.windows(2) {
        refine_chords(&ic, pair[0], pair[1], tolerance, REFINE_DEPTH, &mut polyline);
        polyline.push(pair[1].1);
    }
    Some(IntersectionCurve::new(surface0, surface1, polyline))
}

fn without_backtracks(mut polyline: Polyline) -> Polyline {
    let mut i = 1;
    while i + 1 < polyline.len() {
        if polyline[i - 1].near(&polyline[i + 1]) {
            polyline.remove(i);
            polyline.remove(i);
            i = i.saturating_sub(1).max(1);
        } else {
            i += 1;
        }
    }
    polyline
}

type IntersectionTuple<S> = (Polyline, IntersectionCurve<Polyline, S, S>);

/// Marching intersection curves between two trimmed surfaces, keyed by their
/// tessellated polygon meshes. Returns, per intersection branch, the raw
/// interference polyline and the cleaned polyline intersection curve.
pub(super) fn intersection_curves<S>(
    surface0: S,
    polygon0: &PolygonMesh,
    surface1: S,
    polygon1: &PolygonMesh,
) -> Option<Vec<IntersectionTuple<S>>>
where
    S: ParametricSurface3D
        + Clone
        + SearchParameter<SurfaceParameter, Point = Point3>
        + SearchNearestParameter<SurfaceParameter, Point = Point3>,
{
    let interferences = polygon0.extract_interference(polygon1);
    construct_polylines(&interferences)
        .into_iter()
        .map(without_backtracks)
        .filter(|polyline| polyline.len() >= 2)
        .map(|polyline| {
            let curve =
                build_intersection_curve(surface0.clone(), surface1.clone(), polyline.clone())?;
            Some((polyline, curve))
        })
        .collect()
}
