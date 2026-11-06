//! The `LanguageEngine` trait every language implements, and the registry that picks one by file extension.

use std::path::Path;

use crate::languages::markdown::Markdown;
use crate::languages::script::{JAVASCRIPT, TSX, TYPESCRIPT};
use crate::outline::{FileSummary, Outline};

/// The component that produces outlines and file summaries for one language.
///
/// Engines receive normalized text (see `source_text::normalize`) and return data with byte
/// ranges; shared code renders it for every mode. `Sync` lets the registry be a `static`.
pub(crate) trait LanguageEngine: Sync {
    /// File extensions this engine handles, lowercase and without the dot.
    fn extensions(&self) -> &'static [&'static str];

    /// The full outline, for file mode and debug mode.
    fn outline(&self, text: &str) -> Outline;

    /// Only the header, for glob view; cheaper than `outline` because it can stop parsing early.
    fn file_summary(&self, text: &str) -> FileSummary;
}

/// Every engine, in lookup order, one entry per grammar. Engines are immutable configuration, so
/// they hold no mutable state; add new engines here.
static ENGINES: &[&dyn LanguageEngine] = &[&Markdown, &TYPESCRIPT, &TSX, &JAVASCRIPT];

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
mod tests {
    use super::*;

    #[test]
    fn engine_for_matches_extensions_ignoring_case() {
        assert!(engine_for("docs/guide.md").is_some());
        assert!(engine_for("README.MD").is_some());
        for script in [
            "a.ts", "a.d.ts", "a.mts", "a.cts", "a.tsx", "a.js", "a.jsx", "a.mjs", "a.CJS",
        ] {
            assert!(engine_for(script).is_some(), "{script}");
        }
        assert!(engine_for("notes.txt").is_none());
        assert!(engine_for("Makefile").is_none());
    }
}
