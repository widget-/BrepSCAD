use eframe::egui::{Color32, FontId, Stroke};

pub const FONT_SIZE: f32 = 13.0;
pub const GUTTER_PADDING: f32 = 10.0;

pub fn font_id() -> FontId {
    FontId::monospace(FONT_SIZE)
}

/// Editor colours. The viewer runs with egui's dark theme, so these are chosen
/// for a dark background and kept in one place instead of being scattered
/// through the highlighter and the gutter.
pub struct Theme {
    pub text: Color32,
    pub comment: Color32,
    pub string: Color32,
    pub number: Color32,
    pub keyword: Color32,
    pub declaration: Color32,
    pub literal: Color32,
    pub punctuation: Color32,
    pub error: Color32,
    pub gutter: Color32,
    pub current_line_number: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            text: Color32::from_rgb(220, 223, 228),
            comment: Color32::from_rgb(112, 132, 112),
            string: Color32::from_rgb(206, 145, 120),
            number: Color32::from_rgb(181, 206, 168),
            keyword: Color32::from_rgb(197, 134, 192),
            declaration: Color32::from_rgb(86, 182, 194),
            literal: Color32::from_rgb(86, 156, 214),
            punctuation: Color32::from_rgb(160, 164, 170),
            error: Color32::from_rgb(244, 71, 71),
            gutter: Color32::from_rgb(108, 112, 118),
            current_line_number: Color32::from_rgb(230, 232, 236),
        }
    }
}

impl Theme {
    pub fn error_underline(&self) -> Stroke {
        Stroke::new(1.0, self.error)
    }
}
