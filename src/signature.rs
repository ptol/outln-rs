//! Signatures of code declarations, shared by the tree-sitter engines: the text an outline line prints, where its highlight ends, the outline line they make, and where a signature ends before a declaration's body.

use tree_sitter::Node;

use crate::outline::OutlineLine;
use crate::source_text::{ByteRange, trim_end};
use crate::syntax_tree::{child_of_kind, one_line_source_before};

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
    /// its tokens before `end` on one line ([`one_line_source_before`], keeping nodes of the
    /// `verbatim_kinds` as written), highlighted up to the last of them. `end` is usually where
    /// the next token starts, so the highlight leaves out the whitespace before it.
    pub(crate) fn source_before(
        node: Node<'_>,
        end: usize,
        text: &str,
        verbatim_kinds: &[&str],
    ) -> Self {
        Signature {
            text: one_line_source_before(node, end, text, verbatim_kinds),
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
