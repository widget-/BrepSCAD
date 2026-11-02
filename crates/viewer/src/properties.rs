use std::ops::Range;

use brepscad_syntax::{NodeOrToken, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, parse};

#[derive(Debug, Clone, PartialEq)]
pub struct PropertyRow {
    pub name: String,
    pub value: Option<f64>,
    /// Byte range to replace when editing the numeric value.
    pub range: Range<usize>,
    /// Byte range to jump to when the name is clicked.
    pub target: Range<usize>,
    pub source: String,
}

#[derive(Debug, Clone, Default)]
pub struct CallProperties {
    pub rows: Vec<PropertyRow>,
}

pub fn call_properties(source: &str, module: &str, source_range: &Range<usize>) -> CallProperties {
    let parse = parse(source);
    let syntax = parse.syntax();
    let Some(call) = find_call(&syntax, source_range) else {
        return CallProperties { rows: Vec::new() };
    };

    let mut rows = Vec::new();

    if let Some(arg_list) = call
        .children()
        .find(|child| child.kind() == SyntaxKind::ARG_LIST)
    {
        let mut positional = 0;
        for element in arg_list.children_with_tokens() {
            if let NodeOrToken::Node(node) = &element
                && node.kind() == SyntaxKind::NAMED_ARG
            {
                collect_named_argument(module, node, &mut rows);
                continue;
            }
            if !is_argument(&element) {
                continue;
            }
            let name = positional_names(module)
                .get(positional)
                .copied()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("arg{}", positional + 1));
            collect_argument(module, &name, None, element, &mut rows);
            positional += 1;
        }
    }

    rows.sort_by_key(|row| row.range.start);
    CallProperties { rows }
}

fn collect_named_argument(module: &str, node: &SyntaxNode, rows: &mut Vec<PropertyRow>) {
    let Some(name_token) = node
        .children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == SyntaxKind::IDENT)
    else {
        return;
    };
    let Some(expression) = expression_after_eq(node) else {
        return;
    };
    let name = name_token.text().to_owned();
    collect_argument(
        module,
        &name,
        Some(byte_range(name_token.text_range())),
        expression,
        rows,
    );
}

fn collect_argument(
    module: &str,
    name: &str,
    name_range: Option<Range<usize>>,
    element: SyntaxElement,
    rows: &mut Vec<PropertyRow>,
) {
    match element {
        NodeOrToken::Token(token) => {
            let range = byte_range(token.text_range());
            match token.kind() {
                SyntaxKind::NUMBER => {
                    if let Ok(value) = token.text().parse::<f64>() {
                        rows.push(PropertyRow {
                            name: name.to_owned(),
                            value: Some(value),
                            range: range.clone(),
                            target: name_range.unwrap_or(range),
                            source: token.text().to_owned(),
                        });
                    }
                }
                SyntaxKind::IDENT | SyntaxKind::STRING => rows.push(PropertyRow {
                    name: name.to_owned(),
                    value: None,
                    range: range.clone(),
                    target: name_range.unwrap_or(range),
                    source: token.text().to_owned(),
                }),
                _ => {}
            }
        }
        NodeOrToken::Node(node) => {
            let numbers = numeric_tokens(&node);
            if numbers.is_empty() {
                return;
            }
            if node.kind() == SyntaxKind::VECTOR {
                for (index, token) in numbers.iter().enumerate() {
                    let component = vector_component_name(module, name, index);
                    let range = byte_range(token.text_range());
                    rows.push(PropertyRow {
                        name: component,
                        value: token.text().parse().ok(),
                        range: range.clone(),
                        target: range,
                        source: token.text().to_owned(),
                    });
                }
            } else if numbers.len() == 1 {
                let token = &numbers[0];
                let range = byte_range(token.text_range());
                rows.push(PropertyRow {
                    name: name.to_owned(),
                    value: token.text().parse().ok(),
                    range: range.clone(),
                    target: name_range.unwrap_or(range),
                    source: token.text().to_owned(),
                });
            } else {
                for (index, token) in numbers.iter().enumerate() {
                    let range = byte_range(token.text_range());
                    rows.push(PropertyRow {
                        name: format!("{name}[{index}]"),
                        value: token.text().parse().ok(),
                        range: range.clone(),
                        target: range,
                        source: token.text().to_owned(),
                    });
                }
            }
        }
    }
}

fn numeric_tokens(node: &SyntaxNode) -> Vec<SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::NUMBER)
        .filter(|token| token.text().parse::<f64>().is_ok())
        .collect()
}

fn is_argument(element: &SyntaxElement) -> bool {
    match element {
        NodeOrToken::Node(_) => true,
        NodeOrToken::Token(token) => {
            !token.kind().is_trivia()
                && !matches!(
                    token.kind(),
                    SyntaxKind::L_PAREN | SyntaxKind::R_PAREN | SyntaxKind::COMMA | SyntaxKind::EQ
                )
        }
    }
}

fn expression_after_eq(node: &SyntaxNode) -> Option<SyntaxElement> {
    let mut after_eq = false;
    for element in node.children_with_tokens() {
        match element.kind() {
            SyntaxKind::EQ => after_eq = true,
            kind if kind.is_trivia() => {}
            _ if after_eq => return Some(element),
            _ => {}
        }
    }
    None
}

fn find_call(syntax: &SyntaxNode, source_range: &Range<usize>) -> Option<SyntaxNode> {
    syntax
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::MODULE_CALL)
        .find(|node| usize::from(node.text_range().start()) == source_range.start)
        .or_else(|| {
            syntax
                .descendants()
                .filter(|node| node.kind() == SyntaxKind::MODULE_CALL)
                .find(|node| {
                    let range = node.text_range();
                    usize::from(range.start()) <= source_range.start
                        && source_range.start < usize::from(range.end())
                })
        })
}

fn byte_range(range: brepscad_syntax::TextRange) -> Range<usize> {
    usize::from(range.start())..usize::from(range.end())
}

fn positional_names(module: &str) -> &'static [&'static str] {
    match module {
        "cube" | "square" => &["size", "center"],
        "cylinder" => &["h", "r", "r2", "center"],
        "sphere" | "circle" => &["r"],
        "polygon" => &["points"],
        "polyhedron" => &["points", "faces"],
        "translate" | "scale" | "mirror" => &["v"],
        "rotate" => &["a"],
        "resize" => &["newsize"],
        "linear_extrude" => &["height", "center", "twist", "slices", "scale"],
        "rotate_extrude" => &["angle"],
        "offset" => &["r", "delta"],
        "color" => &["c"],
        "multmatrix" => &["m"],
        _ => &[],
    }
}

fn vector_component_name(module: &str, base: &str, index: usize) -> String {
    if matches!(module, "translate" | "scale" | "mirror" | "rotate") && matches!(base, "v" | "a") {
        return match index {
            0 => "x",
            1 => "y",
            2 => "z",
            _ => "w",
        }
        .to_owned();
    }
    format!("{base}[{index}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cylinder_named_parameters_are_named() {
        let source = "translate([0.2, 0, 0]) cylinder(h=4, r=0.5);\n";
        let start = source.find("cylinder").unwrap();
        let rows = call_properties(source, "cylinder", &(start..source.len())).rows;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "h");
        assert_eq!(rows[0].value, Some(4.0));
        assert_eq!(rows[1].name, "r");
        assert_eq!(rows[1].value, Some(0.5));
        assert_eq!(&source[rows[0].target.clone()], "h");
    }

    #[test]
    fn translate_vector_components_are_named() {
        let source = "translate([0.2, 0, 0]) cylinder(h=4, r=0.5);\n";
        let rows = call_properties(source, "translate", &(0..source.len())).rows;
        let names: Vec<_> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["x", "y", "z"]);
        assert_eq!(rows[0].value, Some(0.2));
    }

    #[test]
    fn positional_cylinder_parameters_use_the_signature_names() {
        let source = "cylinder(4, 0.5);\n";
        let rows = call_properties(source, "cylinder", &(0..source.len())).rows;
        let names: Vec<_> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["h", "r"]);
    }

    #[test]
    fn identifier_arguments_are_clickable_but_not_numeric() {
        let source = "cube(size);\n";
        let rows = call_properties(source, "cube", &(0..source.len())).rows;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "size");
        assert_eq!(rows[0].value, None);
        assert_eq!(&source[rows[0].target.clone()], "size");
    }
}
