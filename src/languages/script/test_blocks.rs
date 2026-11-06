//! Test blocks: top-level `describe`, `it`, `test`, and `suite` calls of Jest, Vitest, Mocha, and node:test, listed by their title.

use tree_sitter::Node;

use super::{Statement, one_line, statement_line};
use crate::outline::OutlineLine;
use crate::signature::Signature;
use crate::syntax_tree::{first_syntax_child, source};

/// Functions that open a test block.
const TEST_FUNCTIONS: [&str; 4] = ["describe", "it", "test", "suite"];

/// Properties that select or skip test blocks: `describe.only`, `it.skip`, `test.each`.
const TEST_MODIFIERS: [&str; 3] = ["only", "skip", "each"];

/// The outline line of a test block, such as `describe('parser')` or `test.each('adds %i')`, or
/// `None` for any other call.
///
/// The title is the first argument and must be a string or template literal. A table test
/// (`test.each(table)('title', fn)`, or with a tagged template table) prints its callee and
/// title without the table.
pub(super) fn test_block_line(
    statement: Statement<'_>,
    call: Node<'_>,
    text: &str,
) -> Option<OutlineLine> {
    let function = call.child_by_field_name("function")?;
    let callee = match function.kind() {
        "call_expression" => function
            .child_by_field_name("function")
            .filter(|inner| is_test_callee(*inner, text) && ends_with_each(*inner, text))?,
        _ if is_test_callee(function, text) && !ends_with_each(function, text) => function,
        _ => return None,
    };
    let title = first_syntax_child(call.child_by_field_name("arguments")?)
        .filter(|title| matches!(title.kind(), "string" | "template_string"))?;
    let signature = Signature {
        text: format!("{}({})", one_line(callee, text), one_line(title, text)),
        highlight_end: title.end_byte(),
    };
    Some(statement_line(statement, "", signature))
}

/// Whether the node names a test function, possibly with modifiers: `it`, `describe.skip`.
fn is_test_callee(node: Node<'_>, text: &str) -> bool {
    match node.kind() {
        "identifier" => TEST_FUNCTIONS.contains(&source(node, text)),
        "member_expression" => {
            let modifier = node
                .child_by_field_name("property")
                .is_some_and(|property| TEST_MODIFIERS.contains(&source(property, text)));
            let object = node
                .child_by_field_name("object")
                .is_some_and(|object| is_test_callee(object, text));
            modifier && object
        }
        _ => false,
    }
}

/// Whether a test callee is a table test, such as `test.each` or `describe.only.each`, which
/// is called with its table first.
fn ends_with_each(callee: Node<'_>, text: &str) -> bool {
    callee
        .child_by_field_name("property")
        .is_some_and(|property| source(property, text) == "each")
}
