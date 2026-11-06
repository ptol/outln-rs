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

`just setup` runs `mise install` and `hk install --mise`; `--mise` makes the hook run through mise, so it finds the pinned tools even outside an activated shell.

## Commands

`just` lists every recipe with what it does; the `justfile` comment above each recipe is its documentation. Casefile recipes (`just case`, `just update-cases`) are explained in [testing.md](testing.md).

Release build: `cargo build --release` (profile in `Cargo.toml`) writes `target/release/outln`; run it from there. Keep it off `PATH` (no `cargo install`) until it reaches parity: the binary is also named `outln` and would shadow the TypeScript build agents navigate with.

## Pre-commit hook

Configured in `hk.pkl`. On `git commit`, hk stashes unstaged edits and untracked files, then:

1. Fixes staged files and re-stages them: rustfmt (`.rs`), dprint (Markdown, TOML, JSON, YAML), trailing whitespace, final newlines (not in `tests/cases`, whose casefiles are exact bytes).
2. Checks: merge-conflict markers, large files, clippy (when Rust or Cargo files are staged), every test (when Rust, Cargo, or casefiles are staged), and `tests/repo_conventions.rs` (on every commit).
3. Restores the stashed edits.

Run it without committing: `hk run pre-commit`. Preview what it would run: `hk run pre-commit --plan`.

## Troubleshooting

- **Wrong Rust version** (`rustc --version` differs from `rust-toolchain.toml`): mise isn't active in this shell. Run `mise install`, or prefix commands with `mise exec --`.
- **Hook can't find a tool**: rerun `hk install --mise`.
- **Stash restore failed after a hook run**: check `git status`, `git diff`, `git diff --cached`, and `git stash list` before changing anything; hk also keeps patch backups (see `hk --help`).
