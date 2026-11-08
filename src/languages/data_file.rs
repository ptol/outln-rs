//! Data file language engine (TOML, YAML, JSON): header comments, top-level keys with comment-aware line ranges and values on one line, the glob view's field summary (describing keys first), and the YAML fields of Markdown frontmatter.
//!
//! The formats differ only in their grammar, comment syntax, preamble, and where their keys sit
//! in the tree, which each format's [`DataFile`] table names; how a key prints, where its line
//! range starts and ends, and which keys a field summary shows are the same for all three, so
//! they live here.
//!
//! A key's outline line prints a scalar value as written, its first paragraph on one line (a
//! value written over several lines has its lines joined by spaces), cut at 40 characters, or
//! 100 for a describing key (`description`, `title`). A field summary shows up to three scalar
//! keys, describing keys first, then the others in source order; a file without scalar keys
//! shows its keys' names. The Markdown engine reads its frontmatter with [`yaml_fields`], so a
//! frontmatter field is a YAML key by the same rules.

mod json;
mod toml;
mod yaml;

use tree_sitter::{Language, Node};

use crate::comment_syntax::CommentSyntax;
use crate::header_comment::{HeaderChoice, HeaderSearch, find_header_comment};
use crate::language_engine::LanguageEngine;
use crate::outline::{
    FieldSummary, FieldsShown, FileSummary, Header, Outline, OutlineLine, is_describing_key,
    summary_fields,
};
use crate::source_text::{ByteRange, SourceText, trim_end};
use crate::syntax_tree::{byte_range, parse};

pub(crate) use yaml::{YamlField, YamlValue};

/// The language engine for one data file format, as the data that differs between formats.
pub(crate) struct DataFile {
    /// File extensions, lowercase and without the dot.
    extensions: &'static [&'static str],
    /// The tree-sitter grammar. A function, because a grammar can't be built in a `static`.
    grammar: fn() -> Language,
    comment_syntax: CommentSyntax,
    /// Where the preamble starting at a byte ends: lines a tool reads rather than a person,
    /// which are neither the header nor a key's comment. `None` if none starts there.
    preamble_end: fn(&str, usize) -> Option<usize>,
    /// The file's top-level keys, in source order.
    keys: fn(Node<'_>, &str) -> Vec<Key>,
    /// The keys a field summary picks its fields from, for a format whose summary doesn't just
    /// take the file's keys; `None` takes them.
    summary_source: Option<fn(Node<'_>, &str) -> SummarySource>,
    /// The part of a file its grammar parses: all of it, or for a grammar that breaks on some
    /// input (YAML's aborts the process on deep nesting), the lines before that input.
    parsable_part: fn(&str) -> &str,
}

pub(crate) static JSON: DataFile = DataFile {
    // JSON with comments (`tsconfig`-style `.jsonc`) is the same grammar, which reads comments
    // and trailing commas; JSON5's unquoted keys and single quotes are not, so `.json5` isn't.
    extensions: &["json", "jsonc"],
    grammar: || tree_sitter_json::LANGUAGE.into(),
    comment_syntax: json::COMMENT_SYNTAX,
    preamble_end: |_, _| None,
    keys: json::keys,
    summary_source: None,
    parsable_part: |text| text,
};
pub(crate) static TOML: DataFile = DataFile {
    extensions: &["toml"],
    grammar: || tree_sitter_toml_ng::LANGUAGE.into(),
    comment_syntax: CommentSyntax::HASH,
    preamble_end: toml::preamble_end,
    keys: toml::keys,
    summary_source: Some(toml::summary_source),
    parsable_part: |text| text,
};
pub(crate) static YAML: DataFile = DataFile {
    extensions: &["yaml", "yml"],
    grammar: || tree_sitter_yaml::LANGUAGE.into(),
    comment_syntax: CommentSyntax::HASH,
    preamble_end: yaml::preamble_end,
    keys: yaml::keys,
    summary_source: None,
    parsable_part: yaml::parsable_part,
};

impl LanguageEngine for DataFile {
    fn extensions(&self) -> &'static [&'static str] {
        self.extensions
    }

    fn outline(&self, source: &SourceText<'_>) -> Outline {
        let text = source.text;
        let parsed = (self.parsable_part)(text);
        let tree = parse(parsed, &(self.grammar)());
        let HeaderSearch { header, end } = self.header(text);
        let keys = (self.keys)(tree.root_node(), parsed);
        let comments = comment_ranges(tree.root_node());
        let lines = outline_lines(keys, source, &comments, end, self.preamble_end);
        Outline { header, lines }
    }

    fn file_summary(&self, text: &str) -> FileSummary {
        // A marker-only comment (a lone `#`) prints nothing, so fields summarize the file instead.
        if let Some(header) = self.header(text).header.filter(Header::has_text) {
            return FileSummary::Header(header);
        }
        // The header is found line by line, but fields need a parse; parsing only the start of
        // the file keeps a glob view over large lockfiles fast.
        let (prefix, cut) = self.summary_prefix(text);
        let tree = parse(prefix, &(self.grammar)());
        let source = self.summary_source(tree.root_node(), prefix);
        field_summary(source, prefix, cut).map_or(FileSummary::Empty, FileSummary::Fields)
    }

    #[cfg(test)]
    fn grammar(&self) -> Option<Language> {
        Some((self.grammar)())
    }
}

/// The fields of the YAML mapping `text` holds, as a Markdown file's frontmatter prints them,
/// or `None` when it holds no mapping: the YAML engine's own rules decide which pairs are
/// top-level and how a value is written on one line.
pub(crate) fn yaml_fields(text: &str) -> Option<Vec<YamlField>> {
    let parsed = yaml::parsable_part(text);
    let tree = parse(parsed, &(YAML.grammar)());
    let mut fields = yaml::fields(tree.root_node(), parsed)?;
    // The grammar puts the comment lines after a value inside it, as for a key's line range.
    let source = SourceText::new(text);
    let comments = comment_ranges(tree.root_node());
    for field in &mut fields {
        field.range.end = content_end(&source, field.range.start, field.range.end, &comments);
    }
    Some(fields)
}

impl DataFile {
    /// The header comment: the first comment at the top of the file with a header paragraph,
    /// after any preamble; and where the comments read for it end.
    fn header(&self, text: &str) -> HeaderSearch {
        find_header_comment(
            text,
            &self.comment_syntax,
            HeaderChoice::FirstComment,
            self.preamble_end,
        )
    }

    /// The part of `text` a field summary reads ([`summary_prefix`]), no more than its grammar
    /// can parse, and whether that is the whole file.
    fn summary_prefix<'text>(&self, text: &'text str) -> (&'text str, Cut) {
        let (prefix, cut) = summary_prefix(text);
        let parsed = (self.parsable_part)(prefix);
        if parsed.len() < prefix.len() {
            (parsed, Cut::Prefix)
        } else {
            (prefix, cut)
        }
    }

    /// The keys a field summary picks its fields from.
    fn summary_source(&self, root: Node<'_>, text: &str) -> SummarySource {
        match self.summary_source {
            Some(summary_source) => summary_source(root, text),
            None => SummarySource {
                table: None,
                keys: (self.keys)(root, text),
            },
        }
    }
}

/// One key of a data file, as its format's module finds it.
#[derive(Debug, PartialEq, Eq)]
struct Key {
    /// The key as printed (`name`, a TOML table's `[package]`), which a field summary orders
    /// its fields by.
    name: String,
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
        let key = printed_key(pair.key);
        let text = match pair.value {
            Value::Scalar => {
                let value = value_text(&key, pair.value_source);
                format!("{key}{}{value}", pair.separator)
                    .trim_end()
                    .to_owned()
            }
            Value::Container => key.clone(),
        };
        Key {
            name: key,
            text,
            value: pair.value,
            node: pair.node,
            highlight: pair.key_range,
        }
    }
}

/// A key as printed: as written, on one line ([`one_line`]), never cut by length, because a key
/// is what an agent looks for. A key of whitespace only (a YAML key of a lone U+2028 line
/// separator, which [`str::trim`] removes) would print as nothing, so it prints quoted, with
/// escapes.
fn printed_key(source: &str) -> String {
    let key = one_line(source, usize::MAX);
    if key.is_empty() {
        format!("{source:?}")
    } else {
        key
    }
}

/// The most characters of a value an outline line prints before cutting it with
/// [`CUT_MARKER`].
const MAX_VALUE_CHARS: usize = 40;

/// The most characters of a `description` or `title` value ([`is_describing_key`]) an outline
/// line prints: it tells an agent what the file is, so it gets more room than other values.
const MAX_DESCRIBING_VALUE_CHARS: usize = 100;

/// Printed where a value was cut: after its first paragraph, or after its most characters.
const CUT_MARKER: char = '…';

/// The scalar value of `key` as written, on one line, cut to [`MAX_VALUE_CHARS`], or to
/// [`MAX_DESCRIBING_VALUE_CHARS`] for a describing key's.
fn value_text(key: &str, source: &str) -> String {
    let max_chars = if is_describing_key(key) {
        MAX_DESCRIBING_VALUE_CHARS
    } else {
        MAX_VALUE_CHARS
    };
    one_line(source, max_chars)
}

/// `source` as written, on one line: its [`first_paragraph`], cut to `max_chars` characters
/// (not bytes, so a character is never split), with [`CUT_MARKER`] where anything was left
/// out. A key may span lines too (a YAML `? [a,\n b]` key, a half-edited TOML table header),
/// and an outline line must not.
///
/// A cut quoted value gets its quote again after the marker (`"Starts here…"`), so the quote
/// still closes; a TOML `"""` string gets all three.
fn one_line(source: &str, max_chars: usize) -> String {
    let folded = first_paragraph(source);
    let mut text: String = folded.text.chars().take(max_chars).collect();
    if folded.has_more || text.len() < folded.text.len() {
        text.push(CUT_MARKER);
        text.push_str(opening_quote(&folded.text));
    }
    text
}

/// The quote a value opens with: `"`, `'`, or a TOML multi-line string's `"""` or `'''`;
/// empty for an unquoted value.
fn opening_quote(value: &str) -> &str {
    let Some(quote) = value
        .chars()
        .next()
        .filter(|first| matches!(first, '"' | '\''))
    else {
        return "";
    };
    let run = value.len() - value.trim_start_matches(quote).len();
    &value[..run.min(MULTI_LINE_QUOTE_LEN)]
}

/// How many quote characters open a TOML multi-line string (`"""`).
const MULTI_LINE_QUOTE_LEN: usize = 3;

/// A text's first paragraph on one line, as [`first_paragraph`] makes it.
struct Folded {
    text: String,
    /// Whether non-blank text follows the first paragraph.
    has_more: bool,
}

/// The first paragraph of `source`: its lines up to the first blank line, each trimmed and
/// joined with one space, as YAML folds a scalar written over several lines.
fn first_paragraph(source: &str) -> Folded {
    let mut lines = source
        .lines()
        .map(str::trim)
        .skip_while(|line| line.is_empty());
    let paragraph: Vec<&str> = lines.by_ref().take_while(|line| !line.is_empty()).collect();
    Folded {
        text: paragraph.join(" "),
        has_more: lines.any(|line| !line.is_empty()),
    }
}

/// The keys a field summary is made from, and the TOML table they belong to.
struct SummarySource {
    table: Option<String>,
    keys: Vec<Key>,
}

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

/// The field summary of a file's scalar keys, picked and ordered by [`summary_fields`]
/// (`description` and `title` first), or, in a file without scalar keys, of its keys' names
/// (`{compilerOptions, include}`); `None` without any key.
///
/// After a [`Cut::Prefix`], a key that runs to the end of what was read may continue past it
/// (its value cut short, or its parse recovered from the missing rest), so only keys ending
/// before that count, and more fields may follow.
fn field_summary(source: SummarySource, prefix: &str, cut: Cut) -> Option<FieldSummary> {
    let content_end = prefix.trim_end().len();
    let complete: Vec<Key> = source
        .keys
        .into_iter()
        .filter(|key| cut == Cut::WholeFile || key.node.end < content_end)
        .collect();
    let has_scalars = complete.iter().any(|key| key.value == Value::Scalar);
    // A key without a scalar value prints only its name, so the fallback is the keys' names.
    let candidates = complete
        .into_iter()
        .filter(|key| !has_scalars || key.value == Value::Scalar);
    let (keys, shown) = summary_fields(candidates, |key| &key.name);
    if keys.is_empty() {
        return None;
    }
    Some(FieldSummary {
        table: source.table,
        fields: keys.into_iter().map(|key| key.text).collect(),
        shown: if cut == Cut::Prefix {
            FieldsShown::First
        } else {
            shown
        },
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
    header_end: Option<usize>,
    preamble_end: fn(&str, usize) -> Option<usize>,
) -> Vec<OutlineLine> {
    let starts: Vec<usize> = keys
        .iter()
        .enumerate()
        .map(|(index, key)| {
            let previous_start = index
                .checked_sub(1)
                .map_or(0, |previous| keys[previous].node.start);
            range_start(
                key,
                previous_start,
                source,
                comments,
                header_end,
                preamble_end,
            )
        })
        .collect();
    keys.into_iter()
        .enumerate()
        .map(|(index, key)| {
            let next_start = starts.get(index + 1).copied().unwrap_or(source.text.len());
            // A key may end in a line break or a comment line (a YAML `? key` written over
            // several lines), which the highlight leaves out and the line range keeps, so the
            // trim of a value's trailing comments never cuts into its key.
            let highlight = trim_end(source.text, key.highlight);
            let value_end = content_end(
                source,
                starts[index],
                key.node.end.min(next_start),
                comments,
            );
            OutlineLine {
                text: key.text,
                range: ByteRange::new(starts[index], value_end.max(highlight.end)),
                highlight,
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
/// comment inside a nested mapping). Comments that reach up to the header comment (which ends
/// at `header_end`, with any license notice passed over for it) are the rest of it, so the
/// range then starts at the key.
fn range_start(
    key: &Key,
    previous_start: usize,
    source: &SourceText<'_>,
    comments: &[ByteRange],
    header_end: Option<usize>,
    preamble_end: fn(&str, usize) -> Option<usize>,
) -> usize {
    let text = source.text;
    let key_line = source.line_around(key.node.start);
    let key_column = key.node.start - key_line.start;
    let mut start = key.node.start;
    let mut line_start = key_line.start;
    while line_start > previous_start && line_start > 0 {
        let line = source.line_around(line_start - 1);
        if header_end.is_some_and(|end| line.start < end) {
            return key.node.start;
        }
        let content = line_content(text, line);
        let is_comment_line = is_comment(content, comments);
        let is_preamble = is_comment_line && preamble_end(text, content.start).is_some();
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
    use crate::language_engine::{outline_of, outline_texts};

    #[test]
    fn a_range_ending_in_a_multi_byte_character_is_trimmed_without_panicking() {
        assert_eq!(outline_texts(&YAML, "name: café\n"), ["name: café"]);
        let lines = outline_of(&TOML, "[t]\nx = 1\n# Café\n[u]\n").lines;
        let ranges: Vec<ByteRange> = lines.into_iter().map(|line| line.range).collect();
        // `[u]`'s range starts at the comment above it; `é` is two bytes.
        assert_eq!(ranges, [ByteRange::new(0, 9), ByteRange::new(10, 21)]);
    }

    #[test]
    fn yaml_keys_print_on_one_line_and_flow_keys_may_lack_a_value() {
        assert_eq!(outline_texts(&YAML, "? [a,\n   b]\n: 1\n"), ["[a, b]: 1"]);
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

    /// Nesting this deep would abort the process in tree-sitter-yaml's scanner.
    #[test]
    fn yaml_nested_too_deep_for_its_grammar_is_read_up_to_the_deep_lines() {
        let nested: String = (1..=300)
            .flat_map(|depth| [" ".repeat(depth), "k:\n".to_owned()])
            .collect();
        let text = format!("first: 1\nnested:\n{nested}last: 2\n");
        assert_eq!(outline_texts(&YAML, &text), ["first: 1", "nested"]);
        assert_eq!(
            YAML.file_summary(&text),
            FileSummary::Fields(FieldSummary {
                table: None,
                fields: vec!["first: 1".to_owned()],
                shown: FieldsShown::First,
            })
        );
        let deep_list = "- ".repeat(300);
        assert_eq!(
            yaml_fields(&format!("title: x\n{deep_list}\n")).map(|fields| fields.len()),
            Some(1)
        );
    }

    #[test]
    fn indicators_inside_a_quoted_yaml_scalar_on_its_line_are_not_nesting() {
        let dashes = "- ".repeat(250);
        for value in [format!("\"{dashes}\""), format!("'it''s {dashes}'")] {
            let text = format!("x: {value}\ny: 1\n");
            assert_eq!(yaml::parsable_part(&text), text);
        }
        // Each of these may nest too deep for the scanner once a syntax error makes it read
        // every indicator (a quoted scalar continued on the next line, or after a plain scalar
        // continued from the line before, where a `: ` ends that scalar), so the line with the
        // dashes is left out.
        for deep in [
            format!("x: 'a' {dashes}"),
            format!("x: \"a\n{dashes}\""),
            format!("a\n\"b: {dashes}\""),
        ] {
            let text = format!("first: 1\n{deep}\n");
            let parsed = yaml::parsable_part(&text);
            assert!(
                parsed.starts_with("first: 1\n") && !parsed.contains(&dashes),
                "{deep:?}"
            );
        }
    }

    #[test]
    fn a_yaml_key_ending_in_a_comment_line_keeps_its_key_inside_its_range() {
        for text in ["? :\n", "? a\n# comment\n: 1\n", "? a\n  # comment\n"] {
            for line in outline_of(&YAML, text).lines {
                assert!(
                    line.range.start <= line.highlight.start
                        && line.highlight.end <= line.range.end,
                    "{text:?}: {line:?}"
                );
            }
        }
    }

    #[test]
    fn a_yaml_key_of_whitespace_only_prints_quoted() {
        assert_eq!(outline_texts(&YAML, "\u{2028}: 1\n"), ["\"\\u{2028}\": 1"]);
        let fields = yaml_fields("\u{2028}: 1\n").unwrap_or_default();
        assert_eq!(fields[0].key, "\"\\u{2028}\"");
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
    fn value_text_folds_the_first_paragraph_and_cuts_long_values_by_characters() {
        assert_eq!(value_text("name", "\"outln\""), "\"outln\"");
        assert_eq!(
            value_text("run", "|\n  one\n  two\n\n  three\n"),
            "| one two…"
        );
        let long = "é".repeat(MAX_DESCRIBING_VALUE_CHARS + 1);
        assert_eq!(
            value_text("name", &long),
            format!("{}…", "é".repeat(MAX_VALUE_CHARS))
        );
        assert_eq!(
            value_text("description", &long),
            format!("{}…", "é".repeat(MAX_DESCRIBING_VALUE_CHARS))
        );
    }

    #[test]
    fn a_cut_quoted_value_still_closes_its_quote() {
        let long = "x".repeat(MAX_VALUE_CHARS);
        let cut = format!("\"{}…\"", "x".repeat(MAX_VALUE_CHARS - 1));
        assert_eq!(value_text("name", &format!("\"{long}\"")), cut);
        assert_eq!(
            value_text("name", "'para one\n\n  para two'"),
            "'para one…'"
        );
        assert_eq!(
            value_text("name", &format!("\"\"\"{long}\"\"\"")),
            format!("\"\"\"{}…\"\"\"", "x".repeat(MAX_VALUE_CHARS - 3))
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
        let tree = parse(prefix, &(JSON.grammar)());
        let source = JSON.summary_source(tree.root_node(), prefix);
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
