//! The analytic ray-caster must agree with the tessellated preview mesh.

use brepscad_kernel_api::{BooleanOp, Kernel, Mesh, Primitive, ShapeId, Sketch, Transform};
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

#[test]
fn spline_walls_are_extracted_and_match_the_mesh() {
    let mut kernel = TruckKernel::new();
    let sketch = kernel
        .sketch(Sketch::Circle { radius: 1.0 })
        .expect("circle");
    let solid = kernel.extrude(sketch, 2.0).expect("extrude");
    let scene = kernel.raycast_scene(solid, 0.01).expect("scene");
    let mesh = kernel.tessellate(solid, 0.01).expect("tessellation");

    let spline_faces = scene
        .faces()
        .iter()
        .filter(|face| matches!(face.surface, Some(brepscad_raycast::Surface::Spline(_))))
        .count();
    assert!(
        spline_faces > 0,
        "an extruded circle has B-spline walls, got {} faces",
        scene.faces().len()
    );

    // Rays across the lateral surface must agree with the tessellated reference,
    // which also proves the trim loops land on the patch's own parameters.
    for x in [-0.9_f64, -0.4, 0.0, 0.4, 0.9] {
        let ray = Ray::new([2.0, x, 0.3], [-1.0, 0.0, 0.0]);
        let scene_hit = scene.cast(&ray).map(|hit| hit.distance);
        let mesh_hit = mesh_distance(&mesh, &ray);
        match (scene_hit, mesh_hit) {
            (Some(analytic), Some(reference)) => assert!(
                (analytic - reference).abs() < 0.02,
                "x = {x}: scene {analytic}, mesh {reference}"
            ),
            (None, Some(reference)) => panic!("x = {x}: scene missed, mesh hit at {reference}"),
            (Some(analytic), None) => panic!("x = {x}: scene hit at {analytic}, mesh missed"),
            (None, None) => {}
        }
    }
}

#[test]
fn sphere_scene_is_one_analytic_ball() {
    let mut kernel = TruckKernel::new();
    let sphere = kernel
        .primitive(Primitive::Sphere { radius: 1.0 })
        .expect("sphere");
    let scene = kernel.raycast_scene(sphere, 0.05).expect("scene");
    assert_eq!(
        scene.faces().len(),
        1,
        "a ball is one exact sphere, not a patchwork"
    );

    let hit = scene
        .cast(&Ray::new([0.0, 0.0, 5.0], [0.0, 0.0, -1.0]))
        .expect("hit");
    assert!(
        (hit.distance - 4.0).abs() < 1.0e-6,
        "distance = {}",
        hit.distance
    );
    assert!(
        (hit.normal[2] - 1.0).abs() < 1.0e-6,
        "normal = {:?}",
        hit.normal
    );

    // A moved ball traces its own centre rather than the origin.
    let moved = kernel
        .transform(sphere, Transform::from_translation([2.0, 0.0, 0.0]))
        .expect("translate");
    let scene = kernel.raycast_scene(moved, 0.05).expect("scene");
    let hit = scene
        .cast(&Ray::new([2.0, 0.0, 5.0], [0.0, 0.0, -1.0]))
        .expect("hit");
    assert!(
        (hit.distance - 4.0).abs() < 1.0e-6,
        "distance = {}",
        hit.distance
    );
}

#[test]
fn sphere_boolean_patches_are_analytic_and_exact() {
    let mut kernel = TruckKernel::new();
    let cube = cube(&mut kernel, [2.0; 3], [0.0; 3]);
    let sphere = kernel
        .primitive(Primitive::Sphere { radius: 1.2 })
        .expect("sphere");

    for op in [BooleanOp::Difference, BooleanOp::Intersection] {
        let result = kernel.boolean(op, cube, sphere).expect("boolean");
        let scene = kernel.raycast_scene(result, 0.01).expect("scene");

        let spline_faces = scene
            .faces()
            .iter()
            .filter(|face| matches!(face.surface, Some(brepscad_raycast::Surface::Spline(_))))
            .count();
        assert_eq!(spline_faces, 4, "{op:?}: one spline per sphere quadrant");
        assert!(
            scene.faces().iter().all(|face| face.surface.is_some()),
            "{op:?}: every face has an analytic surface"
        );
    }

    // The intersection lens has no cube top above the sphere at x = 0.8: the
    // first hit is the exact sphere patch, not a facet.
    let lens = kernel
        .boolean(BooleanOp::Intersection, cube, sphere)
        .expect("intersection");
    let scene = kernel.raycast_scene(lens, 0.01).expect("scene");
    let ray = Ray::new([0.8, 0.0, 3.0], [0.0, 0.0, -1.0]);
    let hit = scene.cast(&ray).expect("lens hit");
    let expected = 3.0 - (1.2_f64 * 1.2 - 0.8 * 0.8).sqrt();
    assert!(
        (hit.distance - expected).abs() < 1.0e-6,
        "distance {} expected {expected}",
        hit.distance
    );
    let radius = (hit.position[0] * hit.position[0]
        + hit.position[1] * hit.position[1]
        + hit.position[2] * hit.position[2])
        .sqrt();
    assert!((radius - 1.2).abs() < 1.0e-6, "radius {radius}");
    assert!(
        dot(hit.normal, hit.position) > 0.0,
        "lens normal points outward: {:?}",
        hit.normal
    );

    // In the carved cube the same point is a cavity wall, so its normal points
    // toward the sphere centre.
    let carved = kernel
        .boolean(BooleanOp::Difference, cube, sphere)
        .expect("difference");
    let scene = kernel.raycast_scene(carved, 0.01).expect("scene");
    let ray = Ray::new([0.99, 0.8, 0.0], [-1.0, 0.0, 0.0]);
    let hit = scene.cast(&ray).expect("cavity hit");
    assert!(
        (hit.position[0] - (1.44_f64 - 0.64).sqrt()).abs() < 1.0e-6,
        "cavity hit at {:?}",
        hit.position
    );
    assert!(
        dot(hit.normal, hit.position) < 0.0,
        "cavity normal points inward: {:?}",
        hit.normal
    );

    // Every quadrant of the cavity wall must face the same way; a flipped patch
    // shows up as a bright seam in the preview.
    for step in 0..16 {
        let angle = std::f64::consts::TAU * step as f64 / 16.0;
        let (sin, cos) = angle.sin_cos();
        let origin = [0.8 * cos, 0.8 * sin, 0.99];
        let ray = Ray::new(origin, [0.0, 0.0, -1.0]);
        let hit = scene.cast(&ray).expect("cavity hit around the wall");
        let radius = (hit.position[0] * hit.position[0]
            + hit.position[1] * hit.position[1]
            + hit.position[2] * hit.position[2])
            .sqrt();
        assert!(
            (radius - 1.2).abs() < 1.0e-6,
            "step {step}: radius {radius}"
        );
        assert!(
            dot(hit.normal, hit.position) < 0.0,
            "step {step}: cavity normal points inward: {:?}",
            hit.normal
        );
    }
}

#[test]
fn extruded_circle_wall_covers_a_dense_ray_grid() {
    let mut kernel = TruckKernel::new();
    let sketch = kernel
        .sketch(Sketch::Circle { radius: 1.0 })
        .expect("circle");
    let solid = kernel.extrude(sketch, 2.0).expect("extrude");
    let scene = kernel.raycast_scene(solid, 0.01).expect("scene");
    let mesh = kernel.tessellate(solid, 0.01).expect("tessellation");

    let mut missed = Vec::new();
    for iy in 0..30 {
        for iz in 0..30 {
            let y = -0.95 + 1.9 * iy as f64 / 29.0;
            let z = -0.95 + 1.9 * iz as f64 / 29.0;
            if y * y + z * z > 0.9 {
                continue;
            }
            let ray = Ray::new([2.0, y, z], [-1.0, 0.0, 0.0]);
            let analytic = scene.cast(&ray);
            let reference = mesh_distance(&mesh, &ray);
            if analytic.is_none() && reference.is_some() {
                missed.push((y, z, reference));
            }
        }
    }
    assert!(
        missed.is_empty(),
        "analytic missed {} wall rays, first {:?}",
        missed.len(),
        &missed[..missed.len().min(5)]
    );
}

#[test]
fn revolved_circle_scene_matches_the_mesh() {
    let mut kernel = TruckKernel::new();
    let sketch = kernel
        .sketch(Sketch::Circle { radius: 0.5 })
        .expect("circle");
    let shifted = kernel
        .transform(sketch, Transform::from_translation([2.0, 0.0, 0.0]))
        .expect("translate profile");
    let torus = kernel
        .revolve(shifted, 0.0, std::f64::consts::TAU)
        .expect("revolve");
    let scene = kernel.raycast_scene(torus, 0.01).expect("scene");
    let mesh = kernel.tessellate(torus, 0.01).expect("tessellation");

    let spline_faces = scene
        .faces()
        .iter()
        .filter(|face| matches!(face.surface, Some(brepscad_raycast::Surface::Spline(_))))
        .count();
    assert!(spline_faces > 0, "the torus should be spline faces");

    check_against_mesh(&scene, &mesh, 0.02);
}
