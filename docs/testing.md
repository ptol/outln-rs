---
description: 'How tests are organized in the outln Rust crate, and how to read, write, and update casefiles.'
readWhen: 'writing or changing tests'
---

# Testing

Run everything with `just test` (`cargo test --locked`).

## Kinds of tests

- **Unit tests** sit next to the code in a `#[cfg(test)] mod tests` at the bottom of the file, for pure functions.
- **Integration tests** are files in `tests/`, each compiled as its own crate against the public library API:
  - `tests/casefiles.rs`: runs every casefile in `tests/cases` (see below). Most behaviour is tested here.
  - `tests/usage.rs`: executable documentation of the public API. Keep it readable; agents learn the API from it.
  - `tests/cli.rs`: runs the compiled binary (`env!("CARGO_BIN_EXE_outln")`) and checks what only the binary does: `src/main.rs` wiring, the real version, exit codes.
  - `tests/repo_conventions.rs`: repository conventions (header comments, inner `allow` attributes, frontmatter, links, pinned tool versions).
  - `tests/support/`: modules shared by test crates through `#[path = "support/<file>.rs"] mod <file>;`. `casefile.rs` parses and writes casefiles; `scratch_dir.rs` makes a temp directory that is removed on drop.
- **Doc tests**: examples in `///` and `//!` comments run as tests, so they can't go stale.

Test names describe the behaviour (`a_failing_file_is_reported_while_the_others_still_print`). `unwrap`, `expect`, and `panic!` are allowed inside `#[test]` functions and `#[cfg(test)]` modules, not in helper functions of `tests/*.rs`: helpers return `Result` and the test calls `expect`.

## Casefiles

A casefile (`tests/cases/<group>/<name>.case`) is one test of outln's behaviour: the arguments, the input files, and the expected stdout, stderr, and exit code. The runner writes the input files into a fresh directory under the system temp dir, calls `outln::run` there with version `0.0.0-test` and no global gitignore, and compares all three outputs.

```text
Free-form description: the behaviour under test, and why the expected output looks this way.
== args
input/guide.md
== file input/guide.md
# Guide

## Install
== stdout
input/guide.md
[L1-L3] # Guide
[L3-L3] ## Install
== stderr
== exit 0
```

The full format is the header of `tests/support/casefile.rs`. In short:

- Sections come in this order: `== args` (one argument per line), any number of `== file <path>`, `== stdout`, `== stderr`, `== exit <code>`.
- A section's content is its lines, each ending with a newline. In `== stdout` and `== stderr`, the text `\x1b` stands for the ESC byte of ANSI colours.
- Add ` escaped` to a section marker (`== file input/crlf.md escaped`) for bytes a plain section can't hold: `\r`, `\n`, `\t`, `\xNN`, `\\`, and a `\` at the end of a line for "no newline here". Use it for CRLF, NUL or invalid UTF-8, a missing final newline, or a line starting with `== ` (write `\x3d= `).
- A `.gitignore` applies only inside a git repository, so a casefile testing one adds `== file <dir>/.git/HEAD`.
- The pre-commit hook leaves `tests/cases/**` byte-for-byte alone (no whitespace or newline fixes), and `.gitattributes` turns off line-ending conversion there.

### Workflow

- **Add a casefile**: write the description, args, and files, then `== stdout`, `== stderr`, `== exit 0` with empty sections, run `just update-cases <name>`, and check the output it wrote by hand before committing.
- **Run some**: `just case <text>` runs the casefiles whose path contains `<text>`; `just test` runs them all.
- **Change output on purpose**: change the code, run `just update-cases`, review every changed casefile with `git diff`, and note the change in [changelog.md](changelog.md).
- A failure prints the casefile as expected and as it would be with the actual output.

### Ported casefiles

Casefiles whose description starts with `Ported from ../outln/__tests__/cases/...` came from the TypeScript build's `.case.yaml` cases. Their expected output is what the TypeScript build printed, unless the description says why it differs. Cases for other languages are ported with their engines: write each as a casefile in this format, and get its expected output from the installed TypeScript `outln` on the same inputs.
