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
