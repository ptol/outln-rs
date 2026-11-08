---
description: 'Architecture Decision Records for outln.'
readWhen: 'making or questioning a design decision, or adding a dependency'
---

# Architecture Decision Records

Each record states the decision, why, and what it costs. Add a new ADR rather than editing an accepted one; mark the old one superseded, or amended when the new one replaces only part of it.

## ADR-001: Build outln in Rust

**Status**: Accepted (2026-11-06); amended by ADR-014 (agents navigate with the published build)

### Decision

Build outln in Rust, in this repo, as a single Cargo package with a library and a thin binary.

### Rationale

- A single static binary needs no runtime on the machines that run outln.
- Performance: native code and tree-sitter's Rust bindings.
- tree-sitter is native and has first-class Rust crates.

### Consequences

- Behaviour changes freely (no backward compatibility); [changelog.md](changelog.md) records each change users or agents would notice.

## ADR-002: Local-only tooling with strict lints and an auto-fixing pre-commit hook

**Status**: Accepted (2026-11-06); amended by ADR-010 (replaces "No CI for now": a CI workflow publishes releases, and commits stay gated only by the hook) and ADR-026 (CI also checks every branch push and pull request)

### Decision

- Rust 1.99.1 pinned in `rust-toolchain.toml` and `mise.toml`; edition 2024.
- clippy `all` and `pedantic`, plus restriction lints for `unwrap`/`expect`/`panic`, printing, wildcard matches, and lint exceptions without a reason; `disallowed-methods` for process streams, environment, and clock outside `src/main.rs`; rustc's `unreachable_pub`; `unsafe` forbidden; warnings fail.
- hk runs the pre-commit hook: it hides unstaged edits, formats staged files (rustfmt, dprint), re-stages them, then runs clippy and every test when Rust or Cargo files are staged, and the repository conventions test on every commit.
- just provides the project commands; mise pins the tools.
- No CI for now.

### Rationale

- Agents write most of the code with permission prompts off, so mechanical checks are the only reliable gate.
- hk is the hook manager that re-stages fixes with unstaged edits hidden; prek and pre-commit abort the commit after a fix by design.
- dprint formats Markdown, TOML, JSON, and YAML without Node.

### Consequences

- A fresh clone needs `mise install` (the pinned tools, including just), then `just setup` (the hook).
- Bumping Rust means bumping two files and fixing any new lints.
- `mise.toml` must pin `rust`, because mise exports `RUSTUP_TOOLCHAIN`, which overrides `rust-toolchain.toml`.

## ADR-003: Language engines return data with byte ranges into normalized text

**Status**: Accepted (2026-11-06); amended by ADR-008 (one engine type may serve several grammars, and `Header` has a header comment variant) ADR-016 (`file_summary` returns a `FileSummary` enum, and a data file's engine parses a prefix of the file for it), and ADR-021 (code languages are `static CodeLanguage` tables and data file formats `static DataFile` tables, not one unit struct per language); amended 2026-11-08 (a lone CR becomes LF too, and NUL bytes are an error only when they look binary or UTF-16: `source_text::normalize`, [changelog.md](changelog.md))

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
- Invalid UTF-8 is an error instead of being decoded with replacement characters.
- `Header` has only a frontmatter variant until the first code-language engine adds header comments.

## ADR-004: Casefiles in a plain-text format, run in process on a temp directory

**Status**: Accepted (2026-11-06); amended 2026-11-08 (update mode also adds missing output sections and writes each casefile in canonical form, input files' bytes unchanged: `tests/casefiles.rs`)

### Decision

- Behaviour is tested with casefiles (`tests/cases/**/*.case`): one plain-text file per case with sections for arguments, input files, stdout, stderr, and exit code; the format is documented in `tests/support/casefile.rs` and parsed with std only.
- `tests/casefiles.rs` is a single `#[test]`: it writes each case's files into a fresh directory under the system temp dir, calls `outln::run` there, and reports every failing case at once. `CASE=<text>` filters; `UPDATE_CASES=1` rewrites only the expected-output sections.
- The library reads the real filesystem, with every path resolved against `Context::cwd`; there is no filesystem trait. This is an exception to "inject filesystem I/O" in [codestyle.md](codestyle.md).

### Rationale

- YAML casefiles would need an update mode that re-serializes whole files, which mangles block scalars, and ANSI escapes would need quoted strings; `serde_yaml` is also archived. A sectioned text format avoids both, and update mode leaves inputs byte-for-byte untouched.
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

- Help, version (`outln <version>`), and usage-error text are clap's, and usage errors exit 2.
- clap is built without `color`, `suggestions`, and `wrap_help` (which reads the terminal width), so its output doesn't depend on the terminal.

## ADR-006: Walk directories with the `ignore` crate and git semantics

**Status**: Accepted (2026-11-06); amended by ADR-022 (a walk past 1,024 entries starts over on the crate's parallel walker, and walk errors are sorted by message)

### Decision

The glob view and debug mode walk directories with `ignore::WalkBuilder` defaults: `.gitignore` files apply inside a git repository (including those of parent directories up to its root), plus `.git/info/exclude` and `.ignore` files; dot entries are skipped, symlinks aren't followed, and `node_modules` is always skipped. The crate's own global-gitignore lookup is off: `src/main.rs` finds git's `core.excludesFile` with `ignore::gitignore::gitconfig_excludes_path` and passes it in as `Context::global_gitignore`. Glob patterns are matched with `globset` over a walk of their literal base directory.

### Rationale

- These are the rules of git, ripgrep, and fd, which users and agents already expect.
- The crate would read `HOME`, `XDG_CONFIG_HOME`, and `GIT_CONFIG_*` itself, out of sight of the lints; passing the path keeps environment access in `src/main.rs` and casefiles independent of the machine.

### Consequences

- A `.gitignore` outside a git repository is ignored, and ancestor rules still apply to an explicitly named directory they ignore.
- The global gitignore is added as an explicit ignore file, so it also applies outside git repositories.
- Results are sorted by byte order, not by locale.

## ADR-007: Parse Markdown with pulldown-cmark

**Status**: Accepted (2026-11-06); amended by ADR-023 (frontmatter is read as YAML)

### Decision

The Markdown engine finds headings with pulldown-cmark (default features off). Frontmatter is detected by hand: `---` on the first line up to the next `---` line, with top-level `key: value` fields. Headings inside block quotes and lists are skipped; a heading's section ends before the next heading of the same or a higher level.

### Rationale

- A line scanner misreads fences, indented code, HTML blocks, and setext headings; a CommonMark parser gets them right.
- pulldown-cmark is pure Rust with three dependencies, aims at full CommonMark compliance, and parsed a 124 KB file in 0.7 ms, 25 times faster than `tree-sitter-md`, whose README says it is "not recommended ... where correctness is important".
- pulldown-cmark's own metadata option also accepts a `---` block in the middle of a document, so frontmatter is found before parsing.

### Consequences

- The Markdown engine doesn't share tree-sitter machinery with the code-language engines; section ranges and rendering are shared anyway.
- Heading text is the raw source line without its `#` markers, so inline markup keeps its syntax.

## ADR-008: Parse TypeScript and JavaScript with tree-sitter, on a shared code-engine layer

**Status**: Accepted (2026-11-06); amended by ADR-012 (an engine also says what its doc comment markers mean, a header is its comment's first paragraph, and line ranges start at leading annotations), ADR-018 (`CommentSyntax` gains block-only comments), and ADR-021 (the three grammars are three `CodeLanguage` statics instead of one `Script` type)

### Decision

- One engine type, `Script` (`src/languages/script.rs`), is registered three times in `ENGINES`, once per grammar: `tree-sitter-typescript`'s `typescript` grammar for `.ts`/`.mts`/`.cts`, its `tsx` grammar for `.tsx`, and `tree-sitter-javascript` for `.js`/`.jsx`/`.mjs`/`.cjs`. Every declaration rule is shared; the grammars differ only in syntax one of them lacks.
- Shared by every future code engine: `src/syntax_tree.rs` (parsing, node byte ranges, a node's source on one line) and `src/header_comment.rs` (`//` and `/* */` header comments). An engine decides only where its header comment may start.
- `Header::Comment` holds header comment lines; `OutlineLine::depth` indents class members.
- The header is found by scanning lines, so `file_summary` never parses.
- Versions: `tree-sitter` 0.27.1 (default features off), `tree-sitter-typescript` 0.23.2 (the latest; no release since November 2024), `tree-sitter-javascript` 0.25.0 (the latest).

### Rationale

- tree-sitter has maintained grammars for every planned language, and it parses broken files by recovering instead of failing.
- One engine type keeps the TypeScript and JavaScript rules in one place; the grammar is a field because `LanguageEngine::outline` receives only the text, so each grammar needs its own registry entry.
- The two grammar crates disagree on a few shapes (the TypeScript grammar parses a member's decorators as siblings in the class body, the JavaScript grammar as part of the member). The engine handles both, and `tests/cases/typescript/extensions-and-grammars.case` outlines one source with every grammar.

### Consequences

- The binary is built from C (the grammars), so building needs a C compiler; release size grew from 2.31 MB to 6.26 MB (glibc, x86-64), and a 760 KB file with 20 000 functions outlines in 0.14 s.
- The TypeScript grammar predates newer syntax such as `using` declarations, which parse as errors in `.ts` files but work in JavaScript.
- Debug-mode highlights come from node byte ranges rather than from the printed text's length, so a multi-line signature is highlighted on every line up to its body.

## ADR-009: Build the static binary for musl with zig as the C compiler

**Status**: Accepted (2026-11-06); amended by ADR-011 (the check also outlines a `.rs` file) and ADR-017 (the check outlines every engine sample and compares with the dev build)

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

**Status**: Accepted (2026-11-06); amended by ADR-014 (agents' machines install this crate), ADR-017 (the package also includes the engine samples), and ADR-026 (before uploading, the workflow runs the Check workflow, `just ci`, instead of its own clippy and test commands)

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
- The published crate can't run its integration tests (`cargo package` warns that it drops each one); unit tests and doctests in `src/` stay.
- The workflow's clippy and test commands copy `just lint` and `just test`, so CI needs no tools beyond rustup; change them together. (Since ADR-026 the workflow calls `check.yml` instead, so there is no copy to keep in step.)
- crates.io's trusted publishing (OIDC, no stored token) needs the crate to exist first; switching to it after the first release removes the secret.

## ADR-011: Parse Rust with tree-sitter-rust

**Status**: Accepted (2026-11-06); amended 2026-11-08 (members of `impl` blocks and traits, and inline modules' items, are listed)

### Decision

- The `Rust` engine (`src/languages/rust.rs`) parses `.rs` files with `tree-sitter-rust` 0.24.2, the latest release, on the shared code-engine layer of ADR-008.
- It lists top-level declarations: functions, `struct`, `enum`, `union`, `trait`, `impl`, `mod`, `const`, `static`, `type`, `extern crate`, `extern` blocks, `macro_rules!`, and re-exports (`pub use`, `pub(crate) use`, and other visibilities). Fields, variants, private `use` declarations, and macro calls aren't listed. Consecutive bodiless `mod name;` declarations declared alike share one outline line (`mod cli, discovery`); `use` declarations don't, because joined paths read worse. Amended 2026-11-08: the members of an `impl` block or trait (functions, constants, types) and the items of an inline `mod` (`mod tests { … }`) are listed one level deeper, by the same rules, down to depth 2 (`code_outline::DEEPEST_DEPTH`), because a long `impl` block (clap's `Arg` has six of 600 to 1,300 lines) printed as one line located none of its methods.
- A signature is the declaration's source up to its body (or a `const`'s `=`, or the final `;`), on one line with `syntax_tree::one_line_source_before`.

### Rationale

- The grammar crate needs no dependencies beyond `tree-sitter-language`, which the other grammars already pull in.
- Source-as-written signatures carry types, generics, and `where` clauses, so agents need not open a file to see them.

### Consequences

- The static binary grew from 6.35 MB to 7.51 MB, and `just build-static` also outlines a `.rs` sample; a 1.6 MB file with 20 000 annotated functions outlines in 0.29 s.
- Declarations inside a part of the file the parser couldn't make sense of (an `ERROR` node) aren't listed ([techdebt.md](techdebt.md)).

## ADR-012: Doc-aware header comments, first paragraphs, and leading annotations

**Status**: Accepted (2026-11-06); amended by ADR-016 (`CommentSyntax` gains `#` comments, preamble is checked before comments, and a data file's key takes the comment lines directly above it), ADR-018 (every CSS comment directly above a rule is its leading annotation), and ADR-021 (`DocMarkers` splits into a `CommentSyntax` doc marker table and a `HeaderChoice`, and an `Annotations` table replaces each engine's node classifier); amended 2026-11-08 (the header prints the first prose paragraph and passes over license notices; below)

### Decision

- `header_comment::find_header_comment` reads every comment at the top of the file, up to the first code, as runs: a block comment, or consecutive `//` lines of one kind; scanning goes on right after each comment or preamble, so code or another comment on the same line counts. The engine passes a `CommentSyntax` (a `DocMarkers` value saying what doc markers mean, and whether block comments nest, as Rust's do) and a `preamble_end` function for what may come before or between comments (a shebang, directives, inner attributes spanning lines, skipped with as much of Rust's lexer as brackets need). With `DocMarkers::Ignored` (TypeScript, JavaScript) the header is the first comment; with `DocMarkers::InnerAndOuter` (Rust) it is the first `//!` or `/*!` comment, or else the first plain comment, never a `///` or `/**` one.
- A header comment is only its first paragraph, up to the first line without text when more text follows, in every engine and mode. Amended 2026-11-08: its first prose paragraph (code fences, badge rows, link definitions, and HTML tags aren't prose, and rows of decoration have no text); a comment whose first prose paragraph is a license notice, or that has none, is passed over for the next one. Rust's plain-comment fallback passes over a one-line comment directly above code (`// Std` above `use std::…`), which labels that code; `HeaderChoice::FirstComment` keeps a comment right above code, since TypeScript, JavaScript, CSS, and data files have no file-doc marker and often put the header there. TypeScript, JavaScript, and CSS tool directives written as comments (`/// <reference />`, `// @ts-nocheck`, `/* eslint-disable */`, `/* stylelint-disable */`) are preamble. Over the crates.io registry (15,719 `.rs` files) the glob view changed for 1,096 files, 756 of them license-only comments that now print no header; over a 24,158-file `node_modules` tree, for 2,778 (2,138 of them lucide-react's `@license` banner).
- A declaration's line range starts at its first leading annotation (doc comment, attribute, or decorator directly above it), found by `leading_annotations::leading_annotations_start`; the engine classifies each preceding node. Attributes and decorators stay attached across blank lines, doc comments don't, plain comments are stepped over, and the header's comment is never one.

### Rationale

- Rust files document themselves with `//!` comments, often after a `//` license comment, and this repo's own files start with one.
- Well-documented files have multi-paragraph headers (a summary, then details and examples); a glob view line should carry the summary.
- An agent reading a declaration's line range should get its docs and attributes (`#[cfg(test)]`, `#[derive]`) in the same read; TypeScript decorators were already in the range, so one rule covers both engines.

### Consequences

- Shared code now grows per language through two small hooks (`CommentSyntax`, `PrecedingNode` classification) instead of per-engine copies; Go, Java, Kotlin, and C# doc comments (`///`, `/** */`) fit `DocMarkers::InnerAndOuter`'s outer kind or need one more variant.
- TypeScript and JavaScript output changed: JSDoc comments now start declarations' and class members' line ranges, and headers print only their first paragraph ([changelog.md](changelog.md)).

## ADR-013: Bump, tag, and push releases with cargo-release

**Status**: Accepted (2026-11-06)

### Decision

- `just release <level>` runs `cargo publish --dry-run --locked`, then `cargo-release` (pinned in `mise.toml`, configured in `release.toml`). It sets the version in `Cargo.toml` and `Cargo.lock`, renames the changelog's `## Unreleased` section to `## <version> (<date>)`, commits, tags `v<version>`, and pushes `main` and the tag, after a confirmation prompt.
- cargo-release doesn't publish (`publish = false`): the tag-triggered workflow of ADR-010 still uploads, and its tag checks stay as the backstop.
- The changelog is grouped by release under `## Unreleased`, not by date; the release fails unless that section has an entry.

### Rationale

- Tag `v0.2.0` was pushed with `Cargo.toml` still at 0.1.0, and the workflow refused it: the manual steps (bump, lock, changelog, commit, tag, push) were easy to get out of order. One command derives the tag from the version it just wrote.
- cargo-release is the established Rust tool for this and ships as a single binary, so mise installs it like the other dev tools; the crate gains no dependency.
- release-plz was rejected: it generates changelogs from Conventional Commits, and this changelog is hand-written, and it would replace the publish workflow.

### Consequences

- Releasing needs `mise install` (for cargo-release) and an interactive terminal for the prompt.
- The release commit runs the pre-commit hook (clippy and every test, since Cargo files are staged), as every commit does.

## ADR-014: Agents navigate with the published Rust build

**Status**: Accepted (2026-11-06)

### Decision

- The `outln` on agents' `PATH` is this crate's latest crates.io release, installed by the maintainer's machine setup (mise's `cargo` backend).

### Rationale

- Release 0.2.0 outlines Markdown, TypeScript, JavaScript, and Rust, every language this repo is written in, and it reads `//!` headers.
- The Go, Java, Kotlin, and C# engines don't matter for navigating this repo, so agents needn't wait for them.

### Consequences

- The installed build can lag this repo's code by a release; `just run <args>` runs the working copy.

## ADR-015: Gate rustdoc warnings and the dependency policy in the hook

**Status**: Accepted (2026-11-06); extends ADR-002's checks

### Decision

- An hk step renders the docs (`cargo doc --no-deps --document-private-items` with `RUSTDOCFLAGS=-D warnings`) when Rust or Cargo files are staged; `just doc` runs it alone.
- cargo-deny, pinned in `mise.toml`, checks `deny.toml` when `Cargo.toml`, `Cargo.lock`, or `deny.toml` is staged, and in every `just check` and `just verify`; `just deny` runs it alone. The policy: no known vulnerabilities, no crate RustSec marks unmaintained, no yanked versions, only licenses on the allow list, one version per crate, no `*` requirements, crates.io only.

### Rationale

- Agents learn the API from doc comments, and clippy and the tests don't render them, so a broken intra-doc link would go unnoticed.
- AGENTS.md asks agents to stop before adding an unmaintained crate or one with many transitive dependencies; nothing enforced that, and an advisory against an existing dependency would go unseen.
- cargo-deny covers advisories, licenses, duplicates, and sources in one tool with one config, ships as a single binary through mise, and adds no crate dependency. cargo-audit covers only advisories.

### Consequences

- Committing a dependency change, and `just check`, need the network: cargo-deny fetches the advisory database on every run.
- A new advisory against an unchanged `Cargo.lock` surfaces at the next `just check`, not at commit time.
- A dependency under a license not on the allow list, or one that pulls in a second version of a crate, fails until the maintainer approves an entry in `deny.toml`; every exception there carries a `reason`.
- With `--document-private-items`, rustdoc doesn't warn about public docs that link to private items (`private_intra_doc_links`), which would render as plain text on docs.rs.

## ADR-016: Outline TOML, YAML, and JSON data files with tree-sitter

**Status**: Accepted (2026-11-06); amends ADR-003 and ADR-012; amended by ADR-019 (the glob view prints no preamble), ADR-021 (each format is a `static DataFile` table instead of a format field), and ADR-023 (field summary order and value limits); amended 2026-11-08 (`.jsonc` files are outlined as JSON, `.json5` still isn't; a JSON object missing a comma keeps its keys before the gap, [changelog.md](changelog.md))

### Decision

- One `DataFile` engine type (`src/languages/data_file.rs`), with the format as a field like `Script`, outlines `.toml`, `.yaml`, `.yml`, and `.json` files, parsed with `tree-sitter-toml-ng` 0.7.0, `tree-sitter-yaml` 0.7.2, and `tree-sitter-json` 0.24.8. Each format's child module only finds its keys in the tree; printing, line ranges, and the field summary are shared.
- Outline lines are top-level keys only, never nested: a scalar value prints after the key as written, on one line and cut at 40 characters with `…`; a table, mapping, object, or list prints only the key. TOML lists the keys before the first table, then each `[table]` and `[[array]]` header as written, in file order and unmerged. YAML outlines only the first document. A root array or scalar has no keys.
- A key's line range starts at the comment lines directly above it, indented no deeper than the key, and ends before the blank and comment lines its value ends with, which the grammars put inside the key before (a TOML table runs to the next one). Comments that run on from the header comment, and preamble such as `#:schema`, are never a key's.
- The header is the first comment, after preamble: `#` comments for TOML and YAML (`CommentSyntax::Hash`), `//` and `/* */` for JSON, which tree-sitter-json parses although JSON has none. Preamble (YAML's `%` directives, `---`, and `# yaml-language-server:` modeline; TOML's `#:` directives) is now checked before a comment, so a directive written as a comment is skipped.
- `file_summary` returns a `FileSummary` enum: a header, a field summary, a Markdown file's first heading (without frontmatter fields; pulldown-cmark still scans the body's block structure, but no events after the heading are read), or nothing. Like a field summary, the first heading is glob view only, because file mode already prints it as the first outline line. A data file without a header comment gets a field summary in the glob view only (file mode would repeat its outline lines): its first three scalar top-level keys, `{a: 1, b: 2, ...}`, or the first TOML table's (`[package] {name = "outln", ...}`) when none come before it. It parses only the first 64 KiB, up to the last line break in them (or a character boundary, for a file on one line); after a cut, keys running to the cut don't count and `...` always follows.
- A JSON root object that never closes parses as an `ERROR` node, which flattens nested objects' pairs among its own, so only its pairs before the first other child are keys.

### Rationale

- Agents open configuration and data files as often as code; a glob view of a repo root now summarizes `Cargo.toml`, `package.json`, and CI workflows instead of skipping them.
- The three grammars are maintained by the tree-sitter organizations, are MIT-licensed, and depend only on `tree-sitter-language`, which the other grammars already pull in; cargo deny passes. Hand-written line scanners would get YAML block scalars and flow collections, and JSON strings containing braces, wrong.
- Top-level keys alone keep outlines short: nested keys of a lockfile or a Kubernetes manifest would make the outline as long as the file.
- A field summary needs a parse, unlike every other glob view header; a 6 MB `package-lock.json` takes about 0.25 s to parse whole, while its first fields are in its first lines.

### Consequences

- The YAML and TOML grammars compile C scanners, which `just build-static` now covers with a sample per format; the static binary grew from 7.52 MB to 7.78 MB.
- The glob view's preamble says "Includes only a header or summary per file.", since a line may be a field summary or a Markdown file's first heading.
- Format-specific extras are left out: `.jsonc` and `.json5` aren't recognized, later YAML documents aren't outlined, and lockfiles get no special handling.

## ADR-017: Check every engine against shared engine samples

**Status**: Accepted (2026-11-06); amends ADR-009 (what `just build-static` outlines) and ADR-010 (the package also includes the engine samples); amended 2026-11-08 (the contract also checks the output's shape, the file summary against the outline, and debug mode, and runs on seeded mutations of the samples: `contract.rs`, `contract/mutation.rs`, [testing.md](testing.md); about ten engines are now planned, not eight)

### Decision

- `tests/engine-samples/` holds one engine sample per `ENGINES` entry, named `sample.<first extension>`: a header comment (frontmatter for Markdown) and declarations of most kinds.
- The unit tests in `src/language_engine/contract.rs` run over `ENGINES`: every extension is lowercase and claimed by one engine; the samples match the registry one to one; each sample outlines to a header and declarations; and for every prefix of every sample, plus edge inputs (empty, whitespace, unclosed markers, a 100 000-byte line), every byte range lies inside the text on character boundaries, each highlight lies inside its line range, and rendering doesn't panic.
- `just build-static` outlines all the samples with the static binary and fails unless the output equals the dev build's.
- The package `include`s `tests/engine-samples/*`, so those unit tests pass on the published crate.

### Rationale

- outln will hold about eight engines, so a check written once over the registry beats one per engine, and a new engine is checked as soon as it's registered.
- Agents outline files they're editing; the prefixes stand for half-edited files, the input most likely to put byte ranges from tree-sitter's error recovery in odd places.
- The static build's samples used to be written inline in the justfile, one `case` branch per extension, so a new grammar could be left out; the contract test now keeps the sample set complete.
- Comparing with the dev build checks every outline line, not only one declaration per grammar, without a second copy of the expected output.

### Consequences

- A new engine needs an engine sample before the unit tests pass.
- The prefix check outlines each sample once per character (about 0.3 s for all eight samples in a debug build); a much longer sample makes the unit tests slower.
- `just build-static` builds and runs the dev build too.
- rustfmt and dprint skip the engine samples (`hk.pkl`, `dprint.json`), so they can keep syntax a formatter would rewrite, such as setext headings.
- The published crate grows by the samples, a few kilobytes.

## ADR-018: Outline CSS with tree-sitter-css, rules as declarations

**Status**: Accepted (2026-11-06); amends ADR-008 and ADR-012

### Decision

- A `Css` engine (`src/languages/css.rs`) outlines `.css` files with `tree-sitter-css` 0.25.0 (the latest, from the tree-sitter organization), through `code_outline` like the Rust and Script engines.
- Its declarations are rules: a style rule prints its selectors, an at-rule its keyword and prelude, up to its block or `;`, on one line. The rules directly inside a top-level at-rule's block are listed at depth 1, as class members are; rules nested inside a style rule, `@keyframes` steps, and CSS declarations (`color: red`) aren't listed, and neither is `@charset`.
- The header is the first `/* */` comment, after an `@charset` rule at the file's start; a new `CommentSyntax::Block` reads block comments only, since `//` starts no comment in CSS.
- Every comment directly above a rule is a leading annotation (classified as a doc comment, so a blank line detaches it), as a data file's comments are for its keys.
- A prelude is read from the source text, not joined from the syntax tree's tokens with `syntax_tree::one_line_source`: comments are left out (a comment between two name characters separates them like a space), whitespace runs become one space (none inside brackets when the run holds a line break or comment), and strings, escapes, and unquoted URLs stay as written.

### Rationale

- Agents open stylesheets to change one rule; selectors and at-rule preludes are what they search for, and a `@media` block's rules are hidden in one line without the nested level.
- One nested level keeps outlines short: a stylesheet using CSS nesting would otherwise list every `&:hover` rule.
- CSS has no doc-comment convention, and a comment directly above a rule almost always describes it.
- tree-sitter-css keeps parts of valid preludes out of its leaves (the number in `600px`, a hex color) or in `ERROR` nodes (`>=` in a media query, an attribute selector's `i` flag), so joining tokens loses them when a prelude spans lines; `>` is the child combinator, not a closing bracket; and a comment between selectors (`.a/**/.b`) must not become a space, which would change what the selector matches.
- The grammar depends only on `tree-sitter-language`, which the other grammars already pull in, and is MIT-licensed; cargo deny passes.

### Consequences

- SCSS and Less aren't recognized (they need their own grammars); SCSS syntax in a `.css` file parses as errors.
- A long selector list, such as a CSS reset's, prints on one long outline line, since signatures are never cut.
- The grammar compiles a C scanner, which `just build-static` covers with `tests/engine-samples/sample.css`; the static binary grew from 7.78 MB to 7.91 MB, and a 1 MB file with 11 000 commented rules outlines in 0.15 s.

## ADR-019: Print each file's line count in the glob view, and no preamble

**Status**: Accepted (2026-11-06); amends ADR-016 (the glob view's preamble); amended by ADR-020 (file mode prints the line count too)

### Decision

- Each glob view line prints the file's line count between its path and its summary: `src/cli.rs (120 lines): Command-line arguments.`, `1 line` for one line. The count is the file's line breaks, plus one for a last line without one, so an empty file has `0 lines`.
- The glob view prints only `glob view: <arguments>` before its file lines, dropping the preamble (`Includes only ... per file.` and `For file-level outlines, use outln [FILE]...`). How to use outln lives in the agent's instructions; the README has a section to paste into them.
- File mode and debug mode are unchanged: an outline's last line range already shows roughly where the file ends.

### Rationale

- An agent choosing what to read needs a file's size as much as its purpose: a 40-line file is cheaper to read whole than to outline first, and a 3 000-line one needs its outline.
- Lines, not bytes, because agents read files by line (`offset`/`limit`) and line ranges are what outlines print.
- The glob view already reads every file whole to find its summary, so counting its lines adds one pass over text in memory and no I/O.
- An agent runs outln only because its instructions say to, so they are where the workflow (glob view, then file mode, then reading only the printed line ranges) is taught once per session; a preamble repeated it on every run, and described a line format that changed with each new kind of summary. The `glob view:` line stays because it tells several runs' outputs apart.

### Consequences

- Every glob view line is longer by about ten characters.
- An agent whose instructions name only the glob view isn't told about file mode by the output.

## ADR-020: Print the line count in file mode too

**Status**: Accepted (2026-11-08); amends ADR-019 (file mode prints the line count too)

### Decision

- File mode prints the file's line count after its path, as the glob view does: `src/cli.rs (120 lines)`. One function, `outline::path_and_line_count`, prints it for both modes. File mode takes the count from the `LineIndex` it already builds for line ranges, so the count and the ranges come from one scan of the text.
- The count stays `(N lines)`, not a line range such as `[L1-L120]`.

### Rationale

- The last line range only roughly shows where a file ends: code after the last declaration, or a long file with few declarations, makes it misleading, and a file without declarations shows nothing.
- An agent choosing between reading a file whole and reading its line ranges needs the size in file mode as much as in the glob view.
- Not a line range: a line range is a declaration's span, a place to read, while the count is a size; the `L1-` would repeat on every file, and an empty file has no range to print.

### Consequences

- Every outline's first line is longer by about ten characters.
- Two functions count lines: `LineCount::of` for the glob view, which builds no `LineIndex`, and `LineIndex::line_count`; a unit test keeps them equal.

## ADR-021: Code-language engines as data

**Status**: Accepted (2026-11-08); amends ADR-008 and ADR-012

### Decision

- A code language is a `static CodeLanguage` (`src/code_outline.rs`), not a type implementing `LanguageEngine`: its extensions, grammar, `CommentSyntax`, `HeaderChoice`, preamble function, `Annotations` table, verbatim kinds, and one `declaration_lines` function. `CodeLanguage` implements `LanguageEngine` once for every code language. TypeScript, TSX, and JavaScript are three statics made by one `const fn`. The data file engine's formats are tables the same way: a `static DataFile` per format (grammar, comment syntax, preamble, and its format module's key functions) instead of a `Format` enum matched in five methods.
- `declaration_lines` gets each node that isn't a leading annotation, at every level, with an `OutlineLevel`: the source text, the language's signature helpers (`one_line`, `signature_before`), the depth, and `declarations_inside`, which lists a node's declarations one level deeper with their own line ranges. Lines are pushed into the outline's `Vec`.
- What a comment means and which comment is the header are separate: `comment_syntax::CommentSyntax` holds the line marker, the block comments, and a table of doc markers (prefix to `CommentKind`); `header_comment::HeaderChoice` picks the header (`FirstComment`, `InnerDocElseFirstPlain`). Leading annotations are classified from a per-language `Annotations` table of comment and attribute node kinds, reading doc-ness from the same marker table.
- `LanguageEngine::outline` takes a `SourceText`, the text with its `LineIndex`, built once per file and shared with the rendering; engines find the line around an offset with a binary search.

### Rationale

- outln will hold about ten code languages. Before, each engine repeated the same wiring (extensions, `outline` calling `code_outline`, `file_summary` from the header, its comment syntax), and every helper threaded the text and the verbatim kinds; a table states only what differs, and the compiler checks that a new language fills every field.
- `DocMarkers` mixed which markers are doc comments with which comment is the header, and each engine re-derived doc-ness its own way (text checks, tree-sitter fields, every comment). Go's header rule (the comment attached to `package`) or Java's (the first non-doc comment) are new `HeaderChoice` variants, and new doc comment forms are new `DocMarker` entries, with no edit to the shared walk.
- Nested declarations (class members, CSS rules in an at-rule) each copied the walk that finds line ranges, with depth and "no header" constants per engine; `declarations_inside` tracks both, so listing Rust `impl` and `trait` members is a call in Rust's `declaration_lines`.
- Scanning back and forth to a line's ends for each declaration made outlines of minified one-line files quadratic (a 1.1 MB TypeScript line of 40k functions took 1.76 s, then 0.19 s).

### Consequences

- An engine that isn't a tree-sitter code language (Markdown, the data file engine) still implements `LanguageEngine` itself.
- `declaration_lines` is a plain function pointer, so a language's rules can't capture state; anything it needs comes from the `OutlineLevel` or the node.
- Rust reads doc comments from their markers instead of tree-sitter-rust's `outer` and `inner` fields; both follow rustc's rules, and outlines of 15 000 crates.io files were unchanged.

## ADR-022: Outline files on several threads, printing in input order

**Status**: Accepted (2026-11-08); its glob view figures predate ADR-024, whose declaration summaries parse every headerless code file's start: the registry's glob view, about half of it headerless `.rs` files, now takes about 0.5 s

### Decision

- `src/main.rs` passes `std::thread::available_parallelism` (1 when unknown) as `Context::threads`; library code never asks the machine.
- The glob view, file mode, and debug mode load and outline their files with `parallel::map_in_order`: scoped threads (`std::thread::scope`) that each claim the next unclaimed file from an atomic counter, then results sorted back into input order before anything is printed. Below 32 files per thread it uses fewer threads, and below 64 files only the calling thread.
- A directory walk starts on the calling thread; after 1 024 entries it starts over with the `ignore` crate's parallel walker on up to 8 threads. Found files are sorted as before, and walk errors are now sorted by message.
- stdout is a 64 KiB `BufWriter`; the stderr writer `src/main.rs` passes to `run` flushes it before every write, so stdout and stderr merged into one pipe keep their order.

### Rationale

- Measured on 16 vCPUs, piped: the glob view of 18k files in `~/.cargo/registry/src` went from 940 ms to about 100 ms (buffering 515 ms, faster byte scans 420 ms, parallel files 180 ms, parallel walk 100 ms); file mode on 3 000 registry `.rs` files under 200 KiB from 1.6 s to 0.36 s.
- Printing after all results are ready keeps the printing code a plain loop. Streaming each result through a channel to the printing thread cost about 2 ms on a 67-file tree, because each result woke that thread; that is more than the parallel work saves there.
- `std::thread::scope` lets threads borrow the file list and the rendering closure without `Arc`, and needs no thread-pool crate.
- A walk can't know its size before it ends, and starting the walker threads made a glob view of a 12-file directory, the most common call, take 3.3 ms instead of 1.8. Up to about 1 000 entries one thread was as fast; starting over after 1 024 cost 0.7 ms on a 3 800-entry tree, whose walk the threads cut from 33 to 25 ms. Past 8 threads the large walk got no faster.
- The thread count is a parameter so tests can run on one thread and compare: `tests/usage.rs` checks that 1 and 8 threads print the same, and casefiles run on 4.

### Consequences

- Output appears only after every file is outlined, and a closed pipe (`outln . | head`) is noticed only then: after about 100 ms for a glob view of 18k files, or 1 s for file mode on 3 000.
- The slowest single file bounds file mode: a 2.5 MB generated `.rs` file takes 380 ms on its own.
- A tree of more than 1 024 entries has its first 1 024 walked twice.
- Engines must stay `Sync` (`LanguageEngine` already requires it) and rendering must not share mutable state between files.

## ADR-023: Read frontmatter with the YAML engine, and describing keys first in file summaries

**Status**: Accepted (2026-11-08); amends ADR-007 and ADR-016

### Decision

- A Markdown file's frontmatter is the block from a `---` first line to the next `---` or `...` line (either may end in whitespace) when its content parses, with tree-sitter-yaml, as a YAML mapping; otherwise it is body text, and its `---` lines are thematic breaks. Its fields are the pairs the YAML engine lists as keys (`data_file::yaml_fields`), so both share which pairs are top-level, which values are containers, and how a value written over several lines folds onto one. A mapping broken by a syntax error, which parses as an `ERROR` root, keeps its pairs before the error, in both.
- Frontmatter values still print as JSON literals, uncut, and only a scalar's first paragraph (up to a blank line) folded onto one line; a list or mapping prints its key alone. Data file values print as written, folded the same way, then cut.
- A file summary (a data file's field summary, a Markdown file's frontmatter fields) shows at most three fields: `description`, then `title`, then the others in source order. A data file without scalar keys shows its keys' names. A `description` or `title` value is cut at 100 characters instead of 40, and a cut quoted value gets its quote back after the `…`.
- YAML outlines the first document with content, so an empty first document is skipped.

### Rationale

- The line scanner read only a key's own line, so `|`, `>-`, an opening `"`, and a value on the next lines became literals (`description: "|"`), and a `---` thematic break at the top of a file swallowed the text up to the next one. The YAML grammar is already a dependency, and sharing the data file engine's YAML rules keeps one definition of a top-level key.
- A frontmatter is a header: like a header comment's first paragraph, it prints whole; cutting a doc's `description` or `readWhen` at 40 or 100 characters would hide what an agent picks docs by. JSON literals keep values delimited in a glob view's comma-separated line.
- `description` and `title` are what tell an agent what a file is: a `package.json` summary showed `name`, a cut description, and `private`, and a `Cargo.toml` summary showed `edition` instead of the description.

### Consequences

- The Markdown engine parses a frontmatter block with tree-sitter on every glob view; frontmatter is short, so this costs microseconds per file.
- Quoted frontmatter text is still not unescaped (`'it''s'` prints `"it''s"`), and a `|` block's line breaks print as spaces.

## ADR-024: Summarize code files without a header by their declarations

**Status**: Accepted (2026-11-08); its timings were taken without ADR-022's threads, with which the registry's glob view takes about 0.5 s

### Decision

- In the glob view, a code file (any `CodeLanguage`) without a header comment with text prints a declaration summary instead of `(no header available)`: its first five distinct top-level declarations, each as `CodeLanguage::summary_text` cuts its outline line (`signature::signature_head`: up to the parameter list and its type parameters; a test block keeps its title; CSS keeps the whole prelude), cut at 48 characters, a repeated one with its count (`impl Arg ×6`), then `...` when more may follow.
- Only a file without a header is parsed, and only its first 4 KiB (`DECLARATION_SUMMARY_PREFIX_BYTES`), cut before the last line in them that starts at column 0 with neither whitespace nor a closing bracket, so the parse rarely ends inside a declaration, which tree-sitter would drop as a syntax error. A cut file's summary ends with `...`.
- `source_text::summary_prefix` cuts a text to a byte budget at a line break, for this and for the data file engine's field summary, which still has its own copy.

### Rationale

- 52% of the crates.io registry's `.rs` files and 77% of a `node_modules` tree's script files have no header comment, so their glob view lines said nothing; their first declarations say what they hold, as a data file's field summary does.
- Parsing is the cost: tree-sitter parses about 15 to 20 MB/s here, so whole files (generated bindings of several megabytes) took the registry's glob view from 0.57 s to 8.3 s. A 4 KiB prefix holds five declarations for most files; 2 KiB left 500 more registry files without any.

### Consequences

- The glob view is slower on code without headers. With hyperfine: the registry (18,136 files, 9,368 headerless `.rs`) 0.62 s before, 2.28 s after (1.73 s with a 2 KiB prefix); a 24,158-file `node_modules` tree (18,569 headerless) 0.44 s before, 2.67 s after. Documented code is unchanged: its header is still found without parsing.
- Counts (`×3`) are of the declarations in the prefix read, not the whole file.
- A declaration whose source starts inside the 4 KiB but runs past the cut may be missing or printed from a partial parse.

## ADR-025: A depth limit for the glob view, with directory summaries

**Status**: Accepted (2026-11-08)

### Decision

- `--depth N` (N ≥ 1) limits how many levels of each directory argument a glob view lists: depth 1 is the files directly inside, like `find -maxdepth 1`. Each directory N levels down that holds supported files prints one directory summary instead of its files: its path with a trailing `/`, its supported files below, and their total line count, sorted where its files would have been.
- The walk itself is not limited: a directory is walked whole, with its ignore rules, and `glob_view` assigns each file below the limit to its directory at the limit (`discovery::directory_at_depth_limit`). The summarized files are loaded on `Context::threads` threads to count their lines; one that fails to load counts as a file without lines and isn't reported.
- The limit applies to directory arguments only: a glob pattern already walks only as deep as it can match, and a file argument is listed as given. `--depth` conflicts with `--debug`.
- Without a limit, a glob view of more than 300 files ends with a hint on stderr naming `--depth 1` (or a narrower path or pattern for globs only).

### Rationale

- `outln .` printed 989 lines (about 48k tokens) on a 1 028-file TypeScript repo, and 18 112 lines on `~/.cargo/registry/src`, with no warning; an agent asking for an overview wanted a few dozen lines.
- A cut-off directory that printed nothing would hide most of a tree; its file and line counts say how big it is and whether to look inside.
- Counting needs every file below the limit, so the walk can't stop at the limit. Measured on `~/.cargo/registry/src` (18k files, 16 vCPUs, a busy machine): `--depth 1` took about 25 ms counting files only and 60 ms with line counts, against 100 ms or more for the unlimited glob view; line counts are worth the 35 ms, because a size in lines is what an agent weighs before reading.
- The hint goes to stderr so stdout stays only the glob view's lines, and comes last, where the reader of a long output looks.

### Consequences

- A depth-limited glob view of a huge tree still reads every supported file in it.
- A file below the limit that can't be read is reported only by a glob view that lists it.

## ADR-026: Check every branch push in CI

**Status**: Accepted (2026-11-08); amends ADR-002 (CI checks pushes, not only releases) and ADR-010 (the publish workflow runs this check)

### Decision

- `.github/workflows/check.yml` runs `just ci` on every branch push and pull request: `just verify` (every step of `hk.pkl`), `just build-static`, and `just test-package` (`cargo package`, then `cargo test` in the unpacked crate). mise (`jdx/mise-action`) installs the tools `mise.toml` pins.
- `.github/workflows/publish.yml` calls it (`uses:`) and uploads only after it passes; a tag push runs it there, not as a branch push.
- The pre-commit hook stays the gate on commits; CI doesn't block a push.

### Rationale

- The hook skips the static build (too slow for every commit, ADR-009) and never builds the packaged crate, so either could break unnoticed until a release; a commit made without the hook installed was checked by nothing.
- One recipe for CI and the laptop: `just ci` reproduces a failure locally, and mise gives CI the hook's tool versions.
- Publishing through the same workflow checks a release with every check instead of a copy of clippy and the tests that could drift from `just lint` and `just test`.

### Consequences

- A CI run needs the network for cargo-deny's advisory database and takes as long as the static build.
- A failure shows up after the push; fix it with a new commit.

## Dependencies

One line per crate: why std was not enough.

- `clap` (derive; no `color`, `suggestions`, `wrap_help`): argument parsing, help, and usage errors (ADR-005).
- `ignore` (no default features to turn off): gitignore-aware directory walking with git's rules (ADR-006).
- `globset` (default features off): glob patterns with `**`, classes, and `{a,b}` alternatives; already a dependency of `ignore`.
- `pulldown-cmark` (default features off): CommonMark parsing for the Markdown engine (ADR-007).
- `tree-sitter` (default features off: no query regex engine): the parser runtime of the code-language and data file engines (ADR-008, ADR-016).
- `tree-sitter-typescript`, `tree-sitter-javascript` (no features): the TypeScript, TSX, and JavaScript grammars (ADR-008).
- `tree-sitter-rust` (no features): the Rust grammar (ADR-011).
- `tree-sitter-json`, `tree-sitter-toml-ng`, `tree-sitter-yaml` (no features): the data file grammars (ADR-016).
- `tree-sitter-css` (no features): the CSS grammar (ADR-018).
