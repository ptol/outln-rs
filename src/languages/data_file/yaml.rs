//! YAML keys for the data file engine and Markdown frontmatter: the pairs of the first non-empty document's root mapping, block or flow style, with scalar values on one line.

use tree_sitter::Node;

use super::{Key, Pair, Value, first_paragraph};
use crate::source_text::{ByteRange, line_end};
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
        return Some(line_end(text, start));
    }
    let after_marker = rest.strip_prefix(DOCUMENT_START)?;
    after_marker
        .starts_with(char::is_whitespace)
        .then_some(start + DOCUMENT_START.len())
        .or_else(|| after_marker.is_empty().then_some(text.len()))
}

/// The keys of the root mapping ([`root_pairs`]).
pub(super) fn keys(root: Node<'_>, text: &str) -> Vec<Key> {
    root_pairs(root)
        .unwrap_or_default()
        .iter()
        .map(|pair| written_key(pair, text))
        .collect()
}

/// One field of a YAML mapping, as a Markdown file's frontmatter prints it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct YamlField {
    /// The key as written, on one line.
    pub(crate) key: String,
    pub(crate) value: YamlValue,
    /// From the key to the end of its value, without the comment lines after it.
    pub(crate) range: ByteRange,
}

/// What a [`YamlField`] holds. A scalar's text is its first paragraph on one line, as
/// [`first_paragraph`] folds it, so a value written over several lines (a quoted or plain
/// scalar continued on the next line, or a `|` or `>` block) prints the way YAML reads it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum YamlValue {
    /// A plain scalar, such as `Guide`, `42`, or `true`; empty for a key without a value.
    Plain(String),
    /// A quoted or block scalar: always a string. Its text is as written between its quotes
    /// (escapes are not decoded), or a block's lines without the `|`/`>` indicator line.
    Text(String),
    /// A mapping or a list, of which only the key prints, as in the YAML engine's outline.
    Container,
}

/// The fields of the root mapping, or `None` when the first non-empty document isn't a
/// mapping (a list, a scalar, only comments, or nothing).
pub(super) fn fields(root: Node<'_>, text: &str) -> Option<Vec<YamlField>> {
    let fields = root_pairs(root)?
        .iter()
        .map(|pair| YamlField {
            key: super::one_line(source(pair.key, text), usize::MAX),
            value: pair.value.map_or(YamlValue::Plain(String::new()), |value| {
                field_value(value, text)
            }),
            range: byte_range(pair.node),
        })
        .collect();
    Some(fields)
}

/// One pair of the root mapping.
struct RootPair<'tree> {
    /// From the key to the end of its value.
    node: Node<'tree>,
    key: Node<'tree>,
    /// `None` for a key without a value (`key:`, or `a` in the flow mapping `{a, b: 2}`).
    value: Option<Node<'tree>>,
}

/// Node kinds that hold a document's content, after any anchor, tag, or comment.
const NODE_KINDS: [&str; 2] = ["block_node", "flow_node"];

/// Node kinds of a mapping.
const MAPPING_KINDS: [&str; 2] = ["block_mapping", "flow_mapping"];

/// Node kinds of a value that prints only its key.
const COLLECTION_KINDS: [&str; 4] = [
    "block_mapping",
    "block_sequence",
    "flow_mapping",
    "flow_sequence",
];

/// The pairs of the root mapping of the first document with content, or `None` if that
/// document's root isn't a mapping. An empty document (`---` right before another `---`) is
/// skipped; later documents aren't outlined.
///
/// A mapping broken by a syntax error parses as an `ERROR` root holding the pairs before the
/// error, which are still listed, so a half-edited file keeps its first keys.
fn root_pairs(root: Node<'_>) -> Option<Vec<RootPair<'_>>> {
    let mapping = if root.kind() == "ERROR" {
        root
    } else {
        children(root)
            .into_iter()
            .filter(|node| node.kind() == "document")
            .find_map(|document| collection_child(document, &NODE_KINDS))
            .and_then(|node| collection_child(node, &MAPPING_KINDS))?
    };
    let pairs: Vec<RootPair<'_>> = children(mapping)
        .into_iter()
        .filter_map(|node| match node.kind() {
            "block_mapping_pair" | "flow_pair" => Some(RootPair {
                node,
                key: node.child_by_field_name("key")?,
                value: node.child_by_field_name("value"),
            }),
            // A flow mapping's `{a, b: 2}` writes the key `a` without a colon or a value.
            "flow_node" if mapping.kind() == "flow_mapping" => Some(RootPair {
                node,
                key: node,
                value: None,
            }),
            _ => None,
        })
        .collect();
    // An `ERROR` root without pairs is some other broken content, not a mapping.
    (mapping.kind() != "ERROR" || !pairs.is_empty()).then_some(pairs)
}

/// The first child of one of the `kinds`; anchors, tags, and comments may come before it.
fn collection_child<'tree>(node: Node<'tree>, kinds: &[&str]) -> Option<Node<'tree>> {
    children(node)
        .into_iter()
        .find(|child| kinds.contains(&child.kind()))
}

/// The key of a root pair; a key without a value is a null scalar and prints as `key:`.
fn written_key(pair: &RootPair<'_>, text: &str) -> Key {
    let is_collection = pair
        .value
        .is_some_and(|value| collection_child(value, &COLLECTION_KINDS).is_some());
    Key::pair(&Pair {
        key: source(pair.key, text),
        key_range: byte_range(pair.key),
        separator: ": ",
        value: if is_collection {
            Value::Container
        } else {
            Value::Scalar
        },
        value_source: pair.value.map_or("", |value| source(value, text)),
        node: byte_range(pair.node),
    })
}

/// A field's value: a collection, or a scalar's text without its quotes or block indicator.
fn field_value(value: Node<'_>, text: &str) -> YamlValue {
    if collection_child(value, &COLLECTION_KINDS).is_some() {
        return YamlValue::Container;
    }
    let Some(scalar) = children(value)
        .into_iter()
        .find(|child| !matches!(child.kind(), "anchor" | "tag" | "comment"))
    else {
        return YamlValue::Plain(String::new());
    };
    let written = source(scalar, text);
    let unquoted = |quote: char| {
        let inner = written.strip_prefix(quote).unwrap_or(written);
        inner.strip_suffix(quote).unwrap_or(inner)
    };
    match scalar.kind() {
        "double_quote_scalar" => YamlValue::Text(first_paragraph(unquoted('"')).text),
        "single_quote_scalar" => YamlValue::Text(first_paragraph(unquoted('\'')).text),
        // The first line holds the `|` or `>` indicator and maybe a comment.
        "block_scalar" => {
            let lines = written.split_once('\n').map_or("", |(_, lines)| lines);
            YamlValue::Text(first_paragraph(lines).text)
        }
        // A plain scalar or an alias (`*default`).
        _ => YamlValue::Plain(first_paragraph(written).text),
    }
}
