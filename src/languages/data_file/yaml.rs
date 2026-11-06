//! YAML keys for the data file engine: the pairs of the first document's root mapping, block or flow style.

use tree_sitter::Node;

use super::{Key, Pair, Value};
use crate::source_text::line_around;
use crate::syntax_tree::{byte_range, children, source};

/// The document start marker, which may come before the header comment.
const DOCUMENT_START: &str = "---";

/// The YAML language server's modeline (`# yaml-language-server: $schema=<url>`), a comment
/// that tools read and the header skips.
const LANGUAGE_SERVER_MODELINE: &str = "yaml-language-server:";

/// Where the preamble starting at byte `start` ends: a directive line (`%YAML 1.2`, `%TAG`),
/// the language server's modeline, or a `---` document start marker, after which a comment may
/// follow on the same line.
pub(super) fn preamble_end(text: &str, start: usize) -> Option<usize> {
    let rest = &text[start..];
    let is_modeline = rest
        .strip_prefix('#')
        .is_some_and(|comment| comment.trim_start().starts_with(LANGUAGE_SERVER_MODELINE));
    if rest.starts_with('%') || is_modeline {
        return Some(line_around(text, start).end);
    }
    let after_marker = rest.strip_prefix(DOCUMENT_START)?;
    after_marker
        .starts_with(char::is_whitespace)
        .then_some(start + DOCUMENT_START.len())
        .or_else(|| after_marker.is_empty().then_some(text.len()))
}

/// The pairs of the first document's root mapping; later documents (after another `---`)
/// aren't outlined, and a document whose root is a list or a scalar has no keys.
pub(super) fn keys(root: Node<'_>, text: &str) -> Vec<Key> {
    let Some(mapping) = children(root)
        .into_iter()
        .find(|node| node.kind() == "document")
        .and_then(|document| collection_child(document, &["block_node", "flow_node"]))
        .and_then(|node| collection_child(node, &["block_mapping", "flow_mapping"]))
    else {
        return Vec::new();
    };
    children(mapping)
        .into_iter()
        .filter_map(|node| match node.kind() {
            "block_mapping_pair" | "flow_pair" => key(node, text),
            // A flow mapping's `{a, b: 2}` writes the key `a` without a colon or a value.
            "flow_node" => Some(written_key(node, node, None, text)),
            _ => None,
        })
        .collect()
}

/// The first child of one of the `kinds`; anchors, tags, and comments may come before it.
fn collection_child<'tree>(node: Node<'tree>, kinds: &[&str]) -> Option<Node<'tree>> {
    children(node)
        .into_iter()
        .find(|child| kinds.contains(&child.kind()))
}

/// A pair's key, or `None` for a pair without one (`: value`).
fn key(pair: Node<'_>, text: &str) -> Option<Key> {
    let key = pair.child_by_field_name("key")?;
    Some(written_key(
        pair,
        key,
        pair.child_by_field_name("value"),
        text,
    ))
}

/// The key `key` of the pair `node`; a key without a value is a null scalar and prints as
/// `key:`.
fn written_key(node: Node<'_>, key: Node<'_>, value: Option<Node<'_>>, text: &str) -> Key {
    let is_collection = value.is_some_and(|value| {
        collection_child(
            value,
            &[
                "block_mapping",
                "block_sequence",
                "flow_mapping",
                "flow_sequence",
            ],
        )
        .is_some()
    });
    Key::pair(&Pair {
        key: source(key, text),
        key_range: byte_range(key),
        separator: ": ",
        value: if is_collection {
            Value::Container
        } else {
            Value::Scalar
        },
        value_source: value.map_or("", |value| source(value, text)),
        node: byte_range(node),
    })
}
