---
description: 'Register of known technical debt in the outln Rust rewrite, with impact and the way out.'
readWhen: 'taking a shortcut, or before changing an area to check its known weak spots'
---

# Technical debt

Each entry: what, impact, and the way out. Remove an entry when it's paid off.

## Installed outln can't show `//!` headers

- **What**: the TypeScript `outln` on `PATH` reads only plain `//` header comments, so `outln <dir>` shows no header for this repo's `.rs` files.
- **Impact**: agents read the first lines of Rust files by hand instead of navigating by header.
- **Way out**: the Rust outln reads `//!` headers; install it in place of the TypeScript build once it reaches parity.

## Glob view messages still say "header comment" for frontmatter

- **What**: the glob view prints `Includes only header comments per file.` and `(no header comment available)`, kept from the TypeScript build, though a Markdown file's header is its frontmatter ([GLOSSARY.md](GLOSSARY.md)).
- **Impact**: the output's wording disagrees with the glossary for Markdown files.
- **Way out**: reword both to "header" (`src/glob_view.rs`, `src/outline.rs`) with a changelog entry, once the code-language engines show which wording fits all files.
