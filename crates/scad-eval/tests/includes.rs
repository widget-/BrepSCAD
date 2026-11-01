mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use brepscad_eval::{
    EvalError, EvaluationOptions, evaluate, evaluate_file, evaluate_file_with_options,
};
use brepscad_kernel_api::Mesh;
use brepscad_kernel_truck::TruckKernel;
use common::bounds;

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "brepscad-include-test-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(&path, contents).expect("write source");
        path
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn extents(mesh: &Mesh) -> [f64; 3] {
    let (min, max) = bounds(mesh);
    [max[0] - min[0], max[1] - min[1], max[2] - min[2]]
}

fn evaluate_file_extents(path: &Path) -> [f64; 3] {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate_file(path, &mut kernel, 0.01).expect("evaluate file");
    assert_eq!(evaluation.shapes.len(), 1);
    extents(&evaluation.shapes[0].mesh)
}

#[test]
fn include_loads_assignments_and_module_definitions() {
    let dir = TempDir::new();
    dir.write(
        "lib.scad",
        "size = 2;\nmodule box(s = size) { cube([s, s, 1]); }\n",
    );
    let main = dir.write("main.scad", "include \"lib.scad\";\nbox();\n");

    assert_eq!(evaluate_file_extents(&main), [2.0, 2.0, 1.0]);
}

#[test]
fn root_assignments_see_included_assignments() {
    let dir = TempDir::new();
    dir.write("lib.scad", "size = 2;\n");
    let main = dir.write(
        "main.scad",
        "include \"lib.scad\";\nvalue = size;\ncube(value);\n",
    );

    assert_eq!(evaluate_file_extents(&main), [2.0; 3]);
}

#[test]
fn include_emits_top_level_geometry() {
    let dir = TempDir::new();
    dir.write("part.scad", "cube(3);\n");
    let main = dir.write("main.scad", "include \"part.scad\";\n");

    assert_eq!(evaluate_file_extents(&main), [3.0; 3]);
}

#[test]
fn include_imports_functions() {
    let dir = TempDir::new();
    dir.write("lib.scad", "function double(x) = x * 2;\n");
    let main = dir.write("main.scad", "include \"lib.scad\";\ncube(double(2));\n");

    assert_eq!(evaluate_file_extents(&main), [4.0; 3]);
}

#[test]
fn use_imports_definitions_without_geometry() {
    let dir = TempDir::new();
    dir.write(
        "lib.scad",
        "size = 9;\nmodule box() { cube(2); }\nfunction double(x) = x * 2;\ncube(5);\n",
    );
    let main = dir.write("main.scad", "use \"lib.scad\";\nbox();\n");

    assert_eq!(evaluate_file_extents(&main), [2.0; 3]);

    let function_main = dir.write(
        "function_main.scad",
        "use \"lib.scad\";\ncube(double(3));\n",
    );
    assert_eq!(evaluate_file_extents(&function_main), [6.0; 3]);
}

#[test]
fn use_resolves_names_in_the_used_file_scope() {
    let dir = TempDir::new();
    dir.write("lib.scad", "module box() { cube(2); }\n");
    let main = dir.write(
        "main.scad",
        "module cube(size) { sphere(r = size); }\nuse \"lib.scad\";\nbox();\n",
    );

    // `box()` must call the built-in cube from its own file scope, not the
    // caller's module cube (which would produce a sphere with diameter 4).
    assert_eq!(evaluate_file_extents(&main), [2.0; 3]);
}

#[test]
fn use_does_not_import_variables() {
    let dir = TempDir::new();
    dir.write("lib.scad", "size = 9;\nmodule box() { cube(size); }\n");
    let main = dir.write(
        "main.scad",
        "use \"lib.scad\";\ncube(is_undef(size) ? 2 : 1);\n",
    );

    assert_eq!(evaluate_file_extents(&main), [2.0; 3]);
}

#[test]
fn angle_bracket_includes_use_library_paths() {
    let dir = TempDir::new();
    dir.write("lib/BOSL2/std.scad", "module libbox() { cube(3); }\n");
    let main = dir.write("main.scad", "include <BOSL2/std.scad>;\nlibbox();\n");

    let mut kernel = TruckKernel::new();
    let options = EvaluationOptions {
        tolerance: 0.01,
        legacy_centering: false,
        library_paths: vec![dir.path().join("lib")],
    };
    let evaluation =
        evaluate_file_with_options(&main, &mut kernel, options).expect("library path include");
    assert_eq!(extents(&evaluation.shapes[0].mesh), [3.0; 3]);
}

#[test]
fn quoted_includes_do_not_search_library_paths() {
    let dir = TempDir::new();
    dir.write("lib/BOSL2/std.scad", "module libbox() { cube(3); }\n");
    let main = dir.write("main.scad", "include \"BOSL2/std.scad\";\nlibbox();\n");

    let mut kernel = TruckKernel::new();
    let options = EvaluationOptions {
        tolerance: 0.01,
        legacy_centering: false,
        library_paths: vec![dir.path().join("lib")],
    };
    let error = evaluate_file_with_options(&main, &mut kernel, options)
        .expect_err("quoted include should not use library paths");
    assert!(matches!(error, EvalError::Source(_)));
}

#[test]
fn function_declarations_hoist_across_includes() {
    let dir = TempDir::new();
    dir.write("first.scad", "cube(triple(2));\n");
    dir.write("second.scad", "function triple(x) = x * 3;\n");
    let main = dir.write(
        "main.scad",
        "include \"first.scad\";\ninclude \"second.scad\";\n",
    );

    assert_eq!(evaluate_file_extents(&main), [6.0; 3]);
}

#[test]
fn nested_includes_resolve_relative_to_each_file() {
    let dir = TempDir::new();
    dir.write("sub/inner.scad", "module inner() { cube(2); }\n");
    dir.write(
        "sub/outer.scad",
        "include \"inner.scad\";\nmodule outer() { inner(); }\n",
    );
    let main = dir.write("main.scad", "include \"sub/outer.scad\";\nouter();\n");

    assert_eq!(evaluate_file_extents(&main), [2.0; 3]);
}

#[test]
fn nested_use_imports_definitions_transitively() {
    let dir = TempDir::new();
    dir.write("inner.scad", "module inner() { cube(2); }\n");
    dir.write("outer.scad", "use \"inner.scad\";\n");
    let main = dir.write("main.scad", "use \"outer.scad\";\ninner();\n");

    assert_eq!(evaluate_file_extents(&main), [2.0; 3]);
}

#[test]
fn angle_bracket_paths_resolve_relative_to_the_source() {
    let dir = TempDir::new();
    dir.write("lib.scad", "module box() { cube(2); }\n");
    let main = dir.write("main.scad", "include <lib.scad>\nbox();\n");

    assert_eq!(evaluate_file_extents(&main), [2.0; 3]);
}

#[test]
fn include_composes_with_a_parent_transform() {
    let dir = TempDir::new();
    dir.write("part.scad", "cube(1);\n");
    let main = dir.write("main.scad", "translate([5, 0, 0]) include \"part.scad\";\n");

    let mut kernel = TruckKernel::new();
    let evaluation = evaluate_file(&main, &mut kernel, 0.01).expect("evaluate file");
    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!((min[0] - 4.5).abs() < 1.0e-6, "min = {min:?}");
    assert!((max[0] - 5.5).abs() < 1.0e-6, "max = {max:?}");
}

#[test]
fn include_cycles_are_reported() {
    let dir = TempDir::new();
    let a = dir.write("a.scad", "include \"b.scad\";\n");
    dir.write("b.scad", "include \"a.scad\";\n");

    let mut kernel = TruckKernel::new();
    let error = evaluate_file(&a, &mut kernel, 0.01).expect_err("cycle");

    assert!(
        matches!(&error, EvalError::Source(message) if message.contains("cycle")),
        "error = {error:?}"
    );
}

#[test]
fn missing_includes_name_the_file() {
    let dir = TempDir::new();
    let main = dir.write("main.scad", "include \"missing.scad\";\n");

    let mut kernel = TruckKernel::new();
    let error = evaluate_file(&main, &mut kernel, 0.01).expect_err("missing file");

    assert!(
        matches!(&error, EvalError::Source(message) if message.contains("missing.scad")),
        "error = {error:?}"
    );
}

#[test]
fn parse_errors_in_included_files_name_the_file() {
    let dir = TempDir::new();
    dir.write("bad.scad", "cube([2, 2;\n");
    let main = dir.write("main.scad", "include \"bad.scad\";\n");

    let mut kernel = TruckKernel::new();
    let error = evaluate_file(&main, &mut kernel, 0.01).expect_err("bad include");

    assert!(
        matches!(&error, EvalError::Parse(message) if message.contains("bad.scad")),
        "error = {error:?}"
    );
}

#[test]
fn includes_require_a_source_path() {
    let mut kernel = TruckKernel::new();
    let error =
        evaluate("include \"part.scad\";", &mut kernel, 0.01).expect_err("no source context");

    assert!(matches!(error, EvalError::Source(_)));
}

#[test]
fn temp_dir_path_is_created() {
    let dir = TempDir::new();
    assert!(dir.path().is_dir());
}
