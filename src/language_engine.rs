//! The `LanguageEngine` trait every language implements, and the registry that picks one by file extension.

use std::path::Path;

use crate::languages::markdown::Markdown;
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

/// Every engine, in lookup order. Immutable, so it holds no state; add new engines here.
static ENGINES: &[&dyn LanguageEngine] = &[&Markdown];

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
        assert!(engine_for("notes.txt").is_none());
        assert!(engine_for("Makefile").is_none());
    }
}
