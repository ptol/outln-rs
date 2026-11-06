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

# Build the static release binary (musl libc, C grammars compiled by zig), then fail unless it is static and outlines a .ts, .tsx, .js, and .rs file.
build-static:
    #!/usr/bin/env bash
    set -euo pipefail
    target=x86_64-unknown-linux-musl
    binary="target/$target/release/outln"
    rustup --quiet target add "$target"
    CC_x86_64_unknown_linux_musl="$PWD/scripts/zig-cc-musl" cargo build --release --locked --target "$target"
    # Read the headers first, so a missing or failing readelf stops the recipe instead of
    # passing as "no dynamic entries".
    elf_headers=$(readelf --program-headers --dynamic "$binary")
    if grep -q -e INTERP -e NEEDED <<< "$elf_headers"; then
        echo "$binary is dynamically linked" >&2
        exit 1
    fi
    sample=$(mktemp -d)
    trap 'rm -rf "$sample"' EXIT
    for extension in ts tsx js rs; do
        declaration='export function run(task)'
        if [ "$extension" = rs ]; then declaration='pub fn run(task: u8)'; fi
        printf '// Sample.\n%s {}\n' "$declaration" > "$sample/sample.$extension"
        expected=$(printf '%s\nSample.\n[L2-L2] %s' "$sample/sample.$extension" "$declaration")
        actual=$("$binary" "$sample/sample.$extension")
        if [ "$actual" != "$expected" ]; then
            printf '%s outlined sample.%s wrongly:\n%s\n' "$binary" "$extension" "$actual" >&2
            exit 1
        fi
    done
    echo "$binary: static, $(stat --format=%s "$binary") bytes"

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

# Build the API docs, private items included, into target/doc; any rustdoc warning (a broken intra-doc link, say) fails.
doc:
    RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --document-private-items --locked

# Check dependencies against deny.toml: advisories, licenses, duplicate versions, and sources (needs the network).
deny:
    cargo deny --locked check

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

# Read-only: every check and test (the steps in hk.pkl) without changing any file.
verify:
    hk check --all

# Release `level` (major, minor, patch, or an exact version like 0.3.0): package the crate as crates.io would, dry-run cargo-release (release.toml) so a failing check changes no file, then bump, commit, tag, and push after you confirm; CI publishes.
release level:
    cargo publish --dry-run --locked
    cargo release '{{ level }}'
    cargo release '{{ level }}' --execute
