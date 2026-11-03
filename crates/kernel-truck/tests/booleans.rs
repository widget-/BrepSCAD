//! Boolean behaviour on compound and empty shells.
//!
//! Master shapeops cannot fuse every overlapping pair of shells, so the adapter
//! keeps unfusable results as compounds and folds empty operands away. These
//! tests pin that behaviour down without depending on how tolerant shapeops is
//! for a given piece of geometry.

use brepscad_kernel_api::{BooleanOp, Kernel, Mesh, Primitive, ShapeId, Transform};
use brepscad_kernel_truck::TruckKernel;
use std::collections::HashMap;

fn cube(kernel: &mut TruckKernel, size: [f64; 3], at: [f64; 3]) -> ShapeId {
    let shape = kernel
        .primitive(Primitive::Cuboid { size })
        .expect("cube primitive");
    if at == [0.0; 3] {
        shape
    } else {
        kernel
            .transform(shape, Transform::from_translation(at))
            .expect("translate cube")
    }
}

fn mesh_bounds(mesh: &Mesh) -> ([f64; 3], [f64; 3]) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for position in &mesh.positions {
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }
    (min, max)
}

fn assert_close(actual: [f64; 3], expected: [f64; 3]) {
    for axis in 0..3 {
        assert!(
            (actual[axis] - expected[axis]).abs() < 1.0e-6,
            "axis {axis}: {actual:?} != {expected:?}"
        );
    }
}

fn signed_volume(mesh: &Mesh) -> f64 {
    mesh.indices
        .chunks_exact(3)
        .map(|triangle| {
            let a = mesh.positions[triangle[0] as usize];
            let b = mesh.positions[triangle[1] as usize];
            let c = mesh.positions[triangle[2] as usize];
            a[0] * (b[1] * c[2] - b[2] * c[1])
                + a[1] * (b[2] * c[0] - b[0] * c[2])
                + a[2] * (b[0] * c[1] - b[1] * c[0])
        })
        .sum::<f64>()
        / 6.0
}

fn welded_boundary_edges(mesh: &Mesh) -> usize {
    let mut welded: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut edges: HashMap<(u32, u32), usize> = HashMap::new();
    for triangle in mesh.indices.chunks_exact(3) {
        let mut indices = [0u32; 3];
        for (slot, index) in triangle.iter().enumerate() {
            let position = mesh.positions[*index as usize];
            let key = (
                (position[0] * 1.0e5).round() as i64,
                (position[1] * 1.0e5).round() as i64,
                (position[2] * 1.0e5).round() as i64,
            );
            let next = welded.len() as u32;
            indices[slot] = *welded.entry(key).or_insert(next);
        }
        for (start, end) in [
            (indices[0], indices[1]),
            (indices[1], indices[2]),
            (indices[2], indices[0]),
        ] {
            let edge = if start < end {
                (start, end)
            } else {
                (end, start)
            };
            *edges.entry(edge).or_default() += 1;
        }
    }
    edges.values().filter(|count| **count == 1).count()
}

fn boundary_edge_endpoints(mesh: &Mesh) -> Vec<([f64; 3], [f64; 3])> {
    let mut welded: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut points: HashMap<u32, [f64; 3]> = HashMap::new();
    let mut edges: HashMap<(u32, u32), usize> = HashMap::new();
    for triangle in mesh.indices.chunks_exact(3) {
        let mut indices = [0u32; 3];
        for (slot, index) in triangle.iter().enumerate() {
            let position = mesh.positions[*index as usize];
            let key = (
                (position[0] * 1.0e5).round() as i64,
                (position[1] * 1.0e5).round() as i64,
                (position[2] * 1.0e5).round() as i64,
            );
            let next = welded.len() as u32;
            let index = *welded.entry(key).or_insert(next);
            points.entry(index).or_insert(position);
            indices[slot] = index;
        }
        for (start, end) in [
            (indices[0], indices[1]),
            (indices[1], indices[2]),
            (indices[2], indices[0]),
        ] {
            let edge = if start < end {
                (start, end)
            } else {
                (end, start)
            };
            *edges.entry(edge).or_default() += 1;
        }
    }
    edges
        .iter()
        .filter(|(_, count)| **count == 1)
        .map(|(&(a, b), _)| (points[&a], points[&b]))
        .collect()
}

fn chamfered_box(kernel: &mut TruckKernel, size: f64, chamfer: f64, at: [f64; 3]) -> ShapeId {
    let h = size / 2.0;
    let c = chamfer;
    let mut points = Vec::new();
    for z in [-h, h] {
        points.extend([
            [h, -h + c, z],
            [h, h - c, z],
            [h - c, h, z],
            [-h + c, h, z],
            [-h, h - c, z],
            [-h, -h + c, z],
            [-h + c, -h, z],
            [h - c, -h, z],
        ]);
    }
    let mut faces = Vec::new();
    faces.push((0..8).rev().collect::<Vec<_>>());
    faces.push((8..16).collect::<Vec<_>>());
    for index in 0..8 {
        let next = (index + 1) % 8;
        faces.push(vec![index, next, 8 + next, 8 + index]);
    }
    let shape = kernel
        .primitive(Primitive::Polyhedron { points, faces })
        .expect("chamfered box");
    if at == [0.0; 3] {
        shape
    } else {
        kernel
            .transform(shape, Transform::from_translation(at))
            .expect("translate chamfered box")
    }
}

/// A sphere poking through all six faces of a cube used to spend almost all of
/// its time in a 101x101 parameter presearch, and the search's seam choice
/// decided whether the sphere patches were trimmed correctly. The two boolean
/// results must partition the cube, and the lens must have its analytic volume,
/// so this pins both the speed and the seam handling.
#[test]
fn cube_sphere_booleans_partition_the_cube() {
    let mut kernel = TruckKernel::new();
    let cube = cube(&mut kernel, [2.0; 3], [0.0; 3]);
    let sphere = kernel
        .primitive(Primitive::Sphere { radius: 1.2 })
        .expect("sphere");

    let started = std::time::Instant::now();
    let carved = kernel
        .boolean(BooleanOp::Difference, cube, sphere)
        .expect("cube minus sphere");
    let lens = kernel
        .boolean(BooleanOp::Intersection, cube, sphere)
        .expect("cube intersect sphere");
    let elapsed = started.elapsed();

    let carved_volume = signed_volume(&kernel.tessellate(carved, 0.05).expect("tessellate"));
    let lens_volume = signed_volume(&kernel.tessellate(lens, 0.05).expect("tessellate"));

    // Volume of a ball of radius 1.2 inside the cube [-1, 1]^3.
    let expected_lens = 6.3804;
    assert!(
        (lens_volume - expected_lens).abs() < 0.35,
        "lens volume = {lens_volume}, expected {expected_lens}"
    );
    assert!(
        (carved_volume + lens_volume - 8.0).abs() < 0.01,
        "carved + lens = {} != 8",
        carved_volume + lens_volume
    );
    // Ten seconds is a hundred times the current cost; the old grid search took
    // about eighteen for these two booleans in a release build.
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "sphere booleans took {elapsed:?}"
    );
}

/// Two cubes that do not touch stay separate shells.
#[test]
fn union_of_disjoint_cubes_keeps_both_shells() {
    let mut kernel = TruckKernel::new();
    let left = cube(&mut kernel, [2.0; 3], [-5.0, 0.0, 0.0]);
    let right = cube(&mut kernel, [2.0; 3], [5.0, 0.0, 0.0]);
    let union = kernel
        .boolean(BooleanOp::Union, left, right)
        .expect("disjoint union");
    let mesh = kernel.tessellate(union, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [-6.0, -1.0, -1.0]);
    assert_close(max, [6.0, 1.0, 1.0]);
}

/// A union that involves a compound keeps the existing shells instead of asking
/// shapeops to fuse every shell pair.
#[test]
fn union_with_a_compound_solid_keeps_all_shells() {
    let mut kernel = TruckKernel::new();
    let left = cube(&mut kernel, [2.0; 3], [-5.0, 0.0, 0.0]);
    let right = cube(&mut kernel, [2.0; 3], [5.0, 0.0, 0.0]);
    let compound = kernel
        .boolean(BooleanOp::Union, left, right)
        .expect("compound union");

    // A bridge that overlaps both cubes, so the result cannot stay two shells.
    let bridge = cube(&mut kernel, [12.0, 1.0, 1.0], [0.0, 0.0, 0.0]);
    let merged = kernel
        .boolean(BooleanOp::Union, compound, bridge)
        .expect("union with a compound");
    let mesh = kernel.tessellate(merged, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [-6.0, -1.0, -1.0]);
    assert_close(max, [6.0, 1.0, 1.0]);
}

/// Subtracting a solid that covers one shell of a compound removes that shell
/// and keeps the rest.
#[test]
fn difference_from_a_compound_drops_a_covered_shell() {
    let mut kernel = TruckKernel::new();
    let left = cube(&mut kernel, [2.0; 3], [-5.0, 0.0, 0.0]);
    let right = cube(&mut kernel, [2.0; 3], [5.0, 0.0, 0.0]);
    let compound = kernel
        .boolean(BooleanOp::Union, left, right)
        .expect("compound union");

    let cutter = cube(&mut kernel, [8.0; 3], [-5.0, 0.0, 0.0]);
    let cut = kernel
        .boolean(BooleanOp::Difference, compound, cutter)
        .expect("difference from a compound");
    let mesh = kernel.tessellate(cut, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [4.0, -1.0, -1.0]);
    assert_close(max, [6.0, 1.0, 1.0]);
}

/// An intersection that misses has no shells; using it as a subtraction tool
/// must return the target untouched instead of failing downstream.
#[test]
fn subtracting_an_empty_solid_returns_the_target() {
    let mut kernel = TruckKernel::new();
    let left = cube(&mut kernel, [2.0; 3], [-5.0, 0.0, 0.0]);
    let right = cube(&mut kernel, [2.0; 3], [5.0, 0.0, 0.0]);
    let empty = kernel
        .boolean(BooleanOp::Intersection, left, right)
        .expect("empty intersection");
    let empty_mesh = kernel.tessellate(empty, 0.05).expect("tessellate");
    assert_eq!(empty_mesh.triangle_count(), 0);

    let target = cube(&mut kernel, [2.0; 3], [0.0, 0.0, 0.0]);
    let result = kernel
        .boolean(BooleanOp::Difference, target, empty)
        .expect("difference by an empty solid");
    let mesh = kernel.tessellate(result, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [-1.0, -1.0, -1.0]);
    assert_close(max, [1.0, 1.0, 1.0]);
}

/// Unioning with an empty solid returns the other operand.
#[test]
fn union_with_an_empty_solid_returns_the_other_operand() {
    let mut kernel = TruckKernel::new();
    let left = cube(&mut kernel, [2.0; 3], [-5.0, 0.0, 0.0]);
    let right = cube(&mut kernel, [2.0; 3], [5.0, 0.0, 0.0]);
    let empty = kernel
        .boolean(BooleanOp::Intersection, left, right)
        .expect("empty intersection");

    let target = cube(&mut kernel, [2.0; 3], [0.0, 0.0, 0.0]);
    let result = kernel
        .boolean(BooleanOp::Union, empty, target)
        .expect("union with an empty solid");
    let mesh = kernel.tessellate(result, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [-1.0, -1.0, -1.0]);
    assert_close(max, [1.0, 1.0, 1.0]);
}

#[test]
fn coplanar_overlapping_union_keeps_both_operands() {
    let mut kernel = TruckKernel::new();
    let a = cube(&mut kernel, [2.0, 2.0, 2.0], [0.0, 0.0, 0.0]);
    let b = cube(&mut kernel, [2.0, 2.0, 2.0], [1.0, 0.0, 0.0]);
    let union = kernel.boolean(BooleanOp::Union, a, b).expect("union");
    let mesh = kernel.tessellate(union, 0.05).expect("mesh");
    let (min, max) = mesh_bounds(&mesh);

    // PR #31 imprints and sews coplanar face pairs, so this is a true fusion:
    // the union is the 3x2x2 box, volume 12. Before that the adapter could only
    // fall back to a compound (3 shells, volume 4-6).
    assert_close(min, [-1.0, -1.0, -1.0]);
    assert_close(max, [2.0, 1.0, 1.0]);
    let volume = signed_volume(&mesh);
    assert!((volume - 12.0).abs() < 0.1, "volume = {volume}");
}

/// A cutter that spans the subject in two axes leaves the slab it does not
/// reach. The classic backend can return the intersection instead, so the
/// adapter's slab recovery has to keep the untouched slab.
#[test]
fn difference_with_a_slab_cutter_keeps_the_remaining_slab() {
    let mut kernel = TruckKernel::new();
    let block = cube(&mut kernel, [10.0; 3], [0.0; 3]);
    let cutter = cube(&mut kernel, [12.0, 12.0, 7.0], [0.0, 0.0, 2.5]);
    let result = kernel
        .boolean(BooleanOp::Difference, block, cutter)
        .expect("slab difference");
    let mesh = kernel.tessellate(result, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [-5.0, -5.0, -5.0]);
    assert_close(max, [5.0, 5.0, -1.0]);
    let volume = signed_volume(&mesh);
    assert!((volume - 400.0).abs() < 1.0, "volume = {volume}");
}

/// A sub-millimetre cutter can be reduced to nothing by the backend. The
/// adapter keeps the subject rather than failing the whole model.
#[test]
fn difference_with_a_tiny_cutter_keeps_the_subject() {
    let mut kernel = TruckKernel::new();
    let block = cube(&mut kernel, [10.0; 3], [0.0; 3]);
    let sliver = cube(&mut kernel, [0.01; 3], [4.995, 4.995, 4.995]);
    let result = kernel
        .boolean(BooleanOp::Difference, block, sliver)
        .expect("tiny difference");
    let mesh = kernel.tessellate(result, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [-5.0, -5.0, -5.0]);
    assert_close(max, [5.0, 5.0, 5.0]);
    let volume = signed_volume(&mesh);
    assert!((volume - 1000.0).abs() < 1.0, "volume = {volume}");
}

/// A cutter that covers the whole subject must leave an empty solid, not the
/// subject unchanged.
#[test]
fn difference_with_a_covering_cutter_is_empty() {
    let mut kernel = TruckKernel::new();
    let block = cube(&mut kernel, [2.0; 3], [0.0; 3]);
    let cutter = cube(&mut kernel, [4.0; 3], [0.0; 3]);
    let result = kernel
        .boolean(BooleanOp::Difference, block, cutter)
        .expect("covered difference");
    let mesh = kernel.tessellate(result, 0.05).expect("tessellate");
    assert_eq!(mesh.triangle_count(), 0);
}

/// A cube minus a smaller concentric cube with the same top and bottom is a
/// closed square tube. Its coplanar top and bottom annuli exercise the classic
/// coplanar difference path.
#[test]
fn nested_coplanar_difference_is_closed() {
    let mut kernel = TruckKernel::new();
    let outer = cube(&mut kernel, [10.0; 3], [0.0; 3]);
    let inner = cube(&mut kernel, [6.0; 3], [0.0; 3]);
    let result = kernel
        .boolean(BooleanOp::Difference, outer, inner)
        .expect("nested difference");
    let mesh = kernel.tessellate(result, 0.05).expect("tessellate");

    let volume = signed_volume(&mesh);
    assert!((volume - 784.0).abs() < 1.0, "volume = {volume}");
    assert_eq!(
        welded_boundary_edges(&mesh),
        0,
        "nested difference is closed"
    );
}

/// A half-space cut on a closed cube must produce a closed half-cube with a
/// planar cut face.
#[test]
fn half_space_difference_of_a_cube_is_closed() {
    let mut kernel = TruckKernel::new();
    let block = cube(&mut kernel, [10.0; 3], [0.0; 3]);
    let cutter = cube(&mut kernel, [12.0; 3], [0.0, -6.0, 0.0]);
    let result = kernel
        .boolean(BooleanOp::Difference, block, cutter)
        .expect("half-space difference");
    let mesh = kernel.tessellate(result, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [-5.0, 0.0, -5.0]);
    assert_close(max, [5.0, 5.0, 5.0]);
    let volume = signed_volume(&mesh);
    assert!((volume - 500.0).abs() < 1.0, "volume = {volume}");
    assert_eq!(welded_boundary_edges(&mesh), 0, "half-space cut is closed");
}

/// A half-space cut on a compound of two cubes must keep both halves closed.
#[test]
fn half_space_difference_of_a_compound_is_closed() {
    let mut kernel = TruckKernel::new();
    let left = cube(&mut kernel, [2.0; 3], [-5.0, 0.0, 0.0]);
    let right = cube(&mut kernel, [2.0; 3], [5.0, 0.0, 0.0]);
    let compound = kernel
        .boolean(BooleanOp::Union, left, right)
        .expect("compound union");
    let cutter = cube(&mut kernel, [30.0, 12.0, 12.0], [0.0, -6.0, 0.0]);
    let result = kernel
        .boolean(BooleanOp::Difference, compound, cutter)
        .expect("half-space compound difference");
    let mesh = kernel.tessellate(result, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [-6.0, 0.0, -1.0]);
    assert_close(max, [6.0, 1.0, 1.0]);
    let volume = signed_volume(&mesh);
    assert!((volume - 8.0).abs() < 0.1, "volume = {volume}");
    assert_eq!(welded_boundary_edges(&mesh), 0, "compound cut is closed");
}

/// The bin-like case: an open-top cup cut by a half-space. The cup stays open
/// at the top, but every boundary edge touching the cut plane must lie on it.
#[test]
fn half_space_difference_of_an_open_cup_has_a_flat_cut() {
    let mut kernel = TruckKernel::new();
    let outer = cube(&mut kernel, [10.0; 3], [0.0; 3]);
    // Cavity cuts through the top but leaves a 2 mm floor.
    let cavity = cube(&mut kernel, [6.0, 6.0, 10.0], [0.0, 0.0, 2.0]);
    let cup = kernel
        .boolean(BooleanOp::Difference, outer, cavity)
        .expect("open cup");
    let cutter = cube(&mut kernel, [12.0; 3], [0.0, -6.0, 0.0]);
    let result = kernel
        .boolean(BooleanOp::Difference, cup, cutter)
        .expect("half-space cup difference");
    let mesh = kernel.tessellate(result, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [-5.0, 0.0, -5.0]);
    assert_close(max, [5.0, 5.0, 5.0]);
    let volume = signed_volume(&mesh);
    assert!((volume - 356.0).abs() < 1.0, "volume = {volume}");

    for (a, b) in boundary_edge_endpoints(&mesh) {
        if a[1].abs() < 1.0e-6 || b[1].abs() < 1.0e-6 {
            assert!(
                a[1].abs() < 1.0e-6 && b[1].abs() < 1.0e-6,
                "non-planar cut edge: {a:?}..{b:?}"
            );
        }
    }
    // A closed cut face has no boundary edges on the plane; if any exist they
    // must be planar.
}

/// A cup whose cavity also opens through the +X wall. The cut plane crosses the
/// side opening, so this reproduces the bin's side-feature cut topology.
#[test]
fn half_space_difference_of_a_side_opening_cup_has_a_flat_cut() {
    let mut kernel = TruckKernel::new();
    let outer = cube(&mut kernel, [10.0; 3], [0.0; 3]);
    // Cavity cuts through the +X wall and the top, leaving a 2 mm floor.
    let cavity = cube(&mut kernel, [10.0, 6.0, 10.0], [2.0, 0.0, 2.0]);
    let cup = kernel
        .boolean(BooleanOp::Difference, outer, cavity)
        .expect("side-opening cup");
    let cutter = cube(&mut kernel, [12.0; 3], [0.0, -6.0, 0.0]);
    let result = kernel
        .boolean(BooleanOp::Difference, cup, cutter)
        .expect("half-space side cup");
    let mesh = kernel.tessellate(result, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [-5.0, 0.0, -5.0]);
    assert_close(max, [5.0, 5.0, 5.0]);
    let volume = signed_volume(&mesh);
    assert!((volume - 308.0).abs() < 1.0, "volume = {volume}");

    for (a, b) in boundary_edge_endpoints(&mesh) {
        if a[1].abs() < 1.0e-6 || b[1].abs() < 1.0e-6 {
            assert!(
                a[1].abs() < 1.0e-6 && b[1].abs() < 1.0e-6,
                "non-planar cut edge: {a:?}..{b:?}"
            );
        }
    }
}

/// Two disjoint open cups unioned into a compound, then cut by a half-space.
/// This matches the bin + snap-grid shell structure more closely.
#[test]
fn half_space_difference_of_a_compound_of_open_cups_has_a_flat_cut() {
    let mut kernel = TruckKernel::new();
    let cup = |kernel: &mut TruckKernel, x: f64| {
        let outer = cube(kernel, [10.0; 3], [x, 0.0, 0.0]);
        let cavity = cube(kernel, [10.0, 6.0, 10.0], [x + 2.0, 0.0, 2.0]);
        kernel
            .boolean(BooleanOp::Difference, outer, cavity)
            .expect("side-opening cup")
    };
    let left = cup(&mut kernel, -6.0);
    let right = cup(&mut kernel, 6.0);
    let compound = kernel
        .boolean(BooleanOp::Union, left, right)
        .expect("compound cups");
    let cutter = cube(&mut kernel, [30.0, 12.0, 12.0], [0.0, -6.0, 0.0]);
    let result = kernel
        .boolean(BooleanOp::Difference, compound, cutter)
        .expect("half-space compound cups");
    let mesh = kernel.tessellate(result, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [-11.0, 0.0, -5.0]);
    assert_close(max, [11.0, 5.0, 5.0]);
    let volume = signed_volume(&mesh);
    assert!((volume - 616.0).abs() < 2.0, "volume = {volume}");

    for (a, b) in boundary_edge_endpoints(&mesh) {
        if a[1].abs() < 1.0e-6 || b[1].abs() < 1.0e-6 {
            assert!(
                a[1].abs() < 1.0e-6 && b[1].abs() < 1.0e-6,
                "non-planar cut edge: {a:?}..{b:?}"
            );
        }
    }
}

/// A cup whose cavity opens through the +Y wall, so the half-space cut plane
/// passes through the opening. This is the bin's side-snap topology in
/// miniature.
#[test]
fn half_space_difference_of_a_y_side_opening_cup_has_a_flat_cut() {
    let mut kernel = TruckKernel::new();
    let outer = cube(&mut kernel, [10.0; 3], [0.0; 3]);
    // Cavity cuts through the +Y wall and the top, leaving a 2 mm floor.
    let cavity = cube(&mut kernel, [6.0, 10.0, 10.0], [0.0, 2.0, 2.0]);
    let cup = kernel
        .boolean(BooleanOp::Difference, outer, cavity)
        .expect("y-side-opening cup");
    let cutter = cube(&mut kernel, [12.0; 3], [0.0, -6.0, 0.0]);
    let result = kernel
        .boolean(BooleanOp::Difference, cup, cutter)
        .expect("half-space y-side cup");
    let mesh = kernel.tessellate(result, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [-5.0, 0.0, -5.0]);
    assert_close(max, [5.0, 5.0, 5.0]);
    let volume = signed_volume(&mesh);
    assert!((volume - 260.0).abs() < 1.0, "volume = {volume}");

    for (a, b) in boundary_edge_endpoints(&mesh) {
        if a[1].abs() < 1.0e-6 || b[1].abs() < 1.0e-6 {
            assert!(
                a[1].abs() < 1.0e-6 && b[1].abs() < 1.0e-6,
                "non-planar cut edge: {a:?}..{b:?}"
            );
        }
    }
}

/// A cup with a notch in the +X wall centered on y=0, so the half-space cut
/// plane passes through an open side feature. This mirrors the bin's snap
/// clips crossing the cut plane.
#[test]
fn half_space_difference_of_a_notched_cup_has_a_flat_cut() {
    let mut kernel = TruckKernel::new();
    let outer = cube(&mut kernel, [10.0; 3], [0.0; 3]);
    // Cavity cuts through the top but leaves a 2 mm floor.
    let cavity = cube(&mut kernel, [6.0, 6.0, 10.0], [0.0, 0.0, 2.0]);
    let cup = kernel
        .boolean(BooleanOp::Difference, outer, cavity)
        .expect("open cup");
    // Notch in the +X wall, centered on the y=0 cut plane.
    let notch = cube(&mut kernel, [4.0, 2.0, 2.0], [3.0, 0.0, 0.0]);
    let notched = kernel
        .boolean(BooleanOp::Difference, cup, notch)
        .expect("notched cup");
    let cutter = cube(&mut kernel, [12.0; 3], [0.0, -6.0, 0.0]);
    let result = kernel
        .boolean(BooleanOp::Difference, notched, cutter)
        .expect("half-space notched cup");
    let mesh = kernel.tessellate(result, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [-5.0, 0.0, -5.0]);
    assert_close(max, [5.0, 5.0, 5.0]);
    let volume = signed_volume(&mesh);
    assert!((volume - 352.0).abs() < 1.0, "volume = {volume}");

    for (a, b) in boundary_edge_endpoints(&mesh) {
        if a[1].abs() < 1.0e-6 || b[1].abs() < 1.0e-6 {
            assert!(
                a[1].abs() < 1.0e-6 && b[1].abs() < 1.0e-6,
                "non-planar cut edge: {a:?}..{b:?}"
            );
        }
    }
}

/// A chamfered octagonal prism cut by the same half-space box as back_half.
/// The outer corner chamfer is the bin feature that reproduced the non-planar
/// cut loop.
#[test]
fn half_space_difference_of_a_chamfered_box_is_closed() {
    let mut kernel = TruckKernel::new();
    let chamfered = chamfered_box(&mut kernel, 10.0, 2.0, [0.0; 3]);
    let cutter = cube(&mut kernel, [12.0; 3], [0.0, -6.0, 0.0]);
    let result = kernel
        .boolean(BooleanOp::Difference, chamfered, cutter)
        .expect("chamfered half-space difference");
    let mesh = kernel.tessellate(result, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [-5.0, 0.0, -5.0]);
    assert_close(max, [5.0, 5.0, 5.0]);
    let volume = signed_volume(&mesh);
    assert!((volume - 460.0).abs() < 1.0, "volume = {volume}");
    assert_eq!(welded_boundary_edges(&mesh), 0, "chamfered cut is closed");
}

/// The bin's outer corner chamfer plus an open-top cavity. The half-space cut
/// crosses the chamfered walls; the cut face must remain planar.
#[test]
#[ignore = "reproduces the chamfered-cup split-edge issue; the planar-only sew is not general"]
fn half_space_difference_of_a_chamfered_cup_has_a_flat_cut() {
    let mut kernel = TruckKernel::new();
    let outer = chamfered_box(&mut kernel, 10.0, 2.0, [0.0; 3]);
    // Cavity cuts through the top but leaves a 2 mm floor.
    let cavity = cube(&mut kernel, [6.0, 6.0, 10.0], [0.0, 0.0, 2.0]);
    let cup = kernel
        .boolean(BooleanOp::Difference, outer, cavity)
        .expect("chamfered cup");
    let cutter = cube(&mut kernel, [12.0; 3], [0.0, -6.0, 0.0]);
    let result = kernel
        .boolean(BooleanOp::Difference, cup, cutter)
        .expect("chamfered half-space cup");
    let mesh = kernel.tessellate(result, 0.05).expect("tessellate");

    let (min, max) = mesh_bounds(&mesh);
    assert_close(min, [-5.0, 0.0, -5.0]);
    assert_close(max, [5.0, 5.0, 5.0]);
    let volume = signed_volume(&mesh);
    assert!((volume - 316.0).abs() < 1.0, "volume = {volume}");

    for (a, b) in boundary_edge_endpoints(&mesh) {
        if a[1].abs() < 1.0e-6 || b[1].abs() < 1.0e-6 {
            assert!(
                a[1].abs() < 1.0e-6 && b[1].abs() < 1.0e-6,
                "non-planar cut edge: {a:?}..{b:?}"
            );
        }
    }
}
