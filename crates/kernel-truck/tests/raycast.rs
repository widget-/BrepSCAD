//! The analytic ray-caster must agree with the tessellated preview mesh.

use brepscad_kernel_api::{BooleanOp, Kernel, Mesh, Primitive, ShapeId, Transform};
use brepscad_kernel_truck::TruckKernel;
use brepscad_raycast::{Ray, Scene};

fn cube(kernel: &mut TruckKernel, size: [f64; 3], at: [f64; 3]) -> ShapeId {
    let shape = kernel
        .primitive(Primitive::Cuboid { size })
        .expect("cube primitive");
    if at == [0.0; 3] {
        shape
    } else {
        kernel
            .transform(shape, Transform::from_translation(at))
            .expect("translate cube")
    }
}

/// Nearest ray/triangle hit, used as the reference the caster is checked
/// against.
fn mesh_distance(mesh: &Mesh, ray: &Ray) -> Option<f64> {
    let mut best = f64::INFINITY;
    for triangle in mesh.indices.chunks_exact(3) {
        let a = mesh.positions[triangle[0] as usize];
        let b = mesh.positions[triangle[1] as usize];
        let c = mesh.positions[triangle[2] as usize];
        if let Some(t) = ray_triangle(ray, a, b, c) {
            best = best.min(t);
        }
    }
    best.is_finite().then_some(best)
}

fn ray_triangle(ray: &Ray, a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Option<f64> {
    let edge1 = sub(b, a);
    let edge2 = sub(c, a);
    let pvec = cross(ray.direction, edge2);
    let det = dot(edge1, pvec);
    if det.abs() < 1.0e-12 {
        return None;
    }
    let inverse = 1.0 / det;
    let tvec = sub(ray.origin, a);
    let u = dot(tvec, pvec) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let qvec = cross(tvec, edge1);
    let v = dot(ray.direction, qvec) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = dot(edge2, qvec) * inverse;
    (distance > 1.0e-9).then_some(distance)
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn cast_rays() -> Vec<Ray> {
    let mut rays = Vec::new();
    let directions = [
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
        [0.3, 0.4, 0.87],
        [-0.5, 0.2, -0.84],
    ];
    let offsets = [
        [0.0, 0.0, 0.0],
        [0.2, 0.1, 0.0],
        [-0.3, 0.25, 0.1],
        [0.1, -0.4, 0.2],
    ];
    for direction in directions {
        for offset in offsets {
            let origin = [
                offset[0] + direction[0] * 6.0,
                offset[1] + direction[1] * 6.0,
                offset[2] + direction[2] * 6.0,
            ];
            rays.push(Ray::new(
                origin,
                [-direction[0], -direction[1], -direction[2]],
            ));
        }
    }
    rays
}

fn check_against_mesh(scene: &Scene, mesh: &Mesh, tolerance: f64) {
    let mut checked = 0;
    for ray in cast_rays() {
        let analytic = scene.cast(&ray).map(|hit| hit.distance);
        let reference = mesh_distance(mesh, &ray);
        match (analytic, reference) {
            (Some(analytic), Some(reference)) => {
                assert!(
                    (analytic - reference).abs() <= tolerance,
                    "ray {ray:?}: analytic {analytic} vs mesh {reference}"
                );
                checked += 1;
            }
            (None, None) => {}
            (analytic, reference) => {
                panic!("ray {ray:?}: analytic {analytic:?} vs mesh {reference:?}");
            }
        }
    }
    assert!(checked >= 8, "only {checked} rays hit anything");
}

#[test]
fn cube_scene_matches_the_tessellated_mesh() {
    let mut kernel = TruckKernel::new();
    let cube = cube(&mut kernel, [2.0, 2.0, 2.0], [0.0; 3]);
    let scene = kernel.raycast_scene(cube, 0.01).expect("scene");
    let mesh = kernel.tessellate(cube, 0.01).expect("tessellation");

    println!("cube faces: {}", scene.faces().len());
    check_against_mesh(&scene, &mesh, 1.0e-6);
}

#[test]
fn translated_cube_scene_matches_the_tessellated_mesh() {
    let mut kernel = TruckKernel::new();
    let cube = cube(&mut kernel, [2.0, 3.0, 4.0], [1.0, -2.0, 0.5]);
    let scene = kernel.raycast_scene(cube, 0.01).expect("scene");
    let mesh = kernel.tessellate(cube, 0.01).expect("tessellation");

    check_against_mesh(&scene, &mesh, 1.0e-6);
}

#[test]
fn cylinder_scene_matches_the_tessellated_mesh() {
    let mut kernel = TruckKernel::new();
    let cylinder = kernel
        .primitive(Primitive::Cylinder {
            radius: 1.0,
            height: 2.0,
        })
        .expect("cylinder");
    let scene = kernel.raycast_scene(cylinder, 0.01).expect("scene");
    let mesh = kernel.tessellate(cylinder, 0.01).expect("tessellation");

    // Four revolved side patches plus the two caps, all analytic: nothing about
    // a cylinder needs tessellation coverage any more.
    assert_eq!(scene.faces().len(), 6);
    let analytic = scene
        .faces()
        .iter()
        .filter(|face| face.surface.is_some())
        .count();
    assert_eq!(
        analytic,
        scene.faces().len(),
        "every cylinder face is analytic"
    );

    // The analytic side is exact, so the comparison tolerance only has to cover
    // the tessellated reference.
    check_against_mesh(&scene, &mesh, 0.02);
}

#[test]
fn boolean_scene_uses_the_trim_loops_of_the_result() {
    // Planar booleans keep every surface analytic while giving the trim loops a
    // hole, which is the case the even-odd test exists for. Truck tessellation
    // of boolean-derived B-spline faces is pathologically slow in debug builds,
    // so the reference here stays planar.
    let mut kernel = TruckKernel::new();
    let block = cube(&mut kernel, [2.0, 2.0, 2.0], [0.0; 3]);
    // A channel that runs clean through the top, the bottom, and both X faces.
    // The cutter must not stop on a face plane, or the boolean leaves a
    // coplanar fragment behind.
    let cutter = cube(&mut kernel, [10.0, 1.0, 4.0], [0.0, 0.0, 0.0]);
    let cut = kernel
        .boolean(BooleanOp::Difference, block, cutter)
        .expect("difference");
    let scene = kernel.raycast_scene(cut, 0.01).expect("scene");
    let mesh = kernel.tessellate(cut, 0.01).expect("tessellation");

    // Down the middle of the channel nothing is left to hit.
    let through_channel = Ray::new([0.0, 0.0, 5.0], [0.0, 0.0, -1.0]);
    assert!(
        scene.cast(&through_channel).is_none(),
        "the channel should be empty"
    );

    // Beside the channel the top and bottom faces are still there.
    let beside_channel = Ray::new([0.0, 0.6, 5.0], [0.0, 0.0, -1.0]);
    let hit = scene.cast(&beside_channel).expect("hit");
    assert!((hit.distance - 4.0).abs() < 1.0e-9, "{hit:?}");
    assert_eq!(hit.normal, [0.0, 0.0, 1.0]);

    check_against_mesh(&scene, &mesh, 1.0e-6);
}
