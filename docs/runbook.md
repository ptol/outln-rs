---
description: 'How to set up, build, run, check, and debug outln, and how its tooling fits together.'
readWhen: 'building, running, installing, or debugging the CLI, or when a hook or check misbehaves'
---

# Runbook

## Setup

```sh
mise install   # the pinned tools, including just itself
just setup
```

`just setup` runs `mise install` (Rust 1.99.1, hk, dprint, just from `mise.toml`) and `hk install --mise` (the git pre-commit hook, run through mise so it finds the pinned tools even outside an activated shell).

## Commands

| Command                 | What it does                                                                       |
| ----------------------- | ---------------------------------------------------------------------------------- |
| `just run ...`          | Builds and runs the CLI with the given arguments.                                  |
| `just fmt`              | Formats every file (rustfmt, dprint).                                              |
| `just lint`             | clippy over all targets; any warning fails.                                        |
| `just test`             | All tests, including doc examples and the conventions test.                        |
| `just case x`           | Only the casefiles whose path contains `x` ([testing.md](testing.md)).             |
| `just update-cases [x]` | Rewrites the expected output of failing casefiles; review with `git diff`.         |
| `just fix`              | clippy `--fix`, then formatting and whitespace fixes over all files.               |
| `just check`            | `fix`, then `verify`, then lists changed files. Run before reporting work as done. |
| `just verify`           | Every check without changing files.                                                |

Release build: `cargo build --release` (fat LTO, one codegen unit, stripped) writes `target/release/outln`. Don't `cargo install` it yet: the binary is also named `outln` and would shadow the TypeScript build agents navigate with.

## Pre-commit hook

Configured in `hk.pkl`. On `git commit`, hk stashes unstaged edits and untracked files, then:

1. Fixes staged files and re-stages them: rustfmt (`.rs`), dprint (Markdown, TOML, JSON, YAML), trailing whitespace, final newlines (not in `tests/cases`, whose casefiles are exact bytes).
2. Checks: merge-conflict markers, large files, clippy (when Rust or Cargo files are staged), every test (when Rust, Cargo, or casefiles are staged), and `tests/repo_conventions.rs` (on every commit).
3. Restores the stashed edits.

Run it without committing: `hk run pre-commit`. Preview what it would run: `hk run pre-commit --plan`.

## Troubleshooting

- **Wrong Rust version** (`rustc --version` isn't 1.99.1): mise isn't active in this shell. Run `mise install`, or prefix commands with `mise exec --`.
- **Hook can't find a tool**: rerun `hk install --mise`.
- **Stash restore failed after a hook run**: check `git status`, `git diff`, `git diff --cached`, and `git stash list` before changing anything; hk also keeps patch backups (see `hk --help`).
