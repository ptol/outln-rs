//! The language engines, one child module per language or family of languages sharing an engine (TypeScript and JavaScript; TOML, YAML, and JSON); `language_engine.rs` registers them.

pub(crate) mod css;
pub(crate) mod data_file;
pub(crate) mod markdown;
pub(crate) mod rust;
pub(crate) mod script;
