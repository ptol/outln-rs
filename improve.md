---
description: 'Spec for three fixes found by running outln over real code: Rust macro calls and script destructuring declarations missing from outlines, and glob view duplicates.'
---

# Improvements spec

Three independent fixes. Each section gives the behaviour today, the target behaviour, where the change goes, and the tests that prove it. Delete this file when all three are merged; their lasting record is the casefiles, ADR amendments, and changelog entries each fix adds.

Fixes 1 and 2 reverse deliberate choices (each has a test or casefile asserting today's behaviour), so each needs its ADR amended. Fix 5 is a bug against documented behaviour. The numbers are those of the review that found them.

For every fix:

- Add a bullet under `## Unreleased` in `docs/changelog.md`.
- Put the reason for the behaviour in the new casefile's description ([docs/testing.md](docs/testing.md)).
- Done when the listed tests pass and `just check` passes.

## 1. List Rust macro calls that span several lines

**Today**: a macro call is never a declaration, so code wrapped in one leaves a hole in the outline. In `portable-atomic-1.15.0/src/lib.rs` nothing is listed between L712 and L2746, which sit inside `cfg_has_atomic_8! { … }`. The same happens with `cfg_if!`, `thread_local!`, `bitflags!`, and `lazy_static!`.

**Target**: a macro call that spans more than one line is a declaration, at every depth where declarations are listed. Its outline line prints the macro's path and `!`; its line range starts at its first leading annotation, like any declaration.

```rust
cfg_if::cfg_if! {
    if #[cfg(unix)] {
        pub fn unix_only() {}
    }
}

thread_local! {
    static COUNTER: Cell<u32> = Cell::new(0);
}

include!("generated.rs");
```

```
[L1-L5] cfg_if::cfg_if!
[L7-L9] thread_local!
```

- A call on one line (`include!("generated.rs");`) stays unlisted: it hides no lines, and files hold many.
- Nothing inside a call is listed. The grammar gives a call's arguments as a token tree, not as declarations; listing them is a separate change.
- The highlight ends after the `!`, as `macro_rules! name` ends after its name.

**Where**: `signature_end` in `src/languages/rust.rs` returns `None` for a macro call. Add an arm for the call's node kind that returns the end of its `!` token when the node spans more than one line. The kind is expected to be `macro_invocation`, a direct child of `source_file`; confirm it, and whether a call ending in `;` is wrapped in `expression_statement`, by printing `Node::to_sexp` for the example above before writing the arm. The line range, leading annotations, and depth then come from the shared code in `src/code_outline.rs`.

**Tests and docs**:

- `src/languages/rust.rs`: `private_uses_and_macro_calls_are_not_listed` asserts today's behaviour for `thread_local! { static A: u8 = 1; }`, a one-line call. Rename it to say one-line calls, and add a multi-line case to `every_declaration_kind_prints_its_signature` or a test of its own.
- New casefile `tests/cases/rust/macro-calls.case`: a multi-line call with a path, one with a doc comment and an attribute above it, a one-line call, and a multi-line call inside an inline `mod`. Add a debug-mode casefile for the highlight.
- Add a multi-line macro call to `tests/engine-samples/sample.rs`.
- Check what the glob view's declaration summary prints for a headerless file that starts with a multi-line call, including one cut by the 4 KiB prefix ([ADR-024](docs/adr.md)); add a casefile for it.
- Amend the Rust engine's ADR (the paragraph at `docs/adr.md:228` that says macro calls aren't listed) with the date and the reason: the hole measured above.
- Update the `//!` header of `src/languages/rust.rs`, the doc comment of `signature_end`, and the Declaration entry in [docs/GLOSSARY.md](docs/GLOSSARY.md), which lists the kinds of declaration. The docs call this a "macro call"; keep that term.

## 2. List TypeScript and JavaScript destructuring declarations

**Today**: `variable_lines` in `src/languages/script.rs` keeps only declarators whose name is an `identifier`, so a destructuring declarator prints nothing. Over a 1,100-file TypeScript monorepo these were the only missed top-level declarations (32 of 14,150).

**Target**: a declarator whose name is a pattern prints the pattern as written, on one line, after the declaration's keywords.

```ts
export const { a, b } = load()
const [first, ...rest] = process.argv
const plain = 1, { c } = obj
```

```
[L1-L1] export const { a, b }
[L2-L2] const [first, ...rest]
[L3-L3] const plain
[L3-L3] const { c }
```

- The value is never printed, including a function value: `const { a } = () => 1` prints `const { a }`.
- The highlight ends at the end of the pattern.
- The rule is the same for `let`, `var`, `using`, and in `.js`, `.jsx`, `.ts`, and `.tsx` files.

**Where**: in `variable_lines`, replace the `.filter(|name| name.kind() == "identifier")` guard with a match on the name's kind: an `identifier` keeps today's path; `object_pattern` and `array_pattern` print `level.one_line(name)` with `highlight_end` at the name's end. Confirm the two pattern kinds are the same in the JavaScript and TypeScript grammars.

**Tests and docs**:

- `tests/cases/typescript/debug-const-binding-pattern-skipped.case` asserts today's behaviour. Rewrite it for the target (the pattern is highlighted) and rename it, since "skipped" no longer holds.
- New casefile `tests/cases/typescript/destructuring-declarations.case` with the example above plus a pattern that wraps over several lines and one with a default value (`{ a = 1 }`).
- Add a destructuring declaration to `tests/engine-samples/sample.ts` and `sample.js`.
- Amend the script engine's ADR where it lists what is and isn't a declaration, and update the `//!` header of `src/languages/script.rs` if it names variable declarations.

## 5. One glob view line per file, however its path is spelled

**Today**: the glob view removes duplicates by comparing display paths as strings (`Listing.files` in `src/glob_view.rs`), so two spellings of one file print twice:

- `outln src ./src` prints every file as `./src/x.rs` and as `src/x.rs`.
- `outln "src/l*.rs" ./src/lib.rs src//lib.rs` prints `lib.rs` three times.

[docs/architecture.md](docs/architecture.md) says the glob view is "sorted by path across all its arguments, without duplicates".

**Target**: the glob view lists each display path in one normal form, so spellings of the same path collapse to one line:

| Given            | Listed as    |
| ---------------- | ------------ |
| `./src/lib.rs`   | `src/lib.rs` |
| `src//lib.rs`    | `src/lib.rs` |
| `src/./lib.rs`   | `src/lib.rs` |
| `./src`, `src/`  | `src/…`      |
| `.`, `./`        | no prefix    |
| `/abs//x.rs`     | `/abs/x.rs`  |
| `../docs/til.md` | unchanged    |

- The normal form is lexical only: drop `.` segments and empty segments, keep a leading `/`, keep `..` segments. Resolving `..` or symlinks needs the filesystem and can change which file a path names, so `src/../src/lib.rs` and a symlinked spelling stay distinct.
- The `glob view: …` first line and every error message keep the arguments as given.
- File mode is unchanged: it prints each argument as given, in argument order, repeats included.
- A directory summary's path (`--depth`) uses the same normal form.

**Where**: one function in `src/discovery.rs` that returns the normal form of a display path (borrow the input when it is already normal; most are). `discover` in `src/glob_view.rs` applies it to each directory and file argument before `FileWalker::files_in`, `Listing::add_walked`, and `listing.files.insert`; `directory_at_depth_limit` measures its prefix from the directory it is given, so it must get the same normalized directory as the walk. `FileWalker::glob_matches` strips one leading `./` today; replace that with the same function applied to the pattern's literal base, leaving the glob syntax untouched.

**Tests and docs**:

- Unit test in `src/discovery.rs` for the table above.
- New casefiles in `tests/cases/glob-view/`: a directory given as `input` and `./input`; a file given three ways alongside a glob that matches it; `--depth 1` with `input` and `./input/`, checking each directory summary prints once with its files counted once.
- Debug mode walks a directory too: check `outln --debug ./dir` and keep its paths consistent with the glob view's.
