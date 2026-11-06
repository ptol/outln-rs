---
description: 'How to set up, build, run, check, release, and debug outln, and how its tooling fits together.'
readWhen: 'building, running, installing, releasing, or debugging the CLI, or when a hook or check misbehaves'
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

Building needs a C compiler (`cc` on `PATH`), because the tree-sitter grammars are C code.

Release build: `cargo build --release` (profile in `Cargo.toml`) writes `target/release/outln`; run it from there. The static binary: `just build-static` writes `target/x86_64-unknown-linux-musl/release/outln`, compiling the grammars with the zig that `mise install` pins, and fails unless the binary is static and outlines a sample `.ts`, `.tsx`, `.js`, and `.rs` file ([ADR-009](adr.md), [ADR-011](adr.md)). On a machine where agents work in this repo, keep it off `PATH` (no `cargo install`) until it reaches parity: the binary is also named `outln` and would shadow the TypeScript build agents navigate with.

## Release

Publishing to crates.io runs in CI, `.github/workflows/publish.yml` ([ADR-010](adr.md)). One-time setup: create a crates.io API token with the `publish-new` and `publish-update` scopes, then `gh secret set CARGO_REGISTRY_TOKEN`. To try the runner setup without uploading, run `gh workflow run publish.yml`: it skips the tag checks and runs the rest with `cargo publish --dry-run`.

1. Bump `version` in `Cargo.toml`, run `cargo check` so `Cargo.lock` follows, add a changelog entry naming the version ([changelog.md](changelog.md)), and commit.
2. Preview the crate: `cargo package --list` (the files `include` in `Cargo.toml` selects) and `cargo publish --dry-run --locked`. Both need the committed, clean tree.
3. `git tag v<version>`, then `git push origin main v<version>`. The workflow refuses a tag that differs from the version or isn't on `main`.
4. Check: the Publish run at `gh run list --workflow publish.yml` is green and `cargo search outln` shows the new version.

When a run fails, first check whether crates.io has the version (`cargo search outln`): an upload can succeed before the job fails.

- Not published: fix the cause, commit, push `main` (`git push origin main`; the workflow refuses a tag that isn't on it), then move the tag (`git tag -f v<version>`, `git push -f origin v<version>`). For a runner hiccup, rerun the job instead (`gh run rerun <run-id>`).
- Published: leave the tag; release the next fix as a new version. A published version can't be replaced, only yanked (`cargo yank --version <version>`, with a local `cargo login` token that has the `yank` scope).

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
