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

One line of an outline naming a single declaration, prefixed with its line range.
_Avoid_: Entry, row

### Line range

The first and last line a declaration spans, 1-based and inclusive, written `[L<first>-L<last>]`.
_Avoid_: Span, location

### Declaration

A top-level definition worth listing in an outline, such as a function, type, or Markdown heading.
_Avoid_: Symbol, definition, item

### Header

What a file says about itself at its top: its header comment, or its frontmatter for Markdown.
_Avoid_: Preamble, file metadata

### Header comment

The comment at the top of a source file that says what the file is for.
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

The part of a file's source that debug mode marks because it became an outline line or a frontmatter field.
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
