//! Minimal evaluator connecting the lossless SCAD syntax tree to a kernel.
//!
//! The supported subset is deliberately small: top-level assignments,
//! primitives, transforms, arithmetic expressions, and the boolean modules.

mod cache;
mod evaluator;
mod model;
mod parameters;
mod parts;
mod profile;
mod value;

use brepscad_kernel_api::KernelError;
use thiserror::Error;

pub use cache::MeshCache;
pub use evaluator::{evaluate, evaluate_to_meshes, evaluate_with_cache};
pub use model::{EvaluatedShape, Evaluation, Parameter, Part, TreeNode};

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("unknown identifier: {0}")]
    UnknownIdentifier(String),
    #[error("unsupported module: {0}")]
    UnsupportedModule(String),
    #[error("bad arguments for {module}: {message}")]
    BadArguments { module: String, message: String },
    #[error("invalid transform: {0}")]
    InvalidTransform(String),
    #[error("expression error: {0}")]
    Expression(String),
    #[error("kernel error: {0}")]
    Kernel(#[from] KernelError),
}

pub(crate) fn bad_args(module: &str, message: impl Into<String>) -> EvalError {
    EvalError::BadArguments {
        module: module.to_owned(),
        message: message.into(),
    }
}
