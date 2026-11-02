mod actions;
mod completion;
mod diagnostics;
mod gutter;
mod highlight;
mod keymap;
mod theme;

pub use actions::{
    EditAction, action as edit_action, can as can_edit, cursor_byte, editor_id,
    reset as reset_editor_state, set_cursor,
};
pub use completion::{Candidate, CandidateKind};
pub use diagnostics::{Diagnostic, Severity, parse_diagnostics};

use std::ops::Range;

use eframe::egui::{
    self, Event, EventFilter, InputState, Key, Modifiers, TextBuffer, TextEdit,
    text::{CCursor, CCursorRange},
    text_edit::{TextEditOutput, TextEditState},
};

use highlight::Highlighter;
use theme::Theme;

#[derive(Default)]
struct CompletionState {
    open: bool,
    selected: usize,
    last_word: String,
    dismissed_word: Option<String>,
}

#[derive(Default)]
pub struct CodeEditor {
    highlighter: Highlighter,
    theme: Theme,
    completion: CompletionState,
}

pub struct EditorOutput {
    pub text_edit: TextEditOutput,
    applied: bool,
}

impl EditorOutput {
    pub fn changed(&self) -> bool {
        self.text_edit.response.response.changed() || self.applied
    }

    pub fn cursor_byte(&self, source: &str) -> Option<usize> {
        output_cursor_byte(&self.text_edit, source)
    }
}

#[derive(Default)]
struct PopupKeys {
    down: usize,
    up: usize,
    accept: bool,
    dismiss: bool,
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
        external: Option<&[Candidate]>,
    ) -> EditorOutput {
        let id = editor_id();
        if self.completion.open {
            ui.ctx().memory_mut(|memory| memory.request_focus(id));
        }
        let focused = ui.memory(|memory| memory.has_focus(id));
        let shift = ui.input(|input| input.modifiers.shift);
        let tab_pressed = ui.input(|input| input.key_pressed(Key::Tab));
        let command_down = ui.input(|input| input.modifiers.command);

        if !focused {
            self.completion.open = false;
        }
        let selection_nonempty = TextEditState::load(ui.ctx(), id)
            .and_then(|state| state.cursor.char_range())
            .is_some_and(|range| !range.is_empty());
        if selection_nonempty {
            self.completion.open = false;
        }
        if focused && ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Space)) {
            self.completion.open = true;
            self.completion.selected = 0;
            self.completion.dismissed_word = None;
        }

        let mut line_action = None;
        if focused && tab_pressed && (shift || !self.completion.open) {
            let consumed = ui.input_mut(|input| {
                if shift {
                    input.consume_key(Modifiers::SHIFT, Key::Tab)
                } else {
                    input.consume_key(Modifiers::NONE, Key::Tab)
                }
            });
            if consumed {
                line_action = Some(if shift {
                    EditAction::Outdent
                } else {
                    EditAction::Indent
                });
                self.completion.open = false;
            }
        }

        let mut keys = PopupKeys::default();
        if self.completion.open && focused {
            ui.input_mut(|input| {
                keys.down = consume_unmodified_key(input, Key::ArrowDown);
                keys.up = consume_unmodified_key(input, Key::ArrowUp);
                keys.accept = consume_unmodified_key(input, Key::Enter) > 0
                    || consume_unmodified_key(input, Key::Tab) > 0;
                keys.dismiss = consume_unmodified_key(input, Key::Escape) > 0;
            });
        }

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
            let text_output = text_edit.show(ui);
            gutter::paint(
                ui,
                &text_output,
                source,
                diagnostics,
                &self.theme,
                gutter_rect,
            );
            output = Some(text_output);
        });

        let mut output = output.expect("the editor is shown exactly once");
        let mut applied = false;
        if let Some(action) = line_action
            && edit_action(ui.ctx(), source, action)
        {
            applied = true;
            self.completion.open = false;
            if let Some(state) = TextEditState::load(ui.ctx(), id) {
                output.cursor_range = state.cursor.char_range();
            }
        }
        let changed = output.response.response.changed();
        let focus_after = output.response.response.has_focus();
        let selection_empty = output.cursor_range.is_some_and(|range| range.is_empty());
        let cursor = output_cursor_byte(&output, source);
        let prefix = cursor.map_or_else(String::new, |cursor| word_prefix(source, cursor));

        if output.response.response.clicked() {
            self.completion.open = false;
        }

        if self.completion.dismissed_word.as_deref() != Some(prefix.as_str()) {
            self.completion.dismissed_word = None;
        }

        if !focus_after || !selection_empty {
            self.completion.open = false;
        }
        if changed {
            if tab_pressed
                || command_down
                || !selection_empty
                || prefix.is_empty()
                || self.completion.dismissed_word.as_deref() == Some(prefix.as_str())
            {
                self.completion.open = false;
            } else {
                self.completion.open = true;
                if self.completion.last_word != prefix {
                    self.completion.last_word = prefix.clone();
                    self.completion.selected = 0;
                }
            }
        }
        if tab_pressed && !keys.accept {
            self.completion.open = false;
        }

        if self.completion.open {
            let Some(cursor) = cursor else {
                self.completion.open = false;
                return EditorOutput {
                    text_edit: output,
                    applied,
                };
            };
            let mut candidates = external
                .filter(|items| !items.is_empty())
                .map(|items| {
                    items
                        .iter()
                        .filter(|candidate| candidate.insert.starts_with(&prefix))
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if candidates.is_empty() {
                candidates = completion::suggestions(source, cursor);
            }

            if keys.dismiss {
                self.completion.open = false;
                self.completion.dismissed_word = Some(prefix);
            } else if candidates.is_empty() {
                self.completion.open = false;
            } else {
                let len = candidates.len();
                let moved = keys.down > 0 || keys.up > 0;
                self.completion.selected =
                    (self.completion.selected + keys.down + len - keys.up % len) % len;
                let selected = self.completion.selected.min(len - 1);

                let clicked = if keys.accept {
                    None
                } else {
                    show_popup(ui.ctx(), id, &output, &candidates, selected, moved)
                };

                if let Some(index) = keys.accept.then_some(selected).or(clicked) {
                    let candidate = &candidates[index];
                    let start = word_start(source, cursor);
                    if accept_completion(
                        ui,
                        id,
                        source,
                        &mut output,
                        start..cursor,
                        &candidate.insert,
                    ) {
                        applied = true;
                        self.completion.open = false;
                        self.completion.last_word.clear();
                        self.completion.dismissed_word = None;
                        ui.ctx().memory_mut(|memory| memory.request_focus(id));
                    }
                }
            }
        }

        EditorOutput {
            text_edit: output,
            applied,
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

fn show_popup(
    ctx: &egui::Context,
    id: egui::Id,
    output: &TextEditOutput,
    candidates: &[Candidate],
    selected: usize,
    scroll_selected: bool,
) -> Option<usize> {
    let cursor = output.cursor_range?;
    let cursor_rect = output
        .galley
        .pos_from_cursor(CCursor::new(cursor.primary.index.0));
    let anchor = output.galley_pos + cursor_rect.left_bottom().to_vec2();
    let screen = ctx.content_rect();
    let width = 360.0_f32.min(screen.width() - 16.0).max(220.0);
    let max_height = 220.0_f32.min(screen.height() - 16.0).max(80.0);
    let x = anchor
        .x
        .min(screen.right() - width - 8.0)
        .max(screen.left() + 8.0);
    let y = if anchor.y + max_height > screen.bottom() {
        (anchor.y - cursor_rect.height().max(16.0) - max_height).max(screen.top() + 8.0)
    } else {
        anchor.y
    };

    let mut clicked = None;
    egui::Area::new(id.with("completion_popup"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(x, y))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(width);
                ui.set_max_width(width);
                egui::ScrollArea::vertical()
                    .max_height(max_height)
                    .show(ui, |ui| {
                        for (index, candidate) in candidates.iter().enumerate() {
                            let row_width = (width - 16.0).max(160.0);
                            let button = egui::Button::new(candidate.label.clone())
                                .shortcut_text(candidate.detail.clone())
                                .truncate()
                                .selected(index == selected)
                                .min_size(egui::vec2(row_width, 0.0));
                            let response = ui.add(button).on_hover_text(&candidate.detail);
                            if index == selected && scroll_selected {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                clicked = Some(index);
                            }
                        }
                    });
            });
        });
    clicked
}

fn accept_completion(
    ui: &egui::Ui,
    id: egui::Id,
    source: &mut String,
    output: &mut TextEditOutput,
    range: Range<usize>,
    insert: &str,
) -> bool {
    if range.start > range.end
        || range.end > source.len()
        || !source.is_char_boundary(range.start)
        || !source.is_char_boundary(range.end)
    {
        return false;
    }

    let old_range = output
        .cursor_range
        .unwrap_or_else(|| CCursorRange::one(CCursor::new(0)));
    let old_source = source.clone();
    source.replace_range(range.clone(), insert);
    let new_char = diagnostics::char_index(source, range.start + insert.len());
    let new_range = CCursorRange::one(CCursor::new(new_char));
    output.cursor_range = Some(new_range);
    output.state.cursor.set_char_range(Some(new_range));
    let mut undoer = output.state.undoer();
    undoer.add_undo(&(old_range, old_source));
    undoer.add_undo(&(new_range, source.clone()));
    output.state.set_undoer(undoer);
    output.state.clone().store(ui.ctx(), id);
    true
}

fn output_cursor_byte(output: &TextEditOutput, source: &str) -> Option<usize> {
    output
        .cursor_range
        .map(|range| diagnostics::byte_index(source, range.primary.index.0))
}

fn word_start(source: &str, cursor: usize) -> usize {
    let mut start = cursor.min(source.len());
    while start > 0 {
        let Some(character) = source[..start].chars().next_back() else {
            break;
        };
        if !completion::is_identifier_continue(character) {
            break;
        }
        start -= character.len_utf8();
    }
    start
}

fn word_prefix(source: &str, cursor: usize) -> String {
    let start = word_start(source, cursor);
    source[start..cursor.min(source.len())].to_owned()
}

fn consume_unmodified_key(input: &mut InputState, key: Key) -> usize {
    let mut count = 0;
    input.events.retain(|event| {
        let matches = matches!(
            event,
            Event::Key {
                key: event_key,
                modifiers,
                pressed: true,
                ..
            } if *event_key == key && modifiers.is_none()
        );
        count += usize::from(matches);
        !matches
    });
    count
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

    #[test]
    fn word_prefix_stops_at_punctuation() {
        assert_eq!(word_prefix("cube(w", 6), "w");
        assert_eq!(word_prefix("cube(", 5), "");
        assert_eq!(word_prefix("$fn", 3), "$fn");
    }
}
