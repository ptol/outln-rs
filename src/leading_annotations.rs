//! Leading annotations, shared by the code-language engines: the doc comments, attributes, and decorators directly above a declaration, where its line range starts.
//!
//! Grammars parse these as siblings before the declaration (tree-sitter-rust's attributes and
//! comments, the TypeScript grammar's class member decorators, every grammar's comments), so the
//! declaration's own node doesn't cover them; each engine says which node is which, and
//! [`leading_annotations_start`] walks back over them.

use tree_sitter::Node;

use crate::source_text::SourceText;

/// What a node before a declaration is to the walk that finds its leading annotations.
///
/// Only `Other` nodes can be declarations: a node classified as a comment or attribute belongs
/// to what follows it (an engine classifies a comment or attribute that doesn't, such as
/// Rust's `//!`, as `Other`). `code_outline::code_outline` relies on this to skip every other
/// node without asking the engine for its outline lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrecedingNode {
    /// A doc comment (`///`, `/** */`, or any comment in CSS, which has no doc comments): a
    /// leading annotation unless a blank line separates it from the next annotation or the
    /// declaration (plain comments between them don't).
    DocComment,
    /// An attribute (`#[...]`) or decorator (`@x`): a leading annotation even across blank
    /// lines, because the syntax ties it to the declaration.
    Attribute,
    /// Any other comment: stepped over, so it doesn't separate annotations from their
    /// declaration, but never where the range starts.
    PlainComment,
    /// Anything else, which ends the walk.
    Other,
}

/// Where a declaration's line range starts: at its first leading annotation, or at
/// `declaration_start` without one.
///
/// `preceding` are the nodes before the declaration among its siblings, in source order, and
/// `classify` says what each is. `header_end` is where the file's header ends (0 without one):
/// the comment a header was read from is never a leading annotation. A comment right after code
/// on its line (`const A: u8 = 1; // one`) belongs to that code, so it ends the walk.
pub(crate) fn leading_annotations_start(
    preceding: &[Node<'_>],
    declaration_start: usize,
    source: &SourceText<'_>,
    header_end: usize,
    classify: impl Fn(Node<'_>, &str) -> PrecedingNode,
) -> usize {
    let text = source.text;
    let mut start = declaration_start;
    // Where the node after the one being looked at starts, and whether a blank line lies
    // between that node and the closest annotation (or the declaration) after it: plain comments
    // stepped over can't hide a blank line, and a blank line inside one doesn't count.
    let mut next_start = declaration_start;
    let mut blank_line_after = false;
    for (index, &node) in preceding.iter().enumerate().rev() {
        if node.start_byte() < header_end {
            break;
        }
        let role = classify(node, text);
        let follows_code = index.checked_sub(1).is_some_and(|previous_index| {
            let previous = preceding[previous_index];
            ends_on_line_of(source, previous, node.start_byte())
                && classify(previous, text) == PrecedingNode::Other
        });
        let blank_line_before_next = blank_line_between(text, node.end_byte(), next_start);
        match role {
            PrecedingNode::DocComment | PrecedingNode::PlainComment if follows_code => break,
            PrecedingNode::DocComment if blank_line_after || blank_line_before_next => break,
            PrecedingNode::DocComment | PrecedingNode::Attribute => {
                start = node.start_byte();
                blank_line_after = false;
            }
            PrecedingNode::PlainComment => blank_line_after |= blank_line_before_next,
            PrecedingNode::Other => break,
        }
        next_start = node.start_byte();
    }
    start
}

/// Whether `node` ends on the line where `offset` is. A line comment's node may include its
/// newline (tree-sitter-rust's comments do), and then it ends on the line before.
fn ends_on_line_of(source: &SourceText<'_>, node: Node<'_>, offset: usize) -> bool {
    node.end_byte() > source.line_around(offset).start
}

/// Whether a blank line lies in the whitespace between a node ending at `end` and the next
/// node, starting at `next_start`. A line comment's node may include its newline, so that
/// newline is counted as if it came after the node.
fn blank_line_between(text: &str, end: usize, next_start: usize) -> bool {
    let gap_start = if text[..end].ends_with('\n') {
        end - 1
    } else {
        end
    };
    // The first and last pieces are the rests of the node's and the next node's lines.
    let mut pieces = text[gap_start..next_start].split('\n');
    pieces.next();
    pieces.next_back();
    pieces.any(|line| line.trim().is_empty())
}
