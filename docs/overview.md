---
description: 'Goals, current status, and scope of outln.'
readWhen: 'starting any task, to learn what the project is for and how far it is'
---

# Overview

## Goal

- A small CLI that prints an outline of source files: the header (header comment, or frontmatter for Markdown) and one line per top-level declaration, with line ranges, so coding agents can read only what they need.
- Ship it as a single static binary with no runtime to install.
- Make it fast enough that agents never hesitate to run it, even over large trees.

## Current status

- Markdown files work in every mode: file mode (frontmatter and headings with section line ranges), glob view (gitignore-aware directory walking and glob patterns; the first heading when there is no frontmatter), and debug mode.
- TypeScript and JavaScript files (`.ts`, `.mts`, `.cts`, `.d.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`) work in every mode: header comments, top-level declarations with class members, function-valued variables, default-exported expressions, and test blocks. Their casefiles are in `tests/cases/typescript/`.
- Rust files (`.rs`) work in every mode: `//!` and `/*!` header comments (or else plain `//` and `/* */` ones), top-level declarations with their signatures as written, `pub use` re-exports, and line ranges that include doc comments and attributes. Their casefiles are in `tests/cases/rust/`.
- Data files (`.toml`, `.yaml`, `.yml`, `.json`) work in every mode: `#` header comments (`//` and `/* */` in JSON), top-level keys with scalar values (TOML table headers as written), line ranges that include the comment above a key, and in glob view a field summary of the first scalar keys when there is no header comment ([ADR-016](adr.md)). Their casefiles are in `tests/cases/{toml,yaml,json}/`.
- `just build-static` builds the single static binary (musl).
- Releases go to crates.io from a version tag ([ADR-010](adr.md)); `cargo install outln` builds from source, and no prebuilt binaries are released yet.
- `tests/cases` holds casefiles for every engine and for the CLI, glob view, directory walking, debug mode, and encoding.
- Not yet: the Go, Java, Kotlin, and C# engines (each comes with header comments and its casefiles); `.go` and other such files are reported as unsupported.

## Non-goals

- Full AST or syntax-tree output.
- Formatting, linting, type-checking, or running source code.
- Cross-file or whole-project analysis.
