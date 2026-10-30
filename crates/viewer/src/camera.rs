use glam::camera::rh::{proj::directx, view};
use glam::{Mat4, Vec2, Vec3};
use visual_openscad_kernel_api::Mesh;

#[derive(Debug, Clone)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
    pub fov_y: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 0.8,
            pitch: 0.45,
            distance: 8.0,
            target: Vec3::ZERO,
            fov_y: 45.0_f32.to_radians(),
        }
    }
}

impl Camera {
    pub fn orbit(&mut self, delta: Vec2) {
        self.yaw -= delta.x * 0.01;
        self.pitch = (self.pitch + delta.y * 0.01).clamp(-1.45, 1.45);
    }

    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance * (1.0 - delta * 0.001)).clamp(0.05, 10_000.0);
    }

    pub fn eye(&self) -> Vec3 {
        let horizontal = self.pitch.cos();
        self.target
            + Vec3::new(
                self.yaw.sin() * horizontal,
                self.yaw.cos() * horizontal,
                self.pitch.sin(),
            ) * self.distance
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        let projection = directx::perspective(self.fov_y, aspect.max(0.01), 0.001, 100_000.0);
        let view = view::look_at_mat4(self.eye(), self.target, Vec3::Z);
        projection * view
    }

    /// Returns a world-space ray through normalized device coordinates.
    pub fn ray(&self, ndc: Vec2, aspect: f32) -> (Vec3, Vec3) {
        let forward = (self.target - self.eye()).normalize();
        let right = forward.cross(Vec3::Z).normalize();
        let up = right.cross(forward);
        let tan_half_fov = (self.fov_y * 0.5).tan();
        let direction = (forward
            + right * (ndc.x * tan_half_fov * aspect.max(0.01))
            + up * (ndc.y * tan_half_fov))
            .normalize();
        (self.eye(), direction)
    }

    pub fn fit(&mut self, mesh: &Mesh) {
        if mesh.positions.is_empty() {
            *self = Self::default();
            return;
        }

        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for position in &mesh.positions {
            let point = Vec3::new(position[0] as f32, position[1] as f32, position[2] as f32);
            min = min.min(point);
            max = max.max(point);
        }

        self.target = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(0.001);
        self.distance = (radius / (self.fov_y * 0.5).tan() * 2.5).clamp(0.05, 10_000.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube_mesh() -> Mesh {
        Mesh {
            positions: vec![
                [-1.0, -1.0, -1.0],
                [1.0, -1.0, -1.0],
                [1.0, 1.0, 1.0],
                [-1.0, 1.0, 1.0],
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }

    #[test]
    fn fit_centers_and_frames_a_mesh() {
        let mut camera = Camera::default();
        camera.fit(&cube_mesh());

        assert!(camera.target.length() < 1.0e-6);
        assert!(camera.distance > 0.0);
    }

    #[test]
    fn view_projection_is_finite() {
        let camera = Camera::default();
        let matrix = camera.view_projection(16.0 / 9.0);
        assert!(matrix.to_cols_array().iter().all(|value| value.is_finite()));
    }

    #[test]
    fn orbit_clamps_pitch() {
        let mut camera = Camera::default();
        camera.orbit(Vec2::new(0.0, 10_000.0));
        assert!(camera.pitch <= 1.45);
        camera.orbit(Vec2::new(0.0, -10_000.0));
        assert!(camera.pitch >= -1.45);
    }

    #[test]
    fn world_up_is_z() {
        let camera = Camera::default();
        let clip = camera.view_projection(16.0 / 9.0) * Vec3::new(0.0, 0.0, 1.0).extend(1.0);
        let ndc_y = clip.y / clip.w;
        assert!(ndc_y > 0.0, "ndc_y = {ndc_y}");
    }

    #[test]
    fn center_ray_points_at_the_target() {
        let camera = Camera::default();
        let (_, direction) = camera.ray(Vec2::ZERO, 16.0 / 9.0);
        let expected = (camera.target - camera.eye()).normalize();
        assert!(direction.dot(expected) > 0.999);
    }
}
