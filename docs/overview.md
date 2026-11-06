---
description: 'Goals, current status, and scope of the outln Rust rewrite.'
readWhen: 'starting any task, to learn what the project is for and how far it is'
---

# Overview

## Goal

- A small CLI that prints an outline of source files: the header (header comment, or frontmatter for Markdown) and one line per top-level declaration, with line ranges, so coding agents can read only what they need.
- Ship it as a single static binary with no runtime to install (the TypeScript build needs Node).
- Make it fast enough that agents never hesitate to run it, even over large trees.

## Reference implementation

The TypeScript implementation in `../outln` (commit `ee514b1`, released as 0.2.1) is the behavioural reference: its `docs/overview.md` lists every feature, and `__tests__/cases/` holds its `.case.yaml` casefiles. How far to follow it is in the Priorities section of `AGENTS.md`.

## Current status

- Markdown files work in every mode: file mode (frontmatter and headings with section line ranges), glob view (gitignore-aware directory walking and glob patterns), and debug mode.
- `tests/cases` holds the TypeScript build's Markdown, CLI, glob view, walker, debug, and encoding cases (source-code inputs rewritten as Markdown), plus new ones for behaviour the TypeScript build lacked.
- No code-language engine yet (TypeScript/JavaScript, Go, Rust, Java, Kotlin, C#; each comes with header comments and its ported casefiles): `.rs`, `.ts`, and other source files are reported as unsupported.

## Non-goals

- Full AST or syntax-tree output.
- Formatting, linting, type-checking, or running source code.
- Cross-file or whole-project analysis.
