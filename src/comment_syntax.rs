//! Comment syntax, shared by every engine but Markdown's: a language's comment markers, what a comment's marker says it documents, where a block comment ends, and stripping markers from a comment's lines.
//!
//! This is lexing only: which comment becomes a file's header is `header_comment.rs`, and which
//! comments belong to the declaration below them is `leading_annotations.rs`; both read a
//! comment's meaning from [`CommentSyntax::kind_of`].

pub(crate) const BLOCK_COMMENT_OPEN: &str = "/*";
pub(crate) const BLOCK_COMMENT_CLOSE: &str = "*/";

/// How a language writes comments: its line comment marker, its block comments, and the
/// markers that make a comment a doc comment.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CommentSyntax {
    /// The marker that starts a line comment, or `None` without line comments (CSS).
    pub(crate) line: Option<LineMarker>,
    /// Whether `/* */` block comments nest, or `None` without block comments (TOML, YAML).
    pub(crate) block: Option<BlockComments>,
    /// The markers that make a comment a doc comment; a comment starting with none of them is
    /// plain.
    pub(crate) doc_markers: &'static [DocMarker],
}

impl CommentSyntax {
    /// `#` line comments only, as in TOML and YAML, without doc comments.
    pub(crate) const HASH: CommentSyntax = CommentSyntax {
        line: Some(LineMarker::Hash),
        block: None,
        doc_markers: &[],
    };

    /// `/* */` block comments only, as in CSS: they don't nest, and there are no doc comments.
    pub(crate) const BLOCK: CommentSyntax = CommentSyntax {
        line: None,
        block: Some(BlockComments::Flat),
        doc_markers: &[],
    };

    /// What the comment starting at the start of `comment` documents, from its marker.
    pub(crate) fn kind_of(&self, comment: &str) -> CommentKind {
        self.doc_markers
            .iter()
            .find(|marker| marker.starts(comment))
            .map_or(CommentKind::Plain, |marker| marker.kind)
    }
}

/// The marker that starts a line comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineMarker {
    /// `//`, as in C, JavaScript, Rust, and JSON.
    Slashes,
    /// `#`.
    Hash,
}

impl LineMarker {
    pub(crate) fn text(self) -> &'static str {
        match self {
            LineMarker::Slashes => "//",
            LineMarker::Hash => "#",
        }
    }

    /// The character the marker repeats; a longer run of it (`////`, `##`) is still the marker.
    fn character(self) -> char {
        match self {
            LineMarker::Slashes => '/',
            LineMarker::Hash => '#',
        }
    }
}

/// Whether a `/*` inside a block comment opens another one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockComments {
    /// The first `*/` closes the comment, as in C, JavaScript, and CSS.
    Flat,
    /// Each `/*` needs its own `*/`, as in Rust: `/* a /* b */ c */` is one comment.
    Nested,
}

/// What a comment documents, from its marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommentKind {
    Plain,
    /// Documents the enclosing file or module, such as Rust's `//!` and `/*!`.
    InnerDoc,
    /// Documents the declaration after it, such as Rust's `///` and `/** */`, or `JSDoc`'s
    /// `/** */`.
    OuterDoc,
}

/// A marker that makes a comment a doc comment: the comment starts with `prefix`, and the
/// character after it is none of `unless_followed_by`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DocMarker {
    pub(crate) prefix: &'static str,
    /// Characters that make a comment starting with `prefix` plain after all, such as the
    /// fourth `/` of `////`.
    pub(crate) unless_followed_by: &'static [char],
    pub(crate) kind: CommentKind,
}

impl DocMarker {
    fn starts(&self, comment: &str) -> bool {
        comment.strip_prefix(self.prefix).is_some_and(|rest| {
            !rest
                .chars()
                .next()
                .is_some_and(|next| self.unless_followed_by.contains(&next))
        })
    }
}

/// `/** */`, documenting the declaration after it, but not `/***` (or more) or the empty `/**/`,
/// as Rust and `JSDoc` read it.
pub(crate) const OUTER_DOC_BLOCK: DocMarker = DocMarker {
    prefix: "/**",
    unless_followed_by: &['*', '/'],
    kind: CommentKind::OuterDoc,
};

/// Rust's `///`, documenting the declaration after it, but not `////` (or more).
pub(crate) const OUTER_DOC_LINE: DocMarker = DocMarker {
    prefix: "///",
    unless_followed_by: &['/'],
    kind: CommentKind::OuterDoc,
};

/// Rust's `//!`, documenting the enclosing file or module.
pub(crate) const INNER_DOC_LINE: DocMarker = DocMarker {
    prefix: "//!",
    unless_followed_by: &[],
    kind: CommentKind::InnerDoc,
};

/// Rust's `/*!`, documenting the enclosing file or module.
pub(crate) const INNER_DOC_BLOCK: DocMarker = DocMarker {
    prefix: "/*!",
    unless_followed_by: &[],
    kind: CommentKind::InnerDoc,
};

/// Where the block comment opening at byte `start` ends, after its closing `*/`, or `None` when
/// it never closes.
pub(crate) fn block_comment_end(
    text: &str,
    start: usize,
    block_comments: BlockComments,
) -> Option<usize> {
    let after_open = start + BLOCK_COMMENT_OPEN.len();
    if block_comments == BlockComments::Flat {
        // Searched after the `/*`, so `/*/` doesn't close the comment it opens.
        let close = text[after_open..].find(BLOCK_COMMENT_CLOSE)?;
        return Some(after_open + close + BLOCK_COMMENT_CLOSE.len());
    }
    // `/` and `*` are ASCII, so stepping byte by byte never matches inside a UTF-8 character.
    let bytes = text.as_bytes();
    let mut depth = 1_usize;
    let mut index = after_open;
    while index + 1 < bytes.len() {
        match &bytes[index..index + 2] {
            b"/*" => {
                depth += 1;
                index += 2;
            }
            b"*/" => {
                depth -= 1;
                index += 2;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => index += 1,
        }
    }
    None
}

/// Where a line sits in a block comment, which says which markers it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockCommentLine {
    /// The comment's only line: `/* text */`.
    Only,
    /// The line with the opening marker.
    First,
    /// A line between the first and the last, which may start with a `*`.
    Middle,
    /// The line with the closing marker, which may start with a `*`.
    Last,
}

impl BlockCommentLine {
    /// The position of line `index` (0-based) in a comment whose last line is `last_index`.
    pub(crate) fn at(index: usize, last_index: usize) -> Self {
        match (index == 0, index == last_index) {
            (true, true) => BlockCommentLine::Only,
            (true, false) => BlockCommentLine::First,
            (false, false) => BlockCommentLine::Middle,
            (false, true) => BlockCommentLine::Last,
        }
    }
}

/// Removes block comment markers from one trimmed line: `/*`, `/**` or `/*!` (with any stars
/// after it) on the first line,
/// a leading `*` on the others, and the closing `*/` on the last. A `*/` ending another line
/// closes a nested comment, so it stays.
///
/// The opening marker goes first, so the `*` of `/**/` can't be mistaken for part of a `*/`.
pub(crate) fn strip_block_markers(line: &str, position: BlockCommentLine) -> &str {
    let opens = matches!(position, BlockCommentLine::Only | BlockCommentLine::First);
    let closes = matches!(position, BlockCommentLine::Only | BlockCommentLine::Last);
    let without_open = if opens {
        line.strip_prefix(BLOCK_COMMENT_OPEN).unwrap_or(line)
    } else {
        line
    };
    let without_close = match without_open.strip_suffix(BLOCK_COMMENT_CLOSE) {
        Some(rest) if closes => rest.trim_end_matches('*'),
        _ => without_open,
    };
    let text = if opens {
        // Stars may come before the `!` (`/**!`) and after it (a `/*!****` banner).
        let after_stars = without_close.trim_start_matches('*');
        let after_bang = after_stars.strip_prefix('!').unwrap_or(after_stars);
        after_bang.trim_start_matches('*')
    } else {
        without_close.strip_prefix('*').unwrap_or(without_close)
    };
    text.trim()
}

/// Removes the line comment marker from one trimmed line: `//`, a doc marker `///` or `//!`, a
/// longer run of slashes such as `////`, or one or more `#`.
pub(crate) fn strip_line_marker(line: &str, marker: LineMarker) -> &str {
    let after_marker = line.trim_start_matches(marker.character());
    match marker {
        LineMarker::Slashes => after_marker
            .strip_prefix('!')
            .unwrap_or(after_marker)
            .trim(),
        LineMarker::Hash => after_marker.trim(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUST_LIKE: CommentSyntax = CommentSyntax {
        line: Some(LineMarker::Slashes),
        block: Some(BlockComments::Nested),
        doc_markers: &[
            INNER_DOC_LINE,
            OUTER_DOC_LINE,
            INNER_DOC_BLOCK,
            OUTER_DOC_BLOCK,
        ],
    };

    #[test]
    fn doc_markers_say_what_a_comment_documents() {
        let cases = [
            ("//! File.", CommentKind::InnerDoc),
            ("/// Item.", CommentKind::OuterDoc),
            ("///", CommentKind::OuterDoc),
            ("//// Banner.", CommentKind::Plain),
            ("// Plain.", CommentKind::Plain),
            ("/*! File. */", CommentKind::InnerDoc),
            ("/** Item. */", CommentKind::OuterDoc),
            ("/*** Banner. */", CommentKind::Plain),
            ("/**/", CommentKind::Plain),
            ("/* Plain. */", CommentKind::Plain),
        ];
        for (comment, kind) in cases {
            assert_eq!(RUST_LIKE.kind_of(comment), kind, "{comment}");
            assert_eq!(CommentSyntax::BLOCK.kind_of(comment), CommentKind::Plain);
        }
    }

    #[test]
    fn nested_block_comments_close_at_their_own_marker() {
        let text = "/* a /* b */ c */ d";
        assert_eq!(block_comment_end(text, 0, BlockComments::Nested), Some(17));
        assert_eq!(block_comment_end(text, 0, BlockComments::Flat), Some(12));
        assert_eq!(block_comment_end("/*/ x", 0, BlockComments::Flat), None);
    }
}
