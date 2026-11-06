//! Runs every casefile in `tests/cases` through `outln::run` and checks its stdout, stderr, and exit code.
//!
//! - `CASE=<text> cargo test --test casefiles` runs only the casefiles whose path contains `<text>`.
//! - `UPDATE_CASES=1` rewrites the expected output of failing casefiles instead of failing; review
//!   the result with `git diff`.
//!
//! The format is documented in `tests/support/casefile.rs`.

#[path = "support/casefile.rs"]
mod casefile;
#[path = "support/scratch_dir.rs"]
mod scratch_dir;

use std::fs;
use std::path::{Path, PathBuf};

use casefile::{Casefile, Section};
use scratch_dir::ScratchDir;

const CASES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/cases");
const CASEFILE_EXTENSION: &str = "case";
/// Printed by `--version` in every casefile, so a release doesn't change expected output.
const VERSION: &str = "0.0.0-test";

/// What the environment asks this run to do.
struct Settings {
    filter: Option<String>,
    update: bool,
}

#[expect(
    clippy::disallowed_methods,
    reason = "the casefile runner's own switches are environment variables, like cargo's"
)]
fn settings_from_environment() -> Settings {
    Settings {
        filter: std::env::var("CASE")
            .ok()
            .filter(|filter| !filter.is_empty()),
        update: std::env::var_os("UPDATE_CASES").is_some_and(|value| value == "1"),
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
        settings.filter.unwrap_or_default()
    );
    check_temp_dir_is_isolated().expect("the temp dir must not be inside a git repository");

    let failures: Vec<String> = selected
        .iter()
        .enumerate()
        .filter_map(|(index, path)| {
            check_casefile(path, index, settings.update)
                .err()
                .map(|failure| format!("--- {}\n{failure}", case_name(path)))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} casefiles failed (rerun with UPDATE_CASES=1 to accept the actual output):\n\n{}",
        failures.len(),
        selected.len(),
        failures.join("\n")
    );
}

/// Runs one casefile; in update mode, rewrites its expected output instead of failing.
fn check_casefile(path: &Path, index: usize, update: bool) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|error| format!("cannot read: {error}"))?;
    let mut casefile = Casefile::parse(&text)?;
    if casefile.to_text() != text {
        return Err(
            "not in canonical form; check the section order and trailing newline".to_owned(),
        );
    }

    let actual = run_outln(&casefile, index)?;
    let expected = (
        casefile.stdout.output_bytes()?,
        casefile.stderr.output_bytes()?,
        casefile.exit_code,
    );
    if actual == expected {
        return Ok(());
    }
    let (stdout, stderr, exit_code) = actual;
    casefile.stdout = Section::from_output(&stdout);
    casefile.stderr = Section::from_output(&stderr);
    casefile.exit_code = exit_code;
    if update {
        return fs::write(path, casefile.to_text())
            .map_err(|error| format!("cannot update: {error}"));
    }
    Err(format!(
        "expected:\n{text}\nactual:\n{}",
        casefile.to_text()
    ))
}

/// Writes the input files into a scratch directory and runs outln there.
fn run_outln(casefile: &Casefile, index: usize) -> Result<(Vec<u8>, Vec<u8>, u8), String> {
    let dir = ScratchDir::new(&format!("case-{index}")).map_err(|error| error.to_string())?;
    for (path, section) in &casefile.files {
        dir.write(path, &section.input_bytes()?)
            .map_err(|error| format!("cannot write {path}: {error}"))?;
    }
    let context = outln::Context {
        cwd: dir.path(),
        version: VERSION,
        global_gitignore: None,
    };
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let status = outln::run(
        casefile.args.arguments()?,
        &context,
        &mut stdout,
        &mut stderr,
    )
    .map_err(|error| format!("writing to a Vec failed: {error}"))?;
    Ok((stdout, stderr, status.code()))
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
