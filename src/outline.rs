//! Outline data that language engines return, and its rendering as file mode and glob view text.

use std::fmt::Write as _;

use crate::source_text::{ByteRange, LineIndex};

/// The compact structural summary of one source file, as data; [`render_outline`] prints it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Outline {
    pub(crate) header: Option<Header>,
    pub(crate) lines: Vec<OutlineLine>,
}

/// What a file says about itself at its top.
///
/// Markdown files carry frontmatter. The code-language engines will add a header comment
/// variant; every `match` on `Header` then has to handle it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Header {
    Frontmatter(Vec<FrontmatterField>),
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
    /// The whole declaration, which becomes the printed line range.
    pub(crate) range: ByteRange,
    /// The part of the source that debug mode highlights.
    pub(crate) highlight: ByteRange,
}

/// The header a glob view prints for one file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FileSummary(pub(crate) Option<Header>);

/// Printed in a glob view for a file without a header.
const NO_HEADER: &str = "(no header comment available)";

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
        // Writing into a `String` cannot fail.
        let _ = writeln!(text, "{} {}", line_index.line_range(line.range), line.text);
    }
    text
}

/// Renders the one line a glob view prints for a file, without its newline.
pub(crate) fn render_file_summary(path: &str, summary: &FileSummary) -> String {
    let header_text: Vec<String> = summary.0.iter().flat_map(header_lines).collect();
    if header_text.is_empty() {
        return format!("{path}: {NO_HEADER}");
    }
    format!("{path}: {}", header_text.join(", "))
}

/// The header as printed lines, one per frontmatter field.
fn header_lines(header: &Header) -> impl Iterator<Item = String> + '_ {
    match header {
        Header::Frontmatter(fields) => fields
            .iter()
            .map(|field| format!("{}: {}", field.key, field.value)),
    }
}

/// Every source range debug mode highlights, in no particular order.
pub(crate) fn highlights(outline: &Outline) -> Vec<ByteRange> {
    let header_ranges = outline.header.iter().flat_map(|header| match header {
        Header::Frontmatter(fields) => fields.iter().map(|field| field.range),
    });
    let line_ranges = outline.lines.iter().map(|line| line.highlight);
    header_ranges.chain(line_ranges).collect()
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
            "a.md: (no header comment available)"
        );
    }
}
