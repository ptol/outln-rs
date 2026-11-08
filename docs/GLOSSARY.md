---
description: 'Canonical vocabulary for outln: outlines, their parts, positions, the CLI modes, language engines, and casefiles.'
readWhen: 'naming a concept in code, docs, or CLI output'
---

# outln glossary

outln turns source files into outlines that coding agents read before opening the files themselves.

## Outlines

### Outline

The compact structural summary of one source file: its path and line count, its header, then one outline line per declaration.
_Avoid_: Summary, AST, tree

### Outline line

One line of an outline naming a single declaration, or a run of consecutive Rust module declarations without bodies (`mod cli, code_outline, ...`), prefixed with its line range and indented by how deeply the declaration is nested.
_Avoid_: Entry, row

### Line range

The first and last line a declaration spans, from its first leading annotation, 1-based and inclusive, written `[L<first>-L<last>]`.
_Avoid_: Span, location

### Line count

How many lines a file has, printed after its path in an outline and a file summary as `(1 line)` or `(120 lines)`: one per line break, plus one for a last line without one, so an empty file has `0 lines`.
_Avoid_: Size, length, LOC

### Declaration

A part of a file worth listing in an outline: a top-level definition such as a function, type, module, `impl` block, macro definition, re-export, default export, or Markdown heading, a member, a declaration inside an inline module, a test block, a data file's key, or a CSS rule.
Code says declaration even where a grammar's node kinds say item (tree-sitter-rust's `function_item`).
_Avoid_: Symbol, definition, item

### Signature

The text an outline line prints for a code declaration: its keywords and visibility, name, type parameters, parameters, return type, and `where` clause (for an `impl` block, its trait and type; for a constant, its type; for a CSS rule, its selectors or prelude), on one line and without its body or value.
_Avoid_: Head, prototype

### Leading annotation

A doc comment (`///`, `/** */`), attribute (`#[...]`), or decorator (`@x`) above a declaration and belonging to it; the declaration's line range starts at its first leading annotation. A blank line detaches a doc comment but not an attribute or decorator, which the syntax ties to the declaration. Data files and CSS have no doc comments, so the comment lines directly above a key or rule are its leading annotation.
_Avoid_: Prefix, trivia

### Member

A declaration inside a class, `impl` block, or trait, listed one level under it: a class's method, accessor, or constructor, or an `impl` block's or trait's function, constant, or type.
_Avoid_: Child, nested declaration, associated item (Rust's own word, fine in Rust-specific code)

### Test block

A call that opens a group of tests or one test, such as `describe('parser', …)`, listed by its title: at the top level, or one level under the test block whose callback holds it.
_Avoid_: Test case, spec, suite

### Key

A top-level key of a data file, or a TOML table header (`[package]`, `[[bin]]`): the data file's kind of declaration. Its outline line prints a scalar value after the key (`name: CI`, `edition = "2024"`), and only the key for a table, mapping, object, or list.
_Avoid_: Property, entry

### Rule

A CSS style rule or at-rule: the CSS kind of declaration. Its outline line prints a style rule's selectors (`.button, a:hover`) or an at-rule's keyword and prelude (`@media (max-width: 600px)`); the rules inside a top-level at-rule's block are listed one level deeper.
_Avoid_: Selector (only part of a rule), ruleset. CSS's own word declaration names a property inside a rule, such as `color: red`, which is never listed.

### Header

What a file says about itself at its top: its header comment, or its frontmatter for Markdown.
_Avoid_: File metadata

### Header comment

The comment at the top of a source file that says what the file is for, such as a Rust file's `//!` comment; outlines print only its first prose paragraph, never a license notice.
_Avoid_: Top comment, file doc, banner

### Preamble

What a file's top holds for tools rather than people to read, which may come before the header comment and is never part of it: a shebang, a directive such as `'use strict'`, `@charset`, or TOML's `#:schema`, a Rust inner attribute (`#![...]`), or a tool directive written as a comment, such as `// @ts-nocheck`.
_Avoid_: Prologue, prelude (a CSS at-rule's prelude is part of a rule). Preamble names neither the header nor a leading annotation.

### Frontmatter

The YAML mapping between `---` lines at the very start of a Markdown file (the closing line may be `...`); its fields are the mapping's top-level keys, which a file summary shows up to three of, describing keys first. A `---` block holding no mapping is body text.
_Avoid_: Metadata block, YAML header

### File summary

The single line a file contributes to a glob view: its path, its line count, and its header, or, for a data file without a header comment, its field summary, for a code file without one, its declaration summary, and for a Markdown file without frontmatter fields, its first heading.
_Avoid_: Outline, description

### Field summary

What a glob view prints for a data file without a header comment: up to three top-level keys with scalar values, describing keys first and then the others in source order, as their outline lines print them, such as `{description: "...", name: "outln", private: true, ...}` (for a TOML file whose scalar keys are all in tables, the first table's, as `[package] {name = "outln", ...}`). A file without scalar keys shows its keys' names, `{compilerOptions, include}`.
_Avoid_: Preview, metadata

### Describing key

A `description` or `title` key, whose value says what the whole file is: a field summary or a Markdown file summary shows it before other fields, and a data file's outline line cuts its value at 100 characters instead of 40.
_Avoid_: Preferred key, summary key, main field

### Directory summary

The single line a glob view prints, in place of file summaries, for a directory at its depth limit that holds supported files: the directory's path with a trailing `/`, how many supported files are below it, and their total line count, such as `packages/core/ (257 files, 31234 lines)`.
_Avoid_: Folder line, rollup, collapsed directory

### Declaration summary

What a glob view prints for a code file without a header comment: its first distinct top-level declarations, each as its outline line starts up to its parameter list, a repeated one with its count, such as `pub struct Arg, impl Arg ×6, impl Clone for Arg, ...`. Only the file's start is read, so a count is of the declarations there, not in the whole file.
_Avoid_: Symbol list, table of contents, preview

## Positions

### Byte range

The half-open stretch of bytes, from a first offset up to but excluding a last one, that a language engine reports for a declaration or a part of the header.
_Avoid_: Span, offsets

### Highlight

The part of a file's source that debug mode marks because it became an outline line, a frontmatter field, or a header comment line. Its highlight style says how it prints: in green on a terminal when `NO_COLOR` is unset or empty, else between the markers `«` and `»`, one pair per line it covers.
_Avoid_: Selection, mark (a marker is only the `«` or `»` around a highlight)

## Modes

### File mode

The mode that prints a full outline for each file argument.
_Avoid_: Outline mode, single mode

### Glob view

The mode that prints one file summary per file matched by directory or glob arguments, and with a depth limit one directory summary per directory at the limit.
_Avoid_: Directory mode, glob mode, listing

### Depth limit

How many directory levels below a directory argument a glob view lists file summaries for (`--depth N`; 1 lists the files directly inside); each directory at the limit prints a directory summary instead. A glob pattern sets its own depth.
_Avoid_: Max depth, level, recursion limit

### Hint

The line a glob view without a depth limit prints on stderr, after more than 300 file summaries, naming a way to list fewer: `Hint: found 989 files; for an overview, try --depth 1 or a narrower path`.
_Avoid_: Warning (nothing failed), tip, notice

### Debug mode

The mode that prints a file's source with the parts that became outline lines highlighted.
_Avoid_: Highlight mode, verbose mode

## Languages

### Language engine

The component that produces outlines and file summaries for one language.
_Avoid_: Parser, plugin, handler

### Data file

A TOML, YAML, or JSON file: its declarations are its keys, and its file summary is its header comment or else its field summary.
_Avoid_: Config file (not every one configures something)

## Testing

### Casefile

One test case of outln's behaviour in a single file: the arguments, the input files, and the expected stdout, stderr, and exit code.
_Avoid_: Golden file, fixture, snapshot

### Engine sample

A small file with a header and declarations of most kinds, one per language engine (one per grammar for an engine with several), that the checks shared by every engine outline.
_Avoid_: Fixture, example file, sample case
