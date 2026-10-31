//! Minimal evaluator connecting the lossless SCAD syntax tree to a kernel.
//!
//! The supported subset is deliberately small: top-level assignments,
//! primitives, transforms, arithmetic expressions, and the boolean modules.

mod cache;
mod evaluator;
mod hull;
mod model;
mod parameters;
mod parts;
mod profile;
mod scope;
mod value;

use brepscad_kernel_api::KernelError;
use thiserror::Error;

pub use cache::MeshCache;
pub use evaluator::{
    EvaluationOptions, environment_library_paths, evaluate, evaluate_file,
    evaluate_file_with_cache_options, evaluate_file_with_options, evaluate_source_with_path,
    evaluate_to_meshes, evaluate_to_meshes_with_options, evaluate_with_cache,
    evaluate_with_cache_options, evaluate_with_options,
};
pub use model::{EvaluatedShape, Evaluation, Parameter, Part, TreeNode};

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("unsupported module: {0}")]
    UnsupportedModule(String),
    #[error("unknown function: {0}")]
    UnknownFunction(String),
    #[error("assertion failed: {0}")]
    Assertion(String),
    #[error("bad arguments for {module}: {message}")]
    BadArguments { module: String, message: String },
    #[error("invalid transform: {0}")]
    InvalidTransform(String),
    #[error("expression error: {0}")]
    Expression(String),
    #[error("source error: {0}")]
    Source(String),
    #[error("kernel error: {0}")]
    Kernel(#[from] KernelError),
}

pub(crate) fn bad_args(module: &str, message: impl Into<String>) -> EvalError {
    EvalError::BadArguments {
        module: module.to_owned(),
        message: message.into(),
    }
}
