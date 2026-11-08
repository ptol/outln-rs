//! The paragraph a header comment prints: its first prose paragraph, after code fences, badges, link definitions, and HTML tags; none for a comment that is a license notice.
//!
//! Doc comments are Markdown, so a crate's `//!` comment may open with a row of badges or a
//! code example before its summary; a license banner says nothing about what the file is for.

use crate::outline::CommentLine;

/// The lines of the comment `lines` that make its header: its first prose paragraph, or `None`
/// when that paragraph is a license notice ([`is_license_notice`]) or the comment has text but
/// no prose. A comment of markers only (`/**/`) is kept whole, so it still counts as a header
/// without text.
///
/// Lines without text before and after the paragraph stay when nothing with text was left out
/// on that side, so debug mode still highlights a `/**` and its `*/`.
pub(super) fn header_paragraph(mut lines: Vec<CommentLine>) -> Option<Vec<CommentLine>> {
    let texts: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
    let paragraphs = paragraphs(&texts);
    if paragraphs.is_empty() {
        return Some(lines);
    }
    let chosen_index = paragraphs
        .iter()
        .position(|paragraph| paragraph.kind == ParagraphKind::Prose)?;
    let chosen = &paragraphs[chosen_index];
    if is_license_notice(&texts[chosen.start..chosen.end]) {
        return None;
    }
    let keep_from = if chosen_index == 0 { 0 } else { chosen.start };
    let keep_to = if chosen_index + 1 == paragraphs.len() {
        lines.len()
    } else {
        chosen.end
    };
    lines.truncate(keep_to);
    lines.drain(..keep_from);
    Some(lines)
}

/// One paragraph of a comment: lines `start..end` of it.
struct Paragraph {
    start: usize,
    end: usize,
    kind: ParagraphKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParagraphKind {
    Prose,
    /// A fenced code block, blank lines inside it included.
    CodeFence,
    /// Lines that are each a row of badges, a link reference definition, or an HTML tag.
    Markup,
}

/// The paragraphs of a comment whose lines have the texts `texts`: runs of lines with text,
/// separated by lines without, except that a fenced code block runs to its closing fence.
fn paragraphs(texts: &[&str]) -> Vec<Paragraph> {
    let mut paragraphs = Vec::new();
    let mut index = 0;
    while index < texts.len() {
        let text = texts[index];
        if text.is_empty() {
            index += 1;
            continue;
        }
        let start = index;
        let (end, kind) = if let Some(fence) = Fence::opening(text) {
            // An unclosed fence runs to the comment's end, as in Markdown.
            let close = texts[start + 1..]
                .iter()
                .position(|line| fence.is_closed_by(line))
                .map_or(texts.len(), |offset| start + 1 + offset + 1);
            (close, ParagraphKind::CodeFence)
        } else {
            let end = texts[start..]
                .iter()
                .position(|line| line.is_empty())
                .map_or(texts.len(), |offset| start + offset);
            let is_markup = texts[start..end].iter().all(|line| is_markup_line(line));
            let kind = if is_markup {
                ParagraphKind::Markup
            } else {
                ParagraphKind::Prose
            };
            (end, kind)
        };
        paragraphs.push(Paragraph { start, end, kind });
        index = end;
    }
    paragraphs
}

/// The fence that opens a fenced code block: a run of at least three backticks or tildes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fence {
    character: char,
    length: usize,
}

/// How many backticks or tildes a fence has at least.
const MIN_FENCE_LENGTH: usize = 3;

impl Fence {
    /// The fence at the start of a line (` ``` `, `~~~~`), with any info string after it, or
    /// `None`.
    fn opening(text: &str) -> Option<Self> {
        let character = text
            .chars()
            .next()
            .filter(|&first| matches!(first, '`' | '~'))?;
        let length = text.len() - text.trim_start_matches(character).len();
        (length >= MIN_FENCE_LENGTH).then_some(Fence { character, length })
    }

    /// Whether a line closes the code block this fence opens: a run of the same character at
    /// least as long as the opening one, with nothing after it, as Markdown's spec
    /// (`CommonMark`) says, so a ` ``` ` line inside a ` ```` ` block is code.
    fn is_closed_by(self, text: &str) -> bool {
        let rest = text.trim_start_matches(self.character);
        text.len() - rest.len() >= self.length && rest.trim().is_empty()
    }
}

/// Whether a line is markup rather than prose: badges only (`[![crates.io](…)](…)`), a link
/// reference definition (`[docs]: https://…`), or HTML tags only (`<br>`, `<p align="center">`).
fn is_markup_line(text: &str) -> bool {
    is_link_definition(text) || is_html_tags_only(text) || is_badges_only(text)
}

fn is_link_definition(text: &str) -> bool {
    text.starts_with('[')
        && text
            .find("]:")
            .is_some_and(|close| !text[1..close].contains(']'))
}

/// Whether a line is HTML tags and whitespace only: `<br>` is, `<b>Purpose.</b>` isn't, since
/// its text prints.
fn is_html_tags_only(text: &str) -> bool {
    let mut rest = text.trim_start();
    if rest.is_empty() {
        return false;
    }
    while !rest.is_empty() {
        let Some(after_tag) = rest
            .strip_prefix('<')
            .and_then(|tag| tag.split_once('>'))
            .map(|(_, after)| after)
        else {
            return false;
        };
        rest = after_tag.trim_start();
    }
    true
}

/// Whether a line holds nothing but Markdown images and links around images, separated by
/// whitespace, `|`, or space entities (`&ensp;`), as a row of badges does.
fn is_badges_only(text: &str) -> bool {
    const SEPARATORS: [&str; 4] = ["&ensp;", "&emsp;", "&nbsp;", "|"];
    let mut rest = text;
    let mut found_badge = false;
    loop {
        rest = rest.trim_start();
        if let Some(separator) = SEPARATORS.iter().find(|&&s| rest.starts_with(s)) {
            rest = &rest[separator.len()..];
            continue;
        }
        if rest.is_empty() {
            return found_badge;
        }
        let Some(after) = after_badge(rest) else {
            return false;
        };
        found_badge = true;
        rest = after;
    }
}

/// The text after the badge at the start of `text`: an image `![alt](url)`, or a link whose
/// label is an image, `[![alt](url)](url)`; either may use a reference (`[![github]][1]`) or
/// none (`[![github]]`). `None` if no badge starts there.
fn after_badge(text: &str) -> Option<&str> {
    let label_start = if text.starts_with("![") {
        1
    } else if text.starts_with("[![") {
        0
    } else {
        return None;
    };
    let after_label = &text[label_start + bracketed_len(&text[label_start..], '[', ']')?..];
    let destination_len = match after_label.chars().next() {
        Some('(') => bracketed_len(after_label, '(', ')')?,
        Some('[') => bracketed_len(after_label, '[', ']')?,
        _ => 0,
    };
    Some(&after_label[destination_len..])
}

/// The length of the bracketed group at the start of `text`, which starts with `open`, up to
/// and including its matching `close`; `None` if it never closes.
fn bracketed_len(text: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0_usize;
    for (index, character) in text.char_indices() {
        if character == open {
            depth += 1;
        } else if character == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index + close.len_utf8());
            }
        }
    }
    None
}

/// How license notices start, in lowercase: the paragraph's first line is compared ignoring
/// ASCII case.
const LICENSE_NOTICE_STARTS: [&str; 14] = [
    "copyright",
    "(c)",
    "©",
    "spdx-license-identifier",
    "@license",
    "@copyright",
    "licensed under",
    "licensed to",
    "license:",
    "this source code form",
    "permission is hereby granted",
    "the mit license",
    "mit license",
    "all rights reserved",
];

/// Whether a paragraph is a license notice rather than a summary: its first line starts like
/// one (`Copyright 2024 …`, `SPDX-License-Identifier: MIT`, `Licensed under …`), or it says
/// which project the file is part of and where its license is (`This file is part of ICU4X.
/// For terms of use, please see the file called LICENSE …`).
fn is_license_notice(paragraph: &[&str]) -> bool {
    let Some(first) = paragraph.first() else {
        return false;
    };
    let starts_with = |prefix: &str| {
        first
            .get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
    };
    if LICENSE_NOTICE_STARTS.into_iter().any(starts_with) {
        return true;
    }
    starts_with("this file is part of")
        && paragraph
            .iter()
            .any(|line| line.to_ascii_lowercase().contains("licen"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_text::ByteRange;

    /// The printed texts of the header paragraph of a comment with lines `texts`.
    fn header_texts(texts: &[&str]) -> Option<Vec<String>> {
        let lines = texts
            .iter()
            .map(|text| CommentLine {
                text: (*text).to_owned(),
                range: ByteRange::new(0, 1),
            })
            .collect();
        Some(
            header_paragraph(lines)?
                .into_iter()
                .map(|line| line.text)
                .collect(),
        )
    }

    fn owned(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|&text| text.to_owned()).collect()
    }

    #[test]
    fn badges_link_definitions_and_html_tags_are_skipped() {
        let comment = [
            "[![github]](https://github.com/a/b)&ensp;[![crates-io]](https://crates.io/b)",
            "",
            "[github]: https://img.shields.io/badge/github",
            "",
            "<br>",
            "",
            "Fast parsing.",
            "",
            "More.",
        ];
        assert_eq!(header_texts(&comment), Some(owned(&["Fast parsing."])));
        let images = ["![logo](logo.svg) | [![ci](ci.svg)][1]", "", "Summary."];
        assert_eq!(header_texts(&images), Some(owned(&["Summary."])));
        let mixed = ["[![ci](ci.svg)](ci) Builds.", "", "Summary."];
        assert_eq!(
            header_texts(&mixed),
            Some(owned(&["[![ci](ci.svg)](ci) Builds."]))
        );
    }

    #[test]
    fn a_code_fence_is_skipped_blank_lines_inside_it_included() {
        let comment = [
            "```compile_fail,E0277",
            "let a = 1;",
            "",
            "a();",
            "```",
            "",
            "Docs.",
        ];
        assert_eq!(header_texts(&comment), Some(owned(&["Docs."])));
        assert_eq!(header_texts(&["```", "code", "```"]), None);
    }

    #[test]
    fn a_fence_closes_only_with_its_character_at_its_length_or_longer() {
        let longer_fence = ["````", "```", "````", "", "Docs."];
        assert_eq!(header_texts(&longer_fence), Some(owned(&["Docs."])));
        let tilde_inside = ["```", "~~~", "```", "", "Docs."];
        assert_eq!(header_texts(&tilde_inside), Some(owned(&["Docs."])));
        let info_after_close = ["```", "```rust", "```", "", "Docs."];
        assert_eq!(header_texts(&info_after_close), Some(owned(&["Docs."])));
        let closed_longer = ["```", "code", "`````", "", "Docs."];
        assert_eq!(header_texts(&closed_longer), Some(owned(&["Docs."])));
    }

    #[test]
    fn a_line_with_text_between_html_tags_is_prose() {
        assert_eq!(
            header_texts(&["<b>Purpose.</b>"]),
            Some(owned(&["<b>Purpose.</b>"]))
        );
        assert_eq!(
            header_texts(&[
                "<p align=\"center\"> <img src=\"logo.svg\"> </p>",
                "",
                "Docs."
            ]),
            Some(owned(&["Docs."]))
        );
        assert_eq!(header_texts(&["<b", "", "Docs."]), Some(owned(&["<b"])));
    }

    #[test]
    fn a_license_notice_is_no_header() {
        for first in [
            "Copyright 2024 The Authors.",
            "(c) Someone",
            "SPDX-License-Identifier: MIT",
            "Licensed under the Apache License, Version 2.0",
            "This Source Code Form is subject to the terms of the Mozilla Public",
            "@license React v17.0.2",
        ] {
            assert_eq!(header_texts(&[first, "", "Docs."]), None, "{first}");
        }
        let part_of = [
            "This file is part of ICU4X. For terms of use, please see the file",
            "called LICENSE.",
        ];
        assert_eq!(header_texts(&part_of), None);
        assert_eq!(
            header_texts(&["This file is part of the parser."]),
            Some(owned(&["This file is part of the parser."]))
        );
    }

    #[test]
    fn lines_without_text_stay_only_on_a_side_with_nothing_left_out() {
        assert_eq!(
            header_texts(&["", "One.", ""]),
            Some(owned(&["", "One.", ""]))
        );
        assert_eq!(
            header_texts(&["", "<br>", "", "One.", ""]),
            Some(owned(&["One.", ""]))
        );
        assert_eq!(header_texts(&["", ""]), Some(owned(&["", ""])));
    }
}
