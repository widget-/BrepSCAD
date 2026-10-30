use brepscad_eval::{MeshCache, evaluate_with_cache};
use brepscad_kernel_truck::TruckKernel;

#[test]
fn reuses_unchanged_leaf_meshes_across_parameter_edits() {
    let mut kernel = TruckKernel::new();
    let mut cache = MeshCache::new();

    let source = "difference() { cube([2, 2, 2]); cylinder(0.5, 4); }\n";
    let first = evaluate_with_cache(source, &mut kernel, 0.01, &mut cache).expect("first");
    assert_eq!(first.shapes[0].parts.len(), 2);
    assert_eq!(cache.hits(), 0);
    assert_eq!(cache.misses(), 2);

    let second = evaluate_with_cache(source, &mut kernel, 0.01, &mut cache).expect("second");
    assert_eq!(second.shapes[0].parts.len(), 2);
    assert_eq!(cache.hits(), 2);
    assert_eq!(cache.misses(), 2);

    let edited = "difference() { cube([3, 2, 2]); cylinder(0.5, 4); }\n";
    let third = evaluate_with_cache(edited, &mut kernel, 0.01, &mut cache).expect("third");
    assert_eq!(third.shapes[0].parts.len(), 2);
    assert_eq!(cache.hits(), 3);
    assert_eq!(cache.misses(), 3);
}

#[test]
fn different_transform_kinds_with_equal_numbers_do_not_collide() {
    let mut kernel = TruckKernel::new();
    let mut cache = MeshCache::new();

    let translated = "difference() { translate([0, 0, 2]) cube([2, 2, 2]); cylinder(0.5, 4); }\n";
    evaluate_with_cache(translated, &mut kernel, 0.01, &mut cache).expect("translated");
    assert_eq!(cache.misses(), 2);

    let rotated = "difference() { rotate([0, 0, 2]) cube([2, 2, 2]); cylinder(0.5, 4); }\n";
    let evaluation = evaluate_with_cache(rotated, &mut kernel, 0.01, &mut cache).expect("rotated");

    assert_eq!(cache.hits(), 1);
    assert_eq!(cache.misses(), 3);
    let min_z = evaluation.shapes[0].parts[0]
        .mesh
        .positions
        .iter()
        .map(|position| position[2])
        .fold(f64::INFINITY, f64::min);
    assert!((min_z + 1.0).abs() < 1e-6, "min_z = {min_z}");
}

#[test]
fn transform_changes_invalidate_inherited_leaves() {
    let mut kernel = TruckKernel::new();
    let mut cache = MeshCache::new();

    let source = "difference() { translate([0, 0, 0]) cube([2, 2, 2]); cylinder(0.5, 4); }\n";
    evaluate_with_cache(source, &mut kernel, 0.01, &mut cache).expect("first");
    assert_eq!(cache.hits(), 0);
    assert_eq!(cache.misses(), 2);

    let moved = "difference() { translate([1, 0, 0]) cube([2, 2, 2]); cylinder(0.5, 4); }\n";
    let evaluation = evaluate_with_cache(moved, &mut kernel, 0.01, &mut cache).expect("moved");

    assert_eq!(cache.hits(), 1);
    assert_eq!(cache.misses(), 3);
    let min_x = evaluation.shapes[0].parts[0]
        .mesh
        .positions
        .iter()
        .map(|position| position[0])
        .fold(f64::INFINITY, f64::min);
    assert!((min_x - 0.0).abs() < 1e-6, "min_x = {min_x}");
}
