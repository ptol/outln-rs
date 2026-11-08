//! Runs every casefile in `tests/cases` through `outln::run` and checks its stdout, stderr, and exit code.
//!
//! - `CASE=<text> cargo test --test casefiles` (`just case <text>`) runs only the casefiles whose
//!   path contains `<text>`.
//! - `UPDATE_CASES=1` (`just update-cases <text>`) writes the actual output into the casefiles
//!   instead of failing: it rewrites differing output sections, adds missing ones, and puts each
//!   casefile in canonical form. Review the result with `git diff`.
//! - A failure prints a line diff of each differing output section; past
//!   `MAX_PRINTED_FAILURES`, failing casefiles are only named.
//!
//! The format is documented in `tests/support/casefile.rs`.

#[path = "support/casefile.rs"]
mod casefile;
#[path = "support/scratch_dir.rs"]
mod scratch_dir;

use std::fmt::Write as _;
use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use casefile::{Casefile, MissingOutput, Section};
use scratch_dir::ScratchDir;

const CASES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/cases");
const CASEFILE_EXTENSION: &str = "case";
/// Printed by `--version` in every casefile, so a release doesn't change expected output.
const VERSION: &str = "0.0.0-test";
/// A change to shared output can fail most casefiles at once; the first few show what changed.
const MAX_PRINTED_FAILURES: usize = 5;
/// Unchanged lines kept around each change in a section diff; longer runs are collapsed.
const CONTEXT_LINES: usize = 2;
/// More than one, as on most machines; casefile trees are small, so they still run on one thread.
const THREADS: NonZeroUsize = NonZeroUsize::new(4).unwrap();

/// What the environment asks this run to do.
struct Settings {
    filter: Option<String>,
    mode: Mode,
}

/// Whether a run checks the casefiles or writes the actual output into them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Check,
    Update,
}

#[expect(
    clippy::disallowed_methods,
    reason = "the casefile runner's own switches are environment variables, like cargo's"
)]
fn settings_from_environment() -> Settings {
    let update = std::env::var_os("UPDATE_CASES").is_some_and(|value| value == "1");
    Settings {
        filter: std::env::var("CASE")
            .ok()
            .filter(|filter| !filter.is_empty()),
        mode: if update { Mode::Update } else { Mode::Check },
    }
}

#[test]
fn casefiles_match_outln_output() {
    let settings = settings_from_environment();
    let cases = find_casefiles(Path::new(CASES_DIR)).expect("tests/cases should be readable");
    let selected: Vec<&PathBuf> = cases
        .iter()
        .filter(|path| {
            settings
                .filter
                .as_ref()
                .is_none_or(|filter| case_name(path).contains(filter.as_str()))
        })
        .collect();
    assert!(
        !selected.is_empty(),
        "no casefile under tests/cases matches CASE={:?}",
        settings.filter.clone().unwrap_or_default()
    );
    check_temp_dir_is_isolated().expect("the temp dir must not be inside a git repository");

    let failures: Vec<(String, String)> = selected
        .iter()
        .enumerate()
        .filter_map(|(index, path)| {
            check_casefile(path, index, settings.mode)
                .err()
                .map(|failure| (case_name(path), failure))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{}",
        failure_report(&failures, selected.len(), settings.filter.as_deref())
    );
}

/// Runs one casefile; in update mode, writes the actual output into it instead of failing.
fn check_casefile(path: &Path, index: usize, mode: Mode) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|error| format!("cannot read: {error}"))?;
    let missing_output = match mode {
        Mode::Check => MissingOutput::Reject,
        Mode::Update => MissingOutput::AddEmpty,
    };
    let casefile = Casefile::parse(&text, missing_output)?;
    let actual = run_outln(&casefile, index)?;
    match mode {
        Mode::Check => check_output(&casefile, &text, &actual),
        Mode::Update => {
            let updated = Casefile {
                stdout: Section::from_output(&actual.stdout),
                stderr: Section::from_output(&actual.stderr),
                exit_code: actual.exit_code,
                ..casefile
            };
            write_if_changed(path, &text, &updated.to_text())
        }
    }
}

/// What one run of outln printed and returned.
struct Output {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: u8,
}

/// Compares the actual output with the casefile's sections; the error shows what differs.
fn check_output(casefile: &Casefile, text: &str, actual: &Output) -> Result<(), String> {
    let mut report = String::new();
    if casefile.stdout.output_bytes()? != actual.stdout {
        let section = Section::from_output(&actual.stdout);
        report.push_str(&section_diff("stdout", &casefile.stdout, &section));
    }
    if casefile.stderr.output_bytes()? != actual.stderr {
        let section = Section::from_output(&actual.stderr);
        report.push_str(&section_diff("stderr", &casefile.stderr, &section));
    }
    if casefile.exit_code != actual.exit_code {
        let _ = writeln!(
            report,
            "== exit: expected {}, actual {}",
            casefile.exit_code, actual.exit_code
        );
    }
    if report.is_empty() && casefile.to_text() != text {
        report.push_str("not in canonical form (a missing final newline, say)\n");
    }
    if report.is_empty() {
        Ok(())
    } else {
        Err(report)
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "update mode writes casefiles; library code only reads files"
)]
fn write_if_changed(path: &Path, old: &str, new: &str) -> Result<(), String> {
    if old == new {
        return Ok(());
    }
    fs::write(path, new).map_err(|error| format!("cannot update: {error}"))
}

/// The assertion message for failing casefiles: the first few in full, the rest by name.
fn failure_report(failures: &[(String, String)], selected: usize, filter: Option<&str>) -> String {
    let mut report = format!("{} of {selected} casefiles failed", failures.len());
    for (name, failure) in failures.iter().take(MAX_PRINTED_FAILURES) {
        let _ = write!(report, "\n\n--- {name}\n{failure}");
    }
    let unprinted = failures.iter().skip(MAX_PRINTED_FAILURES);
    if failures.len() > MAX_PRINTED_FAILURES {
        let _ = write!(
            report,
            "\n\nAlso failing ({}):",
            failures.len() - MAX_PRINTED_FAILURES
        );
        for (name, _) in unprinted {
            let _ = write!(report, "\n  {name}");
        }
    }
    // One failure names its own filter; otherwise the run's filter, if any, covers them all.
    let filter = match failures {
        [(name, _)] => Some(name.as_str()),
        _ => filter,
    };
    let _ = write!(
        report,
        "\n\nTo accept the actual output, run `just update-cases{}` and review the changes \
         with `git diff`.",
        filter
            .map(|filter| format!(" '{filter}'"))
            .unwrap_or_default()
    );
    report
}

/// A section's marker line and a line diff from the expected to the actual section, as written.
fn section_diff(name: &str, expected: &Section, actual: &Section) -> String {
    format!(
        "== {name}\n{}",
        line_diff(expected.as_written(), actual.as_written())
    )
}

/// One line of a line diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffLine<'a> {
    Same(&'a str),
    Removed(&'a str),
    Added(&'a str),
}

/// A line diff of `expected` against `actual`: `-` lines are only expected, `+` lines only
/// actual. Runs of unchanged lines longer than `CONTEXT_LINES` on each side of a change are
/// collapsed into one `  ...` line.
fn line_diff(expected: &str, actual: &str) -> String {
    let lines = diff_lines(
        &expected.lines().collect::<Vec<_>>(),
        &actual.lines().collect::<Vec<_>>(),
    );
    let changed: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !matches!(line, DiffLine::Same(_)))
        .map(|(index, _)| index)
        .collect();
    let near_a_change = |index: usize| {
        changed
            .iter()
            .any(|&change| index.abs_diff(change) <= CONTEXT_LINES)
    };
    let mut text = String::new();
    let mut collapsed = false;
    for (index, line) in lines.iter().enumerate() {
        let (prefix, content) = match line {
            DiffLine::Same(_) if !near_a_change(index) => {
                if !collapsed {
                    text.push_str("  ...\n");
                    collapsed = true;
                }
                continue;
            }
            DiffLine::Same(content) => (' ', content),
            DiffLine::Removed(content) => ('-', content),
            DiffLine::Added(content) => ('+', content),
        };
        collapsed = false;
        let _ = writeln!(text, "{prefix} {content}");
    }
    text
}

/// The shortest edit from `old` to `new` that keeps their longest common subsequence of lines
/// (the textbook dynamic-programming LCS diff; quadratic, which casefile output sizes allow).
fn diff_lines<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<DiffLine<'a>> {
    // `common[i][j]`: the length of the longest common subsequence of `old[i..]` and `new[j..]`.
    let mut common = vec![vec![0_usize; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            common[i][j] = if old[i] == new[j] {
                common[i + 1][j + 1] + 1
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let mut lines = Vec::with_capacity(old.len().max(new.len()));
    let (mut i, mut j) = (0, 0);
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            lines.push(DiffLine::Same(old[i]));
            i += 1;
            j += 1;
        } else if i < old.len() && (j == new.len() || common[i + 1][j] >= common[i][j + 1]) {
            lines.push(DiffLine::Removed(old[i]));
            i += 1;
        } else {
            lines.push(DiffLine::Added(new[j]));
            j += 1;
        }
    }
    lines
}

/// Writes the input files into a scratch directory and runs outln there.
fn run_outln(casefile: &Casefile, index: usize) -> Result<Output, String> {
    let dir = ScratchDir::new(&format!("case-{index}")).map_err(|error| error.to_string())?;
    for (path, section) in &casefile.files {
        dir.write(path, &section.input_bytes()?)
            .map_err(|error| format!("cannot write {path}: {error}"))?;
    }
    let context = outln::Context {
        cwd: dir.path(),
        version: VERSION,
        global_gitignore: None,
        threads: THREADS,
    };
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let status = outln::run(
        casefile.args.arguments()?,
        &context,
        &mut stdout,
        &mut stderr,
    )
    .map_err(|error| format!("writing to a Vec failed: {error}"))?;
    Ok(Output {
        stdout,
        stderr,
        exit_code: status.code(),
    })
}

/// Every casefile under `dir`, sorted by path.
fn find_casefiles(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    let entries = fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            found.extend(find_casefiles(&path)?);
        } else if path
            .extension()
            .is_some_and(|extension| extension == CASEFILE_EXTENSION)
        {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

/// The casefile's path below `tests/cases`, which names it in failures and `CASE` filters.
fn case_name(path: &Path) -> String {
    path.strip_prefix(CASES_DIR)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// outln reads `.ignore` files in every parent directory and `.gitignore` files up to the git
/// root, so either above the temp dir would change what the casefiles see.
#[expect(
    clippy::disallowed_methods,
    reason = "checks the temp dir that ScratchDir writes the casefiles' input files into"
)]
fn check_temp_dir_is_isolated() -> Result<(), String> {
    let temp_dir = std::env::temp_dir();
    for ancestor in temp_dir.ancestors() {
        for marker in [".git", ".ignore"] {
            let path = ancestor.join(marker);
            if path.exists() {
                return Err(format!(
                    "{} exists above the temp dir {}; set TMPDIR to a directory outside any git repository",
                    path.display(),
                    temp_dir.display()
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn line_diff_marks_changes_and_collapses_distant_unchanged_lines() {
    let expected = "a\nb\nc\nd\ne\nf\ng\n";
    let actual = "a\nb\nc\nd\ne\nF\ng\nh\n";
    assert_eq!(
        line_diff(expected, actual),
        "  ...\n  d\n  e\n- f\n+ F\n  g\n+ h\n"
    );
    assert_eq!(line_diff("", "x\n"), "+ x\n");
}

#[test]
fn failure_report_prints_the_first_failures_and_names_the_rest() {
    let failures: Vec<(String, String)> = (0..=MAX_PRINTED_FAILURES)
        .map(|number| {
            (
                format!("group/case-{number}.case"),
                "== exit: expected 0, actual 1\n".to_owned(),
            )
        })
        .collect();
    let report = failure_report(&failures, 10, None);
    assert!(
        report.starts_with("6 of 10 casefiles failed\n\n--- group/case-0.case\n== exit"),
        "{report}"
    );
    assert!(
        report.contains("Also failing (1):\n  group/case-5.case\n"),
        "{report}"
    );
    assert!(!report.contains("--- group/case-5.case"), "{report}");
    assert!(
        report.ends_with("run `just update-cases` and review the changes with `git diff`."),
        "{report}"
    );

    let one = failure_report(&failures[..1], 10, Some("group"));
    assert!(
        one.contains("`just update-cases 'group/case-0.case'`"),
        "{one}"
    );
}
