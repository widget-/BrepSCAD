//! Synthetic box-pair booleans over the separations and configurations the
//! OpenGrid work kept running into.
//!
//! The real models produce knife edges, near-coplanar cuts and vertices that
//! land on an edge without a vertex there. Those are hard to reduce, so this
//! suite recreates the same *conditions* on two boxes where the answer is
//! known analytically: faces overlapping by 1, 0.01, 1e-6 and 1e-9, exactly
//! coplanar, and separated by the same distances; operands of equal size, one
//! enclosed, sharing a face, an edge or a corner, and slanted; with one, two
//! and three simultaneous intersections, under difference, intersection and
//! union.
//!
//! Every case is expected to evaluate without error and to land within `slack`
//! of the analytic volume. `slack` is deliberately generous for separations
//! below the kernel's 0.05 tolerance: an overlap of 0.01 may legitimately be
//! resolved as an overlap or as touching, and both answers are within a cubic
//! millimetre of each other. What the suite is really guarding against is the
//! gross error - a cutter that removes a whole wall, or a union that drops an
//! operand - not the last digit.

mod common;

use brepscad_eval::{EvaluationOptions, evaluate_with_options};
use brepscad_kernel_truck::TruckKernel;
use common::signed_volume;

/// A box of side `size` centred on `(x, y, z)`.
fn cube_at(x: f64, y: f64, z: f64, size: f64) -> String {
    format!("translate([{x}, {y}, {z}]) cube([{size}, {size}, {size}], center = true);")
}

/// A box of side `size` centred on the origin, rotated about z.
fn slanted_cube(size: f64, degrees: f64) -> String {
    format!("rotate([0, 0, {degrees}]) cube([{size}, {size}, {size}], center = true);")
}

/// One parameterised case.
struct Case {
    name: String,
    source: String,
    expected: f64,
    slack: f64,
}

/// Volumes of the three booleans for two boxes whose intersection is `overlap`.
fn pair_cases(
    label: &str,
    second: &str,
    second_volume: f64,
    overlap: f64,
    slack: f64,
    cases: &mut Vec<Case>,
) {
    let first_volume = 1000.0;
    let difference = first_volume - overlap;
    let intersection = overlap;
    let union = first_volume + second_volume - overlap;
    for (op, body, expected) in [
        (
            "difference",
            format!(
                "difference() {{ {}{second} }}",
                cube_at(0.0, 0.0, 0.0, 10.0)
            ),
            difference,
        ),
        (
            "intersection",
            format!(
                "intersection() {{ {}{second} }}",
                cube_at(0.0, 0.0, 0.0, 10.0)
            ),
            intersection,
        ),
        (
            "union",
            format!("union() {{ {}{second} }}", cube_at(0.0, 0.0, 0.0, 10.0)),
            union,
        ),
    ] {
        cases.push(Case {
            name: format!("{label}_{op}"),
            source: body,
            expected,
            slack,
        });
    }
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // Equal boxes sliding along x. A positive overlap means the second box is
    // pulled in; a negative one is a gap.
    for (label, offset, overlap, slack) in [
        ("overlap_1", 9.0, 100.0, 9.0),
        ("overlap_0p01", 9.99, 1.0, 2.0),
        ("overlap_1e-6", 9.999_999, 1.0e-4, 2.0),
        ("overlap_1e-9", 9.999_999_999, 1.0e-7, 2.0),
        ("coplanar", 10.0, 0.0, 2.0),
        ("gap_1e-9", 10.000_000_001, 0.0, 2.0),
        ("gap_1e-6", 10.000_001, 0.0, 2.0),
        ("gap_0p01", 10.01, 0.0, 2.0),
        ("gap_1", 11.0, 0.0, 2.0),
    ] {
        pair_cases(
            label,
            &cube_at(offset, 0.0, 0.0, 10.0),
            1000.0,
            overlap,
            slack,
            &mut cases,
        );
    }

    // Same overlap, but offset on all three axes at once: a corner region
    // rather than a slab.
    pair_cases(
        "diagonal_overlap_1",
        &cube_at(9.0, 9.0, 9.0, 10.0),
        1000.0,
        1.0,
        2.0,
        &mut cases,
    );

    // One box enclosed by the other.
    pair_cases(
        "enclosed",
        &cube_at(0.0, 0.0, 0.0, 4.0),
        64.0,
        64.0,
        2.0,
        &mut cases,
    );
    // Enclosed, but sharing three of the outer box's faces.
    pair_cases(
        "enclosed_sharing_three_faces",
        &cube_at(3.0, 3.0, 3.0, 4.0),
        64.0,
        64.0,
        2.0,
        &mut cases,
    );
    // Sharing exactly one edge, then exactly one corner.
    pair_cases(
        "shared_edge",
        &cube_at(10.0, 10.0, 0.0, 10.0),
        1000.0,
        0.0,
        2.0,
        &mut cases,
    );
    pair_cases(
        "shared_corner",
        &cube_at(10.0, 10.0, 10.0, 10.0),
        1000.0,
        0.0,
        2.0,
        &mut cases,
    );

    // Slanted operands: the inner box rotated, then the outer one. Both stay
    // enclosed, so the analytic volumes are unchanged.
    pair_cases(
        "slanted_inner_enclosed",
        &slanted_cube(4.0, 45.0),
        64.0,
        64.0,
        2.0,
        &mut cases,
    );
    for (op, expected) in [
        ("difference", 936.0),
        ("intersection", 64.0),
        ("union", 1000.0),
    ] {
        let outer = "rotate([0, 0, 45]) cube([10, 10, 10], center = true);";
        let inner = cube_at(0.0, 0.0, 0.0, 4.0);
        let body = match op {
            "difference" => format!("difference() {{ {outer}{inner} }}"),
            "intersection" => format!("intersection() {{ {outer}{inner} }}"),
            _ => format!("union() {{ {outer}{inner} }}"),
        };
        cases.push(Case {
            name: format!("slanted_outer_enclosed_{op}"),
            source: body,
            expected,
            slack: 2.0,
        });
    }

    // Three operands at once: two cutters that also overlap each other.
    let first_cutter = cube_at(9.0, 0.0, 0.0, 10.0);
    let second_cutter = cube_at(0.0, 9.0, 0.0, 10.0);
    let subject = cube_at(0.0, 0.0, 0.0, 10.0);
    for (op, expected) in [
        ("difference", 810.0),
        ("intersection", 10.0),
        ("union", 2800.0),
    ] {
        let body = format!("{op}() {{ {subject}{first_cutter}{second_cutter} }}");
        cases.push(Case {
            name: format!("three_operands_{op}"),
            source: body,
            expected,
            slack: 28.0,
        });
    }

    cases
}

fn check(case: &Case) {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate_with_options(
        &case.source,
        &mut kernel,
        EvaluationOptions {
            tolerance: 0.01,
            ..Default::default()
        },
    )
    .unwrap_or_else(|error| panic!("{}: evaluation failed: {error}", case.name));

    let volume: f64 = evaluation
        .shapes
        .iter()
        .map(|shape| signed_volume(&shape.mesh))
        .sum();
    let triangles: usize = evaluation
        .shapes
        .iter()
        .map(|shape| shape.mesh.triangle_count())
        .sum();

    assert!(
        volume.is_finite(),
        "{}: volume is not finite ({volume})",
        case.name
    );
    if case.expected.abs() <= case.slack {
        assert!(
            volume.abs() <= case.slack,
            "{}: volume {volume} should be about zero",
            case.name
        );
        return;
    }
    assert!(
        triangles > 0,
        "{}: expected geometry, got an empty mesh",
        case.name
    );
    assert!(
        (volume - case.expected).abs() <= case.slack,
        "{}: volume {volume}, expected {} +/- {}",
        case.name,
        case.expected,
        case.slack
    );
}

/// Cases that currently fail, recorded so the failure is visible and a fix
/// shows up as this list shrinking. Each one has a dedicated `#[ignore]`d test
/// below that asserts the *correct* answer. Empty: the near-tangent difference
/// below was the only entry.
const KNOWN_BROKEN: &[&str] = &[];

#[test]
fn box_pairs_stay_within_their_analytic_volumes() {
    let all = cases();
    for name in KNOWN_BROKEN {
        assert!(
            all.iter().any(|case| case.name == *name),
            "stale KNOWN_BROKEN entry: {name}"
        );
    }
    let failures: Vec<String> = all
        .into_iter()
        .filter(|case| !KNOWN_BROKEN.contains(&case.name.as_str()))
        .filter_map(|case| {
            let outcome = std::panic::catch_unwind(|| check(&case));
            match outcome {
                Ok(()) => None,
                Err(payload) => Some(
                    payload
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
                        .unwrap_or_else(|| format!("{}: panicked", case.name)),
                ),
            }
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} of the box-pair cases failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// A difference whose cutter overlaps the subject by less than the kernel's
/// tolerance.
///
/// The cutter reaches 0.01 into the `x = 5` face, so the answer is 999: the
/// subject with a 0.01-wide strip removed. This used to return 833.33, exactly
/// 1000 - 1000/6 - the subject cube with that whole face gone. Two things were
/// wrong: the coplanar test judged the two faces coincident because they are
/// closer than the modelling tolerance, and the classification probe fell back
/// to a face *corner* for non-coplanar pairs, which lies on the other solid's
/// surface.
#[test]
fn near_tangent_difference_keeps_the_trimmed_face() {
    let case = cases()
        .into_iter()
        .find(|case| case.name == "overlap_0p01_difference")
        .expect("case present");
    check(&case);
}
