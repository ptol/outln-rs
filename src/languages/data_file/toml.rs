//! TOML keys for the data file engine: the keys before the first table, then every `[table]` and `[[array]]` header as written on its line; a field summary falls back to the first table's keys.

use tree_sitter::Node;

use super::{Key, Pair, SummarySource, Value, one_line};
use crate::source_text::{ByteRange, line_end};
use crate::syntax_tree::{byte_range, children, source};

/// Where a directive comment starting at byte `start` ends, such as taplo's `#:schema <url>`,
/// which tools read and the header skips.
pub(super) fn preamble_end(text: &str, start: usize) -> Option<usize> {
    text[start..]
        .starts_with("#:")
        .then(|| line_end(text, start))
}

/// The keys before the first table, then one key per table header, in source order. Tables
/// aren't merged by name: `[lints.rust]` and `[lints.clippy]` are separate keys, as is each
/// `[[bin]]`.
pub(super) fn keys(root: Node<'_>, text: &str) -> Vec<Key> {
    children(root)
        .into_iter()
        .filter_map(|node| match node.kind() {
            "pair" => pair_key(node, text),
            "table" | "table_array_element" => Some(table_key(node, text)),
            _ => None,
        })
        .collect()
}

/// The keys a field summary shows: the keys before the first table if one is a scalar, or else
/// the first table's keys if one is (`Cargo.toml` keeps its name and version in `[package]`),
/// or else the file's keys, whose names then say what it holds.
pub(super) fn summary_source(root: Node<'_>, text: &str) -> SummarySource {
    let has_scalar = |keys: &[Key]| keys.iter().any(|key| key.value == Value::Scalar);
    let root_keys = pair_keys(root, text);
    if has_scalar(&root_keys) {
        return SummarySource {
            table: None,
            keys: root_keys,
        };
    }
    let first_table = children(root)
        .into_iter()
        .find(|node| matches!(node.kind(), "table" | "table_array_element"));
    if let Some(table) = first_table {
        let table_keys = pair_keys(table, text);
        if has_scalar(&table_keys) {
            return SummarySource {
                table: Some(table_header(table, text)),
                keys: table_keys,
            };
        }
    }
    SummarySource {
        table: None,
        keys: keys(root, text),
    }
}

/// The keys of the `key = value` pairs directly in `node`: the document's root keys, or a
/// table's.
fn pair_keys(node: Node<'_>, text: &str) -> Vec<Key> {
    children(node)
        .into_iter()
        .filter(|child| child.kind() == "pair")
        .filter_map(|pair| pair_key(pair, text))
        .collect()
}

/// A `key = value` pair's key, or `None` if the parser recovered it without a value.
fn pair_key(pair: Node<'_>, text: &str) -> Option<Key> {
    // A pair's named children are its key and its value, plus a trailing comment.
    let mut cursor = pair.walk();
    let mut parts = pair
        .named_children(&mut cursor)
        .filter(|child| !child.is_extra());
    let key = parts.next()?;
    let value = parts.next()?;
    Some(Key::pair(&Pair {
        key: source(key, text),
        key_range: byte_range(key),
        separator: " = ",
        value: match value.kind() {
            "inline_table" | "array" => Value::Container,
            _ => Value::Scalar,
        },
        value_source: source(value, text),
        node: byte_range(pair),
    }))
}

/// A table's key: its header as written, from `[` to `]` (or `[[` to `]]`).
fn table_key(table: Node<'_>, text: &str) -> Key {
    let header = header_range(table, text);
    let printed = table_header(table, text);
    Key {
        name: printed.clone(),
        text: printed,
        value: Value::Container,
        node: byte_range(table),
        highlight: header,
    }
}

/// A table's header as printed: as written, on one line.
fn table_header(table: Node<'_>, text: &str) -> String {
    one_line(&text[header_range(table, text).as_range()], usize::MAX)
}

/// Where a table's header is written: from the table's start to its closing bracket, but no
/// further than its first line. A header that never closes on its line (`[[bin` in a
/// half-edited file) may be recovered with a bracket from a later line, or none.
fn header_range(table: Node<'_>, text: &str) -> ByteRange {
    let line_end = line_end(text, table.start_byte());
    let end = children(table)
        .into_iter()
        .find(|child| matches!(child.kind(), "]" | "]]"))
        .map_or(line_end, |close| close.end_byte().min(line_end));
    ByteRange::new(table.start_byte(), end)
}
