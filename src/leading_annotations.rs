//! Leading annotations, shared by the code-language engines: the doc comments, attributes, and decorators directly above a declaration, where its line range starts.
//!
//! Grammars parse these as siblings before the declaration (tree-sitter-rust's attributes and
//! comments, the TypeScript grammar's class member decorators, every grammar's comments), so the
//! declaration's own node doesn't cover them; each engine's [`Annotations`] table says which
//! node is which, and [`leading_annotations_start`] walks back over them.

use tree_sitter::Node;

use crate::comment_syntax::{CommentKind, CommentSyntax};
use crate::source_text::SourceText;
use crate::syntax_tree::source;

/// Which nodes before a declaration may be its leading annotations, by node kind: the table
/// [`Annotations::classify`] reads.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Annotations {
    /// The grammar's comment node kinds; a comment's marker says what it documents
    /// ([`CommentSyntax::kind_of`]).
    pub(crate) comment_kinds: &'static [&'static str],
    /// The grammar's attribute and decorator node kinds.
    pub(crate) attribute_kinds: &'static [&'static str],
    /// What a plain comment directly above a declaration is to it.
    pub(crate) plain_comments: PlainComments,
}

/// What a plain comment (one without a doc marker) directly above a declaration is to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlainComments {
    /// Stepped over: it neither starts the line range nor separates annotations from their
    /// declaration, as in languages with doc comments (Rust, TypeScript).
    SteppedOver,
    /// A leading annotation, as a doc comment is: a language without doc comments (CSS)
    /// documents a declaration with the comment directly above it.
    Annotate,
}

impl Annotations {
    /// What `node`, before a declaration, is to the walk over its leading annotations. A comment
    /// documenting the enclosing file or module (Rust's `//!`) belongs to no declaration, so it
    /// is [`PrecedingNode::Other`].
    pub(crate) fn classify(
        &self,
        node: Node<'_>,
        text: &str,
        comment_syntax: &CommentSyntax,
    ) -> PrecedingNode {
        let kind = node.kind();
        if self.attribute_kinds.contains(&kind) {
            return PrecedingNode::Attribute;
        }
        if !self.comment_kinds.contains(&kind) {
            return PrecedingNode::Other;
        }
        match comment_syntax.kind_of(source(node, text)) {
            CommentKind::OuterDoc => PrecedingNode::DocComment,
            CommentKind::InnerDoc => PrecedingNode::Other,
            CommentKind::Plain => match self.plain_comments {
                PlainComments::SteppedOver => PrecedingNode::PlainComment,
                PlainComments::Annotate => PrecedingNode::DocComment,
            },
        }
    }
}

/// What a node before a declaration is to the walk that finds its leading annotations.
///
/// Only `Other` nodes can be declarations: a node classified as a comment or attribute belongs
/// to what follows it (a comment that doesn't, such as Rust's `//!`, is `Other`).
/// `code_outline` relies on this to skip every other node without asking the engine for its
/// outline lines.
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
/// `classify` says what each is. `header_end` is where the file's header ends, for a top-level
/// declaration of a file with one: the comment a header was read from is never a leading
/// annotation. A comment right after code
/// on its line (`const A: u8 = 1; // one`) belongs to that code, so it ends the walk.
pub(crate) fn leading_annotations_start(
    preceding: &[Node<'_>],
    declaration_start: usize,
    source: &SourceText<'_>,
    header_end: Option<usize>,
    classify: impl Fn(Node<'_>) -> PrecedingNode,
) -> usize {
    let text = source.text;
    let mut start = declaration_start;
    // Where the node after the one being looked at starts, and whether a blank line lies
    // between that node and the closest annotation (or the declaration) after it: plain comments
    // stepped over can't hide a blank line, and a blank line inside one doesn't count.
    let mut next_start = declaration_start;
    let mut blank_line_after = false;
    for (index, &node) in preceding.iter().enumerate().rev() {
        if header_end.is_some_and(|end| node.start_byte() < end) {
            break;
        }
        let role = classify(node);
        let follows_code = index.checked_sub(1).is_some_and(|previous_index| {
            let previous = preceding[previous_index];
            ends_on_line_of(source, previous, node.start_byte())
                && classify(previous) == PrecedingNode::Other
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
