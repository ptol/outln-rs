//! Signatures of code declarations, shared by the tree-sitter engines: the text an outline line prints, where its highlight ends, the outline line they make, and where a signature ends before a declaration's body.

use tree_sitter::Node;

use crate::outline::OutlineLine;
use crate::source_text::{ByteRange, quoted_len, trim_end};
use crate::syntax_tree::{OneLineRules, child_of_kind, one_line_source_before};

/// A declaration's signature, such as `function parse(input: string): Ast` or `pub struct
/// Cache<K>`.
pub(crate) struct Signature {
    /// The printed text, on one line.
    pub(crate) text: String,
    /// Byte offset where the signature's source ends, before any body; debug mode's highlight
    /// ends here.
    pub(crate) highlight_end: usize,
}

impl Signature {
    /// The signature made of the node's source up to byte `end`, such as where its body starts:
    /// its tokens before `end` on one line ([`one_line_source_before`], by the language's
    /// `rules`), highlighted up to the last of them. `end` is usually where the next token
    /// starts, so the highlight leaves out the whitespace before it.
    pub(crate) fn source_before(
        node: Node<'_>,
        end: usize,
        text: &str,
        rules: &OneLineRules,
    ) -> Self {
        Signature {
            text: one_line_source_before(node, end, text, rules),
            highlight_end: trim_end(text, ByteRange::new(node.start_byte(), end)).end,
        }
    }

    /// The outline line of a declaration with this signature: `range` becomes its line range,
    /// and its highlight runs from `highlight_start` (the declaration's first token, after
    /// its leading annotations) to the end of the signature.
    pub(crate) fn into_outline_line(
        self,
        range: ByteRange,
        highlight_start: usize,
        depth: usize,
    ) -> OutlineLine {
        OutlineLine {
            text: self.text,
            range,
            highlight: ByteRange::new(highlight_start, self.highlight_end.max(highlight_start)),
            depth,
        }
    }
}

/// Where the node's body (its `body` field) starts, or `None` without one. A body of one of the
/// `signature_kinds` is part of the signature instead, such as the field list of a Rust tuple
/// struct, which the grammar also calls its body.
pub(crate) fn body_start(node: Node<'_>, signature_kinds: &[&str]) -> Option<usize> {
    node.child_by_field_name("body")
        .filter(|body| !signature_kinds.contains(&body.kind()))
        .map(|body| body.start_byte())
}

/// Where the node's `;` starts, or the node's end without one: a declaration without a body
/// (`mod name;`, `type Id = u64;`) ends its signature there.
pub(crate) fn before_semicolon(node: Node<'_>) -> usize {
    child_of_kind(node, ";").map_or(node.end_byte(), |semicolon| semicolon.start_byte())
}

/// The start of a code declaration's outline line, up to its parameter list, which a
/// declaration summary prints: `pub fn parse` for `pub fn parse<T>(input: T) -> Ast`, and
/// `export const shape` for `export const shape = (input: Shape): void`. Type parameters right
/// before the parameter list go too. A line without a parameter list is kept whole, and so is a
/// test block, whose parentheses hold its title (`describe('parser')`).
///
/// The parameter list is the first `(` outside type parameters, whose bounds may hold
/// parentheses of their own: `fn f<F: Fn()>(f: F)` and
/// `function f<T extends (x: number) => void>(a: T)` both cut before `(`.
pub(crate) fn signature_head(text: &str) -> &str {
    // Rust's `pub(crate)` and `pub(in path)` are visibilities, not parameter lists.
    let search_from = text
        .strip_prefix("pub(")
        .and_then(|rest| rest.find(')'))
        .map_or(0, |close| "pub(".len() + close + 1);
    let Some(open) = parameter_list_start(&text[search_from..]).map(|index| search_from + index)
    else {
        return text;
    };
    if text[open + 1..].starts_with(['\'', '"', '`']) {
        return text;
    }
    let head = without_trailing_type_parameters(text[..open].trim_end());
    head.strip_suffix('=').map_or(head, str::trim_end)
}

/// Where the first `(` outside `<…>` groups is in `text`, or `None`.
fn parameter_list_start(text: &str) -> Option<usize> {
    with_angle_depth(text)
        .find(|&(_, character, depth)| character == '(' && depth == 0)
        .map(|(index, _, _)| index)
}

/// `head` without the `<…>` group it ends with, if any: `fn parse<T>` becomes `fn parse`.
fn without_trailing_type_parameters(head: &str) -> &str {
    if !head.ends_with('>') || head.ends_with("->") || head.ends_with("=>") {
        return head;
    }
    let mut group_start = None;
    let mut depth_at_end = 0;
    for (index, character, depth) in with_angle_depth(head) {
        if character == '<' && depth == 1 {
            group_start = Some(index);
        }
        depth_at_end = depth;
    }
    match group_start {
        Some(start) if depth_at_end == 0 => head[..start].trim_end(),
        Some(_) | None => head,
    }
}

/// Each character of `text` outside string literals with its byte offset and how many `<…>`
/// groups are open after it. The `>` of `->` and `=>` (a Rust `Fn() -> T` bound, a TypeScript
/// function type) closes no group, and the `<` of a string-literal type (`T extends "<"`)
/// opens none.
fn with_angle_depth(text: &str) -> impl Iterator<Item = (usize, char, usize)> {
    let mut depth = 0_usize;
    let mut previous = None;
    let mut string_end = 0;
    text.char_indices().filter_map(move |(index, character)| {
        if index < string_end {
            return None;
        }
        if let Some(length) = string_literal_len(&text[index..], character) {
            string_end = index + length;
            previous = None;
            return None;
        }
        match character {
            '<' => depth += 1,
            '>' if !matches!(previous, Some('-' | '=')) => depth = depth.saturating_sub(1),
            _ => {}
        }
        previous = Some(character);
        Some((index, character, depth))
    })
}

/// The length of the string literal `rest` starts with, quotes included, if `first` (its first
/// character) opens one: `"…"` and `` `…` `` strings, and a `'…'` one only when it closes before
/// any whitespace, `,` or `>`, so a Rust lifetime (`<'a, 'b>`, `&'a str`) isn't one.
fn string_literal_len(rest: &str, first: char) -> Option<usize> {
    if !matches!(first, '"' | '\'' | '`') {
        return None;
    }
    let length = quoted_len(rest, first)?;
    let is_lifetime = first == '\''
        && rest[1..length - 1].contains(|c: char| c.is_whitespace() || matches!(c, ',' | '>'));
    (!is_lifetime).then_some(length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_head_stops_before_the_parameter_list_and_its_type_parameters() {
        let cases = [
            ("pub fn parse<T: Read>(input: T) -> Ast", "pub fn parse"),
            (
                "pub(crate) fn is_set(&self) -> bool",
                "pub(crate) fn is_set",
            ),
            (
                "export const shape = <T>(input: T): void",
                "export const shape",
            ),
            (
                "async function load(id: string): Promise<User>",
                "async function load",
            ),
            (
                "impl<T: Display> Render for Wrapper<T>",
                "impl<T: Display> Render for Wrapper<T>",
            ),
            ("describe('parser')", "describe('parser')"),
            ("function f<T extends \"<\">(x: T)", "function f"),
            ("function g<T extends '<'>(x: T)", "function g"),
            ("fn borrow<'a, 'b: 'a>(x: &'a str) -> &'b str", "fn borrow"),
            ("fn lifetime<'a>(x: &'a str)", "fn lifetime"),
            ("pub struct Pair<T>(T, T)", "pub struct Pair"),
            ("pub(in crate::a) struct S", "pub(in crate::a) struct S"),
            ("fn f<F: Fn()>(f: F)", "fn f"),
            ("fn f<F: Fn(u8) -> u8>(f: F) -> u8", "fn f"),
            (
                "function f<T extends (x: number) => void>(a: T)",
                "function f",
            ),
            (
                "impl<F: Fn()> Handler for Wrapper<F>",
                "impl<F: Fn()> Handler for Wrapper<F>",
            ),
        ];
        for (text, head) in cases {
            assert_eq!(signature_head(text), head, "{text}");
        }
    }
}
