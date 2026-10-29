use visual_openscad_kernel_api::{BooleanOp, Kernel, Mesh, Primitive};
use visual_openscad_kernel_truck::TruckKernel;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut kernel = TruckKernel::new();
    let cube = kernel.primitive(Primitive::Cuboid {
        size: [2.0, 2.0, 2.0],
    })?;
    let cylinder = kernel.primitive(Primitive::Cylinder {
        radius: 0.5,
        height: 4.0,
    })?;
    let result = kernel.boolean(BooleanOp::Difference, cube, cylinder)?;
    let mesh = kernel.tessellate(result, 0.01)?;

    println!("triangles: {}", mesh.triangle_count());
    println!("vertices: {}", mesh.positions.len());
    println!("mesh volume: {:.3}", signed_volume(&mesh).abs());
    Ok(())
}

fn signed_volume(mesh: &Mesh) -> f64 {
    mesh.indices
        .chunks_exact(3)
        .map(|triangle| {
            let a = mesh.positions[triangle[0] as usize];
            let b = mesh.positions[triangle[1] as usize];
            let c = mesh.positions[triangle[2] as usize];
            dot(a, cross(b, c)) / 6.0
        })
        .sum()
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
