//! Data file language engine (TOML, YAML, JSON): header comments, top-level keys with comment-aware line ranges, and the glob view's field summary; each format's child module finds its keys in its syntax tree.
//!
//! The formats differ only in their grammar, comment syntax, and where their keys sit in the
//! tree; how a key prints, where its line range starts and ends, and which keys a field summary
//! shows are the same for all three, so they live here.

mod json;
mod toml;
mod yaml;

use tree_sitter::{Language, Node};

use crate::header_comment::{CommentSyntax, find_header_comment};
use crate::language_engine::LanguageEngine;
use crate::outline::{FieldSummary, FieldsShown, FileSummary, Header, Outline, OutlineLine};
use crate::source_text::{ByteRange, SourceText, trim_end};
use crate::syntax_tree::{byte_range, parse};

/// The language engine for one data file format.
pub(crate) struct DataFile {
    format: Format,
}

pub(crate) static JSON: DataFile = DataFile {
    format: Format::Json,
};
pub(crate) static TOML: DataFile = DataFile {
    format: Format::Toml,
};
pub(crate) static YAML: DataFile = DataFile {
    format: Format::Yaml,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Json,
    Toml,
    Yaml,
}

impl LanguageEngine for DataFile {
    fn extensions(&self) -> &'static [&'static str] {
        match self.format {
            Format::Json => &["json"],
            Format::Toml => &["toml"],
            Format::Yaml => &["yaml", "yml"],
        }
    }

    fn outline(&self, source: &SourceText<'_>) -> Outline {
        let text = source.text;
        let tree = parse(text, &self.format.language());
        let header = self.format.header(text);
        let header_end = header.as_ref().map_or(0, Header::end);
        let keys = self.format.keys(tree.root_node(), text);
        let comments = comment_ranges(tree.root_node());
        let lines = outline_lines(keys, source, &comments, header_end, self.format);
        Outline { header, lines }
    }

    fn file_summary(&self, text: &str) -> FileSummary {
        // A marker-only comment (a lone `#`) prints nothing, so fields summarize the file instead.
        if let Some(header) = self.format.header(text).filter(Header::has_text) {
            return FileSummary::Header(header);
        }
        // The header is found line by line, but fields need a parse; parsing only the start of
        // the file keeps a glob view over large lockfiles fast.
        let (prefix, cut) = summary_prefix(text);
        let tree = parse(prefix, &self.format.language());
        let source = self.format.summary_source(tree.root_node(), prefix);
        field_summary(source, prefix, cut).map_or(FileSummary::Empty, FileSummary::Fields)
    }
}

impl Format {
    fn language(self) -> Language {
        match self {
            Format::Json => tree_sitter_json::LANGUAGE.into(),
            Format::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
            Format::Yaml => tree_sitter_yaml::LANGUAGE.into(),
        }
    }

    /// The header comment: the first comment at the top of the file, after any preamble.
    fn header(self, text: &str) -> Option<Header> {
        find_header_comment(text, self.comment_syntax(), |text, start| {
            self.preamble_end(text, start)
        })
    }

    fn comment_syntax(self) -> CommentSyntax {
        match self {
            Format::Json => json::COMMENT_SYNTAX,
            Format::Toml | Format::Yaml => CommentSyntax::Hash,
        }
    }

    /// Where the preamble starting at byte `start` ends: lines a tool reads rather than a
    /// person, which are neither the header nor a key's comment.
    fn preamble_end(self, text: &str, start: usize) -> Option<usize> {
        match self {
            Format::Json => None,
            Format::Toml => toml::preamble_end(text, start),
            Format::Yaml => yaml::preamble_end(text, start),
        }
    }

    /// The file's top-level keys, in source order.
    fn keys(self, root: Node<'_>, text: &str) -> Vec<Key> {
        match self {
            Format::Json => json::keys(root, text),
            Format::Toml => toml::keys(root, text),
            Format::Yaml => yaml::keys(root, text),
        }
    }

    /// The keys a field summary picks its fields from.
    fn summary_source(self, root: Node<'_>, text: &str) -> SummarySource {
        match self {
            Format::Toml => toml::summary_source(root, text),
            Format::Json | Format::Yaml => SummarySource {
                table: None,
                keys: self.keys(root, text),
            },
        }
    }
}

/// One key of a data file, as its format's module finds it.
#[derive(Debug, PartialEq, Eq)]
struct Key {
    /// What its outline line prints: `name: "outln"`, `jobs`, or a TOML table's `[package]`.
    text: String,
    value: Value,
    /// From the key to the end of its value, as the syntax tree has it; for a TOML table, its
    /// header and every key until the next table. The line range starts and ends from here.
    node: ByteRange,
    /// The key itself (or a TOML table's header), which debug mode highlights.
    highlight: ByteRange,
}

/// What a key holds, which says how it prints and whether a field summary may show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Value {
    /// A string, number, boolean, date, or null, printed after the key.
    Scalar,
    /// A table, mapping, object, list, or array; only the key prints.
    Container,
}

/// How a format writes one key and its value, which [`Key::pair`] turns into a [`Key`].
struct Pair<'text> {
    /// The key as printed: as written, without JSON's quotes.
    key: &'text str,
    /// Where the key is written, which debug mode highlights.
    key_range: ByteRange,
    /// What separates a key from a scalar value in the printed line: `: ` or ` = `.
    separator: &'static str,
    value: Value,
    /// The value as written; empty for a YAML key without a value.
    value_source: &'text str,
    /// From the key to the end of its value.
    node: ByteRange,
}

impl Key {
    /// The key of a pair: a scalar value prints after the key, shortened by [`value_text`].
    fn pair(pair: &Pair<'_>) -> Self {
        // Keys are never cut by length: a key is what an agent looks for.
        let key = one_line(pair.key, usize::MAX);
        let text = match pair.value {
            Value::Scalar => {
                let value = value_text(pair.value_source);
                format!("{key}{}{value}", pair.separator)
                    .trim_end()
                    .to_owned()
            }
            Value::Container => key,
        };
        Key {
            text,
            value: pair.value,
            node: pair.node,
            highlight: pair.key_range,
        }
    }
}

/// The most characters of a value an outline line prints before cutting it with
/// [`CUT_MARKER`].
const MAX_VALUE_CHARS: usize = 40;

/// Printed where a value was cut: after its first line, or after [`MAX_VALUE_CHARS`].
const CUT_MARKER: char = '…';

/// A scalar value as written, on one line, cut to [`MAX_VALUE_CHARS`].
fn value_text(source: &str) -> String {
    one_line(source, MAX_VALUE_CHARS)
}

/// `source` as written, on one line: its first line, cut to `max_chars` characters (not bytes,
/// so a character is never split), with [`CUT_MARKER`] where it was cut. A key may span lines
/// too (a YAML `? [a,\n b]` key), and an outline line must not.
fn one_line(source: &str, max_chars: usize) -> String {
    let source = source.trim();
    let first_line = source.lines().next().unwrap_or_default().trim_end();
    let mut text: String = first_line.chars().take(max_chars).collect();
    if text.len() < source.len() {
        text.push(CUT_MARKER);
    }
    text
}

/// The keys a field summary is made from, and the TOML table they belong to.
struct SummarySource {
    table: Option<String>,
    keys: Vec<Key>,
}

/// How many fields a field summary shows.
const MAX_SUMMARY_FIELDS: usize = 3;

/// How much of a file a field summary reads. A file's first fields almost always fit, and a
/// glob view parses every data file, so a lockfile of several megabytes isn't parsed whole.
const SUMMARY_PREFIX_BYTES: usize = 64 * 1024;

/// Whether a field summary read the whole file or only its start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cut {
    WholeFile,
    Prefix,
}

/// The part of `text` a field summary reads: all of it, or its first
/// [`SUMMARY_PREFIX_BYTES`] up to the last line break among them (or a character boundary,
/// for a file on one line).
fn summary_prefix(text: &str) -> (&str, Cut) {
    if text.len() <= SUMMARY_PREFIX_BYTES {
        return (text, Cut::WholeFile);
    }
    let limit = text.floor_char_boundary(SUMMARY_PREFIX_BYTES);
    let end = text[..limit]
        .rfind('\n')
        .map_or(limit, |newline| newline + 1);
    (&text[..end], Cut::Prefix)
}

/// The field summary of a file's first scalar keys, or `None` without any.
///
/// After a [`Cut::Prefix`], a key that runs to the end of what was read may continue past it
/// (its value cut short, or its parse recovered from the missing rest), so only keys ending
/// before that count, and more fields may follow.
fn field_summary(source: SummarySource, prefix: &str, cut: Cut) -> Option<FieldSummary> {
    let content_end = prefix.trim_end().len();
    let mut scalars = source.keys.into_iter().filter(|key| {
        key.value == Value::Scalar && (cut == Cut::WholeFile || key.node.end < content_end)
    });
    let fields: Vec<String> = scalars
        .by_ref()
        .take(MAX_SUMMARY_FIELDS)
        .map(|key| key.text)
        .collect();
    if fields.is_empty() {
        return None;
    }
    let shown = if cut == Cut::Prefix || scalars.next().is_some() {
        FieldsShown::First
    } else {
        FieldsShown::All
    };
    Some(FieldSummary {
        table: source.table,
        fields,
        shown,
    })
}

/// Every comment's byte range, in source order.
///
/// All three grammars name their comment nodes `comment`, and parse them wherever they occur,
/// often inside the key before (a TOML table runs until the next one), so the whole tree is
/// searched.
fn comment_ranges(root: Node<'_>) -> Vec<ByteRange> {
    let mut ranges = Vec::new();
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if node.kind() == "comment" {
            ranges.push(byte_range(node));
        } else if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return ranges;
            }
        }
    }
}

/// One outline line per key, each line range running from the comment directly above the key
/// to the end of its value, without trailing blank and comment lines.
fn outline_lines(
    keys: Vec<Key>,
    source: &SourceText<'_>,
    comments: &[ByteRange],
    header_end: usize,
    format: Format,
) -> Vec<OutlineLine> {
    let starts: Vec<usize> = keys
        .iter()
        .enumerate()
        .map(|(index, key)| {
            let previous_start = index
                .checked_sub(1)
                .map_or(0, |previous| keys[previous].node.start);
            range_start(key, previous_start, source, comments, header_end, format)
        })
        .collect();
    keys.into_iter()
        .enumerate()
        .map(|(index, key)| {
            let next_start = starts.get(index + 1).copied().unwrap_or(source.text.len());
            let end = content_end(
                source,
                starts[index],
                key.node.end.min(next_start),
                comments,
            );
            OutlineLine {
                text: key.text,
                range: ByteRange::new(starts[index], end),
                highlight: key.highlight,
                depth: 0,
            }
        })
        .collect()
}

/// Where a key's line range starts: at the first of the comment lines directly above it (no
/// blank line between). Data files have no doc comment syntax, so those comments play the part
/// a doc comment plays for a code declaration.
///
/// The walk stops at the key before (`previous_start`), at preamble (a `#:schema` directive),
/// and at a comment indented deeper than the key, which belongs to the value above it (a YAML
/// comment inside a nested mapping). Comments that reach up to the header (`header_end`, 0
/// without one) are the rest of the header comment, so the range then starts at the key.
fn range_start(
    key: &Key,
    previous_start: usize,
    source: &SourceText<'_>,
    comments: &[ByteRange],
    header_end: usize,
    format: Format,
) -> usize {
    let text = source.text;
    let key_line = source.line_around(key.node.start);
    let key_column = key.node.start - key_line.start;
    let mut start = key.node.start;
    let mut line_start = key_line.start;
    while line_start > previous_start && line_start > 0 {
        let line = source.line_around(line_start - 1);
        if line.start < header_end {
            return key.node.start;
        }
        let content = line_content(text, line);
        let is_comment_line = is_comment(content, comments);
        let is_preamble = is_comment_line && format.preamble_end(text, content.start).is_some();
        if !is_comment_line || is_preamble || content.start - line.start > key_column {
            break;
        }
        start = content.start;
        line_start = line.start;
    }
    start
}

/// Where a key's line range ends: at `end`, or before the blank and comment-only lines it ends
/// with, which separate it from the next key or document what comes after it. The key's own
/// first line (at `start`) always stays.
fn content_end(source: &SourceText<'_>, start: usize, end: usize, comments: &[ByteRange]) -> usize {
    let text = source.text;
    let mut end = trim_end(text, ByteRange::new(start, end)).end;
    while end > start {
        // `end` is just after the range's last non-whitespace character, so the line around it
        // is the range's last line (`end - 1` could fall inside a multi-byte character).
        let line = source.line_around(end);
        if line.start <= start || !is_comment(line_content(text, line), comments) {
            break;
        }
        end = trim_end(text, ByteRange::new(start, line.start)).end;
    }
    end
}

/// The part of a line between its leading and trailing whitespace; empty for a blank line.
fn line_content(text: &str, line: ByteRange) -> ByteRange {
    let line_text = &text[line.as_range()];
    let start = line.start + (line_text.len() - line_text.trim_start().len());
    ByteRange::new(start, start + line_text.trim().len())
}

/// Whether the non-empty `range` lies inside one comment. `comments` are sorted and don't
/// overlap, so the only candidate is the first comment ending at or after the range's end.
fn is_comment(range: ByteRange, comments: &[ByteRange]) -> bool {
    let index = comments.partition_point(|comment| comment.end < range.end);
    range.start < range.end
        && comments
            .get(index)
            .is_some_and(|comment| comment.start <= range.start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outline_texts(engine: &DataFile, text: &str) -> Vec<String> {
        engine
            .outline(&SourceText::new(text))
            .lines
            .into_iter()
            .map(|line| line.text)
            .collect()
    }

    #[test]
    fn every_grammar_loads_and_parses() {
        for (engine, text, printed) in [
            (&JSON, "{\"a\": 1}\n", "a: 1"),
            (&TOML, "a = 1\n", "a = 1"),
            (&YAML, "a: 1\n", "a: 1"),
        ] {
            let tree = parse(text, &engine.format.language());
            assert!(!tree.root_node().has_error(), "{:?}", engine.format);
            assert_eq!(outline_texts(engine, text), [printed]);
        }
    }

    #[test]
    fn a_range_ending_in_a_multi_byte_character_is_trimmed_without_panicking() {
        assert_eq!(outline_texts(&YAML, "name: café\n"), ["name: café"]);
        let lines = TOML
            .outline(&SourceText::new("[t]\nx = 1\n# Café\n[u]\n"))
            .lines;
        let ranges: Vec<ByteRange> = lines.into_iter().map(|line| line.range).collect();
        // `[u]`'s range starts at the comment above it; `é` is two bytes.
        assert_eq!(ranges, [ByteRange::new(0, 9), ByteRange::new(10, 21)]);
    }

    #[test]
    fn yaml_keys_print_on_one_line_and_flow_keys_may_lack_a_value() {
        assert_eq!(outline_texts(&YAML, "? [a,\n   b]\n: 1\n"), ["[a,…: 1"]);
        assert_eq!(outline_texts(&YAML, "{a, b: 2}\n"), ["a:", "b: 2"]);
    }

    /// Finding each key's line once scanned to the line's ends, which made a minified file
    /// quadratic in its length.
    #[test]
    fn many_keys_on_one_line_are_listed_without_rescanning_the_line() {
        let count = 5_000;
        let pairs: Vec<String> = (0..count).map(|index| format!("\"k{index}\": 1")).collect();
        let text = format!("{{{}}}", pairs.join(", "));
        assert_eq!(outline_texts(&JSON, &text).len(), count);
    }

    #[test]
    fn json_keys_with_surrounding_whitespace_keep_their_quotes() {
        assert_eq!(
            outline_texts(&JSON, "{\"a \": 1, \"a\": 2, \" \": 3}\n"),
            ["\"a \": 1", "a: 2", "\" \": 3"]
        );
    }

    #[test]
    fn a_marker_only_header_comment_falls_back_to_the_field_summary() {
        assert_eq!(
            TOML.file_summary("#\nname = \"demo\"\n"),
            FileSummary::Fields(FieldSummary {
                table: None,
                fields: vec!["name = \"demo\"".to_owned()],
                shown: FieldsShown::All,
            })
        );
    }

    #[test]
    fn value_text_keeps_the_first_line_and_cuts_long_values_by_characters() {
        assert_eq!(value_text("\"outln\""), "\"outln\"");
        assert_eq!(value_text("|\n  block\n"), "|…");
        let long = "é".repeat(MAX_VALUE_CHARS + 1);
        assert_eq!(
            value_text(&long),
            format!("{}…", "é".repeat(MAX_VALUE_CHARS))
        );
    }

    #[test]
    fn summary_prefix_cuts_large_files_at_a_line_break() {
        let line = "\"key\": \"value\",\n";
        let text = line.repeat(SUMMARY_PREFIX_BYTES / line.len() + 2);
        let (prefix, cut) = summary_prefix(&text);
        assert_eq!(cut, Cut::Prefix);
        assert!(prefix.ends_with('\n') && prefix.len() <= SUMMARY_PREFIX_BYTES);
        let one_line = "é".repeat(SUMMARY_PREFIX_BYTES);
        let (prefix, _) = summary_prefix(&one_line);
        assert!(one_line.is_char_boundary(prefix.len()));
        assert_eq!(summary_prefix("a: 1\n"), ("a: 1\n", Cut::WholeFile));
    }

    #[test]
    fn a_field_summary_of_a_large_file_reads_only_its_start() {
        let entries = "    \"dependency\": {\"version\": 1},\n".repeat(SUMMARY_PREFIX_BYTES / 20);
        let text = format!("{{\n  \"name\": \"big\",\n  \"packages\": {{\n{entries}  }}\n}}\n");
        assert!(text.len() > SUMMARY_PREFIX_BYTES);
        assert_eq!(
            JSON.file_summary(&text),
            FileSummary::Fields(FieldSummary {
                table: None,
                fields: vec!["name: \"big\"".to_owned()],
                shown: FieldsShown::First,
            })
        );
    }

    #[test]
    fn a_field_summary_after_a_cut_drops_the_key_running_to_the_cut() {
        let prefix = "{\n\"a\": 1,\n\"b\": \"cut sh";
        let tree = parse(prefix, &Format::Json.language());
        let source = Format::Json.summary_source(tree.root_node(), prefix);
        let summary = field_summary(source, prefix, Cut::Prefix);
        assert_eq!(
            summary,
            Some(FieldSummary {
                table: None,
                fields: vec!["a: 1".to_owned()],
                shown: FieldsShown::First,
            })
        );
    }
}
