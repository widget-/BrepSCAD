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

/// A neutral column-major affine transform.
///
/// `cols[column][row]`. Use [`Transform::compose`] to combine an outer
/// transform with a child transform; the child is applied first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub cols: [[f64; 4]; 4],
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        cols: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    pub fn from_translation([x, y, z]: [f64; 3]) -> Self {
        let mut transform = Self::IDENTITY;
        transform.cols[3][0] = x;
        transform.cols[3][1] = y;
        transform.cols[3][2] = z;
        transform
    }

    pub fn from_scale([x, y, z]: [f64; 3]) -> Self {
        let mut transform = Self::IDENTITY;
        transform.cols[0][0] = x;
        transform.cols[1][1] = y;
        transform.cols[2][2] = z;
        transform
    }

    pub fn from_rotation_x(angle: f64) -> Self {
        let (sin, cos) = angle.sin_cos();
        let mut transform = Self::IDENTITY;
        transform.cols[1][1] = cos;
        transform.cols[1][2] = sin;
        transform.cols[2][1] = -sin;
        transform.cols[2][2] = cos;
        transform
    }

    pub fn from_rotation_y(angle: f64) -> Self {
        let (sin, cos) = angle.sin_cos();
        let mut transform = Self::IDENTITY;
        transform.cols[0][0] = cos;
        transform.cols[0][2] = -sin;
        transform.cols[2][0] = sin;
        transform.cols[2][2] = cos;
        transform
    }

    pub fn from_rotation_z(angle: f64) -> Self {
        let (sin, cos) = angle.sin_cos();
        let mut transform = Self::IDENTITY;
        transform.cols[0][0] = cos;
        transform.cols[0][1] = sin;
        transform.cols[1][0] = -sin;
        transform.cols[1][1] = cos;
        transform
    }

    /// Euler angles in radians, applied around X, then Y, then Z.
    pub fn from_rotation_xyz([x, y, z]: [f64; 3]) -> Self {
        Self::from_rotation_z(z)
            .compose(Self::from_rotation_y(y))
            .compose(Self::from_rotation_x(x))
    }

    /// Builds a transform from row-major values, the order OpenSCAD's
    /// `multmatrix` uses.
    pub fn from_rows(rows: [[f64; 4]; 4]) -> Self {
        let mut cols = [[0.0; 4]; 4];
        for (row, values) in rows.iter().enumerate() {
            for (column, value) in values.iter().enumerate() {
                cols[column][row] = *value;
            }
        }
        Self { cols }
    }

    /// Reflection across the plane through the origin with `normal`.
    ///
    /// Returns `None` if the normal is zero or non-finite.
    pub fn from_mirror([x, y, z]: [f64; 3]) -> Option<Self> {
        let length_squared = x * x + y * y + z * z;
        if !length_squared.is_finite() || length_squared == 0.0 {
            return None;
        }

        let normal = [x, y, z];
        let mut transform = Self::IDENTITY;
        for column in 0..3 {
            for row in 0..3 {
                let factor = -2.0 * normal[row] * normal[column] / length_squared;
                transform.cols[column][row] = if row == column { 1.0 + factor } else { factor };
            }
        }
        Some(transform)
    }

    /// Determinant of the linear (upper-left 3×3) part.
    pub fn determinant(&self) -> f64 {
        let [a, b, c] = [self.cols[0][0], self.cols[1][0], self.cols[2][0]];
        let [d, e, f] = [self.cols[0][1], self.cols[1][1], self.cols[2][1]];
        let [g, h, i] = [self.cols[0][2], self.cols[1][2], self.cols[2][2]];
        a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g)
    }

    /// Whether every entry is finite.
    pub fn is_finite(&self) -> bool {
        self.cols.iter().flatten().all(|value| value.is_finite())
    }

    /// Returns `self * child`: the child transform is applied first.
    pub fn compose(self, child: Self) -> Self {
        let mut cols = [[0.0; 4]; 4];
        for (column, out_col) in cols.iter_mut().enumerate() {
            for (row, out) in out_col.iter_mut().enumerate() {
                *out = (0..4)
                    .map(|k| self.cols[k][row] * child.cols[column][k])
                    .sum();
            }
        }
        Self { cols }
    }

    pub fn apply(self, [x, y, z]: [f64; 3]) -> [f64; 3] {
        let out_x =
            self.cols[0][0] * x + self.cols[1][0] * y + self.cols[2][0] * z + self.cols[3][0];
        let out_y =
            self.cols[0][1] * x + self.cols[1][1] * y + self.cols[2][1] * z + self.cols[3][1];
        let out_z =
            self.cols[0][2] * x + self.cols[1][2] * y + self.cols[2][2] * z + self.cols[3][2];
        [out_x, out_y, out_z]
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }
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

    #[test]
    fn transform_composes_child_before_parent() {
        let transform = Transform::from_translation([1.0, 0.0, 0.0])
            .compose(Transform::from_scale([2.0, 2.0, 2.0]));

        let point = transform.apply([1.0, 0.0, 0.0]);
        assert!((point[0] - 3.0).abs() < 1e-12);
        assert!(point[1].abs() < 1e-12);
        assert!(point[2].abs() < 1e-12);
    }

    #[test]
    fn rotation_xyz_rotates_x_into_y() {
        let transform = Transform::from_rotation_xyz([0.0, 0.0, std::f64::consts::FRAC_PI_2]);

        let point = transform.apply([1.0, 0.0, 0.0]);
        assert!(point[0].abs() < 1e-12);
        assert!((point[1] - 1.0).abs() < 1e-12);
        assert!(point[2].abs() < 1e-12);
    }

    #[test]
    fn from_rows_uses_open_scad_row_order() {
        let transform = Transform::from_rows([
            [1.0, 0.0, 0.0, 5.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]);

        assert_eq!(transform.apply([1.0, 2.0, 3.0]), [6.0, 2.0, 3.0]);
    }

    #[test]
    fn mirror_reflects_across_the_normal_plane() {
        let transform = Transform::from_mirror([1.0, 0.0, 0.0]).expect("normal");
        let point = transform.apply([2.0, 1.0, 3.0]);

        assert!((point[0] + 2.0).abs() < 1e-12);
        assert!((point[1] - 1.0).abs() < 1e-12);
        assert!((point[2] - 3.0).abs() < 1e-12);
        assert!((transform.determinant() + 1.0).abs() < 1e-12);
        assert!(Transform::from_mirror([0.0, 0.0, 0.0]).is_none());
    }

    #[test]
    fn determinant_reports_scale_and_singularity() {
        let scaled = Transform::from_scale([2.0, 3.0, 4.0]);
        assert!(scaled.is_finite());
        assert!((scaled.determinant() - 24.0).abs() < 1e-12);

        let singular = Transform::from_scale([1.0, 0.0, 1.0]);
        assert!(singular.determinant().abs() < 1e-12);

        let mut not_finite = Transform::IDENTITY;
        not_finite.cols[0][0] = f64::NAN;
        assert!(!not_finite.is_finite());
    }
}
