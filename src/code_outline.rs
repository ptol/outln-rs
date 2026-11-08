//! The outline loop the code-language engines share: each top-level node becomes outline lines whose line range starts at the node's first leading annotation.

use tree_sitter::{Language, Node};

use crate::leading_annotations::{PrecedingNode, leading_annotations_start};
use crate::outline::{Header, Outline, OutlineLine};
use crate::source_text::{ByteRange, SourceText};
use crate::syntax_tree::{children, parse};

/// The outline of a code file: `header`, then the outline lines `declaration_lines` makes from
/// each top-level node of `text` parsed with `language`.
///
/// `declaration_lines` gets each node with its line range: from the node's first leading
/// annotation ([`leading_annotations_start`], with `classify` saying what each node before it
/// is) to the node's end. It returns no lines for a node that isn't a declaration; returning
/// any `IntoIterator` lets an engine with one line per node return an `Option`, which
/// allocates nothing.
///
/// A node `classify` calls a comment, doc comment, or attribute is never a declaration itself,
/// so it's skipped before the walk back: walking from each line of a long comment run would
/// make the outline quadratic in the run's length.
pub(crate) fn code_outline<Lines: IntoIterator<Item = OutlineLine>>(
    source: &SourceText<'_>,
    language: &Language,
    header: Option<Header>,
    classify: impl Fn(Node<'_>, &str) -> PrecedingNode,
    declaration_lines: impl Fn(Node<'_>, ByteRange, &SourceText<'_>) -> Lines,
) -> Outline {
    let text = source.text;
    let tree = parse(text, language);
    let header_end = header.as_ref().map_or(0, Header::end);
    let nodes = children(tree.root_node());
    let lines = with_line_ranges(&nodes, source, header_end, &classify)
        .flat_map(|(node, range)| declaration_lines(node, range, source))
        .collect();
    Outline { header, lines }
}

/// Each of the `siblings` that `classify` calls [`PrecedingNode::Other`], so each one that may be
/// a declaration, with its line range: from its first leading annotation
/// ([`leading_annotations_start`]) to its end. `header_end` is where the file's header ends, or
/// 0 for siblings below the top level, which can't contain it.
pub(crate) fn with_line_ranges<'tree>(
    siblings: &[Node<'tree>],
    source: &SourceText<'_>,
    header_end: usize,
    classify: impl Fn(Node<'_>, &str) -> PrecedingNode,
) -> impl Iterator<Item = (Node<'tree>, ByteRange)> {
    siblings
        .iter()
        .enumerate()
        .filter_map(move |(index, &node)| {
            // A comment or attribute is skipped before the walk back: walking from each line
            // of a long comment run would be quadratic in the run's length.
            if classify(node, source.text) != PrecedingNode::Other {
                return None;
            }
            let start = leading_annotations_start(
                &siblings[..index],
                node.start_byte(),
                source,
                header_end,
                &classify,
            );
            Some((node, ByteRange::new(start, node.end_byte())))
        })
}
