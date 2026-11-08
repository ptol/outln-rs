//! Outline data that language engines return, and its rendering as file mode and glob view text.

use std::fmt::Write as _;

use crate::source_text::{ByteRange, LineCount, LineIndex};

/// The compact structural summary of one source file, as data; [`render_outline`] prints it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Outline {
    pub(crate) header: Option<Header>,
    pub(crate) lines: Vec<OutlineLine>,
}

/// What a file says about itself at its top: frontmatter for Markdown, a header comment for code
/// and data files.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Header {
    Frontmatter(Vec<FrontmatterField>),
    Comment(Vec<CommentLine>),
}

impl Header {
    /// Whether the header prints anything: a comment of markers only (`/**/`, a lone `#`)
    /// doesn't.
    pub(crate) fn has_text(&self) -> bool {
        !header_lines(self).is_empty()
    }

    /// The byte offset where the header's last line ends in the source.
    pub(crate) fn end(&self) -> usize {
        let last_range = match self {
            Header::Frontmatter(fields) => fields.last().map(|field| field.range),
            Header::Comment(lines) => lines.last().map(|line| line.range),
        };
        last_range.map_or(0, |range| range.end)
    }
}

/// One non-blank source line of a header comment.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CommentLine {
    /// The line without comment markers; empty for a marker-only line such as `/**`, which
    /// debug mode still highlights but no mode prints.
    pub(crate) text: String,
    /// The comment's part of the source line, highlighted in debug mode.
    pub(crate) range: ByteRange,
}

/// One top-level field of a Markdown file's frontmatter, printed as `key: value`, or as `key`
/// alone for a list or mapping.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FrontmatterField {
    pub(crate) key: String,
    /// The value as a JSON literal (`"text"`, `42`, `true`, `null`), or `None` for a list or
    /// mapping.
    pub(crate) value: Option<String>,
    /// From the key to the end of its value, highlighted in debug mode.
    pub(crate) range: ByteRange,
}

/// One declaration of an outline, before its line range is computed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct OutlineLine {
    /// The declaration as printed after its line range, such as `## Install`.
    pub(crate) text: String,
    /// The whole declaration from its first leading annotation, which becomes the printed line
    /// range.
    pub(crate) range: ByteRange,
    /// The part of the source that debug mode highlights.
    pub(crate) highlight: ByteRange,
    /// How deep the declaration is nested: 0 at the top level, 1 for a class member or a CSS rule
    /// inside a top-level at-rule. Each level indents the printed line by [`INDENT_PER_DEPTH`].
    pub(crate) depth: usize,
}

/// The indentation of one nesting level of outline lines.
const INDENT_PER_DEPTH: &str = "  ";

/// What a glob view prints for one file after its path and line count.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FileSummary {
    Header(Header),
    /// A data file without a header comment summarizes its first fields instead.
    Fields(FieldSummary),
    /// A Markdown file without frontmatter fields is summarized by its first heading, as its outline
    /// line prints it (`# Guide`).
    FirstHeading(String),
    /// No header, fields, or heading to summarize.
    Empty,
}

impl From<Option<Header>> for FileSummary {
    fn from(header: Option<Header>) -> Self {
        header.map_or(FileSummary::Empty, FileSummary::Header)
    }
}

/// A data file's first top-level keys with scalar values, as `{name: "outln", ...}`, which a
/// glob view prints for a data file without a header comment.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FieldSummary {
    /// The TOML table the fields come from, such as `[package]`, when they aren't top-level
    /// keys of the file.
    pub(crate) table: Option<String>,
    /// Each field as its outline line prints it: `name: "outln"`, or `name = "outln"` in TOML.
    pub(crate) fields: Vec<String>,
    pub(crate) shown: FieldsShown,
}

/// Whether a field summary shows every field it could, or more may follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldsShown {
    All,
    /// More fields were found than a summary shows, or the summary read only the start of the
    /// file; printed as `...`.
    First,
}

/// How many fields a file summary shows: a data file's field summary, or a Markdown file's
/// frontmatter fields.
const MAX_SUMMARY_FIELDS: usize = 3;

/// Keys whose value says what the whole file is, which a file summary shows first, in this
/// order.
const DESCRIBING_KEYS: [&str; 2] = ["description", "title"];

/// Whether a field's key is one of the [`DESCRIBING_KEYS`].
pub(crate) fn is_describing_key(key: &str) -> bool {
    DESCRIBING_KEYS.contains(&key)
}

/// The fields a file summary shows: the [`DESCRIBING_KEYS`] first, then the others in source
/// order, at most [`MAX_SUMMARY_FIELDS`], and whether some were left out. `key` gives each
/// field's key.
pub(crate) fn summary_fields<T>(
    fields: impl IntoIterator<Item = T>,
    key: impl Fn(&T) -> &str,
) -> (Vec<T>, FieldsShown) {
    let mut fields: Vec<T> = fields.into_iter().collect();
    // A stable sort, so fields of the same rank keep their source order.
    fields.sort_by_key(|field| {
        DESCRIBING_KEYS
            .iter()
            .position(|describing| *describing == key(field))
            .unwrap_or(DESCRIBING_KEYS.len())
    });
    let shown = if fields.len() > MAX_SUMMARY_FIELDS {
        fields.truncate(MAX_SUMMARY_FIELDS);
        FieldsShown::First
    } else {
        FieldsShown::All
    };
    (fields, shown)
}

/// Printed in a glob view for a file without a header.
const NO_HEADER: &str = "(no header available)";

/// Printed after the shown fields of a [`FieldSummary`] when more may follow.
const MORE_FIELDS: &str = "...";

/// Renders a file mode outline: the path and line count (`src/cli.rs (120 lines)`), the header,
/// then one outline line per declaration.
pub(crate) fn render_outline(path: &str, outline: &Outline, line_index: &LineIndex) -> String {
    let mut text = path_and_line_count(path, line_index.line_count());
    text.push('\n');
    if let Some(header) = &outline.header {
        for line in header_lines(header) {
            text.push_str(&line);
            text.push('\n');
        }
    }
    for line in &outline.lines {
        let indent = INDENT_PER_DEPTH.repeat(line.depth);
        // Writing into a `String` cannot fail.
        let _ = writeln!(
            text,
            "{indent}{} {}",
            line_index.line_range(line.range),
            line.text
        );
    }
    text
}

/// Renders the one line a glob view prints for a file, without its newline: its path, its line
/// count, then its summary, as `src/cli.rs (120 lines): Command-line arguments.`.
pub(crate) fn render_file_summary(
    path: &str,
    line_count: LineCount,
    summary: &FileSummary,
) -> String {
    let printed = match summary {
        FileSummary::Header(Header::Frontmatter(fields)) => Some(frontmatter_summary(fields)),
        FileSummary::Header(header @ Header::Comment(_)) => {
            // A header comment's lines are one sentence wrapped over several lines.
            let lines = header_lines(header);
            (!lines.is_empty()).then(|| lines.join(" "))
        }
        FileSummary::Fields(fields) => Some(render_field_summary(fields)),
        FileSummary::FirstHeading(heading) => Some(heading.clone()),
        FileSummary::Empty => None,
    };
    let mut line = path_and_line_count(path, line_count);
    line.push_str(": ");
    line.push_str(printed.as_deref().unwrap_or(NO_HEADER));
    line
}

/// A file's path and line count, as `src/cli.rs (120 lines)`: the start of both its outline and
/// its file summary, so file mode and glob view print a file's size the same way.
fn path_and_line_count(path: &str, line_count: LineCount) -> String {
    format!("{path} ({line_count})")
}

/// Frontmatter fields as a file summary prints them, picked by [`summary_fields`]:
/// `description: "Guide", draft: true, ...`.
fn frontmatter_summary(fields: &[FrontmatterField]) -> String {
    let (shown_fields, shown) = summary_fields(fields, |field| field.key.as_str());
    let mut parts: Vec<String> = shown_fields.into_iter().map(field_line).collect();
    if shown == FieldsShown::First {
        parts.push(MORE_FIELDS.to_owned());
    }
    parts.join(", ")
}

/// A frontmatter field as printed: `key: value`, or `key` for a list or mapping.
fn field_line(field: &FrontmatterField) -> String {
    match &field.value {
        Some(value) => format!("{}: {value}", field.key),
        None => field.key.clone(),
    }
}

/// A field summary as `[table] {a: 1, b: 2, ...}`.
fn render_field_summary(summary: &FieldSummary) -> String {
    let mut parts: Vec<&str> = summary.fields.iter().map(String::as_str).collect();
    if summary.shown == FieldsShown::First {
        parts.push(MORE_FIELDS);
    }
    let fields = format!("{{{}}}", parts.join(", "));
    match &summary.table {
        Some(table) => format!("{table} {fields}"),
        None => fields,
    }
}

/// The header as printed lines: one per frontmatter field, or one per comment line with text.
pub(crate) fn header_lines(header: &Header) -> Vec<String> {
    match header {
        Header::Frontmatter(fields) => fields.iter().map(field_line).collect(),
        Header::Comment(lines) => lines
            .iter()
            .filter(|line| !line.text.is_empty())
            .map(|line| line.text.clone())
            .collect(),
    }
}

/// Every source range debug mode highlights, in no particular order.
pub(crate) fn highlights(outline: &Outline) -> Vec<ByteRange> {
    let mut ranges: Vec<ByteRange> = match &outline.header {
        None => Vec::new(),
        Some(Header::Frontmatter(fields)) => fields.iter().map(|field| field.range).collect(),
        Some(Header::Comment(lines)) => lines.iter().map(|line| line.range).collect(),
    };
    ranges.extend(outline.lines.iter().map(|line| line.highlight));
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(key: &str, value: &str) -> FrontmatterField {
        FrontmatterField {
            key: key.to_owned(),
            value: Some(value.to_owned()),
            range: ByteRange::new(0, 0),
        }
    }

    #[test]
    fn file_summary_joins_frontmatter_fields() {
        let summary = FileSummary::Header(Header::Frontmatter(vec![
            field("title", "\"Guide\""),
            field("draft", "true"),
        ]));
        assert_eq!(
            render_file_summary("guide.md", LineCount::of("---\n"), &summary),
            "guide.md (1 line): title: \"Guide\", draft: true"
        );
    }

    #[test]
    fn file_summary_without_header_says_so() {
        assert_eq!(
            render_file_summary("a.md", LineCount::of(""), &FileSummary::Empty),
            "a.md (0 lines): (no header available)"
        );
    }

    #[test]
    fn field_summary_prints_its_table_and_whether_more_fields_follow() {
        let summary = FileSummary::Fields(FieldSummary {
            table: Some("[package]".to_owned()),
            fields: vec![
                "name = \"outln\"".to_owned(),
                "edition = \"2024\"".to_owned(),
            ],
            shown: FieldsShown::First,
        });
        assert_eq!(
            render_file_summary("Cargo.toml", LineCount::of("a\nb\n"), &summary),
            "Cargo.toml (2 lines): [package] {name = \"outln\", edition = \"2024\", ...}"
        );
        let all = FileSummary::Fields(FieldSummary {
            table: None,
            fields: vec!["a: 1".to_owned()],
            shown: FieldsShown::All,
        });
        assert_eq!(
            render_file_summary("a.yaml", LineCount::of("a: 1\n"), &all),
            "a.yaml (1 line): {a: 1}"
        );
    }
}
