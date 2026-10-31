use brepscad_syntax::{Parse, SyntaxKind, parse};

fn roundtrip(source: &str) -> Parse {
    let parsed = parse(source);
    assert!(parsed.errors().is_empty(), "errors: {:?}", parsed.errors());
    assert_eq!(parsed.text(), source);
    parsed
}

fn count_nodes(parsed: &Parse, kind: SyntaxKind) -> usize {
    parsed
        .syntax()
        .descendants()
        .filter(|node| node.kind() == kind)
        .count()
}

fn tokens(parsed: &Parse, kind: SyntaxKind) -> Vec<String> {
    parsed
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == kind)
        .map(|token| token.text().to_owned())
        .collect()
}

#[test]
fn round_trips_exponentiation() {
    let parsed = roundtrip("x = 2 ^ 3 ^ 2;\n");

    assert_eq!(count_nodes(&parsed, SyntaxKind::BINARY_EXPR), 2);
    assert_eq!(tokens(&parsed, SyntaxKind::CARET), ["^", "^"]);
}

#[test]
fn round_trips_digit_leading_identifiers() {
    let parsed = roundtrip(
        "module 12ptStar(size) cube(size);\n\
         function 8bit_polyfont(x) = x;\n\
         12ptStar(8bit_polyfont(2));\n",
    );

    let identifiers = tokens(&parsed, SyntaxKind::IDENT);
    assert!(identifiers.iter().any(|name| name == "12ptStar"));
    assert!(identifiers.iter().any(|name| name == "8bit_polyfont"));
}

#[test]
fn round_trips_trailing_commas() {
    let parsed = roundtrip(
        "v = [1, 2, 3,];\n\
         cube([1, 2, 3,]);\n\
         function pair(a, b,) = [a, b,];\n\
         pair(1, 2,);\n",
    );

    assert_eq!(count_nodes(&parsed, SyntaxKind::VECTOR), 3);
}

#[test]
fn round_trips_each_and_undef() {
    let parsed = roundtrip("v = [each [1, 2], each [3, 4], undef];\n");

    assert_eq!(count_nodes(&parsed, SyntaxKind::EACH_EXPR), 2);
    assert_eq!(tokens(&parsed, SyntaxKind::UNDEF), ["undef"]);
}

#[test]
fn round_trips_echo_and_assert_prefixes() {
    let parsed = roundtrip("x = cond ? echo(\"warn\") true : assert(false, \"bad\") false;\n");

    assert_eq!(count_nodes(&parsed, SyntaxKind::ECHO_EXPR), 1);
    assert_eq!(count_nodes(&parsed, SyntaxKind::ASSERT_EXPR), 1);
}

#[test]
fn round_trips_statement_modifiers() {
    let parsed = roundtrip(
        "*cube(1);\n\
         #cube(2);\n\
         %cube(3);\n\
         !cube(4);\n",
    );

    assert_eq!(count_nodes(&parsed, SyntaxKind::MODIFIER), 4);
}

#[test]
fn round_trips_dot_notation() {
    let parsed = roundtrip("v = [1, 2, 3];\nx = v.x + v.y + v.z + v.w;\n");

    assert_eq!(count_nodes(&parsed, SyntaxKind::DOT_EXPR), 4);
}

#[test]
fn round_trips_c_style_for_comprehensions() {
    let parsed = roundtrip("v = [for (i = 0, j = 10; i < 3; i = i + 1, j = j + 2) i + j];\n");

    assert_eq!(count_nodes(&parsed, SyntaxKind::C_FOR_EXPR), 1);
}

#[test]
fn round_trips_empty_statements() {
    roundtrip("; cube(1); ;\n");
}

#[test]
fn round_trips_number_forms() {
    let parsed = roundtrip("x = .5 + 1. + 1e-3 + 1E3;\n");
    let numbers = tokens(&parsed, SyntaxKind::NUMBER);

    assert_eq!(numbers, [".5", "1.", "1e-3", "1E3"]);
}

#[test]
fn round_trips_function_literals_and_calls() {
    let parsed = roundtrip(
        "f = function(x, y = 2) x + y;\n\
         g = function(x) function(y) x + y;\n\
         z = f(1)(2);\n",
    );

    assert_eq!(count_nodes(&parsed, SyntaxKind::FUNCTION_LITERAL), 3);
    assert_eq!(count_nodes(&parsed, SyntaxKind::INVOKE_EXPR), 1);
}

#[test]
fn numeric_prefix_without_exponent_stays_an_identifier() {
    let parsed = roundtrip("x = 3e;\ny = 3e2;\n");
    let identifiers = tokens(&parsed, SyntaxKind::IDENT);

    assert!(identifiers.iter().any(|name| name == "3e"));
    assert!(
        tokens(&parsed, SyntaxKind::NUMBER)
            .iter()
            .any(|n| n == "3e2")
    );
}
