//! Rust language engine: header comments (`//!` and `/*!` preferred over plain comments) and top-level declarations with their leading annotations, parsed with tree-sitter.

use tree_sitter::Node;

use crate::code_outline::code_outline;
use crate::header_comment::{
    BlockComments, CommentSyntax, DocMarkers, block_comment_end, find_header_comment,
};
use crate::language_engine::LanguageEngine;
use crate::leading_annotations::PrecedingNode;
use crate::outline::{FileSummary, Header, Outline, OutlineLine};
use crate::signature::Signature;
use crate::source_text::{ByteRange, SourceText, after_whitespace, line_end, trim_end};
use crate::syntax_tree::{child_of_kind, has_child_of_kind, one_line_source_before};

/// The language engine for Rust.
pub(crate) struct Rust;

impl LanguageEngine for Rust {
    fn extensions(&self) -> &'static [&'static str] {
        &["rs"]
    }

    fn outline(&self, source: &SourceText<'_>) -> Outline {
        code_outline(
            source,
            &tree_sitter_rust::LANGUAGE.into(),
            header(source.text),
            preceding_node,
            declaration_line,
        )
    }

    fn file_summary(&self, text: &str) -> FileSummary {
        // The header is found line by line, so the glob view never parses a file.
        FileSummary::from(header(text))
    }
}

/// The header comment: the first `//!` or `/*!` comment, or else the first plain comment,
/// among the comments before the first declaration. A shebang and inner attributes
/// (`#![...]`) may come before or between them.
fn header(text: &str) -> Option<Header> {
    find_header_comment(text, COMMENT_SYNTAX, preamble_end)
}

/// Rust's doc comments say what they document, and its block comments nest.
const COMMENT_SYNTAX: CommentSyntax = CommentSyntax::CFamily {
    doc_markers: DocMarkers::InnerAndOuter,
    block_comments: BlockComments::Nested,
};

/// Where the preamble starting at byte `start` ends: a shebang at the start of the file, or an
/// inner attribute `#![...]`, which may span several lines.
fn preamble_end(text: &str, start: usize) -> Option<usize> {
    if !text[start..].starts_with("#!") {
        return None;
    }
    let after_hash_bang = after_whitespace(text, start + "#!".len());
    if text[after_hash_bang..].starts_with('[') {
        return attribute_end(text, after_hash_bang);
    }
    // In Rust, `#!` starts a shebang only on the first line and when no `[` follows it.
    (start == 0).then_some(line_end(text, start))
}

/// Where the attribute whose `[` is at byte `open` ends, after its matching `]`, or `None` if it
/// never closes. Brackets inside string and character literals and comments don't count; this
/// is only as much of Rust's lexer as an attribute needs, because the glob view never parses.
fn attribute_end(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0_usize;
    let mut index = open;
    while let Some(character) = text[index..].chars().next() {
        let rest = &text[index..];
        index += if rest.starts_with("//") {
            line_end(text, index) - index
        } else if rest.starts_with("/*") {
            block_comment_end(text, index, BlockComments::Nested)? - index
        } else if rest.starts_with('"') || is_raw_string_start(rest) {
            string_literal_len(rest)?
        } else if character == '\'' {
            char_literal_len(rest).unwrap_or(1)
        } else {
            match character {
                '[' => depth += 1,
                ']' => depth = depth.checked_sub(1)?,
                _ => {}
            }
            if depth == 0 {
                return Some(index + 1);
            }
            character.len_utf8()
        };
    }
    None
}

/// Whether a raw string literal (`r"..."`, `r#"..."#`) starts at the start of `rest`, rather than
/// a raw identifier (`r#type`).
fn is_raw_string_start(rest: &str) -> bool {
    rest.strip_prefix('r')
        .is_some_and(|raw| raw.trim_start_matches('#').starts_with('"'))
}

/// The length of the string literal at the start of `rest`: `"..."` with `\` escapes, or a raw
/// string `r"..."` or `r#"..."#` without them.
fn string_literal_len(rest: &str) -> Option<usize> {
    if let Some(raw) = rest.strip_prefix('r').filter(|_| is_raw_string_start(rest)) {
        let hashes = raw.len() - raw.trim_start_matches('#').len();
        let closing = format!("\"{}", "#".repeat(hashes));
        let body_start = 1 + hashes + 1;
        return Some(body_start + rest.get(body_start..)?.find(&closing)? + closing.len());
    }
    let mut escaped = false;
    for (offset, character) in rest.char_indices().skip(1) {
        match character {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return Some(offset + 1),
            _ => {}
        }
    }
    None
}

/// The length of the character literal at the start of `rest` (`'['`, `'\''`, `'\u{5d}'`), or
/// `None` when the `'` starts a lifetime instead.
fn char_literal_len(rest: &str) -> Option<usize> {
    let after_quote = &rest[1..];
    let first = after_quote.chars().next()?;
    let literal_len = if first == '\\' {
        // An escape runs to the next `'` after the escaped character: `\n`, `\'`, `\u{5d}`.
        2 + after_quote.get(2..)?.find('\'')?
    } else {
        first.len_utf8()
    };
    after_quote
        .get(literal_len..)?
        .starts_with('\'')
        .then_some(1 + literal_len + 1)
}

/// What a node before a declaration is to its leading annotations: `///` and `/** */` doc
/// comments and `#[...]` attributes belong to it. A `//!` comment or `#![...]` attribute
/// belongs to the enclosing module instead, so it ends the walk.
fn preceding_node(node: Node<'_>, _text: &str) -> PrecedingNode {
    match node.kind() {
        "attribute_item" => PrecedingNode::Attribute,
        "line_comment" | "block_comment" => {
            if node.child_by_field_name("outer").is_some() {
                PrecedingNode::DocComment
            } else if node.child_by_field_name("inner").is_some() {
                PrecedingNode::Other
            } else {
                PrecedingNode::PlainComment
            }
        }
        _ => PrecedingNode::Other,
    }
}

/// Literal node kinds kept as written when source is put on one line, such as the ABI string
/// of `extern "C"`.
const VERBATIM_KINDS: &[&str] = &["string_literal", "raw_string_literal", "char_literal"];

/// The outline line of a top-level node with line range `range`, or `None` for a node that
/// isn't a listed declaration.
fn declaration_line(
    node: Node<'_>,
    range: ByteRange,
    source: &SourceText<'_>,
) -> Option<OutlineLine> {
    let text = source.text;
    let signature_end = signature_end(node)?;
    let signature = Signature {
        text: one_line_source_before(node, signature_end, text, VERBATIM_KINDS),
        // `signature_end` is where the next token starts, so this leaves out the whitespace
        // before a body or `=`.
        highlight_end: trim_end(text, ByteRange::new(node.start_byte(), signature_end)).end,
    };
    // Attributes are siblings of the declaration, so its node starts at its first keyword.
    Some(signature.into_outline_line(range, node.start_byte(), 0))
}

/// Where a declaration's signature ends, or `None` for a node that isn't a listed declaration
/// (a private `use`, a macro invocation, a comment, an attribute, a syntax error).
///
/// The signature is the source up to the body of a function, `impl`, `trait`, `enum`, `union`,
/// braced `struct`, inline `mod`, and `extern` block; up to the `=` of a `const` or `static`, so
/// its type stays and its value goes; up to the name of a `macro_rules!`; and up to the final
/// `;` of anything else (a tuple or unit `struct`, a `type` alias with its type, `mod name;`, a
/// function without a body, `extern crate`, `pub use`).
fn signature_end(node: Node<'_>) -> Option<usize> {
    let body_start = || {
        node.child_by_field_name("body")
            .filter(|body| body.kind() != "ordered_field_declaration_list")
            .map(|body| body.start_byte())
    };
    match node.kind() {
        "function_item" | "impl_item" | "trait_item" | "enum_item" | "union_item"
        | "struct_item" | "mod_item" | "foreign_mod_item" => {
            Some(body_start().unwrap_or_else(|| before_semicolon(node)))
        }
        "const_item" | "static_item" => Some(
            child_of_kind(node, "=")
                .map_or_else(|| before_semicolon(node), |equals| equals.start_byte()),
        ),
        "macro_definition" => node.child_by_field_name("name").map(|name| name.end_byte()),
        "use_declaration" if has_child_of_kind(node, "visibility_modifier") => {
            Some(before_semicolon(node))
        }
        "function_signature_item" | "type_item" | "extern_crate_declaration" => {
            Some(before_semicolon(node))
        }
        _ => None,
    }
}

/// Where the node's `;` starts, or the node's end without one; in the declarations that have
/// one, it is their last token.
fn before_semicolon(node: Node<'_>) -> usize {
    child_of_kind(node, ";").map_or(node.end_byte(), |semicolon| semicolon.start_byte())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outline_lines(text: &str) -> Vec<(String, ByteRange)> {
        Rust.outline(&SourceText::new(text))
            .lines
            .into_iter()
            .map(|line| (line.text, line.range))
            .collect()
    }

    fn signatures(text: &str) -> Vec<String> {
        outline_lines(text)
            .into_iter()
            .map(|(text, _)| text)
            .collect()
    }

    #[test]
    fn every_declaration_kind_prints_its_signature() {
        let cases = [
            ("fn main() {}", "fn main()"),
            (
                "pub(crate) async unsafe fn f<'a, T>(x: &'a T) -> Option<T> where T: Clone { None }",
                "pub(crate) async unsafe fn f<'a, T>(x: &'a T) -> Option<T> where T: Clone",
            ),
            (
                "extern \"C\" fn callback(data: *mut c_void) {}",
                "extern \"C\" fn callback(data: *mut c_void)",
            ),
            ("pub struct Point<T> { x: T }", "pub struct Point<T>"),
            (
                "struct Pair<T>(T, T) where T: Copy;",
                "struct Pair<T>(T, T) where T: Copy",
            ),
            ("struct Unit;", "struct Unit"),
            ("enum Role { Admin }", "enum Role"),
            ("union Bits { n: u32 }", "union Bits"),
            (
                "pub trait Engine: Sync { fn run(&self); }",
                "pub trait Engine: Sync",
            ),
            (
                "impl<T: Display> Render for Wrapper<T> {}",
                "impl<T: Display> Render for Wrapper<T>",
            ),
            ("mod storage;", "mod storage"),
            ("mod tests { fn t() {} }", "mod tests"),
            ("pub const MAX: usize = 3;", "pub const MAX: usize"),
            ("static mut COUNT: u64 = 0;", "static mut COUNT: u64"),
            ("pub type Id = u64;", "pub type Id = u64"),
            ("extern crate alloc;", "extern crate alloc"),
            ("extern \"C\" { fn c_log(code: i32); }", "extern \"C\""),
            (
                "macro_rules! make_id { ($x:expr) => { $x }; }",
                "macro_rules! make_id",
            ),
            (
                "pub use crate::outline::{Outline, OutlineLine};",
                "pub use crate::outline::{Outline, OutlineLine}",
            ),
            ("pub(crate) use a::b;", "pub(crate) use a::b"),
        ];
        for (source, signature) in cases {
            assert_eq!(signatures(source), [signature], "{source}");
        }
    }

    #[test]
    fn private_uses_and_macro_calls_are_not_listed() {
        assert_eq!(
            signatures("use std::io;\nthread_local! { static A: u8 = 1; }\n"),
            [] as [std::string::String; 0]
        );
    }

    #[test]
    fn a_wrapped_signature_prints_on_one_line_without_the_where_clause_comma() {
        let text =
            "pub fn generic<T>(\n    value: T,\n) -> T\nwhere\n    T: Clone,\n{\n    value\n}\n";
        assert_eq!(
            signatures(text),
            ["pub fn generic<T>(value: T) -> T where T: Clone"]
        );
    }

    #[test]
    fn the_line_range_starts_at_the_first_doc_comment_or_attribute_above() {
        let text = "fn a() {}\n/// Docs.\n#[derive(Debug)]\n// Note.\nstruct B;\n";
        assert_eq!(outline_lines(text)[1].1, ByteRange::new(10, text.len() - 1));
    }

    #[test]
    fn attributes_stay_attached_across_blank_lines_but_doc_comments_do_not() {
        let text = "/// Far.\n\n#[cfg(test)]\n\nmod tests {}\n";
        assert_eq!(outline_lines(text)[0].1.start, text.find("#[").unwrap());
    }

    #[test]
    fn plain_comments_inner_docs_and_trailing_comments_do_not_start_a_range() {
        let text = "//! Module.\nconst A: u8 = 1; // One.\n// Plain.\nfn b() {}\n";
        assert_eq!(outline_lines(text)[1].1.start, text.find("fn b").unwrap());
    }

    #[test]
    fn the_header_comment_prefers_inner_docs_and_skips_inner_attributes() {
        let text = "// Copyright.\n#![expect(\n    clippy::x,\n    reason = \"a ] in a string\"\n)]\n//! Docs.\nfn main() {}\n";
        let Some(Header::Comment(lines)) = header(text) else {
            panic!("expected a header comment");
        };
        assert_eq!(lines[0].text, "Docs.");
    }

    #[test]
    fn a_shebang_is_only_on_the_first_line() {
        assert!(preamble_end("#!/usr/bin/env rust-script\n", 0).is_some());
        assert!(preamble_end("\n#!/usr/bin/env rust-script\n", 1).is_none());
        assert_eq!(preamble_end("#! [allow(dead_code)]\n", 0), Some(21));
    }

    #[test]
    fn brackets_in_literals_and_comments_do_not_end_an_inner_attribute() {
        let cases = [
            "#![doc = \"]\"]",
            "#![doc = r#\"\"]\"#]",
            "#![cfg_attr(x, doc = ']')]",
            "#![cfg_attr(x, doc = '\\'')]",
            "#![allow(\n// ]\ndead_code)]",
            "#![allow(/* ] */ dead_code)]",
            "#![allow(/* /* */ ] */ dead_code)]",
            "#![allow(r#type)]",
        ];
        for attribute in cases {
            assert_eq!(
                preamble_end(attribute, 0),
                Some(attribute.len()),
                "{attribute}"
            );
        }
    }

    #[test]
    fn code_after_an_inner_attribute_on_its_line_ends_the_header_search() {
        assert_eq!(
            header("#![allow(dead_code)] fn main() {}\n// Late.\n"),
            None
        );
    }

    #[test]
    fn an_attribute_stays_attached_across_a_plain_comment_and_a_blank_line() {
        let text = "#[cfg(test)]\n// Note.\n\nfn f() {}\n";
        assert_eq!(outline_lines(text)[0].1.start, 0);
    }

    #[test]
    fn a_doc_comment_right_below_the_inner_doc_header_is_attached() {
        let text = "//! Header.\n/// Docs.\nfn f() {}\n";
        assert_eq!(outline_lines(text)[0].1.start, text.find("///").unwrap());
    }

    #[test]
    fn a_blank_line_inside_a_plain_comment_does_not_detach_a_doc_comment() {
        // A `//!` header, so the plain block comment isn't the header comment.
        let text = "//! Header.\n/// Docs.\n/*\nnote\n\n*/\nfn f() {}\n";
        assert_eq!(outline_lines(text)[0].1.start, text.find("///").unwrap());
    }

    #[test]
    fn a_doc_comment_after_another_comment_on_its_line_is_still_attached() {
        let text = "/* Banner. */ /// Docs.\nfn f() {}\n";
        assert_eq!(outline_lines(text)[0].1.start, text.find("///").unwrap());
    }
}
