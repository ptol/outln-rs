//! Class members of TypeScript and JavaScript classes: methods, accessors, and the constructor, as indented outline lines.

use tree_sitter::Node;

use super::signatures::{FUNCTION_SIGNATURE_FIELDS, push_fields};
use crate::code_outline::OutlineLevel;
use crate::outline::OutlineLine;
use crate::signature::Signature;
use crate::source_text::ByteRange;
use crate::syntax_tree::{child_of_kind, source};

/// The outline line of a method-like member of a class body with line range `range`, one level
/// deeper than its class, or `None` for a member that isn't listed.
///
/// Fields, static blocks, overload signatures, and members with computed names
/// (`[Symbol.iterator]()`) or quoted names are skipped. Modifiers such as `static`, `private`,
/// and `async` are left out of the printed signature. The JavaScript grammar makes a member's
/// decorators part of it, while the TypeScript grammar parses them as siblings before it; both
/// give the member a line range that includes them.
pub(super) fn member_line(
    level: &OutlineLevel<'_>,
    member: Node<'_>,
    range: ByteRange,
) -> Option<OutlineLine> {
    let (signature, highlight_start) = member_signature(member, level)?;
    Some(level.outline_line(signature, range, highlight_start))
}

/// The member's signature (`name<T>(parameters): Return`, `get name(): Type`,
/// `set name(value)`), and where its highlight starts: at `get`/`set` or the name, after any
/// modifiers. `None` for a member that isn't listed.
fn member_signature(member: Node<'_>, level: &OutlineLevel<'_>) -> Option<(Signature, usize)> {
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
    signature.text.push_str(source(name, level.text()));
    match accessor.map(|accessor| accessor.kind()) {
        // A getter takes no parameters, so its list prints as `()` whatever is inside it.
        Some("get") => {
            signature.text.push_str("()");
            if let Some(parameters) = member.child_by_field_name("parameters") {
                signature.highlight_end = parameters.end_byte();
            }
            push_fields(&mut signature, member, &["return_type"], level);
        }
        Some(_) => push_fields(&mut signature, member, &["parameters"], level),
        None => push_fields(&mut signature, member, &FUNCTION_SIGNATURE_FIELDS, level),
    }
    let highlight_start = accessor.map_or(name.start_byte(), |accessor| accessor.start_byte());
    Some((signature, highlight_start))
}
