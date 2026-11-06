mod common;

use brepscad_eval::{EvalError, evaluate, evaluate_to_meshes};
use brepscad_kernel_truck::TruckKernel;
use brepscad_syntax::{TextEdit, TextRange, TextSize, apply_edit};
use common::{bounds, signed_volume};

#[test]
fn evaluates_a_cube() {
    let mut kernel = TruckKernel::new();
    let meshes = evaluate_to_meshes("cube([2, 3, 4]);", &mut kernel, 0.01).expect("evaluate");

    assert_eq!(meshes.len(), 1);
    assert!(meshes[0].triangle_count() > 0);
}

#[test]
fn exposes_numeric_parameters_for_selection() {
    let source =
        "size = [2, 2, 2];\ndifference() {\n    cube(size);\n    cylinder(h=4, r=0.5);\n}\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    assert_eq!(evaluation.shapes.len(), 1);
    let parameters = &evaluation.shapes[0].parameters;
    assert_eq!(parameters.len(), 5);
    assert!(parameters.iter().any(|parameter| parameter.value == 0.5));
    assert!(parameters.iter().any(|parameter| parameter.source == "4"));
}

#[test]
fn per_primitive_center_controls_placement() {
    let mut kernel = TruckKernel::new();

    let corner_cube = evaluate("cube(2, center = false);", &mut kernel, 0.01).expect("corner cube");
    let (min, max) = bounds(&corner_cube.shapes[0].mesh);
    assert!(min[0].abs() < 1.0e-6, "min = {min:?}");
    assert!((max[0] - 2.0).abs() < 1.0e-6, "max = {max:?}");

    let centered_cube =
        evaluate("cube(2, center = true);", &mut kernel, 0.01).expect("centered cube");
    let (min, max) = bounds(&centered_cube.shapes[0].mesh);
    assert!((min[0] + 1.0).abs() < 1.0e-6, "min = {min:?}");
    assert!((max[0] - 1.0).abs() < 1.0e-6, "max = {max:?}");

    let base_cylinder = evaluate("cylinder(h = 2, r = 1, center = false);", &mut kernel, 0.01)
        .expect("base cylinder");
    let (min, max) = bounds(&base_cylinder.shapes[0].mesh);
    assert!(min[2].abs() < 1.0e-6, "min = {min:?}");
    assert!((max[2] - 2.0).abs() < 1.0e-6, "max = {max:?}");

    let centered_cylinder = evaluate("cylinder(h = 2, r = 1, center = true);", &mut kernel, 0.01)
        .expect("centered cylinder");
    let (min, max) = bounds(&centered_cylinder.shapes[0].mesh);
    assert!((min[2] + 1.0).abs() < 1.0e-6, "min = {min:?}");
    assert!((max[2] - 1.0).abs() < 1.0e-6, "max = {max:?}");
}

#[test]
fn empty_booleans_produce_no_geometry() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "union() {}\ndifference() {}\nintersection() {}",
        &mut kernel,
        0.01,
    )
    .expect("empty booleans");

    assert!(evaluation.shapes.is_empty());
}

#[test]
fn color_and_render_pass_through_children() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "color(\"red\") cube(2);\nrender() cube(3);",
        &mut kernel,
        0.01,
    )
    .expect("pass-through modules");

    assert_eq!(evaluation.shapes.len(), 2);
    let sizes = evaluation
        .shapes
        .iter()
        .map(|shape| {
            let (min, max) = bounds(&shape.mesh);
            [max[0] - min[0], max[1] - min[1], max[2] - min[2]]
        })
        .collect::<Vec<_>>();
    assert_eq!(sizes, [[2.0; 3], [3.0; 3]]);
}

#[test]
fn exposes_selectable_operand_parts() {
    let source = "size = [2, 2, 2];\ndifference() { cube(size); cylinder(h=4, r=0.5); }\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    assert_eq!(shape.parts.len(), 2);
    assert_eq!(shape.parts[0].module, "cube");
    assert_eq!(shape.parts[1].module, "cylinder");
    assert_eq!(shape.parts[0].parameters.len(), 3);
    assert_eq!(shape.parts[1].parameters.len(), 2);
    assert!(
        shape
            .parts
            .iter()
            .all(|part| !part.mesh.positions.is_empty())
    );
}

#[test]
fn parameter_ranges_edit_the_expected_literal() {
    let source = "size = [2, 2, 2];\ndifference() { cube(size); cylinder(h=4, r=0.5); }\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let part = &evaluation.shapes[0].parts[0];
    let parameter = part
        .parameters
        .iter()
        .find(|parameter| parameter.value == 2.0)
        .expect("size literal");

    let range = TextRange::new(
        TextSize::from(parameter.range.start as u32),
        TextSize::from(parameter.range.end as u32),
    );
    let edited = apply_edit(source, &TextEdit::new(range, "3")).expect("edit");

    assert!(edited.contains("size = [3, 2, 2];"), "{edited}");
}

#[test]
fn evaluates_named_primitive_arguments() {
    let mut kernel = TruckKernel::new();
    let cube = evaluate("cube(size = [2, 4, 6]);", &mut kernel, 0.01).expect("cube");
    let (min, max) = bounds(&cube.shapes[0].mesh);
    assert!((max[0] - min[0] - 2.0).abs() < 1e-6);
    assert!((max[1] - min[1] - 4.0).abs() < 1e-6);
    assert!((max[2] - min[2] - 6.0).abs() < 1e-6);

    let cylinder = evaluate("cylinder(r=0.5, h=4);", &mut kernel, 0.01).expect("cylinder");
    let (min, max) = bounds(&cylinder.shapes[0].mesh);
    assert!((max[2] - min[2] - 4.0).abs() < 1e-6);
}

#[test]
fn named_argument_names_do_not_leak_assignment_parameters() {
    let source = "size = [9, 9, 9];\ncube(size = 2);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let part = &evaluation.shapes[0].parts[0];

    assert_eq!(part.parameters.len(), 1);
    assert_eq!(part.parameters[0].value, 2.0);
}

#[test]
fn named_argument_values_still_resolve_assignment_parameters() {
    let source = "scale = [2, 3, 4];\ncube(size = scale);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    let (min, max) = bounds(&shape.mesh);
    assert!((max[0] - min[0] - 2.0).abs() < 1e-6);
    assert!((max[1] - min[1] - 3.0).abs() < 1e-6);
    assert!((max[2] - min[2] - 4.0).abs() < 1e-6);
    assert!(
        shape.parts[0]
            .parameters
            .iter()
            .any(|parameter| parameter.value == 3.0)
    );
}

#[test]
fn evaluates_a_polyhedron() {
    let source = "polyhedron(\n    points = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]],\n    faces = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]]\n);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    let volume = signed_volume(&shape.mesh);
    assert!((volume - 1.0 / 6.0).abs() < 1e-9, "volume = {volume}");
    assert_eq!(shape.parts.len(), 1);
    assert_eq!(shape.parts[0].module, "polyhedron");
}

#[test]
fn rejects_bad_polyhedron_indices() {
    let source = "polyhedron(points = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]], faces = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 9]]);\n";
    let mut kernel = TruckKernel::new();
    let error = evaluate_to_meshes(source, &mut kernel, 0.01).expect_err("bad index");

    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "polyhedron"));
}

#[test]
fn evaluates_a_sphere_and_its_diameter() {
    let mut kernel = TruckKernel::new();

    let sphere = evaluate("sphere(r = 1);", &mut kernel, 0.01).expect("sphere");
    let (min, max) = bounds(&sphere.shapes[0].mesh);
    for axis in 0..3 {
        assert!(
            (max[axis] - min[axis] - 2.0).abs() < 0.02,
            "axis {axis}: {}",
            max[axis] - min[axis]
        );
    }

    let by_diameter = evaluate("sphere(d = 2);", &mut kernel, 0.01).expect("diameter");
    let (min, max) = bounds(&by_diameter.shapes[0].mesh);
    assert!((max[0] - min[0] - 2.0).abs() < 0.02);
}

#[test]
fn cylinder_accepts_openscad_positional_radii_and_frustums() {
    let mut kernel = TruckKernel::new();

    let base = evaluate("cylinder(4, 1, center = false);", &mut kernel, 0.01).expect("cylinder");
    let (min, max) = bounds(&base.shapes[0].mesh);
    assert!((max[0] - min[0] - 2.0).abs() < 0.02);
    assert!((max[1] - min[1] - 2.0).abs() < 0.02);
    assert!((max[2] - min[2] - 4.0).abs() < 1e-6);
    assert!(min[2].abs() < 1e-6);

    let frustum = evaluate("cylinder(4, 1, 2, false);", &mut kernel, 0.01).expect("frustum");
    let (min, max) = bounds(&frustum.shapes[0].mesh);
    assert!((max[0] - min[0] - 4.0).abs() < 0.02);
    assert!((max[1] - min[1] - 4.0).abs() < 0.02);
    assert!((max[2] - min[2] - 4.0).abs() < 1e-6);
    let expected = std::f64::consts::PI * 4.0 / 3.0 * (1.0 + 2.0 + 4.0);
    assert!((signed_volume(&frustum.shapes[0].mesh) - expected).abs() < 0.4);

    let cone = evaluate("cylinder(4, 0, 2, false);", &mut kernel, 0.01).expect("cone");
    let expected = std::f64::consts::PI * 4.0 * 4.0 / 3.0;
    assert!((signed_volume(&cone.shapes[0].mesh) - expected).abs() < 0.3);
}

/// A sweep whose last slice keeps its full chamfer while the height collapses
/// to 1e-6 makes its end cap a self-intersecting octagon. The solid still
/// builds - OpenSCAD renders it - but the face has no well-defined outward
/// side, so the evaluator has to say so rather than let it show up as red
/// faces later.
#[test]
fn a_self_intersecting_polyhedron_face_is_reported() {
    let source = r#"
function rev(v) = [for (i = [0 : len(v) - 1]) v[len(v) - 1 - i]];
function crect(base, chamfer) = [
    [ base[0]/2 - chamfer, -base[1]/2],
    [ base[0]/2, -base[1]/2 + chamfer],
    [ base[0]/2,  base[1]/2 - chamfer],
    [ base[0]/2 - chamfer,  base[1]/2],
    [-base[0]/2 + chamfer,  base[1]/2],
    [-base[0]/2,  base[1]/2 - chamfer],
    [-base[0]/2, -base[1]/2 + chamfer],
    [-base[0]/2 + chamfer, -base[1]/2]
];

module sweep(slices) {
    n = len(slices);
    pp = 8;
    points = [
        for (s = slices)
            for (p = crect([s[1], s[2]], s[3]))
                [p[0], p[1], s[0]]
    ];
    bottom = rev([for (i = [0 : pp - 1]) i]);
    top_start = (n - 1) * pp;
    top = [for (i = [0 : pp - 1]) top_start + i];
    sides = concat(
        [for (i = [0 : n - 2]) for (j = [0 : pp - 1])
            [i*pp + j, i*pp + (j + 1) % pp, (i + 1)*pp + (j + 1) % pp]],
        [for (i = [0 : n - 2]) for (j = [0 : pp - 1])
            [i*pp + j, (i + 1)*pp + (j + 1) % pp, (i + 1)*pp + j]]
    );
    faces = [for (f = concat([bottom], [top], sides)) rev(f)];
    polyhedron(points = points, faces = faces, convexity = 10);
}

sweep([
    [0, 12, 0.4, 0.4 / 3],
    [1.3, 12, 0.4, 0.4 / 3],
    [1.5, 11.6, 0.000001, 0.4 / 3]
]);
"#;
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    assert!(
        evaluation
            .messages
            .iter()
            .any(|message| message.contains("crosses itself")),
        "expected a self-intersection warning, got {:?}",
        evaluation.messages
    );
}

/// The same sweep without the collapsing slice is a valid solid and must not
/// warn.
#[test]
fn a_valid_sweep_is_not_reported() {
    let source = r#"
function rev(v) = [for (i = [0 : len(v) - 1]) v[len(v) - 1 - i]];
function crect(base, chamfer) = [
    [ base[0]/2 - chamfer, -base[1]/2],
    [ base[0]/2, -base[1]/2 + chamfer],
    [ base[0]/2,  base[1]/2 - chamfer],
    [ base[0]/2 - chamfer,  base[1]/2],
    [-base[0]/2 + chamfer,  base[1]/2],
    [-base[0]/2,  base[1]/2 - chamfer],
    [-base[0]/2, -base[1]/2 + chamfer],
    [-base[0]/2 + chamfer, -base[1]/2]
];

module sweep(slices) {
    n = len(slices);
    pp = 8;
    points = [
        for (s = slices)
            for (p = crect([s[1], s[2]], s[3]))
                [p[0], p[1], s[0]]
    ];
    bottom = rev([for (i = [0 : pp - 1]) i]);
    top_start = (n - 1) * pp;
    top = [for (i = [0 : pp - 1]) top_start + i];
    sides = concat(
        [for (i = [0 : n - 2]) for (j = [0 : pp - 1])
            [i*pp + j, i*pp + (j + 1) % pp, (i + 1)*pp + (j + 1) % pp]],
        [for (i = [0 : n - 2]) for (j = [0 : pp - 1])
            [i*pp + j, (i + 1)*pp + (j + 1) % pp, (i + 1)*pp + j]]
    );
    faces = [for (f = concat([bottom], [top], sides)) rev(f)];
    polyhedron(points = points, faces = faces, convexity = 10);
}

sweep([
    [0, 12, 0.4, 0.4 / 3],
    [1.3, 12, 0.4, 0.4 / 3]
]);
"#;
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    assert!(
        evaluation
            .messages
            .iter()
            .all(|message| !message.contains("crosses itself")),
        "unexpected warning: {:?}",
        evaluation.messages
    );
}
