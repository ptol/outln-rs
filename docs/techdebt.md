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

## Data file keys find their leading comments with a walk of their own

- **What**: `data_file::range_start` and `content_end` walk line by line over comment ranges collected from the whole tree, because the data grammars put a key's comments inside the key before (a TOML table runs to the next one), so they repeat rules of `leading_annotations_start`: a blank line detaches a comment, a comment after code on its line belongs to that code, and the header's comment is never a key's. Some rules differ: a data file stops at a comment indented deeper than the key and at preamble, counts only lines that are wholly one comment (two JSON `/* */` comments on one line above a key don't count; the code walk would attach both), and trims trailing comment lines off a key's range.
- **Impact**: a change to the shared rules must be made in both walks, and a new format with comments in the same place adds to the line-based copy.
- **Way out**: make `leading_annotations_start` walk a sorted slice of candidates (a byte range and a `PrecedingNode`) instead of sibling nodes. Code engines build it from a declaration's siblings; the data file engine from the comment lines between the previous key and this one, with deeper-indented and preamble comments as `PrecedingNode::Other`. Keep `content_end` as the data file engine's own trim, and check that `just case toml`, `just case yaml`, and `just case json` pass unchanged.

## The static build isn't checked automatically

- **What**: `just build-static` ([ADR-009](adr.md)) runs neither in `just check` nor in the pre-commit hook.
- **Impact**: a dependency or toolchain change can break the static build without any check failing.
- **Way out**: run it after changing dependencies or the toolchain; gate it in CI (the Publish workflow is the only one so far, [ADR-010](adr.md)).
