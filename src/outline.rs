//! Outline data that language engines return, and its rendering as file mode and glob view text.

use std::fmt::Write as _;

use crate::source_text::{ByteRange, LineIndex};

/// The compact structural summary of one source file, as data; [`render_outline`] prints it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Outline {
    pub(crate) header: Option<Header>,
    pub(crate) lines: Vec<OutlineLine>,
}

/// What a file says about itself at its top: frontmatter for Markdown, a header comment for code.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Header {
    Frontmatter(Vec<FrontmatterField>),
    Comment(Vec<CommentLine>),
}

impl Header {
    /// The byte offset where the header's last line ends in the source.
    pub(crate) fn end(&self) -> usize {
        let last_range = match self {
            Header::Frontmatter(fields) => fields.last().map(|field| field.range),
            Header::Comment(lines) => lines.last().map(|line| line.range),
        };
        last_range.map_or(0, |range| range.end)
    }

    /// What a glob view puts between the header's printed lines: fields are a list, while
    /// comment lines are one sentence wrapped over several lines.
    fn summary_separator(&self) -> &'static str {
        match self {
            Header::Frontmatter(_) => ", ",
            Header::Comment(_) => " ",
        }
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

/// One top-level `key: value` line of a Markdown file's frontmatter.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FrontmatterField {
    pub(crate) key: String,
    /// The value as a JSON literal (`"text"`, `42`, `true`, `null`).
    pub(crate) value: String,
    /// The whole source line, highlighted in debug mode.
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
    /// How deep the declaration is nested: 0 at the top level, 1 for a class member. Each level
    /// indents the printed line by [`INDENT_PER_DEPTH`].
    pub(crate) depth: usize,
}

/// The indentation of one nesting level of outline lines.
const INDENT_PER_DEPTH: &str = "  ";

/// The header a glob view prints for one file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FileSummary(pub(crate) Option<Header>);

/// Printed in a glob view for a file without a header.
const NO_HEADER: &str = "(no header available)";

/// Renders a file mode outline: the path, the header, then one outline line per declaration.
pub(crate) fn render_outline(path: &str, outline: &Outline, line_index: &LineIndex) -> String {
    let mut text = format!("{path}\n");
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

/// Renders the one line a glob view prints for a file, without its newline.
pub(crate) fn render_file_summary(path: &str, summary: &FileSummary) -> String {
    let printed = summary.0.as_ref().and_then(|header| {
        let lines = header_lines(header);
        (!lines.is_empty()).then(|| lines.join(header.summary_separator()))
    });
    format!("{path}: {}", printed.as_deref().unwrap_or(NO_HEADER))
}

/// The header as printed lines: one per frontmatter field, or one per comment line with text.
fn header_lines(header: &Header) -> Vec<String> {
    match header {
        Header::Frontmatter(fields) => fields
            .iter()
            .map(|field| format!("{}: {}", field.key, field.value))
            .collect(),
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
            value: value.to_owned(),
            range: ByteRange::new(0, 0),
        }
    }

    #[test]
    fn file_summary_joins_frontmatter_fields() {
        let summary = FileSummary(Some(Header::Frontmatter(vec![
            field("title", "\"Guide\""),
            field("draft", "true"),
        ])));
        assert_eq!(
            render_file_summary("guide.md", &summary),
            "guide.md: title: \"Guide\", draft: true"
        );
    }

    #[test]
    fn file_summary_without_header_says_so() {
        assert_eq!(
            render_file_summary("a.md", &FileSummary(None)),
            "a.md: (no header available)"
        );
    }
}
