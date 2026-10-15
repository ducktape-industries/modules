use crate::view::label;

#[test]
fn a_row_is_named_by_its_program_its_outcome_and_its_place() {
    let named = |applied| label("chat", applied, "1,234", 3);
    assert_eq!(named(true), "chat, Accepted, block 1,234 · 3");
    assert_eq!(named(false), "chat, Rejected, block 1,234 · 3");
}
