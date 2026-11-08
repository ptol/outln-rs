//! JSON keys for the data file engine: the pairs of the root object, printed without their keys' quotes.

use tree_sitter::Node;

use super::{Key, Pair, Value};
use crate::comment_syntax::{BlockComments, CommentSyntax, LineMarker};
use crate::syntax_tree::{byte_range, children, source};

/// JSON has no comments, but tree-sitter-json parses `//` and `/* */` ones (as `tsconfig.json`
/// files use them), so a leading one is the header.
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
/// root's.
pub(super) fn keys(root: Node<'_>, text: &str) -> Vec<Key> {
    // tree-sitter marks `ERROR` nodes as extras, like comments, so comments are skipped by kind.
    let is_syntax = |node: &Node<'_>| node.is_named() && node.kind() != "comment";
    let value = if root.kind() == "ERROR" {
        Some(root)
    } else {
        children(root).into_iter().find(is_syntax)
    };
    let pairs: Vec<Node<'_>> = match value {
        Some(object) if object.kind() == "object" => children(object)
            .into_iter()
            .filter(|child| child.kind() == "pair")
            .collect(),
        Some(broken) if broken.kind() == "ERROR" && source(broken, text).starts_with('{') => {
            children(broken)
                .into_iter()
                .filter(is_syntax)
                .take_while(|child| child.kind() == "pair")
                .collect()
        }
        _ => Vec::new(),
    };
    pairs
        .into_iter()
        .filter_map(|pair| key(pair, text))
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
