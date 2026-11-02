use brepscad_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, parse};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CandidateKind {
    Declaration,
    Parameter,
    Assignment,
    Builtin,
    Special,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub insert: String,
    pub label: String,
    pub detail: String,
    pub kind: CandidateKind,
}

/// Collects completion candidates for `cursor_byte`, a byte offset into
/// `source`. This is deliberately independent of egui so it can be tested
/// without a UI and reused by the LSP server later.
pub fn suggestions(source: &str, cursor_byte: usize) -> Vec<Candidate> {
    let cursor_byte = cursor_byte.min(source.len());
    if !source.is_char_boundary(cursor_byte) {
        return Vec::new();
    }

    let parse = parse(source);
    let syntax = parse.syntax();
    let (token, node) = token_and_node_at(&syntax, cursor_byte);
    if let Some(token) = &token
        && is_in_comment_or_string(token, cursor_byte)
    {
        return Vec::new();
    }

    let prefix = identifier_prefix(source, cursor_byte);
    let mut candidates = Vec::new();
    collect_declarations(&syntax, &mut candidates);
    collect_visible_parameters(&node, &mut candidates);
    collect_assignments(&syntax, &mut candidates);
    collect_builtins(&mut candidates);
    collect_specials(&mut candidates);

    candidates.retain(|candidate| candidate.insert.starts_with(&prefix));
    candidates.sort_by(|a, b| (a.kind, &a.insert, &a.label).cmp(&(b.kind, &b.insert, &b.label)));
    candidates.dedup_by(|a, b| a.insert == b.insert);
    candidates
}

fn token_and_node_at(syntax: &SyntaxNode, cursor: usize) -> (Option<SyntaxToken>, SyntaxNode) {
    let mut last = None;
    for element in syntax.descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };
        let range = token.text_range();
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        let node = token.parent().unwrap_or_else(|| syntax.clone());
        if start <= cursor && cursor < end {
            return (Some(token), node);
        }
        last = Some((token, node));
    }
    if cursor >= usize::from(syntax.text_range().end()) {
        last.map_or((None, syntax.clone()), |(token, node)| (Some(token), node))
    } else {
        (None, syntax.clone())
    }
}

fn is_in_comment_or_string(token: &SyntaxToken, cursor: usize) -> bool {
    let end = usize::from(token.text_range().end());
    if matches!(token.kind(), SyntaxKind::WHITESPACE | SyntaxKind::EOF)
        && cursor == usize::from(token.text_range().start())
        && let Some(previous) = token.prev_token()
        && previous.kind() == SyntaxKind::LINE_COMMENT
        && usize::from(previous.text_range().end()) == cursor
    {
        return true;
    }
    match token.kind() {
        SyntaxKind::LINE_COMMENT => true,
        SyntaxKind::BLOCK_COMMENT | SyntaxKind::STRING => cursor < end,
        _ => false,
    }
}

fn identifier_prefix(source: &str, cursor: usize) -> String {
    let mut start = cursor;
    for (index, character) in source[..cursor].char_indices().rev() {
        if is_identifier_continue(character) {
            start = index;
        } else {
            break;
        }
    }
    source[start..cursor].to_owned()
}

pub fn is_identifier_continue(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '$'
}

fn collect_declarations(syntax: &SyntaxNode, candidates: &mut Vec<Candidate>) {
    for node in syntax.descendants() {
        let (kind, keyword) = match node.kind() {
            SyntaxKind::MODULE_DECLARATION => ("module", "module"),
            SyntaxKind::FUNCTION_DECLARATION => ("function", "function"),
            _ => continue,
        };
        let Some(name) = declaration_name(&node, keyword) else {
            continue;
        };
        let parameters = parameters(&node)
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
            .join(", ");
        candidates.push(Candidate {
            insert: name.clone(),
            label: format!("{name}({parameters})"),
            detail: kind.to_owned(),
            kind: CandidateKind::Declaration,
        });
    }
}

fn collect_assignments(syntax: &SyntaxNode, candidates: &mut Vec<Candidate>) {
    for node in syntax.children() {
        if node.kind() != SyntaxKind::ASSIGNMENT {
            continue;
        }
        let Some(name) = first_ident(&node) else {
            continue;
        };
        if candidates.iter().any(|candidate| {
            candidate.insert == name && candidate.kind <= CandidateKind::Assignment
        }) {
            continue;
        }
        candidates.push(Candidate {
            insert: name.clone(),
            label: name,
            detail: "assignment".to_owned(),
            kind: CandidateKind::Assignment,
        });
    }
}

fn collect_visible_parameters(node: &SyntaxNode, candidates: &mut Vec<Candidate>) {
    for ancestor in node.ancestors() {
        if !matches!(
            ancestor.kind(),
            SyntaxKind::MODULE_DECLARATION | SyntaxKind::FUNCTION_DECLARATION
        ) {
            continue;
        }
        for (name, default) in parameters(&ancestor) {
            candidates.push(Candidate {
                insert: name.clone(),
                label: match default {
                    Some(default) => format!("{name} = {default}"),
                    None => name,
                },
                detail: "parameter".to_owned(),
                kind: CandidateKind::Parameter,
            });
        }
        return;
    }
}

fn declaration_name(node: &SyntaxNode, keyword: &str) -> Option<String> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::IDENT)
        .filter(|token| token.text() != keyword)
        .map(|token| token.text().to_owned())
        .next()
}

fn parameters(node: &SyntaxNode) -> Vec<(String, Option<String>)> {
    let Some(param_list) = node
        .children()
        .find(|child| child.kind() == SyntaxKind::PARAM_LIST)
    else {
        return Vec::new();
    };
    param_list
        .children()
        .filter(|child| child.kind() == SyntaxKind::PARAM)
        .filter_map(|param| {
            let name = first_ident(&param)?;
            let default = param
                .children()
                .find(|child| child.kind() != SyntaxKind::PARAM)
                .map(|child| child.text().to_string())
                .map(|text| text.trim().to_owned())
                .filter(|text| !text.is_empty());
            Some((name, default))
        })
        .collect()
}

fn first_ident(node: &SyntaxNode) -> Option<String> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == SyntaxKind::IDENT)
        .map(|token| token.text().to_owned())
}

fn collect_builtins(candidates: &mut Vec<Candidate>) {
    for builtin in brepscad_eval::builtins::BUILTINS {
        candidates.push(Candidate {
            insert: builtin.name.to_owned(),
            label: builtin.name.to_owned(),
            detail: builtin.signature.to_owned(),
            kind: CandidateKind::Builtin,
        });
    }
}

fn collect_specials(candidates: &mut Vec<Candidate>) {
    for (name, detail) in brepscad_eval::builtins::SPECIAL_VARIABLES {
        candidates.push(Candidate {
            insert: (*name).to_owned(),
            label: (*name).to_owned(),
            detail: (*detail).to_owned(),
            kind: CandidateKind::Special,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inserts(source: &str, cursor: usize) -> Vec<String> {
        suggestions(source, cursor)
            .into_iter()
            .map(|candidate| candidate.insert)
            .collect()
    }

    #[test]
    fn declaration_names_are_collected() {
        let source = "module bracket(width = 4) {\n  cube(width);\n}\n";
        let found = inserts(source, source.len());
        assert!(found.contains(&"bracket".to_owned()));

        let inside = inserts(source, source.find("width").unwrap());
        assert!(inside.contains(&"width".to_owned()));
    }

    #[test]
    fn completion_is_suppressed_in_comments_and_strings() {
        let source = "// cube\ncube(\"sphere\");\n";
        let comment = source.find("cube").unwrap() + 4;
        assert!(suggestions(source, comment).is_empty());
        let string = source.find("sphere").unwrap() + 3;
        assert!(suggestions(source, string).is_empty());
    }

    #[test]
    fn prefix_filters_candidates() {
        let found = inserts("", 0);
        assert!(found.contains(&"cube".to_owned()));

        let prefixed = inserts("cu", 2);
        assert!(prefixed.iter().all(|name| name.starts_with("cu")));
        assert!(!prefixed.iter().any(|name| name == "sphere"));
    }

    #[test]
    fn visible_parameters_are_offered_inside_a_declaration() {
        let source = "module bracket(width = 4) {\n  cube(width);\n}\n";
        let cursor = source.find("cube(width)").unwrap();
        assert!(inserts(source, cursor).contains(&"width".to_owned()));
    }

    #[test]
    fn special_variables_are_offered() {
        let found = inserts("$fn", 3);
        assert_eq!(found, vec!["$fn".to_owned()]);
    }
}
