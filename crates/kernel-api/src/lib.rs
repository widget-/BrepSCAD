//! Kernel abstraction shared by all B-rep backends.
//!
//! Keep kernel-specific geometry types out of this crate and out of the UI.

mod transform;

pub use transform::Transform;

/// Opaque handle to geometry owned by a kernel arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShapeId(pub u64);

/// A primitive requested from a kernel.
#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    Cuboid {
        size: [f64; 3],
    },
    Cylinder {
        radius: f64,
        height: f64,
    },
    Sphere {
        radius: f64,
    },
    Polyhedron {
        points: Vec<[f64; 3]>,
        faces: Vec<Vec<usize>>,
    },
}

/// A planar sketch requested from a kernel.
#[derive(Debug, Clone, PartialEq)]
pub enum Sketch {
    /// Axis-aligned rectangle centered on the origin in the XY plane.
    Square { size: [f64; 2] },
    /// Disk centered on the origin in the XY plane.
    Circle { radius: f64 },
    /// Simple polygon in the XY plane; points are used as given.
    Polygon { points: Vec<[f64; 2]> },
}

/// A planar region with an outer contour and optional holes.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileRegion {
    pub outer: Vec<[f64; 2]>,
    pub holes: Vec<Vec<[f64; 2]>>,
}

/// A boolean operation between two shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOp {
    Union,
    Difference,
    Intersection,
}

/// A neutral triangle mesh produced by tessellation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f64; 3]>,
    /// Unit normals parallel to `positions`, taken from the B-rep surfaces the
    /// triangles were tessellated from. Empty when the backend cannot supply
    /// them, in which case consumers derive flat normals from the triangles.
    pub normals: Vec<[f64; 3]>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// True when every position has a matching normal.
    pub fn has_normals(&self) -> bool {
        !self.positions.is_empty() && self.normals.len() == self.positions.len()
    }
}

/// A solid given as a closed polyhedron: shared vertices and one index loop per
/// face, in the order the B-rep boundary walks it.
///
/// Only planar-faced solids have one. `hull` and convex `minkowski` need nothing
/// more than this, and taking it from the topology avoids tessellating shapes
/// whose result never depends on the display tolerance.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Polyhedron {
    pub vertices: Vec<[f64; 3]>,
    pub faces: Vec<Vec<usize>>,
}

/// Runtime capability flags used by the UI to enable or grey out operations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub primitives: bool,
    pub sketches: bool,
    pub extrusions: bool,
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

    fn sketch(&mut self, _sketch: Sketch) -> Result<ShapeId, KernelError> {
        Err(KernelError::Unsupported("sketch"))
    }

    /// Build one or more planar regions with holes, as produced by 2D CSG.
    fn sketch_profile(&mut self, _regions: &[ProfileRegion]) -> Result<ShapeId, KernelError> {
        Err(KernelError::Unsupported("sketch_profile"))
    }

    fn extrude(&mut self, _shape: ShapeId, _height: f64) -> Result<ShapeId, KernelError> {
        Err(KernelError::Unsupported("extrude"))
    }

    /// Revolve a planar sketch around the Z axis.
    ///
    /// `angle` is in radians, with the sweep direction following its sign. The
    /// sketch's local X becomes the radius and local Y becomes Z, matching
    /// OpenSCAD's `rotate_extrude`.
    /// Revolve a planar sketch around the Z axis.
    ///
    /// `start` rotates the profile about Z before sweeping; `angle` is the
    /// sweep in radians.
    fn revolve(
        &mut self,
        _shape: ShapeId,
        _start: f64,
        _angle: f64,
    ) -> Result<ShapeId, KernelError> {
        Err(KernelError::Unsupported("revolve"))
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

    /// Analytic scene for a solid shape, used by ray-cast preview and picking.
    ///
    /// Backends that cannot describe a shape analytically return `None`, and
    /// callers fall back to the tessellated mesh.
    fn raycast_scene(&self, _shape: ShapeId, _tolerance: f64) -> Option<brepscad_raycast::Scene> {
        None
    }

    /// Axis-aligned bounds in the shape's local coordinates.
    fn bounds(
        &self,
        _shape: ShapeId,
        _tolerance: f64,
    ) -> Result<([f64; 3], [f64; 3]), KernelError> {
        Err(KernelError::Unsupported("bounds"))
    }

    /// The solid as a polyhedron, when every face is planar.
    ///
    /// Backends that cannot answer, or shapes with any curved or non-simple
    /// face, return `None`.
    fn polyhedron(&self, _shape: ShapeId) -> Option<Polyhedron> {
        None
    }

    /// `shape ⊕ ball(radius)`, for backends that can round a solid off.
    fn offset(&mut self, _shape: ShapeId, _radius: f64) -> Result<ShapeId, KernelError> {
        Err(KernelError::Unsupported("offset"))
    }

    /// The radius, when the shape is a ball.
    ///
    /// `minkowski` uses this to recognise the `solid ⊕ sphere` rounding idiom:
    /// with the radius in hand it can offset the solid exactly instead of
    /// summing sampled points.
    fn ball_radius(&self, _shape: ShapeId) -> Option<f64> {
        None
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
        assert!(!caps.sketches);
        assert!(!caps.extrusions);
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
            normals: Vec::new(),
            indices: vec![0, 1, 2],
        };
        assert_eq!(mesh.triangle_count(), 1);
    }

    #[test]
    fn normals_are_only_trusted_when_they_cover_every_position() {
        let mut mesh = Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            indices: vec![0, 1, 2],
        };
        assert!(mesh.has_normals());

        mesh.normals.pop();
        assert!(!mesh.has_normals());

        mesh.normals.clear();
        mesh.positions.clear();
        assert!(!mesh.has_normals());
    }
}
