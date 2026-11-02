mod common;

use brepscad_eval::evaluate;
use brepscad_kernel_truck::TruckKernel;
use common::signed_volume;

fn volume(source: &str) -> f64 {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.001).expect("evaluate");
    signed_volume(&evaluation.shapes[0].mesh)
}

#[test]
fn fn_8_makes_an_octagonal_cylinder() {
    let actual = volume("cylinder(r = 1, h = 2, $fn = 8);");
    let expected = 8.0 * std::f64::consts::FRAC_PI_4.sin();

    assert!(
        (actual - expected).abs() < expected * 0.01,
        "volume = {actual}"
    );
}

#[test]
fn fn_cutoff_switches_to_analytic_at_33() {
    let faceted = volume("cylinder(r = 1, h = 2, $fn = 32);");
    let analytic = volume("cylinder(r = 1, h = 2, $fn = 33);");
    let expected = 2.0 * std::f64::consts::PI;

    assert!(
        (faceted - 32.0 * (std::f64::consts::PI / 16.0).sin()).abs() < 1.0e-3,
        "faceted volume = {faceted}"
    );
    assert!(
        (analytic - expected).abs() < expected * 0.01,
        "analytic volume = {analytic}"
    );
    assert!(analytic > faceted);
}

#[test]
fn fn_1_clamps_to_three_segments() {
    let actual = volume("cylinder(r = 1, h = 1, $fn = 1);");
    let expected = 3.0 * (2.0 * std::f64::consts::PI / 3.0).sin() / 2.0;

    assert!(
        (actual - expected).abs() < expected * 0.01,
        "volume = {actual}"
    );
}

#[test]
fn fn_applies_to_circle_extrusions() {
    let actual = volume("linear_extrude(1) circle(r = 1, $fn = 8);");
    let expected = 4.0 * std::f64::consts::FRAC_PI_4.sin();

    assert!(
        (actual - expected).abs() < expected * 0.01,
        "volume = {actual}"
    );
}

#[test]
fn fn_inherits_through_linear_extrude() {
    let actual = volume("linear_extrude(1, $fn = 8) circle(r = 1);");
    let expected = 4.0 * std::f64::consts::FRAC_PI_4.sin();

    assert!(
        (actual - expected).abs() < expected * 0.01,
        "volume = {actual}"
    );
}

#[test]
fn fn_inherits_through_transform() {
    let actual = volume("translate([1, 0, 0], $fn = 8) cylinder(r = 1, h = 1);");
    let expected = 4.0 * std::f64::consts::FRAC_PI_4.sin();

    assert!(
        (actual - expected).abs() < expected * 0.01,
        "volume = {actual}"
    );
}

#[test]
fn fn_does_not_change_spheres() {
    // `$fn` on a sphere is a smoothness knob in the libraries we checked, never
    // a request for a polyhedron, so it no longer reaches the geometry at all.
    let coarse = volume("sphere(r = 1, $fn = 8);");
    let default = volume("sphere(r = 1);");
    let expected = 4.0 / 3.0 * std::f64::consts::PI;

    assert!(
        (coarse - default).abs() < 1.0e-9,
        "coarse = {coarse}, default = {default}"
    );
    assert!(
        (default - expected).abs() < expected * 0.02,
        "default volume = {default}"
    );
}

#[test]
fn fn_applies_to_2d_csg_profiles() {
    let actual = volume("linear_extrude(1) difference() { square(4); circle(r = 1, $fn = 8); }");
    let expected = 16.0 - 4.0 * std::f64::consts::FRAC_PI_4.sin();

    assert!(
        (actual - expected).abs() < expected * 0.001,
        "volume = {actual}"
    );
}

#[test]
fn fn_inherits_through_booleans() {
    let actual = volume("linear_extrude(1) difference($fn = 8) { square(4); circle(r = 1); }");
    let expected = 16.0 - 4.0 * std::f64::consts::FRAC_PI_4.sin();

    assert!(
        (actual - expected).abs() < expected * 0.001,
        "volume = {actual}"
    );
}

#[test]
fn fa_and_fs_do_not_facet_geometry() {
    let actual = volume("cylinder(r = 1, h = 1, $fa = 5, $fs = 1);");
    let expected = std::f64::consts::PI;

    assert!(
        (actual - expected).abs() < expected * 0.01,
        "volume = {actual}"
    );
}

#[test]
fn top_level_fn_assignment_applies() {
    let actual = volume("$fn = 8; cylinder(r = 1, h = 1);");
    let expected = 4.0 * std::f64::consts::FRAC_PI_4.sin();

    assert!(
        (actual - expected).abs() < expected * 0.01,
        "volume = {actual}"
    );
}
