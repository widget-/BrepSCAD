//! Bounding volume hierarchy over face bounds.
//!
//! A plain median-split tree: enough to keep ray casts off the whole face list
//! without a spatial-acceleration project. Children are stored in a flat array
//! and leaves carry a range into the scene's face order.

use std::cmp::Ordering;

use crate::{Face, Ray};

pub(crate) const NO_CHILD: u32 = u32::MAX;
/// Faces per leaf. Small models stay in one node; larger ones get split.
const LEAF_SIZE: usize = 4;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Node {
    pub bounds: ([f64; 3], [f64; 3]),
    pub left: u32,
    pub right: u32,
    pub start: u32,
    pub count: u32,
}

impl Node {
    fn leaf(bounds: ([f64; 3], [f64; 3]), start: usize, count: usize) -> Self {
        Self {
            bounds,
            left: NO_CHILD,
            right: NO_CHILD,
            start: start as u32,
            count: count as u32,
        }
    }

    pub(crate) fn is_leaf(&self) -> bool {
        self.count > 0
    }
}

pub(crate) fn build(faces: &[Face]) -> (Vec<Node>, Vec<u32>) {
    let mut order: Vec<u32> = (0..faces.len() as u32).collect();
    let mut nodes = Vec::new();
    if !faces.is_empty() {
        build_node(faces, &mut order, 0, faces.len(), &mut nodes);
    }
    (nodes, order)
}

fn build_node(
    faces: &[Face],
    order: &mut [u32],
    start: usize,
    count: usize,
    nodes: &mut Vec<Node>,
) -> u32 {
    let index = nodes.len() as u32;
    let bounds = bounds_of(faces, &order[start..start + count]);
    nodes.push(Node::leaf(bounds, start, count));
    if count <= LEAF_SIZE {
        return index;
    }

    let axis = widest_axis(faces, &order[start..start + count]);
    order[start..start + count].sort_by(|left, right| {
        let left = centre(&faces[*left as usize])[axis];
        let right = centre(&faces[*right as usize])[axis];
        left.partial_cmp(&right).unwrap_or(Ordering::Equal)
    });
    let half = count / 2;
    let left = build_node(faces, order, start, half, nodes);
    let right = build_node(faces, order, start + half, count - half, nodes);

    // Interior nodes carry no payload of their own.
    nodes[index as usize].left = left;
    nodes[index as usize].right = right;
    nodes[index as usize].count = 0;
    index
}

fn bounds_of(faces: &[Face], order: &[u32]) -> ([f64; 3], [f64; 3]) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for index in order {
        let (face_min, face_max) = faces[*index as usize].bounds;
        for axis in 0..3 {
            min[axis] = min[axis].min(face_min[axis]);
            max[axis] = max[axis].max(face_max[axis]);
        }
    }
    (min, max)
}

fn centre(face: &Face) -> [f64; 3] {
    let (min, max) = face.bounds;
    [
        (min[0] + max[0]) / 2.0,
        (min[1] + max[1]) / 2.0,
        (min[2] + max[2]) / 2.0,
    ]
}

fn widest_axis(faces: &[Face], order: &[u32]) -> usize {
    let (min, max) = bounds_of(faces, order);
    let mut axis = 0;
    let mut extent = max[0] - min[0];
    for candidate in 1..3 {
        let candidate_extent = max[candidate] - min[candidate];
        if candidate_extent > extent {
            axis = candidate;
            extent = candidate_extent;
        }
    }
    axis
}

/// Entry distance of the ray into the box, or `None` when it misses.
pub(crate) fn slab_hit(bounds: &([f64; 3], [f64; 3]), ray: &Ray) -> Option<f64> {
    let (min, max) = *bounds;
    let mut enter = f64::NEG_INFINITY;
    let mut exit = f64::INFINITY;
    for axis in 0..3 {
        let origin = ray.origin[axis];
        let direction = ray.direction[axis];
        if direction.abs() < 1.0e-12 {
            if origin < min[axis] || origin > max[axis] {
                return None;
            }
            continue;
        }
        let inverse = 1.0 / direction;
        let mut near = (min[axis] - origin) * inverse;
        let mut far = (max[axis] - origin) * inverse;
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        enter = enter.max(near);
        exit = exit.min(far);
        if enter > exit {
            return None;
        }
    }
    if exit < 0.0 {
        None
    } else {
        Some(enter.max(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Surface, TrimRegion};

    fn face_at(z: f64) -> Face {
        let half = 1.0;
        Face::new(
            Surface::Plane {
                origin: [0.0, 0.0, z],
                normal: [0.0, 0.0, 1.0],
                u_axis: [1.0, 0.0, 0.0],
                v_axis: [0.0, 1.0, 0.0],
            },
            TrimRegion::new(vec![vec![
                [-half, -half],
                [half, -half],
                [half, half],
                [-half, half],
            ]]),
            ([-half, -half, z], [half, half, z]),
        )
    }

    #[test]
    fn slab_hit_reports_entry_distance() {
        let bounds = ([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]);
        let ray = Ray::new([0.0, 0.0, 5.0], [0.0, 0.0, -1.0]);
        let entry = slab_hit(&bounds, &ray).expect("hit");
        assert!((entry - 4.0).abs() < 1e-9);
    }

    #[test]
    fn slab_hit_ignores_rays_that_miss() {
        let bounds = ([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]);
        let ray = Ray::new([5.0, 0.0, 5.0], [0.0, 0.0, -1.0]);
        assert!(slab_hit(&bounds, &ray).is_none());
    }

    #[test]
    fn many_faces_are_partitioned_into_leaves() {
        let faces: Vec<_> = (0..17).map(|i| face_at(i as f64)).collect();
        let (nodes, order) = build(&faces);
        assert!(nodes.len() > 1, "expected an interior node");
        assert_eq!(order.len(), faces.len());
        assert!(nodes[0].left != NO_CHILD);
        assert!(!nodes[0].is_leaf());
    }
}
