use rowan::Language;

/// Every token and node kind in the tiny OpenSCAD subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
#[allow(non_camel_case_types)]
pub enum SyntaxKind {
    WHITESPACE = 0,
    LINE_COMMENT,
    BLOCK_COMMENT,
    IDENT,
    NUMBER,
    L_BRACK,
    R_BRACK,
    L_PAREN,
    R_PAREN,
    L_BRACE,
    R_BRACE,
    COMMA,
    SEMICOLON,
    EQ,
    SOURCE_FILE,
    ASSIGNMENT,
    MODULE_CALL,
    ARG_LIST,
    BLOCK,
    VECTOR,
    NAMED_ARG,
    ERROR,
    EOF,
    __LAST,
}

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            Self::WHITESPACE | Self::LINE_COMMENT | Self::BLOCK_COMMENT
        )
    }
}

/// Rowan language marker for the OpenSCAD subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScadLanguage {}

impl Language for ScadLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        match raw.0 {
            0 => SyntaxKind::WHITESPACE,
            1 => SyntaxKind::LINE_COMMENT,
            2 => SyntaxKind::BLOCK_COMMENT,
            3 => SyntaxKind::IDENT,
            4 => SyntaxKind::NUMBER,
            5 => SyntaxKind::L_BRACK,
            6 => SyntaxKind::R_BRACK,
            7 => SyntaxKind::L_PAREN,
            8 => SyntaxKind::R_PAREN,
            9 => SyntaxKind::L_BRACE,
            10 => SyntaxKind::R_BRACE,
            11 => SyntaxKind::COMMA,
            12 => SyntaxKind::SEMICOLON,
            13 => SyntaxKind::EQ,
            14 => SyntaxKind::SOURCE_FILE,
            15 => SyntaxKind::ASSIGNMENT,
            16 => SyntaxKind::MODULE_CALL,
            17 => SyntaxKind::ARG_LIST,
            18 => SyntaxKind::BLOCK,
            19 => SyntaxKind::VECTOR,
            20 => SyntaxKind::NAMED_ARG,
            21 => SyntaxKind::ERROR,
            22 => SyntaxKind::EOF,
            _ => SyntaxKind::ERROR,
        }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        ScadLanguage::kind_to_raw(kind)
    }
}

pub type SyntaxNode = rowan::SyntaxNode<ScadLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<ScadLanguage>;

/// A diagnostic produced while lexing or parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub message: String,
    pub offset: usize,
}

impl SyntaxError {
    pub(crate) fn new(message: impl Into<String>, offset: usize) -> Self {
        Self {
            message: message.into(),
            offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_mapping_round_trips() {
        for raw in 0..(SyntaxKind::__LAST as u16) {
            let kind = ScadLanguage::kind_from_raw(rowan::SyntaxKind(raw));
            assert_eq!(ScadLanguage::kind_to_raw(kind).0, raw);
        }
    }
}
