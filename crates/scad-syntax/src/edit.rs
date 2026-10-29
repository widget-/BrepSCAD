use rowan::TextRange;
use thiserror::Error;

/// A replacement of a source range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub range: TextRange,
    pub replacement: String,
}

impl TextEdit {
    pub fn new(range: TextRange, replacement: impl Into<String>) -> Self {
        Self {
            range,
            replacement: replacement.into(),
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EditError {
    #[error("edit range {start}..{end} is out of bounds for text of length {len}")]
    OutOfBounds {
        start: usize,
        end: usize,
        len: usize,
    },
    #[error("edit range {start}..{end} does not fall on UTF-8 character boundaries")]
    NotCharBoundary { start: usize, end: usize },
}

/// Applies one edit and returns the new source.
pub fn apply_edit(text: &str, edit: &TextEdit) -> Result<String, EditError> {
    let start = usize::from(edit.range.start());
    let end = usize::from(edit.range.end());

    if start > end || end > text.len() {
        return Err(EditError::OutOfBounds {
            start,
            end,
            len: text.len(),
        });
    }
    if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return Err(EditError::NotCharBoundary { start, end });
    }

    let mut result = text.to_owned();
    result.replace_range(start..end, &edit.replacement);
    Ok(result)
}
