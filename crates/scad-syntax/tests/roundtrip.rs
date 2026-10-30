use visual_openscad_scad_syntax::{
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
