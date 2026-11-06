---
description: 'Module layout, data flow, and the shared design every language engine builds on in the outln Rust crate.'
readWhen: 'adding or moving modules, adding a language engine, or changing how data flows through the CLI'
---

# Architecture

## Layout

One Cargo package, `outln`, with a library and a binary:

- `src/main.rs`: the binary, the imperative shell. It reads the process state (arguments, working directory, the global gitignore's path), opens stdout and stderr, and turns the result into an exit code; a closed pipe exits 0.
- `src/lib.rs`: `run`, `Context` (what `run` needs from the process), and `ExitStatus`. It parses the arguments and picks the mode.
- `src/cli.rs`: the clap definition and help text; parsing returns help, version, and usage errors as a `clap::Error` that `run` writes to the injected writers.
- Modes: `src/file_mode.rs`, `src/glob_view.rs`, `src/debug_mode.rs`.
- `src/discovery.rs`: classifies each path argument once (`ArgumentKind`: existing or missing directory or file, or glob) and walks directories and globs with the `ignore` crate, keeping supported files.
- `src/per_file.rs`: the loop file mode and debug mode share: render each file, print it, report failures.
- `src/source_file.rs`: loads one file argument: picks its engine, reads it, normalizes its text.
- `src/source_text.rs`: normalization (UTF-8 only, BOM stripped, CRLF to LF), `ByteRange`, and `LineIndex`, which turns byte ranges into line ranges.
- `src/outline.rs`: the data engines return (`Outline`, `Header`, `OutlineLine`, `FileSummary`) and its rendering.
- `src/language_engine.rs`: the `LanguageEngine` trait and the `ENGINES` registry; `src/language_engine/contract.rs` holds the unit tests every registered engine must pass ([ADR-017](adr.md)).
- `src/languages/<language>.rs`: one engine per language: `src/languages/markdown.rs`, `src/languages/script.rs` for TypeScript and JavaScript (with its children in `src/languages/script/`), `src/languages/rust.rs`, and `src/languages/data_file.rs` for TOML, YAML, and JSON (with one child per format in `src/languages/data_file/`).
- Shared by the tree-sitter engines: `src/syntax_tree.rs` (tree-sitter parsing, node byte ranges, a node's source on one line) and `src/header_comment.rs` (`//` and `/* */` or `#` header comments, with doc markers); by the code-language engines: `src/code_outline.rs` (the outline loop: each top-level node with its line range becomes outline lines), `src/signature.rs` (`Signature` and the outline line it makes) and `src/leading_annotations.rs` (where a declaration's line range starts). The data file engine finds its keys' line ranges itself, because its grammars put a key's comments inside the key before.
- `src/input_error.rs`: `InputError`, every per-input failure with its stderr message.
- `tests/`: see [testing.md](testing.md).

New modules go under `src/` as `src/<name>.rs` (with `src/<name>/` for its children), one purpose per file.

## Data flow

1. `main` builds a `Context` and calls `outln::run(arguments, &context, &mut stdout, &mut stderr)`.
2. `run` parses the arguments and classifies each path. With `--debug` it runs debug mode; otherwise the glob view if any path is a directory (existing or not) or a glob, else file mode.
3. Each mode loads files through `source_file::load`, asks the file's engine for an `Outline` (file mode, debug mode) or a `FileSummary` (glob view: a header, a data file's field summary, a Markdown file's first heading, or nothing), and renders it with shared code.
4. A failing input is written to `err` as an `InputError` and makes the status `ExitStatus::Failure`; the other inputs are still printed. Only a failed write to `out` or `err` ends `run` early, as an `io::Error`.

## Adding a language engine

1. Add `src/languages/<language>.rs` with a struct implementing `LanguageEngine`, and declare it in `src/languages.rs`. A language with several grammars (TypeScript, TSX, JavaScript) gets one engine type with the grammar as a field, like `Script`.
2. Add it to `ENGINES` in `src/language_engine.rs`, once per grammar.
3. Return data, not text: an `Outline` whose `OutlineLine`s carry the printed text plus byte ranges (`range` for the line range, `highlight` for debug mode) and a `depth` for nested declarations. `file_summary` returns a `FileSummary` (usually `FileSummary::from` the header) and should avoid parsing, because the glob view calls it for every file: code engines find the header line by line, and the data file engine parses only a prefix for its field summary ([ADR-016](adr.md)).
4. A code-language engine's `outline` is one call to `code_outline::code_outline` with its grammar, its header, a function classifying the nodes before a declaration as doc comments, attributes, or plain comments (for leading annotations; a node so classified is never asked for outline lines), and a function turning one top-level node and its line range into outline lines, as `Rust` and `Script` do; it prints signatures with `syntax_tree::one_line_source` (or `one_line_source_before`, up to the body) into a `signature::Signature`. For header comments, call `header_comment::find_header_comment` with the language's `CommentSyntax` (`CFamily` with what its doc markers mean and whether its block comments nest, or `Hash` for `#` lines) and the preamble it allows before the comment (a shebang, directives, inner attributes; preamble is checked before comments, so a directive may look like one). A new comment syntax is a new `CommentSyntax` variant; only a header that isn't a comment needs a new `Header` variant, and the compiler then points at every `match` that must render it.
5. Add an engine sample per grammar, `tests/engine-samples/sample.<first extension>`, with a header and declarations of most kinds ([testing.md](testing.md)).
6. Add its casefiles to `tests/cases/<language>/` ([testing.md](testing.md)).
7. Record the parser choice as an ADR and the new language in [overview.md](overview.md) and [changelog.md](changelog.md).

Done when `just case <language>` passes, `just run` on a file of the language prints its outline, and `just check` passes.

## Decisions

The reasons behind this design are in [adr.md](adr.md); `outln docs/adr.md` lists the records.
