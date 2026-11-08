---
description: 'Notable changes to outln by release.'
readWhen: 'finishing a change that users or agents would notice'
---

# Changelog

Add entries as bullets under `## Unreleased`; `just release` renames it to the version and date ([runbook.md](runbook.md), Release).

## Unreleased

- File mode: an outline's first line prints the file's line count after its path, as the glob view does: `src/cli.rs (120 lines)` ([ADR-020](adr.md)).
- Rust: the members of an `impl` block or trait (functions, constants, associated types) and the items of an inline `mod` are listed one level deeper, each with its own line range; an `impl` block inside an inline module lists its members at depth 2, and nothing deeper is listed ([ADR-011](adr.md)).
- TypeScript: an exported import alias, `export import Alias = NS.member`, is listed as `export import Alias`, as an unexported one is listed as `import Alias`.
- Code files nested tens of thousands of levels deep (a Rust type `Vec<Vec<…>>` with a comment inside, a parameter default of nested arrays, `describe.only.only…`) no longer crash outln with a stack overflow, which lost the output of every other file in the run.
- Header comments, in every engine but Markdown's:
  - A comment whose first prose paragraph is a license notice (`Copyright …`, `SPDX-License-Identifier: …`, `Licensed under …`, `This Source Code Form …`, `@license …`, `This file is part of X` with a license pointer) is passed over for the next comment; a file with only a license comment has no header.
  - Paragraphs that aren't prose are skipped for the next one: a fenced code block (` ```compile_fail `), and lines that are only badges (`[![github]](…)`), link reference definitions, or HTML tags (`<br>`). A comment with no prose paragraph is passed over.
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
