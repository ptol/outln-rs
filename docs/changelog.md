---
description: 'Notable changes to the outln Rust rewrite by release, including deliberate divergences from the TypeScript build.'
readWhen: 'finishing a change that users or agents would notice'
---

# Changelog

Add entries as bullets under `## Unreleased`; `just release` renames it to the version and date ([runbook.md](runbook.md), Release).

## Unreleased

## 0.2.0 (2026-11-06)

- Rust support in every mode (`.rs`), ported from the TypeScript build with its casefiles in `tests/cases/rust/`. Deliberate differences from the TypeScript build:
  - **Header comments**: the first `//!` or `/*!` comment before the first code is the header, even after a `//` license comment or `#![...]` inner attributes (nested `/* */` comments included); without one, the first plain `//` or `/* */` comment is. `///` and `/**` comments are never the header. The TypeScript build read only plain `//` lines, so this repo's own files had no header.
  - **Signatures** print the source as written on one line (`pub(crate) fn parse<'a>(text: &'a str) -> Tree where T: Clone`, `impl<T: Display> Render for Wrapper<T>`, `const MAX: usize`, `type Id = u64`) instead of keyword and name with spaces (`fn main ()`, `impl Render for Wrapper`, `const MAX`, `type Id`).
  - **`use`**: only re-exports (`pub use`, `pub(crate) use`, and other visibilities) are listed, like TypeScript's `export { … } from`; the TypeScript build listed every `use`.
  - **Line ranges** start at the declaration's `///` doc comments and `#[...]` attributes.
  - **Debug mode** highlights a signature up to its body, a `where` clause on its own lines included.
- Every engine: a header comment prints only its first paragraph, up to the first comment line without text when more text follows (a ` *` or `//` line, or a blank line inside a block comment).
- TypeScript and JavaScript: a `/** */` comment directly above a declaration or class member starts its line range, as decorators do; a header comment never does.

## 0.1.0 (2026-11-06)

- Releases are published to crates.io as `outln` (`cargo install outln --locked`) by a GitHub Actions workflow when a `v<version>` tag is pushed ([ADR-010](adr.md)).
- TypeScript and JavaScript support in every mode (`.ts`, `.mts`, `.cts`, `.d.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`), ported from the TypeScript build with its casefiles in `tests/cases/typescript/`. Deliberate differences from the TypeScript build:
  - **New declarations**: a variable holding an arrow function or function expression prints its signature (`export const parse = (input: string): Ast`); `export default <expression>` prints as `export default defineConfig(…)`, `{…}`, `[…]`, an arrow function's signature, or `…`; top-level `describe`/`it`/`test`/`suite` test blocks (with `.only`, `.skip`, `.each`) print as `describe('parser')`; `export declare` statements are listed; JavaScript `using` declarations are listed like `const`.
  - **One-line signatures**: a signature wrapped over several lines prints on one line instead of copying its line breaks, comments inside it are left out, and a line break inside a template literal prints as `\n`. Debug mode highlights it on every line up to its body, not only the first.
  - **Highlights** cover the declaration's source as written (from `export` or the first keyword to the end of its signature) instead of as many columns as the printed text has.
  - **Class members**: a member's line range includes its decorators, as a class's does. `declare const enum` keeps its `const`.
  - **Header comments**: the glob view strips `///` and `//!` markers and drops marker-only lines, as file mode does (it printed `/ Triple.` and double spaces), debug mode highlights a block comment only up to its `*/`, and an empty `/**/` header prints nothing instead of `/`.
  - **Wording**: the glob view says `Includes only headers per file.` and `(no header available)`, since a Markdown file's header is its frontmatter.
- `just build-static` builds a static musl binary, compiling the C grammars with zig.
- Markdown support in file mode, glob view, and debug mode, ported from the TypeScript build, with a casefile runner (`tests/casefiles.rs`) and 81 casefiles. Deliberate differences from the TypeScript build:
  - **Markdown parsing**: headings come from a CommonMark parser (pulldown-cmark). Lines inside `~~~` fences, indented code, and HTML blocks are no longer reported as headings, a ` ``` ` line inside a `~~~` fence no longer breaks fence tracking, underlined (setext) headings and `#` headings indented up to three spaces are reported, a lone `#` is an empty heading, and headings inside block quotes and lists are skipped. Frontmatter list items (`- x`) and comments (`# x`) are no longer printed as fields.
  - **Debug mode**: an existing path containing glob characters (`app/[id]/page.md`) is outlined instead of rejected, and a `{a,b}` pattern is rejected as a glob. Files are separated by a blank line even when one has no final newline.
  - **Glob patterns**: every `{...}` group counts as glob syntax (not only the first), alternatives may contain `/`, and a pattern without `**` walks only as deep as it can match.
  - **Unreadable directories**: a directory below a directory argument that can't be listed is reported (`Directory <path> could not be read: ...`) and fails the run; the other files are still printed. Glob walks skip such directories silently.
  - **CLI**: help, version, and usage-error texts come from clap. The version prints as `outln <version>`, usage errors (unknown option, no paths, `--debug` without a path) exit 2 instead of 1, and errors use clap's wording.
  - **Directory walking**: git semantics through the `ignore` crate. A `.gitignore` outside a git repository is ignored, `.ignore` files and `.git/info/exclude` are honoured, git's global excludes file applies, and ancestor rules still apply to an explicitly named directory they ignore. Files are sorted by byte order instead of locale order.
  - **Encoding**: a file that isn't valid UTF-8 is reported as `FILE <path> IS NOT UTF-8 TEXT` instead of being decoded with replacement characters, and an unreadable file as `FILE <path> COULD NOT BE READ`.
  - **Unsupported files**: an unsupported file is reported as unsupported without being read.
