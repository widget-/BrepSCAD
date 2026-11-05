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
use monstertruck_core::cgmath64::Point3;

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
