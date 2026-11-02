use serde::{Deserialize, Serialize};

/// An LSP position. `character` is measured in UTF-16 code units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

pub fn offset_to_position(text: &str, offset: usize) -> Position {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }

    let before = &text[..offset];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
    let character = text[line_start..offset].encode_utf16().count() as u32;
    Position { line, character }
}

pub fn position_to_offset(text: &str, position: Position) -> usize {
    let mut line_start = 0;
    for _ in 0..position.line {
        let Some(newline) = text[line_start..].find('\n') else {
            return text.len();
        };
        line_start += newline + 1;
    }

    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |newline| line_start + newline);
    let mut utf16 = 0;
    for (byte, character) in text[line_start..line_end].char_indices() {
        if utf16 >= position.character as usize {
            return line_start + byte;
        }
        utf16 += character.len_utf16();
        if utf16 > position.character as usize {
            return line_start + byte;
        }
    }
    line_end
}

pub fn range_to_lsp(text: &str, range: std::ops::Range<usize>) -> Range {
    Range {
        start: offset_to_position(text, range.start),
        end: offset_to_position(text, range.end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_round_trips() {
        let text = "cube(1);\nsphere(2);\n";
        assert_eq!(
            offset_to_position(text, 9),
            Position {
                line: 1,
                character: 0
            }
        );
        assert_eq!(
            position_to_offset(
                text,
                Position {
                    line: 1,
                    character: 3
                }
            ),
            12
        );
    }

    #[test]
    fn non_ascii_uses_utf16_units() {
        let text = "aé\n😀b";
        assert_eq!(
            offset_to_position(text, 4),
            Position {
                line: 1,
                character: 0
            }
        );
        assert_eq!(
            offset_to_position(text, 9),
            Position {
                line: 1,
                character: 3
            }
        );
        assert_eq!(
            position_to_offset(
                text,
                Position {
                    line: 1,
                    character: 3
                }
            ),
            9
        );
    }

    #[test]
    fn crlf_and_out_of_range_positions_clamp() {
        let text = "a\r\nb";
        assert_eq!(
            offset_to_position(text, 3),
            Position {
                line: 1,
                character: 0
            }
        );
        assert_eq!(
            position_to_offset(
                text,
                Position {
                    line: 9,
                    character: 9
                }
            ),
            text.len()
        );
    }
}
