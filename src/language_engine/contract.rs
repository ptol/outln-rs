//! The checks every registered language engine must pass: one engine per extension, an engine sample per engine that parses without syntax errors, and the output contract (well-formed outlines, file summaries, and debug mode) on any input: prefixes, edge inputs, and seeded mutations of the engine samples.
//!
//! These run over `ENGINES`, so a new engine is checked as soon as it's registered; its own
//! behaviour is tested in its module and its casefiles. `just fuzz` runs the mutations with
//! more inputs and another seed, and times adversarial inputs (docs/testing.md).

mod mutation;

use std::any::Any;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, Instant};

use super::{ENGINES, LanguageEngine};
use crate::HighlightStyle;
use crate::debug_mode::{MARKER_END, MARKER_START, highlight};
use crate::outline::{
    FileSummary, Header, Outline, header_lines, highlights, render_file_summary, render_outline,
};
use crate::source_file::load;
use crate::source_text::{ByteRange, LineCount, LineIndex, SourceText, normalize};
use crate::syntax_tree::parse;
use mutation::{Material, Random, mutate, shrink};

/// The engine samples, relative to the crate root: one per engine, named `sample.<extension>`
/// after the engine's first extension. `just build-static` outlines them too.
const ENGINE_SAMPLES: &str = "tests/engine-samples";

/// Inputs no engine's sample covers: nothing, whitespace only, markers that open a comment,
/// string, or block and never close it, multi-byte characters at the end (a BOM that doesn't
/// start the file survives normalization) and a key of them, line breaks written as CR, and
/// half-edited lines that once printed a multi-line or blank outline line (a TOML `[[bin`
/// header, a CSS rule without a selector).
const EDGE_INPUTS: &[&str] = &[
    "",
    "\n",
    " \t\n\n",
    "é",
    "#",
    "//",
    "/*",
    "/**/",
    "---\n",
    "{[(\"'`",
    "\u{2028}\u{feff}",
    "\u{2028}:\n-",
    "\r",
    "a\r\rb\r\n",
    "[[bin\n\nname = \"x\"\n[[bin]]\n",
    "{ color: red }\n@media print {\n  { color: black }\n}\n",
];

/// The length of the one-line input, long enough to show a per-character cost.
const LONG_LINE_BYTES: usize = 100_000;

/// How deep the deeply nested input nests, deeper than tree-sitter-yaml's scanner can save its
/// state for (254 levels; `data_file::yaml::parsable_part`).
const DEEP_NESTING_LEVELS: usize = 300;

/// A failing input longer than this is reported by its length instead of printed.
const MAX_PRINTED_INPUT_BYTES: usize = 2_000;

fn sample_name(engine: &dyn LanguageEngine) -> String {
    format!("sample.{}", first_extension(engine))
}

fn first_extension(engine: &dyn LanguageEngine) -> &'static str {
    engine
        .extensions()
        .first()
        .expect("every engine has an extension")
}

fn samples_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(ENGINE_SAMPLES)
}

/// The sample's text, loaded as file mode loads a file argument.
fn sample_text(engine: &dyn LanguageEngine) -> String {
    load(&samples_directory(), &sample_name(engine))
        .unwrap_or_else(|error| panic!("{error}"))
        .text
}

/// `text` as `source_file::load` hands it to an engine (BOM stripped, CR line breaks made LF),
/// or `None` if it wouldn't load as text.
fn normalized(text: &str) -> Option<String> {
    normalize(text.as_bytes().to_vec()).ok()
}

#[test]
fn every_extension_is_lowercase_and_belongs_to_one_engine() {
    let mut seen = BTreeSet::new();
    for engine in ENGINES {
        assert!(
            !engine.extensions().is_empty(),
            "an engine in ENGINES has no extensions"
        );
        for &extension in engine.extensions() {
            assert!(
                !extension.is_empty()
                    && !extension.contains('.')
                    && extension == extension.to_ascii_lowercase(),
                "extension {extension:?} must be lowercase and without the dot"
            );
            // Lowercase, so this also catches two engines differing only in case, which
            // `engine_for` would treat as one.
            assert!(
                seen.insert(extension),
                "extension {extension:?} is claimed by two engines; `engine_for` only ever picks the first"
            );
        }
    }
}

#[test]
fn every_engine_has_one_engine_sample_and_every_sample_an_engine() {
    let expected: BTreeSet<String> = ENGINES.iter().map(|&engine| sample_name(engine)).collect();
    let directory = samples_directory();
    let found: BTreeSet<String> = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        found, expected,
        "{ENGINE_SAMPLES} must hold exactly one `sample.<first extension>` per engine"
    );
}

#[test]
fn every_engine_sample_has_a_header_and_declarations() {
    for &engine in ENGINES {
        let name = sample_name(engine);
        let text = sample_text(engine);
        let outline = engine.outline(&SourceText::new(&text));
        assert!(
            outline.header.as_ref().is_some_and(Header::has_text),
            "{name} needs a header with text"
        );
        assert!(!outline.lines.is_empty(), "{name} needs declarations");
        assert!(
            !matches!(engine.file_summary(&text), FileSummary::Empty),
            "{name} needs a file summary"
        );
    }
}

/// A syntax error in a sample would hide its declarations behind error recovery, and it shows a
/// grammar that doesn't load or doesn't parse the language it's registered for.
#[test]
fn every_engine_sample_parses_without_syntax_errors() {
    for &engine in ENGINES {
        let Some(grammar) = engine.grammar() else {
            continue;
        };
        let tree = parse(&sample_text(engine), &grammar);
        assert!(
            !tree.root_node().has_error(),
            "{} has a syntax error: {}",
            sample_name(engine),
            tree.root_node().to_sexp()
        );
    }
}

/// Every prefix of each sample stands for a file cut off while being edited, which is when
/// agents outline files most and when syntax errors put byte ranges in odd places.
#[test]
fn every_prefix_of_every_engine_sample_keeps_the_output_contract() {
    for &engine in ENGINES {
        let text = sample_text(engine);
        let ends = text
            .char_indices()
            .map(|(index, _)| index)
            .chain([text.len()]);
        for end in ends {
            check_output(engine, &text[..end]);
        }
    }
}

#[test]
fn edge_inputs_keep_the_output_contract() {
    let long_line = "a".repeat(LONG_LINE_BYTES);
    let deep_nesting = "- ".repeat(DEEP_NESTING_LEVELS);
    let deep_frontmatter = format!("---\n{deep_nesting}\n---\n");
    let generated = [long_line, deep_nesting, deep_frontmatter];
    for &engine in ENGINES {
        for input in EDGE_INPUTS
            .iter()
            .copied()
            .chain(generated.iter().map(String::as_str))
        {
            let text = normalized(input).expect("edge inputs are text");
            check_output(engine, &text);
        }
    }
}

/// Fails, naming the engine, the input, and the broken rule, unless `text`'s output keeps the
/// contract ([`contract_violation`]).
fn check_output(engine: &dyn LanguageEngine, text: &str) {
    if let Some(violation) = contract_violation(engine, text) {
        panic!(
            "the engine of {} broke the output contract on {}: {violation}",
            sample_name(engine),
            printed_input(text)
        );
    }
}

/// The input as a failure message shows it: quoted with escapes, or by its length.
fn printed_input(text: &str) -> String {
    if text.len() <= MAX_PRINTED_INPUT_BYTES {
        format!("{text:?}")
    } else {
        format!("an input of {} bytes", text.len())
    }
}

/// Which rule of the output contract `text` breaks, an engine's own panic included, or `None`
/// when its output keeps every rule.
fn contract_violation(engine: &dyn LanguageEngine, text: &str) -> Option<String> {
    // The closure only reads `engine` and `text`, so nothing is left half-changed by a panic.
    match catch_unwind(AssertUnwindSafe(|| check_contract(engine, text))) {
        Ok(Ok(())) => None,
        Ok(Err(violation)) => Some(violation),
        Err(panic) => Some(format!("panicked: {}", panic_message(panic.as_ref()))),
    }
}

/// A panic's message: `panic!` with a format string carries a `String`, with a literal a
/// `&str`.
fn panic_message(panic: &(dyn Any + Send)) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|&text| text.to_owned()))
        .unwrap_or_else(|| "a panic without a message".to_owned())
}

/// `Ok` when `holds`, else the broken rule `describe` writes.
fn ensure(holds: bool, describe: impl FnOnce() -> String) -> Result<(), String> {
    if holds { Ok(()) } else { Err(describe()) }
}

/// The output contract: outlines `text` and summarizes it, then checks the outline's ranges and
/// lines, the file mode and glob view text rendered from them, how the file summary agrees with
/// the outline, and debug mode's highlighting. The first broken rule is the error.
fn check_contract(engine: &dyn LanguageEngine, text: &str) -> Result<(), String> {
    let source = SourceText::new(text);
    let outline = engine.outline(&source);
    check_byte_ranges(text, &outline)?;
    check_outline_lines(&outline, &source.line_index)?;
    check_rendered_outline(&outline, &source.line_index)?;
    let summary = engine.file_summary(text);
    check_summary_matches_outline(&summary, &outline)?;
    let summary_line = render_file_summary("sample", LineCount::of(text), &summary);
    check_one_line("the file summary", &summary_line)?;
    check_debug_mode(text, &outline)
}

/// Every byte range of the outline is a range of whole characters in the text, and each outline
/// line's highlight lies inside its range.
fn check_byte_ranges(text: &str, outline: &Outline) -> Result<(), String> {
    for range in output_ranges(outline) {
        ensure(text.get(range.as_range()).is_some(), || {
            format!(
                "{range:?} isn't a range of whole characters in the text's {} bytes",
                text.len()
            )
        })?;
    }
    for line in &outline.lines {
        let highlight = line.highlight;
        ensure(
            line.range.start <= highlight.start && highlight.end <= line.range.end,
            || {
                format!(
                    "highlight {highlight:?} of {:?} is outside its range {:?}",
                    line.text, line.range
                )
            },
        )?;
    }
    Ok(())
}

/// Every byte range of an outline: the header's, and each outline line's range and highlight.
fn output_ranges(outline: &Outline) -> Vec<ByteRange> {
    let mut ranges = highlights(outline);
    ranges.extend(outline.lines.iter().map(|line| line.range));
    ranges
}

/// Outline lines print on one line each, in source order (their ranges' starts never go back),
/// with line ranges whose first line comes before their last, and nested as a tree: the first
/// line is at the top level, each line at most one level deeper than the line before, and a
/// nested line's range inside the range of the line it's nested in.
fn check_outline_lines(outline: &Outline, line_index: &LineIndex) -> Result<(), String> {
    // The range of the latest line at each depth above the current one.
    let mut enclosing: Vec<ByteRange> = Vec::new();
    let mut previous_start = 0;
    for line in &outline.lines {
        let (text, range) = (&line.text, line.range);
        check_one_line("the outline line", text)?;
        let lines = line_index.line_range(range);
        ensure(lines.first <= lines.last, || {
            format!("{text:?} has the reversed line range {lines}")
        })?;
        ensure(range.start >= previous_start, || {
            format!("{text:?} at {range:?} comes before the outline line above it")
        })?;
        previous_start = range.start;
        ensure(line.depth <= enclosing.len(), || {
            format!(
                "{text:?} is at depth {}, deeper than one below the line above it",
                line.depth
            )
        })?;
        enclosing.truncate(line.depth);
        if let Some(&parent) = enclosing.last() {
            ensure(
                parent.start <= range.start && range.end <= parent.end,
                || format!("{text:?} at {range:?} is nested in {parent:?} but outside it"),
            )?;
        }
        enclosing.push(range);
    }
    Ok(())
}

/// The rendered file mode outline has exactly one line for its path and line count, one per
/// printed header line, and one per outline line, none of them blank.
fn check_rendered_outline(outline: &Outline, line_index: &LineIndex) -> Result<(), String> {
    let printed_header = outline
        .header
        .as_ref()
        .map(header_lines)
        .unwrap_or_default();
    for line in &printed_header {
        check_one_line("the header line", line)?;
    }
    let rendered = render_outline("sample", outline, line_index);
    let expected_lines = 1 + printed_header.len() + outline.lines.len();
    ensure(rendered.matches('\n').count() == expected_lines, || {
        format!("the outline should print {expected_lines} lines, but prints {rendered:?}")
    })?;
    rendered
        .lines()
        .try_for_each(|line| check_one_line("the printed line", line))
}

/// `line` is one non-blank line: no line break, CR included, and not only whitespace.
fn check_one_line(what: &str, line: &str) -> Result<(), String> {
    ensure(!line.contains(['\n', '\r']), || {
        format!("{what} {line:?} spans several lines")
    })?;
    ensure(!line.trim().is_empty(), || {
        format!("{what} {line:?} is blank")
    })
}

/// The glob view summarizes a file with what its outline shows, so the two modes never
/// disagree:
///
/// - A header that prints text is the summary, equal to the outline's header, ranges included;
///   a summary that is a header is the outline's header (a code file's marker-only comment
///   prints as no header in both).
/// - Without a header that prints text, the summary may be drawn from the outline's lines
///   instead: a Markdown file's first heading is its first outline line, and a data file's
///   field summary shows keys as their outline lines print them. A TOML field summary from a
///   table (`[package] {name = "outln"}`) shows that table's keys, which have no outline lines
///   of their own; then the table is an outline line. A code file's declaration summary is
///   drawn from the outline of only its first few KiB, so only its entries' shape is checked.
fn check_summary_matches_outline(summary: &FileSummary, outline: &Outline) -> Result<(), String> {
    let printed_header = outline.header.as_ref().filter(|header| header.has_text());
    let line_texts: Vec<&str> = outline
        .lines
        .iter()
        .map(|line| line.text.as_str())
        .collect();
    let no_printed_header = || {
        ensure(printed_header.is_none(), || {
            format!("the summary {summary:?} isn't the outline's header {printed_header:?}")
        })
    };
    match summary {
        FileSummary::Header(header) => ensure(outline.header.as_ref() == Some(header), || {
            format!(
                "the summary's header {header:?} isn't the outline's header {:?}",
                outline.header
            )
        }),
        FileSummary::Fields(fields) => {
            no_printed_header()?;
            let shown_lines: Vec<&str> = match &fields.table {
                Some(table) => vec![table.as_str()],
                None => fields.fields.iter().map(String::as_str).collect(),
            };
            ensure(
                shown_lines.iter().all(|shown| line_texts.contains(shown)),
                || {
                    format!(
                        "the field summary {fields:?} isn't made of the outline lines {line_texts:?}"
                    )
                },
            )
        }
        FileSummary::FirstHeading(heading) => {
            no_printed_header()?;
            ensure(line_texts.first() == Some(&heading.as_str()), || {
                format!(
                    "the first heading {heading:?} isn't the first outline line of {line_texts:?}"
                )
            })
        }
        // Drawn from the outline of only the file's first few KiB, so its entries needn't be
        // the whole file's outline lines; each must still be one line with text.
        FileSummary::Declarations(declarations) => {
            no_printed_header()?;
            ensure(
                declarations.declarations.iter().all(|declaration| {
                    declaration.count > 0
                        && !declaration.text.trim().is_empty()
                        && !declaration.text.contains(['\n', '\r'])
                }),
                || {
                    format!(
                        "the declaration summary {declarations:?} has an empty or multi-line entry"
                    )
                },
            )
        }
        FileSummary::Empty => no_printed_header(),
    }
}

/// Debug mode prints the text unchanged apart from its highlight markers.
fn check_debug_mode(text: &str, outline: &Outline) -> Result<(), String> {
    let marked = highlight(text, highlights(outline), HighlightStyle::Markers);
    let without_markers = |text: &str| text.replace(MARKER_START, "").replace(MARKER_END, "");
    ensure(without_markers(&marked) == without_markers(text), || {
        format!("debug mode changed the text beyond its markers: {marked:?}")
    })
}

/// How many mutated inputs per engine a plain test run checks: about half a second for all
/// engines in a debug build. `just fuzz` sets another count.
const DEFAULT_MUTATIONS_PER_ENGINE: usize = 200;

/// The seed of a plain test run, fixed so the run checks the same inputs every time.
const DEFAULT_SEED: u64 = 1;

/// What every engine's mutations may insert: brackets, quotes, comment markers, separators, and
/// the line breaks and characters that put byte ranges in odd places.
const SHARED_TOKENS: &[&str] = &[
    "{", "}", "(", ")", "[", "]", "<", ">", "\"", "'", "`", "/*", "*/", "//", "#", "---", ":", ",",
    ";", "=", "\\", "\n", "\n\n", "\r", "\r\n", "\t", " ", "é", "\u{feff}", "\u{2028}",
];

/// What an engine's mutations insert besides [`SHARED_TOKENS`]: its keywords and markers, by
/// its first extension. An engine missing here gets the shared tokens only.
fn language_tokens(engine: &dyn LanguageEngine) -> &'static [&'static str] {
    match first_extension(engine) {
        "md" => &[
            "# ",
            "## ",
            "###### ",
            "```",
            "~~~",
            "> ",
            "- ",
            "1. ",
            "===",
            "<!--",
            "-->",
            "    ",
            "description: ",
            "...",
        ],
        "ts" | "tsx" | "js" => &[
            "function ",
            "class ",
            "export ",
            "default ",
            "const ",
            "let ",
            "async ",
            "=>",
            "${",
            "<div>",
            "</div>",
            "/>",
            "@decorator\n",
            "interface ",
            "namespace ",
            "declare ",
            "describe('a', () => {",
            "'use strict';\n",
            "#!",
            "/** */",
        ],
        "rs" => &[
            "fn ",
            "impl ",
            "trait ",
            "mod ",
            "pub ",
            "struct ",
            "where ",
            "#[test]\n",
            "#![",
            "//!",
            "///",
            "/*!",
            "r#\"",
            "'a ",
            "macro_rules! ",
            "extern ",
            "#!",
        ],
        "css" => &[
            "@media ",
            "@import ",
            "@layer ",
            "@supports ",
            ".a ",
            "#b ",
            "::",
            "&",
            "!important",
            "url(",
        ],
        "json" => &["\"k\": ", "null", "true", "1e9", "[]", "{}"],
        "toml" => &[
            "[[",
            "]]",
            " = ",
            "\"\"\"",
            "'''",
            "#:schema ",
            "[a.b]\n",
            "[[bin]]\n",
            "a.b = 1\n",
        ],
        "yaml" => &[
            "- ",
            "? ",
            "| ",
            ">-",
            "&anchor ",
            "*anchor",
            "...\n",
            "%YAML 1.2\n",
            "!!str ",
            "key: ",
            "  ",
        ],
        _ => &[],
    }
}

/// How many mutated inputs per engine to check, and the seed they're made from.
struct FuzzSettings {
    mutations_per_engine: usize,
    seed: u64,
}

impl FuzzSettings {
    /// The settings of a plain test run, or those `just fuzz` passes in `FUZZ_ITERATIONS` and
    /// `FUZZ_SEED`.
    fn from_environment() -> Self {
        Self {
            mutations_per_engine: number_from_environment(
                "FUZZ_ITERATIONS",
                DEFAULT_MUTATIONS_PER_ENGINE,
            ),
            seed: number_from_environment("FUZZ_SEED", DEFAULT_SEED),
        }
    }
}

/// The number in the environment variable `name`, or `default` when it's unset or empty.
#[expect(
    clippy::disallowed_methods,
    reason = "`just fuzz` passes its iteration count and seed as environment variables, like the casefile runner's switches"
)]
fn number_from_environment<T: FromStr>(name: &str, default: T) -> T {
    let Some(value) = std::env::var(name).ok().filter(|value| !value.is_empty()) else {
        return default;
    };
    value
        .parse()
        .unwrap_or_else(|_| panic!("{name}={value:?} must be a whole number"))
}

/// Writes a progress or result line into the test output, which `just fuzz` shows.
#[expect(
    clippy::print_stderr,
    reason = "`just fuzz` reports its seed and timings in the test output"
)]
fn report(line: &str) {
    eprintln!("{line}");
}

/// Seeded mutations of the engine samples stand for the half-edited files agents outline:
/// deleted and pasted spans, stray brackets and comment markers, cut-off ends. Each engine's
/// first failing input is shrunk and reported with the seed that reproduces it.
#[test]
fn mutated_engine_samples_keep_the_output_contract() {
    let settings = FuzzSettings::from_environment();
    let samples: Vec<String> = ENGINES.iter().map(|&engine| sample_text(engine)).collect();
    let failures: Vec<String> = ENGINES
        .iter()
        .zip(&samples)
        .enumerate()
        .filter_map(|(stream, (&engine, sample))| {
            first_mutation_failure(engine, stream, sample, &samples, &settings)
        })
        .collect();
    report(&format!(
        "checked {} mutated inputs per engine with seed {}",
        settings.mutations_per_engine, settings.seed
    ));
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The report of the first mutated input of `sample` (engine number `stream`) that breaks the
/// contract, shrunk, or `None` if every one keeps it.
fn first_mutation_failure(
    engine: &dyn LanguageEngine,
    stream: usize,
    sample: &str,
    samples: &[String],
    settings: &FuzzSettings,
) -> Option<String> {
    let tokens: Vec<&str> = SHARED_TOKENS
        .iter()
        .chain(language_tokens(engine))
        .copied()
        .collect();
    let material = Material {
        tokens: &tokens,
        others: samples,
    };
    let breaks_contract = |text: &str| {
        normalized(text).is_some_and(|text| contract_violation(engine, &text).is_some())
    };
    (0..settings.mutations_per_engine).find_map(|index| {
        let mut text = sample.to_owned();
        mutate(
            &mut text,
            &material,
            &mut Random::for_input(settings.seed, stream, index),
        );
        let text = normalized(&text)?;
        let violation = contract_violation(engine, &text)?;
        let shrunk = normalized(&shrink(&text, breaks_contract)).unwrap_or_default();
        let shrunk_violation = contract_violation(engine, &shrunk).unwrap_or_default();
        Some(format!(
            "the engine of {} broke the output contract on mutated input {index} of seed {} \
             (reproduce with `just fuzz {} {}`): {violation}\n\
             input: {}\n\
             shrunk to {}: {shrunk_violation}",
            sample_name(engine),
            settings.seed,
            index + 1,
            settings.seed,
            printed_input(&text),
            printed_input(&shrunk),
        ))
    })
}

/// Adversarial inputs: one unit repeated, which error recovery may handle in superlinear time
/// (a run of unclosed `{` grows 12 to 16 times slower for 4 times the input in the
/// tree-sitter code engines).
const ADVERSARIAL_UNITS: &[&str] = &[
    "{", "(", "[", "\"", "'", "/*", "<a>", "a {", "- ", "> ", "a:\n ", "# a\n", "[a]\n", "*",
];

/// How often the unit repeats in the smaller input.
const ADVERSARIAL_REPEATS: usize = 1_000;

/// How many times larger the larger input is.
const GROWTH: usize = 4;

/// How many times slower the larger input may be: linear time is 4 and quadratic 16, so only a
/// cubic or worse cost fails, which timing noise can't fake.
const MAX_SLOWDOWN: f64 = 40.0;

/// How often each input is timed; the fastest run counts, which filters out a busy machine.
const TIMING_RUNS: usize = 3;

/// For each engine and adversarial unit, times outlining and summarizing the unit repeated
/// `n` and `4n` times, reports the ratio, and fails on an extreme one. Timing depends on the
/// machine, so only `just fuzz` runs it.
#[test]
#[ignore = "measures time, so only `just fuzz` runs it"]
fn adversarial_inputs_take_close_to_linear_time() {
    let mut too_slow = Vec::new();
    for &engine in ENGINES {
        let mut ratios = String::new();
        for unit in ADVERSARIAL_UNITS {
            let small = fastest_run(engine, &unit.repeat(ADVERSARIAL_REPEATS));
            let large = fastest_run(engine, &unit.repeat(ADVERSARIAL_REPEATS * GROWTH));
            let ratio = large.as_secs_f64() / small.as_secs_f64().max(f64::MIN_POSITIVE);
            let _ = write!(ratios, " {unit:?} {ratio:.1}x ({large:.1?})");
            if ratio > MAX_SLOWDOWN {
                too_slow.push(format!("{} on {unit:?}: {ratio:.1}x", sample_name(engine)));
            }
        }
        report(&format!(
            "{}: time for {GROWTH}x the input:{ratios}",
            sample_name(engine)
        ));
    }
    assert!(
        too_slow.is_empty(),
        "more than {MAX_SLOWDOWN}x slower for {GROWTH}x the input: {}",
        too_slow.join(", ")
    );
}

/// The fastest of [`TIMING_RUNS`] runs of outlining and summarizing `text`.
#[expect(
    clippy::disallowed_methods,
    reason = "a timing check reads the clock; only `just fuzz` runs it"
)]
fn fastest_run(engine: &dyn LanguageEngine, text: &str) -> Duration {
    (0..TIMING_RUNS)
        .map(|_| {
            let start = Instant::now();
            engine.outline(&SourceText::new(text));
            engine.file_summary(text);
            start.elapsed()
        })
        .min()
        .unwrap_or_default()
}
