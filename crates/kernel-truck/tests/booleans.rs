//! Boolean behaviour on compound and empty shells.
//!
//! Master shapeops cannot fuse every overlapping pair of shells, so the adapter
//! keeps unfusable results as compounds and folds empty operands away. These
//! tests pin that behaviour down without depending on how tolerant shapeops is
//! for a given piece of geometry.

use brepscad_kernel_api::{BooleanOp, Kernel, Mesh, Primitive, ShapeId, Transform};
use brepscad_kernel_truck::TruckKernel;

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

    // Master shapeops classifies coplanar face pairs as "inside" and returns a
    // union that stops at x = 1, so the adapter rejects the result and falls
    // back. Either way, the union has to contain both cubes.
    assert!(min[0] <= -1.0 + 1.0e-6, "min = {min:?}");
    assert!(max[0] >= 2.0 - 1.0e-6, "max = {max:?}");
}
