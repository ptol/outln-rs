---
description: 'Register of known technical debt in outln, with impact and the way out.'
readWhen: 'taking a shortcut, or before changing an area to check its known weak spots'
---

# Technical debt

Each entry: what, impact, and the way out. Remove an entry when it's paid off.

## The TypeScript grammar is frozen at 0.23.2

- **What**: `tree-sitter-typescript` has had no crates.io release since November 2024 ([ADR-008](adr.md)), so newer syntax such as `using` declarations parses as errors in `.ts`, `.mts`, `.cts`, and `.tsx` files.
- **Impact**: declarations written with such syntax are missing from TypeScript outlines; the same code in a JavaScript file is listed.
- **Way out**: move to a newer release when one ships, or pin a git commit of the grammar, and rerun `just case typescript`.

## Declarations inside syntax errors are dropped

- **What**: tree-sitter wraps code it can't parse in `ERROR` nodes, and the Script, Rust, CSS, and data file engines list only the top-level nodes they recognize, so a declaration caught inside an `ERROR` node isn't listed (the data file engine reads a JSON root object that never closes up to its first broken key, [ADR-016](adr.md)).
- **Impact**: outlines of half-edited files can miss declarations around the broken spot.
- **Way out**: for the code-language engines, have `code_outline::code_outline` apply the engine's rules to an `ERROR` node's children (with that node's children as the siblings for leading annotations); the data file engine needs rules per format, since a broken JSON object flattens nested keys into its `ERROR` node. Add a broken-file casefile per engine.
