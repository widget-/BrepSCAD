use anyhow::{Context, Result};
use visual_openscad_kernel_truck::TruckKernel;
use visual_openscad_scad_eval::evaluate_to_meshes;

const DEFAULT_SOURCE: &str = r#"
size = [2, 2, 2];
difference() {
    cube(size);
    cylinder(0.5, 4);
}
"#;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let first = args.next();

    if matches!(first.as_deref(), Some("--help" | "-h")) {
        println!("usage: visual-openscad [file.scad]");
        return Ok(());
    }
    if args.next().is_some() {
        anyhow::bail!("expected at most one file path");
    }

    let source = match first {
        Some(path) => {
            std::fs::read_to_string(&path).with_context(|| format!("failed to read {path}"))?
        }
        None => DEFAULT_SOURCE.to_owned(),
    };

    let mut kernel = TruckKernel::new();
    let meshes = evaluate_to_meshes(&source, &mut kernel, 0.01)?;

    println!("meshes: {}", meshes.len());
    for (index, mesh) in meshes.iter().enumerate() {
        println!(
            "mesh {index}: {} triangles, {} vertices",
            mesh.triangle_count(),
            mesh.positions.len()
        );
    }
    Ok(())
}
