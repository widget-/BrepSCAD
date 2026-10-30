//! Polygonal 2D profiles and boolean operations.
//!
//! Profiles are the 2D CSG layer. They are kernel-neutral: a profile is an
//! outer contour plus zero or more holes, and boolean results are flattened to
//! polygons before being handed back to the B-rep kernel for extrusion.

use brepscad_kernel_api::{BooleanOp, Transform};
use clipper2_rust::ClipType;
use clipper2_rust::clipper::inflate_paths_d;
use clipper2_rust::core::{FillRule, PathD, PathsD, PointD};
use clipper2_rust::engine_public::{ClipperD, PolyTreeD};
use clipper2_rust::offset::{EndType, JoinType};

use crate::EvalError;

pub(crate) const CIRCLE_SEGMENTS: usize = 64;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Region {
    pub(crate) outer: Vec<[f64; 2]>,
    pub(crate) holes: Vec<Vec<[f64; 2]>>,
}

pub(crate) type Profile = Vec<Region>;

pub(crate) fn square(size: [f64; 2]) -> Profile {
    let [width, height] = size;
    vec![Region {
        outer: vec![
            [-width / 2.0, -height / 2.0],
            [width / 2.0, -height / 2.0],
            [width / 2.0, height / 2.0],
            [-width / 2.0, height / 2.0],
        ],
        holes: Vec::new(),
    }]
}

#[cfg(test)]
pub(crate) fn circle(radius: f64) -> Profile {
    circle_with_segments(radius, CIRCLE_SEGMENTS)
}

pub(crate) fn circle_with_segments(radius: f64, segments: usize) -> Profile {
    let segments = segments.max(3);
    let outer = (0..segments)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / segments as f64;
            [radius * angle.cos(), radius * angle.sin()]
        })
        .collect();
    vec![Region {
        outer,
        holes: Vec::new(),
    }]
}

pub(crate) fn polygon(points: &[[f64; 2]]) -> Profile {
    let mut region = Region {
        outer: points.to_vec(),
        holes: Vec::new(),
    };
    normalize(&mut region);
    vec![region]
}

pub(crate) fn transform(profile: &Profile, transform: Transform) -> Profile {
    profile
        .iter()
        .map(|region| Region {
            outer: transform_points(&region.outer, transform),
            holes: region
                .holes
                .iter()
                .map(|hole| transform_points(hole, transform))
                .collect(),
        })
        .collect()
}

pub(crate) fn boolean(
    op: BooleanOp,
    subject: &Profile,
    clip: &Profile,
) -> Result<Profile, EvalError> {
    let mut clipper = ClipperD::new(6);
    clipper.add_subject(&to_paths(subject));
    clipper.add_clip(&to_paths(clip));

    let clip_type = match op {
        BooleanOp::Union => ClipType::Union,
        BooleanOp::Difference => ClipType::Difference,
        BooleanOp::Intersection => ClipType::Intersection,
    };
    let mut tree = PolyTreeD::new();
    let mut open = Vec::new();
    if !clipper.execute_tree(clip_type, FillRule::EvenOdd, &mut tree, &mut open) {
        return Err(EvalError::Expression(
            "2D boolean operation failed".to_owned(),
        ));
    }

    Ok(tree_to_profile(&tree))
}

/// Join style for 2D profile offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OffsetJoin {
    Round,
    Miter,
    Bevel,
}

/// Offset a profile outward for positive `distance`, inward for negative.
/// `r` offsets use round joins, or bevel joins when `chamfer` is set; `delta`
/// offsets use miter joins.
pub(crate) fn offset(
    profile: &Profile,
    distance: f64,
    join: OffsetJoin,
) -> Result<Profile, EvalError> {
    if distance == 0.0 {
        return Ok(profile.clone());
    }
    let join = match join {
        OffsetJoin::Round => JoinType::Round,
        OffsetJoin::Miter => JoinType::Miter,
        OffsetJoin::Bevel => JoinType::Bevel,
    };
    let paths = inflate_paths_d(
        &to_paths(profile),
        distance,
        join,
        EndType::Polygon,
        10.0,
        6,
        0.01,
    );
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut clipper = ClipperD::new(6);
    clipper.add_subject(&paths);
    let mut tree = PolyTreeD::new();
    let mut open = Vec::new();
    if !clipper.execute_tree(ClipType::Union, FillRule::EvenOdd, &mut tree, &mut open) {
        return Err(EvalError::Expression("2D offset cleanup failed".to_owned()));
    }
    Ok(tree_to_profile(&tree))
}

fn tree_to_profile(tree: &PolyTreeD) -> Profile {
    let mut profile = Vec::new();
    for &index in tree.nodes[0].children() {
        collect_region(tree, index, &mut profile);
    }
    profile
}

#[cfg(test)]
pub(crate) fn area(profile: &Profile) -> f64 {
    profile
        .iter()
        .map(|region| {
            signed_area(&region.outer).abs()
                - region
                    .holes
                    .iter()
                    .map(|hole| signed_area(hole).abs())
                    .sum::<f64>()
        })
        .sum()
}

pub(crate) fn to_regions(profile: &Profile) -> Vec<brepscad_kernel_api::ProfileRegion> {
    profile
        .iter()
        .map(|region| brepscad_kernel_api::ProfileRegion {
            outer: region.outer.clone(),
            holes: region.holes.clone(),
        })
        .collect()
}

fn collect_region(tree: &PolyTreeD, index: usize, profile: &mut Profile) {
    if tree.is_hole(index) {
        return;
    }

    let mut region = Region {
        outer: to_points(tree.nodes[index].polygon()),
        holes: Vec::new(),
    };
    for &child in tree.nodes[index].children() {
        region.holes.push(to_points(tree.nodes[child].polygon()));
        for &grandchild in tree.nodes[child].children() {
            collect_region(tree, grandchild, profile);
        }
    }
    normalize(&mut region);
    if signed_area(&region.outer).abs() > 1.0e-9 {
        profile.push(region);
    }
}

fn to_paths(profile: &Profile) -> PathsD {
    let mut paths = Vec::new();
    for region in profile {
        paths.push(to_path(&region.outer));
        for hole in &region.holes {
            paths.push(to_path(hole));
        }
    }
    paths
}

fn to_path(points: &[[f64; 2]]) -> PathD {
    points.iter().map(|[x, y]| PointD::new(*x, *y)).collect()
}

fn to_points(path: &PathD) -> Vec<[f64; 2]> {
    path.iter().map(|point| [point.x, point.y]).collect()
}

fn transform_points(points: &[[f64; 2]], transform: Transform) -> Vec<[f64; 2]> {
    points
        .iter()
        .map(|[x, y]| {
            let [x, y, _] = transform.apply([*x, *y, 0.0]);
            [x, y]
        })
        .collect()
}

fn normalize(region: &mut Region) {
    if signed_area(&region.outer) < 0.0 {
        region.outer.reverse();
    }
    for hole in &mut region.holes {
        if signed_area(hole) > 0.0 {
            hole.reverse();
        }
    }
}

fn signed_area(points: &[[f64; 2]]) -> f64 {
    let mut area = 0.0;
    for index in 0..points.len() {
        let [x0, y0] = points[index];
        let [x1, y1] = points[(index + 1) % points.len()];
        area += x0 * y1 - x1 * y0;
    }
    area / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn translated(profile: &Profile, x: f64, y: f64) -> Profile {
        transform(profile, Transform::from_translation([x, y, 0.0]))
    }

    #[test]
    fn square_minus_circle_leaves_a_hole() {
        let subject = square([4.0, 4.0]);
        let clip = circle(1.0);
        let result = boolean(BooleanOp::Difference, &subject, &clip).expect("difference");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].holes.len(), 1);
        let expected = 16.0 - std::f64::consts::PI;
        assert!(
            (area(&result) - expected).abs() < 0.01,
            "area = {}",
            area(&result)
        );
    }

    #[test]
    fn union_of_disjoint_squares_keeps_two_regions() {
        let a = square([1.0, 1.0]);
        let b = translated(&square([1.0, 1.0]), 3.0, 0.0);
        let result = boolean(BooleanOp::Union, &a, &b).expect("union");

        assert_eq!(result.len(), 2);
        assert!((area(&result) - 2.0).abs() < 1.0e-6);
    }

    #[test]
    fn intersection_uses_the_overlap() {
        let a = square([2.0, 2.0]);
        let b = translated(&square([2.0, 2.0]), 1.0, 0.0);
        let result = boolean(BooleanOp::Intersection, &a, &b).expect("intersection");

        assert_eq!(result.len(), 1);
        assert!(
            (area(&result) - 2.0).abs() < 1.0e-6,
            "area = {}",
            area(&result)
        );
    }

    #[test]
    fn difference_can_split_a_region() {
        let subject = square([4.0, 2.0]);
        let clip = square([0.5, 6.0]);
        let result = boolean(BooleanOp::Difference, &subject, &clip).expect("difference");

        assert_eq!(result.len(), 2);
        assert!(
            (area(&result) - 7.0).abs() < 1.0e-6,
            "area = {}",
            area(&result)
        );
    }

    #[test]
    fn rounds_an_outward_offset() {
        let result = offset(&square([2.0, 2.0]), 0.5, OffsetJoin::Round).expect("offset");

        assert_eq!(result.len(), 1);
        assert!(result[0].holes.is_empty());
        let (min, max) = bounds(&result);
        assert!((min[0] + 1.5).abs() < 1.0e-6, "min = {min:?}");
        assert!((max[0] - 1.5).abs() < 1.0e-6, "max = {max:?}");
        assert!(area(&result) > 8.5, "area = {}", area(&result));
    }

    #[test]
    fn miter_inset_shrinks_a_square() {
        let result = offset(&square([4.0, 4.0]), -0.5, OffsetJoin::Miter).expect("offset");

        assert_eq!(result.len(), 1);
        let (min, max) = bounds(&result);
        assert!((min[0] + 1.5).abs() < 1.0e-6, "min = {min:?}");
        assert!((max[0] - 1.5).abs() < 1.0e-6, "max = {max:?}");
        assert!(
            (area(&result) - 9.0).abs() < 1.0e-6,
            "area = {}",
            area(&result)
        );
    }

    #[test]
    fn inward_offset_can_empty_a_profile() {
        let result = offset(&square([1.0, 1.0]), -1.0, OffsetJoin::Round).expect("offset");
        assert!(result.is_empty() || area(&result) < 1.0e-6);
    }

    #[test]
    fn transforms_points_in_the_xy_plane() {
        let profile = translated(&square([2.0, 2.0]), 3.0, 4.0);
        let (min, max) = bounds(&profile);

        assert!((min[0] - 2.0).abs() < 1.0e-9, "min = {min:?}");
        assert!((max[0] - 4.0).abs() < 1.0e-9, "max = {max:?}");
        assert!((min[1] - 3.0).abs() < 1.0e-9);
        assert!((max[1] - 5.0).abs() < 1.0e-9);
    }

    fn bounds(profile: &Profile) -> ([f64; 2], [f64; 2]) {
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for region in profile {
            for [x, y] in &region.outer {
                min[0] = min[0].min(*x);
                min[1] = min[1].min(*y);
                max[0] = max[0].max(*x);
                max[1] = max[1].max(*y);
            }
        }
        (min, max)
    }
}
