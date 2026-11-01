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
