//! The casefile format of `tests/cases/**/*.case`: parsing, decoding sections to bytes, and writing in canonical form.
//!
//! A casefile is free-form description lines, then sections, each opened by a marker line:
//!
//! ```text
//! Description of the behaviour under test.
//! == args
//! input/guide.md
//! == file input/guide.md
//! # Guide
//! == stdout
//! input/guide.md (1 line)
//! [L1-L1] # Guide
//! == stderr
//! == exit 0
//! ```
//!
//! - `== args` comes first, with one argument per line; then any number of `== file <path>`
//!   sections; then `== stdout`, `== stderr`, and `== exit <code>`, in that order.
//! - A section's content is its lines, each with its newline, so a plain section always ends with
//!   a newline (or is empty).
//! - In plain `== stdout` and `== stderr` sections, the text `\x1b` stands for the ESC byte of
//!   ANSI colours; everything else is literal.
//! - A section marked `escaped` (`== file input/crlf.md escaped`) decodes `\\`, `\n`, `\r`, `\t`,
//!   and `\xNN` (one byte), and a `\` at the end of a line removes that line's newline. Use it
//!   for CRLF, NUL or invalid UTF-8 bytes, a missing final newline, or a content line that starts
//!   with `== ` (write it as `\x3d= `). In `== args`, each line is decoded on its own.

use std::fmt::Write as _;

/// Opens every section; no content line of a plain section may start with it.
const MARKER: &str = "== ";
const ESCAPED_FLAG: &str = " escaped";
/// How plain output sections write the ESC byte of ANSI colour codes.
const ESC_TEXT: &str = "\\x1b";

/// What [`Casefile::parse`] does with a casefile whose output sections at the end are missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MissingOutput {
    /// A missing section is an error: the casefile expects no output it doesn't state.
    Reject,
    /// Missing sections start empty, with exit code 0, so update mode can fill them in.
    AddEmpty,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Casefile {
    /// The lines before the first section, kept verbatim.
    pub(crate) description: String,
    pub(crate) args: Section,
    pub(crate) files: Vec<(String, Section)>,
    pub(crate) stdout: Section,
    pub(crate) stderr: Section,
    pub(crate) exit_code: u8,
}

/// A section's lines as written in the casefile, each ending with a newline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Section {
    escaped: bool,
    body: String,
}

impl Section {
    fn empty() -> Self {
        Self {
            escaped: false,
            body: String::new(),
        }
    }

    /// The section's lines as written in the casefile, escapes undecoded.
    pub(crate) fn as_written(&self) -> &str {
        &self.body
    }

    /// The bytes of an input file.
    pub(crate) fn input_bytes(&self) -> Result<Vec<u8>, String> {
        if self.escaped {
            decode_escapes(&self.body)
        } else {
            Ok(self.body.clone().into_bytes())
        }
    }

    /// The bytes of expected stdout or stderr.
    pub(crate) fn output_bytes(&self) -> Result<Vec<u8>, String> {
        if self.escaped {
            decode_escapes(&self.body)
        } else {
            Ok(self.body.replace(ESC_TEXT, "\x1b").into_bytes())
        }
    }

    /// One argument per line.
    pub(crate) fn arguments(&self) -> Result<Vec<String>, String> {
        self.body
            .lines()
            .map(|line| {
                if !self.escaped {
                    return Ok(line.to_owned());
                }
                let bytes = decode_escapes(line)?;
                String::from_utf8(bytes).map_err(|_| format!("argument {line:?} is not UTF-8"))
            })
            .collect()
    }

    /// The section that expects exactly `bytes` of output: plain when the bytes allow it.
    pub(crate) fn from_output(bytes: &[u8]) -> Self {
        match std::str::from_utf8(bytes) {
            Ok(text) if can_be_plain_output(text) => Self {
                escaped: false,
                body: text.replace('\x1b', ESC_TEXT),
            },
            _ => Self {
                escaped: true,
                body: encode_escapes(bytes),
            },
        }
    }
}

impl Casefile {
    pub(crate) fn parse(text: &str, missing_output: MissingOutput) -> Result<Self, String> {
        let mut description = String::new();
        let mut sections: Vec<(Marker, String)> = Vec::new();
        for line in text.split_inclusive('\n') {
            if let Some(marker) = line.strip_prefix(MARKER) {
                sections.push((Marker::parse(marker.trim_end_matches('\n'))?, String::new()));
                continue;
            }
            let target = match sections.last_mut() {
                Some((_, body)) => body,
                None => &mut description,
            };
            target.push_str(line);
            if !line.ends_with('\n') {
                target.push('\n');
            }
        }
        Self::from_sections(description, sections, missing_output)
    }

    /// Checks for a description, then the canonical section order: args, files, stdout, stderr,
    /// exit.
    fn from_sections(
        description: String,
        sections: Vec<(Marker, String)>,
        missing_output: MissingOutput,
    ) -> Result<Self, String> {
        if description.trim().is_empty() {
            return Err(
                "the casefile must start with a description, before `== args`: what \
                 behaviour is under test and why the expected output looks this way \
                 (docs/testing.md)"
                    .to_owned(),
            );
        }
        let add_empty = missing_output == MissingOutput::AddEmpty;
        let mut sections = sections.into_iter().peekable();
        let args = match sections.next() {
            Some((Marker::Args { escaped }, body)) => Section { escaped, body },
            _ => return Err("the first section must be `== args`".to_owned()),
        };
        let mut files = Vec::new();
        while let Some((Marker::File { .. }, _)) = sections.peek() {
            if let Some((Marker::File { path, escaped }, body)) = sections.next() {
                files.push((path, Section { escaped, body }));
            }
        }
        let stdout = match sections.next() {
            Some((Marker::Stdout { escaped }, body)) => Section { escaped, body },
            None if add_empty => Section::empty(),
            _ => return Err("`== file` sections must be followed by `== stdout`".to_owned()),
        };
        let stderr = match sections.next() {
            Some((Marker::Stderr { escaped }, body)) => Section { escaped, body },
            None if add_empty => Section::empty(),
            _ => return Err("`== stdout` must be followed by `== stderr`".to_owned()),
        };
        let exit_code = match sections.next() {
            Some((Marker::Exit(code), body)) if body.is_empty() => code,
            None if add_empty => 0,
            _ => {
                return Err(
                    "`== stderr` must be followed by `== exit <code>`, the last line".to_owned(),
                );
            }
        };
        if sections.next().is_some() {
            return Err("nothing may follow `== exit <code>`".to_owned());
        }
        Ok(Self {
            description,
            args,
            files,
            stdout,
            stderr,
            exit_code,
        })
    }

    /// The casefile text in canonical form; `parse` then `to_text` returns a canonical file unchanged.
    pub(crate) fn to_text(&self) -> String {
        let mut text = self.description.clone();
        write_section(&mut text, "args", &self.args);
        for (path, section) in &self.files {
            write_section(&mut text, &format!("file {path}"), section);
        }
        write_section(&mut text, "stdout", &self.stdout);
        write_section(&mut text, "stderr", &self.stderr);
        // Writing into a `String` cannot fail.
        let _ = writeln!(text, "{MARKER}exit {}", self.exit_code);
        text
    }
}

fn write_section(text: &mut String, name: &str, section: &Section) {
    text.push_str(MARKER);
    text.push_str(name);
    if section.escaped {
        text.push_str(ESCAPED_FLAG);
    }
    text.push('\n');
    text.push_str(&section.body);
}

/// A section's marker line, without the leading `== `.
enum Marker {
    Args { escaped: bool },
    File { path: String, escaped: bool },
    Stdout { escaped: bool },
    Stderr { escaped: bool },
    Exit(u8),
}

impl Marker {
    fn parse(text: &str) -> Result<Self, String> {
        let (name, escaped) = match text.strip_suffix(ESCAPED_FLAG) {
            Some(name) => (name, true),
            None => (text, false),
        };
        match name {
            "args" => return Ok(Self::Args { escaped }),
            "stdout" => return Ok(Self::Stdout { escaped }),
            "stderr" => return Ok(Self::Stderr { escaped }),
            _ => {}
        }
        if let Some(path) = name.strip_prefix("file ").filter(|path| !path.is_empty()) {
            return Ok(Self::File {
                path: path.to_owned(),
                escaped,
            });
        }
        if let Some(code) = name.strip_prefix("exit ") {
            if escaped {
                return Err(format!("`== exit {code}` can't be escaped"));
            }
            return code
                .parse()
                .map(Self::Exit)
                .map_err(|_| format!("`== exit {code}` needs an exit code from 0 to 255"));
        }
        Err(format!(
            "unknown section `{MARKER}{text}`; a content line starting with `{MARKER}` must be \
             written in an escaped section as `\\x3d= `"
        ))
    }
}

/// Plain output sections can hold UTF-8 text that ends with a newline (or is empty), has no
/// control characters besides newline, tab, and ESC, has no literal `\x1b`, and no marker lines.
fn can_be_plain_output(text: &str) -> bool {
    (text.is_empty() || text.ends_with('\n'))
        && !text.contains(ESC_TEXT)
        && text
            .chars()
            .all(|character| matches!(character, '\n' | '\t' | '\x1b') || !character.is_control())
        && !text.lines().any(|line| line.starts_with(MARKER))
}

fn decode_escapes(body: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(body.len());
    let mut rest = body.bytes();
    while let Some(byte) = rest.next() {
        if byte != b'\\' {
            bytes.push(byte);
            continue;
        }
        match rest.next() {
            Some(b'\\') => bytes.push(b'\\'),
            Some(b'n') => bytes.push(b'\n'),
            Some(b'r') => bytes.push(b'\r'),
            Some(b't') => bytes.push(b'\t'),
            // A line continuation: the newline isn't content.
            Some(b'\n') => {}
            Some(b'x') => {
                let digits = [rest.next(), rest.next()];
                let [Some(high), Some(low)] = digits else {
                    return Err("`\\x` needs two hex digits".to_owned());
                };
                let hex = std::str::from_utf8(&[high, low])
                    .map_err(|_| "`\\x` needs two hex digits")?
                    .to_owned();
                bytes.push(
                    u8::from_str_radix(&hex, 16)
                        .map_err(|_| format!("`\\x{hex}` isn't a hex byte"))?,
                );
            }
            other => {
                let escape = other.map_or(String::new(), |byte| char::from(byte).to_string());
                return Err(format!("unknown escape `\\{escape}` in an escaped section"));
            }
        }
    }
    Ok(bytes)
}

fn encode_escapes(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        let valid = chunk.valid();
        for (index, character) in valid.char_indices() {
            let at_line_start = text.is_empty() || text.ends_with('\n');
            match character {
                '\\' => text.push_str("\\\\"),
                '\r' => text.push_str("\\r"),
                '\t' => text.push_str("\\t"),
                '=' if at_line_start && valid[index..].starts_with(MARKER) => {
                    text.push_str("\\x3d");
                }
                '\n' => text.push('\n'),
                control if control.is_control() => {
                    let mut buffer = [0; 4];
                    for byte in control.encode_utf8(&mut buffer).bytes() {
                        let _ = write!(text, "\\x{byte:02x}");
                    }
                }
                other => text.push(other),
            }
        }
        for byte in chunk.invalid() {
            let _ = write!(text, "\\x{byte:02x}");
        }
    }
    if !text.is_empty() && !text.ends_with('\n') {
        // Ends the last line without making its newline part of the content.
        text.push_str("\\\n");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANONICAL: &str = "Checks a heading.\n== args\ninput/a.md\n== file input/a.md\n# A\n== file input/b.md escaped\nno newline\\\n== stdout\n\\x1b[32m# A\\x1b[0m\n== stderr\n== exit 0\n";

    #[test]
    fn canonical_text_round_trips() {
        let casefile = Casefile::parse(CANONICAL, MissingOutput::Reject).unwrap();
        assert_eq!(casefile.to_text(), CANONICAL);
        assert_eq!(casefile.args.arguments().unwrap(), ["input/a.md"]);
        assert_eq!(casefile.files[1].1.input_bytes().unwrap(), b"no newline");
        assert_eq!(
            casefile.stdout.output_bytes().unwrap(),
            b"\x1b[32m# A\x1b[0m\n"
        );
        assert_eq!(casefile.stderr.output_bytes().unwrap(), b"");
    }

    #[test]
    fn sections_out_of_order_are_rejected() {
        let error = Casefile::parse(
            "Checks the order.\n== stdout\n== args\n== stderr\n== exit 0\n",
            MissingOutput::Reject,
        )
        .unwrap_err();
        assert!(error.contains("first section"), "{error}");
    }

    #[test]
    fn a_casefile_without_a_description_is_rejected() {
        for text in [
            "== args\n== stdout\n== stderr\n== exit 0\n",
            "\n  \n== args\n",
        ] {
            let error = Casefile::parse(text, MissingOutput::AddEmpty).unwrap_err();
            assert!(error.contains("description"), "{error}");
        }
    }

    #[test]
    fn missing_output_sections_are_rejected_or_added_empty() {
        let text = "Checks a heading.\n== args\ninput/a.md\n== file input/a.md\n# A\n";
        let error = Casefile::parse(text, MissingOutput::Reject).unwrap_err();
        assert!(error.contains("`== stdout`"), "{error}");

        let casefile = Casefile::parse(text, MissingOutput::AddEmpty).unwrap();
        assert_eq!(
            casefile.to_text(),
            format!("{text}== stdout\n== stderr\n== exit 0\n")
        );
    }

    #[test]
    fn escapes_decode_to_bytes() {
        let section = Section {
            escaped: true,
            body: "a\\r\\t\\\\\\x00\\xff\n\\x3d= b\\\n".to_owned(),
        };
        assert_eq!(section.input_bytes().unwrap(), b"a\r\t\\\x00\xff\n== b");
    }

    #[test]
    fn output_needing_escapes_survives_a_round_trip() {
        let outputs: [&[u8]; 6] = [
            b"plain\n\x1b[32mgreen\x1b[0m\n",
            b"no final newline",
            b"crlf\r\n",
            b"\xff\xfe invalid\n",
            b"== looks like a marker\n",
            b"literal \\x1b text\n",
        ];
        for output in outputs {
            let section = Section::from_output(output);
            assert_eq!(section.output_bytes().unwrap(), output, "{section:?}");
        }
        assert!(!Section::from_output(b"plain\n").escaped);
    }
}
