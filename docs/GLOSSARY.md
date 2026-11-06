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

A part of a file worth listing in an outline: a top-level definition such as a function, type, module, `impl` block, macro definition, re-export, default export, or Markdown heading, a class member, or a test block.
Code says declaration even where a grammar's node kinds say item (tree-sitter-rust's `function_item`).
_Avoid_: Symbol, definition, item

### Signature

The text an outline line prints for a code declaration: its keywords and visibility, name, type parameters, parameters, return type, and `where` clause (for an `impl` block, its trait and type; for a constant, its type), on one line and without its body or value.
_Avoid_: Head, prototype

### Leading annotation

A doc comment (`///`, `/** */`), attribute (`#[...]`), or decorator (`@x`) above a declaration and belonging to it; the declaration's line range starts at its first leading annotation. A blank line detaches a doc comment but not an attribute or decorator, which the syntax ties to the declaration.
_Avoid_: Preamble, prefix, trivia

### Class member

A method, accessor, or constructor of a class, listed under the class one level deeper.
_Avoid_: Child, nested declaration

### Test block

A top-level call that opens a group of tests or one test, such as `describe('parser', …)`, listed by its title.
_Avoid_: Test case, spec, suite

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

The single line a file contributes to a glob view: its path and its header.
_Avoid_: Outline, description

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

## Testing

### Casefile

One test case of outln's behaviour in a single file: the arguments, the input files, and the expected stdout, stderr, and exit code.
_Avoid_: Golden file, fixture, snapshot
