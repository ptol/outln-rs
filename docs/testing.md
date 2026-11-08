---
description: 'How tests are organized in the outln Rust crate, and how to read, write, and update casefiles.'
readWhen: 'writing or changing tests'
---

# Testing

Run everything with `just test` (`cargo test --locked`).

## Kinds of tests

- **Unit tests** sit next to the code in a `#[cfg(test)] mod tests` at the bottom of the file, for pure functions.
- **Engine contract tests** (`src/language_engine/contract.rs`) run over the whole `ENGINES` registry, so a new engine is checked once registered: unique extensions, an engine sample per engine that parses without syntax errors (for engines with a tree-sitter grammar), and the **output contract** on every prefix of each engine sample, on edge inputs, and on seeded mutations of the samples (see Fuzzing). The output contract (`check_contract`): byte ranges of whole characters inside the text, highlights inside their line ranges; outline lines in source order, nested as a tree, each printed on one non-blank line; a file summary on one line that agrees with the outline (a printed header is the summary; otherwise the summary is drawn from the outline's lines, as `check_summary_matches_outline` defines); and debug mode that changes nothing but its markers. Engine unit tests outline text with `language_engine::outline_of` and `outline_texts` instead of helpers of their own. Engine samples (`tests/engine-samples/sample.<first extension>`, one per registry entry) are small files with a header and declarations of most kinds; `just build-static` outlines them too ([ADR-017](adr.md)). A broken rule, an engine's own panic included, fails naming the engine sample, the input, and the rule.
- **Integration tests** are files in `tests/`, each compiled as its own crate against the public library API:
  - `tests/casefiles.rs`: runs every casefile in `tests/cases` (see below). Most behaviour is tested here.
  - `tests/usage.rs`: executable documentation of the public API. Keep it readable; agents learn the API from it.
  - `tests/cli.rs`: runs the compiled binary (`env!("CARGO_BIN_EXE_outln")`) and checks what only the binary does: `src/main.rs` wiring, the real version, exit codes.
  - `tests/repo_conventions.rs`: repository conventions; its header lists them.
  - `tests/support/`: modules shared by test crates through `#[path = "support/<file>.rs"] mod <file>;`. `casefile.rs` parses and writes casefiles; `scratch_dir.rs` makes a temp directory, unique within the run, that is removed on drop.
- **Doc tests**: examples in `///` and `//!` comments run as tests, so they can't go stale.

Test names describe the behaviour (`a_failing_file_is_reported_while_the_others_still_print`). `unwrap`, `expect`, and `panic!` are allowed inside `#[test]` functions and `#[cfg(test)]` modules only, because clippy's `allow-*-in-tests` settings cover nothing else: helper functions in `tests/*.rs` return `Result` and the test calls `expect`. The `disallowed-methods` of `clippy.toml` (writing files, starting processes, reading the environment) apply to tests too: a test helper that needs one carries `#[expect(clippy::disallowed_methods, reason = "...")]`, as `ScratchDir` does.

A test that can't check anything in some environment (a permission test running as root) prints why and returns, rather than passing silently.

## Fuzzing

`mutated_engine_samples_keep_the_output_contract` applies one to three seeded edits to each engine sample (delete or paste a span, insert or repeat a token from a per-language list, truncate either end, duplicate a line), normalizes the result as `source_file::load` does, and checks the output contract; `contract/mutation.rs` holds the edits, the random number generator, and the shrinking. A plain test run checks 200 inputs per engine with seed 1, in about half a second.

- **Fuzz harder**: `just fuzz [iterations] [seed]` (default 10 000 inputs per engine, a random seed, printed first; 50 000 take about two minutes) also runs `adversarial_inputs_take_close_to_linear_time`, which prints how much slower each engine gets on four times a repeated unit (`{`, `- `, ...) and fails above 40 times.
- **Reproduce a failure**: it prints the seed, the input's number, the input, and a shrunk input that still fails; `just fuzz <number + 1> <seed>` reruns it, because each input depends only on the seed, the engine, and its number. Turn the shrunk input into a unit test or casefile with the fix.
- **A process abort** (`SIGABRT`, a C assertion in a grammar, which no `catch_unwind` can stop) prints no input: find its number by bisecting the iteration count with the same seed.

## Casefiles

A casefile (`tests/cases/<group>/<name>.case`) is one test of outln's behaviour: the arguments, the input files, and the expected stdout, stderr, and exit code. The runner writes the input files into a fresh directory under the system temp dir, calls `outln::run` there with version `0.0.0-test`, no global gitignore, and debug mode's `«` `»` highlight markers (as when stdout is a pipe), and compares all three outputs.

The format, with an example, is the `//!` header of `tests/support/casefile.rs`; read it before writing a casefile. Beyond it:

- The name says what behaviour is under test, in kebab-case and without a number (`multi-declarator-const.case`); a debug mode case outside `debug/` has `debug` in its name, so `just case debug` finds it.
- The free-form description, required before `== args`, says what behaviour is under test and why the expected output looks this way, specific to this case.
- A `.gitignore` applies only inside a git repository, so a casefile testing one adds `== file <dir>/.git/HEAD`.
- The pre-commit hook leaves `tests/cases/**` byte-for-byte alone (no whitespace or newline fixes), and `.gitattributes` turns off line-ending conversion there.

### Workflow

- **Add a casefile**: write the description, `== args`, and `== file` sections, run `just update-cases <name>` (it adds the `== stdout`, `== stderr`, and `== exit` sections), and check the output it wrote by hand before committing.
- **Run some**: `just case <text>` runs the casefiles whose path contains `<text>`; `just test` runs them all.
- **Change output on purpose**: change the code, run `just update-cases`, review every changed casefile with `git diff`, and note the change in [changelog.md](changelog.md).
- A failure prints a line diff of each differing output section, as written in the casefile (`-` expected, `+` actual), for the first five failing casefiles, then names the rest. Update mode also puts a casefile in canonical form.

### Expected output

`just update-cases` writes the expected output; check it line by line against the behaviour the description states before committing.
