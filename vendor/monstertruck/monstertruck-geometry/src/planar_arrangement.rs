//! A planar arrangement: coplanar curves split at their mutual intersections.
//!
//! PATCH(BrepSCAD): the planar boolean fallback decides face topology from
//! chord polylines — it clips against a sampled outline, derives hole ownership
//! from a probe, and walks a half-edge graph built from the sampled points.
//! Those are the sources of the `NotClosedWire` and `NotSimpleWire` shells that
//! `Shell::extract` rejects. This module is the foundation for replacing them:
//! it splits the curves themselves at their intersections, so the pieces meet
//! at shared nodes rather than at points that merely happen to be close.
//!
//! The input curves must be coplanar; the plane is not passed in because the
//! intersection routines work on the curves as they are, in 3D.

use crate::prelude::*;
use monstertruck_core::cgmath64::{Point2, Point3, Vector3};

/// One piece of an input curve, running between two nodes.
#[derive(Clone, Debug)]
pub struct ArrangementSegment<C> {
    /// The piece itself.
    pub curve: C,
    /// Index into [`Arrangement::nodes`] the piece starts at.
    pub from: usize,
    /// Index into [`Arrangement::nodes`] the piece ends at.
    pub to: usize,
    /// Index of the input curve this piece was cut from.
    pub source: usize,
}

/// Curves split at their mutual intersections, with endpoints welded into
/// shared nodes.
#[derive(Clone, Debug)]
pub struct Arrangement<C> {
    /// Welded node positions, shared by the pieces that meet there.
    pub nodes: Vec<Point3>,
    /// The pieces, grouped by input curve.
    pub segments: Vec<ArrangementSegment<C>>,
}

/// Splits each curve at its intersections with the others.
///
/// Two parameters of one curve count as the same cut when they evaluate to
/// points within `tol`, because parameters are not arc length. Every piece
/// endpoint is welded into [`Arrangement::nodes`] on the same rule, so pieces
/// that meet share a node index instead of relying on coordinate equality.
pub fn arrange<C>(curves: &[C], tol: f64) -> Arrangement<C>
where
    C: ParametricCurve<Point = Point3>
        + ParameterDivision1D<Point = Point3>
        + BoundedCurve
        + Cut
        + Clone,
{
    let mut cuts: Vec<Vec<f64>> = curves
        .iter()
        .map(|curve| {
            let (start, end) = curve.range_tuple();
            vec![start, end]
        })
        .collect();

    for first in 0..curves.len() {
        for second in (first + 1)..curves.len() {
            let hits = curve_curve_intersections(
                &curves[first],
                curves[first].range_tuple(),
                &curves[second],
                curves[second].range_tuple(),
                tol,
            );
            for hit in hits {
                cuts[first].push(hit.first_parameter);
                cuts[second].push(hit.second_parameter);
            }
        }
    }

    let mut nodes: Vec<Point3> = Vec::new();
    let mut segments: Vec<ArrangementSegment<C>> = Vec::new();
    for (source, curve) in curves.iter().enumerate() {
        let mut parameters = cuts[source].clone();
        parameters.sort_by(|left, right| left.total_cmp(right));
        let mut kept: Vec<f64> = Vec::new();
        for parameter in parameters {
            let point = curve.evaluate(parameter);
            if kept
                .iter()
                .any(|known| curve.evaluate(*known).distance(point) <= tol)
            {
                continue;
            }
            kept.push(parameter);
        }
        if kept.len() < 2 {
            continue;
        }

        let mut rest = curve.clone();
        let mut from = node_of(&mut nodes, curve.evaluate(kept[0]), tol);
        for window in kept.windows(2) {
            let tail = rest.cut(window[1]);
            let piece = rest.clone();
            let to = node_of(&mut nodes, curve.evaluate(window[1]), tol);
            segments.push(ArrangementSegment {
                curve: piece,
                from,
                to,
                source,
            });
            rest = tail;
            from = to;
        }
    }
    Arrangement { nodes, segments }
}

/// Index of `point` in `nodes`, appending it when it is not there yet.
fn node_of(nodes: &mut Vec<Point3>, point: Point3, tol: f64) -> usize {
    match nodes.iter().position(|known| known.distance(point) <= tol) {
        Some(index) => index,
        None => {
            nodes.push(point);
            nodes.len() - 1
        }
    }
}

/// A closed loop of arrangement pieces.
#[derive(Clone, Debug)]
pub struct Loop {
    /// `(segment index, traversed forward)` in walk order.
    pub pieces: Vec<(usize, bool)>,
    /// Signed area in the plane, positive when the loop runs counter-clockwise
    /// about the plane normal passed to [`extract_loops`].
    pub area: f64,
}

/// Walks every face of the arrangement.
///
/// At each node the outgoing pieces are ordered by tangent angle, and a walk
/// arriving along a piece leaves by the next piece clockwise from the one it
/// came in on. That traces each face exactly once, so every piece is used once
/// per side and the loops together cover the whole arrangement, including the
/// unbounded face (which comes out with the opposite area sign).
pub fn extract_loops<C>(
    arrangement: &Arrangement<C>,
    plane_normal: Vector3,
    tol: f64,
) -> Vec<Loop>
where
    C: ParametricCurve<Point = Point3, Vector = Vector3>
        + ParameterDivision1D<Point = Point3>
        + BoundedCurve,
{
    if plane_normal.magnitude2() <= 1.0e-24 {
        return Vec::new();
    }
    let normal = plane_normal.normalize();
    let helper = match normal.x.abs() < 0.9 {
        true => Vector3::unit_x(),
        false => Vector3::unit_y(),
    };
    let u = normal.cross(helper).normalize();
    let v = normal.cross(u);

    // Each piece is usable in both directions; `index ^ 1` is the other side of
    // the same piece.
    let mut uses: Vec<(usize, bool)> = Vec::with_capacity(arrangement.segments.len() * 2);
    let mut from_node: Vec<usize> = Vec::with_capacity(arrangement.segments.len() * 2);
    for (index, segment) in arrangement.segments.iter().enumerate() {
        uses.push((index, true));
        from_node.push(segment.from);
        uses.push((index, false));
        from_node.push(segment.to);
    }

    let angle = |use_index: usize| -> f64 {
        let (segment_index, forward) = uses[use_index];
        let curve = &arrangement.segments[segment_index].curve;
        let (start, end) = curve.range_tuple();
        let tangent = match forward {
            true => curve.derivative(start),
            false => -curve.derivative(end),
        };
        tangent.dot(v).atan2(tangent.dot(u))
    };

    let mut at_node: Vec<Vec<usize>> = vec![Vec::new(); arrangement.nodes.len()];
    for (index, node) in from_node.iter().enumerate() {
        at_node[*node].push(index);
    }
    for list in at_node.iter_mut() {
        list.sort_by(|left, right| angle(*left).total_cmp(&angle(*right)));
    }

    let mut used = vec![false; uses.len()];
    let mut loops = Vec::new();
    for start in 0..uses.len() {
        if used[start] {
            continue;
        }
        let mut pieces: Vec<(usize, bool)> = Vec::new();
        let mut current = start;
        loop {
            used[current] = true;
            pieces.push(uses[current]);
            let (segment_index, forward) = uses[current];
            let node = match forward {
                true => arrangement.segments[segment_index].to,
                false => arrangement.segments[segment_index].from,
            };
            let back = current ^ 1;
            let list = &at_node[node];
            let Some(position) = list.iter().position(|entry| *entry == back) else {
                break;
            };
            let next = list[(position + list.len() - 1) % list.len()];
            if next == start || used[next] {
                break;
            }
            current = next;
        }
        let area = loop_area(arrangement, &pieces, u, v, tol);
        loops.push(Loop { pieces, area });
    }
    loops
}

/// Shoelace area of the loop, sampling each piece in the plane.
fn loop_area<C>(
    arrangement: &Arrangement<C>,
    pieces: &[(usize, bool)],
    u: Vector3,
    v: Vector3,
    tol: f64,
) -> f64
where
    C: ParametricCurve<Point = Point3, Vector = Vector3>
        + ParameterDivision1D<Point = Point3>
        + BoundedCurve,
{
    let mut points: Vec<Point2> = Vec::new();
    for &(segment_index, forward) in pieces {
        let curve = &arrangement.segments[segment_index].curve;
        let (_, samples) = curve.parameter_division(curve.range_tuple(), tol);
        let mut flat: Vec<Point2> = samples
            .iter()
            .map(|point| Point2::new(point.dot(u), point.dot(v)))
            .collect();
        if !forward {
            flat.reverse();
        }
        // The next piece starts where this one ends.
        flat.pop();
        points.extend(flat);
    }
    let count = points.len();
    if count < 3 {
        return 0.0;
    }
    (0..count)
        .map(|index| {
            let here = points[index];
            let next = points[(index + 1) % count];
            here.x * next.y - next.x * here.y
        })
        .sum::<f64>()
        / 2.0
}
