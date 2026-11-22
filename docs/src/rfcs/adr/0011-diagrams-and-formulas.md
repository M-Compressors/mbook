# ADR-0011: Diagrams and formulas are resources of their own kinds, shown the way a picture is

**Status:** Accepted\
**Date:** 2026-11-22\
**Deciders:** Format author

## Context

A textbook has formulas on every page and a technical book has diagrams.
Until now a book could carry them only as pictures: the packer drew each one
and stored it as an AVIF file ([ADR-0002](0002-stored-avif.md)). A picture of
a formula does not follow the size of the text, its typeface or the page
tone, cannot be read aloud, and cannot be corrected without the tool that
drew it. The appendix of [RFC 0001](../0001-mbook-format.md) measures a
textbook with 3,000 such pictures.

Both have a notation that is text. A diagram is described in
[Mermaid](https://mermaid.js.org/), a formula in the LaTeX subset that
[KaTeX](https://katex.org/) sets. Two things constrain how a book stores
them:

- A chapter is GitHub Flavored Markdown without raw HTML
  ([ADR-0006](0006-markdown-dialect.md)), and an unpacked book is a folder
  that any Markdown viewer shows.
- A reader that does not know a later addition must still show the book
  ([RFC 0001](../0001-mbook-format.md#versioning)).

## Decision

A book may carry *diagrams* and *formulas*. Each is a resource of its own,
with a path, like a picture:

| `kind` | Resource | Decoded content |
| ---: | --- | --- |
| 5 | Diagram | UTF-8 text in Mermaid notation |
| 6 | Formula | UTF-8 text in the notation of KaTeX |

A chapter shows one the way it shows a picture, `![alt](path "title")`, and
the resource table tells which of the three a path names. Alone in a
paragraph it is a figure; inside a line it is a part of that line. The
alternative text is what a reader shows when it cannot draw the diagram or
set the formula.

## Options considered

| Dimension | A: resources of new kinds, shown like pictures | B: additions to the dialect (` ```mermaid ` fences, `$…$`) | C: pictures only, as before |
| --- | --- | --- | --- |
| Reader complexity | A Mermaid renderer and a formula typesetter, both optional: without them the alternative text is shown | The same two, plus a Markdown parser with a math extension; without them the source is shown as code and as literal `$…$` | None beyond AVIF |
| File size | The source, compressed, plus 74 to 76 bytes per resource | The source inside its chapter, compressed with it | An AVIF file per formula: several hundred bytes at least |
| Compatibility | A new `kind` is a compatible addition: an older reader skips the resource and shows the alternative text | A chapter is no longer plain GFM for parsers without math; `$` in prose (prices) changes meaning in existing books | Every reader shows it |
| Evolution | A kind per notation; a later notation is one more kind | Every notation is a dialect change and needs a new escape rule | Nothing to evolve; nothing to edit either |
| Reuse | A formula used ten times is stored once | Repeated in the text | Stored once |

**Chosen: A.**

## Trade-off analysis

- **C** is what the format did. It asks nothing of a reader and gives up
  everything that makes a formula text: size, tone, speech and correction.
- **B** is what GitHub renders, and the first draft of the format listed
  math among the future additions to the dialect. It changes what a chapter
  is: `$5 and $10` in a novel becomes a formula, so existing text needs
  escaping, and a parser without the extension shows the delimiters. A
  reader that cannot draw a diagram shows its source as a code block, with
  no words that say what the diagram is of.
- **A** leaves the dialect alone. `![alt](path)` is Markdown every parser
  reads; only the kind of the target is new, and a reader learns it from
  the table it already has. The price is one small frame for each formula,
  and a plain Markdown viewer over an unpacked book shows a broken image
  with the alternative text, where B would show at least the source.

The alternative text decides it. A book meant to be read by everyone needs
a fallback in words for each diagram and formula, and the picture syntax is
the one place in GFM that has such a field.

## Format details

The format author confirmed these details on 2026-11-22. RFC 0001
records the format rules.

| Detail | Decision |
| --- | --- |
| The codes | `kind` 5 for a diagram, 6 for a formula |
| The reference | The picture syntax; no syntax of its own |
| Display or inline | A formula alone in a paragraph is set as a display formula, any other as a part of its line |
| The size columns | Zero: a reader lays a diagram and a formula out itself |
| The encoding | That of a chapter: UTF-8, line feeds, no other control character but tab |
| An empty source | Not allowed: a writer refuses it |
| The version of a notation | Mermaid 11 and KaTeX 0.16 are references for authors. A reader shows the alternative text for what it cannot render. |
| Checking the notation | Not a writer's duty: `mbook` stores the source as text and links no renderer |
| The recommended file names | `.mmd` for a diagram, `.tex` for a formula |
| The cover | Stays a picture |
| The format version | Stays 1.0 while the format is a draft, although a new `kind` is the kind of change `minor` exists for |
| Active content | A reader runs no script, follows no link and fetches nothing for a diagram or a formula |

## Consequences

- Easier: a formula follows the size, the typeface and the page tone of the
  text, and a diagram the page tone.
- Easier: an editor changes a diagram or a formula as text, in the book.
- Easier: a formula-heavy book is smaller, a few dozen bytes of source per
  formula instead of an AVIF file.
- Harder: a reader that wants to show them needs two more renderers, and
  both notations are defined by an implementation, not by a specification.
- Harder: the size of a diagram or a formula is not in the tables, so a
  reader cannot reserve its space before it is laid out.
- Harder: a Markdown viewer over an unpacked folder shows the alternative
  text only.
- To revisit: a converter that turns ` ```mermaid ` fences and `$…$` of a
  Markdown source into these resources; whether a reader should be told
  which version of a notation a book was written for; the per-resource
  overhead for books with thousands of one-letter formulas.

## Action items

1. [x] Reader: give kinds 5 and 6 ids of their own and read them as text.
2. [x] Writer: store diagrams and formulas after the pictures; accept them
   as targets of the picture syntax.
3. [x] Application: render both, and edit them in the book editor.
4. [ ] Converter: carry over the diagrams and formulas of a Markdown source.
