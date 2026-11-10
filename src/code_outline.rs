//! Code-language engines as data: a `CodeLanguage` describes one language (grammar, comments, leading annotations, how its declarations print), and its one `LanguageEngine` implementation outlines them all, each declaration with a line range from its first leading annotation.
//!
//! A language is a `static CodeLanguage` (see `languages/rust.rs`); what differs between
//! languages is the table's data and its `declaration_lines` function, which gets each node with
//! an [`OutlineLevel`] to print signatures with and to list a node's declarations one level
//! deeper.

use tree_sitter::{Language, Node};

use crate::comment_syntax::CommentSyntax;
use crate::header_comment::{HeaderChoice, HeaderSearch, find_header_comment};
use crate::language_engine::LanguageEngine;
use crate::leading_annotations::{Annotations, PrecedingNode, leading_annotations_start};
use crate::outline::{
    DeclarationSummary, DeclarationsShown, FileSummary, Header, Outline, OutlineLine,
};
use crate::signature::Signature;
use crate::source_text::{ByteRange, Cut, SourceText, summary_prefix};
use crate::syntax_tree::{OneLineRules, children, one_line_source, parse};

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
    /// How source prints on one line ([`one_line_source`]): the literal node kinds kept as
    /// written, and the member lists that need separators.
    pub(crate) one_line: OneLineRules,
    /// Appends the outline lines of one node with its line range; nothing for a node that isn't
    /// a declaration. Gets every node that isn't a leading annotation, at every level.
    pub(crate) declaration_lines: DeclarationLines,
    /// The part of a top-level outline line that a declaration summary prints, such as
    /// [`signature::signature_head`](crate::signature::signature_head).
    pub(crate) summary_text: fn(&str) -> &str,
}

/// How a [`CodeLanguage`] turns one node with its line range into outline lines, written into
/// the outline's lines so a node with any number of lines (none, a class and its members)
/// allocates nothing of its own.
pub(crate) type DeclarationLines =
    fn(&OutlineLevel<'_>, Node<'_>, ByteRange, &mut Vec<OutlineLine>);

impl CodeLanguage {
    /// The header comment of `text`, and where the comments read for it end, found line by
    /// line without parsing.
    fn header_search(&self, text: &str) -> HeaderSearch {
        find_header_comment(
            text,
            &self.comment_syntax,
            self.header_choice,
            self.preamble_end,
        )
    }

    /// The header comment of `text`.
    #[cfg(test)]
    pub(crate) fn header(&self, text: &str) -> Option<Header> {
        self.header_search(text).header
    }

    /// The declaration summary of `text`'s top-level declarations, or `None` without any.
    /// `header_end` is where the comments read for its header end, from the header search the
    /// caller already made.
    fn declaration_summary(
        &self,
        text: &str,
        header_end: Option<usize>,
    ) -> Option<DeclarationSummary> {
        let lines = self.declaration_lines(&SourceText::new(text), header_end, TOP_LEVEL_DEPTH);
        DeclarationSummary::of(lines.iter().map(|line| (self.summary_text)(&line.text)))
    }

    /// The outline lines of `source`'s declarations, parsed whole, down to depth `deepest`
    /// (`TOP_LEVEL_DEPTH` lists only the top-level ones). `header_end` is where the comments
    /// read for the header end ([`HeaderSearch::end`]).
    fn declaration_lines(
        &self,
        source: &SourceText<'_>,
        header_end: Option<usize>,
        deepest: usize,
    ) -> Vec<OutlineLine> {
        let tree = parse(source.text, &(self.grammar)());
        let top_level = OutlineLevel {
            source,
            language: self,
            depth: TOP_LEVEL_DEPTH,
            deepest,
        };
        let mut lines = Vec::new();
        top_level.list(&children(tree.root_node()), header_end, &mut lines);
        lines
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
        let HeaderSearch { header, end } = self.header_search(source.text);
        let lines = self.declaration_lines(source, end, DEEPEST_DEPTH);
        Outline { header, lines }
    }

    /// The header, found line by line; only a file without one is parsed, for its declaration
    /// summary, and only its start ([`declaration_summary_prefix`]), so the glob view stays fast.
    fn file_summary(&self, text: &str) -> FileSummary {
        let HeaderSearch { header, end } = self.header_search(text);
        if let Some(header) = header.filter(Header::has_text) {
            return FileSummary::Header(header);
        }
        let (prefix, cut) = declaration_summary_prefix(text);
        let Some(mut summary) = self.declaration_summary(prefix, end) else {
            return FileSummary::Empty;
        };
        // More declarations may follow what was read.
        if cut == Cut::Prefix {
            summary.shown = DeclarationsShown::First;
        }
        FileSummary::Declarations(summary)
    }

    #[cfg(test)]
    fn grammar(&self) -> Option<Language> {
        Some((self.grammar)())
    }
}

/// How much of a code file without a header a declaration summary parses, at most. A glob
/// view summarizes every file, and parsing is most of its time on code without headers, so it
/// parses only enough for the summary's first declarations; a generated file of several
/// megabytes is never parsed whole.
const DECLARATION_SUMMARY_PREFIX_BYTES: usize = 4 * 1024;

/// The start of a code file without a header that its declaration summary parses: all of it, or
/// its first [`DECLARATION_SUMMARY_PREFIX_BYTES`] cut before the last line among them that
/// starts at column 0 with neither whitespace nor a closing bracket, which usually starts a
/// top-level declaration. Cutting there keeps the parse from ending inside a declaration, which
/// tree-sitter would then turn into a syntax error and the summary would lose.
fn declaration_summary_prefix(text: &str) -> (&str, Cut) {
    let (prefix, cut) = summary_prefix(text, DECLARATION_SUMMARY_PREFIX_BYTES);
    if cut == Cut::WholeFile {
        return (prefix, cut);
    }
    let starts_top_level = |line: &str| {
        line.starts_with(|character: char| {
            !character.is_whitespace() && !matches!(character, '}' | ')' | ']')
        })
    };
    let last_top_level_start = prefix
        .match_indices('\n')
        .map(|(newline, _)| newline + 1)
        .rfind(|&line_start| starts_top_level(&prefix[line_start..]));
    (
        last_top_level_start.map_or(prefix, |start| &prefix[..start]),
        cut,
    )
}

/// The depth of top-level declarations; each [`OutlineLevel::declarations_inside`] goes one
/// deeper.
const TOP_LEVEL_DEPTH: usize = 0;

/// The deepest level an outline lists, in every code language: a top-level declaration, what it
/// contains (a Rust module's items, a class's members, the tests in a `describe` block), and
/// what those contain (the methods of an `impl` block in a module), but nothing deeper. An
/// outline is for choosing what to read, and deeper levels would bury the top-level lines; the
/// limit also bounds how deep the walk recurses.
const DEEPEST_DEPTH: usize = 2;

/// One level of a code file's outline: what a [`CodeLanguage`]'s `declaration_lines` gets with
/// each node, to read the source, print signatures, and list the declarations inside a node
/// one level deeper.
#[derive(Clone, Copy)]
pub(crate) struct OutlineLevel<'a> {
    source: &'a SourceText<'a>,
    language: &'a CodeLanguage,
    depth: usize,
    /// The deepest level listed: [`DEEPEST_DEPTH`] for an outline, [`TOP_LEVEL_DEPTH`] for a
    /// declaration summary, which prints only top-level declarations.
    deepest: usize,
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

    /// The node's source on one line ([`one_line_source`]), by the language's
    /// [`OneLineRules`].
    pub(crate) fn one_line(&self, node: Node<'_>) -> String {
        one_line_source(node, self.text(), &self.language.one_line)
    }

    /// The signature made of the node's source up to byte `end`, where its body starts
    /// ([`Signature::source_before`]).
    pub(crate) fn signature_before(&self, node: Node<'_>, end: usize) -> Signature {
        Signature::source_before(node, end, self.text(), &self.language.one_line)
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
    /// block. The language's `declaration_lines` gets each of them at the deeper level. Nothing
    /// below the deepest level listed ([`DEEPEST_DEPTH`] in an outline) is.
    pub(crate) fn declarations_inside(&self, parent: Node<'_>, lines: &mut Vec<OutlineLine>) {
        if self.depth >= self.deepest {
            return;
        }
        let deeper = OutlineLevel {
            depth: self.depth + 1,
            ..*self
        };
        // Below the top level, no node can contain the file's header comment.
        deeper.list(&children(parent), None, lines);
    }

    /// Appends the outline lines of each of `siblings` that isn't a leading annotation, with its
    /// line range: from its first leading annotation ([`leading_annotations_start`]) to its end.
    /// `header_end` is where the comments read for the file's header end
    /// ([`HeaderSearch::end`]), for the top level of a file with any.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::rust::RUST;
    use crate::outline::SummarizedDeclaration;

    #[test]
    fn a_macro_call_cut_by_the_summary_prefix_is_still_summarized() {
        // No line inside the call starts at column 0, so the prefix ends inside the call, and
        // tree-sitter closes it there.
        let items = "    pub fn item() {}\n".repeat(DECLARATION_SUMMARY_PREFIX_BYTES / 10);
        let text = format!("cfg_if::cfg_if! {{\n{items}}}\n\npub fn after() {{}}\n");
        assert!(text.len() > DECLARATION_SUMMARY_PREFIX_BYTES);
        assert_eq!(
            RUST.file_summary(&text),
            FileSummary::Declarations(DeclarationSummary {
                declarations: vec![SummarizedDeclaration {
                    text: "cfg_if::cfg_if!".to_owned(),
                    count: 1,
                }],
                shown: DeclarationsShown::First,
            })
        );
    }
}
