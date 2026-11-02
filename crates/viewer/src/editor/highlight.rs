use brepscad_syntax::{Parse, SyntaxKind, SyntaxToken, parse};
use eframe::egui::text::{LayoutJob, TextFormat};

use super::diagnostics::Diagnostic;
use super::theme::{Theme, font_id};

/// Cached parse and `LayoutJob`. The layouter runs every frame, so rebuilding
/// both only when the text or diagnostics change keeps typing responsive on
/// larger files.
pub struct Highlighter {
    source: String,
    parse: Parse,
    diagnostics: Vec<Diagnostic>,
    job: LayoutJob,
}

impl Default for Highlighter {
    fn default() -> Self {
        let source = String::new();
        let parse = parse(&source);
        Self {
            source,
            parse,
            diagnostics: Vec::new(),
            job: LayoutJob::default(),
        }
    }
}

impl Highlighter {
    /// Refreshes the cache and returns the layout job. The caller sets the wrap
    /// width on a clone before handing it to the font system.
    pub fn job(&mut self, source: &str, diagnostics: &[Diagnostic], theme: &Theme) -> &LayoutJob {
        if self.source != source || self.diagnostics != diagnostics {
            self.parse = parse(source);
            self.job = layout_job(source, &self.parse, diagnostics, theme);
            self.source = source.to_owned();
            self.diagnostics = diagnostics.to_vec();
        }
        &self.job
    }
}

pub fn layout_job(
    source: &str,
    parse: &Parse,
    diagnostics: &[Diagnostic],
    theme: &Theme,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    job.keep_trailing_whitespace = true;

    for element in parse.syntax().descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };
        if token.kind() == SyntaxKind::EOF {
            continue;
        }
        append_token(&mut job, source, &token, diagnostics, theme);
    }

    job
}

fn append_token(
    job: &mut LayoutJob,
    source: &str,
    token: &SyntaxToken,
    diagnostics: &[Diagnostic],
    theme: &Theme,
) {
    let range = token.text_range();
    let start = usize::from(range.start());
    let end = usize::from(range.end());
    if start >= end {
        return;
    }

    let format = format_for(token, theme);
    let mut cuts = vec![start, end];
    for diagnostic in diagnostics {
        let diagnostic_start = diagnostic.range.start.max(start);
        let diagnostic_end = diagnostic.range.end.min(end);
        if diagnostic_start < diagnostic_end {
            cuts.push(diagnostic_start);
            cuts.push(diagnostic_end);
        }
    }
    cuts.sort_unstable();
    cuts.dedup();

    for pair in cuts.windows(2) {
        let (segment_start, segment_end) = (pair[0], pair[1]);
        if segment_start >= segment_end {
            continue;
        }
        let mut segment_format = format.clone();
        if diagnostics.iter().any(|diagnostic| {
            diagnostic.range.start <= segment_start && segment_end <= diagnostic.range.end
        }) {
            segment_format.underline = theme.error_underline();
        }
        job.append(&source[segment_start..segment_end], 0.0, segment_format);
    }
}

pub fn format_for(token: &SyntaxToken, theme: &Theme) -> TextFormat {
    let mut format = TextFormat {
        font_id: font_id(),
        color: theme.text,
        ..Default::default()
    };

    match token.kind() {
        SyntaxKind::WHITESPACE => {}
        SyntaxKind::LINE_COMMENT | SyntaxKind::BLOCK_COMMENT => {
            format.color = theme.comment;
            format.italics = true;
        }
        SyntaxKind::STRING => format.color = theme.string,
        SyntaxKind::NUMBER => format.color = theme.number,
        SyntaxKind::TRUE | SyntaxKind::FALSE | SyntaxKind::UNDEF => format.color = theme.literal,
        SyntaxKind::IDENT => {
            let text = token.text();
            if is_keyword(text) {
                format.color = theme.keyword;
            } else if is_declaration_name(token) {
                format.color = theme.declaration;
            }
        }
        SyntaxKind::PLUS
        | SyntaxKind::MINUS
        | SyntaxKind::STAR
        | SyntaxKind::SLASH
        | SyntaxKind::PERCENT
        | SyntaxKind::EQ
        | SyntaxKind::EQEQ
        | SyntaxKind::NE
        | SyntaxKind::LT
        | SyntaxKind::LE
        | SyntaxKind::GT
        | SyntaxKind::GE
        | SyntaxKind::AND_AND
        | SyntaxKind::OR_OR
        | SyntaxKind::BANG
        | SyntaxKind::L_PAREN
        | SyntaxKind::R_PAREN
        | SyntaxKind::L_BRACK
        | SyntaxKind::R_BRACK
        | SyntaxKind::L_BRACE
        | SyntaxKind::R_BRACE
        | SyntaxKind::COMMA
        | SyntaxKind::SEMICOLON
        | SyntaxKind::COLON
        | SyntaxKind::QUESTION
        | SyntaxKind::DOT
        | SyntaxKind::CARET
        | SyntaxKind::HASH => format.color = theme.punctuation,
        _ => {}
    }

    format
}

pub fn is_keyword(text: &str) -> bool {
    matches!(
        text,
        "module"
            | "function"
            | "for"
            | "if"
            | "else"
            | "let"
            | "each"
            | "include"
            | "use"
            | "echo"
            | "assert"
    )
}

fn is_declaration_name(token: &SyntaxToken) -> bool {
    let Some(parent) = token.parent() else {
        return false;
    };

    match parent.kind() {
        SyntaxKind::MODULE_DECLARATION | SyntaxKind::FUNCTION_DECLARATION => {
            let mut idents = parent
                .children_with_tokens()
                .filter_map(|element| element.into_token())
                .filter(|child| child.kind() == SyntaxKind::IDENT);
            // The first identifier is the `module` / `function` keyword; the
            // next one is the declared name.
            let _keyword = idents.next();
            idents
                .next()
                .is_some_and(|child| child.text_range() == token.text_range())
        }
        SyntaxKind::PARAM => parent
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|child| child.kind() == SyntaxKind::IDENT)
            .is_some_and(|child| child.text_range() == token.text_range()),
        SyntaxKind::ASSIGNMENT => parent
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|child| child.kind() == SyntaxKind::IDENT)
            .is_some_and(|child| child.text_range() == token.text_range()),
        _ => false,
    }
}

/// Converts a byte range to a `TextFormat` only for tests and callers that
/// want to inspect the colour choice without a syntax token.
#[cfg(test)]
pub fn color_for(kind: SyntaxKind, text: &str, theme: &Theme) -> eframe::egui::Color32 {
    match kind {
        SyntaxKind::LINE_COMMENT | SyntaxKind::BLOCK_COMMENT => theme.comment,
        SyntaxKind::STRING => theme.string,
        SyntaxKind::NUMBER => theme.number,
        SyntaxKind::TRUE | SyntaxKind::FALSE | SyntaxKind::UNDEF => theme.literal,
        SyntaxKind::IDENT if is_keyword(text) => theme.keyword,
        SyntaxKind::IDENT => theme.text,
        SyntaxKind::PLUS
        | SyntaxKind::MINUS
        | SyntaxKind::STAR
        | SyntaxKind::SLASH
        | SyntaxKind::PERCENT
        | SyntaxKind::EQ
        | SyntaxKind::EQEQ
        | SyntaxKind::NE
        | SyntaxKind::LT
        | SyntaxKind::LE
        | SyntaxKind::GT
        | SyntaxKind::GE
        | SyntaxKind::AND_AND
        | SyntaxKind::OR_OR
        | SyntaxKind::BANG
        | SyntaxKind::L_PAREN
        | SyntaxKind::R_PAREN
        | SyntaxKind::L_BRACK
        | SyntaxKind::R_BRACK
        | SyntaxKind::L_BRACE
        | SyntaxKind::R_BRACE
        | SyntaxKind::COMMA
        | SyntaxKind::SEMICOLON
        | SyntaxKind::COLON
        | SyntaxKind::QUESTION
        | SyntaxKind::DOT
        | SyntaxKind::CARET
        | SyntaxKind::HASH => theme.punctuation,
        _ => theme.text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Stroke;

    #[test]
    fn sections_cover_the_source_exactly() {
        let source = "module wheel(r = 4) {\n  // a comment\n  cube([r, r, 1]);\n}\n";
        let parse = parse(source);
        let theme = Theme::default();
        let job = layout_job(source, &parse, &[], &theme);
        let text: String = job
            .sections
            .iter()
            .map(|section| &job.text[section.byte_range.start.0..section.byte_range.end.0])
            .collect();
        assert_eq!(text, source);
        assert_eq!(job.text, source);
    }

    #[test]
    fn keywords_and_comments_are_classified() {
        let theme = Theme::default();
        assert_eq!(
            color_for(SyntaxKind::IDENT, "module", &theme),
            theme.keyword
        );
        assert_eq!(color_for(SyntaxKind::IDENT, "wheel", &theme), theme.text);
        assert_eq!(
            color_for(SyntaxKind::LINE_COMMENT, "// hi", &theme),
            theme.comment
        );
        assert_eq!(color_for(SyntaxKind::NUMBER, "4", &theme), theme.number);
    }

    #[test]
    fn diagnostic_ranges_add_underlines_without_dropping_text() {
        let source = "cube(4)\n";
        let parse = parse(source);
        let theme = Theme::default();
        let diagnostics = vec![Diagnostic::error("missing semicolon", 5..6)];
        let job = layout_job(source, &parse, &diagnostics, &theme);
        let underlined: String = job
            .sections
            .iter()
            .filter(|section| section.format.underline != Stroke::NONE)
            .map(|section| &job.text[section.byte_range.start.0..section.byte_range.end.0])
            .collect();
        assert_eq!(underlined, "4");
    }
}
