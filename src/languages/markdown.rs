//! Markdown language engine: frontmatter fields and headings with the line ranges of their sections, and the first heading as the glob view's summary without frontmatter fields.

use std::fmt::Write as _;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::language_engine::LanguageEngine;
use crate::outline::{FileSummary, FrontmatterField, Header, Outline, OutlineLine};
use crate::source_text::{ByteRange, SourceText, lines_with_offsets, trim_end};

/// The language engine for `.md` files.
pub(crate) struct Markdown;

impl LanguageEngine for Markdown {
    fn extensions(&self) -> &'static [&'static str] {
        &["md"]
    }

    fn outline(&self, source: &SourceText<'_>) -> Outline {
        let frontmatter = parse_frontmatter(source.text);
        let body_start = frontmatter.as_ref().map_or(0, |found| found.body_start);
        Outline {
            header: frontmatter.and_then(Frontmatter::into_header),
            lines: heading_lines(source, body_start),
        }
    }

    /// The frontmatter, or else the first heading, which a file without frontmatter fields usually
    /// opens with as its title.
    fn file_summary(&self, text: &str) -> FileSummary {
        let frontmatter = parse_frontmatter(text);
        let body_start = frontmatter.as_ref().map_or(0, |found| found.body_start);
        if let Some(header) = frontmatter.and_then(Frontmatter::into_header) {
            return FileSummary::Header(header);
        }
        // Only the first heading is needed, so events after it aren't read (pulldown-cmark
        // still scans the whole body's block structure when the parser is created).
        top_level_headings(&text[body_start..], body_start)
            .next()
            .map_or(FileSummary::Empty, |heading| {
                FileSummary::FirstHeading(heading_text(
                    &text[heading.range.as_range()],
                    heading.level,
                ))
            })
    }
}

/// The line that opens and closes a frontmatter block.
const FRONTMATTER_DELIMITER: &str = "---";

/// A terminated frontmatter block at the start of a file.
struct Frontmatter {
    fields: Vec<FrontmatterField>,
    /// Byte offset of the first line after the closing delimiter.
    body_start: usize,
}

impl Frontmatter {
    /// A block without `key: value` lines prints nothing, so it is no header.
    fn into_header(self) -> Option<Header> {
        (!self.fields.is_empty()).then_some(Header::Frontmatter(self.fields))
    }
}

/// Finds the frontmatter block: `---` on the first line up to the next `---` line.
///
/// Detected by hand rather than with pulldown-cmark's metadata option, because that option
/// also accepts a `---` block in the middle of a document. An unterminated block is body text.
fn parse_frontmatter(text: &str) -> Option<Frontmatter> {
    let mut lines = lines_with_offsets(text);
    let (_, first_line) = lines.next()?;
    if first_line != FRONTMATTER_DELIMITER {
        return None;
    }
    let mut fields = Vec::new();
    for (start, line) in lines {
        if line == FRONTMATTER_DELIMITER {
            let body_start = (start + line.len() + 1).min(text.len());
            return Some(Frontmatter { fields, body_start });
        }
        fields.extend(parse_field(line, start));
    }
    None
}

/// Lines starting with these are never top-level fields: nested YAML and comments.
const NOT_A_FIELD_START: [char; 3] = [' ', '\t', '#'];

/// Parses a top-level `key: value` line; other lines are skipped.
fn parse_field(line: &str, start: usize) -> Option<FrontmatterField> {
    if line.starts_with(NOT_A_FIELD_START) || is_list_item(line) {
        return None;
    }
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some(FrontmatterField {
        key: key.to_owned(),
        value: yaml_scalar_as_json(value),
        range: ByteRange::new(start, start + line.len()),
    })
}

/// A YAML list item: `-` alone or followed by whitespace. `-title: x` is a field named `-title`.
fn is_list_item(line: &str) -> bool {
    line.strip_prefix('-')
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
}

/// Turns a YAML scalar into a JSON literal: quoted text and plain text become JSON strings,
/// numbers and `true`/`false`/`null` stay as they are, and an empty value or `~` is `null`.
///
/// Quoted text is not unescaped: frontmatter fields print as written, not as full YAML reads them.
fn yaml_scalar_as_json(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return "null".to_owned();
    }
    let is_quoted = [('"', '"'), ('\'', '\'')]
        .iter()
        .any(|&(open, close)| value.starts_with(open) && value.ends_with(close));
    if is_quoted {
        // A lone quote character is both the opening and the closing quote.
        return json_string(value.get(1..value.len() - 1).unwrap_or(""));
    }
    let lowercase = value.to_lowercase();
    match lowercase.as_str() {
        "true" | "false" | "null" => lowercase,
        "~" => "null".to_owned(),
        _ if is_decimal_number(value) => value.to_owned(),
        _ => json_string(value),
    }
}

/// Matches `-?\d+(\.\d+)?` with ASCII digits.
fn is_decimal_number(value: &str) -> bool {
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, "1"));
    let all_digits =
        |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    all_digits(whole) && all_digits(fraction)
}

/// Quotes text as a JSON string, escaping what JSON requires.
fn json_string(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for character in text.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            '\u{8}' => quoted.push_str("\\b"),
            '\u{c}' => quoted.push_str("\\f"),
            control if control < ' ' => {
                // Writing into a `String` cannot fail.
                let _ = write!(quoted, "\\u{:04x}", u32::from(control));
            }
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

/// A heading found by the parser, before its section is computed.
struct Heading {
    level: usize,
    /// The heading itself: one line for `#` headings, text and underline for `===`/`---` ones.
    range: ByteRange,
}

/// One outline line per heading of the body, each spanning its section.
///
/// A section ends before the next heading of the same or a higher level, without trailing
/// blank lines. Headings inside block quotes and lists aren't document sections, so they're
/// skipped.
fn heading_lines(source: &SourceText<'_>, body_start: usize) -> Vec<OutlineLine> {
    let text = source.text;
    let headings: Vec<Heading> = top_level_headings(&text[body_start..], body_start).collect();
    headings
        .iter()
        .enumerate()
        .map(|(index, heading)| {
            let line_start = source.line_around(heading.range.start).start;
            let section_end = headings[index + 1..]
                .iter()
                .find(|next| next.level <= heading.level)
                .map_or(text.len(), |next| {
                    source.line_around(next.range.start).start
                });
            OutlineLine {
                text: heading_text(&text[heading.range.as_range()], heading.level),
                range: trim_end(text, ByteRange::new(line_start, section_end)),
                highlight: trim_end(text, ByteRange::new(line_start, heading.range.end)),
                depth: 0,
            }
        })
        .collect()
}

/// Top-level headings of `body`, with ranges shifted by `offset` into the whole text.
///
/// An iterator, so a caller that needs only the first heading reads no events after it.
fn top_level_headings(body: &str, offset: usize) -> impl Iterator<Item = Heading> {
    let mut container_depth: usize = 0;
    Parser::new_ext(body, Options::empty())
        .into_offset_iter()
        .filter_map(move |(event, range)| {
            if let Event::Start(Tag::Heading { level, .. }) = event {
                return (container_depth == 0).then(|| Heading {
                    level: level as usize,
                    range: ByteRange::new(range.start, range.end).shifted(offset),
                });
            }
            if matches!(event, Event::Start(Tag::BlockQuote(_) | Tag::List(_))) {
                container_depth += 1;
            } else if matches!(event, Event::End(TagEnd::BlockQuote(_) | TagEnd::List(_))) {
                container_depth = container_depth.saturating_sub(1);
            }
            None
        })
}

/// The heading as printed: `#` repeated for its level, then its text without markup syntax.
///
/// The text is the raw source, so inline markup such as `code` keeps its backticks.
fn heading_text(source: &str, level: usize) -> String {
    let hashes = "#".repeat(level);
    let title = atx_title(source).unwrap_or_else(|| setext_title(source));
    if title.is_empty() {
        hashes
    } else {
        format!("{hashes} {title}")
    }
}

/// The text of a `# Title #` heading, or `None` for a setext (underlined) heading.
fn atx_title(source: &str) -> Option<String> {
    let line = source.lines().next()?.trim_start();
    let after_hashes = line.trim_start_matches('#');
    let hash_count = line.len() - after_hashes.len();
    // Seven or more `#` aren't an ATX opener: such a line can be a setext heading's text.
    let is_atx = (1..=MAX_HEADING_LEVEL).contains(&hash_count)
        && (after_hashes.is_empty() || after_hashes.starts_with([' ', '\t']));
    is_atx.then(|| strip_closing_hashes(after_hashes).to_owned())
}

/// The deepest heading level, `######`.
const MAX_HEADING_LEVEL: usize = 6;

/// The text of a heading underlined with `===` or `---`, its lines joined with spaces.
fn setext_title(source: &str) -> String {
    let mut lines: Vec<&str> = source.trim_end().lines().collect();
    lines.pop();
    lines
        .iter()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Removes an optional closing `#` sequence, which must follow whitespace or be the whole text.
fn strip_closing_hashes(text: &str) -> &str {
    let trimmed = text.trim_end();
    let without_hashes = trimmed.trim_end_matches('#');
    let has_closing_hashes = without_hashes.len() < trimmed.len()
        && (without_hashes.is_empty() || without_hashes.ends_with(char::is_whitespace));
    if has_closing_hashes {
        without_hashes.trim()
    } else {
        trimmed.trim()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language_engine::outline_texts;

    #[test]
    fn yaml_scalars_become_json_literals() {
        let cases = [
            ("", "null"),
            (" ~ ", "null"),
            ("True", "true"),
            ("42", "42"),
            ("-1.5", "-1.5"),
            ("1.", "\"1.\""),
            ("'single'", "\"single\""),
            ("\"say \\\"hi\\\"\"", "\"say \\\\\\\"hi\\\\\\\"\""),
            ("\"", "\"\""),
            ("[a, b]", "\"[a, b]\""),
        ];
        for (yaml, json) in cases {
            assert_eq!(yaml_scalar_as_json(yaml), json, "YAML {yaml:?}");
        }
    }

    #[test]
    fn frontmatter_keeps_top_level_fields_only() {
        let summary = Markdown.file_summary(
            "---\ntitle: Guide\n  nested: x\nno colon\n: no key\n- item: x\n-\n# note: x\n-flag: on\n---\n# Body\n",
        );
        let FileSummary::Header(Header::Frontmatter(fields)) = summary else {
            panic!("expected frontmatter, got {summary:?}");
        };
        let keys: Vec<&str> = fields.iter().map(|field| field.key.as_str()).collect();
        assert_eq!(keys, ["title", "-flag"]);
        assert_eq!(fields[0].range, ByteRange::new(4, 16));
    }

    #[test]
    fn unterminated_frontmatter_is_body_text() {
        assert_eq!(
            Markdown.file_summary("---\ntitle: x\n# Body\n"),
            FileSummary::FirstHeading("# Body".to_owned())
        );
    }

    #[test]
    fn without_frontmatter_fields_the_first_section_heading_is_the_summary() {
        let first_heading = |text: &str| {
            let FileSummary::FirstHeading(heading) = Markdown.file_summary(text) else {
                return None;
            };
            Some(heading)
        };
        assert_eq!(
            first_heading("> # Quoted\n\nGuide\n=====\n# Later\n"),
            Some("# Guide".to_owned())
        );
        assert_eq!(
            first_heading("---\n---\n## Usage ##\n"),
            Some("## Usage".to_owned())
        );
        assert_eq!(Markdown.file_summary("Just text.\n"), FileSummary::Empty);
    }

    #[test]
    fn headings_in_code_html_quotes_and_lists_are_skipped() {
        let text = "# One\n~~~\n```\n# fenced\n~~~\n    # indented\n<!--\n# html\n-->\n> # quoted\n- # listed\n## Two ##\n";
        assert_eq!(outline_texts(&Markdown, text), ["# One", "## Two"]);
    }

    #[test]
    fn setext_headings_get_hashes_for_their_level() {
        assert_eq!(
            outline_texts(
                &Markdown,
                "Title\n=====\n\nSub\n---\n\n####### Seven\n---\n"
            ),
            ["# Title", "## Sub", "## ####### Seven"]
        );
    }

    #[test]
    fn closing_hashes_need_whitespace_before_them() {
        assert_eq!(
            outline_texts(&Markdown, "# C# ##\n# #\n# a#b\n"),
            ["# C#", "#", "# a#b"]
        );
    }
}
