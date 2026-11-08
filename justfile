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

# Build the static release binary (musl libc, C grammars compiled by zig), then fail unless it is static and outlines every engine sample (tests/engine-samples) exactly as the dev build does.
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
    # One engine sample per grammar (a unit test keeps the set complete), so every grammar's C
    # code, scanners included, runs in the static binary; the dev build is the tested reference.
    samples=(tests/engine-samples/*)
    expected=$(cargo run --quiet --locked -- "${samples[@]}")
    if ! actual=$("$binary" "${samples[@]}"); then
        echo "$binary failed on the engine samples" >&2
        exit 1
    fi
    if [ "$actual" != "$expected" ]; then
        echo "$binary outlines the engine samples differently from the dev build:" >&2
        diff <(printf '%s\n' "$expected") <(printf '%s\n' "$actual") >&2 || true
        exit 1
    fi
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

# Check dependencies against deny.toml: advisories, licenses, duplicate versions, and sources. Offline, it checks against the advisory database fetched last time and prints a note.
deny:
    #!/usr/bin/env bash
    set -euo pipefail
    # Fetch on its own first, so only a failed fetch falls back to offline; a failed check fails.
    if cargo deny --locked fetch db; then
        cargo deny --locked check
    else
        echo "note: could not fetch the RustSec advisory database (offline?); checking against the copy fetched last time, so new advisories may be missing" >&2
        cargo deny --locked --offline check
    fi

# Package the crate as crates.io would, then run its tests in the unpacked package, which holds only the files `include` in Cargo.toml selects.
test-package:
    #!/usr/bin/env bash
    set -euo pipefail
    # --allow-dirty: like every other check, test the working tree, uncommitted edits included.
    cargo package --locked --allow-dirty
    id=$(cargo pkgid)
    # The shared target dir reuses the dependencies already built for the repository.
    cargo test --locked --manifest-path "target/package/outln-${id##*[#@]}/Cargo.toml" --target-dir target

# Run only the casefiles whose path contains `filter` (all of them without one).
case filter="":
    CASE='{{ filter }}' cargo test --locked --quiet --test casefiles

# Write the actual output into the casefiles (optionally only those matching `filter`): differing or missing output sections, in canonical form; review with `git diff`.
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

# What CI runs (.github/workflows/check.yml): every check, the static build, and the packaged crate's tests.
ci: verify build-static test-package

# Release `level` (major, minor, patch, or an exact version like 0.3.0): package the crate as crates.io would, dry-run cargo-release (release.toml) so a failing check changes no file, then bump, commit, tag, and push after you confirm; CI publishes.
release level:
    cargo publish --dry-run --locked
    cargo release '{{ level }}'
    cargo release '{{ level }}' --execute
