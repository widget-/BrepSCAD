use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use brepscad_eval::{
    Evaluation, EvaluationOptions, MeshCache, TreeNode, evaluate_source_with_path_diagnostics,
    evaluate_with_cache_options_diagnostics,
};
use brepscad_kernel_api::Mesh;
use brepscad_kernel_truck::TruckKernel;
use brepscad_syntax::{TextEdit, TextRange, TextSize, apply_edit, parse};
use eframe::egui;
use egui::containers::scroll_area::ScrollSource;
use glam::{Vec2, Vec3};

use crate::camera::Camera;
use crate::editor::{self, CodeEditor, Diagnostic};
use crate::lsp;
use crate::mesh::{combine_evaluation, combine_parts};
use crate::picking::{PickPart, nearest_scene, nearest_triangle};
use crate::properties;
use crate::renderer::MeshRenderer;

const SAMPLE: &str = include_str!("../../../assets/sample.scad");

#[derive(Debug, Clone)]
struct ParameterDraft {
    name: String,
    target: Range<usize>,
    range: Range<usize>,
    source: String,
    value: Option<f64>,
}

struct EvaluationResult {
    evaluation: Evaluation,
    error: Option<String>,
    error_diagnostic: Option<Diagnostic>,
    cache: MeshCache,
    elapsed: Duration,
    source: String,
    source_path: Option<PathBuf>,
    legacy_centering: bool,
    analytic_preview: bool,
}

struct EvaluationJob {
    receiver: Receiver<EvaluationResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileDialogKind {
    Open,
    Save,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingFileAction {
    New,
    Open,
    Reload,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirtyChoice {
    Save,
    Discard,
    Cancel,
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
    /// Draw with the analytic ray tracer; the mesh path is the fallback.
    analytic_preview: bool,
    mesh: Mesh,
    pick_mesh: Mesh,
    pick_parts: Vec<PickPart>,
    renderer: MeshRenderer,
    camera: Camera,
    selected_part: Option<(usize, usize)>,
    selected_node: Option<(usize, String, Range<usize>)>,
    parameters: Vec<ParameterDraft>,
    error: Option<String>,
    editor: CodeEditor,
    diagnostics: Vec<Diagnostic>,
    evaluation_diagnostic: Option<Diagnostic>,
    lsp: Option<lsp::LspClient>,
    lsp_uri: Option<String>,
    lsp_diagnostics: Vec<Diagnostic>,
    lsp_completion: Option<(usize, Vec<editor::Candidate>)>,
    lsp_completion_pending: Option<(u64, usize)>,
    lsp_hover: Option<String>,
    lsp_hover_pending: Option<u64>,
    lsp_definition_pending: Option<u64>,
    lsp_log: Option<String>,
    lsp_waiting: bool,
    last_lsp_cursor: Option<usize>,
    pending_evaluate: bool,
    evaluation_job: Option<EvaluationJob>,
    file_dialog: Option<Receiver<FileDialogResult>>,
    pending_file_action: Option<PendingFileAction>,
    allow_close: bool,
    show_shortcuts: bool,
    show_about: bool,
    last_eval_ms: f64,
    /// Avoid re-sending the viewport title every frame: `send_viewport_cmd`
    /// requests a repaint, which would otherwise keep the event loop spinning.
    last_title: Option<String>,
}

impl ViewerApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("the viewer requires the wgpu backend");

        let mut mesh_cache = MeshCache::new();
        let (evaluation, error, error_diagnostic) =
            evaluate_source(SAMPLE, None, &mut mesh_cache, false, true);
        let mesh = combine_evaluation(&evaluation);
        let (pick_mesh, pick_parts) = combine_parts(&evaluation);
        let mut camera = Camera::default();
        camera.fit(&mesh);
        let mut renderer = MeshRenderer::new(&render_state.device, &mesh);
        renderer.set_scene(&render_state.device, evaluation.scene.as_ref());
        renderer.set_analytic(true);

        let source = SAMPLE.to_owned();
        let mut diagnostics = editor::parse_diagnostics(&source, parse(&source).errors());
        if let Some(diagnostic) = &error_diagnostic {
            diagnostics.push(diagnostic.clone());
        }

        let lsp_uri = "file:///untitled.scad".to_owned();
        let (lsp, lsp_log) = match lsp::LspClient::start() {
            Ok(mut client) => {
                client.open(lsp_uri.clone(), &source);
                (Some(client), None)
            }
            Err(error) => (None, Some(error)),
        };
        let lsp_waiting = lsp.is_some();

        let mut app = Self {
            source,
            current_path: None,
            dirty: false,
            evaluation,
            mesh_cache,
            legacy_centering: false,
            analytic_preview: true,
            mesh,
            pick_mesh,
            pick_parts,
            renderer,
            camera,
            selected_part: None,
            selected_node: None,
            parameters: Vec::new(),
            error,
            editor: CodeEditor::new(),
            diagnostics,
            evaluation_diagnostic: error_diagnostic,
            lsp,
            lsp_uri: Some(lsp_uri),
            lsp_diagnostics: Vec::new(),
            lsp_completion: None,
            lsp_completion_pending: None,
            lsp_hover: None,
            lsp_hover_pending: None,
            lsp_definition_pending: None,
            lsp_log,
            lsp_waiting,
            last_lsp_cursor: None,
            pending_evaluate: false,
            evaluation_job: None,
            file_dialog: None,
            pending_file_action: None,
            allow_close: false,
            show_shortcuts: false,
            show_about: false,
            last_eval_ms: 0.0,
            last_title: None,
        };
        if let Some(path) = path {
            app.load_path(&cc.egui_ctx, path);
        }
        app
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
        let analytic_preview = self.analytic_preview;
        let mut cache = std::mem::take(&mut self.mesh_cache);
        let (sender, receiver) = mpsc::channel();

        std::thread::spawn(move || {
            let started = Instant::now();
            let (evaluation, error, error_diagnostic) = evaluate_source(
                &source,
                source_path.as_deref(),
                &mut cache,
                legacy_centering,
                analytic_preview,
            );
            let _ = sender.send(EvaluationResult {
                evaluation,
                error,
                error_diagnostic,
                cache,
                elapsed: started.elapsed(),
                source,
                source_path,
                legacy_centering,
                analytic_preview,
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
                    && result.legacy_centering == self.legacy_centering
                    && result.analytic_preview == self.analytic_preview;

                if current {
                    if result.error.is_none()
                        && let Some(render_state) = frame.wgpu_render_state()
                    {
                        let mesh = combine_evaluation(&result.evaluation);
                        let (pick_mesh, pick_parts) = combine_parts(&result.evaluation);
                        self.camera.fit(&mesh);
                        self.renderer.set_mesh(&render_state.device, &mesh);
                        self.renderer
                            .set_scene(&render_state.device, result.evaluation.scene.as_ref());
                        self.renderer.set_analytic(self.analytic_preview);
                        self.mesh = mesh;
                        self.pick_mesh = pick_mesh;
                        self.pick_parts = pick_parts;
                        self.evaluation = result.evaluation;
                        self.selected_part = None;
                        self.selected_node = None;
                        self.parameters.clear();
                    }
                    self.error = result.error;
                    self.evaluation_diagnostic = result.error_diagnostic;
                    self.refresh_diagnostics();
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
        if self.selected_part == selection && self.selected_node.is_none() {
            return;
        }

        self.selected_part = selection;
        self.selected_node = None;
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

        let module = part.module.clone();
        let source_range = part.source_range.clone();
        self.load_parameter_rows(&module, &source_range);
    }

    fn select_node(&mut self, shape_index: usize, module: String, source_range: Range<usize>) {
        self.selected_part = None;
        self.selected_node = Some((shape_index, module.clone(), source_range.clone()));
        self.parameters.clear();
        self.load_parameter_rows(&module, &source_range);
    }

    fn load_parameter_rows(&mut self, module: &str, source_range: &Range<usize>) {
        self.parameters = properties::call_properties(&self.source, module, source_range)
            .rows
            .into_iter()
            .map(|row| ParameterDraft {
                name: row.name,
                target: row.target,
                range: row.range,
                source: row.source,
                value: row.value,
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
                self.source_changed();
                self.pending_evaluate = true;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn refresh_diagnostics(&mut self) {
        let mut diagnostics = editor::parse_diagnostics(&self.source, parse(&self.source).errors());
        if let Some(diagnostic) = &self.evaluation_diagnostic {
            diagnostics.push(diagnostic.clone());
        }
        diagnostics.extend(self.lsp_diagnostics.iter().cloned());
        diagnostics.sort_by_key(|diagnostic| {
            (
                diagnostic.range.start,
                diagnostic.range.end,
                diagnostic.message.clone(),
            )
        });
        diagnostics.dedup_by(|a, b| a.range == b.range && a.message == b.message);
        self.diagnostics = diagnostics;
    }

    fn source_changed(&mut self) {
        self.dirty = true;
        self.selected_part = None;
        self.selected_node = None;
        self.parameters.clear();
        self.evaluation_diagnostic = None;
        self.lsp_diagnostics.clear();
        self.lsp_completion = None;
        self.last_lsp_cursor = None;
        self.lsp_waiting = self.lsp.is_some();
        self.refresh_diagnostics();
        if let Some(lsp) = &mut self.lsp {
            lsp.change(&self.source);
        }
    }

    fn poll_lsp(&mut self, ctx: &egui::Context) {
        let events = self.lsp.as_mut().map(|lsp| lsp.poll()).unwrap_or_default();
        for event in events {
            match event {
                lsp::Event::Diagnostics { uri, diagnostics } => {
                    if Some(uri.as_str()) != self.lsp_uri.as_deref() {
                        continue;
                    }
                    self.lsp_waiting = false;
                    self.lsp_diagnostics = diagnostics
                        .into_iter()
                        .filter_map(|diagnostic| lsp_diagnostic(&self.source, diagnostic))
                        .collect();
                    self.refresh_diagnostics();
                }
                lsp::Event::Completion { id, items } => {
                    if self
                        .lsp_completion_pending
                        .as_ref()
                        .is_some_and(|(pending, _)| *pending == id)
                    {
                        let (_, cursor) = self.lsp_completion_pending.take().unwrap();
                        self.lsp_completion =
                            Some((cursor, items.into_iter().map(lsp_candidate).collect()));
                    }
                }
                lsp::Event::Hover { id, text } => {
                    if self.lsp_hover_pending == Some(id) {
                        self.lsp_hover_pending = None;
                        self.lsp_hover = text;
                    }
                }
                lsp::Event::Definition { id, location } => {
                    if self.lsp_definition_pending == Some(id) {
                        self.lsp_definition_pending = None;
                        if let Some(location) = location {
                            if Some(location.uri.as_str()) == self.lsp_uri.as_deref() {
                                let byte =
                                    lsp::position_to_offset(&self.source, location.range.start);
                                editor::set_cursor(ctx, &self.source, byte);
                            } else if let Some(path) = lsp::uri_to_path(&location.uri) {
                                self.load_path(ctx, path);
                            }
                        }
                    }
                }
                lsp::Event::Log(message) => {
                    self.lsp_log = Some(message);
                    self.lsp = None;
                    self.lsp_waiting = false;
                }
            }
        }
    }

    fn request_lsp_completion(&mut self, cursor: usize) {
        if self.last_lsp_cursor == Some(cursor) {
            return;
        }
        self.last_lsp_cursor = Some(cursor);
        let position = lsp::offset_to_position(&self.source, cursor);
        let Some(lsp) = &mut self.lsp else {
            return;
        };
        if let Some(id) = lsp.request_completion(position) {
            self.lsp_completion_pending = Some((id, cursor));
        }
    }

    fn request_lsp_hover(&mut self, ctx: &egui::Context) {
        let cursor = editor::cursor_byte(ctx, &self.source).unwrap_or(0);
        let position = lsp::offset_to_position(&self.source, cursor);
        let Some(lsp) = &mut self.lsp else {
            return;
        };
        if let Some(id) = lsp.request_hover(position) {
            self.lsp_hover_pending = Some(id);
        }
    }

    fn request_lsp_definition(&mut self, ctx: &egui::Context) {
        let cursor = editor::cursor_byte(ctx, &self.source).unwrap_or(0);
        let position = lsp::offset_to_position(&self.source, cursor);
        let Some(lsp) = &mut self.lsp else {
            return;
        };
        if let Some(id) = lsp.request_definition(position) {
            self.lsp_definition_pending = Some(id);
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

    fn poll_file_dialog(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.file_dialog.take() else {
            return;
        };
        match receiver.try_recv() {
            Ok(result) => {
                let Some(path) = result.path else {
                    return;
                };
                match result.kind {
                    FileDialogKind::Open => self.load_path(ctx, path),
                    FileDialogKind::Save => {
                        self.save_to(path);
                    }
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

    fn load_path(&mut self, ctx: &egui::Context, path: PathBuf) {
        match read_source_file(&path) {
            Ok(source) => {
                let uri = lsp::file_uri(&path);
                self.source = source;
                self.current_path = Some(path);
                self.dirty = false;
                self.selected_part = None;
                self.selected_node = None;
                self.parameters.clear();
                self.error = None;
                self.evaluation_diagnostic = None;
                self.lsp_diagnostics.clear();
                self.last_lsp_cursor = None;
                self.refresh_diagnostics();
                if let Some(uri) = uri
                    && let Some(lsp) = &mut self.lsp
                {
                    lsp.open(uri, &self.source);
                    self.lsp_waiting = true;
                }
                editor::reset_editor_state(ctx);
                self.pending_evaluate = true;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn save_to(&mut self, path: PathBuf) -> bool {
        match write_source_file(&path, &self.source) {
            Ok(()) => {
                let context_changed = self.current_path.as_ref() != Some(&path);
                self.current_path = Some(path);
                self.dirty = false;
                self.error = None;
                if context_changed {
                    self.pending_evaluate = true;
                }
                if let Some(lsp) = &mut self.lsp {
                    lsp.save();
                }
                true
            }
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }

    fn save_file(&mut self) -> bool {
        match self.current_path.clone() {
            Some(path) => self.save_to(path),
            None => {
                self.save_as_dialog();
                false
            }
        }
    }

    fn new_file(&mut self, ctx: &egui::Context) {
        self.source.clear();
        self.current_path = None;
        self.dirty = false;
        self.selected_part = None;
        self.selected_node = None;
        self.parameters.clear();
        self.error = None;
        self.evaluation_diagnostic = None;
        self.lsp_diagnostics.clear();
        self.last_lsp_cursor = None;
        self.refresh_diagnostics();
        if let Some(lsp) = &mut self.lsp {
            lsp.open("file:///untitled.scad".to_owned(), "");
            self.lsp_waiting = true;
        }
        editor::reset_editor_state(ctx);
        self.pending_evaluate = true;
    }

    fn request_file_action(&mut self, ctx: &egui::Context, action: PendingFileAction) {
        if self.dirty {
            self.pending_file_action = Some(action);
        } else {
            self.execute_file_action(ctx, action);
        }
    }

    fn execute_file_action(&mut self, ctx: &egui::Context, action: PendingFileAction) {
        match action {
            PendingFileAction::New => self.new_file(ctx),
            PendingFileAction::Open => self.open_dialog(),
            PendingFileAction::Reload => {
                if let Some(path) = self.current_path.clone() {
                    self.load_path(ctx, path);
                }
            }
            PendingFileAction::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn advance_pending_file_action(&mut self, ctx: &egui::Context) {
        if self.dirty {
            return;
        }
        if let Some(action) = self.pending_file_action.take() {
            self.execute_file_action(ctx, action);
        }
    }

    fn perform_edit(&mut self, ctx: &egui::Context, action: editor::EditAction) {
        if editor::edit_action(ctx, &mut self.source, action) {
            self.source_changed();
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)) {
            self.pending_evaluate = true;
        }

        if ctx.input_mut(|input| {
            input.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::S,
            )
        }) {
            self.save_as_dialog();
        } else if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save_file();
        }

        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::N)) {
            self.request_file_action(ctx, PendingFileAction::New);
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            self.request_file_action(ctx, PendingFileAction::Open);
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Q)) {
            self.request_file_action(ctx, PendingFileAction::Quit);
        }

        if !ctx.memory(|memory| memory.has_focus(editor::editor_id())) {
            return;
        }

        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F1)) {
            self.request_lsp_hover(ctx);
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F12)) {
            self.request_lsp_definition(ctx);
        }

        let redo = ctx.input_mut(|input| {
            input.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::Z,
            ) || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
        });
        let undo = !redo
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
        if redo {
            self.perform_edit(ctx, editor::EditAction::Redo);
        } else if undo {
            self.perform_edit(ctx, editor::EditAction::Undo);
        }

        let paste_plain = ctx.input_mut(|input| {
            input.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::V,
            )
        });
        if paste_plain {
            self.perform_edit(ctx, editor::EditAction::PastePlain);
        } else if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::V)) {
            self.perform_edit(ctx, editor::EditAction::Paste);
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::X)) {
            self.perform_edit(ctx, editor::EditAction::Cut);
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::C)) {
            self.perform_edit(ctx, editor::EditAction::Copy);
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::A)) {
            self.perform_edit(ctx, editor::EditAction::SelectAll);
        }
        let comment = ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::COMMAND, egui::Key::Slash)
                || input.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Slash,
                )
                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Questionmark)
        });
        if comment {
            self.perform_edit(ctx, editor::EditAction::ToggleLineComment);
        }
    }

    fn menu_bar_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if menu_button(ui, "New", &shortcut_label("N")).clicked() {
                    self.request_file_action(ctx, PendingFileAction::New);
                    ui.close();
                }
                if menu_button(ui, "Open…", &shortcut_label("O")).clicked() {
                    self.request_file_action(ctx, PendingFileAction::Open);
                    ui.close();
                }
                ui.separator();
                if menu_button(ui, "Save", &shortcut_label("S")).clicked() {
                    self.save_file();
                    ui.close();
                }
                if menu_button(ui, "Save As…", &shortcut_label("Shift+S")).clicked() {
                    self.save_as_dialog();
                    ui.close();
                }
                if menu_button_enabled(ui, self.current_path.is_some(), "Reload from Disk", "")
                    .clicked()
                {
                    self.request_file_action(ctx, PendingFileAction::Reload);
                    ui.close();
                }
                ui.separator();
                if menu_button(ui, "Quit", &shortcut_label("Q")).clicked() {
                    self.request_file_action(ctx, PendingFileAction::Quit);
                    ui.close();
                }
            });

            ui.menu_button("Edit", |ui| {
                let undo = menu_button_enabled(
                    ui,
                    editor::can_edit(ctx, &self.source, editor::EditAction::Undo),
                    "Undo",
                    &shortcut_label("Z"),
                );
                if undo.clicked() {
                    self.perform_edit(ctx, editor::EditAction::Undo);
                    ui.close();
                }
                let redo = menu_button_enabled(
                    ui,
                    editor::can_edit(ctx, &self.source, editor::EditAction::Redo),
                    "Redo",
                    &shortcut_label("Shift+Z"),
                );
                if redo.clicked() {
                    self.perform_edit(ctx, editor::EditAction::Redo);
                    ui.close();
                }
                ui.separator();
                let cut = menu_button_enabled(
                    ui,
                    editor::can_edit(ctx, &self.source, editor::EditAction::Cut),
                    "Cut",
                    &shortcut_label("X"),
                );
                if cut.clicked() {
                    self.perform_edit(ctx, editor::EditAction::Cut);
                    ui.close();
                }
                let copy = menu_button_enabled(
                    ui,
                    editor::can_edit(ctx, &self.source, editor::EditAction::Copy),
                    "Copy",
                    &shortcut_label("C"),
                );
                if copy.clicked() {
                    self.perform_edit(ctx, editor::EditAction::Copy);
                    ui.close();
                }
                if menu_button(ui, "Paste", &shortcut_label("V")).clicked() {
                    self.perform_edit(ctx, editor::EditAction::Paste);
                    ui.close();
                }
                if menu_button(ui, "Paste as Plain Text", &shortcut_label("Shift+V")).clicked() {
                    self.perform_edit(ctx, editor::EditAction::PastePlain);
                    ui.close();
                }
                if menu_button(ui, "Select All", &shortcut_label("A")).clicked() {
                    self.perform_edit(ctx, editor::EditAction::SelectAll);
                    ui.close();
                }
                ui.separator();
                if menu_button(ui, "Toggle Line Comment", &shortcut_label("/")).clicked() {
                    self.perform_edit(ctx, editor::EditAction::ToggleLineComment);
                    ui.close();
                }
                if menu_button(ui, "Indent", "Tab").clicked() {
                    self.perform_edit(ctx, editor::EditAction::Indent);
                    ui.close();
                }
                if menu_button(ui, "Outdent", "Shift+Tab").clicked() {
                    self.perform_edit(ctx, editor::EditAction::Outdent);
                    ui.close();
                }
            });

            ui.menu_button("View", |ui| {
                if ui.button("Fit View").clicked() {
                    self.camera.fit(&self.mesh);
                    ui.close();
                }
                if ui.button("Reset Camera").clicked() {
                    self.camera = Camera::default();
                    self.camera.fit(&self.mesh);
                    ui.close();
                }
                ui.separator();
                if ui
                    .checkbox(&mut self.legacy_centering, "Legacy OpenSCAD centering")
                    .changed()
                {
                    self.pending_evaluate = true;
                }
                if ui
                    .checkbox(
                        &mut self.analytic_preview,
                        "Analytic preview (experimental)",
                    )
                    .changed()
                {
                    self.renderer.set_analytic(self.analytic_preview);
                    self.pending_evaluate = true;
                }
            });

            ui.menu_button("Help", |ui| {
                if ui.button("Keyboard Shortcuts").clicked() {
                    self.show_shortcuts = true;
                    ui.close();
                }
                if ui.button("About BrepSCAD").clicked() {
                    self.show_about = true;
                    ui.close();
                }
            });
        });
    }

    fn toolbar_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("BrepSCAD");
            ui.separator();
            if ui.button("Evaluate").clicked() {
                self.pending_evaluate = true;
            }
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
            ui.separator();
            match &self.current_path {
                Some(path) => {
                    ui.label(path.display().to_string());
                }
                None => {
                    ui.weak("untitled");
                }
            }
            if self.dirty {
                ui.colored_label(egui::Color32::YELLOW, "modified");
            }
            if self.file_dialog.is_some() {
                ui.spinner();
                ui.weak("File dialog…");
            }
        });
    }

    fn dirty_prompt_ui(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending_file_action else {
            return;
        };
        if !self.dirty {
            self.pending_file_action = None;
            self.execute_file_action(ctx, action);
            return;
        }

        let mut choice = None;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Save changes before continuing?");
                ui.horizontal(|ui| {
                    let spacing = ui.spacing().item_spacing.x;
                    let button_width = ((ui.available_width() - 2.0 * spacing) / 3.0).max(80.0);
                    if ui
                        .add_sized(egui::vec2(button_width, 24.0), egui::Button::new("Save"))
                        .clicked()
                    {
                        choice = Some(DirtyChoice::Save);
                    }
                    if ui
                        .add_sized(egui::vec2(button_width, 24.0), egui::Button::new("Discard"))
                        .clicked()
                    {
                        choice = Some(DirtyChoice::Discard);
                    }
                    if ui
                        .add_sized(egui::vec2(button_width, 24.0), egui::Button::new("Cancel"))
                        .clicked()
                    {
                        choice = Some(DirtyChoice::Cancel);
                    }
                });
            });

        match choice {
            Some(DirtyChoice::Save) if self.save_file() => {
                self.pending_file_action = None;
                self.execute_file_action(ctx, action);
            }
            Some(DirtyChoice::Save) => {}
            Some(DirtyChoice::Discard) => {
                self.dirty = false;
                self.pending_file_action = None;
                self.execute_file_action(ctx, action);
            }
            Some(DirtyChoice::Cancel) => {
                self.pending_file_action = None;
            }
            None => {}
        }
    }

    fn windows_ui(&mut self, ctx: &egui::Context) {
        if self.show_shortcuts {
            let mut open = true;
            egui::Window::new("Keyboard Shortcuts")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(460.0)
                .show(ctx, |ui| {
                    egui::Grid::new("shortcuts_grid")
                        .num_columns(2)
                        .spacing([24.0, 6.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("Shortcut");
                            ui.strong("Action");
                            ui.end_row();
                            for (keys, action) in [
                                ("Ctrl+Enter", "Evaluate"),
                                ("Ctrl+S / Ctrl+Shift+S", "Save / Save As"),
                                ("Ctrl+Z / Ctrl+Shift+Z", "Undo / Redo"),
                                ("Ctrl+X / Ctrl+C / Ctrl+V", "Cut / Copy / Paste"),
                                ("Ctrl+Shift+V", "Paste as plain text"),
                                ("Ctrl+A", "Select all"),
                                ("Ctrl+/", "Toggle line comment"),
                                ("Tab / Shift+Tab", "Indent / Outdent"),
                                ("F1 / F12", "Language server hover / definition"),
                            ] {
                                ui.monospace(keys);
                                ui.label(action);
                                ui.end_row();
                            }
                        });
                });
            self.show_shortcuts = open;
        }

        if self.show_about {
            let mut open = true;
            egui::Window::new("About BrepSCAD")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.heading("BrepSCAD");
                    ui.label("A small OpenSCAD-compatible B-rep viewer and evaluator.");
                    ui.label("Editor: rowan CST highlighting, parse diagnostics, completion.");
                    ui.label("Language server: stdio JSON-RPC with diagnostics, completion, hover and definition.");
                });
            self.show_about = open;
        }

        if let Some(text) = self.lsp_hover.clone() {
            let mut open = true;
            egui::Window::new("Hover")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0))
                .show(ctx, |ui| {
                    ui.label(text);
                });
            if !open {
                self.lsp_hover = None;
            }
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !ctx.input(|input| input.viewport().close_requested()) {
            return;
        }
        if self.allow_close {
            self.allow_close = false;
        } else if self.dirty {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending_file_action = Some(PendingFileAction::Quit);
        } else {
            self.allow_close = true;
        }
    }

    fn pick_part(&self, ndc: Vec2, aspect: f32) -> Option<(usize, usize)> {
        let (origin, direction) = self.camera.ray(ndc, aspect);

        // Analytic hits are exact; mesh hits stay the fallback for parts that
        // have no scene (sketches, kernels without ray casting).
        let scene = if self.analytic_preview {
            self.nearest_part_scene(origin, direction)
        } else {
            None
        };
        let mesh = nearest_triangle(&self.pick_mesh, origin, direction).and_then(
            |(triangle, distance)| {
                self.pick_parts
                    .iter()
                    .find(|part| {
                        triangle >= part.triangle_start
                            && triangle < part.triangle_start + part.triangle_count
                    })
                    .map(|part| (distance, part.shape_index, part.part_index))
            },
        );

        match (scene, mesh) {
            (Some(scene), Some(mesh)) if mesh.0 < scene.0 => Some((mesh.1, mesh.2)),
            (Some(scene), _) => Some((scene.1, scene.2)),
            (None, Some(mesh)) => Some((mesh.1, mesh.2)),
            (None, None) => None,
        }
    }

    /// Nearest analytic hit across the parts, when they carry scenes.
    fn nearest_part_scene(&self, origin: Vec3, direction: Vec3) -> Option<(f32, usize, usize)> {
        let mut best: Option<(f32, usize, usize)> = None;
        for (shape_index, shape) in self.evaluation.shapes.iter().enumerate() {
            for (part_index, part) in shape.parts.iter().enumerate() {
                let Some(scene) = &part.scene else {
                    continue;
                };
                let Some(distance) = nearest_scene(scene, origin, direction) else {
                    continue;
                };
                if best.is_none_or(|(nearest, _, _)| distance < nearest) {
                    best = Some((distance, shape_index, part_index));
                }
            }
        }
        best
    }

    fn diagnostics_ui(&mut self, ui: &mut egui::Ui) {
        ui.set_width(ui.available_width());
        let mut any = false;
        if let Some(error) = &self.error
            && !self
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message == *error)
        {
            ui.colored_label(egui::Color32::RED, error);
            any = true;
        }
        for diagnostic in &self.diagnostics {
            let color = match diagnostic.severity {
                editor::Severity::Error => egui::Color32::RED,
                editor::Severity::Warning => egui::Color32::YELLOW,
                editor::Severity::Info => egui::Color32::GRAY,
            };
            let location = match &diagnostic.source_path {
                Some(path) => path.display().to_string(),
                None => format!("line {}", line_number(&self.source, diagnostic.range.start)),
            };
            ui.colored_label(color, format!("{location}: {}", diagnostic.message));
            any = true;
        }
        if let Some(log) = &self.lsp_log {
            ui.weak(log);
            any = true;
        }
        if !any {
            ui.weak("No diagnostics.");
        }
    }

    fn shapes_ui(&mut self, ui: &mut egui::Ui) {
        ui.set_width(ui.available_width());
        ui.label(format!("{} shape(s)", self.evaluation.shapes.len()));
        ui.label(format!("{} triangles", self.mesh.triangle_count()));
        ui.label(format!("{} vertices", self.mesh.positions.len()));

        if self.evaluation.shapes.is_empty() {
            ui.label("Evaluate the source to build a tree.");
            return;
        }

        let selected_node = self
            .selected_node
            .as_ref()
            .map(|(shape_index, _, source_range)| (*shape_index, source_range.start));
        let mut clicked = None;
        let mut clicked_node = None;
        for (shape_index, shape) in self.evaluation.shapes.iter().enumerate() {
            let mut view = TreeView {
                shape_index,
                selected_part: self.selected_part,
                selected_node,
                source: &self.source,
                clicked: &mut clicked,
                clicked_node: &mut clicked_node,
            };
            tree_ui(ui, &shape.tree, &mut view);
        }

        if let Some(part) = clicked {
            self.select_part(Some(part));
        } else if let Some((shape_index, module, source_range)) = clicked_node {
            self.select_node(shape_index, module, source_range);
        }
    }

    fn properties_ui(&mut self, ui: &mut egui::Ui) {
        ui.set_width(ui.available_width());

        let target = if let Some((_shape_index, module, source_range)) = &self.selected_node {
            Some((module.clone(), source_range.clone()))
        } else if let Some((shape_index, part_index)) = self.selected_part {
            self.evaluation
                .shapes
                .get(shape_index)
                .and_then(|shape| shape.parts.get(part_index))
                .map(|part| (part.module.clone(), part.source_range.clone()))
        } else {
            None
        };

        let Some((module, source_range)) = target else {
            self.selected_part = None;
            self.selected_node = None;
            self.parameters.clear();
            ui.label("Click a shape in the tree or the model to select a part.");
            return;
        };

        let title = format!(
            "{} (Line {})",
            module_title(&module),
            line_number(&self.source, source_range.start)
        );
        if ui.link(title).clicked() {
            jump_to(ui, &self.source, source_range.start);
        }

        if self.parameters.is_empty() {
            ui.label("No named parameters in this node.");
            return;
        }

        let source = self.source.clone();
        let mut apply = None;
        egui::Grid::new("properties_parameters")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                for parameter in &mut self.parameters {
                    if ui.link(&parameter.name).clicked() {
                        jump_to(ui, &source, parameter.target.start);
                    }
                    match &mut parameter.value {
                        Some(value) => {
                            let response = ui.add(egui::DragValue::new(value).speed(0.01));
                            let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                            if response.drag_stopped()
                                || response.lost_focus()
                                || (enter && response.has_focus())
                            {
                                apply = Some((parameter.range.clone(), format_number(*value)));
                            }
                        }
                        None => {
                            ui.monospace(&parameter.source);
                        }
                    }
                    ui.end_row();
                }
            });

        if let Some((range, replacement)) = apply {
            self.apply_parameter_edit(range, replacement);
        }
    }
}

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_close_request(&ctx);
        self.handle_shortcuts(&ctx);

        let title = match &self.current_path {
            Some(path) => format!("BrepSCAD — {}", path.display()),
            None => "BrepSCAD — sample".to_owned(),
        };
        let title = if self.dirty {
            format!("{title} *")
        } else {
            title
        };
        if self.last_title.as_deref() != Some(&title) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = Some(title);
        }

        self.poll_file_dialog(&ctx);
        self.advance_pending_file_action(&ctx);
        self.poll_evaluation(&ctx, frame);
        self.poll_lsp(&ctx);
        if self.pending_evaluate && self.evaluation_job.is_none() {
            self.start_evaluation();
        }

        egui::Panel::top("top").show(ui, |ui| {
            self.menu_bar_ui(&ctx, ui);
            ui.separator();
            self.toolbar_ui(ui);
        });

        egui::Panel::left("source")
            .resizable(true)
            .default_size(440.0)
            .show(ui, |ui| {
                egui::Panel::top("source_section")
                    .resizable(true)
                    .default_size(320.0)
                    .min_size(100.0)
                    .show(ui, |ui| {
                        ui.heading("Source");
                        egui::ScrollArea::vertical()
                            .id_salt("source_scroll")
                            .auto_shrink([false, false])
                            .scroll_source(ScrollSource {
                                drag: egui::containers::scroll_area::DragScroll::Never,
                                ..ScrollSource::default()
                            })
                            .show(ui, |ui| {
                                let cursor = editor::cursor_byte(&ctx, &self.source);
                                let external = self
                                    .lsp_completion
                                    .as_ref()
                                    .filter(|(saved, _)| Some(*saved) == cursor)
                                    .map(|(_, items)| items.clone());
                                let output = self.editor.show(
                                    ui,
                                    &mut self.source,
                                    &self.diagnostics,
                                    external.as_deref(),
                                );
                                if output.changed() {
                                    self.source_changed();
                                }
                                if let Some(cursor) = output.cursor_byte(&self.source) {
                                    self.request_lsp_completion(cursor);
                                }
                            });
                    });

                egui::Panel::top("diagnostics_section")
                    .resizable(true)
                    .default_size(100.0)
                    .min_size(38.0)
                    .show(ui, |ui| {
                        ui.heading("Diagnostics");
                        egui::ScrollArea::vertical()
                            .id_salt("diagnostics_scroll")
                            .auto_shrink([false, false])
                            .scroll_source(ScrollSource {
                                drag: egui::containers::scroll_area::DragScroll::Never,
                                ..ScrollSource::default()
                            })
                            .show(ui, |ui| self.diagnostics_ui(ui));
                    });

                egui::Panel::top("shapes_section")
                    .resizable(true)
                    .default_size(220.0)
                    .min_size(60.0)
                    .show(ui, |ui| {
                        ui.heading("Shapes");
                        egui::ScrollArea::vertical()
                            .id_salt("shapes_scroll")
                            .auto_shrink([false, false])
                            .scroll_source(ScrollSource {
                                drag: egui::containers::scroll_area::DragScroll::Never,
                                ..ScrollSource::default()
                            })
                            .show(ui, |ui| self.shapes_ui(ui));
                    });

                ui.heading("Properties");
                egui::ScrollArea::vertical()
                    .id_salt("properties_scroll")
                    .auto_shrink([false, false])
                    .scroll_source(ScrollSource {
                        drag: egui::containers::scroll_area::DragScroll::Never,
                        ..ScrollSource::default()
                    })
                    .show(ui, |ui| self.properties_ui(ui));
            });

        egui::CentralPanel::default().show(ui, |ui| {
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

            if response.clicked()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let ndc = Vec2::new(
                    ((pointer.x - rect.left()) / rect.width()) * 2.0 - 1.0,
                    1.0 - ((pointer.y - rect.top()) / rect.height()) * 2.0,
                );
                self.select_part(self.pick_part(ndc, aspect));
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

        self.dirty_prompt_ui(&ctx);
        self.windows_ui(&ctx);

        if self.pending_evaluate
            || self.file_dialog.is_some()
            || self.pending_file_action.is_some()
            || self.show_shortcuts
            || self.show_about
            || self.lsp_waiting
            || self.lsp_completion_pending.is_some()
            || self.lsp_hover_pending.is_some()
            || self.lsp_definition_pending.is_some()
            || self.lsp_hover.is_some()
        {
            ctx.request_repaint();
        }
    }
}

fn shortcut_label(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("Cmd+{key}")
    } else {
        format!("Ctrl+{key}")
    }
}

fn menu_button(ui: &mut egui::Ui, label: &str, shortcut: &str) -> egui::Response {
    let mut button = egui::Button::new(label);
    if !shortcut.is_empty() {
        button = button.shortcut_text(shortcut);
    }
    ui.add(button)
}

fn menu_button_enabled(
    ui: &mut egui::Ui,
    enabled: bool,
    label: &str,
    shortcut: &str,
) -> egui::Response {
    let mut button = egui::Button::new(label);
    if !shortcut.is_empty() {
        button = button.shortcut_text(shortcut);
    }
    ui.add_enabled(enabled, button)
}

struct TreeView<'a> {
    shape_index: usize,
    selected_part: Option<(usize, usize)>,
    selected_node: Option<(usize, usize)>,
    source: &'a str,
    clicked: &'a mut Option<(usize, usize)>,
    clicked_node: &'a mut Option<(usize, String, Range<usize>)>,
}

fn tree_ui(ui: &mut egui::Ui, node: &TreeNode, view: &mut TreeView<'_>) {
    let label = format!(
        "{}  ·  line {}",
        node.module,
        line_number(view.source, node.source_range.start)
    );

    match node.part_index {
        Some(part_index) => {
            if ui
                .selectable_label(
                    view.selected_part == Some((view.shape_index, part_index)),
                    label,
                )
                .clicked()
            {
                *view.clicked = Some((view.shape_index, part_index));
            }
        }
        None => {
            let selected_here =
                view.selected_node == Some((view.shape_index, node.source_range.start));
            let label = egui::RichText::new(label);
            let label = if selected_here { label.strong() } else { label };
            let response = egui::CollapsingHeader::new(label)
                .id_salt((view.shape_index, node.source_range.start))
                .default_open(true)
                .show(ui, |ui| {
                    for child in &node.children {
                        tree_ui(ui, child, view);
                    }
                });
            if response.header_response.clicked() {
                *view.clicked_node = Some((
                    view.shape_index,
                    node.module.clone(),
                    node.source_range.clone(),
                ));
            }
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
    analytic: bool,
) -> (Evaluation, Option<String>, Option<Diagnostic>) {
    let mut kernel = TruckKernel::new();
    let options = EvaluationOptions {
        tolerance: 0.01,
        legacy_centering,
        library_paths: brepscad_eval::environment_library_paths(),
        analytic_scene: analytic,
    };
    let current_path = source_path;
    let result = match source_path {
        Some(path) => {
            evaluate_source_with_path_diagnostics(source, path, &mut kernel, options, cache)
        }
        None => evaluate_with_cache_options_diagnostics(source, &mut kernel, options, cache),
    };
    match result {
        Ok(evaluation) => (evaluation, None, None),
        Err(failure) => {
            let message = failure.error.to_string();
            let diagnostic = editor::Diagnostic {
                severity: editor::Severity::Error,
                message: message.clone(),
                range: failure.range.unwrap_or(0..0),
                source_path: failure
                    .source_path
                    .filter(|path| Some(path.as_path()) != current_path),
            };
            (
                Evaluation {
                    shapes: Vec::new(),
                    messages: Vec::new(),
                    scene: None,
                },
                Some(message),
                Some(diagnostic),
            )
        }
    }
}

fn lsp_diagnostic(source: &str, diagnostic: lsp::LspDiagnostic) -> Option<editor::Diagnostic> {
    let mut start = lsp::position_to_offset(source, diagnostic.range.start).min(source.len());
    let mut end = lsp::position_to_offset(source, diagnostic.range.end).min(source.len());
    while start > 0 && !source.is_char_boundary(start) {
        start -= 1;
    }
    while end > 0 && !source.is_char_boundary(end) {
        end -= 1;
    }
    if start > end {
        return None;
    }
    Some(editor::Diagnostic {
        severity: match diagnostic.severity {
            1 => editor::Severity::Error,
            2 => editor::Severity::Warning,
            _ => editor::Severity::Info,
        },
        message: diagnostic.message,
        range: start..end,
        source_path: None,
    })
}

fn lsp_candidate(item: lsp::CompletionItem) -> editor::Candidate {
    editor::Candidate {
        insert: item.insert_text,
        label: item.label,
        detail: item.detail,
        kind: match item.kind {
            9 => editor::CandidateKind::Declaration,
            3 => editor::CandidateKind::Builtin,
            _ => editor::CandidateKind::Parameter,
        },
    }
}

fn line_number(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn module_title(module: &str) -> String {
    let mut characters = module.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

fn jump_to(ui: &egui::Ui, source: &str, offset: usize) {
    editor::set_cursor(ui.ctx(), source, offset);
    ui.ctx()
        .memory_mut(|memory| memory.request_focus(editor::editor_id()));
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
        let (evaluation, error, _) =
            evaluate_source(SAMPLE, None, &mut MeshCache::default(), false, true);
        assert!(error.is_none(), "{error:?}");
        assert_eq!(evaluation.shapes.len(), 1);
        assert_eq!(evaluation.shapes[0].parts.len(), 2);
        let scene = evaluation.scene.as_ref().expect("analytic scene");
        assert!(!scene.faces().is_empty(), "the sample should produce faces");
    }

    #[test]
    fn unsaved_source_uses_the_open_file_context() {
        let dir = temp_dir();
        std::fs::write(dir.join("part.scad"), "cube(2);\n").expect("write part");
        let main = dir.join("main.scad");
        std::fs::write(&main, "cube(99);\n").expect("write main");

        let (evaluation, error, _) = evaluate_source(
            "include \"part.scad\";\n",
            Some(&main),
            &mut MeshCache::default(),
            false,
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
