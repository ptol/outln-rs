---
description: 'Agent workflow for the outln Rust rewrite: setup, code navigation, conventions, lint, verification, and priorities.'
---

# outln

`outln` prints compact structural outlines of source files (a header, then top-level declarations with line ranges) so coding agents can choose what to read. This repo is a Rust rewrite of the TypeScript implementation in `../outln`; so far it supports Markdown files only. [docs/overview.md](docs/overview.md) has the goals and current status.

The maintainer is learning Rust. In your final summary, briefly explain each non-obvious Rust concept your change relies on (an ownership or borrowing choice, a lifetime, a trait, a macro, why a crate was added), a sentence or two each. Prefer simple, readable idioms over clever ones.

## Setup

On a fresh clone, run `mise install` (it installs the tools pinned in `mise.toml`: Rust toolchain, hk, dprint, just), then `just setup` (it installs the git pre-commit hook). `just` lists every project command with what it does; [docs/runbook.md](docs/runbook.md) covers setup, the hook, and troubleshooting.

## Navigate code

Default sequence, cheapest first. Each step is its own tool call, because its output picks the target of the next one:

1. **Find the docs for the task**: run `outln docs` and read every doc whose `readWhen` matches your task. [docs/codestyle.md](docs/codestyle.md) applies to every source change.
2. **Learn the structure**: read [docs/architecture.md](docs/architecture.md) for modules and data flow.
3. **Learn the API**: read `tests/usage.rs`, which is executable documentation of the public library API, and the doc examples in `src/lib.rs`.
4. **Find the file**: `outln <dir>` prints one line per file, taken from its header comment.
5. **Find the section**: `outln <file>...` prints declarations with line ranges like `[L1-L10]`; read only those ranges with `Read` `offset`/`limit`.

The `outln` on `PATH` is the installed TypeScript build, not this repo's binary (run that with `just run <args>`). It does not read `//!` header comments yet, in either mode: step 4 shows `(no header comment available)` for `.rs` files, so read the first lines of a Rust file instead (`Read` with `limit: 10`). Step 5 works for `.rs` declarations. It rejects `.toml` files.

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

Prefer the standard library. Add a well-established crate when std falls clearly short (tree-sitter grammars, `ignore` for gitignore-aware walking, `clap`, `thiserror`): turn off default features you don't need, and record why in [docs/adr.md](docs/adr.md). Stop and ask the maintainer before adding a crate that is unmaintained, large (many transitive dependencies), or brings `unsafe`-heavy native code beyond tree-sitter grammars. `Cargo.lock` is committed; change it only through `cargo`.

## Editing

Edit source and docs with a tool that fails when the old text isn't there (`Edit`, `apply_patch`), not a shell `sed` or Python `str.replace`, which silently change nothing.

## Formatting

Don't reformat code you didn't change; the pre-commit hook formats staged files (rustfmt for `.rs`, dprint for Markdown, TOML, JSON, and YAML) and re-stages them. `just check` formats the whole repo, but since every commit is formatted, that only changes files you touched.

## Lint

Fix the code instead of silencing a lint. If a lint is wrong for a whole pattern, change `[lints]` in `Cargo.toml` (or `clippy.toml`) with a comment explaining why. A single exception is `#[expect(clippy::lint_name, reason = "why")]` on the narrowest item; `#[allow]` and attributes without a `reason` are themselves lint errors, and the conventions test rejects inner `#![allow(...)]`, which clippy can't see. `unsafe` code is forbidden and can't be re-enabled inline.

Side effects belong to `src/main.rs`, the imperative shell, and library code receives them as parameters. Lints reject printing (`print_stdout`, `print_stderr`) and opening the process streams, reading the environment or clock, exiting the process, or calling clap's methods that do these (`disallowed-methods` in `clippy.toml`); in `src/main.rs`, the code that does these things carries `#[expect(clippy::disallowed_methods, reason = "...")]`, and so does the casefile runner's one function that reads its environment switches. The exception is the filesystem: library code reads files directly, resolving every path against `Context::cwd`, never the process's working directory ([ADR-004](docs/adr.md)).

## Verify changes

- Run `just check` before reporting work as done, and fix every failure until it passes. It applies automatic fixes first (clippy suggestions, formatting, whitespace), then runs every check and test, and ends by listing the files that changed; review those changes as your own.
- `just verify` runs the same checks without changing any file.
- There is no CI: the pre-commit hook is the only automatic gate. It checks formatting and conventions on every commit, runs clippy and every test when Rust or Cargo files are staged, and runs every test when casefiles are. Never bypass the hook (`git commit --no-verify`, `-n`, `HK=0`); when it fails, fix the cause.

## Priorities

Coding agents write most of this codebase and are its main readers. Optimize for the next agent's session: prefer designs an agent can understand from the fewest files, and back conventions with types, lints, and tests that catch mistakes mechanically. When an agent stumbles (reads the wrong file, misses a convention, repeats a mistake), fix the cause in code, tooling, or docs so the next agent gets it right.

The project is in active development: ignore backward compatibility, including CLI output. The TypeScript implementation is a reference for behaviour, not a spec; note deliberate output changes in [docs/changelog.md](docs/changelog.md). The goals are a single static binary and speed: measure before optimizing, and avoid needless allocation and copying.

outln will hold about 8 language engines (TypeScript/JavaScript, Go, Rust, Java, Kotlin, C#, Markdown) and more later, so anything written or checked once per engine is multiplied: prefer shared machinery, traits, and table-driven tests over per-engine copies. Fix problems at their root. When a change fights the current design, refactor the design so the change fits naturally, even if that touches more files than a local patch would. Leave the codebase easier to extend than you found it.
