//! Test blocks: `describe`, `it`, `test`, and `suite` calls of Jest, Vitest, Mocha, and node:test, listed by their title, top-level ones and, one level deeper each, those in a test block's callback.

use tree_sitter::Node;

use super::{Statement, statement_line};
use crate::code_outline::OutlineLevel;
use crate::outline::OutlineLine;
use crate::signature::Signature;
use crate::syntax_tree::{first_syntax_child, source};

/// Functions that open a test block.
const TEST_FUNCTIONS: [&str; 4] = ["describe", "it", "test", "suite"];

/// Properties that select or skip test blocks: `describe.only`, `it.skip`, `test.each`.
const TEST_MODIFIERS: [&str; 3] = ["only", "skip", "each"];

/// The outline lines of a test block: its own line, then the test blocks in its callback's body
/// one level deeper (the `it` calls in a `describe`), down to the deepest level outlines list.
/// Nothing for any other call.
pub(super) fn test_block_lines(
    statement: Statement<'_>,
    call: Node<'_>,
    level: &OutlineLevel<'_>,
) -> Vec<OutlineLine> {
    let Some(line) = test_block_line(statement, call, level) else {
        return Vec::new();
    };
    let mut lines = vec![line];
    if let Some(body) = callback_body(call) {
        level.declarations_inside(body, &mut lines);
    }
    lines
}

/// The body of a test block's callback: its last argument when that is a function with a block
/// body (`() => { … }`, `function () { … }`).
fn callback_body(call: Node<'_>) -> Option<Node<'_>> {
    let arguments = call.child_by_field_name("arguments")?;
    let mut cursor = arguments.walk();
    let callback = arguments
        .named_children(&mut cursor)
        .filter(|argument| !argument.is_extra())
        .last()?;
    if !matches!(
        callback.kind(),
        "arrow_function" | "function_expression" | "function"
    ) {
        return None;
    }
    callback
        .child_by_field_name("body")
        .filter(|body| body.kind() == "statement_block")
}

/// The outline line of a test block, such as `describe('parser')` or `test.each('adds %i')`, or
/// `None` for any other call.
///
/// The title is the first argument and must be a string or template literal. A table test
/// (`test.each(table)('title', fn)`, or with a tagged template table) prints its callee and
/// title without the table.
fn test_block_line(
    statement: Statement<'_>,
    call: Node<'_>,
    level: &OutlineLevel<'_>,
) -> Option<OutlineLine> {
    let function = call.child_by_field_name("function")?;
    let callee = match function.kind() {
        "call_expression" => function
            .child_by_field_name("function")
            .filter(|inner| is_test_callee(*inner, level) && ends_with_each(*inner, level))?,
        _ if is_test_callee(function, level) && !ends_with_each(function, level) => function,
        _ => return None,
    };
    let title = first_syntax_child(call.child_by_field_name("arguments")?)
        .filter(|title| matches!(title.kind(), "string" | "template_string"))?;
    let signature = Signature {
        text: format!("{}({})", level.one_line(callee), level.one_line(title)),
        highlight_end: title.end_byte(),
    };
    Some(statement_line(level, statement, "", signature))
}

/// Whether the node names a test function, possibly with modifiers: `it`, `describe.skip`.
///
/// Walks down the chain of modifiers in a loop rather than by recursion, so a chain of
/// thousands (`describe.only.only…`) can't overflow the stack.
fn is_test_callee(mut node: Node<'_>, level: &OutlineLevel<'_>) -> bool {
    loop {
        match node.kind() {
            "identifier" => return TEST_FUNCTIONS.contains(&source(node, level.text())),
            "member_expression" => {
                let has_modifier = node
                    .child_by_field_name("property")
                    .is_some_and(|property| {
                        TEST_MODIFIERS.contains(&source(property, level.text()))
                    });
                let Some(object) = node.child_by_field_name("object").filter(|_| has_modifier)
                else {
                    return false;
                };
                node = object;
            }
            _ => return false,
        }
    }
}

/// Whether a test callee is a table test, such as `test.each` or `describe.only.each`, which
/// is called with its table first.
fn ends_with_each(callee: Node<'_>, level: &OutlineLevel<'_>) -> bool {
    callee
        .child_by_field_name("property")
        .is_some_and(|property| source(property, level.text()) == "each")
}

#[cfg(test)]
mod tests {
    use crate::language_engine::outline_texts;
    use crate::languages::script::JAVASCRIPT;

    /// Deeper than a test thread's stack allowed when the walk down the modifiers recursed.
    #[test]
    fn a_long_chain_of_modifiers_is_checked_without_recursion() {
        let chain = ".only".repeat(100_000);
        let text = format!("describe{chain}('t', () => {{}});\n");
        assert_eq!(
            outline_texts(&JAVASCRIPT, &text),
            [format!("describe{chain}('t')")]
        );
    }
}
