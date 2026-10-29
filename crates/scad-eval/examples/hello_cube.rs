use visual_openscad_kernel_truck::TruckKernel;
use visual_openscad_scad_eval::evaluate_to_meshes;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
size = [2, 2, 2];
difference() {
    cube(size);
    cylinder(0.5, 4);
}
"#;

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
