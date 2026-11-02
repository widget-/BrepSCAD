use std::ops::Range;

use brepscad_syntax::{TextEdit, TextRange, TextSize, apply_edit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEditResult {
    pub source: String,
    pub selection: Range<usize>,
}

/// Toggles `// ` on every line touched by `selection`. A collapsed selection
/// edits the current line. If every non-blank line is already commented the
/// prefixes are removed; otherwise one is added after the existing indentation.
pub fn toggle_line_comment(source: &str, selection: Range<usize>) -> LineEditResult {
    let ranges = selected_line_ranges(source, selection.clone());
    let commented = ranges
        .iter()
        .any(|range| !source[range.start..range.end].trim().is_empty())
        && ranges
            .iter()
            .filter(|range| !source[range.start..range.end].trim().is_empty())
            .all(|range| {
                source[range.start..range.end]
                    .trim_start()
                    .starts_with("//")
            });

    let mut edits = Vec::new();
    for range in &ranges {
        let line = &source[range.start..range.end];
        let indentation = line.len() - line.trim_start().len();
        let position = range.start + indentation;
        if commented {
            let rest = &line[indentation..];
            if rest.starts_with("// ") {
                edits.push(TextEdit::new(text_range(position, position + 3), ""));
            } else if rest.starts_with("//") {
                edits.push(TextEdit::new(text_range(position, position + 2), ""));
            }
        } else {
            edits.push(TextEdit::new(text_range(position, position), "// "));
        }
    }

    apply_edits(source, selection, edits)
}

pub fn indent_lines(source: &str, selection: Range<usize>, indent: &str) -> LineEditResult {
    let edits = line_starts(source, selection.clone())
        .into_iter()
        .map(|start| TextEdit::new(text_range(start, start), indent))
        .collect();
    apply_edits(source, selection, edits)
}

pub fn outdent_lines(source: &str, selection: Range<usize>, indent: &str) -> LineEditResult {
    let mut edits = Vec::new();
    for start in line_starts(source, selection.clone()) {
        let line = &source[start..line_end(source, start)];
        let remove = if line.starts_with('\t') {
            1
        } else {
            line.bytes()
                .take(indent.len())
                .take_while(|byte| *byte == b' ')
                .count()
        };
        if remove > 0 {
            edits.push(TextEdit::new(text_range(start, start + remove), ""));
        }
    }
    apply_edits(source, selection, edits)
}

fn selected_line_ranges(source: &str, selection: Range<usize>) -> Vec<Range<usize>> {
    line_starts(source, selection)
        .into_iter()
        .map(|start| start..line_end(source, start))
        .collect()
}

fn line_starts(source: &str, selection: Range<usize>) -> Vec<usize> {
    let start = line_start(source, selection.start);
    let end = line_start(source, selection.end.saturating_sub(1).max(selection.start));
    let mut starts = vec![start];
    let mut position = start;
    while position < end {
        let Some(newline) = source[position..].find('\n') else {
            break;
        };
        position += newline + 1;
        if position <= end {
            starts.push(position);
        }
    }
    starts
}

fn line_start(source: &str, byte: usize) -> usize {
    source[..byte.min(source.len())]
        .rfind('\n')
        .map_or(0, |newline| newline + 1)
}

fn line_end(source: &str, start: usize) -> usize {
    source[start..]
        .find('\n')
        .map_or(source.len(), |newline| start + newline)
}

fn text_range(start: usize, end: usize) -> TextRange {
    TextRange::new(TextSize::from(start as u32), TextSize::from(end as u32))
}

fn shift_point(point: usize, edits: &[TextEdit]) -> usize {
    let mut delta: isize = 0;
    for edit in edits {
        let start = usize::from(edit.range.start());
        let end = usize::from(edit.range.end());
        let change = edit.replacement.len() as isize - (end - start) as isize;
        if end <= point || start < point {
            delta += change;
        }
    }
    point.saturating_add_signed(delta)
}

fn apply_edits(source: &str, selection: Range<usize>, mut edits: Vec<TextEdit>) -> LineEditResult {
    let start = shift_point(selection.start, &edits);
    let end = shift_point(selection.end, &edits);

    edits.sort_by_key(|edit| std::cmp::Reverse(usize::from(edit.range.start())));
    let mut result = source.to_owned();
    for edit in edits {
        result = apply_edit(&result, &edit).expect("line edits are in bounds");
    }
    let len = result.len();
    LineEditResult {
        source: result,
        selection: start.min(len)..end.min(len),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_toggles_the_current_line() {
        let first = toggle_line_comment("cube(1);", 2..2);
        assert_eq!(first.source, "// cube(1);");
        let second = toggle_line_comment(&first.source, first.selection);
        assert_eq!(second.source, "cube(1);");
    }

    #[test]
    fn comment_preserves_indentation_and_toggles_multiple_lines() {
        let source = "  cube(1);\n    sphere(2);\n";
        let result = toggle_line_comment(source, 0..source.len());
        assert_eq!(result.source, "  // cube(1);\n    // sphere(2);\n");
        assert_eq!(
            toggle_line_comment(&result.source, result.selection).source,
            source
        );
    }

    #[test]
    fn indent_and_outdent_selected_lines() {
        let source = "a\nb\nc";
        let indented = indent_lines(source, 2..3, "  ");
        assert_eq!(indented.source, "a\n  b\nc");
        assert_eq!(outdent_lines(&indented.source, 0..7, "  ").source, source);
    }

    #[test]
    fn indent_and_outdent_every_line_in_a_range() {
        let source = "a\nb\nc\n";
        let indented = indent_lines(source, 0..source.len(), "  ");
        assert_eq!(indented.source, "  a\n  b\n  c\n");
        let outdented = outdent_lines(&indented.source, 0..indented.source.len(), "  ");
        assert_eq!(outdented.source, source);
    }

    #[test]
    fn selection_shift_is_tracked_after_insertion() {
        let result = toggle_line_comment("cube(1);", 4..6);
        assert_eq!(result.source, "// cube(1);");
        assert_eq!(result.selection, 7..9);
    }
}
