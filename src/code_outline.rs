//! The outline loop the code-language engines share: each top-level node becomes outline lines whose line range starts at the node's first leading annotation.

use tree_sitter::{Language, Node};

use crate::leading_annotations::{PrecedingNode, leading_annotations_start};
use crate::outline::{Header, Outline, OutlineLine};
use crate::source_text::ByteRange;
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
    text: &str,
    language: &Language,
    header: Option<Header>,
    classify: impl Fn(Node<'_>, &str) -> PrecedingNode,
    declaration_lines: impl Fn(Node<'_>, ByteRange, &str) -> Lines,
) -> Outline {
    let tree = parse(text, language);
    let header_end = header.as_ref().map_or(0, Header::end);
    let nodes = children(tree.root_node());
    let mut lines = Vec::new();
    for (index, &node) in nodes.iter().enumerate() {
        if classify(node, text) != PrecedingNode::Other {
            continue;
        }
        let range_start = leading_annotations_start(
            &nodes[..index],
            node.start_byte(),
            text,
            header_end,
            &classify,
        );
        let range = ByteRange::new(range_start, node.end_byte());
        lines.extend(declaration_lines(node, range, text));
    }
    Outline { header, lines }
}
