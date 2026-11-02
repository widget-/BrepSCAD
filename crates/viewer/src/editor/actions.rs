use std::ops::Range;

use eframe::egui::{
    self,
    text::{CCursor, CCursorRange},
    text_edit::TextEditState,
};

use super::diagnostics;
use super::keymap;

const INDENT: &str = "    ";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAction {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    PastePlain,
    SelectAll,
    ToggleLineComment,
    Indent,
    Outdent,
}

pub fn editor_id() -> egui::Id {
    egui::Id::unique("brepscad_source_editor")
}

pub fn action(ctx: &egui::Context, source: &mut String, action: EditAction) -> bool {
    match action {
        EditAction::Undo => undo(ctx, source, false),
        EditAction::Redo => undo(ctx, source, true),
        EditAction::Cut => cut(ctx, source),
        EditAction::Copy => copy(ctx, source),
        EditAction::Paste => paste(ctx, source, false),
        EditAction::PastePlain => paste(ctx, source, true),
        EditAction::SelectAll => select_all(ctx, source),
        EditAction::ToggleLineComment => {
            let Some(selection) = selection_bytes(ctx, source) else {
                return false;
            };
            let result = keymap::toggle_line_comment(source, selection);
            apply_line_result(ctx, source, result)
        }
        EditAction::Indent => {
            let Some(selection) = selection_bytes(ctx, source) else {
                return false;
            };
            let result = keymap::indent_lines(source, selection, INDENT);
            apply_line_result(ctx, source, result)
        }
        EditAction::Outdent => {
            let Some(selection) = selection_bytes(ctx, source) else {
                return false;
            };
            let result = keymap::outdent_lines(source, selection, INDENT);
            apply_line_result(ctx, source, result)
        }
    }
}

pub fn can(ctx: &egui::Context, source: &str, action: EditAction) -> bool {
    match action {
        EditAction::Undo => {
            let (state, range) = current_state(ctx, source);
            state.undoer().has_undo(&(range, source.to_owned()))
        }
        EditAction::Redo => {
            let (state, range) = current_state(ctx, source);
            state.undoer().has_redo(&(range, source.to_owned()))
        }
        EditAction::Cut | EditAction::Copy => {
            selection_bytes(ctx, source).is_some_and(|selection| selection.start < selection.end)
        }
        EditAction::Paste | EditAction::PastePlain | EditAction::SelectAll => true,
        EditAction::ToggleLineComment | EditAction::Indent | EditAction::Outdent => true,
    }
}

pub fn reset(ctx: &egui::Context) {
    let mut state = TextEditState::load(ctx, editor_id()).unwrap_or_default();
    state.clear_undoer();
    state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(0))));
    state.store(ctx, editor_id());
}

fn undo(ctx: &egui::Context, source: &mut String, redo: bool) -> bool {
    let (mut state, range) = current_state(ctx, source);
    let current = (range, source.clone());
    let mut undoer = state.undoer();
    let restored = if redo {
        undoer.redo(&current).cloned()
    } else {
        undoer.undo(&current).cloned()
    };
    let Some((range, text)) = restored else {
        return false;
    };
    *source = text;
    state.cursor.set_char_range(Some(range));
    state.set_undoer(undoer);
    state.store(ctx, editor_id());
    true
}

fn copy(ctx: &egui::Context, source: &str) -> bool {
    let Some(selection) = selection_bytes(ctx, source) else {
        return false;
    };
    if selection.start == selection.end {
        return false;
    }
    ctx.copy_text(source[selection].to_owned());
    false
}

fn cut(ctx: &egui::Context, source: &mut String) -> bool {
    let Some(selection) = selection_bytes(ctx, source) else {
        return false;
    };
    if selection.start == selection.end {
        return false;
    }
    ctx.copy_text(source[selection.clone()].to_owned());
    replace_source(ctx, source, selection.clone(), "", selection.start)
}

fn paste(ctx: &egui::Context, source: &mut String, plain: bool) -> bool {
    let Ok(mut clipboard) = arboard::Clipboard::new() else {
        return false;
    };
    let Ok(text) = clipboard.get_text() else {
        return false;
    };
    let text = normalize_clipboard(&text, plain);
    let Some(selection) = selection_bytes(ctx, source) else {
        return false;
    };
    let cursor = selection.start + text.len();
    replace_source(ctx, source, selection, &text, cursor)
}

fn select_all(ctx: &egui::Context, source: &str) -> bool {
    let mut state = TextEditState::load(ctx, editor_id()).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(CCursorRange::select_all_str(source)));
    state.store(ctx, editor_id());
    false
}

fn apply_line_result(
    ctx: &egui::Context,
    source: &mut String,
    result: keymap::LineEditResult,
) -> bool {
    if result.source == *source {
        return false;
    }
    if !replace_source(
        ctx,
        source,
        0..source.len(),
        &result.source,
        result.selection.start,
    ) {
        return false;
    }
    let mut state = TextEditState::load(ctx, editor_id()).unwrap_or_default();
    let range = byte_range_to_chars(&result.source, result.selection);
    state.cursor.set_char_range(Some(range));
    state.store(ctx, editor_id());
    true
}

fn replace_source(
    ctx: &egui::Context,
    source: &mut String,
    range: Range<usize>,
    replacement: &str,
    cursor_byte: usize,
) -> bool {
    if range.start > range.end
        || range.end > source.len()
        || !source.is_char_boundary(range.start)
        || !source.is_char_boundary(range.end)
    {
        return false;
    }

    let old_range = current_state(ctx, source).1;
    let old = (old_range, source.clone());
    source.replace_range(range, replacement);
    let cursor = byte_range_to_chars(source, cursor_byte..cursor_byte);
    let mut state = TextEditState::load(ctx, editor_id()).unwrap_or_default();
    state.cursor.set_char_range(Some(cursor));
    let mut undoer = state.undoer();
    undoer.add_undo(&old);
    undoer.add_undo(&(cursor, source.clone()));
    state.set_undoer(undoer);
    state.store(ctx, editor_id());
    true
}

fn current_state(ctx: &egui::Context, source: &str) -> (TextEditState, CCursorRange) {
    let state = TextEditState::load(ctx, editor_id()).unwrap_or_default();
    let range = state
        .cursor
        .char_range()
        .unwrap_or_else(|| CCursorRange::one(CCursor::end_of_str(source)));
    (state, range)
}

fn selection_bytes(ctx: &egui::Context, source: &str) -> Option<Range<usize>> {
    let (_, range) = current_state(ctx, source);
    let chars = range.as_sorted_char_range();
    Some(
        diagnostics::byte_index(source, chars.start.0)
            ..diagnostics::byte_index(source, chars.end.0),
    )
}

fn byte_range_to_chars(source: &str, range: Range<usize>) -> CCursorRange {
    CCursorRange::two(
        CCursor::new(diagnostics::char_index(source, range.start)),
        CCursor::new(diagnostics::char_index(source, range.end)),
    )
}

pub fn normalize_clipboard(text: &str, plain: bool) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if plain { text.replace('\0', "") } else { text }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_normalization_handles_crlf_and_nuls() {
        assert_eq!(normalize_clipboard("a\r\nb\rc", false), "a\nb\nc");
        assert_eq!(normalize_clipboard("a\0b\r\n", true), "ab\n");
    }

    #[test]
    fn undo_and_redo_round_trip_a_replacement() {
        let ctx = egui::Context::default();
        let mut source = "cube(1);".to_owned();
        reset(&ctx);
        assert!(replace_source(&ctx, &mut source, 5..6, "2", 6));
        assert_eq!(source, "cube(2);");
        assert!(action(&ctx, &mut source, EditAction::Undo));
        assert_eq!(source, "cube(1);");
        assert!(action(&ctx, &mut source, EditAction::Redo));
        assert_eq!(source, "cube(2);");
    }

    #[test]
    fn select_all_and_toggle_comment_work_through_the_action_layer() {
        let ctx = egui::Context::default();
        let mut source = "cube(1);\n".to_owned();
        reset(&ctx);
        assert!(!action(&ctx, &mut source, EditAction::SelectAll));
        assert!(action(&ctx, &mut source, EditAction::ToggleLineComment));
        assert_eq!(source, "// cube(1);\n");
    }
}
