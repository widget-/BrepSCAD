use brepscad_syntax::{
    EditError, Parse, SyntaxKind, SyntaxToken, TextEdit, TextRange, TextSize, apply_edit, parse,
};

fn first_number(parsed: &Parse) -> SyntaxToken {
    parsed
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == SyntaxKind::NUMBER)
        .expect("expected a number token")
}

#[test]
fn round_trips_comments_and_whitespace() {
    let source = "// leading\nsize = [ 10, 20, 30 ]; /* middle */\n\ncube( size ); // trailing\n";
    let parsed = parse(source);

    assert!(parsed.errors().is_empty(), "{:?}", parsed.errors());
    assert_eq!(parsed.text(), source);
}

#[test]
fn replaces_first_number_without_disturbing_the_rest() {
    let source = "cube([10, 20, 30]); // keep\n";
    let parsed = parse(source);
    assert!(parsed.errors().is_empty(), "{:?}", parsed.errors());

    let number = first_number(&parsed);
    let edit = TextEdit::new(number.text_range(), "12");
    let edited = apply_edit(source, &edit).expect("valid edit");

    assert_eq!(edited, "cube([12, 20, 30]); // keep\n");

    let reparsed = parse(&edited);
    assert!(reparsed.errors().is_empty(), "{:?}", reparsed.errors());
    assert_eq!(reparsed.text(), edited);
}

#[test]
fn invalid_input_still_produces_a_lossless_tree() {
    let source = "cube([10, 20;\n// missing close\n";
    let parsed = parse(source);

    assert!(!parsed.errors().is_empty());
    assert_eq!(parsed.text(), source);
}

#[test]
fn parses_assignment_and_module_call() {
    let source = "size = [10, 20, 30];\ncube(size);\n";
    let parsed = parse(source);

    assert!(parsed.errors().is_empty(), "{:?}", parsed.errors());
    assert_eq!(parsed.text(), source);

    let assignment = parsed
        .syntax()
        .descendants()
        .find(|node| node.kind() == SyntaxKind::ASSIGNMENT)
        .expect("assignment node");
    assert_eq!(assignment.text().to_string(), "size = [10, 20, 30];");
}

#[test]
fn round_trips_boolean_block() {
    let source = "difference() {\n    cube([2, 2, 2]);\n    cylinder(0.5, 4);\n}\n";
    let parsed = parse(source);

    assert!(parsed.errors().is_empty(), "{:?}", parsed.errors());
    assert_eq!(parsed.text(), source);
    assert!(
        parsed
            .syntax()
            .descendants()
            .any(|node| node.kind() == SyntaxKind::BLOCK)
    );
}

#[test]
fn round_trips_transform_with_single_child() {
    let source = "translate([1, 0, 0]) cube([2, 2, 2]);\n";
    let parsed = parse(source);

    assert!(parsed.errors().is_empty(), "{:?}", parsed.errors());
    assert_eq!(parsed.text(), source);
}

#[test]
fn round_trips_named_arguments() {
    let source = "cube(size = [2, 3, 4]);\ncylinder(r = 0.5, h = 4);\ntranslate(v = [1, 0, 0]) cube(size = 2);\n";
    let parsed = parse(source);

    assert!(parsed.errors().is_empty(), "{:?}", parsed.errors());
    assert_eq!(parsed.text(), source);
    assert_eq!(
        parsed
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::NAMED_ARG)
            .count(),
        5
    );
}

#[test]
fn parses_unary_negative_expressions() {
    let source = "translate([-1, 0, -2.5]) cube([2, 2, 2]);\n";
    let parsed = parse(source);

    assert!(parsed.errors().is_empty(), "{:?}", parsed.errors());
    assert_eq!(parsed.text(), source);
    assert_eq!(
        parsed
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::UNARY_EXPR)
            .count(),
        2
    );
    assert!(
        parsed
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .any(|token| token.kind() == SyntaxKind::MINUS)
    );
}

#[test]
fn parses_arithmetic_with_precedence_and_groups() {
    let source = "x = 1 + 2 * 3; y = (1 + 2) * 3;\n";
    let parsed = parse(source);

    assert!(parsed.errors().is_empty(), "{:?}", parsed.errors());
    assert_eq!(parsed.text(), source);

    let assignment = parsed
        .syntax()
        .descendants()
        .find(|node| {
            node.kind() == SyntaxKind::ASSIGNMENT && node.text().to_string().starts_with('x')
        })
        .expect("x assignment");
    let sum = assignment
        .children()
        .find(|node| node.kind() == SyntaxKind::BINARY_EXPR)
        .expect("sum");
    assert_eq!(sum.text().to_string().trim(), "1 + 2 * 3");
    let product = sum
        .children()
        .find(|node| node.kind() == SyntaxKind::BINARY_EXPR)
        .expect("product");
    assert_eq!(product.text().to_string().trim(), "2 * 3");

    let assignment = parsed
        .syntax()
        .descendants()
        .find(|node| {
            node.kind() == SyntaxKind::ASSIGNMENT && node.text().to_string().starts_with('y')
        })
        .expect("y assignment");
    let product = assignment
        .children()
        .find(|node| node.kind() == SyntaxKind::BINARY_EXPR)
        .expect("product");
    assert_eq!(product.text().to_string().trim(), "(1 + 2) * 3");
    let group = product
        .children()
        .find(|node| node.kind() == SyntaxKind::GROUP)
        .expect("group");
    assert_eq!(group.text().to_string().trim(), "(1 + 2)");
}

#[test]
fn round_trips_for_ranges_and_if_else() {
    let source = "for (i = [0:2:6]) {\n    if (i >= 2 && !false) cube(i);\n    else cube(1);\n}\n";
    let parsed = parse(source);

    assert!(parsed.errors().is_empty(), "{:?}", parsed.errors());
    assert_eq!(parsed.text(), source);
    assert_eq!(
        parsed
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::RANGE)
            .count(),
        1
    );
    assert_eq!(
        parsed
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::ELSE)
            .count(),
        1
    );
    assert!(
        parsed
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .any(|token| token.kind() == SyntaxKind::FALSE)
    );
}

#[test]
fn parses_comparison_and_logical_precedence() {
    let source = "x = 1 + 2 < 4 && true || false;\n";
    let parsed = parse(source);

    assert!(parsed.errors().is_empty(), "{:?}", parsed.errors());
    assert_eq!(parsed.text(), source);

    let assignment = parsed
        .syntax()
        .descendants()
        .find(|node| node.kind() == SyntaxKind::ASSIGNMENT)
        .expect("assignment");
    let expression = assignment
        .children()
        .find(|node| node.kind() == SyntaxKind::BINARY_EXPR)
        .expect("expression");
    assert_eq!(
        expression.text().to_string().trim(),
        "1 + 2 < 4 && true || false"
    );
    assert!(
        parsed
            .syntax()
            .descendants()
            .any(|node| node.kind() == SyntaxKind::BINARY_EXPR)
    );
}

#[test]
fn rejects_out_of_bounds_edits() {
    let edit = TextEdit::new(
        TextRange::new(TextSize::from(100u32), TextSize::from(101u32)),
        "x",
    );

    assert!(matches!(
        apply_edit("cube();", &edit),
        Err(EditError::OutOfBounds { .. })
    ));
}
