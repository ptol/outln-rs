//! Class members of TypeScript and JavaScript classes: methods, accessors, and the constructor, as indented outline lines.

use tree_sitter::Node;

use super::signatures::{FUNCTION_SIGNATURE_FIELDS, Signature, push_fields};
use crate::outline::OutlineLine;
use crate::source_text::ByteRange;
use crate::syntax_tree::{child_of_kind, children, source};

/// How deep class members are nested in an outline.
const MEMBER_DEPTH: usize = 1;

/// One outline line per method-like member of a class body, in source order.
///
/// Fields, static blocks, overload signatures, and members with computed names
/// (`[Symbol.iterator]()`) or quoted names are skipped. Modifiers such as `static`, `private`,
/// and `async` are left out of the printed signature.
pub(super) fn member_lines(class_body: Node<'_>, text: &str) -> Vec<OutlineLine> {
    let members = children(class_body);
    members
        .iter()
        .enumerate()
        .filter_map(|(index, member)| {
            let (signature, highlight_start) = member_signature(*member, text)?;
            let start = decorators_start(&members[..index]).unwrap_or(member.start_byte());
            Some(OutlineLine {
                text: signature.text,
                range: ByteRange::new(start, member.end_byte()),
                highlight: ByteRange::new(highlight_start, signature.highlight_end),
                depth: MEMBER_DEPTH,
            })
        })
        .collect()
}

/// The member's signature (`name<T>(parameters): Return`, `get name(): Type`,
/// `set name(value)`), and where its highlight starts: at `get`/`set` or the name, after any
/// modifiers. `None` for a member that isn't listed.
fn member_signature(member: Node<'_>, text: &str) -> Option<(Signature, usize)> {
    if !matches!(
        member.kind(),
        "method_definition" | "abstract_method_signature"
    ) {
        return None;
    }
    let name = member.child_by_field_name("name").filter(|name| {
        matches!(
            name.kind(),
            "property_identifier" | "private_property_identifier"
        )
    })?;
    let accessor = child_of_kind(member, "get").or_else(|| child_of_kind(member, "set"));
    let mut signature = Signature {
        text: String::new(),
        highlight_end: name.end_byte(),
    };
    if let Some(accessor) = accessor {
        signature.text.push_str(accessor.kind());
        signature.text.push(' ');
    }
    signature.text.push_str(source(name, text));
    match accessor.map(|accessor| accessor.kind()) {
        // A getter takes no parameters, so its list prints as `()` whatever is inside it.
        Some("get") => {
            signature.text.push_str("()");
            if let Some(parameters) = member.child_by_field_name("parameters") {
                signature.highlight_end = parameters.end_byte();
            }
            push_fields(&mut signature, member, &["return_type"], text);
        }
        Some(_) => push_fields(&mut signature, member, &["parameters"], text),
        None => push_fields(&mut signature, member, &FUNCTION_SIGNATURE_FIELDS, text),
    }
    let highlight_start = accessor.map_or(name.start_byte(), |accessor| accessor.start_byte());
    Some((signature, highlight_start))
}

/// Where the decorators directly before a member start, if it has any; comments between them
/// and the member don't separate them.
///
/// The TypeScript grammar parses a member's decorators as siblings before it in the class body,
/// while the JavaScript grammar makes them part of the member, so this covers the TypeScript
/// shape: both then give the member a line range that includes its decorators.
fn decorators_start(preceding_members: &[Node<'_>]) -> Option<usize> {
    let mut start = None;
    for sibling in preceding_members.iter().rev() {
        match sibling.kind() {
            "decorator" => start = Some(sibling.start_byte()),
            "comment" => {}
            _ => break,
        }
    }
    start
}
