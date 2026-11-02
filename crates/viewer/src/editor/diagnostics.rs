use std::ops::Range;
use std::path::PathBuf;

use brepscad_syntax::SyntaxError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    /// Byte offsets into the file named by `source_path`.
    pub range: Range<usize>,
    pub source_path: Option<PathBuf>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, range: Range<usize>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            range,
            source_path: None,
        }
    }
}

pub fn parse_diagnostics(source: &str, errors: &[SyntaxError]) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<_> = errors
        .iter()
        .map(|error| Diagnostic::error(&error.message, error_range(source, error.offset)))
        .collect();
    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.range.start,
            diagnostic.range.end,
            diagnostic.message.clone(),
        )
    });
    diagnostics.dedup_by(|a, b| a.range == b.range && a.message == b.message);
    diagnostics
}

/// Turns a parser offset into a range covering the character at the offset.
/// Parser offsets may point at EOF (for example "expected a statement"), so an
/// empty tail is attributed to the preceding character instead of vanishing.
pub fn error_range(source: &str, offset: usize) -> Range<usize> {
    let len = source.len();
    let mut start = offset.min(len);
    while start > 0 && !source.is_char_boundary(start) {
        start -= 1;
    }

    if start < len {
        let end = source[start..]
            .chars()
            .next()
            .map_or(start, |c| start + c.len_utf8());
        start..end
    } else if len > 0 {
        let mut previous = len - 1;
        while previous > 0 && !source.is_char_boundary(previous) {
            previous -= 1;
        }
        previous..len
    } else {
        0..0
    }
}

pub fn line_index(source: &str, byte: usize) -> usize {
    source[..byte.min(source.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

pub fn lines(source: &str) -> Vec<Range<usize>> {
    let mut starts = vec![0];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }

    starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = starts
                .get(index + 1)
                .map_or(source.len(), |next| next.saturating_sub(1));
            *start..end
        })
        .collect()
}

pub fn byte_index(source: &str, char_index: usize) -> usize {
    source
        .char_indices()
        .nth(char_index)
        .map_or(source.len(), |(byte, _)| byte)
}

pub fn char_index(source: &str, byte: usize) -> usize {
    source[..byte.min(source.len())].chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_at_eof_covers_the_last_character() {
        assert_eq!(error_range("cube", 4), 3..4);
        assert_eq!(error_range("", 0), 0..0);
    }

    #[test]
    fn error_offset_is_clamped_to_char_boundaries() {
        assert_eq!(error_range("é", 1), 0..2);
    }

    #[test]
    fn lines_exclude_newlines_and_keep_the_last_line() {
        let source = "a\nbc\n";
        assert_eq!(lines(source), vec![0..1, 2..4, 5..5]);
        assert_eq!(line_index(source, 0), 0);
        assert_eq!(line_index(source, 3), 1);
        assert_eq!(line_index(source, 5), 2);
    }

    #[test]
    fn char_and_byte_indices_round_trip() {
        let source = "aé\nz";
        assert_eq!(byte_index(source, 1), 1);
        assert_eq!(byte_index(source, 2), 3);
        assert_eq!(char_index(source, 3), 2);
    }
}
