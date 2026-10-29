//! Lossless syntax tree and text edits for a small OpenSCAD subset.
//!
//! The first slice supports assignments, module calls, numbers, identifiers,
//! vectors, comments, and whitespace. The tree preserves the source byte for
//! byte, so edits can be applied as text ranges without reformatting.

#![forbid(unsafe_code)]

mod edit;
mod lexer;
mod parser;
mod syntax;

pub use edit::{EditError, TextEdit, apply_edit};
pub use parser::{Parse, parse};
pub use rowan::{TextRange, TextSize};
pub use syntax::{ScadLanguage, SyntaxError, SyntaxKind, SyntaxNode, SyntaxToken};
