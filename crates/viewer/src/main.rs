mod analytic;
mod app;
mod camera;
mod editor;
mod lsp;
mod mesh;
mod picking;
mod properties;
mod renderer;
mod scene_gpu;

use app::ViewerApp;
use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let mut path = None;
    let mut legacy_centering = false;
    for argument in std::env::args_os().skip(1) {
        match argument.to_str() {
            Some("--legacy-centering") => legacy_centering = true,
            _ => path = Some(PathBuf::from(argument)),
        }
    }
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("BrepSCAD"),
        ..Default::default()
    };

    eframe::run_native(
        "BrepSCAD",
        options,
        Box::new(move |cc| Ok(Box::new(ViewerApp::new(cc, path, legacy_centering)))),
    )
}
