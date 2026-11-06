//! Repository conventions that keep `outln` output and the tooling healthy, checked mechanically.
//!
//! - Every `.rs` file starts with a `//!` header comment saying what the file is for.
//! - No `.rs` file silences lints with an inner `allow` attribute at its top, which clippy can't catch.
//! - Every Markdown file starts with YAML frontmatter that has a non-empty `description`.
//! - Markdown files under `docs/` also have `readWhen`: the tasks that call for reading them.
//! - Relative inline links (`[text](path)`) outside code point to files that exist;
//!   reference-style links (`[text][id]`) and links that never close aren't checked.
//! - Tool versions pinned in more than one file agree.
//!
//! Files come from `git ls-files`, so ignored paths such as `target/` are never checked.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn repository_follows_conventions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = repository_files(root).expect("the repository's files should be listable");
    let violations: Vec<String> = files
        .into_iter()
        .filter_map(|relative| Some((FileKind::of(&relative)?, relative)))
        .flat_map(|(kind, relative)| {
            let violations = match fs::read_to_string(root.join(&relative)) {
                Ok(content) => check_file(kind, &relative, &content, |target| {
                    root.join(target).exists()
                }),
                // Still listed by `git ls-files --cached` after an unstaged delete: nothing to check.
                Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
                Err(error) => vec![format!("could not be read as UTF-8 text: {error}")],
            };
            violations
                .into_iter()
                .map(move |violation| format!("{}: {violation}", relative.display()))
        })
        .collect();

    assert!(
        violations.is_empty(),
        "repository convention violations (see AGENTS.md):\n{}",
        violations.join("\n")
    );
}

#[test]
fn pinned_tool_versions_agree() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let read = |name: &str| {
        fs::read_to_string(root.join(name))
            .unwrap_or_else(|error| panic!("reading {name}: {error}"))
    };
    let mise = read("mise.toml");
    let rust_toolchain = read("rust-toolchain.toml");

    let toolchain = toml_string(&rust_toolchain, "toolchain", "channel");
    assert!(
        toolchain.is_some(),
        "rust-toolchain.toml should set `channel` under `[toolchain]`"
    );
    assert_eq!(
        toml_string(&mise, "tools", "rust"),
        toolchain,
        "mise.toml's `rust` must equal rust-toolchain.toml's `channel`: mise exports \
         RUSTUP_TOOLCHAIN, which overrides the toolchain file (docs/til.md)"
    );

    let hk = toml_string(&mise, "tools", "hk").expect("mise.toml should pin `hk` under `[tools]`");
    let hk_config = read("hk.pkl");
    let package_urls = hk_config.matches("jdx/hk/releases/download/").count();
    let pinned_urls = hk_config
        .matches(&format!("jdx/hk/releases/download/v{hk}/hk@{hk}#"))
        .count();
    assert!(package_urls > 0, "hk.pkl should import hk's Pkl packages");
    assert_eq!(
        package_urls, pinned_urls,
        "every hk package URL in hk.pkl must use the hk version pinned in mise.toml ({hk}), \
         in both the release path and the package name"
    );
}

/// Tracked plus untracked-but-not-ignored files, relative to `root`.
fn repository_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .current_dir(root)
        .output()
        .map_err(|error| {
            format!(
                "could not run `git ls-files` in {}: {error}",
                root.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "`git ls-files` failed with {} in {}; expected a git repository",
            output.status,
            root.display()
        ));
    }
    let listing = String::from_utf8(output.stdout)
        .map_err(|error| format!("`git ls-files` printed a non-UTF-8 path: {error}"))?;
    Ok(listing
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect())
}

/// The kinds of files these conventions cover; every other file is skipped.
#[derive(Clone, Copy)]
enum FileKind {
    Rust,
    Markdown,
}

impl FileKind {
    fn of(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            "rs" => Some(Self::Rust),
            "md" => Some(Self::Markdown),
            _ => None,
        }
    }
}

/// All convention violations in one file; `exists` answers whether a repo-relative path exists.
fn check_file(
    kind: FileKind,
    relative: &Path,
    content: &str,
    exists: impl Fn(&Path) -> bool,
) -> Vec<String> {
    match kind {
        FileKind::Rust => {
            let mut violations: Vec<String> = check_rust_header(content).into_iter().collect();
            violations.extend(check_inner_allow(content));
            violations
        }
        FileKind::Markdown => {
            let mut violations = check_frontmatter(content, relative.starts_with("docs"));
            violations.extend(check_links(relative, content, exists));
            violations
        }
    }
}

fn check_rust_header(content: &str) -> Option<String> {
    let first_line = content.lines().next().unwrap_or_default();
    let has_header = first_line
        .strip_prefix("//!")
        .is_some_and(|text| !text.trim().is_empty());
    (!has_header).then(|| {
        format!("first line must be a `//! <what this file is for>` header comment, found {first_line:?}")
    })
}

/// Clippy's `allow_attributes` only checks outer `#[allow]`, so inner ones are caught here.
///
/// Inner attributes must come before a file's first item, so only that leading block of
/// comments and `#![...]` attributes is scanned; text in later raw strings can't trip it.
fn check_inner_allow(content: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let mut open_brackets = 0_usize;
    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        let starts_attribute = trimmed.starts_with("#!");
        if open_brackets == 0
            && !starts_attribute
            && !trimmed.is_empty()
            && !trimmed.starts_with("//")
        {
            break;
        }
        let compact: String = trimmed.split_whitespace().collect();
        // Covers `#![allow(...)]`, `#![allow (...)]`, and `#![cfg_attr(..., allow(...))]`.
        if starts_attribute && compact.contains("allow(") {
            violations.push(format!(
                "line {}: use `#![expect(lint, reason = \"...\")]` instead of an inner `allow`",
                index + 1
            ));
        }
        open_brackets = (open_brackets + trimmed.matches('[').count())
            .saturating_sub(trimmed.matches(']').count());
    }
    violations
}

fn check_frontmatter(content: &str, requires_read_when: bool) -> Vec<String> {
    let Some(fields) = frontmatter_fields(content) else {
        return vec!["must start with YAML frontmatter: a `---` line, `key: value` lines, and a closing `---` line".to_owned()];
    };
    let required: &[&str] = if requires_read_when {
        &["description", "readWhen"]
    } else {
        &["description"]
    };
    required
        .iter()
        .filter(|key| {
            !fields
                .iter()
                .any(|(field, value)| field == *key && !is_blank(value))
        })
        .map(|key| format!("frontmatter needs a non-empty `{key}`"))
        .collect()
}

/// Top-level `key: value` pairs of the leading frontmatter block; `None` without a closed block.
fn frontmatter_fields(content: &str) -> Option<Vec<(&str, &str)>> {
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return None;
    }
    let mut fields = Vec::new();
    for line in lines {
        if line == "---" {
            return Some(fields);
        }
        // Indented or list lines continue a previous value; they aren't keys.
        if line.starts_with([' ', '\t', '-']) {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            fields.push((key.trim(), value.trim()));
        }
    }
    None
}

/// Empty strings, YAML nulls, and values that are only a comment.
fn is_blank(value: &str) -> bool {
    matches!(value, "" | "''" | "\"\"" | "~")
        || value.eq_ignore_ascii_case("null")
        || value.starts_with('#')
}

fn check_links(relative: &Path, content: &str, exists: impl Fn(&Path) -> bool) -> Vec<String> {
    let directory = relative.parent().unwrap_or_else(|| Path::new(""));
    relative_link_targets(content)
        .into_iter()
        .filter(|target| !exists(&directory.join(target)))
        .map(|target| {
            format!(
                "link target `{}` does not exist",
                directory.join(&target).display()
            )
        })
        .collect()
}

/// Paths of relative inline links, without `#fragment` or `?query`, outside fenced and inline code.
fn relative_link_targets(markdown: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut open_fence: Option<&str> = None;
    for line in markdown.lines() {
        let fence = fence_marker(line.trim_start());
        if let Some(open) = open_fence {
            // A fence closes with the same character, at least as many times as it opened.
            if fence.is_some_and(|marker| marker.starts_with(open)) {
                open_fence = None;
            }
            continue;
        }
        if fence.is_some() {
            open_fence = fence;
            continue;
        }
        targets.extend(inline_link_targets(&without_code_spans(line)));
    }
    targets
}

/// The leading run of three or more `` ` `` or `~` that opens or closes a fenced code block.
fn fence_marker(trimmed: &str) -> Option<&str> {
    let marker = trimmed
        .chars()
        .next()
        .filter(|first| matches!(first, '`' | '~'))?;
    let length = trimmed.len() - trimmed.trim_start_matches(marker).len();
    (length >= 3).then(|| &trimmed[..length])
}

/// The line with every code span removed, so code examples never count as links.
///
/// As in `CommonMark`, a span opens with a run of backticks and closes at the next run of the
/// same length (so ``` `` `x` `` ``` is one span); a run that never closes is literal text.
fn without_code_spans(line: &str) -> String {
    let mut kept = String::new();
    let mut rest = line;
    while let Some(start) = rest.find('`') {
        kept.push_str(&rest[..start]);
        let (opening, after) = rest[start..].split_at(backtick_run_length(&rest[start..]));
        if let Some(end) = find_backtick_run(after, opening.len()) {
            rest = &after[end + opening.len()..];
        } else {
            kept.push_str(opening);
            rest = after;
        }
    }
    kept.push_str(rest);
    kept
}

fn backtick_run_length(text: &str) -> usize {
    text.len() - text.trim_start_matches('`').len()
}

/// Byte offset of the next run of exactly `length` backticks in `text`.
fn find_backtick_run(text: &str, length: usize) -> Option<usize> {
    let mut offset = 0;
    while let Some(found) = text[offset..].find('`') {
        let start = offset + found;
        let run = backtick_run_length(&text[start..]);
        if run == length {
            return Some(start);
        }
        offset = start + run;
    }
    None
}

fn inline_link_targets(line: &str) -> Vec<String> {
    line.split("](")
        .skip(1)
        .filter_map(link_destination)
        .filter(|destination| is_relative_path(destination))
        .filter_map(|destination| destination.split(['#', '?']).next())
        .filter(|path| !path.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// The destination at the start of `after_paren` (the text after `](`), without an optional title.
/// Handles leading whitespace, `<path with spaces>`, and balanced parentheses; `None` when no `)`
/// closes the link.
fn link_destination(after_paren: &str) -> Option<&str> {
    let (destination, rest) = split_destination(after_paren.trim_start())?;
    // What follows is `)` or a title and then `)`; a link that never closes isn't a link.
    rest.contains(')').then_some(destination)
}

fn split_destination(text: &str) -> Option<(&str, &str)> {
    if let Some(bracketed) = text.strip_prefix('<') {
        return bracketed.split_once('>');
    }
    let mut depth = 0_usize;
    for (index, character) in text.char_indices() {
        match character {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(text.split_at(index)),
            ')' => depth -= 1,
            ' ' | '\t' => return Some(text.split_at(index)),
            _ => {}
        }
    }
    None
}

fn is_relative_path(destination: &str) -> bool {
    !destination.starts_with('#')
        && !destination.starts_with('/')
        && !destination.contains("://")
        && !destination.starts_with("mailto:")
}

/// The string value of `key = "value"` (or `'value'`) inside `[table]` of a TOML file.
/// Enough for the simple pin files checked here, not a general TOML parser.
fn toml_string<'a>(toml: &'a str, table: &str, key: &str) -> Option<&'a str> {
    let mut current_table = "";
    for line in toml.lines().map(str::trim) {
        if let Some(header) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            current_table = header.trim();
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if current_table == table && name.trim() == key {
            let value = value.trim_start();
            let quote = value
                .chars()
                .next()
                .filter(|first| matches!(first, '"' | '\''))?;
            return value[1..].split(quote).next();
        }
    }
    None
}

// The checker's own cases: a false positive here would block every commit.

const NO_VIOLATIONS: [String; 0] = [];

#[test]
fn rust_header_must_be_a_module_doc_comment_on_the_first_line() {
    assert_eq!(
        check_rust_header("//! Parses arguments.\nfn main() {}\n"),
        None
    );
    assert!(check_rust_header("// Parses arguments.\n").is_some());
    assert!(check_rust_header("\n//! Parses arguments.\n").is_some());
    assert!(check_rust_header("//!\n//! Parses arguments.\n").is_some());
    assert!(check_rust_header("").is_some());
}

#[test]
fn inner_allow_attributes_are_rejected_but_expect_is_fine() {
    let content = "//! Header.\n#![expect(\n    clippy::x,\n    reason = \"y\"\n)]\n  #![allow (clippy::x)]\n#![cfg_attr(test, allow(clippy::y))]\n";
    assert_eq!(
        check_inner_allow(content),
        [
            "line 6: use `#![expect(lint, reason = \"...\")]` instead of an inner `allow`",
            "line 7: use `#![expect(lint, reason = \"...\")]` instead of an inner `allow`",
        ]
    );
}

#[test]
fn inner_allow_scan_stops_at_the_first_item() {
    let content = "//! Header.\n\nconst FIXTURE: &str = r\"\n#![allow(dead_code)]\n\";\n";
    assert_eq!(check_inner_allow(content), NO_VIOLATIONS);
}

#[test]
fn frontmatter_needs_description_and_read_when_only_in_docs() {
    let with_description = "---\ndescription: 'What it is.'\n---\n# Title\n";
    assert_eq!(check_frontmatter(with_description, false), NO_VIOLATIONS);
    assert_eq!(
        check_frontmatter(with_description, true),
        ["frontmatter needs a non-empty `readWhen`"]
    );

    let docs_page = "---\ndescription: 'What it is.'\nreadWhen: 'changing code'\n---\n";
    assert_eq!(check_frontmatter(docs_page, true), NO_VIOLATIONS);

    for blank in ["''", "null", "Null", "~", "# only a comment", ""] {
        let page = format!("---\ndescription: {blank}\n---\n");
        assert_eq!(check_frontmatter(&page, false).len(), 1, "{blank:?}");
    }
    assert_eq!(
        check_frontmatter("---\ndescription: 'never closed'\n", false).len(),
        1
    );
    assert_eq!(check_frontmatter("# No frontmatter\n", false).len(), 1);
}

#[test]
fn frontmatter_ignores_nested_lines() {
    let nested = "---\ndescription: >\n  folded: text\ntags:\n  - readWhen: no\n---\n";
    assert_eq!(
        check_frontmatter(nested, true),
        ["frontmatter needs a non-empty `readWhen`"]
    );
}

#[test]
fn link_targets_skip_code_urls_anchors_and_titles() {
    let markdown = "\
See [arch](architecture.md#layers) and [adr](<adr.md> \"ADRs\") and [t](til.md \"TIL\").
Not links: `[x](inline-code.md)`, [web](https://example.com), [top](#top), [abs](/etc/hosts).
```md
[fenced](fenced.md)
```
````md
```
[inside a longer fence](longer-fence.md)
````
~~~
[tilde](tilde.md)
~~~
[readme](../README.md?plain=1) [unclosed](nowhere.md [angle](<never-closed.md> [title](t.md \"unfinished
";
    assert_eq!(
        relative_link_targets(markdown),
        ["architecture.md", "adr.md", "til.md", "../README.md"]
    );
}

#[test]
fn link_targets_keep_spaces_in_angle_brackets_and_balanced_parentheses() {
    assert_eq!(
        // Without `<...>`, CommonMark destinations can't contain spaces but can nest parentheses.
        relative_link_targets("[g](<docs/my guide.md>) [p](notes(draft).md) [w]( padded.md )"),
        ["docs/my guide.md", "notes(draft).md", "padded.md"]
    );
}

#[test]
fn code_spans_match_backtick_runs_of_equal_length() {
    assert_eq!(
        without_code_spans("a `` `x` [l](in-span.md) `` b `c` d"),
        "a  b  d"
    );
    // A run that never closes is literal text, so the link after it still counts.
    assert_eq!(relative_link_targets("a ` b [l](after.md)"), ["after.md"]);
}

#[test]
fn links_resolve_relative_to_the_linking_file() {
    let exists = |path: &Path| path == Path::new("docs/../README.md");
    assert_eq!(
        check_links(Path::new("docs/overview.md"), "[r](../README.md)", exists),
        NO_VIOLATIONS
    );
    assert_eq!(
        check_links(Path::new("docs/overview.md"), "[m](missing.md)", exists).len(),
        1
    );
}

#[test]
fn toml_string_reads_quoted_values_in_the_named_table() {
    let toml = "rust = \"0.0\"\n[tools]\nhk = '2.11.1'\nrust = \"1.99.1\" # pinned\n[other]\njust = \"1\"\n";
    assert_eq!(toml_string(toml, "tools", "rust"), Some("1.99.1"));
    assert_eq!(toml_string(toml, "tools", "hk"), Some("2.11.1"));
    assert_eq!(toml_string(toml, "tools", "just"), None);
    assert_eq!(toml_string(toml, "tools", "missing"), None);
}
