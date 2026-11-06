# Project commands (the equivalent of package.json scripts). `just` lists them.

set shell := ["bash", "-euo", "pipefail", "-c"]

# List the available commands.
default:
    @just --list --unsorted

# Once per clone: install the pinned tools (mise.toml) and the git pre-commit hook.
setup:
    mise install
    hk install --mise

# Build and run the CLI, passing every argument through unchanged (quoting and globs included).
[positional-arguments]
run *args:
    cargo run --quiet -- "$@"

# Format every file: Rust with rustfmt, Markdown/TOML/JSON/YAML with dprint.
fmt:
    cargo fmt
    dprint fmt

# Lint with clippy; any warning fails.
lint:
    cargo clippy --all-targets --locked -- -D warnings

# Run all tests, including doc examples and the repository conventions test.
test:
    cargo test --locked

# Run only the casefiles whose path contains `filter` (all of them without one).
case filter="":
    CASE='{{ filter }}' cargo test --locked --quiet --test casefiles

# Rewrite the expected output of failing casefiles (optionally only those matching `filter`); review with `git diff`.
update-cases filter="":
    CASE='{{ filter }}' UPDATE_CASES=1 cargo test --locked --quiet --test casefiles

# Apply every automatic fix: clippy suggestions, then formatting and hygiene (they can touch files you didn't change).
fix:
    cargo clippy --all-targets --locked --fix --allow-dirty --allow-staged --quiet
    hk fix --all

# Before reporting work as done: apply automatic fixes, run every check, then list what changed.
check: fix verify
    git status --short
    git diff --stat

# Read-only: every check without changing any file.
verify:
    hk check --all
    cargo test --locked
