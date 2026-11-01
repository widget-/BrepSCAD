mod common;

use brepscad_eval::{EvalError, evaluate};
use brepscad_kernel_truck::TruckKernel;
use common::{bounds, signed_volume};

fn extents(mesh: &brepscad_kernel_api::Mesh) -> [f64; 3] {
    let (min, max) = bounds(mesh);
    [max[0] - min[0], max[1] - min[1], max[2] - min[2]]
}

#[test]
fn module_call_emits_body_geometry() {
    let mut kernel = TruckKernel::new();
    let evaluation =
        evaluate("module unit() { cube(1); }\nunit();", &mut kernel, 0.01).expect("module call");

    assert_eq!(evaluation.shapes.len(), 1);
    assert_eq!(extents(&evaluation.shapes[0].mesh), [1.0, 1.0, 1.0]);
}

#[test]
fn module_can_be_used_before_its_declaration() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate("unit();\nmodule unit() { cube(2); }", &mut kernel, 0.01)
        .expect("forward declaration");

    assert_eq!(evaluation.shapes.len(), 1);
    assert_eq!(extents(&evaluation.shapes[0].mesh), [2.0, 2.0, 2.0]);
}

#[test]
fn module_parameters_accept_positional_and_named_arguments() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "module box(size, h) { cube([size, size, h]); }\nbox(2, 3);\nbox(size = 4, h = 5);",
        &mut kernel,
        0.01,
    )
    .expect("module arguments");

    assert_eq!(evaluation.shapes.len(), 2);
    assert_eq!(extents(&evaluation.shapes[0].mesh), [2.0, 2.0, 3.0]);
    assert_eq!(extents(&evaluation.shapes[1].mesh), [4.0, 4.0, 5.0]);
}

#[test]
fn module_defaults_can_reference_earlier_parameters() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "module box(size = 2, h = size * 3) { cube([size, size, h]); }\nbox();\nbox(4);",
        &mut kernel,
        0.01,
    )
    .expect("default parameters");

    assert_eq!(evaluation.shapes.len(), 2);
    assert_eq!(extents(&evaluation.shapes[0].mesh), [2.0, 2.0, 6.0]);
    assert_eq!(extents(&evaluation.shapes[1].mesh), [4.0, 4.0, 12.0]);
}

#[test]
fn module_assignments_shadow_and_restore_caller_variables() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "x = 5;\nmodule small() { x = 1; cube(x); }\nsmall();\ncube(x);",
        &mut kernel,
        0.01,
    )
    .expect("scoped assignment");

    assert_eq!(evaluation.shapes.len(), 2);
    assert_eq!(extents(&evaluation.shapes[0].mesh), [1.0, 1.0, 1.0]);
    assert_eq!(extents(&evaluation.shapes[1].mesh), [5.0, 5.0, 5.0]);
}

#[test]
fn module_bodies_use_their_defining_scope() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "width = 3;\n\
         module plate() { cube([width, 1, 1]); }\n\
         module caller() { width = 5; plate(); }\n\
         caller();",
        &mut kernel,
        0.01,
    )
    .expect("lexical scope");

    assert_eq!(extents(&evaluation.shapes[0].mesh), [3.0, 1.0, 1.0]);
}

#[test]
fn special_variables_are_dynamic() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "$fn = 8;\n\
         module shape() { cube([$fn, 1, 1]); }\n\
         module caller() { $fn = 4; shape(); }\n\
         caller();",
        &mut kernel,
        0.01,
    )
    .expect("dynamic special variables");

    assert_eq!(extents(&evaluation.shapes[0].mesh), [4.0, 1.0, 1.0]);
}

#[test]
fn nested_modules_capture_the_outer_call_scope() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "module outer() {\n\
             width = 3;\n\
             module inner() { cube([width, 1, 1]); }\n\
             inner();\n\
         }\n\
         outer();",
        &mut kernel,
        0.01,
    )
    .expect("nested module scope");

    assert_eq!(extents(&evaluation.shapes[0].mesh), [3.0, 1.0, 1.0]);
}

#[test]
fn children_render_at_the_call_site() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "module shifted() { translate([10, 0, 0]) children(); }\nshifted() cube(2);",
        &mut kernel,
        0.01,
    )
    .expect("children");

    assert_eq!(evaluation.shapes.len(), 1);
    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!((min[0] - 9.0).abs() < 1.0e-6, "min = {min:?}");
    assert!((max[0] - 11.0).abs() < 1.0e-6, "max = {max:?}");
}

#[test]
fn children_index_selects_one_child() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "module second() { children(1); }\nsecond() { cube(1); translate([10, 0, 0]) cube(2); }",
        &mut kernel,
        0.01,
    )
    .expect("indexed children");

    assert_eq!(evaluation.shapes.len(), 1);
    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!((min[0] - 9.0).abs() < 1.0e-6, "min = {min:?}");
    assert!((max[0] - 11.0).abs() < 1.0e-6, "max = {max:?}");
}

#[test]
fn children_use_the_calling_scope() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "hidden = 2;\nmodule wrapper() { hidden = 100; children(); }\nwrapper() cube(hidden);",
        &mut kernel,
        0.01,
    )
    .expect("caller scope");

    assert_eq!(extents(&evaluation.shapes[0].mesh), [2.0, 2.0, 2.0]);
}

#[test]
fn children_count_is_available_as_a_special_variable() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "module stack() { for (i = [0:$children - 1]) translate([0, 0, i * 2]) children(i); }\nstack() { cube(1); cube(1); }",
        &mut kernel,
        0.01,
    )
    .expect("children count");

    assert_eq!(evaluation.shapes.len(), 2);
    let mins = evaluation
        .shapes
        .iter()
        .map(|shape| bounds(&shape.mesh).0[2])
        .collect::<Vec<_>>();
    assert!((mins[0] + 0.5).abs() < 1.0e-6, "mins = {mins:?}");
    assert!((mins[1] - 1.5).abs() < 1.0e-6, "mins = {mins:?}");
}

#[test]
fn modules_can_produce_two_dimensional_children() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "module square2() { square(2); }\nlinear_extrude(3) square2();",
        &mut kernel,
        0.01,
    )
    .expect("2D module");

    assert_eq!(evaluation.shapes.len(), 1);
    let size = extents(&evaluation.shapes[0].mesh);
    assert!((size[0] - 2.0).abs() < 1.0e-6, "size = {size:?}");
    assert!((size[1] - 2.0).abs() < 1.0e-6, "size = {size:?}");
}

#[test]
fn modules_can_recurse() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "module tower(n) { if (n > 0) { translate([0, 0, n]) cube(1); tower(n - 1); } }\ntower(3);",
        &mut kernel,
        0.01,
    )
    .expect("recursive module");

    assert_eq!(evaluation.shapes.len(), 3);
}

#[test]
fn modules_nested_in_a_module_do_not_escape_its_scope() {
    let mut kernel = TruckKernel::new();
    let inner = evaluate(
        "module outer() { module inner() { cube(1); } inner(); }\nouter();",
        &mut kernel,
        0.01,
    )
    .expect("nested module");
    assert_eq!(inner.shapes.len(), 1);

    let error = evaluate(
        "module outer() { module inner() { cube(1); } }\ninner();",
        &mut kernel,
        0.01,
    )
    .expect_err("inner is out of scope");
    assert!(matches!(error, EvalError::UnsupportedModule(name) if name == "inner"));
}

#[test]
fn module_call_parameters_are_exposed_for_editing() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "module plate(width) { cube([width, width, 1]); }\nplate(4);",
        &mut kernel,
        0.01,
    )
    .expect("module parameters");

    let part = &evaluation.shapes[0].parts[0];
    assert!(
        part.parameters
            .iter()
            .any(|parameter| parameter.value == 4.0),
        "parameters = {:?}",
        part.parameters
    );
}

#[test]
fn module_special_variables_flow_into_children() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "module plain() { children(); }\nplain($fn = 8) cylinder(r = 1, h = 2);",
        &mut kernel,
        0.001,
    )
    .expect("special variables");

    let actual = signed_volume(&evaluation.shapes[0].mesh);
    let expected = 8.0 * std::f64::consts::FRAC_PI_4.sin();
    assert!(
        (actual - expected).abs() < expected * 0.01,
        "volume = {actual}"
    );
}

#[test]
fn unknown_named_arguments_are_rejected() {
    let mut kernel = TruckKernel::new();
    let error = evaluate(
        "module box(size) { cube(size); }\nbox(width = 2);",
        &mut kernel,
        0.01,
    )
    .expect_err("unknown argument");

    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "box"));
}

#[test]
fn missing_arguments_are_undef() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "module box(size) { cube(is_undef(size) ? 2 : 1); }\nbox();",
        &mut kernel,
        0.01,
    )
    .expect("missing argument should be undef");

    assert_eq!(extents(&evaluation.shapes[0].mesh), [2.0; 3]);
}

#[test]
fn too_many_positional_arguments_are_rejected() {
    let mut kernel = TruckKernel::new();
    let error = evaluate(
        "module box(size) { cube(size); }\nbox(1, 2);",
        &mut kernel,
        0.01,
    )
    .expect_err("too many arguments");

    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "box"));
}

#[test]
fn children_outside_a_module_is_an_error() {
    let mut kernel = TruckKernel::new();
    let error = evaluate("children();", &mut kernel, 0.01).expect_err("stray children");

    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "children"));
}

#[test]
fn out_of_range_children_index_is_an_error() {
    let mut kernel = TruckKernel::new();
    let error = evaluate(
        "module only() { children(3); }\nonly() cube(1);",
        &mut kernel,
        0.01,
    )
    .expect_err("bad child index");

    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "children"));
}
#[test]
fn children_inside_a_call_site_use_the_callers_children() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "module inner() { children(); }\nmodule outer() { inner() children(); }\nouter() cube(2);",
        &mut kernel,
        0.01,
    )
    .expect("nested children");

    assert_eq!(evaluation.shapes.len(), 1);
    assert_eq!(extents(&evaluation.shapes[0].mesh), [2.0, 2.0, 2.0]);
}

#[test]
fn children_see_special_variables_set_in_the_module_body() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "module wrapper() { $color = \"red\"; children(); }\nmodule child() { cube($color == \"red\" ? 1 : 2); }\nwrapper() child();",
        &mut kernel,
        0.01,
    )
    .expect("dynamic special");

    assert_eq!(evaluation.shapes.len(), 1);
    assert_eq!(extents(&evaluation.shapes[0].mesh), [1.0, 1.0, 1.0]);
}
#[test]
fn intersection_unions_shapes_emitted_by_one_child() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "module pair() { square(1); translate([2, 0]) square(1); }\n\
         linear_extrude(1) intersection() {\n\
             pair();\n\
             translate([1, 0]) square(2, center = true);\n\
         }",
        &mut kernel,
        0.01,
    )
    .expect("grouped intersection");
    assert_eq!(evaluation.shapes.len(), 1);
    let volume = signed_volume(&evaluation.shapes[0].mesh);
    assert!((volume - 1.0).abs() < 0.01, "volume = {volume}");
}
