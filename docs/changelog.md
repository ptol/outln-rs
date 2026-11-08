---
description: 'Notable changes to outln by release.'
readWhen: 'finishing a change that users or agents would notice'
---

# Changelog

Add entries as bullets under `## Unreleased`; `just release` renames it to the version and date ([runbook.md](runbook.md), Release).

## Unreleased

- File mode: an outline's first line prints the file's line count after its path, as the glob view does: `src/cli.rs (120 lines)` ([ADR-020](adr.md)).
- Faster on large inputs: directories are walked and files outlined on several threads, and stdout is buffered. A glob view of 18k files takes about 0.1 s instead of 0.9 s, and file mode on 3 000 files 0.4 s instead of 1.6 s. Output is unchanged and in the same order, and when stdout and stderr go to one pipe, each error still follows the output printed before it ([ADR-022](adr.md)).

- Error messages are all sentence case: `File <path> is not UTF-8 text`, `File <path> could not be read: permission denied` (now with the reason), and `File <path> has an unsupported file type; supported: .md, .ts, ...`, which lists every supported extension. Debug mode says `--debug requires exactly one file or directory path, not 2` or `... not the glob pattern <pattern>`, and reports a missing argument ending in `/` as `Directory <path> does not exist`, as the glob view does.
- Path arguments: a file inside a directory outln may not search is reported as `File <path> could not be read: permission denied`, not as missing; an existing file given with a trailing `/` as `File <path>/ is not a directory; remove the trailing /`; a FIFO, socket, or device as `File <path> is not a regular file (a FIFO, socket, or device)`, where a FIFO used to block forever; and an empty argument (`outln ""`) is a usage error (exit 2) instead of a glob view of the working directory.
- NUL bytes: a valid UTF-8 file with a few NUL bytes (a raw NUL in a JavaScript string) is outlined instead of rejected. A file whose NULs say UTF-16 or binary (invalid UTF-8 with NULs, a NUL in the first two bytes, or more than one NUL per 16 bytes) is reported as `File <path> looks like a binary or UTF-16 file, not UTF-8 text`; invalid UTF-8 without NULs is still `File <path> is not UTF-8 text`.
- `.jsonc` files (JSON with comments, such as `knip.jsonc`) are outlined by the JSON engine instead of skipped. `.json5` isn't supported: its unquoted keys and single quotes aren't JSON's grammar.
- Debug mode prints green only when stdout is a terminal and `NO_COLOR` isn't set. Otherwise, as for agents and pipes, each highlighted part of a line is wrapped in `«` and `»` instead of ANSI escape codes: `«export function parse(input: string): Ast» {`. Library callers choose with `Context::highlight_style`.

## 0.3.0 (2026-11-06)

- Glob view: each line prints the file's line count after its path, `src/cli.rs (120 lines): Command-line arguments.` (`1 line`, `0 lines` for an empty file), and the two lines after `glob view:` (`Includes only ... per file.` and `For file-level outlines, use ...`) are gone; the README has a section to paste into an agent's instructions instead ([ADR-019](adr.md)).
- CSS support in every mode (`.css`), with casefiles in `tests/cases/css/` ([ADR-018](adr.md)):
  - **Outline lines** are rules: a style rule prints its selectors (`.button, .nav > a:hover`), an at-rule its keyword and prelude (`@media (max-width: 600px)`, `@import url("reset.css")`, `@font-face`), on one line. The rules inside a top-level at-rule's block (`@media`, `@supports`, `@layer`, `@container`) are listed one level deeper; rules nested in a style rule and `@keyframes` steps aren't. `@charset` isn't listed.
  - **Header comments**: the first `/* */` comment, after `@charset`; `//` lines aren't comments.
  - **Line ranges** start at the comment directly above a rule; a blank line detaches it.
- TOML, YAML, and JSON support in every mode (`.toml`, `.yaml`, `.yml`, `.json`) ([ADR-016](adr.md)):
  - **Outline lines** are top-level keys: a scalar value prints after its key as written, cut after its first line or 40 characters (`name: CI`, `edition = "2024"`); tables, mappings, objects, and lists print only the key. TOML lists its root keys, then every `[table]` and `[[array]]` header as written; YAML outlines only its first document.
  - **Line ranges** start at the comment lines directly above a key, and end before the blank and comment lines after its value.
  - **Header comments** are the first `#` comment (`//` or `/* */` in JSON) after directives such as `#:schema`, `%YAML`, `---`, and `# yaml-language-server:`.
  - **Glob view**: a data file without a header comment prints a field summary of its first three scalar keys, `{name: "outln", version: "0.2.0", private: true, ...}`, or the first TOML table's (`[package] {name = "outln", ...}`), read from the file's first 64 KiB.
- Markdown: a glob view prints a file's first heading, as its outline line prints it (`# User Guide`), when the file has no frontmatter fields, instead of `(no header available)`. File mode is unchanged.
- `--help` says the glob view prints "one line per file with its line count and its header or summary".
- Every engine: preamble (a shebang, directives) is now checked before a comment when looking for the header comment; no TypeScript, JavaScript, or Rust preamble looks like a comment, so their output is unchanged.
- Agents in this repo navigate with the published build ([ADR-014](adr.md)).

## 0.2.0 (2026-11-06)

- Rust support in every mode (`.rs`), with casefiles in `tests/cases/rust/`:
  - **Header comments**: the first `//!` or `/*!` comment before the first code is the header, even after a `//` license comment or `#![...]` inner attributes (nested `/* */` comments included); without one, the first plain `//` or `/* */` comment is. `///` and `/**` comments are never the header.
  - **Signatures** print the source as written on one line (`pub(crate) fn parse<'a>(text: &'a str) -> Tree where T: Clone`, `impl<T: Display> Render for Wrapper<T>`, `const MAX: usize`, `type Id = u64`).
  - **`use`**: only re-exports (`pub use`, `pub(crate) use`, and other visibilities) are listed, like TypeScript's `export { … } from`.
  - **Line ranges** start at the declaration's `///` doc comments and `#[...]` attributes.
  - **Debug mode** highlights a signature up to its body, a `where` clause on its own lines included.
- Every engine: a header comment prints only its first paragraph, up to the first comment line without text when more text follows (a ` *` or `//` line, or a blank line inside a block comment).
- TypeScript and JavaScript: a `/** */` comment directly above a declaration or class member starts its line range, as decorators do; a header comment never does.

## 0.1.0 (2026-11-06)

- Releases are published to crates.io as `outln` (`cargo install outln --locked`) by a GitHub Actions workflow when a `v<version>` tag is pushed ([ADR-010](adr.md)).
- TypeScript and JavaScript support in every mode (`.ts`, `.mts`, `.cts`, `.d.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`), with casefiles in `tests/cases/typescript/`:
  - **New declarations**: a variable holding an arrow function or function expression prints its signature (`export const parse = (input: string): Ast`); `export default <expression>` prints as `export default defineConfig(…)`, `{…}`, `[…]`, an arrow function's signature, or `…`; top-level `describe`/`it`/`test`/`suite` test blocks (with `.only`, `.skip`, `.each`) print as `describe('parser')`; `export declare` statements are listed; JavaScript `using` declarations are listed like `const`.
  - **One-line signatures**: a signature wrapped over several lines prints on one line, comments inside it are left out, and a line break inside a template literal prints as `\n`. Debug mode highlights it on every line up to its body.
  - **Highlights** cover the declaration's source as written (from `export` or the first keyword to the end of its signature).
  - **Class members**: a member's line range includes its decorators, as a class's does. `declare const enum` keeps its `const`.
  - **Header comments**: the glob view strips `///` and `//!` markers and drops marker-only lines, as file mode does, debug mode highlights a block comment only up to its `*/`, and an empty `/**/` header prints nothing.
  - **Wording**: the glob view says `Includes only headers per file.` and `(no header available)`, since a Markdown file's header is its frontmatter.
- `just build-static` builds a static musl binary, compiling the C grammars with zig.
- Markdown support in file mode, glob view, and debug mode, with a casefile runner (`tests/casefiles.rs`) and 81 casefiles:
  - **Markdown parsing**: headings come from a CommonMark parser (pulldown-cmark). Lines inside `~~~` fences, indented code, and HTML blocks aren't headings, a ` ``` ` line inside a `~~~` fence doesn't close it, underlined (setext) headings and `#` headings indented up to three spaces are reported, a lone `#` is an empty heading, and headings inside block quotes and lists are skipped. Frontmatter list items (`- x`) and comments (`# x`) aren't printed as fields.
  - **Debug mode**: an existing path containing glob characters (`app/[id]/page.md`) is outlined, and a `{a,b}` pattern is rejected as a glob. Files are separated by a blank line even when one has no final newline.
  - **Glob patterns**: every `{...}` group counts as glob syntax, alternatives may contain `/`, and a pattern without `**` walks only as deep as it can match.
  - **Unreadable directories**: a directory below a directory argument that can't be listed is reported (`Directory <path> could not be read: ...`) and fails the run; the other files are still printed. Glob walks skip such directories silently.
  - **CLI**: help, version, and usage-error texts come from clap. The version prints as `outln <version>`, usage errors (unknown option, no paths, `--debug` without a path) exit 2, and errors use clap's wording.
  - **Directory walking**: git semantics through the `ignore` crate. A `.gitignore` outside a git repository is ignored, `.ignore` files and `.git/info/exclude` are honoured, git's global excludes file applies, and ancestor rules still apply to an explicitly named directory they ignore. Files are sorted by byte order.
  - **Encoding**: a file that isn't valid UTF-8 is reported as `FILE <path> IS NOT UTF-8 TEXT`, and an unreadable file as `FILE <path> COULD NOT BE READ`.
  - **Unsupported files**: an unsupported file is reported as unsupported without being read.
