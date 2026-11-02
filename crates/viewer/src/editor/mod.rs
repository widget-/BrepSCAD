mod completion;
mod diagnostics;
mod gutter;
mod highlight;
#[allow(dead_code)]
mod keymap;
mod theme;

pub use diagnostics::{Diagnostic, Severity, parse_diagnostics};

use eframe::egui::{self, EventFilter, TextBuffer, TextEdit, text_edit::TextEditOutput};

use highlight::Highlighter;
use theme::Theme;

#[derive(Default)]
pub struct CodeEditor {
    highlighter: Highlighter,
    theme: Theme,
}

pub struct EditorOutput {
    pub text_edit: TextEditOutput,
    pub accepted: Option<egui::Suggestion>,
}

impl EditorOutput {
    pub fn changed(&self) -> bool {
        self.text_edit.response.response.changed() || self.accepted.is_some()
    }
}

impl CodeEditor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        source: &mut String,
        diagnostics: &[Diagnostic],
    ) -> EditorOutput {
        let id = ui.make_persistent_id("source_editor");
        let job = self
            .highlighter
            .job(source, diagnostics, &self.theme)
            .clone();
        let mut layouter = move |ui: &egui::Ui, _buffer: &dyn TextBuffer, wrap_width: f32| {
            let mut job = job.clone();
            job.wrap.max_width = wrap_width;
            ui.fonts_mut(|fonts| fonts.layout_job(job))
        };

        let gutter_width = gutter::width(source);
        let mut output = None;
        ui.horizontal_top(|ui| {
            let (_, gutter_rect) = ui.allocate_space(egui::vec2(gutter_width, 0.0));
            let text_edit = TextEdit::multiline(source)
                .id(id)
                .code_editor()
                .desired_width(f32::INFINITY)
                .desired_rows(20)
                .layouter(&mut layouter)
                .event_filter(EventFilter {
                    tab: true,
                    ..Default::default()
                });
            let popup = egui::CompletionPopup::new(id)
                .word_boundary(|character| !completion::is_identifier_continue(character))
                .show(ui, text_edit, |query| {
                    let cursor = diagnostics::byte_index(query.text, query.word_range.end.0);
                    completion::suggestions(query.text, cursor)
                        .into_iter()
                        .map(|candidate| {
                            egui::Suggestion::new(candidate.insert).description(candidate.detail)
                        })
                        .collect()
                });
            gutter::paint(
                ui,
                &popup.text_edit,
                source,
                diagnostics,
                &self.theme,
                gutter_rect,
            );
            output = Some(popup);
        });

        let output = output.expect("the editor is shown exactly once");
        EditorOutput {
            text_edit: output.text_edit,
            accepted: output.accepted,
        }
    }

    /// Returns the highlight job for callers that need to inspect it. Kept
    /// crate-visible so tests can verify the highlighter without a window.
    #[cfg(test)]
    fn layout_job(
        &mut self,
        source: &str,
        diagnostics: &[Diagnostic],
    ) -> eframe::egui::text::LayoutJob {
        self.highlighter
            .job(source, diagnostics, &self.theme)
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_job_preserves_source() {
        let mut editor = CodeEditor::new();
        let source = "cube(4);\n";
        let job = editor.layout_job(source, &[]);
        assert_eq!(job.text, source);
    }
}
