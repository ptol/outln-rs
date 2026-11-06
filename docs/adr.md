---
description: 'Architecture Decision Records for the outln Rust rewrite.'
readWhen: 'making or questioning a design decision, or adding a dependency'
---

# Architecture Decision Records

Each record states the decision, why, and what it costs. Add a new ADR rather than editing an accepted one; mark the old one superseded, or amended when the new one replaces only part of it.

## ADR-001: Rewrite outln in Rust

**Status**: Accepted (2026-11-06)

### Decision

Rewrite the TypeScript CLI (`../outln`) in Rust, in this repo, as a single Cargo package with a library and a thin binary.

### Rationale

- A single static binary removes the Node runtime from every machine that runs outln.
- Performance: native code and tree-sitter's Rust bindings with no JavaScript bridge.
- tree-sitter, the parsing library the TypeScript build already uses, is native and has first-class Rust crates.

### Consequences

- The TypeScript build stays the installed `outln` until the rewrite reaches feature parity.
- Behaviour may diverge from the TypeScript build on purpose (no backward compatibility); [changelog.md](changelog.md) records each divergence.

## ADR-002: Local-only tooling with strict lints and an auto-fixing pre-commit hook

**Status**: Accepted (2026-11-06); amended by ADR-010 (replaces "No CI for now": a CI workflow publishes releases, and commits stay gated only by the hook)

### Decision

- Rust 1.99.1 pinned in `rust-toolchain.toml` and `mise.toml`; edition 2024.
- clippy `all` and `pedantic`, plus restriction lints for `unwrap`/`expect`/`panic`, printing, wildcard matches, and lint exceptions without a reason; `disallowed-methods` for process streams, environment, and clock outside `src/main.rs`; rustc's `unreachable_pub`; `unsafe` forbidden; warnings fail.
- hk runs the pre-commit hook: it hides unstaged edits, formats staged files (rustfmt, dprint), re-stages them, then runs clippy and every test when Rust or Cargo files are staged, and the repository conventions test on every commit.
- just provides the project commands; mise pins the tools.
- No CI for now.

### Rationale

- Agents write most of the code with permission prompts off, so mechanical checks are the only reliable gate.
- hk is the hook manager that re-stages fixes with unstaged edits hidden, like lint-staged did in the TypeScript repo; prek and pre-commit abort the commit after a fix by design.
- dprint formats Markdown, TOML, JSON, and YAML without Node.

### Consequences

- A fresh clone needs `mise install` (the pinned tools, including just), then `just setup` (the hook).
- Bumping Rust means bumping two files and fixing any new lints.
- `mise.toml` must pin `rust`, because mise exports `RUSTUP_TOOLCHAIN`, which overrides `rust-toolchain.toml`.

## ADR-003: Language engines return data with byte ranges into normalized text

**Status**: Accepted (2026-11-06); amended by ADR-008 (one engine type may serve several grammars, and `Header` has a header comment variant)

### Decision

- `trait LanguageEngine { extensions, outline, file_summary }`, implemented by one unit struct per language and registered in an immutable `static ENGINES` list, matched by extension ignoring ASCII case.
- Engines return data (`Outline`: an optional `Header` plus `OutlineLine`s), never text; `src/outline.rs` and `src/debug_mode.rs` render it for every mode.
- Positions are half-open `ByteRange`s into normalized text: UTF-8 only (invalid UTF-8 or a NUL byte is an error), BOM stripped, CRLF converted to LF before any engine runs. `LineIndex` turns them into 1-based inclusive line ranges; a newline belongs to the line it ends.
- `file_summary` is separate from `outline`, so the glob view can stop parsing after the header (the file is still read whole).

### Rationale

- About eight engines will share the renderer, the line arithmetic, and debug highlighting; with data plus byte ranges each is written once.
- tree-sitter, which the code-language engines will use, reports byte ranges, so they need no conversion of their own.
- The glob view may summarize thousands of files; parsing each fully would waste most of the work.

### Consequences

- Engines never see `\r\n` or a BOM; debug mode prints the normalized text.
- Invalid UTF-8 is an error instead of being decoded with replacement characters (the TypeScript build did that).
- `Header` has only a frontmatter variant until the first code-language engine adds header comments.

## ADR-004: Casefiles in a plain-text format, run in process on a temp directory

**Status**: Accepted (2026-11-06)

### Decision

- Behaviour is tested with casefiles (`tests/cases/**/*.case`): one plain-text file per case with sections for arguments, input files, stdout, stderr, and exit code; the format is documented in `tests/support/casefile.rs` and parsed with std only.
- `tests/casefiles.rs` is a single `#[test]`: it writes each case's files into a fresh directory under the system temp dir, calls `outln::run` there, and reports every failing case at once. `CASE=<text>` filters; `UPDATE_CASES=1` rewrites only the expected-output sections.
- The library reads the real filesystem, with every path resolved against `Context::cwd`; there is no filesystem trait. This is an exception to "inject filesystem I/O" in [codestyle.md](codestyle.md).

### Rationale

- The TypeScript build's YAML casefiles worked, but its update mode re-serialized whole files and mangled block scalars, and ANSI escapes needed quoted strings; `serde_yaml` is also archived. A sectioned text format fixes both, and update mode leaves inputs byte-for-byte untouched.
- The `ignore` crate walks only the real filesystem, so an in-memory filesystem would have meant reimplementing gitignore-aware walking.
- No Rust crate reads one-file-per-case CLI tests with inline input files: `trycmd` uses folders of several files per case and runs the binary as a subprocess, `insta` keeps snapshots apart from inputs, and `libtest-mimic`/`datatest-stable` (19 and 30 dependencies) would only add per-case test names.

### Consequences

- Casefiles depend on the temp dir: the runner fails if a `.git` or `.ignore` sits above it, and the hook and `.gitattributes` keep casefiles byte-exact.
- `cargo test` shows one test for all casefiles; `just case <text>` runs a subset. Revisit with libtest-mimic if the single test gets slow.
- Behaviour a casefile can't express (symlinks, a global gitignore, read errors) is tested in `tests/usage.rs` or unit tests.

## ADR-005: Parse the CLI with clap inside the library

**Status**: Accepted (2026-11-06)

### Decision

`src/cli.rs` defines the arguments with clap's derive API. `run` calls `try_get_matches_from` and writes help, the version, and usage errors to its injected writers; `clippy.toml` forbids clap's methods that read the process arguments, print, or exit.

### Rationale

- clap is the standard Rust CLI parser and keeps help, errors, and new flags consistent as the CLI grows.
- Parsing inside `run` lets casefiles test usage errors.

### Consequences

- Help, version (`outln <version>`), and usage-error text are clap's, and usage errors exit 2; the TypeScript build's own texts and exit code 1 are gone ([changelog.md](changelog.md)).
- clap is built without `color`, `suggestions`, and `wrap_help` (which reads the terminal width), so its output doesn't depend on the terminal.

## ADR-006: Walk directories with the `ignore` crate and git semantics

**Status**: Accepted (2026-11-06)

### Decision

The glob view and debug mode walk directories with `ignore::WalkBuilder` defaults: `.gitignore` files apply inside a git repository (including those of parent directories up to its root), plus `.git/info/exclude` and `.ignore` files; dot entries are skipped, symlinks aren't followed, and `node_modules` is always skipped. The crate's own global-gitignore lookup is off: `src/main.rs` finds git's `core.excludesFile` with `ignore::gitignore::gitconfig_excludes_path` and passes it in as `Context::global_gitignore`. Glob patterns are matched with `globset` over a walk of their literal base directory.

### Rationale

- These are the rules of git, ripgrep, and fd, which users and agents already expect.
- The crate would read `HOME`, `XDG_CONFIG_HOME`, and `GIT_CONFIG_*` itself, out of sight of the lints; passing the path keeps environment access in `src/main.rs` and casefiles independent of the machine.

### Consequences

- Unlike the TypeScript build, a `.gitignore` outside a git repository is ignored, and ancestor rules still apply to an explicitly named directory they ignore ([changelog.md](changelog.md)).
- The global gitignore is added as an explicit ignore file, so it also applies outside git repositories.
- Results are sorted by byte order, not by locale.

## ADR-007: Parse Markdown with pulldown-cmark

**Status**: Accepted (2026-11-06)

### Decision

The Markdown engine finds headings with pulldown-cmark (default features off). Frontmatter is detected by hand: `---` on the first line up to the next `---` line, with top-level `key: value` fields. Headings inside block quotes and lists are skipped; a heading's section ends before the next heading of the same or a higher level.

### Rationale

- The TypeScript build's line scanner misread fences, indented code, HTML blocks, and setext headings; a CommonMark parser gets them right.
- pulldown-cmark is pure Rust with three dependencies, aims at full CommonMark compliance, and parsed a 124 KB file in 0.7 ms, 25 times faster than `tree-sitter-md`, whose README says it is "not recommended ... where correctness is important".
- pulldown-cmark's own metadata option also accepts a `---` block in the middle of a document, so frontmatter is found before parsing.

### Consequences

- The Markdown engine doesn't share tree-sitter machinery with the code-language engines; section ranges and rendering are shared anyway.
- Heading text is the raw source line without its `#` markers, so inline markup keeps its syntax.

## ADR-008: Parse TypeScript and JavaScript with tree-sitter, on a shared code-engine layer

**Status**: Accepted (2026-11-06)

### Decision

- One engine type, `Script` (`src/languages/script.rs`), is registered three times in `ENGINES`, once per grammar: `tree-sitter-typescript`'s `typescript` grammar for `.ts`/`.mts`/`.cts`, its `tsx` grammar for `.tsx`, and `tree-sitter-javascript` for `.js`/`.jsx`/`.mjs`/`.cjs`. Every declaration rule is shared; the grammars differ only in syntax one of them lacks.
- Shared by every future code engine: `src/syntax_tree.rs` (parsing, node byte ranges, a node's source on one line) and `src/header_comment.rs` (`//` and `/* */` header comments). An engine decides only where its header comment may start.
- `Header::Comment` holds header comment lines; `OutlineLine::depth` indents class members.
- The header is found by scanning lines, so `file_summary` never parses.
- Versions: `tree-sitter` 0.27.1 (default features off), `tree-sitter-typescript` 0.23.2 (the reference's version; no release since November 2024), `tree-sitter-javascript` 0.25.0 (the latest).

### Rationale

- tree-sitter is the reference's parser, so the ported casefiles pass with few deliberate changes, and it parses broken files by recovering instead of failing.
- One engine type keeps the TypeScript and JavaScript rules in one place; the grammar is a field because `LanguageEngine::outline` receives only the text, so each grammar needs its own registry entry.
- The two grammar crates disagree on a few shapes (the TypeScript grammar parses a member's decorators as siblings in the class body, the JavaScript grammar as part of the member). The engine handles both, and `tests/cases/typescript/extensions-and-grammars.case` outlines one source with every grammar.

### Consequences

- The binary is built from C (the grammars), so building needs a C compiler; release size grew from 2.31 MB to 6.26 MB (glibc, x86-64), and a 760 KB file with 20 000 functions outlines in 0.14 s.
- The TypeScript grammar predates newer syntax such as `using` declarations, which parse as errors in `.ts` files but work in JavaScript.
- Debug-mode highlights come from node byte ranges rather than from the printed text's length, so a multi-line signature is highlighted on every line up to its body.

## ADR-009: Build the static binary for musl with zig as the C compiler

**Status**: Accepted (2026-11-06)

### Decision

`just build-static` builds the release binary for `x86_64-unknown-linux-musl`, compiling the grammars' C code with `zig cc` (pinned in `mise.toml`, called through `scripts/zig-cc-musl`), then fails unless the binary has no ELF interpreter or shared libraries and outlines a `.ts`, `.tsx`, and `.js` file. It isn't part of `just check` or the pre-commit hook.

### Rationale

- A single static binary is a project goal (ADR-001), and the C grammars are the first native code to link.
- The system C compiler builds against glibc headers; zig ships musl headers and libc for every target, installed by mise like the other tools.
- A fat-LTO release build of the grammars is too slow to run on every check.

### Consequences

- The static build can break unnoticed between runs of the recipe; run it after changing dependencies or the toolchain.
- The static binary is 6.35 MB.

## ADR-010: Publish to crates.io from a tag-triggered GitHub Actions workflow

**Status**: Accepted (2026-11-06)

### Decision

- The crate is published to crates.io as `outln`, so users install it with `cargo install outln --locked`.
- Pushing a tag `v<version>` runs `.github/workflows/publish.yml`: it fails unless the tag equals `Cargo.toml`'s version and `main` contains it, runs clippy and the tests, then runs `cargo publish` with the `CARGO_REGISTRY_TOKEN` repository secret. Run by hand, it skips the tag checks and runs the rest with `cargo publish --dry-run`.
- The package `include`s only the `.rs` files under `src/`, the manifest, `Cargo.lock`, and `LICENSE`, and ships no README (`readme = false`).

### Rationale

- A published version can never be replaced, so the release re-runs clippy and the tests on a clean machine instead of trusting the local hook.
- Publishing from CI instead of a laptop keeps the token out of local shells, and the tag checks stop a release whose tag and manifest disagree or whose commit never reached `main`.
- An `include` allowlist keeps casefiles, docs, and dev tooling out of the crate; new repo files stay out without editing an `exclude` list.
- crates.io renders a README with comrak and no frontmatter extension, so `README.md`'s YAML frontmatter (required by `tests/repo_conventions.rs`) would show as a rule and a `description: …` heading at the top of the crate page.

### Consequences

- `cargo install` builds from source: users need Rust at least `rust-version` and a C compiler for the grammars. Prebuilt binaries (`just build-static`) aren't released yet.
- The crate page has no README; it shows the description and links to the repository.
- The binary is named `outln`, like the TypeScript build: whichever comes first on `PATH` wins. On a machine where agents work in this repo, keep the TypeScript build installed (runbook).
- The published crate can't run its integration tests (`cargo package` warns that it drops each one); unit tests and doctests in `src/` stay.
- The workflow's clippy and test commands copy `just lint` and `just test`, so CI needs no tools beyond rustup; change them together.
- crates.io's trusted publishing (OIDC, no stored token) needs the crate to exist first; switching to it after the first release removes the secret.

## Dependencies

One line per crate: why std was not enough.

- `clap` (derive; no `color`, `suggestions`, `wrap_help`): argument parsing, help, and usage errors (ADR-005).
- `ignore` (no default features to turn off): gitignore-aware directory walking with git's rules (ADR-006).
- `globset` (default features off): glob patterns with `**`, classes, and `{a,b}` alternatives; already a dependency of `ignore`.
- `pulldown-cmark` (default features off): CommonMark parsing for the Markdown engine (ADR-007).
- `tree-sitter` (default features off: no query regex engine): the parser runtime of the code-language engines (ADR-008).
- `tree-sitter-typescript`, `tree-sitter-javascript` (no features): the TypeScript, TSX, and JavaScript grammars (ADR-008).
