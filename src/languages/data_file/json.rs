//! JSON keys for the data file engine: the pairs of the root object, printed without their keys' quotes.

use tree_sitter::Node;

use super::{Key, Pair, Value};
use crate::comment_syntax::{BlockComments, CommentSyntax, LineMarker};
use crate::syntax_tree::{byte_range, children, source};

/// JSON has no comments, but tree-sitter-json parses `//` and `/* */` ones (as `tsconfig.json`
/// files use them), so a leading one with a prose paragraph that isn't a license notice is the
/// header, as in the other data files.
pub(super) const COMMENT_SYNTAX: CommentSyntax = CommentSyntax {
    line: Some(LineMarker::Slashes),
    block: Some(BlockComments::Flat),
    doc_markers: &[],
};

/// The pairs of the root object; a file whose root is an array or a scalar has no keys.
///
/// An object that never closes (a file cut short, or a field summary's prefix) parses as an
/// `ERROR` node, which flattens the pairs of nested objects into its own children, so then
/// only the pairs before its first other child (the key whose value broke off) are the
/// root's. An object that closes but misses a comma (`"a": 1 "b": 2`, as in a half-edited
/// file) holds the pairs before the gap in an `ERROR` child, read by the same rule.
pub(super) fn keys(root: Node<'_>, text: &str) -> Vec<Key> {
    let value = if root.kind() == "ERROR" {
        Some(root)
    } else {
        children(root).into_iter().find(is_syntax)
    };
    let pairs: Vec<Node<'_>> = match value {
        Some(object) if object.kind() == "object" => children(object)
            .into_iter()
            .flat_map(|child| match child.kind() {
                "pair" => vec![child],
                "ERROR" => leading_pairs(child),
                _ => Vec::new(),
            })
            .collect(),
        Some(broken) if broken.kind() == "ERROR" && source(broken, text).starts_with('{') => {
            leading_pairs(broken)
        }
        _ => Vec::new(),
    };
    pairs
        .into_iter()
        .filter_map(|pair| key(pair, text))
        .collect()
}

/// Whether a node is part of the JSON value rather than a comment. tree-sitter marks `ERROR`
/// nodes as extras, like comments, so comments are told apart by kind.
fn is_syntax(node: &Node<'_>) -> bool {
    node.is_named() && node.kind() != "comment"
}

/// The pairs an `ERROR` node starts with, before its first child of another kind.
fn leading_pairs(error: Node<'_>) -> Vec<Node<'_>> {
    children(error)
        .into_iter()
        .filter(is_syntax)
        .take_while(|child| child.kind() == "pair")
        .collect()
}

/// A pair's key, or `None` if the parser recovered it without a key or a value.
fn key(pair: Node<'_>, text: &str) -> Option<Key> {
    let key = pair.child_by_field_name("key")?;
    let value = pair.child_by_field_name("value")?;
    // Printed as written between its quotes, escapes included. A key that is empty or starts
    // or ends with whitespace keeps its quotes, so `"a "` and `"a"` still print differently.
    let written = source(key, text);
    let printed = written
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .filter(|inner| !inner.is_empty() && inner.trim() == *inner)
        .unwrap_or(written);
    Some(Key::pair(&Pair {
        key: printed,
        key_range: byte_range(key),
        separator: ": ",
        value: match value.kind() {
            "object" | "array" => Value::Container,
            _ => Value::Scalar,
        },
        value_source: source(value, text),
        node: byte_range(pair),
    }))
}
