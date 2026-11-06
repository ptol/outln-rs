//! The language engines, one child module per language or family of languages sharing an engine (TypeScript and JavaScript); `language_engine.rs` registers them.

pub(crate) mod markdown;
pub(crate) mod rust;
pub(crate) mod script;
