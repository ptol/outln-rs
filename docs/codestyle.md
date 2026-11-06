---
description: 'Rust code style rules for outln: functions, ownership, types, errors, modules, and comments.'
readWhen: 'changing source code'
---

# Code style

## Functions

- Prefer short, pure functions. Inject side effects (output streams, time, randomness, environment access, logging) through parameters, usually as a trait or `impl Write`/`impl Read`. Filesystem reads are the exception ([ADR-004](adr.md)): read directly, resolving paths against `Context::cwd`.
- Functional core, imperative shell: `src/main.rs` wires real I/O into the library; everything else stays testable with in-memory values or files in a temp directory.
- Keep each function at one level of abstraction; extract lower-level details into named helpers when that makes the caller read as steps.
- Command-query separation: a function either returns a value or causes an effect.
- Replace boolean parameters with a two-variant `enum` or separate functions.
- Use guard clauses and early returns over deep nesting: `let ... else`, `?`, and `if ... { return ... }`.
- Pass dependencies and configuration explicitly; no global state (`static mut`, lazy globals holding config, thread-locals). An immutable `static` table of stateless values, such as the `ENGINES` registry, is fine.
- Search for an existing helper before writing a new one.
- Don't copy a block and change a few lines; extract one shared version with the differences as parameters.

## Ownership and mutation

- Borrow before you own: take `&str`, `&[T]`, `&Path` in parameters; return owned values when the caller needs them.
- Clone only when ownership is truly needed, not to satisfy the borrow checker by reflex; an expensive clone in a hot path needs a reason.
- Mutating data you own, or hold as `&mut`, is idiomatic and fine; keep the mutation local and visible in the signature.
- Shared mutable state (`Rc<RefCell<_>>`, `Arc<Mutex<_>>`) needs a comment saying why simpler ownership doesn't work.
- Prefer iterators over index loops; they avoid bounds errors and usually compile to the same code.

## Types

- Make illegal states unrepresentable.
- Parse, don't validate: turn untrusted input (arguments, file contents) into typed values at the boundary with `FromStr`/`TryFrom` or a parse function.
- Prefer `enum`s with data over boolean flags and clusters of `Option` fields.
- Use newtypes for domain primitives (`struct LineNumber(u32)`) instead of bare `u32`/`String`.
- Single source of truth: derive values instead of storing duplicated state.
- `match` over our own enums lists every variant (the `wildcard_enum_match_arm` lint enforces it), so adding a variant forces a decision at each match.
- Name magic numbers and strings as `const`s, with the unit in the name or type (`MAX_FILE_BYTES`).

## Errors

- Return expected failures as `Result` or `Option`; `panic!`, `unwrap`, and `expect` are for bugs and broken invariants only, and lints reject them outside tests.
- Use one error `enum` per module boundary, with a variant per failure the caller can act on; convert with `?` and `From`.
- Fail fast: check invariants early (`debug_assert!` for internal ones) instead of propagating bad state.
- Error messages name what failed, the offending value, and what was expected.

## Modules

- Deep modules: small interfaces hiding substantial implementation. Keep `pub` minimal; prefer `pub(crate)` (the `unreachable_pub` lint flags `pub` that isn't reachable from the crate root).
- One clear purpose per file, named for it; no `mod.rs` grab-bags or `utils.rs`.
- Keep independent concerns independently changeable, so a reader can reason about one module locally.
- Name code with the domain's terms from [GLOSSARY.md](GLOSSARY.md).
- Among several valid implementations, pick the one least surprising to maintainers and users.

## Comments

- Public items get `///` doc comments; examples in them run as tests, so prefer an example over prose.
- Names carry the _what_; comments carry the _why_.
- Comment the _why_ wherever a reader of the surrounding code alone would ask "why this way?": a constraint enforced in another file or crate, an assumption the code relies on but does not check, a deliberate trade-off against the obvious alternative, or a technique from a known algorithm or paper (name it). State the reason in a line or two and, when it lives elsewhere, point to where.
