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
- `src/language_engine.rs`: the `LanguageEngine` trait and the `ENGINES` registry.
- `src/languages/<language>.rs`: one engine per language; `src/languages/markdown.rs` is the first.
- `src/input_error.rs`: `InputError`, every per-input failure with its stderr message.
- `tests/`: see [testing.md](testing.md).

New modules go under `src/` as `src/<name>.rs` (with `src/<name>/` for its children), one purpose per file.

## Data flow

1. `main` builds a `Context` and calls `outln::run(arguments, &context, &mut stdout, &mut stderr)`.
2. `run` parses the arguments and classifies each path. With `--debug` it runs debug mode; otherwise the glob view if any path is a directory (existing or not) or a glob, else file mode.
3. Each mode loads files through `source_file::load`, asks the file's engine for an `Outline` (file mode, debug mode) or a `FileSummary` (glob view), and renders it with shared code.
4. A failing input is written to `err` as an `InputError` and makes the status `ExitStatus::Failure`; the other inputs are still printed. Only a failed write to `out` or `err` ends `run` early, as an `io::Error`.

## Adding a language engine

1. Add `src/languages/<language>.rs` with a unit struct implementing `LanguageEngine`, and declare it in `src/languages.rs`.
2. Add it to `ENGINES` in `src/language_engine.rs`.
3. Return data, not text: an `Outline` whose `OutlineLine`s carry the printed text plus byte ranges (`range` for the line range, `highlight` for debug mode). `file_summary` returns only the header and should stop parsing once it has it, because the glob view calls it for every file.
4. If the language has header comments, add a `Header` variant for them; the compiler then points at every `match` that must render it.
5. Port its TypeScript casefiles to `tests/cases/<language>/` ([testing.md](testing.md)).
6. Record the parser choice as an ADR and the new language in [overview.md](overview.md) and [changelog.md](changelog.md).

Done when `just case <language>` passes, `just run` on a file of the language prints its outline, and `just check` passes.

## Decisions

The reasons behind this design are in [adr.md](adr.md); `outln docs/adr.md` lists the records.
