---
description: 'What outln is, how to install, build, and run it, what to paste into agent instructions to use it, and where the project docs live.'
---

# outln

A CLI that prints compact structural outlines of source files (a header, then top-level declarations with line ranges) for coding agents. So far it outlines Markdown, TypeScript, JavaScript, Rust, and CSS files, and TOML, YAML, and JSON data files.

## Install

```sh
cargo install outln --locked
```

It builds from source, so it needs a recent Rust (see `rust-version` in `Cargo.toml`) and a C compiler for the tree-sitter grammars. `--locked` builds with the dependency versions this release was tested with.

```sh
outln src/app.ts README.md # an outline of each file: header, declarations, line ranges
outln docs "src/**/*.ts"   # a glob view: one line per file with its line count and header or summary
outln --help               # every mode and option
```

## Use it from your agent's instructions

An agent runs outln only when its instructions say to, and outln's output doesn't explain itself. Paste this into your repo's `AGENTS.md` or `CLAUDE.md`, and adapt the paths:

```md
## Navigate code

Cheapest first. Run each step as its own command, because its output picks the target of the next one:

1. **Find the file**: `outln <dir>` (or a glob such as `outln "src/**/*.ts"`) prints one line per file: its path, its line count, and its header comment or frontmatter.
2. **Find the section**: `outln <file>...` prints each file's header and its declarations with line ranges like `[L12-L40]`.
3. **Read**: read only those line ranges (`Read` with `offset`/`limit`); read a file whole when its line count is small.

Read outln's full output, stderr included. To shorten it, narrow the path or glob, or filter with `grep`.
```

## Develop

```sh
mise install  # once per clone: the pinned tools, including just
just setup    # once per clone: the git pre-commit hook
just run docs # build and run the CLI: here, a glob view of docs/
just check    # auto-fix, then every check and test
```

## Docs

- [AGENTS.md](AGENTS.md): how agents (and people) work in this repo.
- [docs/overview.md](docs/overview.md): goals, status, and scope.
- [docs/runbook.md](docs/runbook.md): setup, the pre-commit hook, releasing, and troubleshooting.

## License

MIT, see [LICENSE](LICENSE).
