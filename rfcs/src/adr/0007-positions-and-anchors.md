# ADR-0007: Byte offsets and a stored anchor table

**Status:** Accepted (the anchor table); byte offsets in `toc` are proposed\
**Date:** 2026-11-21\
**Deciders:** Format author

## Context

Three things point into the middle of a chapter: entries of the table of
contents, links with a fragment (`ch01.md#the-station`), and a reader's
bookmarks. With GitHub Flavored Markdown
([ADR-0006](0006-markdown-dialect.md)) an anchor is not written by the
author. GitHub derives it from the heading text, and that rule is defined
only by its implementation.

## Decision

A position is a text resource number and a byte offset into the decoded
chapter. The table of contents stores positions. Resource 4 is a table that
maps anchor names to positions; the writer fills it using GitHub's naming
rule, and readers only look names up.

## Options considered

| Dimension | A: offsets and a stored anchor table | B: readers compute GitHub anchors | C: explicit `{#id}` attributes |
| --- | --- | --- | --- |
| Reader complexity | Table lookup | Must reproduce GitHub's algorithm exactly | Parser extension for attributes |
| File size | About 150 B for three anchors; grows with headings | 0 | A few bytes per heading in the text |
| Compatibility | Chapters stay plain GFM | Chapters stay plain GFM | Not GFM |
| Evolution | The naming rule can change without touching readers | Any change breaks old readers | Stable |
| Risk | Table and text can disagree if a writer is wrong | Two readers can disagree | None |

**Chosen: A.**

## Trade-off analysis

- **B** needs no table, but it makes an unspecified algorithm part of every
  reader. Any difference in Unicode case mapping or punctuation handling
  produces links that work in one reader and not in another.
- **C** is exact and simple, and was dropped together with the dialect that
  supported it.
- **A** runs the algorithm once, in the writer, where the result is checked
  against every link of the book. The cost is one small table, and a class of
  writer bugs in which the table and the text disagree.

Byte offsets were chosen over character or block indices because they need
no parsing to apply and are the value a parser already reports for a heading.

## Consequences

- Easier: links never depend on a reader's Markdown implementation.
- Easier: contents entries, links and bookmarks share one kind of position.
- Easier: a contents row with a non-zero offset must match an anchor row
  (decided 2026-11-21), so a contents target is a path and an optional anchor
  name, and an editing library never handles raw offsets
  ([RFC 0002](../0002-mbook-core-api.md#navigation)).
- Harder: editing a chapter invalidates saved positions and requires
  rebuilding `toc` and `anchors`.
- To revisit: whether the GitHub naming rule should be specified exactly for
  writers, or stay a recommendation.

## Action items

1. [ ] Packer: generate anchor names and verify every link fragment.
2. [ ] Packer: check that every offset is at the start of a line.
3. [ ] Reader: resolve fragments by exact byte comparison after
   percent-decoding.
