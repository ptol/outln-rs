//! The `LanguageEngine` trait every language implements, and the registry that picks one by file extension.

use std::path::Path;

#[cfg(test)]
use tree_sitter::Language;

use crate::languages::css::CSS;
use crate::languages::data_file::{JSON, TOML, YAML};
use crate::languages::markdown::Markdown;
use crate::languages::rust::RUST;
use crate::languages::script::{JAVASCRIPT, TSX, TYPESCRIPT};
use crate::outline::{FileSummary, Outline};
use crate::source_text::SourceText;

/// The component that produces outlines and file summaries for one language.
///
/// Engines receive normalized text (see `source_text::normalize`), with its line index for
/// outlines, and return data with byte ranges; shared code renders it for every mode. `Sync`
/// lets the registry be a `static`.
pub(crate) trait LanguageEngine: Sync {
    /// File extensions this engine handles, lowercase and without the dot.
    fn extensions(&self) -> &'static [&'static str];

    /// The full outline, for file mode and debug mode.
    fn outline(&self, source: &SourceText<'_>) -> Outline;

    /// What the glob view prints for the file: its header, or a field summary or first heading
    /// without one; cheaper than `outline`, because it parses little or nothing.
    fn file_summary(&self, text: &str) -> FileSummary;

    /// The tree-sitter grammar the engine parses with, if any, so the contract tests can check
    /// that its engine sample parses without syntax errors.
    #[cfg(test)]
    fn grammar(&self) -> Option<Language> {
        None
    }
}

/// The outline of `text`, for tests that don't build a [`SourceText`] themselves.
#[cfg(test)]
pub(crate) fn outline_of(engine: &dyn LanguageEngine, text: &str) -> Outline {
    engine.outline(&SourceText::new(text))
}

/// The printed text of each outline line of `text`.
#[cfg(test)]
pub(crate) fn outline_texts(engine: &dyn LanguageEngine, text: &str) -> Vec<String> {
    outline_of(engine, text)
        .lines
        .into_iter()
        .map(|line| line.text)
        .collect()
}

/// Every engine, in lookup order, one entry per grammar. Engines are immutable configuration, so
/// they hold no mutable state; add new engines here.
static ENGINES: &[&dyn LanguageEngine] = &[
    &Markdown,
    &TYPESCRIPT,
    &TSX,
    &JAVASCRIPT,
    &RUST,
    &CSS,
    &JSON,
    &TOML,
    &YAML,
];

/// The engine for a path, matched by extension ignoring ASCII case, or `None` if unsupported.
pub(crate) fn engine_for(path: &str) -> Option<&'static dyn LanguageEngine> {
    let extension = Path::new(path).extension()?.to_str()?;
    ENGINES.iter().copied().find(|engine| {
        engine
            .extensions()
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(extension))
    })
}

#[cfg(test)]
mod contract;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_for_matches_extensions_ignoring_case() {
        assert!(engine_for("docs/guide.md").is_some());
        assert!(engine_for("README.MD").is_some());
        for code in [
            "a.ts",
            "a.d.ts",
            "a.mts",
            "a.cts",
            "a.tsx",
            "a.js",
            "a.jsx",
            "a.mjs",
            "a.CJS",
            "a.rs",
            "a.CSS",
            "a.json",
            "Cargo.toml",
            "a.yaml",
            "a.YML",
        ] {
            assert!(engine_for(code).is_some(), "{code}");
        }
        assert!(engine_for("notes.txt").is_none());
        assert!(engine_for("Makefile").is_none());
    }
}
