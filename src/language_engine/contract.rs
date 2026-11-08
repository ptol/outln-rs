//! The checks every registered language engine must pass: one engine per extension, an engine sample per engine that parses without syntax errors, and output that stays inside the text on any input, half-edited files included.
//!
//! These run over `ENGINES`, so a new engine is checked as soon as it's registered; its own
//! behaviour is tested in its module and its casefiles.

use std::collections::BTreeSet;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use super::{ENGINES, LanguageEngine};
use crate::debug_mode::highlight;
use crate::outline::{
    FileSummary, Header, Outline, highlights, render_file_summary, render_outline,
};
use crate::source_file::load;
use crate::source_text::{ByteRange, LineCount, SourceText};
use crate::syntax_tree::parse;

/// The engine samples, relative to the crate root: one per engine, named `sample.<extension>`
/// after the engine's first extension. `just build-static` outlines them too.
const ENGINE_SAMPLES: &str = "tests/engine-samples";

/// Inputs no engine's sample covers: nothing, whitespace only, markers that open a comment,
/// string, or block and never close it, and multi-byte characters at the end (a BOM that
/// doesn't start the file survives normalization).
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
];

/// The length of the one-line input, long enough to show a per-character cost.
const LONG_LINE_BYTES: usize = 100_000;

/// A failing input longer than this is reported by its length instead of printed.
const MAX_PRINTED_INPUT_BYTES: usize = 2_000;

fn sample_name(engine: &dyn LanguageEngine) -> String {
    let extension = engine
        .extensions()
        .first()
        .expect("every engine has an extension");
    format!("sample.{extension}")
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
fn output_stays_inside_the_text_for_every_prefix_of_every_engine_sample() {
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
fn output_stays_inside_the_text_for_edge_inputs() {
    let long_line = "a".repeat(LONG_LINE_BYTES);
    for &engine in ENGINES {
        for input in EDGE_INPUTS.iter().copied().chain([long_line.as_str()]) {
            check_output(engine, input);
        }
    }
}

/// Runs [`check_ranges_and_render`], and on any panic, an engine's own included, fails again
/// naming the engine and the input, which the first panic message doesn't.
fn check_output(engine: &dyn LanguageEngine, text: &str) {
    // The closure only reads `engine` and `text`, so nothing is left half-changed by a panic.
    let checked = catch_unwind(AssertUnwindSafe(|| check_ranges_and_render(engine, text)));
    if checked.is_err() {
        let input = if text.len() <= MAX_PRINTED_INPUT_BYTES {
            format!("{text:?}")
        } else {
            format!("an input of {} bytes", text.len())
        };
        panic!("the engine of {} failed on {input}", sample_name(engine));
    }
}

/// Outlines and summarizes `text`, checks every byte range in the outline, then renders it as
/// file mode, debug mode, and the glob view do.
fn check_ranges_and_render(engine: &dyn LanguageEngine, text: &str) {
    let source = SourceText::new(text);
    let outline = engine.outline(&source);
    for range in output_ranges(&outline) {
        assert!(
            text.get(range.as_range()).is_some(),
            "{range:?} isn't a range of whole characters in the text's {} bytes",
            text.len()
        );
    }
    for line in &outline.lines {
        let line_highlight = line.highlight;
        assert!(
            line.range.start <= line_highlight.start && line_highlight.end <= line.range.end,
            "highlight {line_highlight:?} of {:?} is outside its range {:?}",
            line.text,
            line.range
        );
    }
    render_outline("sample", &outline, &source.line_index);
    highlight(text, highlights(&outline));
    render_file_summary("sample", LineCount::of(text), &engine.file_summary(text));
}

/// Every byte range of an outline: the header's, and each outline line's range and highlight.
fn output_ranges(outline: &Outline) -> Vec<ByteRange> {
    let mut ranges = highlights(outline);
    ranges.extend(outline.lines.iter().map(|line| line.range));
    ranges
}
