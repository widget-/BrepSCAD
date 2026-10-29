//! Kernel abstraction scaffold.
//!
//! Backend adapters (OCCT, SolidCore, Truck) will implement [`Kernel`].
//! Keep kernel-specific geometry types out of this crate and out of the UI.

/// Opaque handle to geometry owned by a kernel arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShapeId(pub u64);

/// Runtime capability flags used by the UI to enable or grey out operations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub booleans: bool,
    pub fillet: bool,
    pub step_io: bool,
}

/// Errors returned by a kernel adapter.
#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("operation not supported by this kernel: {0}")]
    Unsupported(&'static str),
}

/// Minimal kernel interface. Geometry methods will be added with the first
/// real backend.
pub trait Kernel {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;
}

/// Version of the kernel-api crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_capabilities_are_empty() {
        let caps = Capabilities::default();
        assert!(!caps.booleans);
        assert!(!caps.fillet);
        assert!(!caps.step_io);
    }

    #[test]
    fn unsupported_error_mentions_operation() {
        let err = KernelError::Unsupported("fillet");
        assert!(err.to_string().contains("fillet"));
    }
}
