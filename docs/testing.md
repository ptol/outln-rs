---
description: 'How tests are organized in the outln Rust crate, and how to read, write, and update casefiles.'
readWhen: 'writing or changing tests'
---

# Testing

Run everything with `just test` (`cargo test --locked`).

## Kinds of tests

- **Unit tests** sit next to the code in a `#[cfg(test)] mod tests` at the bottom of the file, for pure functions.
- **Engine contract tests** (`src/language_engine/contract.rs`) run over the whole `ENGINES` registry, so a new engine is checked once registered: unique extensions, and output whose byte ranges stay inside the text for every prefix of each engine sample and for edge inputs. Engine samples (`tests/engine-samples/sample.<first extension>`, one per registry entry) are small files with a header and declarations of most kinds; `just build-static` outlines them too ([ADR-017](adr.md)). A failure of the prefix or edge-input checks, an engine's own panic included, names the engine sample and the input it failed on.
- **Integration tests** are files in `tests/`, each compiled as its own crate against the public library API:
  - `tests/casefiles.rs`: runs every casefile in `tests/cases` (see below). Most behaviour is tested here.
  - `tests/usage.rs`: executable documentation of the public API. Keep it readable; agents learn the API from it.
  - `tests/cli.rs`: runs the compiled binary (`env!("CARGO_BIN_EXE_outln")`) and checks what only the binary does: `src/main.rs` wiring, the real version, exit codes.
  - `tests/repo_conventions.rs`: repository conventions; its header lists them.
  - `tests/support/`: modules shared by test crates through `#[path = "support/<file>.rs"] mod <file>;`. `casefile.rs` parses and writes casefiles; `scratch_dir.rs` makes a temp directory that is removed on drop.
- **Doc tests**: examples in `///` and `//!` comments run as tests, so they can't go stale.

Test names describe the behaviour (`a_failing_file_is_reported_while_the_others_still_print`). `unwrap`, `expect`, and `panic!` are allowed inside `#[test]` functions and `#[cfg(test)]` modules only, because clippy's `allow-*-in-tests` settings cover nothing else: helper functions in `tests/*.rs` return `Result` and the test calls `expect`.

## Casefiles

A casefile (`tests/cases/<group>/<name>.case`) is one test of outln's behaviour: the arguments, the input files, and the expected stdout, stderr, and exit code. The runner writes the input files into a fresh directory under the system temp dir, calls `outln::run` there with version `0.0.0-test` and no global gitignore, and compares all three outputs.

The format, with an example, is the `//!` header of `tests/support/casefile.rs`; read it before writing a casefile. Beyond it:

- The free-form description says what behaviour is under test and why the expected output looks this way.
- A `.gitignore` applies only inside a git repository, so a casefile testing one adds `== file <dir>/.git/HEAD`.
- The pre-commit hook leaves `tests/cases/**` byte-for-byte alone (no whitespace or newline fixes), and `.gitattributes` turns off line-ending conversion there.

### Workflow

- **Add a casefile**: write the description, args, and files, then `== stdout`, `== stderr`, `== exit 0` with empty sections, run `just update-cases <name>`, and check the output it wrote by hand before committing.
- **Run some**: `just case <text>` runs the casefiles whose path contains `<text>`; `just test` runs them all.
- **Change output on purpose**: change the code, run `just update-cases`, review every changed casefile with `git diff`, and note the change in [changelog.md](changelog.md).
- A failure prints the casefile as expected and as it would be with the actual output.

### Expected output

`just update-cases` writes the expected output; check it line by line against the behaviour the description states before committing.
