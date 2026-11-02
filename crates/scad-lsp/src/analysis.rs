use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use brepscad_eval::builtins::{self, BUILTINS, BuiltinKind, SPECIAL_VARIABLES};
use brepscad_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, parse};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeclKind {
    Module,
    Function,
    Assignment,
    Parameter,
}

#[derive(Debug, Clone)]
pub struct Declaration {
    pub name: String,
    pub kind: DeclKind,
    pub range: Range<usize>,
    pub selection: Range<usize>,
    pub signature: String,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct RawDiagnostic {
    pub range: Range<usize>,
    pub severity: u8,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct CompletionItem {
    pub label: String,
    pub insert_text: String,
    pub detail: String,
    pub kind: u8,
}

#[derive(Debug, Clone)]
pub struct HoverInfo {
    pub contents: String,
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct Definition {
    pub path: Option<PathBuf>,
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: u8,
    pub range: Range<usize>,
}

pub fn declarations(source: &str) -> Vec<Declaration> {
    let parse = parse(source);
    let syntax = parse.syntax();
    let mut declarations = Vec::new();

    for node in syntax.descendants() {
        let (kind, keyword, detail) = match node.kind() {
            SyntaxKind::MODULE_DECLARATION => (DeclKind::Module, "module", "module"),
            SyntaxKind::FUNCTION_DECLARATION => (DeclKind::Function, "function", "function"),
            _ => continue,
        };
        let Some(name_token) = declaration_name(&node, keyword) else {
            continue;
        };
        let parameters = parameters(&node)
            .into_iter()
            .map(|parameter| parameter.name)
            .collect::<Vec<_>>()
            .join(", ");
        declarations.push(Declaration {
            name: name_token.text().to_owned(),
            kind,
            range: byte_range(node.text_range()),
            selection: byte_range(name_token.text_range()),
            signature: format!("{}({parameters})", name_token.text()),
            detail: detail.to_owned(),
        });
    }

    for node in syntax.children() {
        if node.kind() != SyntaxKind::ASSIGNMENT {
            continue;
        }
        let Some(name_token) = first_ident(&node) else {
            continue;
        };
        declarations.push(Declaration {
            name: name_token.text().to_owned(),
            kind: DeclKind::Assignment,
            range: byte_range(node.text_range()),
            selection: byte_range(name_token.text_range()),
            signature: name_token.text().to_owned(),
            detail: "assignment".to_owned(),
        });
    }

    declarations.sort_by_key(|declaration| declaration.range.start);
    declarations
}

pub fn diagnostics(source: &str) -> Vec<RawDiagnostic> {
    let parse = parse(source);
    let syntax = parse.syntax();
    let mut diagnostics = Vec::new();

    for error in parse.errors() {
        diagnostics.push(RawDiagnostic {
            range: error_range(source, error.offset),
            severity: 1,
            message: error.message.clone(),
        });
    }

    let declarations = declarations(source);
    let mut top_level: HashMap<(&str, DeclKind), Range<usize>> = HashMap::new();
    for declaration in &declarations {
        if !matches!(declaration.kind, DeclKind::Module | DeclKind::Function) {
            continue;
        }
        if top_level
            .insert(
                (&declaration.name, declaration.kind),
                declaration.selection.clone(),
            )
            .is_some()
        {
            diagnostics.push(RawDiagnostic {
                range: declaration.selection.clone(),
                severity: 2,
                message: format!("duplicate declaration: {}", declaration.name),
            });
        }
    }

    let modules: Vec<&str> = declarations
        .iter()
        .filter(|declaration| declaration.kind == DeclKind::Module)
        .map(|declaration| declaration.name.as_str())
        .collect();
    let functions: Vec<&str> = declarations
        .iter()
        .filter(|declaration| declaration.kind == DeclKind::Function)
        .map(|declaration| declaration.name.as_str())
        .collect();

    for node in syntax.descendants() {
        let kind = node.kind();
        if !matches!(kind, SyntaxKind::MODULE_CALL | SyntaxKind::CALL_EXPR) {
            continue;
        }
        let Some(name) = first_ident(&node) else {
            continue;
        };
        let known = match kind {
            SyntaxKind::MODULE_CALL => {
                modules.contains(&name.text())
                    || builtins::lookup(name.text())
                        .is_some_and(|builtin| builtin.kind == BuiltinKind::Module)
            }
            _ => {
                functions.contains(&name.text())
                    || builtins::lookup(name.text())
                        .is_some_and(|builtin| builtin.kind == BuiltinKind::Function)
            }
        };
        if !known {
            let what = if kind == SyntaxKind::MODULE_CALL {
                "module"
            } else {
                "function"
            };
            diagnostics.push(RawDiagnostic {
                range: byte_range(name.text_range()),
                severity: 2,
                message: format!("unknown {what}: {}", name.text()),
            });
        }
    }

    diagnostics.sort_by_key(|diagnostic| (diagnostic.range.start, diagnostic.range.end));
    diagnostics.dedup_by(|a, b| a.range == b.range && a.message == b.message);
    diagnostics
}

pub fn completion(source: &str, offset: usize) -> Vec<CompletionItem> {
    if in_comment_or_string(source, offset) {
        return Vec::new();
    }

    let prefix = identifier_prefix(source, offset);
    let mut items = Vec::new();
    for declaration in declarations(source) {
        items.push(CompletionItem {
            label: declaration.signature.clone(),
            insert_text: declaration.name.clone(),
            detail: declaration.detail.clone(),
            kind: match declaration.kind {
                DeclKind::Module => 9,
                DeclKind::Function => 3,
                DeclKind::Assignment => 6,
                DeclKind::Parameter => 6,
            },
        });
    }
    for parameter in visible_parameters(source, offset) {
        items.push(CompletionItem {
            label: parameter.signature,
            insert_text: parameter.name,
            detail: "parameter".to_owned(),
            kind: 6,
        });
    }
    for builtin in BUILTINS {
        items.push(CompletionItem {
            label: builtin.name.to_owned(),
            insert_text: builtin.name.to_owned(),
            detail: builtin.signature.to_owned(),
            kind: match builtin.kind {
                BuiltinKind::Module => 9,
                BuiltinKind::Function => 3,
            },
        });
    }
    for (name, documentation) in SPECIAL_VARIABLES {
        items.push(CompletionItem {
            label: (*name).to_owned(),
            insert_text: (*name).to_owned(),
            detail: (*documentation).to_owned(),
            kind: 6,
        });
    }

    items.retain(|item| item.insert_text.starts_with(&prefix));
    items.sort_by(|a, b| (a.kind, &a.insert_text).cmp(&(b.kind, &b.insert_text)));
    items.dedup_by(|a, b| a.insert_text == b.insert_text);
    items
}

pub fn hover(source: &str, offset: usize) -> Option<HoverInfo> {
    let (word, range) = word_at(source, offset)?;
    if let Some(builtin) = builtins::lookup(&word) {
        return Some(HoverInfo {
            contents: format!(
                "```openscad\n{}\n```\n\n{}",
                builtin.signature, builtin.documentation
            ),
            range,
        });
    }
    if let Some((_, documentation)) = SPECIAL_VARIABLES.iter().find(|(name, _)| *name == word) {
        return Some(HoverInfo {
            contents: format!("`{word}`\n\n{documentation}"),
            range,
        });
    }
    if let Some(declaration) = declarations(source)
        .into_iter()
        .find(|declaration| declaration.name == word)
    {
        return Some(HoverInfo {
            contents: format!(
                "```openscad\n{}\n```\n\n{}",
                declaration.signature, declaration.detail
            ),
            range,
        });
    }
    None
}

pub fn definition(source: &str, current_path: Option<&Path>, offset: usize) -> Option<Definition> {
    if let Some(path) = include_target(source, current_path, offset) {
        return Some(Definition {
            path: Some(path),
            range: 0..0,
        });
    }

    let (word, _) = word_at(source, offset)?;
    if let Some(declaration) = declarations(source)
        .into_iter()
        .find(|declaration| declaration.name == word)
    {
        return Some(Definition {
            path: None,
            range: declaration.selection,
        });
    }
    parameter_at(source, offset, &word).map(|range| Definition { path: None, range })
}

pub fn symbols(source: &str) -> Vec<DocumentSymbol> {
    declarations(source)
        .into_iter()
        .map(|declaration| DocumentSymbol {
            name: declaration.name,
            kind: match declaration.kind {
                DeclKind::Module => 2,
                DeclKind::Function => 12,
                DeclKind::Assignment => 13,
                DeclKind::Parameter => 13,
            },
            range: declaration.range,
        })
        .collect()
}

fn visible_parameters(source: &str, offset: usize) -> Vec<Declaration> {
    let parse = parse(source);
    let syntax = parse.syntax();
    let Some(token) = token_at(&syntax, offset) else {
        return Vec::new();
    };
    let Some(node) = token.parent() else {
        return Vec::new();
    };

    for ancestor in node.ancestors() {
        if !matches!(
            ancestor.kind(),
            SyntaxKind::MODULE_DECLARATION | SyntaxKind::FUNCTION_DECLARATION
        ) {
            continue;
        }
        return parameters(&ancestor)
            .into_iter()
            .map(|parameter| Declaration {
                name: parameter.name.clone(),
                kind: DeclKind::Parameter,
                range: parameter.range,
                selection: parameter.selection,
                signature: match parameter.default {
                    Some(default) => format!("{} = {default}", parameter.name),
                    None => parameter.name,
                },
                detail: "parameter".to_owned(),
            })
            .collect();
    }
    Vec::new()
}

fn parameter_at(source: &str, offset: usize, word: &str) -> Option<Range<usize>> {
    let parse = parse(source);
    let syntax = parse.syntax();
    let token = token_at(&syntax, offset)?;
    let node = token.parent()?;
    for ancestor in node.ancestors() {
        if !matches!(
            ancestor.kind(),
            SyntaxKind::MODULE_DECLARATION | SyntaxKind::FUNCTION_DECLARATION
        ) {
            continue;
        }
        return parameters(&ancestor)
            .into_iter()
            .find(|parameter| parameter.name == word)
            .map(|parameter| parameter.selection);
    }
    None
}

#[derive(Debug, Clone)]
struct ParameterInfo {
    name: String,
    default: Option<String>,
    range: Range<usize>,
    selection: Range<usize>,
}

fn parameters(node: &SyntaxNode) -> Vec<ParameterInfo> {
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
            let name_token = first_ident(&param)?;
            let default = param
                .children()
                .find(|child| child.kind() != SyntaxKind::PARAM)
                .map(|child| child.text().to_string().trim().to_owned())
                .filter(|text| !text.is_empty());
            Some(ParameterInfo {
                name: name_token.text().to_owned(),
                default,
                range: byte_range(param.text_range()),
                selection: byte_range(name_token.text_range()),
            })
        })
        .collect()
}

fn declaration_name(node: &SyntaxNode, keyword: &str) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::IDENT)
        .find(|token| token.text() != keyword)
}

fn first_ident(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == SyntaxKind::IDENT)
}

fn token_at(syntax: &SyntaxNode, offset: usize) -> Option<SyntaxToken> {
    let mut last = None;
    for element in syntax.descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };
        let range = token.text_range();
        let start = usize::from(range.start());
        let end = usize::from(range.end());
        if start <= offset && offset < end {
            return Some(token);
        }
        last = Some(token);
    }
    if offset >= usize::from(syntax.text_range().end()) {
        last
    } else {
        None
    }
}

fn in_comment_or_string(source: &str, offset: usize) -> bool {
    let parse = parse(source);
    let syntax = parse.syntax();
    let Some(token) = token_at(&syntax, offset) else {
        return false;
    };
    let end = usize::from(token.text_range().end());
    match token.kind() {
        SyntaxKind::LINE_COMMENT => true,
        SyntaxKind::BLOCK_COMMENT | SyntaxKind::STRING => offset < end,
        _ => false,
    }
}

fn word_at(source: &str, offset: usize) -> Option<(String, Range<usize>)> {
    let offset = offset.min(source.len());
    if !source.is_char_boundary(offset) {
        return None;
    }
    let mut start = offset;
    while start > 0 {
        let previous = source[..start].chars().next_back()?;
        if !is_identifier_continue(previous) {
            break;
        }
        start -= previous.len_utf8();
    }
    let mut end = offset;
    while end < source.len() {
        let next = source[end..].chars().next()?;
        if !is_identifier_continue(next) {
            break;
        }
        end += next.len_utf8();
    }
    (start < end).then(|| (source[start..end].to_owned(), start..end))
}

fn identifier_prefix(source: &str, offset: usize) -> String {
    let offset = offset.min(source.len());
    let mut start = offset;
    while start > 0 {
        let Some(previous) = source[..start].chars().next_back() else {
            break;
        };
        if !is_identifier_continue(previous) {
            break;
        }
        start -= previous.len_utf8();
    }
    source[start..offset].to_owned()
}

fn is_identifier_continue(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '$'
}

fn include_target(source: &str, current_path: Option<&Path>, offset: usize) -> Option<PathBuf> {
    let current_path = current_path?;
    let parse = parse(source);
    let syntax = parse.syntax();
    let token = token_at(&syntax, offset)?;
    if token.kind() != SyntaxKind::STRING {
        return None;
    }
    let parent = token.parent()?;
    if !matches!(parent.kind(), SyntaxKind::INCLUDE | SyntaxKind::USE) {
        return None;
    }
    let request = token.text().trim_matches('"');
    let candidate = current_path.parent()?.join(request);
    candidate.exists().then_some(candidate)
}

fn byte_range(range: brepscad_syntax::TextRange) -> Range<usize> {
    usize::from(range.start())..usize::from(range.end())
}

fn error_range(source: &str, offset: usize) -> Range<usize> {
    let len = source.len();
    let mut start = offset.min(len);
    while start > 0 && !source.is_char_boundary(start) {
        start -= 1;
    }
    if start < len {
        let end = source[start..]
            .chars()
            .next()
            .map_or(start, |character| start + character.len_utf8());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declarations_include_modules_functions_and_assignments() {
        let source = "size = 2;\nmodule bracket(w) { cube(w); }\nfunction double(x) = x * 2;\n";
        let declarations = declarations(source);
        assert!(declarations.iter().any(|d| d.name == "size"));
        assert!(declarations.iter().any(|d| d.name == "bracket"));
        assert!(declarations.iter().any(|d| d.name == "double"));
    }

    #[test]
    fn diagnostics_find_parse_and_unknown_call_errors() {
        let parse_errors = diagnostics("cube(");
        assert!(parse_errors.iter().any(|d| d.severity == 1));

        let unknown = diagnostics("cube(missing(1));\n");
        assert!(
            unknown
                .iter()
                .any(|d| d.message.contains("unknown function"))
        );
    }

    #[test]
    fn completion_includes_user_declarations_and_builtins() {
        let source = "module bracket(w) { cube(w); }\n";
        let items = completion(source, source.len());
        assert!(items.iter().any(|item| item.insert_text == "bracket"));
        assert!(items.iter().any(|item| item.insert_text == "cube"));
    }

    #[test]
    fn hover_and_definition_find_builtins_and_declarations() {
        let source = "module bracket(w) { cube(w); }\n";
        let hover = hover(source, source.find("cube").unwrap() + 1).expect("hover");
        assert!(hover.contents.contains("cube"));

        let definition =
            definition(source, None, source.find("cube(w)").unwrap() + 5).expect("definition");
        assert!(definition.range.start < definition.range.end);
    }

    #[test]
    fn include_strings_resolve_to_files() {
        let directory =
            std::env::temp_dir().join(format!("brepscad-lsp-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let included = directory.join("part.scad");
        std::fs::write(&included, "cube(1);").unwrap();
        let main = directory.join("main.scad");
        let source = "include \"part.scad\";\n";
        let definition =
            definition(source, Some(&main), source.find("part").unwrap()).expect("include");
        assert_eq!(definition.path.as_deref(), Some(included.as_path()));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
