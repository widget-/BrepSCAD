//! Kernel abstraction shared by all B-rep backends.
//!
//! Keep kernel-specific geometry types out of this crate and out of the UI.

/// Opaque handle to geometry owned by a kernel arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShapeId(pub u64);

/// A primitive requested from a kernel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Primitive {
    Cuboid { size: [f64; 3] },
    Cylinder { radius: f64, height: f64 },
}

/// A boolean operation between two shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOp {
    Union,
    Difference,
    Intersection,
}

/// A neutral affine transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Transform {
    Translate([f64; 3]),
    /// Euler angles in radians, applied around X, then Y, then Z.
    Rotate([f64; 3]),
    Scale([f64; 3]),
}

/// A neutral triangle mesh produced by tessellation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f64; 3]>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Runtime capability flags used by the UI to enable or grey out operations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub primitives: bool,
    pub transforms: bool,
    pub booleans: bool,
    pub tessellation: bool,
    pub fillet: bool,
    pub step_io: bool,
}

/// Errors returned by a kernel adapter.
#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("operation not supported by this kernel: {0}")]
    Unsupported(&'static str),
    #[error("unknown shape: {0:?}")]
    UnknownShape(ShapeId),
    #[error("kernel backend error: {0}")]
    Backend(String),
}

/// Minimal kernel interface. Geometry methods have default "unsupported"
/// implementations so a backend can grow incrementally.
pub trait Kernel {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;

    fn primitive(&mut self, _primitive: Primitive) -> Result<ShapeId, KernelError> {
        Err(KernelError::Unsupported("primitive"))
    }

    fn transform(
        &mut self,
        _shape: ShapeId,
        _transform: Transform,
    ) -> Result<ShapeId, KernelError> {
        Err(KernelError::Unsupported("transform"))
    }

    fn boolean(
        &mut self,
        _op: BooleanOp,
        _a: ShapeId,
        _b: ShapeId,
    ) -> Result<ShapeId, KernelError> {
        Err(KernelError::Unsupported("boolean"))
    }

    fn tessellate(&self, _shape: ShapeId, _tolerance: f64) -> Result<Mesh, KernelError> {
        Err(KernelError::Unsupported("tessellate"))
    }
}

/// Version of the kernel-api crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_capabilities_are_empty() {
        let caps = Capabilities::default();
        assert!(!caps.primitives);
        assert!(!caps.transforms);
        assert!(!caps.booleans);
        assert!(!caps.tessellation);
        assert!(!caps.fillet);
        assert!(!caps.step_io);
    }

    #[test]
    fn unsupported_error_mentions_operation() {
        let err = KernelError::Unsupported("fillet");
        assert!(err.to_string().contains("fillet"));
    }

    #[test]
    fn triangle_count_uses_index_triples() {
        let mesh = Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            indices: vec![0, 1, 2],
        };
        assert_eq!(mesh.triangle_count(), 1);
    }
}
