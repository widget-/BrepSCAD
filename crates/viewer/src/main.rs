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
    let path = std::env::args_os().nth(1).map(PathBuf::from);
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
        Box::new(move |cc| Ok(Box::new(ViewerApp::new(cc, path)))),
    )
}
