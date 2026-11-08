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

- **What**: tree-sitter wraps code it can't parse in `ERROR` nodes, and the Script, Rust, CSS, and data file engines list only the top-level nodes they recognize, so a declaration caught inside an `ERROR` node isn't listed. The data file engine recovers some keys from `ERROR` nodes: a JSON root object that never closes keeps its keys up to its first broken one ([ADR-016](adr.md)), a JSON object missing a comma keeps the keys on both sides of the gap (`data_file::json::keys`), and a YAML root mapping broken by a syntax error keeps its keys before the error (`data_file::yaml::root_pairs`, [ADR-023](adr.md)), dropping those after it.
- **Impact**: outlines of half-edited files can miss declarations around the broken spot.
- **Way out**: for the code-language engines, have `OutlineLevel::list` (`src/code_outline.rs`) apply the language's `declaration_lines` to an `ERROR` node's children (with that node's children as the siblings for leading annotations); the data file engine needs rules per format, since a broken JSON object flattens nested keys into its `ERROR` node. The casefiles of broken files show what each engine drops today and change when this is paid off: `tests/cases/engines/half-edited-*.case` (Markdown, TSX, JavaScript, Rust, CSS, JSON, TOML), `typescript/syntax-errors.case`, `json/root-array-and-broken.case`, `toml/half-edited-table-header.case`, `yaml/broken-mapping-keeps-its-first-keys.case`, and `markdown/broken-frontmatter-keeps-its-first-fields.case`.

## Data file keys find their leading comments with a walk of their own

- **What**: `data_file::range_start` and `content_end` walk line by line over comment ranges collected from the whole tree, because the data grammars put a key's comments inside the key before (a TOML table runs to the next one), so they repeat rules of `leading_annotations_start`: a blank line detaches a comment, a comment after code on its line belongs to that code, and the header's comment is never a key's. Some rules differ: a data file stops at a comment indented deeper than the key and at preamble, counts only lines that are wholly one comment (two JSON `/* */` comments on one line above a key don't count; the code walk would attach both), and trims trailing comment lines off a key's range.
- **Impact**: a change to the shared rules must be made in both walks, and a new format with comments in the same place adds to the line-based copy.
- **Way out**: make `leading_annotations_start` walk a sorted slice of candidates (a byte range and a `PrecedingNode`) instead of sibling nodes. Code engines build it from a declaration's siblings; the data file engine from the comment lines between the previous key and this one, with deeper-indented and preamble comments as `PrecedingNode::Other`. Keep `content_end` as the data file engine's own trim, and check that `just case toml`, `just case yaml`, and `just case json` pass unchanged.

## Parsing deeply nested code can take seconds

- **What**: tree-sitter's parser takes time quadratic in the nesting depth for some deeply nested code, such as a TypeScript parameter default nested 20 000 `[` deep or a Rust type nested 20 000 `Vec<` deep with a comment or extra whitespace inside (3 to 9 s; 200 s at 100 000 levels). The time is spent in tree-sitter's reductions (`ts_parser__reduce`, `ts_parser__do_all_potential_reductions`), not in outln's walks, which are iterative and linear.
- **Impact**: file mode and debug mode stall on such a file (generated or adversarial input); the glob view parses at most the first 4 KiB of a code file without a header ([ADR-024](adr.md)), too little to nest that deep, so it is unaffected.
- **Way out**: parse with `Parser::parse_with_options` and a progress callback that cancels after a time or byte budget, and report a cancelled parse as a new `InputError` (or outline the file without declarations).

## Deep YAML nesting is guarded by an estimate

- **What**: tree-sitter-yaml 0.7.2's scanner overflows tree-sitter's 1024-byte state buffer from 254 open block nodes, and a C assertion aborts the process. `data_file::yaml::parsable_part` counts open block nodes line by line, over-approximating, and parses only the lines before the count passes 200. A line inside a multi-line flow collection (`[`, `{`) indented less than its parent closes nodes in the count that the scanner keeps open, so a file built for it can still abort outln.
- **Impact**: such a file ends the whole run, glob views included; a real file nested more than about 100 levels deep loses its keys after the deep lines.
- **Way out**: a tree-sitter-yaml release whose `serialize` checks the buffer size (report it upstream); then remove `parsable_part` and check that `just fuzz` still passes.
