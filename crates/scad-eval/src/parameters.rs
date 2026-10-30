//! Numeric-literal and identifier discovery for editable parameters.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;

use brepscad_syntax::{NodeOrToken, SyntaxElement, SyntaxKind, SyntaxNode, TextRange};

use crate::EvalError;
use crate::model::Parameter;

pub(crate) fn first_ident(node: &SyntaxNode) -> Option<String> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == SyntaxKind::IDENT)
        .map(|token| token.text().to_owned())
}

pub(crate) fn parse_number(text: &str) -> Result<f64, EvalError> {
    text.parse::<f64>()
        .map_err(|_| EvalError::Parse(format!("invalid number: {text}")))
}

pub(crate) fn collect_parameters(
    node: &SyntaxNode,
    assignments: &HashMap<String, SyntaxNode>,
) -> Vec<Parameter> {
    let mut parameters = BTreeMap::new();
    collect_numbers(node, &mut parameters);

    let mut names = BTreeSet::new();
    collect_identifiers(node, &mut names);

    let mut visited = BTreeSet::new();
    for name in names {
        collect_assignment_numbers(&name, assignments, &mut parameters, &mut visited);
    }

    parameters.into_values().collect()
}

fn collect_numbers(node: &SyntaxNode, parameters: &mut BTreeMap<usize, Parameter>) {
    for element in node.descendants_with_tokens() {
        let Some(token) = element.as_token() else {
            continue;
        };
        if token.kind() != SyntaxKind::NUMBER {
            continue;
        }
        let Ok(value) = token.text().parse::<f64>() else {
            continue;
        };
        let range = text_range_to_range(token.text_range());
        parameters.entry(range.start).or_insert_with(|| Parameter {
            range,
            value,
            source: token.text().to_owned(),
        });
    }
}

fn collect_identifiers(node: &SyntaxNode, names: &mut BTreeSet<String>) {
    for element in node.children_with_tokens() {
        collect_element_identifiers(element, names);
    }
}

fn collect_element_identifiers(element: SyntaxElement, names: &mut BTreeSet<String>) {
    match element {
        NodeOrToken::Token(token) => {
            if token.kind() == SyntaxKind::IDENT {
                names.insert(token.text().to_owned());
            }
        }
        NodeOrToken::Node(node) => {
            if node.kind() == SyntaxKind::NAMED_ARG {
                let mut after_eq = false;
                for element in node.children_with_tokens() {
                    match element.kind() {
                        SyntaxKind::EQ => after_eq = true,
                        kind if kind.is_trivia() => {}
                        _ if after_eq => collect_element_identifiers(element, names),
                        _ => {}
                    }
                }
            } else {
                collect_identifiers(&node, names);
            }
        }
    }
}

fn collect_assignment_numbers(
    name: &str,
    assignments: &HashMap<String, SyntaxNode>,
    parameters: &mut BTreeMap<usize, Parameter>,
    visited: &mut BTreeSet<String>,
) {
    if !visited.insert(name.to_owned()) {
        return;
    }
    let Some(node) = assignments.get(name) else {
        return;
    };

    collect_numbers(node, parameters);

    let mut names = BTreeSet::new();
    collect_identifiers(node, &mut names);
    for nested in names {
        collect_assignment_numbers(&nested, assignments, parameters, visited);
    }
}

pub(crate) fn text_range_to_range(range: TextRange) -> Range<usize> {
    usize::from(range.start())..usize::from(range.end())
}
