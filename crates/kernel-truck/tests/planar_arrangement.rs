//! Splitting coplanar curves at their mutual intersections into a segment graph
//! whose pieces share node indices.

use monstertruck_geometry::prelude::*;

fn point(x: f64, y: f64) -> Point3 {
    Point3::new(x, y, 0.0)
}

fn bezier(control: Vec<Point3>) -> BsplineCurve<Point3> {
    let degree = control.len() - 1;
    BsplineCurve::new(KnotVector::bezier_knot(degree), control)
}

fn horizontal(y: f64) -> BsplineCurve<Point3> {
    bezier(vec![point(0.0, y), point(10.0, y)])
}

#[test]
fn two_crossing_lines_split_into_four_segments() {
    let curves = vec![
        horizontal(0.0),
        bezier(vec![point(5.0, -5.0), point(5.0, 5.0)]),
    ];
    let arrangement = arrange(&curves, 0.01);
    assert_eq!(arrangement.segments.len(), 4);
    assert_eq!(arrangement.nodes.len(), 5);
}

#[test]
fn crossing_node_is_shared_by_four_pieces() {
    let curves = vec![
        horizontal(0.0),
        bezier(vec![point(5.0, -5.0), point(5.0, 5.0)]),
    ];
    let arrangement = arrange(&curves, 0.01);
    let crossing = arrangement
        .nodes
        .iter()
        .position(|node| node.distance(point(5.0, 0.0)) < 1.0e-6)
        .expect("crossing node");
    let touching = arrangement
        .segments
        .iter()
        .filter(|segment| segment.from == crossing || segment.to == crossing)
        .count();
    assert_eq!(touching, 4);
}

#[test]
fn a_line_crossing_a_parabola_twice_splits_both() {
    let curves = vec![
        bezier(vec![point(0.0, 0.0), point(5.0, -4.0), point(10.0, 0.0)]),
        horizontal(-1.0),
    ];
    let arrangement = arrange(&curves, 0.01);
    assert_eq!(arrangement.segments.len(), 6);
    assert_eq!(arrangement.nodes.len(), 6);
}

#[test]
fn disjoint_curves_are_not_split() {
    let curves = vec![horizontal(0.0), horizontal(10.0)];
    let arrangement = arrange(&curves, 0.01);
    assert_eq!(arrangement.segments.len(), 2);
    assert_eq!(arrangement.nodes.len(), 4);
}

#[test]
fn pieces_run_end_to_end_along_their_source_curve() {
    let curves = vec![
        bezier(vec![point(0.0, 0.0), point(5.0, -4.0), point(10.0, 0.0)]),
        horizontal(-1.0),
    ];
    let arrangement = arrange(&curves, 0.01);
    for source in 0..curves.len() {
        let mut pieces: Vec<_> = arrangement
            .segments
            .iter()
            .filter(|segment| segment.source == source)
            .collect();
        // Order along the source curve: node indices carry no ordering.
        pieces.sort_by(|left, right| {
            left.curve
                .range_tuple()
                .0
                .total_cmp(&right.curve.range_tuple().0)
        });
        assert_eq!(pieces.len(), 3);
        // Consecutive pieces must share a node, so the split left no gap.
        for pair in pieces.windows(2) {
            assert!(
                pair[0].to == pair[1].from || pair[0].from == pair[1].to,
                "pieces {:?} and {:?} do not meet",
                (pair[0].from, pair[0].to),
                (pair[1].from, pair[1].to)
            );
        }
        for piece in pieces {
            let (start, end) = piece.curve.range_tuple();
            assert!(end > start);
            assert!(arrangement.nodes[piece.from].distance(piece.curve.evaluate(start)) < 0.01);
            assert!(arrangement.nodes[piece.to].distance(piece.curve.evaluate(end)) < 0.01);
        }
    }
}
