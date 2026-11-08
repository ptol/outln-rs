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
- `src/source_text.rs`: normalization (UTF-8 only, BOM stripped, CRLF to LF), `ByteRange`, `LineIndex`, which turns byte ranges into line ranges, and `SourceText`, a file's text with its `LineIndex`, built once per file for the engine's `outline` and the rendering. Engines find the line around an offset with `SourceText::line_around` (a binary search), never by scanning the text back and forth, which is quadratic on long lines.
- `src/outline.rs`: the data engines return (`Outline`, `Header`, `OutlineLine`, `FileSummary`) and its rendering.
- `src/language_engine.rs`: the `LanguageEngine` trait and the `ENGINES` registry; `src/language_engine/contract.rs` holds the unit tests every registered engine must pass ([ADR-017](adr.md)).
- `src/languages/<language>.rs`: one engine per language: `src/languages/markdown.rs`, `src/languages/script.rs` for TypeScript and JavaScript (with its children in `src/languages/script/`), `src/languages/rust.rs`, `src/languages/css.rs`, and `src/languages/data_file.rs` for TOML, YAML, and JSON (with one child per format in `src/languages/data_file/`). A code language's engine is a `static CodeLanguage` (`RUST`, `CSS`, `TYPESCRIPT`, `TSX`, `JAVASCRIPT`), not a type of its own, and each data file format a `static DataFile` table (`JSON`, `TOML`, `YAML`) of what differs between formats ([ADR-021](adr.md)).
- Shared by the tree-sitter engines: `src/syntax_tree.rs` (tree-sitter parsing, node byte ranges, a node's source on one line), `src/comment_syntax.rs` (comment lexing: a language's `CommentSyntax` of line marker, block comments, and doc markers, which say whether a comment documents the file, the declaration after it, or nothing; where a block comment ends; stripping markers), and `src/header_comment.rs` (which comment at the top of a file is the header, by a `HeaderChoice`, and, in `src/header_comment/paragraph.rs`, the paragraph it prints: the first prose one, never a license notice; `directive_comment_end` lets an engine's preamble take tool directives written as comments); by the code-language engines: `src/code_outline.rs` (`CodeLanguage`, a code language as data, and its one `LanguageEngine` implementation; `OutlineLevel`, what a language's `declaration_lines` gets with each node: the text, signature helpers, and `declarations_inside`, which lists a node's declarations one level deeper with their own line ranges), `src/signature.rs` (`Signature`, the outline line it makes, and where a signature ends before a body: `Signature::source_before`, `body_start`, `before_semicolon`), and `src/leading_annotations.rs` (where a declaration's line range starts, and `Annotations`, the per-language table of which nodes are doc comments, attributes, or plain comments). The data file engine finds its keys' line ranges itself, because its grammars put a key's comments inside the key before ([techdebt.md](techdebt.md)).
- `src/input_error.rs`: `InputError`, every per-input failure with its stderr message.
- `tests/`: see [testing.md](testing.md).

New modules go under `src/` as `src/<name>.rs` (with `src/<name>/` for its children), one purpose per file.

## Data flow

1. `main` builds a `Context` and calls `outln::run(arguments, &context, &mut stdout, &mut stderr)`.
2. `run` parses the arguments and classifies each path. With `--debug` it runs debug mode; otherwise the glob view if any path is a directory (existing or not) or a glob, else file mode.
3. Each mode loads files through `source_file::load`, asks the file's engine for an `Outline` (file mode, debug mode) or a `FileSummary` (glob view: a header, a data file's field summary, a code file's declaration summary, a Markdown file's first heading, or nothing), and renders it with shared code.
4. A failing input is written to `err` as an `InputError` and makes the status `ExitStatus::Failure`; the other inputs are still printed. Only a failed write to `out` or `err` ends `run` early, as an `io::Error`.

## Adding a language engine

1. Add `src/languages/<language>.rs` and declare it in `src/languages.rs`. A code language is a `pub(crate) static <LANGUAGE>: CodeLanguage`, like `RUST`; a language with several grammars gets one static per grammar from a `const fn`, like `script` for `TYPESCRIPT`, `TSX`, and `JAVASCRIPT`. Only an engine that isn't a tree-sitter code language (Markdown, the data file engine) implements `LanguageEngine` itself.
2. Add it to `ENGINES` in `src/language_engine.rs`, once per grammar.
3. Return data, not text: an `Outline` whose `OutlineLine`s carry the printed text plus byte ranges (`range` for the line range, `highlight` for debug mode) and a `depth` for nested declarations. `file_summary` returns a `FileSummary` (usually `FileSummary::from` the header) and should avoid parsing, because the glob view calls it for every file: code engines find the header line by line and parse only a prefix of a file without one for its declaration summary (`CodeLanguage::summary_text` picks the part of each outline line it prints; [ADR-024](adr.md)), and the data file engine parses only a prefix for its field summary ([ADR-016](adr.md)). An engine that parses with tree-sitter returns its grammar from the test-only `LanguageEngine::grammar`, so the contract tests check that its engine sample parses without errors.
4. A `CodeLanguage` is data plus one function ([ADR-021](adr.md)):
   - `comment_syntax`: a `comment_syntax::CommentSyntax` of its line marker, its block comments (flat or nested), and its doc markers (`OUTER_DOC_BLOCK` for `/** */`, Rust's `///`, `//!`, `/*!`; a new doc comment form is a new `DocMarker`).
   - `header_choice`: which comment at the top is the header (`HeaderChoice::FirstComment`, or Rust's `InnerDocElseFirstPlain`; a language with another rule, such as Go's comment attached to `package`, is a new variant), and `preamble_end`, what may come before it (a shebang, directives, inner attributes; preamble is checked before comments, so a directive may look like one). Only a header that isn't a comment needs a new `Header` variant, and the compiler then points at every `match` that must render it.
   - `annotations`: the grammar's comment and attribute node kinds, and whether plain comments are stepped over or, in a language without doc comments (CSS), annotate the declaration below them.
   - `one_line`: a `syntax_tree::OneLineRules` of the literal node kinds a signature keeps as written (`verbatim_kinds`) and the node kinds whose members a line break alone may separate (`member_lists`, TypeScript's `object_type`), which get a `;` between them on one line.
   - `declaration_lines(level, node, range, lines)`: pushes the outline lines of one node with its line range, or nothing for a node that isn't a declaration. It gets every node that isn't a leading annotation, at every level, so it checks the node kind or `level.is_top_level()`. It prints signatures with `level.one_line(node)` or `level.signature_before(node, end)` (up to the body: `signature::body_start`, `before_semicolon`) and makes lines with `level.outline_line`, which applies the level's depth; `level.declarations_inside(body, lines)` lists the declarations in a node's body one level deeper (members of a class, `impl` block, or trait, a Rust module's items, the rules in a CSS at-rule), giving each its own line range, down to depth 2 (`DEEPEST_DEPTH`) in every language. `CSS` reads its preludes from the source text instead of `one_line`, because its grammar keeps parts of valid preludes out of the leaves `one_line_source` joins ([ADR-018](adr.md)).
5. Add an engine sample per grammar, `tests/engine-samples/sample.<first extension>`, with a header and declarations of most kinds ([testing.md](testing.md)).
6. Add its casefiles to `tests/cases/<language>/` ([testing.md](testing.md)).
7. Record the parser choice as an ADR and the new language in [overview.md](overview.md) and [changelog.md](changelog.md).

Done when `just case <language>` passes, `just run` on a file of the language prints its outline, and `just check` passes.

## Decisions

The reasons behind this design are in [adr.md](adr.md); `outln docs/adr.md` lists the records.
