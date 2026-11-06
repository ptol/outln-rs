---
description: 'Canonical vocabulary for outln: outlines, their parts, positions, the CLI modes, language engines, and casefiles.'
readWhen: 'naming a concept in code, docs, or CLI output'
---

# outln glossary

outln turns source files into outlines that coding agents read before opening the files themselves.

## Outlines

### Outline

The compact structural summary of one source file: its path, its header, then one outline line per declaration.
_Avoid_: Summary, AST, tree

### Outline line

One line of an outline naming a single declaration, prefixed with its line range and indented by how deeply the declaration is nested.
_Avoid_: Entry, row

### Line range

The first and last line a declaration spans, from its first leading annotation, 1-based and inclusive, written `[L<first>-L<last>]`.
_Avoid_: Span, location

### Declaration

A part of a file worth listing in an outline: a top-level definition such as a function, type, module, `impl` block, macro definition, re-export, default export, or Markdown heading, a class member, a test block, a data file's key, or a CSS rule.
Code says declaration even where a grammar's node kinds say item (tree-sitter-rust's `function_item`).
_Avoid_: Symbol, definition, item

### Signature

The text an outline line prints for a code declaration: its keywords and visibility, name, type parameters, parameters, return type, and `where` clause (for an `impl` block, its trait and type; for a constant, its type; for a CSS rule, its selectors or prelude), on one line and without its body or value.
_Avoid_: Head, prototype

### Leading annotation

A doc comment (`///`, `/** */`), attribute (`#[...]`), or decorator (`@x`) above a declaration and belonging to it; the declaration's line range starts at its first leading annotation. A blank line detaches a doc comment but not an attribute or decorator, which the syntax ties to the declaration. Data files and CSS have no doc comments, so the comment lines directly above a key or rule are its leading annotation.
_Avoid_: Preamble, prefix, trivia

### Class member

A method, accessor, or constructor of a class, listed under the class one level deeper.
_Avoid_: Child, nested declaration

### Test block

A top-level call that opens a group of tests or one test, such as `describe('parser', …)`, listed by its title.
_Avoid_: Test case, spec, suite

### Key

A top-level key of a data file, or a TOML table header (`[package]`, `[[bin]]`): the data file's kind of declaration. Its outline line prints a scalar value after the key (`name: CI`, `edition = "2024"`), and only the key for a table, mapping, object, or list.
_Avoid_: Property, entry

### Rule

A CSS style rule or at-rule: the CSS kind of declaration. Its outline line prints a style rule's selectors (`.button, a:hover`) or an at-rule's keyword and prelude (`@media (max-width: 600px)`); the rules inside a top-level at-rule's block are listed one level deeper.
_Avoid_: Selector (only part of a rule), ruleset. CSS's own word declaration names a property inside a rule, such as `color: red`, which is never listed.

### Header

What a file says about itself at its top: its header comment, or its frontmatter for Markdown.
_Avoid_: Preamble, file metadata

### Header comment

The comment at the top of a source file that says what the file is for, such as a Rust file's `//!` comment; outlines print only its first paragraph.
_Avoid_: Top comment, file doc, banner

### Frontmatter

The block of `key: value` fields between `---` lines at the very start of a Markdown file.
_Avoid_: Metadata block, YAML header

### File summary

The single line a file contributes to a glob view: its path, its line count, and its header, or, for a data file without a header comment, its field summary, and for a Markdown file without frontmatter fields, its first heading.
_Avoid_: Outline, description

### Field summary

What a glob view prints for a data file without a header comment: its first top-level keys with scalar values, as their outline lines print them, such as `{name: "outln", version: "0.2.0", private: true, ...}` (for a TOML file whose scalar keys are all in tables, the first table's, as `[package] {name = "outln", ...}`).
_Avoid_: Preview, metadata

## Positions

### Byte range

The half-open stretch of bytes, from a first offset up to but excluding a last one, that a language engine reports for a declaration or a part of the header.
_Avoid_: Span, offsets

### Highlight

The part of a file's source that debug mode marks because it became an outline line, a frontmatter field, or a header comment line.
_Avoid_: Selection, mark

## Modes

### File mode

The mode that prints a full outline for each file argument.
_Avoid_: Outline mode, single mode

### Glob view

The mode that prints one file summary per file matched by directory or glob arguments.
_Avoid_: Directory mode, glob mode, listing

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
