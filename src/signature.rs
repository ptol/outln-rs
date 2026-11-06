//! Signatures of code declarations, shared by the tree-sitter engines: the text an outline line prints, where its highlight ends, and the outline line they make.

use crate::outline::OutlineLine;
use crate::source_text::ByteRange;

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
    /// The outline line of a declaration with this signature: `range` becomes its line range,
    /// and its highlight runs from `highlight_start` (the declaration's first keyword, after
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
