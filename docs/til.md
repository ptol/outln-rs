---
description: 'Reusable learnings and gotchas from developing outln in Rust.'
readWhen: 'stuck on a toolchain, hook, or library surprise, or right after solving one'
---

# Today I Learned

Add an entry when something cost you time and would cost the next agent time too. Lead with the symptom.

## mise overrides rust-toolchain.toml

`rustc --version` showed the global Rust instead of the pinned one: mise exports `RUSTUP_TOOLCHAIN` for its active `rust`, and that environment variable beats `rust-toolchain.toml`. `mise.toml` therefore pins `rust` to the same version.

## Plain rustfmt formats as edition 2015

`rustfmt` run directly (by hand, by an editor, or through stdin) ignores `Cargo.toml`, so without `rustfmt.toml` it parses code as edition 2015 and rejects newer syntax. `rustfmt.toml` sets the edition; `cargo fmt` reads it from `Cargo.toml`, and hk's rustfmt steps also pass `--edition 2024`.

## rustfmt on a path also formats its child modules

`rustfmt src/lib.rs` also rewrites every out-of-line module it declares (`mod child;` → `src/child.rs`), so a hook fixing only staged files could leave unstaged changes behind. The `rustfmt` fix step in `hk.pkl` formats each file through stdin, which never follows `mod` declarations.

## zig cc rejects Rust's target triple

Setting `CC_x86_64_unknown_linux_musl="zig cc -target x86_64-linux-musl"` failed with `unable to parse target query 'x86_64-unknown-linux-musl'`: cc-rs adds `--target=<rust triple>` to every compile, and zig spells triples without the vendor. `scripts/zig-cc-musl` drops that flag and passes zig's own `-target`.

## A failed `cargo release --execute` leaves Cargo.toml bumped

When a `pre-release-replacements` search (`exactly = 1`) found nothing, `cargo release <level> --execute` stopped with the version already written to `Cargo.toml` and `Cargo.lock`, so the tree was dirty for the next attempt. The dry run (no `--execute`) runs the same checks without writing, so `just release` runs it first. After such a failure, `git checkout Cargo.toml Cargo.lock`.
