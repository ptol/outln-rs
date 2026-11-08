//! `InputError`: every way a path argument or file can fail, with the message printed on stderr.

use std::fmt;

use crate::language_engine::supported_extensions;

/// A failure tied to one input; the other inputs are still processed and printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InputError {
    FileMissing(String),
    DirectoryMissing(String),
    /// A FIFO, socket, or device given as an argument.
    NotRegularFile(String),
    /// A file argument with a trailing `/`.
    FileNotDirectory(String),
    /// Lists the supported extensions, so the reader learns what would have worked.
    UnsupportedFileType(String),
    NotUtf8Text(String),
    /// Its NUL bytes say UTF-16 or binary (`source_text::normalize`).
    LooksBinary(String),
    /// `detail` is the I/O error's kind, such as `permission denied`.
    Unreadable {
        path: String,
        detail: String,
    },
    /// A directory below a walked argument couldn't be listed; the other files still count.
    DirectoryUnreadable {
        directory: String,
        detail: String,
    },
    /// No argument of a glob view produced a supported file; holds the arguments.
    NoSupportedFiles(Vec<String>),
    /// Debug mode got several paths; holds how many.
    DebugNeedsOnePath(usize),
    /// Debug mode got a glob pattern; holds it.
    DebugGlob(String),
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileMissing(path) => write!(f, "File {path} does not exist"),
            Self::DirectoryMissing(path) => write!(f, "Directory {path} does not exist"),
            Self::NotRegularFile(path) => write!(
                f,
                "File {path} is not a regular file (a FIFO, socket, or device)"
            ),
            Self::FileNotDirectory(path) => {
                write!(f, "File {path} is not a directory; remove the trailing /")
            }
            Self::UnsupportedFileType(path) => {
                write!(f, "File {path} has an unsupported file type; supported: ")?;
                for (index, extension) in supported_extensions().enumerate() {
                    let separator = if index == 0 { "" } else { ", " };
                    write!(f, "{separator}.{extension}")?;
                }
                Ok(())
            }
            Self::NotUtf8Text(path) => write!(f, "File {path} is not UTF-8 text"),
            Self::LooksBinary(path) => write!(
                f,
                "File {path} looks like a binary or UTF-16 file, not UTF-8 text"
            ),
            Self::Unreadable { path, detail } => {
                write!(f, "File {path} could not be read: {detail}")
            }
            Self::DirectoryUnreadable { directory, detail } => {
                write!(f, "Directory {directory} could not be read: {detail}")
            }
            Self::NoSupportedFiles(arguments) => {
                write!(f, "No supported files matched: {}", arguments.join(" "))
            }
            Self::DebugNeedsOnePath(count) => write!(
                f,
                "--debug requires exactly one file or directory path, not {count}"
            ),
            Self::DebugGlob(pattern) => write!(
                f,
                "--debug requires a file or directory path, not the glob pattern {pattern}"
            ),
        }
    }
}
