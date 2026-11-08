//! Signatures of TypeScript and JavaScript declarations: the text an outline line prints, and where its highlight ends.

use tree_sitter::Node;

use crate::code_outline::OutlineLevel;
use crate::signature::Signature;
use crate::syntax_tree::{child_of_kind, children, first_syntax_child, has_child_of_kind, source};

/// The fields after a function's or method's name, in source order, printed without spaces
/// between them (the return type's node includes its `:`).
pub(super) const FUNCTION_SIGNATURE_FIELDS: [&str; 3] =
    ["type_parameters", "parameters", "return_type"];

/// `async function* name<T>(parameters): Return` for a function declaration, a function
/// expression, or an overload signature; parts the source doesn't have are left out.
pub(super) fn function(node: Node<'_>, level: &OutlineLevel<'_>) -> Signature {
    let keyword = child_of_kind(node, "function");
    let mut signature = Signature {
        text: String::new(),
        highlight_end: keyword.map_or(node.start_byte(), |keyword| keyword.end_byte()),
    };
    if has_child_of_kind(node, "async") {
        signature.text.push_str("async ");
    }
    signature.text.push_str("function");
    if let Some(star) = child_of_kind(node, "*") {
        signature.text.push('*');
        signature.highlight_end = star.end_byte();
    }
    if let Some(name) = node.child_by_field_name("name") {
        signature.text.push(' ');
        signature.text.push_str(source(name, level.text()));
        signature.highlight_end = name.end_byte();
    }
    push_fields(&mut signature, node, &FUNCTION_SIGNATURE_FIELDS, level);
    signature
}

/// The signature of a function stored in a variable or exported as a default, or `None` for
/// other values. An arrow function prints without its keyword or `=>`: `async <T>(parameters):
/// Return`, and a lone parameter without parentheses gets them (`x => x` prints as `(x)`). A
/// function expression prints like a function declaration, `function` included.
pub(super) fn function_value(value: Node<'_>, level: &OutlineLevel<'_>) -> Option<Signature> {
    match value.kind() {
        "function_expression" | "generator_function" => Some(function(value, level)),
        "arrow_function" => {
            let mut signature = Signature {
                text: String::new(),
                highlight_end: value.start_byte(),
            };
            if let Some(keyword) = child_of_kind(value, "async") {
                signature.text.push_str("async ");
                signature.highlight_end = keyword.end_byte();
            }
            push_fields(&mut signature, value, &["type_parameters"], level);
            if let Some(parameter) = value.child_by_field_name("parameter") {
                signature.text.push('(');
                signature.text.push_str(source(parameter, level.text()));
                signature.text.push(')');
                signature.highlight_end = parameter.end_byte();
            }
            push_fields(&mut signature, value, &["parameters", "return_type"], level);
            Some(signature)
        }
        _ => None,
    }
}

/// What an `export default <expression>` prints after `export default`, for an expression
/// that isn't a function or class: a function value's signature, an identifier as written, a
/// call's callee with `(…)` (`()` without arguments), `{…}` for an object, `[…]` for an array
/// (`{}` and `[]` when empty), and `…` for anything else, highlighted up to `default_keyword_end`.
pub(super) fn default_value(
    value: Node<'_>,
    default_keyword_end: usize,
    level: &OutlineLevel<'_>,
) -> Signature {
    if let Some(function) = function_value(value, level) {
        return function;
    }
    // An empty object or array has no children but its brackets and comments; an array hole
    // (`[,]`) is a comma child, so it counts as contents.
    let is_empty = children(value)
        .iter()
        .all(|child| child.is_extra() || matches!(child.kind(), "{" | "}" | "[" | "]"));
    let opening_bracket_end = value.start_byte() + 1;
    let (printed, highlight_end) = match value.kind() {
        "identifier" => (source(value, level.text()).to_owned(), value.end_byte()),
        "call_expression" => {
            let callee = value.child_by_field_name("function").unwrap_or(value);
            let has_arguments = value
                .child_by_field_name("arguments")
                .is_some_and(|arguments| first_syntax_child(arguments).is_some());
            let arguments = if has_arguments { "(…)" } else { "()" };
            (
                format!("{}{arguments}", level.one_line(callee)),
                callee.end_byte(),
            )
        }
        "object" if is_empty => ("{}".to_owned(), opening_bracket_end),
        "object" => ("{…}".to_owned(), opening_bracket_end),
        "array" if is_empty => ("[]".to_owned(), opening_bracket_end),
        "array" => ("[…]".to_owned(), opening_bracket_end),
        _ => ("…".to_owned(), default_keyword_end),
    };
    Signature {
        text: printed,
        highlight_end,
    }
}

/// The expression inside parentheses and `as`/`satisfies` type assertions, which don't change
/// what a value is.
pub(super) fn without_wrappers(mut expression: Node<'_>) -> Node<'_> {
    while matches!(
        expression.kind(),
        "parenthesized_expression" | "as_expression" | "satisfies_expression"
    ) {
        let Some(inner) = first_syntax_child(expression) else {
            break;
        };
        expression = inner;
    }
    expression
}

/// `class Name<T>`, `abstract class Name`, `interface Name<T>`, `type Name<T>`, `enum Name`, or
/// `const enum Name`. An anonymous class (`export default class {}`) prints as `class`.
pub(super) fn type_like(node: Node<'_>, level: &OutlineLevel<'_>) -> Signature {
    let (keyword, last_keyword_token) = match node.kind() {
        "abstract_class_declaration" => ("abstract class", "class"),
        "interface_declaration" => ("interface", "interface"),
        "type_alias_declaration" => ("type", "type"),
        "enum_declaration" if has_child_of_kind(node, "const") => ("const enum", "enum"),
        "enum_declaration" => ("enum", "enum"),
        _ => ("class", "class"),
    };
    let mut signature = Signature {
        text: keyword.to_owned(),
        highlight_end: child_of_kind(node, last_keyword_token)
            .map_or(node.start_byte(), |token| token.end_byte()),
    };
    let name = child_of_kind(node, "type_identifier").or_else(|| child_of_kind(node, "identifier"));
    if let Some(name) = name {
        signature.text.push(' ');
        signature.text.push_str(source(name, level.text()));
        signature.highlight_end = name.end_byte();
    }
    push_fields(&mut signature, node, &["type_parameters"], level);
    signature
}

/// `namespace Name` or `module Name`; a dotted name (`namespace A.B`) prints as written.
pub(super) fn namespace(node: Node<'_>, level: &OutlineLevel<'_>) -> Signature {
    let keyword = if node.kind() == "internal_module" {
        "namespace"
    } else {
        "module"
    };
    match node.child_by_field_name("name") {
        Some(name) => Signature {
            text: format!("{keyword} {}", level.one_line(name)),
            highlight_end: name.end_byte(),
        },
        None => Signature {
            text: keyword.to_owned(),
            highlight_end: node.start_byte(),
        },
    }
}

/// Appends the source of each field the node has, on one line, and moves the highlight's end
/// past it.
pub(super) fn push_fields(
    signature: &mut Signature,
    node: Node<'_>,
    fields: &[&str],
    level: &OutlineLevel<'_>,
) {
    for field in fields {
        if let Some(child) = node.child_by_field_name(field) {
            signature.text.push_str(&level.one_line(child));
            signature.highlight_end = child.end_byte();
        }
    }
}
