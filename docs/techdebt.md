---
description: 'Register of known technical debt in the outln Rust rewrite, with impact and the way out.'
readWhen: 'taking a shortcut, or before changing an area to check its known weak spots'
---

# Technical debt

Each entry: what, impact, and the way out. Remove an entry when it's paid off.

## The TypeScript grammar is frozen at 0.23.2

- **What**: `tree-sitter-typescript` has had no crates.io release since November 2024 ([ADR-008](adr.md)), so newer syntax such as `using` declarations parses as errors in `.ts`, `.mts`, `.cts`, and `.tsx` files.
- **Impact**: declarations written with such syntax are missing from TypeScript outlines; the same code in a JavaScript file is listed.
- **Way out**: move to a newer release when one ships, or pin a git commit of the grammar, and rerun `just case typescript`.

## Declarations inside syntax errors are dropped

- **What**: tree-sitter wraps code it can't parse in `ERROR` nodes, and the Script and Rust engines list only the top-level nodes they recognize, so a declaration caught inside an `ERROR` node isn't listed.
- **Impact**: outlines of half-edited files can miss declarations around the broken spot.
- **Way out**: a shared dispatch that applies the engine's rules to an `ERROR` node's children, with a broken-file casefile per engine.

## The static build isn't checked automatically

- **What**: `just build-static` ([ADR-009](adr.md)) runs neither in `just check` nor in the pre-commit hook.
- **Impact**: a dependency or toolchain change can break the static build without any check failing.
- **Way out**: run it after changing dependencies or the toolchain; gate it in CI (the Publish workflow is the only one so far, [ADR-010](adr.md)).
