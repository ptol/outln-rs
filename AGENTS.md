---
description: 'Agent workflow for outln: setup, code navigation, conventions, lint, verification, and priorities.'
---

# outln

`outln` prints compact structural outlines of source files (the path and line count, a header, then top-level declarations with line ranges) so coding agents can choose what to read. It is written in Rust; [docs/overview.md](docs/overview.md) has the goals and the languages it supports so far.

The maintainer is learning Rust. In your final summary, briefly explain each non-obvious Rust concept your change relies on (an ownership or borrowing choice, a lifetime, a trait, a macro, why a crate was added), a sentence or two each. Prefer simple, readable idioms over clever ones.

## Setup

On a fresh clone, run `mise install` (it installs the tools pinned in `mise.toml`), then `just setup` (it installs the git pre-commit hook). `just` lists every project command with what it does; [docs/runbook.md](docs/runbook.md) covers setup, the hook, and troubleshooting.

## Navigate code

The `outln` on `PATH` is this crate's latest crates.io release ([ADR-014](docs/adr.md)), which can lag the code here; run the working copy with `just run <args>`, also when `outln` isn't on `PATH` (mise doesn't install it).

Default sequence, cheapest first. Each step is its own tool call, because its output picks the target of the next one:

1. **Find the docs for the task**: run `outln docs` and read every doc whose `readWhen` matches your task. [docs/codestyle.md](docs/codestyle.md) applies to every source change.
2. **Learn the structure**: read [docs/architecture.md](docs/architecture.md) for modules and data flow.
3. **Learn the API**: read `tests/usage.rs`, which is executable documentation of the public library API, and the doc examples in `src/lib.rs`.
4. **Find the file**: `outln <dir>` prints one line per file: its line count, and a summary taken from its header (header comment or frontmatter), or else a data file's field summary (its `description`/`title`, then its first scalar keys), a code file's declaration summary (its first top-level declarations, `pub struct Arg, impl Arg ×6, ...`), or a Markdown file's first heading. On a large tree, `outln --depth 1 <dir>` prints the files directly inside it and one line per subdirectory with its file and line counts. Quote a glob (`outln "src/**/*.rs"`): the shell expands an unquoted one into files, and outln prints their full outlines instead.
5. **Find the section**: `outln <file>...` prints each file's line count and its declarations with line ranges like `[L1-L10]`; read only those ranges with `Read` `offset`/`limit`, or a short file whole.

`outln` prints its own error for a missing path, so read its full output and stderr. When the output is too long, narrow the path or glob.

Run `outln` in its own command: the outline exists to pick what you read next, and a command that also reads files (`cat`, `sed`, `head`, `tail`, a pager) reads before seeing it. Don't pipe its output into `head`, `tail`, `sed` or a pager either; filter it with `grep` or narrow the path.

### Keep outln output useful

- Every `.rs` file starts with a `//!` header comment on its first line saying what the file is for; update it when the file's purpose changes.
- Keep top-level definitions focused, with descriptive names; avoid large unstructured blocks.
- Every Markdown file needs YAML frontmatter with a `description`, and docs in `docs/` a `readWhen`; [docs/docs-rules.md](docs/docs-rules.md) has the rules for writing docs.
- `tests/repo_conventions.rs` fails on a missing header comment or frontmatter field, an inline relative link to a missing file, an inner `#![allow(...)]`, or tool versions that disagree between files; the pre-commit hook runs it on every commit.

## Domain terms

Name domain concepts in code, docs, and CLI output only with terms from [docs/GLOSSARY.md](docs/GLOSSARY.md) (find them with `outln docs/GLOSSARY.md` and read only their entries). When a concept has no term, don't invent a name: propose one with the `domain-modeling` skill (or, without it, in the format of the existing entries, and tell the maintainer) and add it to the glossary before using it. Words under _Avoid_ are known wrong names; leaving a word off that list doesn't make it acceptable.

## Dependencies

Prefer the standard library. Add a well-established crate when std falls clearly short (tree-sitter grammars, `ignore` for gitignore-aware walking, `clap`, `thiserror`): turn off default features you don't need, and record why in [docs/adr.md](docs/adr.md). Stop and ask the maintainer before adding a crate that is unmaintained, large (many transitive dependencies), or brings `unsafe`-heavy native code beyond tree-sitter grammars. `cargo deny` (`deny.toml`, run by the hook and `just deny`) fails on advisories, unmaintained or yanked crates, licenses off its allow list, and duplicate versions; don't add exceptions there without the maintainer. `Cargo.lock` is committed; change it only through `cargo`.

## Editing

Edit source and docs with a tool that fails when the old text isn't there (`Edit`, `apply_patch`), not a shell `sed` or Python `str.replace`, which silently change nothing.

## Formatting

Don't reformat code you didn't change; the pre-commit hook formats staged files (rustfmt for `.rs`, dprint for Markdown, TOML, JSON, and YAML) and re-stages them. `just check` formats the whole repo, but since every commit is formatted, that only changes files you touched.

## Lint

Fix the code instead of silencing a lint. If a lint is wrong for a whole pattern, change `[lints]` in `Cargo.toml` (or `clippy.toml`) with a comment explaining why. A single exception is `#[expect(clippy::lint_name, reason = "why")]` on the narrowest item; `#[allow]` and attributes without a `reason` are themselves lint errors, and the conventions test rejects inner `#![allow(...)]`, which clippy can't see. `unsafe` code is forbidden and can't be re-enabled inline.

Side effects belong to `src/main.rs`, the imperative shell, and library code receives them as parameters. Lints reject printing (`print_stdout`, `print_stderr`) and opening the process streams, reading the environment or clock, exiting the process, or calling clap's methods that do these (`disallowed-methods` in `clippy.toml`); in `src/main.rs`, the code that does these things carries `#[expect(clippy::disallowed_methods, reason = "...")]`, and so do the test functions that read environment switches (the casefile runner's, and the fuzzer's `number_from_environment` for `just fuzz`) and the test helpers that write scratch files or run the binary. The exception is the filesystem: library code reads files directly, resolving every path against `Context::cwd`, never the process's working directory ([ADR-004](docs/adr.md)).

## Verify changes

- Run `just check` before reporting work as done, and fix every failure until it passes. It applies automatic fixes first (clippy suggestions, formatting, whitespace), then runs every check and test, and ends by listing the files that changed; review those changes as your own.
- `just verify` runs the same checks without changing any file.
- CI (`.github/workflows/check.yml`) runs `just ci` on every branch push and pull request: `just verify`, the static build, and the packaged crate's tests; a version tag runs it through the Publish workflow, before uploading a release ([ADR-026](docs/adr.md)). Locally, the pre-commit hook is the gate on commits. It checks formatting and conventions on every commit, runs clippy, rustdoc, and every test when Rust or Cargo files are staged, runs every test when casefiles or engine samples are, and runs `cargo deny` when the dependencies or `deny.toml` change. Never bypass the hook (`git commit --no-verify`, `-n`, `HK=0`); when it fails, fix the cause.

## Priorities

Coding agents write most of this codebase and are its main readers. Optimize for the next agent's session: prefer designs an agent can understand from the fewest files, and back conventions with types, lints, and tests that catch mistakes mechanically. When an agent stumbles (reads the wrong file, misses a convention, repeats a mistake), fix the cause in code, tooling, or docs so the next agent gets it right.

The project is in active development: ignore backward compatibility, including CLI output; note output changes in [docs/changelog.md](docs/changelog.md). The goals are a single static binary and speed: measure before optimizing, and avoid needless allocation and copying.

outln will hold about ten language engines (TypeScript/JavaScript, Go, Rust, Java, Kotlin, C#, Markdown, CSS, and TOML/YAML/JSON data files) and more later, so anything written or checked once per engine is multiplied: prefer shared machinery, traits, and table-driven tests over per-engine copies. Fix problems at their root. When a change fights the current design, refactor the design so the change fits naturally, even if that touches more files than a local patch would. Leave the codebase easier to extend than you found it.
