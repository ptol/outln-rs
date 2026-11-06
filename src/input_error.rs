//! `InputError`: every way a path argument or file can fail, with the message printed on stderr.

use std::fmt;

/// A failure tied to one input; the other inputs are still processed and printed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InputError {
    FileMissing(String),
    DirectoryMissing(String),
    UnsupportedFileType(String),
    NotUtf8Text(String),
    Unreadable(String),
    /// A directory below a walked argument couldn't be listed; the other files still count.
    DirectoryUnreadable {
        directory: String,
        detail: String,
    },
    /// No argument of a glob view produced a supported file; holds the arguments.
    NoSupportedFiles(Vec<String>),
    /// Debug mode got several paths or a glob pattern.
    DebugNeedsOnePath,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileMissing(path) => write!(f, "File {path} does not exist"),
            Self::DirectoryMissing(path) => write!(f, "Directory {path} does not exist"),
            Self::UnsupportedFileType(path) => write!(f, "FILE {path} HAS UNSUPPORTED FILE TYPE"),
            Self::NotUtf8Text(path) => write!(f, "FILE {path} IS NOT UTF-8 TEXT"),
            Self::Unreadable(path) => write!(f, "FILE {path} COULD NOT BE READ"),
            Self::DirectoryUnreadable { directory, detail } => {
                write!(f, "Directory {directory} could not be read: {detail}")
            }
            Self::NoSupportedFiles(arguments) => {
                write!(f, "No supported files matched: {}", arguments.join(" "))
            }
            Self::DebugNeedsOnePath => write!(f, "--debug requires exactly one input file path."),
        }
    }
}
