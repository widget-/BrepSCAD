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

fn square(min: f64, max: f64) -> Vec<BsplineCurve<Point3>> {
    vec![
        bezier(vec![point(min, min), point(max, min)]),
        bezier(vec![point(max, min), point(max, max)]),
        bezier(vec![point(max, max), point(min, max)]),
        bezier(vec![point(min, max), point(min, min)]),
    ]
}

fn areas(loops: &[Loop]) -> Vec<f64> {
    let mut areas: Vec<f64> = loops.iter().map(|item| item.area).collect();
    areas.sort_by(|left, right| left.total_cmp(right));
    areas
}

#[test]
fn a_square_cut_by_a_line_has_two_halves() {
    let mut curves = square(0.0, 10.0);
    curves.push(bezier(vec![point(5.0, 0.0), point(5.0, 10.0)]));
    let arrangement = arrange(&curves, 0.01);
    let loops = extract_loops(&arrangement, Vector3::unit_z(), 0.01);
    let areas = areas(&loops);
    assert_eq!(areas.len(), 3, "two halves plus the outside: {areas:?}");
    assert!((areas[0] + 100.0).abs() < 0.1, "outside: {areas:?}");
    assert!((areas[1] - 50.0).abs() < 0.1, "half: {areas:?}");
    assert!((areas[2] - 50.0).abs() < 0.1, "half: {areas:?}");
}

#[test]
fn every_piece_is_walked_once_per_side() {
    let mut curves = square(0.0, 10.0);
    curves.push(bezier(vec![point(5.0, 0.0), point(5.0, 10.0)]));
    let arrangement = arrange(&curves, 0.01);
    let loops = extract_loops(&arrangement, Vector3::unit_z(), 0.01);
    let walked: usize = loops.iter().map(|item| item.pieces.len()).sum();
    assert_eq!(walked, arrangement.segments.len() * 2);
}

#[test]
fn an_unconnected_inner_square_comes_out_as_a_hole() {
    let mut curves = square(0.0, 10.0);
    curves.extend(square(3.0, 7.0));
    let arrangement = arrange(&curves, 0.01);
    let loops = extract_loops(&arrangement, Vector3::unit_z(), 0.01);
    let areas = areas(&loops);
    assert_eq!(areas.len(), 4, "outer, hole, island, outside: {areas:?}");
    assert!((areas[0] + 100.0).abs() < 0.1, "outside: {areas:?}");
    assert!((areas[1] + 16.0).abs() < 0.1, "hole: {areas:?}");
    assert!((areas[2] - 16.0).abs() < 0.1, "island: {areas:?}");
    assert!((areas[3] - 100.0).abs() < 0.1, "outer: {areas:?}");
}

#[test]
fn a_square_cut_by_a_line_has_no_holes() {
    let mut curves = square(0.0, 10.0);
    curves.push(bezier(vec![point(5.0, 0.0), point(5.0, 10.0)]));
    let arrangement = arrange(&curves, 0.01);
    let loops = extract_loops(&arrangement, Vector3::unit_z(), 0.01);
    let regions = nest_loops(&arrangement, &loops, Vector3::unit_z(), 0.01);
    assert_eq!(regions.len(), 2, "the two halves");
    assert!(regions.iter().all(|region| region.holes.is_empty()));
}

#[test]
fn an_inner_square_becomes_a_hole_of_the_outer() {
    let mut curves = square(0.0, 10.0);
    curves.extend(square(3.0, 7.0));
    let arrangement = arrange(&curves, 0.01);
    let loops = extract_loops(&arrangement, Vector3::unit_z(), 0.01);
    let regions = nest_loops(&arrangement, &loops, Vector3::unit_z(), 0.01);
    assert_eq!(regions.len(), 2, "the annulus and the island");

    let outer = regions
        .iter()
        .find(|region| (loops[region.outer].area - 100.0).abs() < 0.1)
        .expect("outer region");
    assert_eq!(outer.holes.len(), 1);
    assert!((loops[outer.holes[0]].area + 16.0).abs() < 0.1);

    let island = regions
        .iter()
        .find(|region| (loops[region.outer].area - 16.0).abs() < 0.1)
        .expect("island region");
    assert!(island.holes.is_empty());

    // The unbounded face is a clockwise copy of the outermost boundary and must
    // not be adopted as a hole by the boundary it lies on.
    for region in &regions {
        for hole in &region.holes {
            assert!(
                (loops[*hole].area + 100.0).abs() > 0.1,
                "unbounded face adopted as a hole"
            );
        }
    }
}
