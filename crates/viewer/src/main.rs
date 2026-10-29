mod camera;
mod renderer;

use camera::Camera;
use eframe::egui;
use eframe::wgpu;
use glam::Vec2;
use renderer::MeshRenderer;
use visual_openscad_kernel_api::Mesh;
use visual_openscad_kernel_truck::TruckKernel;
use visual_openscad_scad_eval::evaluate_to_meshes;

const SAMPLE: &str = r#"size = [2, 2, 2];
difference() {
    cube(size);
    cylinder(0.5, 4);
}
"#;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
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

struct ViewerApp {
    source: String,
    mesh: Mesh,
    renderer: MeshRenderer,
    camera: Camera,
    error: Option<String>,
    pending_evaluate: bool,
}

impl ViewerApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("the viewer requires the wgpu backend");

        let (mesh, error) = evaluate(SAMPLE);
        let mut camera = Camera::default();
        camera.fit(&mesh);
        let renderer = MeshRenderer::new(&render_state.device, &mesh);

        Self {
            source: SAMPLE.to_owned(),
            mesh,
            renderer,
            camera,
            error,
            pending_evaluate: false,
        }
    }

    fn reload(&mut self, device: &wgpu::Device) {
        let mut kernel = TruckKernel::new();
        match evaluate_to_meshes(&self.source, &mut kernel, 0.01) {
            Ok(meshes) => {
                let mesh = combine_meshes(&meshes);
                self.camera.fit(&mesh);
                self.renderer.set_mesh(device, &mesh);
                self.mesh = mesh;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if ctx.input(|input| input.key_pressed(egui::Key::Enter) && input.modifiers.command) {
            self.pending_evaluate = true;
        }

        if self.pending_evaluate {
            if let Some(render_state) = frame.wgpu_render_state() {
                self.reload(&render_state.device);
            }
            self.pending_evaluate = false;
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Visual OpenSCAD");
                ui.separator();
                if ui.button("Evaluate").clicked() {
                    self.pending_evaluate = true;
                }
                ui.label("Ctrl+Enter");
            });
        });

        egui::SidePanel::left("source")
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.heading("Source");
                ui.add(
                    egui::TextEdit::multiline(&mut self.source)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(24),
                );

                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::RED, error);
                }

                ui.separator();
                ui.label(format!("{} triangles", self.mesh.triangle_count()));
                ui.label(format!("{} vertices", self.mesh.positions.len()));
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());

            if response.dragged() {
                let delta = response.drag_motion();
                self.camera.orbit(Vec2::new(delta.x, delta.y));
            }

            if response.hovered() {
                let scroll = ui.input(|input| input.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    self.camera.zoom(scroll);
                }
            }

            let pixels_per_point = ctx.pixels_per_point();
            let size_px = [
                (rect.width() * pixels_per_point).round().max(1.0) as u32,
                (rect.height() * pixels_per_point).round().max(1.0) as u32,
            ];
            let aspect = rect.width() / rect.height().max(1.0);

            if let Some(render_state) = frame.wgpu_render_state() {
                let mut egui_renderer = render_state.renderer.write();
                self.renderer
                    .ensure_targets(&render_state.device, &mut egui_renderer, size_px);
                let texture_id = self.renderer.render(
                    &render_state.device,
                    &render_state.queue,
                    &self.camera,
                    aspect,
                );
                drop(egui_renderer);

                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter()
                    .image(texture_id, rect, uv, egui::Color32::WHITE);
            }

            ui.painter().text(
                rect.left_top() + egui::vec2(12.0, 12.0),
                egui::Align2::LEFT_TOP,
                "drag to orbit · scroll to zoom",
                egui::FontId::proportional(14.0),
                egui::Color32::from_gray(180),
            );
        });
    }
}

fn evaluate(source: &str) -> (Mesh, Option<String>) {
    let mut kernel = TruckKernel::new();
    match evaluate_to_meshes(source, &mut kernel, 0.01) {
        Ok(meshes) => (combine_meshes(&meshes), None),
        Err(error) => (Mesh::default(), Some(error.to_string())),
    }
}

fn combine_meshes(meshes: &[Mesh]) -> Mesh {
    let mut combined = Mesh::default();
    for mesh in meshes {
        let offset = combined.positions.len() as u32;
        combined.positions.extend_from_slice(&mesh.positions);
        combined
            .indices
            .extend(mesh.indices.iter().map(|index| index + offset));
    }
    combined
}
