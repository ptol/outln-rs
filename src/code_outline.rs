//! Code-language engines as data: a `CodeLanguage` describes one language (grammar, comments, leading annotations, how its declarations print), and its one `LanguageEngine` implementation outlines them all, each declaration with a line range from its first leading annotation.
//!
//! A language is a `static CodeLanguage` (see `languages/rust.rs`); what differs between
//! languages is the table's data and its `declaration_lines` function, which gets each node with
//! an [`OutlineLevel`] to print signatures with and to list a node's declarations one level
//! deeper.

use tree_sitter::{Language, Node};

use crate::comment_syntax::CommentSyntax;
use crate::header_comment::{HeaderChoice, find_header_comment};
use crate::language_engine::LanguageEngine;
use crate::leading_annotations::{Annotations, PrecedingNode, leading_annotations_start};
use crate::outline::{FileSummary, Header, Outline, OutlineLine};
use crate::signature::Signature;
use crate::source_text::{ByteRange, SourceText};
use crate::syntax_tree::{children, one_line_source, parse};

/// A code language, as the data its language engine needs.
pub(crate) struct CodeLanguage {
    /// File extensions, lowercase and without the dot.
    pub(crate) extensions: &'static [&'static str],
    /// The tree-sitter grammar. A function, because a grammar can't be built in a `static`.
    pub(crate) grammar: fn() -> Language,
    pub(crate) comment_syntax: CommentSyntax,
    /// Which comment at the top of a file is its header comment.
    pub(crate) header_choice: HeaderChoice,
    /// Where the preamble starting at a byte ends (a shebang, a directive, an inner attribute),
    /// or `None` if none starts there; see [`find_header_comment`].
    pub(crate) preamble_end: fn(&str, usize) -> Option<usize>,
    /// Which nodes before a declaration are its leading annotations.
    pub(crate) annotations: Annotations,
    /// Literal node kinds kept as written when source is put on one line
    /// ([`one_line_source`]), such as string literals.
    pub(crate) verbatim_kinds: &'static [&'static str],
    /// Appends the outline lines of one node with its line range; nothing for a node that isn't
    /// a declaration. Gets every node that isn't a leading annotation, at every level.
    pub(crate) declaration_lines: DeclarationLines,
}

/// How a [`CodeLanguage`] turns one node with its line range into outline lines, written into
/// the outline's lines so a node with any number of lines (none, a class and its members)
/// allocates nothing of its own.
pub(crate) type DeclarationLines =
    fn(&OutlineLevel<'_>, Node<'_>, ByteRange, &mut Vec<OutlineLine>);

impl CodeLanguage {
    /// The header comment of `text`, found line by line without parsing.
    pub(crate) fn header(&self, text: &str) -> Option<Header> {
        find_header_comment(
            text,
            &self.comment_syntax,
            self.header_choice,
            self.preamble_end,
        )
    }

    /// What `node`, before a declaration, is to the walk over its leading annotations.
    fn classify(&self, node: Node<'_>, text: &str) -> PrecedingNode {
        self.annotations.classify(node, text, &self.comment_syntax)
    }
}

impl LanguageEngine for CodeLanguage {
    fn extensions(&self) -> &'static [&'static str] {
        self.extensions
    }

    fn outline(&self, source: &SourceText<'_>) -> Outline {
        let header = self.header(source.text);
        let tree = parse(source.text, &(self.grammar)());
        let top_level = OutlineLevel {
            source,
            language: self,
            depth: TOP_LEVEL_DEPTH,
        };
        let mut lines = Vec::new();
        let header_end = header.as_ref().map(Header::end);
        top_level.list(&children(tree.root_node()), header_end, &mut lines);
        Outline { header, lines }
    }

    fn file_summary(&self, text: &str) -> FileSummary {
        // The header is found line by line, so the glob view never parses a code file.
        FileSummary::from(self.header(text))
    }

    #[cfg(test)]
    fn grammar(&self) -> Option<Language> {
        Some((self.grammar)())
    }
}

/// The depth of top-level declarations; each [`OutlineLevel::declarations_inside`] goes one
/// deeper.
const TOP_LEVEL_DEPTH: usize = 0;

/// One level of a code file's outline: what a [`CodeLanguage`]'s `declaration_lines` gets with
/// each node, to read the source, print signatures, and list the declarations inside a node
/// one level deeper.
#[derive(Clone, Copy)]
pub(crate) struct OutlineLevel<'a> {
    source: &'a SourceText<'a>,
    language: &'a CodeLanguage,
    depth: usize,
}

impl<'a> OutlineLevel<'a> {
    pub(crate) fn text(&self) -> &'a str {
        self.source.text
    }

    pub(crate) fn source(&self) -> &'a SourceText<'a> {
        self.source
    }

    /// Whether this level lists top-level declarations.
    pub(crate) fn is_top_level(&self) -> bool {
        self.depth == TOP_LEVEL_DEPTH
    }

    /// The node's source on one line ([`one_line_source`]), keeping the language's verbatim
    /// kinds as written.
    pub(crate) fn one_line(&self, node: Node<'_>) -> String {
        one_line_source(node, self.text(), self.language.verbatim_kinds)
    }

    /// The signature made of the node's source up to byte `end`, where its body starts
    /// ([`Signature::source_before`]).
    pub(crate) fn signature_before(&self, node: Node<'_>, end: usize) -> Signature {
        Signature::source_before(node, end, self.text(), self.language.verbatim_kinds)
    }

    /// The outline line of a declaration at this level with this signature, line range
    /// `range`, and a highlight from `highlight_start` (its first token after its leading
    /// annotations) to the end of the signature.
    pub(crate) fn outline_line(
        &self,
        signature: Signature,
        range: ByteRange,
        highlight_start: usize,
    ) -> OutlineLine {
        signature.into_outline_line(range, highlight_start, self.depth)
    }

    /// Appends the outline lines of the declarations among `parent`'s children one level
    /// deeper, each with its own line range: a class's members, the rules in an at-rule's
    /// block. The language's `declaration_lines` gets each of them at the deeper level.
    pub(crate) fn declarations_inside(&self, parent: Node<'_>, lines: &mut Vec<OutlineLine>) {
        let deeper = OutlineLevel {
            depth: self.depth + 1,
            ..*self
        };
        // Below the top level, no node can contain the file's header comment.
        deeper.list(&children(parent), None, lines);
    }

    /// Appends the outline lines of each of `siblings` that isn't a leading annotation, with its
    /// line range: from its first leading annotation ([`leading_annotations_start`]) to its end.
    /// `header_end` is where the file's header comment ends, for the top level of a file with
    /// one.
    fn list(&self, siblings: &[Node<'_>], header_end: Option<usize>, lines: &mut Vec<OutlineLine>) {
        let classify = |node: Node<'_>| self.language.classify(node, self.text());
        for (index, &node) in siblings.iter().enumerate() {
            // A comment or attribute is never a declaration itself, so it's skipped before the
            // walk back: walking from each line of a long comment run would be quadratic in the
            // run's length.
            if classify(node) != PrecedingNode::Other {
                continue;
            }
            let start = leading_annotations_start(
                &siblings[..index],
                node.start_byte(),
                self.source,
                header_end,
                classify,
            );
            let range = ByteRange::new(start, node.end_byte());
            (self.language.declaration_lines)(self, node, range, lines);
        }
    }
}
