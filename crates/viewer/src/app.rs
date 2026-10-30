use std::ops::Range;
use std::time::Instant;

use brepscad_eval::{
    Evaluation, EvaluationOptions, MeshCache, TreeNode, evaluate_with_cache_options,
};
use brepscad_kernel_api::Mesh;
use brepscad_kernel_truck::TruckKernel;
use brepscad_syntax::{TextEdit, TextRange, TextSize, apply_edit};
use eframe::egui;
use eframe::wgpu;
use egui::containers::scroll_area::ScrollSource;
use glam::Vec2;

use crate::camera::Camera;
use crate::mesh::{combine_evaluation, combine_parts};
use crate::picking::{PickPart, nearest_triangle};
use crate::renderer::MeshRenderer;

const SAMPLE: &str = include_str!("../../../assets/sample.scad");

#[derive(Debug, Clone)]
struct ParameterDraft {
    range: Range<usize>,
    source: String,
    value: f64,
    line: usize,
}

pub(crate) struct ViewerApp {
    source: String,
    evaluation: Evaluation,
    mesh_cache: MeshCache,
    legacy_centering: bool,
    mesh: Mesh,
    pick_mesh: Mesh,
    pick_parts: Vec<PickPart>,
    renderer: MeshRenderer,
    camera: Camera,
    selected_part: Option<(usize, usize)>,
    parameters: Vec<ParameterDraft>,
    error: Option<String>,
    pending_evaluate: bool,
    last_eval_ms: f64,
}

impl ViewerApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("the viewer requires the wgpu backend");

        let mut mesh_cache = MeshCache::new();
        let (evaluation, error) = evaluate_source(SAMPLE, &mut mesh_cache, false);
        let mesh = combine_evaluation(&evaluation);
        let (pick_mesh, pick_parts) = combine_parts(&evaluation);
        let mut camera = Camera::default();
        camera.fit(&mesh);
        let renderer = MeshRenderer::new(&render_state.device, &mesh);

        Self {
            source: SAMPLE.to_owned(),
            evaluation,
            mesh_cache,
            legacy_centering: false,
            mesh,
            pick_mesh,
            pick_parts,
            renderer,
            camera,
            selected_part: None,
            parameters: Vec::new(),
            error,
            pending_evaluate: false,
            last_eval_ms: 0.0,
        }
    }

    fn reload(&mut self, device: &wgpu::Device) {
        let started = Instant::now();
        let (evaluation, error) =
            evaluate_source(&self.source, &mut self.mesh_cache, self.legacy_centering);

        if error.is_none() {
            let mesh = combine_evaluation(&evaluation);
            let (pick_mesh, pick_parts) = combine_parts(&evaluation);
            self.camera.fit(&mesh);
            self.renderer.set_mesh(device, &mesh);
            self.mesh = mesh;
            self.pick_mesh = pick_mesh;
            self.pick_parts = pick_parts;
            self.evaluation = evaluation;
            self.selected_part = None;
            self.parameters.clear();
        }

        self.error = error;
        self.last_eval_ms = started.elapsed().as_secs_f64() * 1000.0;
    }

    fn select_part(&mut self, selection: Option<(usize, usize)>) {
        if self.selected_part == selection {
            return;
        }

        self.selected_part = selection;
        self.parameters.clear();

        let Some((shape_index, part_index)) = selection else {
            return;
        };
        let Some(part) = self
            .evaluation
            .shapes
            .get(shape_index)
            .and_then(|shape| shape.parts.get(part_index))
        else {
            self.selected_part = None;
            return;
        };

        let source = &self.source;
        self.parameters = part
            .parameters
            .iter()
            .map(|parameter| ParameterDraft {
                range: parameter.range.clone(),
                source: parameter.source.clone(),
                value: parameter.value,
                line: line_number(source, parameter.range.start),
            })
            .collect();
    }

    fn apply_parameter_edit(&mut self, range: Range<usize>, replacement: String) {
        let text_range = TextRange::new(
            TextSize::from(range.start as u32),
            TextSize::from(range.end as u32),
        );
        match apply_edit(&self.source, &TextEdit::new(text_range, replacement)) {
            Ok(source) => {
                self.source = source;
                self.selected_part = None;
                self.parameters.clear();
                self.pending_evaluate = true;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn pick_part(&self, ndc: Vec2, aspect: f32) -> Option<(usize, usize)> {
        let (origin, direction) = self.camera.ray(ndc, aspect);
        let triangle = nearest_triangle(&self.pick_mesh, origin, direction)?;
        self.pick_parts
            .iter()
            .find(|part| {
                triangle >= part.triangle_start
                    && triangle < part.triangle_start + part.triangle_count
            })
            .map(|part| (part.shape_index, part.part_index))
    }

    fn shapes_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Shapes");

        if self.evaluation.shapes.is_empty() {
            ui.label("Evaluate the source to build a tree.");
            return;
        }

        let mut clicked = None;
        for (shape_index, shape) in self.evaluation.shapes.iter().enumerate() {
            tree_ui(
                ui,
                &shape.tree,
                shape_index,
                self.selected_part,
                &self.source,
                &mut clicked,
            );
        }

        if clicked.is_some() {
            self.select_part(clicked);
        }
    }

    fn properties_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Properties");

        let Some((shape_index, part_index)) = self.selected_part else {
            ui.label("Click the model to select a part.");
            return;
        };

        let (module, source_range) = match self
            .evaluation
            .shapes
            .get(shape_index)
            .and_then(|shape| shape.parts.get(part_index))
        {
            Some(part) => (part.module.clone(), part.source_range.clone()),
            None => {
                self.selected_part = None;
                self.parameters.clear();
                return;
            }
        };

        ui.label(format!(
            "{module} · shape {shape_index} part {part_index} · bytes {}..{}",
            source_range.start, source_range.end
        ));

        if self.parameters.is_empty() {
            ui.label("No editable numbers in this part.");
            return;
        }

        let mut apply = None;
        for parameter in &mut self.parameters {
            ui.horizontal(|ui| {
                ui.label(format!("line {}", parameter.line));
                let response = ui.add(egui::DragValue::new(&mut parameter.value).speed(0.01));
                ui.monospace(&parameter.source);
                let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                if response.drag_stopped()
                    || response.lost_focus()
                    || (enter && response.has_focus())
                {
                    apply = Some((parameter.range.clone(), format_number(parameter.value)));
                }
            });
        }

        if let Some((range, replacement)) = apply {
            self.apply_parameter_edit(range, replacement);
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
                ui.heading("BrepSCAD");
                ui.separator();
                if ui.button("Evaluate").clicked() {
                    self.pending_evaluate = true;
                }
                let legacy = ui.checkbox(&mut self.legacy_centering, "Legacy OpenSCAD centering");
                if legacy.changed() {
                    self.pending_evaluate = true;
                }
                legacy.on_hover_text(
                    "Place cubes and squares at the origin, and cylinder/extrusion bases at z = 0",
                );
                ui.label("Ctrl+Enter");
                if self.last_eval_ms > 0.0 {
                    ui.separator();
                    ui.label(format!("{:.0} ms", self.last_eval_ms));
                }
            });
        });

        egui::SidePanel::left("source")
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .scroll_source(ScrollSource {
                        drag: false,
                        ..ScrollSource::default()
                    })
                    .show(ui, |ui| {
                        ui.heading("Source");
                        let source_response = ui.add(
                            egui::TextEdit::multiline(&mut self.source)
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .desired_rows(20),
                        );
                        if source_response.changed() {
                            self.selected_part = None;
                            self.parameters.clear();
                        }

                        if let Some(error) = &self.error {
                            ui.colored_label(egui::Color32::RED, error);
                        }

                        ui.separator();
                        ui.label(format!("{} shape(s)", self.evaluation.shapes.len()));
                        ui.label(format!("{} triangles", self.mesh.triangle_count()));
                        ui.label(format!("{} vertices", self.mesh.positions.len()));

                        self.shapes_ui(ui);
                        self.properties_ui(ui);
                    });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            let aspect = rect.width() / rect.height().max(1.0);

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

            if response.clicked() {
                if let Some(pointer) = response.interact_pointer_pos() {
                    let ndc = Vec2::new(
                        ((pointer.x - rect.left()) / rect.width()) * 2.0 - 1.0,
                        1.0 - ((pointer.y - rect.top()) / rect.height()) * 2.0,
                    );
                    self.select_part(self.pick_part(ndc, aspect));
                }
            }

            let pixels_per_point = ctx.pixels_per_point();
            let size_px = [
                (rect.width() * pixels_per_point).round().max(1.0) as u32,
                (rect.height() * pixels_per_point).round().max(1.0) as u32,
            ];

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
                "drag to orbit · scroll to zoom · click to select",
                egui::FontId::proportional(14.0),
                egui::Color32::from_gray(180),
            );
        });

        if self.pending_evaluate {
            ctx.request_repaint();
        }
    }
}

fn tree_ui(
    ui: &mut egui::Ui,
    node: &TreeNode,
    shape_index: usize,
    selected: Option<(usize, usize)>,
    source: &str,
    clicked: &mut Option<(usize, usize)>,
) {
    let label = format!(
        "{}  ·  line {}",
        node.module,
        line_number(source, node.source_range.start)
    );

    match node.part_index {
        Some(part_index) => {
            if ui
                .selectable_label(selected == Some((shape_index, part_index)), label)
                .clicked()
            {
                *clicked = Some((shape_index, part_index));
            }
        }
        None => {
            egui::CollapsingHeader::new(label)
                .id_salt((shape_index, node.source_range.start))
                .default_open(true)
                .show(ui, |ui| {
                    for child in &node.children {
                        tree_ui(ui, child, shape_index, selected, source, clicked);
                    }
                });
        }
    }
}

fn evaluate_source(
    source: &str,
    cache: &mut MeshCache,
    legacy_centering: bool,
) -> (Evaluation, Option<String>) {
    let mut kernel = TruckKernel::new();
    match evaluate_with_cache_options(
        source,
        &mut kernel,
        EvaluationOptions {
            tolerance: 0.01,
            legacy_centering,
        },
        cache,
    ) {
        Ok(evaluation) => (evaluation, None),
        Err(error) => (Evaluation { shapes: Vec::new() }, Some(error.to_string())),
    }
}

fn line_number(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn format_number(value: f64) -> String {
    let mut text = format!("{value:.6}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_sample_evaluates() {
        let (evaluation, error) = evaluate_source(SAMPLE, &mut MeshCache::default(), false);
        assert!(error.is_none(), "{error:?}");
        assert_eq!(evaluation.shapes.len(), 1);
        assert_eq!(evaluation.shapes[0].parts.len(), 2);
    }

    #[test]
    fn line_numbers_are_one_based() {
        assert_eq!(line_number("a\nb\nc", 2), 2);
        assert_eq!(line_number("a\nb\nc", 0), 1);
    }

    #[test]
    fn formats_parameter_values_compactly() {
        assert_eq!(format_number(2.0), "2");
        assert_eq!(format_number(2.5), "2.5");
        assert_eq!(format_number(2.1 + 1.0e-9), "2.1");
    }
}
