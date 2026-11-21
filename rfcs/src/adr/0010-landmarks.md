# ADR-0010: Landmarks are a table of their own, present only when a book has them

**Status:** Accepted; the points under "Proposed details" are proposed\
**Date:** 2026-11-21\
**Deciders:** Format author

## Context

A book has places a reader wants to reach without looking for them: where
the text begins, the glossary, the index, the notes. The table of contents
does not answer "where is the index": it lists titles, in the order of the
text, and an application cannot tell which entry is which. EPUB solves this
with a second list next to the contents, the *landmarks*, in which every
entry has a kind from a fixed vocabulary. A converter
([RFC 0003](../0003-mbook-converter-api.md)) had nowhere to put that list.

## Decision

A book may carry *landmarks*: a list in which each row has a kind, an
optional title and a place in the book. The place is what a link names: a
chapter, and optionally a named position inside it. The list is stored as a
table in a resource of its own, of a new `kind`, 4, and a book without
landmarks has no such resource.

## Options considered

| Dimension | A: an optional resource of a new kind | B: a sixth fixed resource | C: keys of `meta` |
| --- | --- | --- | --- |
| Reader complexity | One more table shape; found through the resource table | One more table shape; found by number | None: a key and a link destination |
| Cost of opening a file | None; the table is decoded when it is wanted | One more frame for every book | None |
| File size | About 150 bytes, only in books that have landmarks | About 150 bytes in every book | A few bytes per landmark |
| Compatibility | A new `kind` is a compatible addition; no number changes | Every resource number after 4 changes; existing files and examples become invalid | A new key is a compatible addition |
| Evolution | Columns can be added at the end | Columns can be added at the end | A landmark is one string: no room for a title or for more |

**Chosen: A.**

## Trade-off analysis

- **B** makes landmarks as easy to read as the contents, and charges every
  book for a table most books do not have. It also moves the cover and
  every chapter up by one number, in a format whose first five resources
  were fixed on purpose ([ADR-0003](0003-format-header.md)).
- **C** is how the cover is named today. A landmark, though, needs a title
  that an application can show ("Glossary of terms"), may repeat (several
  appendices), and is navigation, not a description of the book.
- **A** follows the anchor tables of chapters
  ([ADR-0008](0008-chapter-anchor-tables.md)): what only some books need
  lives in a resource that only those books have, and the resource table
  already says where it is.

## Proposed details

Decided by the format author: landmarks exist, their place is named like the
target of a link, and they are an optional resource of `kind` 4 with the
columns `kind`, `title`, `resource`, `offset`. The list of kinds was settled
on 2026-11-21 as well:

- It is the structural vocabulary of EPUB, with the names written out
  (`list-of-figures` for `loi`, `start` for `bodymatter`).
- The narrative parts are kinds of their own: `introduction`, `prologue`,
  `epilogue`, `conclusion`.
- `about-the-author` is added, although EPUB has no such type: it is among
  the places books most often have.
- A table of contents set in the text is not a kind. The `toc` table is the
  contents of a book, and a second one in the text only repeats it.
- `acknowledgements` is spelled the British way; EPUB writes
  `acknowledgments`.
- The three kinds of notes of EPUB (`footnotes`, `endnotes`, `rearnotes`)
  are one kind, `notes`.

Written into RFC 0001 as proposals:

| Detail | Proposal |
| --- | --- |
| How many such resources | At most one per file |
| Where it stands | Directly after resource 4, before the cover |
| The form of a kind | That of a `meta` key: `[a-z0-9]+(-[a-z0-9]+)*`, with `x-` for private use |
| Repetition | Any kind may repeat, except `start` |
| A position inside a chapter | Must match a row of `anchors` or of the chapter's anchor table, so that it can be written as a link destination |
| An unknown kind | Shown by its title; skipped when it has none |
| A damaged table | An error confined to the landmarks: the book is still read |

## Consequences

- Easier: an application can offer "go to the beginning", "glossary" and
  "index" without guessing from titles.
- Easier: a reader that opens a book for the first time can start at the
  text instead of the cover page.
- Easier: the landmarks of an EPUB survive conversion.
- Harder: a reader has a second list of places besides the contents, and
  the two can disagree.
- Harder: the names of kinds are a vocabulary this format now maintains.
- To revisit: whether `start` should also be reachable without decoding a
  table, as a `meta` key.

## Action items

1. [x] Reader: accept `kind` 4, at most once, and read the table on request.
2. [x] Writer: emit the table after resource 4 when a book has landmarks.
3. [x] Converter: carry over the landmarks of EPUB 3 and the guide of
   EPUB 2.
