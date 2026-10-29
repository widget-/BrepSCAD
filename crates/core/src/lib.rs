//! Core library scaffold.
//!
//! The parser, HIR, evaluator, and instantiated model tree will live here.

/// Returns a greeting used by the scaffold binary.
pub fn hello() -> &'static str {
    "Hello from visual-openscad"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_is_not_empty() {
        assert!(!hello().is_empty());
    }
}
