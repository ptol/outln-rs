---
description: 'Rules for repo docs: where each kind of knowledge lives, doc kinds, frontmatter and headers for outln, moving docs, keeping them current, their checks, and what stays out.'
readWhen: 'writing, moving, splitting, or deleting docs'
---

# Docs rules

How to write prose for agents (pointers, pruning, leading words) is in the `writing-for-agents` skill. This file covers where docs go and how they stay accurate in this repo.

## Core rules

- **Only what changes the work**: each line gives a step, a constraint, a decision, a reason, or a gotcha. Test: if deleting the line would not change how the next agent works, delete it. Typical filler: introductions, history of how things came to be, summaries of what follows, restating the heading, praise or motivation.
- **Pointers over copies**: if a fact takes one file read or one command to find (`justfile`, `Cargo.toml`, `--help`), point to it rather than restate it. Keep specifications that tests check, such as casefiles: they are the source the code follows.
- **Record the why**: write down what code can't show: the reasons behind choices, gotchas, rejected alternatives and what they measured.
- **One area, one doc**: each doc covers one area, and each area has one doc. Before creating a doc, run `outln docs` and look for the one that owns the area.
- **One source of truth**: each fact lives in one place; other docs link to it.
- **Few words**: prefer lists and short sentences.
- **Positive phrasing**: state the target behaviour. A rule that has to be a "don't" also says what to do instead.

## Placement

Put knowledge next to what it describes: a fact about one module lives in that module's comments.

| Knowledge                                                               | Home                                                                                                                           |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Public API behaviour                                                    | `///` doc comment at the declaration, with an example (it runs as a doctest)                                                   |
| Usage examples                                                          | `tests/usage.rs` and the doc examples in `src/lib.rs`, which run, so they fail when the API changes                            |
| File purpose                                                            | the file's `//!` header comment                                                                                                |
| One language engine's quirks and how it maps the language to an outline | the `//!` header and doc comments of `src/languages/<language>.rs`                                                             |
| CLI behaviour, exact output, and why it looks that way                  | a casefile, its description holding the why, or the test that [testing.md](testing.md) names for what a casefile can't express |
| Module layout, data flow, and steps every language engine follows       | [architecture.md](architecture.md)                                                                                             |
| A design decision or a new dependency, with its reason                  | a new ADR in [adr.md](adr.md); add a crate to its Dependencies list                                                            |
| Rust code style                                                         | [codestyle.md](codestyle.md)                                                                                                   |
| A `just` recipe                                                         | its comment in `justfile` (`just` lists them)                                                                                  |
| A toolchain, hook, or library gotcha that cost time                     | [til.md](til.md)                                                                                                               |
| A shortcut taken, its impact, and the way out                           | [techdebt.md](techdebt.md)                                                                                                     |
| A change users or agents would notice                                   | [changelog.md](changelog.md)                                                                                                   |
| Goals and current status                                                | [overview.md](overview.md)                                                                                                     |
| Domain terms                                                            | [GLOSSARY.md](GLOSSARY.md), following the Domain terms section of `AGENTS.md`                                                  |
| Rules every session needs                                               | `AGENTS.md`, kept tiny because it costs context every turn                                                                     |

`outln docs` shows only `docs/`. Knowledge elsewhere is reached by a pointer: a step in `AGENTS.md`'s Navigate code section, or a link from the doc for the task.

## Doc kinds

Each doc is mainly one of these kinds. Split off a part when the readers of the rest never need it.

- **Guide**: ordered steps toward one task, ending in a check that tells the agent the task is done; a step whose end isn't obvious gets its own check (Adding a language engine in `architecture.md`).
- **Rules**: reference looked up when needed (`codestyle.md`, this file).
- **Lessons**: what something cost and how it was solved, led by the symptom (`til.md`). A later result that disproves a lesson replaces it.
- **Catalog**: a flat list of entries looked up by name or ID (`GLOSSARY.md`, `adr.md`, `techdebt.md`, `changelog.md`).

## Frontmatter and headers

`outln docs` is the docs map: it prints each doc's frontmatter, so the frontmatter alone must tell an agent whether to open the doc.

- `description`: a summary of what the doc contains, at most 200 characters. Every Markdown file has one, including `README.md` and `AGENTS.md`.
- `readWhen` (required in `docs/`): the task that calls for the doc, phrased to follow "Read when" (`'writing or changing tests'`). Two docs share a task only when both are really needed for it.
- Headers: one `#` title, then `##`/`###` sections named for what they contain, so `outln <file>` lets an agent read only the section it needs.
- File names: kebab-case, naming the area (`testing.md`, `docs-rules.md`). `AGENTS.md`, `README.md`, and `GLOSSARY.md` keep their conventional upper-case names.

## References

- Link to files with relative paths, and name code by its top-level symbol, which `outln <file>` lists (`LanguageEngine`, `ENGINES`).
- Point to a section by its heading and to code by its symbol, not by line number.
- Commands and short syntax examples belong in docs; copy no source or test code, since doctests, `tests/usage.rs`, and casefiles hold the executable examples.

## Moving and deleting

- Moving, renaming, or deleting a doc updates every incoming reference in the same change: links in docs, `//!` and `///` comments, casefile descriptions, `README.md`, and `AGENTS.md`. Find them with `git grep -n --untracked '<file name>'`.
- A change that makes a doc wrong updates it in the same commit. Delete a doc, section, or entry once nothing still needs it (a paid-off `techdebt.md` entry, a superseded lesson).
- An accepted ADR is the exception: add a new ADR and mark the old one superseded, as `adr.md` says.

## Size

- Keep a doc under 300 lines; past that, split it by area or by branch.
- Catalogs are exempt: they are read by entry, not end to end.

## Out of docs

- The plan for a change and its TODOs go in the commit message, or in `techdebt.md` once a shortcut is committed; session notes and investigation logs are not kept. What outln still lacks is status, kept in `overview.md`.

## Checks

`tests/repo_conventions.rs` (part of `just test`, `just check`, and every commit's pre-commit hook) enforces the mechanical rules; its `//!` header lists them. Links are checked only in Markdown files, and only that their target file exists. Keep the rest by hand: description length, heading structure, `#anchor` targets, and links in doc comments and casefiles.

## When an agent stumbles

When an agent misses or misreads a doc, fix the cause: its `readWhen`, its location, or the pointer to it, not only its content.
