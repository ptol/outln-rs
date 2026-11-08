//! Loads one file argument: picks its language engine, then reads and normalizes its text.

use std::fs;
use std::path::Path;

use crate::input_error::InputError;
use crate::language_engine::{LanguageEngine, engine_for};
use crate::source_text::normalize;

/// A supported file's normalized text and the engine that outlines it.
pub(crate) struct SourceFile {
    pub(crate) engine: &'static dyn LanguageEngine,
    pub(crate) text: String,
}

/// Loads `path`, resolved against `cwd`. The engine is checked first, so an unsupported file
/// is never read.
pub(crate) fn load(cwd: &Path, path: &str) -> Result<SourceFile, InputError> {
    let engine =
        engine_for(path).ok_or_else(|| InputError::UnsupportedFileType(path.to_owned()))?;
    let bytes = fs::read(cwd.join(path)).map_err(|error| InputError::Unreadable {
        path: path.to_owned(),
        detail: error.kind().to_string(),
    })?;
    let text = normalize(bytes).map_err(|_| InputError::NotUtf8Text(path.to_owned()))?;
    Ok(SourceFile { engine, text })
}
