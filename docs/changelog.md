---
description: 'Notable changes to outln by release.'
readWhen: 'finishing a change that users or agents would notice'
---

# Changelog

Add entries as bullets under `## Unreleased`; `just release` renames it to the version and date ([runbook.md](runbook.md), Release).

## Unreleased

## 0.4.2 (2026-11-10)

- Rust: a macro call that spans several lines is listed, at every depth, as the macro's path and `!` (`[L1-L5] cfg_if::cfg_if!`), so code wrapped in `cfg_if!`, `thread_local!`, or `bitflags!` no longer leaves a hole in the outline. A call on one line stays unlisted, and nothing inside a call is listed.
- TypeScript and JavaScript: a destructuring declaration is listed with its pattern as written, on one line and without its value: `export const { a, b }`, `const [first, ...rest]`. A declaration summary keeps a pattern's default value whole (`const { a = make() }`) instead of cutting it at the `(`.
- Glob view: each file prints once however its path is spelled. Paths are listed in a lexical normal form, without `.` and empty segments: `outln src ./src` lists `src/lib.rs` once instead of twice, and `./src//lib.rs` lists as `src/lib.rs`. `..` segments and symlinks aren't resolved. The `glob view:` line and errors about an argument keep it as given; an error about a listed file names its listed path. File mode is unchanged.

## 0.4.1 (2026-11-09)

- Publishes 0.4.0's changes (below) to crates.io: 0.4.0 was tagged but never uploaded, because CI couldn't install rustfmt for the pinned toolchain.

## 0.4.0 (2026-11-09)

- Every engine: a lone CR (a file with classic Mac OS line endings) is a line break, as CRLF already was: such a file's line count, line ranges, and header comment count each CR-ended line, and no raw CR is printed.
- CSS: a style rule without selectors (`{ color: red }`, in a half-edited file) is no longer listed as a blank outline line.
- Markdown frontmatter is read as YAML, by the YAML engine's rules ([ADR-023](adr.md)):
  - **Values written over several lines** (a quoted or plain scalar continued on the next line, a `|` or `>-` block) print their first paragraph on one line, its lines joined by spaces, as a JSON string: `description: "Starts here and continues here"` instead of `description: "\"Starts here and"` or `description: "|"`. File mode still prints every value uncut.
  - **Lists and mappings** print their key alone (`tags`) instead of `tags: null`, as in a YAML file's outline. Debug mode highlights a field from its key to the end of its value.
  - **Delimiters**: `---` may be followed by whitespace, and `...` (YAML's document end) closes the block too. A `---` block that holds no YAML mapping (`---`, `# Title`, `Some text.`, `---`) is body text, so its headings are listed; one whose YAML breaks partway keeps the fields before the error.
  - **Glob view**: a file summary shows at most three fields, `description` first, then `title`, then the others in source order, with `...` when more follow.
- Data files ([ADR-023](adr.md)):
  - **Field summary**: `description`, then `title`, come first, then the other scalar keys in source order, three in all (`{description: "...", name: "demo", private: true, ...}`, `[package] {description = "...", name = "outln", version = "0.3.0", ...}`); a file without scalar keys prints its keys' names, `{compilerOptions, include}`, instead of `(no header available)`. A describing key written in quotes (`"description"`, `'title'`) counts as one, in frontmatter too, and keeps its quotes.
  - **Values**: a `description` or `title` value is cut at 100 characters instead of 40, in outline lines too, and a cut quoted value ends with its quote after the `…` (`"Playwright smoke test…"`). A value written over several lines (a YAML `|` block, a TOML `"""` string) prints its first paragraph with its lines joined by spaces instead of only its first line; so does a key (`? [a,\n b]`).
  - **TOML**: a table header that doesn't close on its line (`[[bin` in a half-edited file) prints only that line, in outline lines and a field summary's table name.
  - **YAML**: an empty first document (`---` right before another `---`) is skipped, so the next document's keys are listed; a root mapping broken by a syntax error lists its keys before the error.
  - **Deeply nested YAML** (`- ` written 254 times on one line, in a YAML file or Markdown frontmatter) is read up to the line that might nest too deep, instead of aborting the whole run with a tree-sitter assertion. The estimate counts indentation and the indicators `- `, `? `, `: `, `|`, and `>` outside comments and outside quoted strings that close on their line, so a file is read whole unless about 100 of them stack up, nested or on one line.
  - **YAML**: a key of whitespace only (a lone U+2028 line separator) prints quoted with escapes, `"\u{2028}"`, instead of as a blank outline line.
  - **JSON**: an object missing a comma (`"version": "1.0.0"` followed by `"private": true`, in a half-edited file) lists the keys before the gap too, instead of only those after it.
  - **YAML**: a key written over several lines that ends in a comment line (`? key` then `# comment`) gets a line range that holds the whole key, so debug mode's highlight of it no longer runs past its line range.
- File mode: an outline's first line prints the file's line count after its path, as the glob view does: `src/cli.rs (120 lines)` ([ADR-020](adr.md)).
- Faster on large inputs: directories are walked and files outlined on several threads, and stdout is buffered. A glob view of 18k files takes about 0.1 s instead of 0.9 s (measured before declaration summaries, which parse each headerless code file's start: about 0.5 s on that tree, where half the files have no header, [ADR-024](adr.md)), and file mode on 3 000 files 0.4 s instead of 1.6 s. Output is unchanged and in the same order, and when stdout and stderr go to one pipe, each error still follows the output printed before it ([ADR-022](adr.md)).
- Error messages are all sentence case: `File <path> is not UTF-8 text`, `File <path> could not be read: permission denied` (now with the reason), and `File <path> has an unsupported file type; supported: .md, .ts, ...`, which lists every supported extension. Debug mode says `--debug requires exactly one file or directory path, not 2` or `... not the glob pattern <pattern>`, and reports a missing argument ending in `/` as `Directory <path> does not exist`, as the glob view does.
- Path arguments: a file inside a directory outln may not search is reported as `File <path> could not be read: permission denied`, not as missing; an existing file given with a trailing `/` as `File <path>/ is not a directory; remove the trailing /`; a FIFO, socket, or device as `File <path> is not a regular file (a FIFO, socket, or device)`, where a FIFO used to block forever. These hold for a path with glob characters too (`x[1].rs/`, `secret/[id].rs`), which used to become a glob pattern matching nothing. An empty argument (`outln ""`) is a usage error (exit 2) instead of a glob view of the working directory.
- NUL bytes: a valid UTF-8 file with a few NUL bytes (a raw NUL in a JavaScript string) is outlined instead of rejected. A file whose NULs say UTF-16 or binary (invalid UTF-8 with NULs, a NUL in the first two bytes, or more than one NUL per 16 bytes) is reported as `File <path> looks like a binary or UTF-16 file, not UTF-8 text`; invalid UTF-8 without NULs is still `File <path> is not UTF-8 text`.
- `.jsonc` files (JSON with comments, such as `knip.jsonc`) are outlined by the JSON engine instead of skipped. `.json5` isn't supported: its unquoted keys and single quotes aren't JSON's grammar.
- Debug mode prints green only when stdout is a terminal and `NO_COLOR` isn't set to a non-empty value (`--help` says so too). Otherwise, as for agents and pipes, each highlighted part of a line is wrapped in `«` and `»` instead of ANSI escape codes: `«export function parse(input: string): Ast» {`. Library callers choose with `Context::highlight_style`.
- Glob view: `--depth N` lists only N levels of each directory argument (`--depth 1`: the files directly inside), and prints each directory N levels down as one directory summary in its sorted place, `packages/core/ (257 files, 31234 lines)`, instead of its files. Glob patterns set their own depth; `--depth` with `--debug`, or with only file arguments (which file mode would ignore it for), is a usage error. Unless `--depth` cut a directory argument into directory summaries, a glob view of more than 300 files ends with a hint on stderr: `Hint: found 989 files; for an overview, try --depth 1 or a narrower path` ([ADR-025](adr.md)).
- `--help` lists the supported file extensions, taken from the engine registry, and says to quote glob patterns: the shell expands an unquoted one into file arguments, and outln then prints full outlines of every matching file.
- Rust: the members of an `impl` block or trait (functions, constants, associated types) and the items of an inline `mod` are listed one level deeper, each with its own line range; an `impl` block inside an inline module lists its members at depth 2, and nothing deeper is listed ([ADR-011](adr.md)).
- TypeScript: an exported import alias, `export import Alias = NS.member`, is listed as `export import Alias`, as an unexported one is listed as `import Alias`.
- TypeScript: a signature with an object type whose members are on separate lines without `;` or `,` prints `; ` between them on its one line: `statsOf(actionKey: string): { readonly visits: number; readonly meanReward: number }` instead of running the members together.
- TypeScript and JavaScript: test blocks inside a test block's callback (the `it` and `describe` calls in a `describe`) are listed one level deeper, each with its own line range, down to depth 2.
- Glob view: a code file (Rust, TypeScript, JavaScript, CSS) without a header comment prints a declaration summary instead of `(no header available)`: its first five distinct top-level declarations up to their parameter lists (`fn f` for `fn f<F: Fn()>(f: F)`, whose bound's parentheses are no parameter list), a repeated one with its count, as `pub struct Arg, impl Arg ×6, impl From<&'_ Self> for Arg, ...` (a CSS file's first selectors). Only the first 4 KiB of such a file are parsed ([ADR-024](adr.md)).
- Code files nested tens of thousands of levels deep (a Rust type `Vec<Vec<…>>` with a comment inside, a parameter default of nested arrays, `describe.only.only…`) no longer crash outln with a stack overflow, which lost the output of every other file in the run.
- Rust: consecutive module declarations without a body, declared alike (same visibility, no attributes or doc comments) on consecutive lines, print on one outline line, `[L21-L38] mod cli, code_outline, comment_syntax, ...`, instead of one line each.
- Header comments, in every engine but Markdown's:
  - A comment whose first prose paragraph is a license notice (`Copyright …`, `SPDX-License-Identifier: …`, `Licensed under …`, `This Source Code Form …`, `@license …`, `This file is part of X` with a license pointer) is passed over for the next comment; a file with only a license comment has no header. A comment passed over is no declaration's leading annotation either, so a CSS rule, a data file's key, or a TypeScript declaration right below a license comment starts at its own line.
  - Paragraphs that aren't prose are skipped for the next one: a fenced code block (` ```compile_fail `), and lines that are only badges (`[![github]](…)`), link reference definitions, or HTML tags (`<br>`); a line with text between tags (`<b>Purpose.</b>`) is prose. A code block closes, as in CommonMark, only at a fence of its own character at least as long as the one that opened it. A comment with no prose paragraph is passed over.
  - Rows of decoration (`*****`, `=====`, `-----`) print nothing and separate paragraphs, so a banner's title prints without them; `/*!*****` no longer keeps the stars.
  - Rust: a one-line plain comment directly above code (`// Std` above `use std::…`) labels that code and is no longer the header; one separated from the code by a blank line, or running over several lines, still is. `//!` and `/*!` comments are unaffected.
  - TypeScript and JavaScript: tool directives written as comments are preamble, so the comment after them is the header: `/// <reference …/>`, `/// <amd-…>`, `// @ts-…`, `@flow`, `@jsx`, `@jest-environment`, `/* eslint… */`, `/* global … */`, `// prettier-ignore`, `// biome-ignore`, `/* istanbul ignore … */`, `/* c8 … */`, and other linter and coverage pragmas. CSS: `/* stylelint-… */`, `prettier-ignore`, and `biome-ignore` comments.

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
