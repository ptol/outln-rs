---
description: 'Notable changes to the outln Rust rewrite by date, including deliberate divergences from the TypeScript build.'
readWhen: 'finishing a change that users or agents would notice'
---

# Changelog

## 2026-11-06

- Markdown support in file mode, glob view, and debug mode, ported from the TypeScript build, with a casefile runner (`tests/casefiles.rs`) and 81 casefiles. Deliberate differences from the TypeScript build:
  - **Markdown parsing**: headings come from a CommonMark parser (pulldown-cmark). Lines inside `~~~` fences, indented code, and HTML blocks are no longer reported as headings, a ` ``` ` line inside a `~~~` fence no longer breaks fence tracking, underlined (setext) headings and `#` headings indented up to three spaces are reported, a lone `#` is an empty heading, and headings inside block quotes and lists are skipped. Frontmatter list items (`- x`) and comments (`# x`) are no longer printed as fields.
  - **Debug mode**: an existing path containing glob characters (`app/[id]/page.md`) is outlined instead of rejected, and a `{a,b}` pattern is rejected as a glob. Files are separated by a blank line even when one has no final newline.
  - **Glob patterns**: every `{...}` group counts as glob syntax (not only the first), alternatives may contain `/`, and a pattern without `**` walks only as deep as it can match.
  - **Unreadable directories**: a directory below a directory argument that can't be listed is reported (`Directory <path> could not be read: ...`) and fails the run; the other files are still printed. Glob walks skip such directories silently.
  - **CLI**: help, version, and usage-error texts come from clap. The version prints as `outln <version>`, usage errors (unknown option, no paths, `--debug` without a path) exit 2 instead of 1, and errors use clap's wording.
  - **Directory walking**: git semantics through the `ignore` crate. A `.gitignore` outside a git repository is ignored, `.ignore` files and `.git/info/exclude` are honoured, git's global excludes file applies, and ancestor rules still apply to an explicitly named directory they ignore. Files are sorted by byte order instead of locale order.
  - **Encoding**: a file that isn't valid UTF-8 is reported as `FILE <path> IS NOT UTF-8 TEXT` instead of being decoded with replacement characters, and an unreadable file as `FILE <path> COULD NOT BE READ`.
  - **Unsupported files**: an unsupported file is reported as unsupported without being read.
