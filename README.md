---
description: 'What outln is, how to build and run it, and where the project docs live.'
---

# outln

A CLI that prints compact structural outlines of source files (a header, then top-level declarations with line ranges) for coding agents. This is a Rust rewrite of the TypeScript implementation in `../outln`; so far it outlines Markdown files only.

## Quick start

```sh
mise install  # once per clone: the pinned tools, including just
just setup    # once per clone: the git pre-commit hook
just run docs # build and run the CLI: here, a glob view of docs/
just check    # auto-fix, then every check and test
```

## Docs

- [AGENTS.md](AGENTS.md): how agents (and people) work in this repo.
- [docs/overview.md](docs/overview.md): goals, status, and scope.
- [docs/runbook.md](docs/runbook.md): setup, the pre-commit hook, and troubleshooting.

## License

MIT, see [LICENSE](LICENSE).
