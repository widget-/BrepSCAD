use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use brepscad_eval::{
    Evaluation, EvaluationOptions, MeshCache, TreeNode, evaluate_source_with_path,
    evaluate_with_cache_options,
};
use brepscad_kernel_api::Mesh;
use brepscad_kernel_truck::TruckKernel;
use brepscad_syntax::{TextEdit, TextRange, TextSize, apply_edit};
use eframe::egui;
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

struct EvaluationResult {
    evaluation: Evaluation,
    error: Option<String>,
    cache: MeshCache,
    elapsed: Duration,
    source: String,
    source_path: Option<PathBuf>,
    legacy_centering: bool,
}

struct EvaluationJob {
    receiver: Receiver<EvaluationResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileDialogKind {
    Open,
    Save,
}

struct FileDialogResult {
    kind: FileDialogKind,
    path: Option<PathBuf>,
}

pub(crate) struct ViewerApp {
    source: String,
    current_path: Option<PathBuf>,
    dirty: bool,
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
    evaluation_job: Option<EvaluationJob>,
    file_dialog: Option<Receiver<FileDialogResult>>,
    last_eval_ms: f64,
}

impl ViewerApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("the viewer requires the wgpu backend");

        let mut mesh_cache = MeshCache::new();
        let (evaluation, error) = evaluate_source(SAMPLE, None, &mut mesh_cache, false);
        let adapter = render_state.adapter.get_info();
        eprintln!(
            "[gpu-probe] {:?} {:?} {:?} limits: complete={} buf={} tex={}",
            adapter.backend,
            adapter.name,
            adapter.device_type,
            render_state.device.limits().max_compute_workgroup_size_x,
            render_state
                .device
                .limits()
                .max_storage_buffers_per_shader_stage,
            render_state
                .device
                .limits()
                .max_storage_textures_per_shader_stage
        );
        let mesh = combine_evaluation(&evaluation);
        let (pick_mesh, pick_parts) = combine_parts(&evaluation);
        let mut camera = Camera::default();
        camera.fit(&mesh);
        let mut renderer = MeshRenderer::new(&render_state.device, &mesh);
        renderer.set_analytic(analytic_preview_enabled());

        Self {
            source: SAMPLE.to_owned(),
            current_path: None,
            dirty: false,
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
            evaluation_job: None,
            file_dialog: None,
            last_eval_ms: 0.0,
        }
    }

    fn start_evaluation(&mut self) {
        if self.evaluation_job.is_some() {
            self.pending_evaluate = true;
            return;
        }

        self.pending_evaluate = false;
        let source = self.source.clone();
        let source_path = self.current_path.clone();
        let legacy_centering = self.legacy_centering;
        let mut cache = std::mem::take(&mut self.mesh_cache);
        let (sender, receiver) = mpsc::channel();

        std::thread::spawn(move || {
            let started = Instant::now();
            let (evaluation, error) = evaluate_source(
                &source,
                source_path.as_deref(),
                &mut cache,
                legacy_centering,
            );
            let _ = sender.send(EvaluationResult {
                evaluation,
                error,
                cache,
                elapsed: started.elapsed(),
                source,
                source_path,
                legacy_centering,
            });
        });

        self.evaluation_job = Some(EvaluationJob { receiver });
    }

    fn poll_evaluation(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let Some(job) = self.evaluation_job.take() else {
            return;
        };

        match job.receiver.try_recv() {
            Ok(result) => {
                self.mesh_cache = result.cache;
                self.last_eval_ms = result.elapsed.as_secs_f64() * 1000.0;
                let current = result.source == self.source
                    && result.source_path == self.current_path
                    && result.legacy_centering == self.legacy_centering;

                if current {
                    if result.error.is_none() {
                        if let Some(render_state) = frame.wgpu_render_state() {
                            let mesh = combine_evaluation(&result.evaluation);
                            let (pick_mesh, pick_parts) = combine_parts(&result.evaluation);
                            self.camera.fit(&mesh);
                            self.renderer.set_mesh(&render_state.device, &mesh);
                            self.renderer
                                .set_scene(&render_state.device, result.evaluation.scene.as_ref());
                            self.mesh = mesh;
                            self.pick_mesh = pick_mesh;
                            self.pick_parts = pick_parts;
                            self.evaluation = result.evaluation;
                            self.selected_part = None;
                            self.parameters.clear();
                        }
                    }
                    self.error = result.error;
                } else {
                    // The source or placement mode changed while this job ran.
                    self.pending_evaluate = true;
                }
            }
            Err(TryRecvError::Empty) => {
                self.evaluation_job = Some(job);
                ctx.request_repaint_after(Duration::from_millis(50));
            }
            Err(TryRecvError::Disconnected) => {
                self.error = Some("evaluation worker stopped".to_owned());
            }
        }
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
                self.dirty = true;
                self.selected_part = None;
                self.parameters.clear();
                self.pending_evaluate = true;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn start_file_dialog(&mut self, kind: FileDialogKind) {
        if self.file_dialog.is_some() {
            return;
        }

        let (sender, receiver) = mpsc::channel();
        let current_path = self.current_path.clone();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new().add_filter("OpenSCAD", &["scad"]);
            if let Some(parent) = current_path.as_deref().and_then(Path::parent) {
                dialog = dialog.set_directory(parent);
            }
            let path = match kind {
                FileDialogKind::Open => dialog.pick_file(),
                FileDialogKind::Save => {
                    let file_name = current_path
                        .as_deref()
                        .and_then(Path::file_name)
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "model.scad".to_owned());
                    dialog.set_file_name(file_name).save_file()
                }
            };
            let _ = sender.send(FileDialogResult { kind, path });
        });
        self.file_dialog = Some(receiver);
    }

    fn open_dialog(&mut self) {
        self.start_file_dialog(FileDialogKind::Open);
    }

    fn save_as_dialog(&mut self) {
        self.start_file_dialog(FileDialogKind::Save);
    }

    fn poll_file_dialog(&mut self) {
        let Some(receiver) = self.file_dialog.take() else {
            return;
        };
        match receiver.try_recv() {
            Ok(result) => {
                let Some(path) = result.path else {
                    return;
                };
                match result.kind {
                    FileDialogKind::Open => self.load_path(path),
                    FileDialogKind::Save => self.save_to(path),
                }
            }
            Err(TryRecvError::Empty) => {
                self.file_dialog = Some(receiver);
            }
            Err(TryRecvError::Disconnected) => {
                self.error = Some("file dialog stopped unexpectedly".to_owned());
            }
        }
    }

    fn load_path(&mut self, path: PathBuf) {
        match read_source_file(&path) {
            Ok(source) => {
                self.source = source;
                self.current_path = Some(path);
                self.dirty = false;
                self.selected_part = None;
                self.parameters.clear();
                self.error = None;
                self.pending_evaluate = true;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn save_to(&mut self, path: PathBuf) {
        match write_source_file(&path, &self.source) {
            Ok(()) => {
                let context_changed = self.current_path.as_ref() != Some(&path);
                self.current_path = Some(path);
                self.dirty = false;
                self.error = None;
                if context_changed {
                    self.pending_evaluate = true;
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn save_file(&mut self) {
        match self.current_path.clone() {
            Some(path) => self.save_to(path),
            None => self.save_as_dialog(),
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
        if ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::S)) {
            if ctx.input(|input| input.modifiers.shift) {
                self.save_as_dialog();
            } else {
                self.save_file();
            }
        }

        let title = match &self.current_path {
            Some(path) => format!("BrepSCAD — {}", path.display()),
            None => "BrepSCAD — sample".to_owned(),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(if self.dirty {
            format!("{title} *")
        } else {
            title
        }));

        self.poll_file_dialog();
        self.poll_evaluation(ctx, frame);
        if self.pending_evaluate && self.evaluation_job.is_none() {
            self.start_evaluation();
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
                if self.evaluation_job.is_some() {
                    ui.separator();
                    ui.spinner();
                    ui.label("Evaluating…");
                }
                if self.last_eval_ms > 0.0 {
                    ui.separator();
                    ui.label(format!("{:.0} ms", self.last_eval_ms));
                }
            });
            ui.horizontal(|ui| {
                ui.label("File");
                if ui.button("Open…").clicked() {
                    self.open_dialog();
                }
                if ui.button("Save").clicked() {
                    self.save_file();
                }
                if ui.button("Save As…").clicked() {
                    self.save_as_dialog();
                }
                match &self.current_path {
                    Some(path) => {
                        ui.label(path.display().to_string());
                    }
                    None => {
                        ui.label("untitled");
                    }
                }
                if self.dirty {
                    ui.label("modified");
                }
                if self.file_dialog.is_some() {
                    ui.spinner();
                    ui.label("File dialog…");
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
                            self.dirty = true;
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

        if self.pending_evaluate || self.file_dialog.is_some() {
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

fn read_source_file(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("failed to open {}: {error}", path.display()))
}

fn write_source_file(path: &Path, source: &str) -> Result<(), String> {
    std::fs::write(path, source)
        .map_err(|error| format!("failed to save {}: {error}", path.display()))
}

fn evaluate_source(
    source: &str,
    source_path: Option<&Path>,
    cache: &mut MeshCache,
    legacy_centering: bool,
) -> (Evaluation, Option<String>) {
    let mut kernel = TruckKernel::new();
    let options = EvaluationOptions {
        tolerance: 0.01,
        legacy_centering,
        library_paths: brepscad_eval::environment_library_paths(),
        analytic_scene: analytic_preview_enabled(),
    };
    let result = match source_path {
        Some(path) => evaluate_source_with_path(source, path, &mut kernel, options, cache),
        None => evaluate_with_cache_options(source, &mut kernel, options, cache),
    };
    match result {
        Ok(evaluation) => (evaluation, None),
        Err(error) => (
            Evaluation {
                shapes: Vec::new(),
                messages: Vec::new(),
                scene: None,
            },
            Some(error.to_string()),
        ),
    }
}

/// The analytic preview is opt-in: it costs an extra pass over the B-rep and a
/// compute dispatch per frame.
fn analytic_preview_enabled() -> bool {
    std::env::var_os("BREPSCAD_ANALYTIC_PREVIEW").is_some()
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn temp_dir() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "brepscad-viewer-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn mesh_extent(mesh: &Mesh) -> f64 {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for position in &mesh.positions {
            min = min.min(position[0]);
            max = max.max(position[0]);
        }
        max - min
    }

    #[test]
    fn built_in_sample_evaluates() {
        let (evaluation, error) = evaluate_source(SAMPLE, None, &mut MeshCache::default(), false);
        assert!(error.is_none(), "{error:?}");
        assert_eq!(evaluation.shapes.len(), 1);
        assert_eq!(evaluation.shapes[0].parts.len(), 2);
    }

    #[test]
    fn unsaved_source_uses_the_open_file_context() {
        let dir = temp_dir();
        std::fs::write(dir.join("part.scad"), "cube(2);\n").expect("write part");
        let main = dir.join("main.scad");
        std::fs::write(&main, "cube(99);\n").expect("write main");

        let (evaluation, error) = evaluate_source(
            "include \"part.scad\";\n",
            Some(&main),
            &mut MeshCache::default(),
            false,
        );

        assert!(error.is_none(), "{error:?}");
        assert_eq!(evaluation.shapes.len(), 1);
        assert!((mesh_extent(&evaluation.shapes[0].mesh) - 2.0).abs() < 1.0e-6);
        std::fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[test]
    fn source_files_round_trip() {
        let dir = temp_dir();
        let path = dir.join("model.scad");

        write_source_file(&path, "cube(1);\n").expect("write source");
        assert_eq!(read_source_file(&path).expect("read source"), "cube(1);\n");
        assert!(
            read_source_file(&dir.join("missing.scad"))
                .expect_err("missing file")
                .contains("missing.scad")
        );
        std::fs::remove_dir_all(dir).expect("remove temp dir");
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
