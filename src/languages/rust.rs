//! Rust language engine: header comments (`//!` and `/*!` preferred over plain comments), top-level declarations with their leading annotations, the members of `impl` blocks and traits and the items of inline modules one level deeper, and runs of `mod name;` on one line, parsed with tree-sitter.

use tree_sitter::Node;

use crate::code_outline::{CodeLanguage, OutlineLevel};
use crate::comment_syntax::{
    BLOCK_COMMENT_OPEN, BlockComments, CommentSyntax, INNER_DOC_BLOCK, INNER_DOC_LINE, LineMarker,
    OUTER_DOC_BLOCK, OUTER_DOC_LINE, block_comment_end,
};
use crate::header_comment::HeaderChoice;
use crate::leading_annotations::{Annotations, PlainComments};
use crate::outline::OutlineLine;
use crate::signature::{before_semicolon, body_start, signature_head};
use crate::source_text::{ByteRange, after_whitespace, line_end, quoted_len};
use crate::syntax_tree::{OneLineRules, child_of_kind, has_child_of_kind, source};

/// The language engine for Rust.
///
/// The header comment is the first `//!` or `/*!` comment, or else the first plain comment that
/// isn't a one-line label right above code, among the comments before the first declaration; a
/// shebang and inner attributes (`#![...]`) may come before or between them.
pub(crate) static RUST: CodeLanguage = CodeLanguage {
    extensions: &["rs"],
    grammar: || tree_sitter_rust::LANGUAGE.into(),
    // Rust's doc comments say what they document, and its block comments nest.
    comment_syntax: CommentSyntax {
        line: Some(LineMarker::Slashes),
        block: Some(BlockComments::Nested),
        doc_markers: &[
            INNER_DOC_LINE,
            OUTER_DOC_LINE,
            INNER_DOC_BLOCK,
            OUTER_DOC_BLOCK,
        ],
    },
    header_choice: HeaderChoice::InnerDocElseFirstPlain,
    preamble_end,
    // `///` and `/** */` doc comments and `#[...]` attributes belong to the declaration below
    // them; a `#![...]` attribute (`inner_attribute_item`), like a `//!` comment, belongs to the
    // enclosing module instead.
    annotations: Annotations {
        comment_kinds: &["line_comment", "block_comment"],
        attribute_kinds: &["attribute_item"],
        plain_comments: PlainComments::SteppedOver,
    },
    // Literals kept as written, such as the ABI string of `extern "C"`.
    // Struct fields and enum variants need commas, so no member list lacks separators.
    one_line: OneLineRules {
        verbatim_kinds: &["string_literal", "raw_string_literal", "char_literal"],
        member_lists: &[],
    },
    declaration_lines,
    summary_text: signature_head,
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
        index += if rest.starts_with(LineMarker::Slashes.text()) {
            line_end(text, index) - index
        } else if rest.starts_with(BLOCK_COMMENT_OPEN) {
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
    quoted_len(rest, '"')
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

/// The outline line of a declaration with line range `range`, then, for an `impl` block, a
/// trait, or an inline module, the declarations in its body one level deeper (members, or a
/// module's items); nothing for a node that isn't a listed declaration.
///
/// Every level lists the same kinds, so a module's items print as top-level ones do, and an
/// `impl` block's or trait's members are its functions, constants, and types.
fn declaration_lines(
    level: &OutlineLevel<'_>,
    node: Node<'_>,
    range: ByteRange,
    lines: &mut Vec<OutlineLine>,
) {
    let Some(signature_end) = signature_end(node) else {
        return;
    };
    let signature = level.signature_before(node, signature_end);
    let module_name = bodiless_module_name(node, range, level.text());
    // Attributes are siblings of the declaration, so its node starts at its first keyword.
    let line = level.outline_line(signature, range, node.start_byte());
    if let Some(name) = module_name
        && let Some(run) = lines
            .last_mut()
            .filter(|previous| continues_module_run(previous, &line, name, level.text()))
    {
        run.text.push_str(MODULE_RUN_SEPARATOR);
        run.text.push_str(name);
        run.range = ByteRange::new(run.range.start, line.range.end);
        run.highlight = ByteRange::new(run.highlight.start, line.highlight.end);
        return;
    }
    lines.push(line);
    if matches!(node.kind(), "impl_item" | "trait_item" | "mod_item")
        && let Some(body) = node.child_by_field_name("body")
    {
        level.declarations_inside(body, lines);
    }
}

/// Between the names of a run of module declarations on one outline line: `mod cli, discovery`.
const MODULE_RUN_SEPARATOR: &str = ", ";

/// The name of a module declaration without a body or leading annotations (`mod cli;`), which
/// may join a run of them on one outline line, or `None` for any other node.
fn bodiless_module_name<'text>(
    node: Node<'_>,
    range: ByteRange,
    text: &'text str,
) -> Option<&'text str> {
    let is_bodiless = node.kind() == "mod_item" && node.child_by_field_name("body").is_none();
    let is_unannotated = range.start == node.start_byte();
    if !(is_bodiless && is_unannotated) {
        return None;
    }
    node.child_by_field_name("name")
        .map(|name| source(name, text))
}

/// Whether `line`, a module declaration without a body named `name`, joins `previous` as the
/// next module of a run: `previous` prints one or more modules declared the same way (same
/// visibility, no leading annotations) at the same depth, and ends with its `;` on the line
/// right above. A list of `mod` lines says nothing one line with their names doesn't, and
/// `lib.rs` files declare dozens.
fn continues_module_run(
    previous: &OutlineLine,
    line: &OutlineLine,
    name: &str,
    text: &str,
) -> bool {
    // A module's own members come after its line too, inside its range: only a module at the
    // same depth that ends before this one can start a run.
    if previous.depth != line.depth || previous.range.end > line.highlight.start {
        return false;
    }
    let Some(head) = line.text.strip_suffix(name) else {
        return false;
    };
    let is_module_run = previous.text.strip_prefix(head).is_some_and(|names| {
        names.split(MODULE_RUN_SEPARATOR).all(|name| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '_' | '#'))
        })
    });
    let gap = &text[previous.range.end..line.highlight.start];
    let is_right_above = text[..previous.range.end].ends_with(';')
        && gap.trim().is_empty()
        && gap.matches('\n').count() == 1;
    previous.range.start == previous.highlight.start && is_module_run && is_right_above
}

/// Where a declaration's signature ends, or `None` for a node that isn't a listed declaration
/// (a private `use`, a macro invocation, a comment, an attribute, a syntax error).
///
/// The signature is the source up to the body of a function, `impl`, `trait`, `enum`, `union`,
/// braced `struct`, inline `mod`, and `extern` block; up to the `=` of a `const` or `static`, so
/// its type stays and its value goes; up to the name of a `macro_rules!`; and up to the final
/// `;` of anything else (a tuple or unit `struct`, a `type` alias with its type, `mod name;`, a
/// function without a body, `extern crate`, `pub use`, a trait's `type Item: Clone;`).
fn signature_end(node: Node<'_>) -> Option<usize> {
    match node.kind() {
        "function_item" | "impl_item" | "trait_item" | "enum_item" | "union_item"
        | "struct_item" | "mod_item" | "foreign_mod_item" => {
            Some(body_start(node, TUPLE_FIELDS).unwrap_or_else(|| before_semicolon(node)))
        }
        "const_item" | "static_item" => Some(
            child_of_kind(node, "=")
                .map_or_else(|| before_semicolon(node), |equals| equals.start_byte()),
        ),
        "macro_definition" => node.child_by_field_name("name").map(|name| name.end_byte()),
        "use_declaration" if has_child_of_kind(node, "visibility_modifier") => {
            Some(before_semicolon(node))
        }
        "function_signature_item"
        | "type_item"
        | "associated_type"
        | "extern_crate_declaration" => Some(before_semicolon(node)),
        _ => None,
    }
}

/// A tuple struct's fields, which tree-sitter-rust calls its body but which are part of its
/// signature: `struct Pair<T>(T, T) where T: Copy`.
const TUPLE_FIELDS: &[&str] = &["ordered_field_declaration_list"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language_engine::{outline_of, outline_texts};
    use crate::outline::Header;

    fn signatures(text: &str) -> Vec<String> {
        outline_texts(&RUST, text)
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
            ("pub trait Engine: Sync {}", "pub trait Engine: Sync"),
            (
                "impl<T: Display> Render for Wrapper<T> {}",
                "impl<T: Display> Render for Wrapper<T>",
            ),
            ("mod storage;", "mod storage"),
            ("mod tests {}", "mod tests"),
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
    fn impl_trait_and_inline_module_bodies_are_listed_one_level_deeper_up_to_depth_two() {
        let text = "\
trait Shape { type Unit: Copy; const SIDES: u8; fn area(&self) -> f64; fn name() {} }
impl Square { pub const ONE: Self = Self(1);
    /// Docs.
    fn new() -> Self { todo!() } mac!(); }
mod outer { use a::b; pub struct S; impl S { fn m() {} } mod inner { fn deepest() {} } }
";
        let outline = outline_of(&RUST, text);
        let lines: Vec<(usize, &str)> = outline
            .lines
            .iter()
            .map(|line| (line.depth, line.text.as_str()))
            .collect();
        assert_eq!(
            lines,
            [
                (0, "trait Shape"),
                (1, "type Unit: Copy"),
                (1, "const SIDES: u8"),
                (1, "fn area(&self) -> f64"),
                (1, "fn name()"),
                (0, "impl Square"),
                (1, "pub const ONE: Self"),
                (1, "fn new() -> Self"),
                (0, "mod outer"),
                (1, "pub struct S"),
                (1, "impl S"),
                (2, "fn m()"),
                (1, "mod inner"),
                (2, "fn deepest()"),
            ]
        );
        let method = &outline.lines[7];
        assert_eq!(method.range.start, text.find("/// Docs.").unwrap());
    }

    #[test]
    fn consecutive_bodiless_modules_declared_alike_share_one_outline_line() {
        let text = "\
mod a;
mod b;
mod r#c;

mod d;
pub mod e;
pub mod f;
#[cfg(test)]
mod g;
mod h;
mod i {}
mod j;
";
        let outline = outline_of(&RUST, text);
        let lines: Vec<&str> = outline
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect();
        assert_eq!(
            lines,
            [
                "mod a, b, r#c",
                "mod d",
                "pub mod e, f",
                "mod g",
                "mod h",
                "mod i",
                "mod j"
            ]
        );
        assert_eq!(
            outline.lines[0].range,
            ByteRange::new(0, "mod a;\nmod b;\nmod r#c;".len())
        );
        assert_eq!(
            outline.lines[0].highlight.end,
            "mod a;\nmod b;\nmod r#c".len()
        );
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
        assert_eq!(
            outline_of(&RUST, text).lines[1].range,
            ByteRange::new(10, text.len() - 1)
        );
    }

    #[test]
    fn attributes_stay_attached_across_blank_lines_but_doc_comments_do_not() {
        let text = "/// Far.\n\n#[cfg(test)]\n\nmod tests {}\n";
        assert_eq!(
            outline_of(&RUST, text).lines[0].range.start,
            text.find("#[").unwrap()
        );
    }

    #[test]
    fn plain_comments_inner_docs_and_trailing_comments_do_not_start_a_range() {
        let text = "//! Module.\nconst A: u8 = 1; // One.\n// Plain.\nfn b() {}\n";
        assert_eq!(
            outline_of(&RUST, text).lines[1].range.start,
            text.find("fn b").unwrap()
        );
    }

    #[test]
    fn the_header_comment_prefers_inner_docs_and_skips_inner_attributes() {
        let text = "// Copyright.\n#![expect(\n    clippy::x,\n    reason = \"a ] in a string\"\n)]\n//! Docs.\nfn main() {}\n";
        let Some(Header::Comment(lines)) = RUST.header(text) else {
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
            RUST.header("#![allow(dead_code)] fn main() {}\n// Late.\n"),
            None
        );
    }

    #[test]
    fn an_attribute_stays_attached_across_a_plain_comment_and_a_blank_line() {
        let text = "#[cfg(test)]\n// Note.\n\nfn f() {}\n";
        assert_eq!(outline_of(&RUST, text).lines[0].range.start, 0);
    }

    #[test]
    fn a_doc_comment_right_below_the_inner_doc_header_is_attached() {
        let text = "//! Header.\n/// Docs.\nfn f() {}\n";
        assert_eq!(
            outline_of(&RUST, text).lines[0].range.start,
            text.find("///").unwrap()
        );
    }

    #[test]
    fn a_blank_line_inside_a_plain_comment_does_not_detach_a_doc_comment() {
        // A `//!` header, so the plain block comment isn't the header comment.
        let text = "//! Header.\n/// Docs.\n/*\nnote\n\n*/\nfn f() {}\n";
        assert_eq!(
            outline_of(&RUST, text).lines[0].range.start,
            text.find("///").unwrap()
        );
    }

    #[test]
    fn a_doc_comment_after_another_comment_on_its_line_is_still_attached() {
        let text = "/* Banner. */ /// Docs.\nfn f() {}\n";
        assert_eq!(
            outline_of(&RUST, text).lines[0].range.start,
            text.find("///").unwrap()
        );
    }
}
