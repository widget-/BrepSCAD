use visual_openscad_kernel_truck::TruckKernel;
use visual_openscad_scad_eval::evaluate_to_meshes;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = include_str!("../../../assets/sample.scad");

    let mut kernel = TruckKernel::new();
    let meshes = evaluate_to_meshes(source, &mut kernel, 0.01)?;

    for (index, mesh) in meshes.iter().enumerate() {
        println!(
            "mesh {index}: {} triangles, {} vertices",
            mesh.triangle_count(),
            mesh.positions.len()
        );
    }
    Ok(())
}
