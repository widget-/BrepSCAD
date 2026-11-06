mod common;

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use brepscad_eval::{EvaluationOptions, evaluate, evaluate_file_with_options, evaluate_to_meshes};
use brepscad_kernel_api::{
    BooleanOp, Capabilities, Kernel, KernelError, Mesh, Primitive, ShapeId, Transform,
};
use brepscad_kernel_truck::TruckKernel;
use common::{bounds, signed_volume, welded_boundary_edges};

/// Counts boolean calls so tests can see whether an operand reached the kernel.
struct CountingKernel {
    inner: TruckKernel,
    booleans: Rc<Cell<usize>>,
}

impl CountingKernel {
    fn new() -> (Self, Rc<Cell<usize>>) {
        let booleans = Rc::new(Cell::new(0));
        (
            Self {
                inner: TruckKernel::new(),
                booleans: Rc::clone(&booleans),
            },
            booleans,
        )
    }
}

impl Kernel for CountingKernel {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }

    fn primitive(&mut self, primitive: Primitive) -> Result<ShapeId, KernelError> {
        self.inner.primitive(primitive)
    }

    fn transform(&mut self, shape: ShapeId, transform: Transform) -> Result<ShapeId, KernelError> {
        self.inner.transform(shape, transform)
    }

    fn boolean(&mut self, op: BooleanOp, a: ShapeId, b: ShapeId) -> Result<ShapeId, KernelError> {
        self.booleans.set(self.booleans.get() + 1);
        self.inner.boolean(op, a, b)
    }

    fn tessellate(&self, shape: ShapeId, tolerance: f64) -> Result<Mesh, KernelError> {
        self.inner.tessellate(shape, tolerance)
    }

    fn aabb(&self, shape: ShapeId) -> Option<([f64; 3], [f64; 3])> {
        self.inner.aabb(shape)
    }
}

#[test]
fn evaluates_a_cube_with_a_hole() {
    let source =
        "size = [2, 2, 2];\ndifference() {\n    cube(size);\n    cylinder(h=4, r=0.5);\n}\n";
    let mut kernel = TruckKernel::new();
    let meshes = evaluate_to_meshes(source, &mut kernel, 0.01).expect("evaluate");

    assert_eq!(meshes.len(), 1);
    assert!(meshes[0].triangle_count() > 0);
}

#[test]
fn exposes_a_source_shaped_tree() {
    let source = "difference() {\n    cube([2, 2, 2]);\n    translate([0.2, 0, 0]) cylinder(h=4, r=0.5);\n}\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    let tree = &shape.tree;
    assert_eq!(tree.module, "difference");
    assert_eq!(tree.part_index, None);
    assert_eq!(tree.children.len(), 2);

    let cube = &tree.children[0];
    assert_eq!(cube.module, "cube");
    assert_eq!(cube.part_index, Some(0));
    assert!(cube.children.is_empty());
    assert_eq!(shape.parts[0].module, "cube");
    assert_eq!(shape.parts[0].source_range, cube.source_range);

    let translate = &tree.children[1];
    assert_eq!(translate.module, "translate");
    assert_eq!(translate.part_index, None);
    assert_eq!(translate.children[0].module, "cylinder");
    assert_eq!(translate.children[0].part_index, Some(1));
    assert_eq!(shape.parts[1].module, "cylinder");
    assert_eq!(
        shape.parts[1].source_range,
        translate.children[0].source_range
    );
}

#[test]
fn evaluates_single_child_boolean_forms() {
    let source = "difference() translate([1, 0, 0]) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    assert_eq!(shape.parts.len(), 1);
    assert!(shape.mesh.triangle_count() > 0);
    let (min, _) = bounds(&shape.mesh);
    assert!((min[0] - 0.0).abs() < 1e-6, "min_x = {}", min[0]);
}

#[test]
fn difference_with_a_sphere_tessellates() {
    let source = "difference() { cube([2, 2, 2]); sphere(1.2); }\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    assert!(evaluation.shapes[0].mesh.triangle_count() > 0);
    assert_eq!(evaluation.shapes[0].parts.len(), 2);
}

/// The horizontal and vertical clip cutouts overlap inside the shell. Their
/// ends stop at full cross-sections: the original 1e-6 taper kept its 0.4/0.2
/// chamfer, which makes the end cap a self-intersecting octagon, and the
/// resulting knife-edge overlap is below the boolean's tolerance. Expected
/// volume is the OpenSCAD result, 3849.09.
#[test]
fn snap_clip_cutouts_keep_the_flex_slot() {
    let source = include_str!("data/snap_clip_cutout.scad");
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    assert_eq!(evaluation.shapes.len(), 1);
    let volume = signed_volume(&evaluation.shapes[0].mesh);
    assert!(
        (volume - 3849.09).abs() < 20.0,
        "volume = {volume}; expected about 3849.09"
    );
    assert_eq!(
        welded_boundary_edges(&evaluation.shapes[0].mesh),
        0,
        "both cutouts leave a closed shell"
    );
}

/// The hull cut's bottom plane is exactly at the cell cutters' 45-degree
/// crease. Its intersection curves run along several face boundaries; those
/// faces must be ray-cast instead of keeping their boundary-derived status, or
/// the internal cell walls survive and the volume is about 27148 instead of
/// 23761.
#[test]
fn hull_cut_on_the_cell_crease_keeps_the_inner_cavity() {
    let source = include_str!("data/opengrid_hull_cut.scad");
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    assert_eq!(evaluation.shapes.len(), 1);
    let volume = signed_volume(&evaluation.shapes[0].mesh);
    assert!(
        (volume - 23760.94).abs() < 20.0,
        "volume = {volume}; expected about 23760.94"
    );
}

/// A cutter whose bounds miss the subject is a no-op. The evaluator skips the
/// kernel call but keeps the cutter in the source-shaped tree.
#[test]
fn difference_skips_a_cutter_whose_bounds_miss_the_subject() {
    let (mut kernel, booleans) = CountingKernel::new();
    let source = "difference() {\n    cube(10, center = true);\n    translate([100, 0, 0]) cube(2, center = true);\n}\n";
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    assert_eq!(evaluation.shapes.len(), 1);
    assert_eq!(evaluation.shapes[0].tree.children.len(), 2);
    assert_eq!(booleans.get(), 0, "disjoint cutter reached the kernel");
}

/// A cutter that touches the subject must still be subtracted.
#[test]
fn difference_keeps_a_cutter_that_touches_the_subject() {
    let (mut kernel, booleans) = CountingKernel::new();
    let source = "difference() {\n    cube(10, center = true);\n    translate([4, 0, 0]) cube(2, center = true);\n}\n";
    evaluate(source, &mut kernel, 0.01).expect("evaluate");

    assert_eq!(booleans.get(), 1, "touching cutter was pruned");
}

fn evaluate_snap_fixture(name: &str) -> brepscad_eval::Evaluation {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/opengrid_snap")
        .join(name);
    let mut kernel = TruckKernel::new();
    evaluate_file_with_options(&path, &mut kernel, EvaluationOptions::default())
        .expect("evaluate snap fixture")
}

/// One clip from the OpenGrid snap used to overflow the stack: the boolean
/// engine's curve/surface projection recursed once per retry, and the
/// shell-set union recursed again on failed fusions. See the fixture README.
///
/// The expected volume is OpenSCAD's, measured with
/// `openscad -o out.off --export-format off` on the same fixture. It is not
/// what the engine returns today (about 1502.6): that figure came from an
/// earlier BrepSCAD run and was pinned here as a stack-overflow repro, not as
/// a correctness reference. The gap is the planar fallback leaving the clip
/// cutouts open.
#[test]
fn opengrid_snap_with_one_clip_does_not_overflow_the_stack() {
    let evaluation = evaluate_snap_fixture("snap_one_clip.scad");
    assert_eq!(evaluation.shapes.len(), 1);
    let volume = signed_volume(&evaluation.shapes[0].mesh);
    assert!(
        (volume - 1495.32).abs() < 5.0,
        "volume = {volume}; OpenSCAD gives 1495.32. The planar fallback leaves \
         the clip cutouts open, so the mesh keeps material the cutter removed."
    );
}

/// Two clips are the smallest variant that overflowed the shell-set merge on
/// its own, after the projection retry recursion was fixed. The expected
/// volume is OpenSCAD's; see the note on the one-clip test.
#[test]
fn opengrid_snap_with_two_clips_does_not_overflow_the_stack() {
    let evaluation = evaluate_snap_fixture("snap_two_clips.scad");
    assert_eq!(evaluation.shapes.len(), 1);
    let volume = signed_volume(&evaluation.shapes[0].mesh);
    assert!(
        (volume - 1452.39).abs() < 5.0,
        "volume = {volume}; OpenSCAD gives 1452.39. The planar fallback leaves \
         the clip cutouts open, so the mesh drops material the subject keeps."
    );
}
