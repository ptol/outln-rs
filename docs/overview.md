---
description: 'Goals, current status, and scope of outln.'
readWhen: 'starting any task, to learn what the project is for and how far it is'
---

# Overview

## Goal

- A small CLI that prints an outline of source files: the path and line count, the header (header comment, or frontmatter for Markdown) and one line per top-level declaration, with line ranges, so coding agents can read only what they need.
- A glob view of a tree, one line per file (its line count and header, or else a summary of its fields, declarations, or first heading) or per directory at a depth limit, so agents can choose which files to outline.
- Ship it as a single static binary with no runtime to install.
- Make it fast enough that agents never hesitate to run it, even over large trees.

## Current status

Each language below works in every mode (file mode, glob view, debug mode). What its outline holds is in its engine file's `//!` header, and its casefiles are in `tests/cases/<language>/`.

- Markdown: `src/languages/markdown.rs`.
- TypeScript and JavaScript: `src/languages/script.rs`.
- Rust: `src/languages/rust.rs`.
- TOML, YAML, and JSON data files (JSON with comments, `.jsonc`, included): `src/languages/data_file.rs`.
- CSS: `src/languages/css.rs`.

Beyond the engines:

- `just build-static` builds the single static binary (musl).
- Releases go to crates.io from a version tag ([ADR-010](adr.md)); `cargo install outln` builds from source, and no prebuilt binaries are released yet.
- Not yet: the Go, Java, Kotlin, and C# engines (each comes with header comments and its casefiles); `.go` and other such files are reported as unsupported.

## Non-goals

- Full AST or syntax-tree output.
- Formatting, linting, type-checking, or running source code.
- Cross-file or whole-project analysis.
