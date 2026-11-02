use eframe::egui::{self, Align2, Rect, text::CCursor, text_edit::TextEditOutput};

use super::diagnostics::{self, Diagnostic, Severity};
use super::theme::{FONT_SIZE, GUTTER_PADDING, Theme, font_id};

pub fn width(source: &str) -> f32 {
    let digits = diagnostics::lines(source).len().to_string().len().max(2);
    GUTTER_PADDING * 2.0 + digits as f32 * 8.0
}

pub fn paint(
    ui: &egui::Ui,
    output: &TextEditOutput,
    source: &str,
    diagnostics: &[Diagnostic],
    theme: &Theme,
    gutter_rect: Rect,
) {
    let Some(cursor) = output.cursor_range else {
        // The cursor is absent until the editor first receives focus, but the
        // gutter should already show line numbers on the first frame.
        let current_line = None;
        paint_lines(
            ui,
            output,
            source,
            diagnostics,
            theme,
            gutter_rect,
            current_line,
        );
        return;
    };
    let cursor_byte = diagnostics::byte_index(source, cursor.primary.index.0);
    let current_line = Some(diagnostics::line_index(source, cursor_byte));
    paint_lines(
        ui,
        output,
        source,
        diagnostics,
        theme,
        gutter_rect,
        current_line,
    );
}

fn paint_lines(
    ui: &egui::Ui,
    output: &TextEditOutput,
    source: &str,
    diagnostics: &[Diagnostic],
    theme: &Theme,
    gutter_rect: Rect,
    current_line: Option<usize>,
) {
    let error_lines: Vec<usize> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| diagnostics::line_index(source, diagnostic.range.start))
        .collect();

    let font = font_id();
    let painter = ui.painter();
    for (index, line) in diagnostics::lines(source).iter().enumerate() {
        let rect = output
            .galley
            .pos_from_cursor(CCursor::new(diagnostics::char_index(source, line.start)));
        let y = output.galley_pos.y + rect.top();
        let number_color = if current_line == Some(index) {
            theme.current_line_number
        } else {
            theme.gutter
        };
        painter.text(
            egui::pos2(gutter_rect.right() - GUTTER_PADDING, y),
            Align2::RIGHT_TOP,
            (index + 1).to_string(),
            font.clone(),
            number_color,
        );
        if error_lines.contains(&index) {
            painter.circle_filled(
                egui::pos2(gutter_rect.left() + 3.0, y + FONT_SIZE * 0.55),
                2.0,
                theme.error,
            );
        }
    }
}
