//! Kernel-neutral affine transform.

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

    /// Rotation of `angle` radians around `axis`.
    pub fn from_rotation_axis(axis: [f64; 3], angle: f64) -> Option<Self> {
        let [x, y, z] = axis;
        let length_squared = x * x + y * y + z * z;
        if !length_squared.is_finite() || length_squared == 0.0 {
            return None;
        }
        let inverse = length_squared.sqrt().recip();
        let [x, y, z] = [x * inverse, y * inverse, z * inverse];
        let (sin, cos) = angle.sin_cos();
        let one_minus_cos = 1.0 - cos;
        Some(Self::from_rows([
            [
                one_minus_cos * x * x + cos,
                one_minus_cos * x * y - sin * z,
                one_minus_cos * x * z + sin * y,
                0.0,
            ],
            [
                one_minus_cos * x * y + sin * z,
                one_minus_cos * y * y + cos,
                one_minus_cos * y * z - sin * x,
                0.0,
            ],
            [
                one_minus_cos * x * z - sin * y,
                one_minus_cos * y * z + sin * x,
                one_minus_cos * z * z + cos,
                0.0,
            ],
            [0.0, 0.0, 0.0, 1.0],
        ]))
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn axis_angle_matches_axis_rotations() {
        let z = Transform::from_rotation_axis([0.0, 0.0, 2.0], std::f64::consts::FRAC_PI_2)
            .expect("axis rotation");
        let expected = Transform::from_rotation_z(std::f64::consts::FRAC_PI_2);
        for column in 0..4 {
            for row in 0..4 {
                assert!((z.cols[column][row] - expected.cols[column][row]).abs() < 1.0e-12);
            }
        }
        assert!(Transform::from_rotation_axis([0.0, 0.0, 0.0], 1.0).is_none());
    }
}
