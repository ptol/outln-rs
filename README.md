---
description: 'What outln is, how to install, build, and run it, and where the project docs live.'
---

# outln

A CLI that prints compact structural outlines of source files (a header, then top-level declarations with line ranges) for coding agents. This is a Rust rewrite of the TypeScript implementation in `../outln`; so far it outlines Markdown, TypeScript, and JavaScript files.

## Install

```sh
cargo install outln --locked
```

It builds from source, so it needs a recent Rust (see `rust-version` in `Cargo.toml`) and a C compiler for the tree-sitter grammars. `--locked` builds with the dependency versions this release was tested with.

```sh
outln src/app.ts README.md # an outline of each file: header, declarations, line ranges
outln docs "src/**/*.ts"   # a glob view: one line per file with its header
outln --help               # every mode and option
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
