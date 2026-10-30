mod app;
mod camera;
mod mesh;
mod picking;
mod renderer;

use app::ViewerApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("Visual OpenSCAD"),
        ..Default::default()
    };

    eframe::run_native(
        "Visual OpenSCAD",
        options,
        Box::new(|cc| Ok(Box::new(ViewerApp::new(cc)))),
    )
}
