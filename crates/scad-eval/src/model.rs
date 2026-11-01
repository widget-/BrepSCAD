//! Public evaluation results and the source-shaped tree.

use std::ops::Range;

use brepscad_kernel_api::Mesh;

/// The result of evaluating a source file.
#[derive(Debug, Clone)]
pub struct Evaluation {
    pub shapes: Vec<EvaluatedShape>,
    /// Messages collected from `echo(...)`, in evaluation order.
    pub messages: Vec<String>,
    /// Analytic scene over every top-level shape, when the caller asked for it
    /// and the kernel could describe them. Used by ray-cast preview and picking.
    pub scene: Option<brepscad_raycast::Scene>,
}

/// One top-level shape, its final mesh, and its selectable operands.
#[derive(Debug, Clone)]
pub struct EvaluatedShape {
    pub mesh: Mesh,
    pub source_range: Range<usize>,
    pub parameters: Vec<Parameter>,
    pub parts: Vec<Part>,
    pub tree: TreeNode,
}

/// One node in the source-shaped tree of a top-level shape.
///
/// Leaf nodes carry the index of their entry in [`EvaluatedShape::parts`].
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub module: String,
    pub source_range: Range<usize>,
    pub children: Vec<TreeNode>,
    pub part_index: Option<usize>,
}

/// A selectable operand, usually a primitive inside a boolean.
#[derive(Debug, Clone)]
pub struct Part {
    pub module: String,
    pub source_range: Range<usize>,
    pub parameters: Vec<Parameter>,
    pub mesh: Mesh,
}

/// A numeric literal reachable from a shape or part's source range.
#[derive(Debug, Clone)]
pub struct Parameter {
    pub range: Range<usize>,
    pub value: f64,
    pub source: String,
}
