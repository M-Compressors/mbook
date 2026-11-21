# ADR-0008: Anchors that are not headings live in a table before their chapter

**Status:** Accepted; the points under "Proposed details" are proposed\
**Date:** 2026-11-21\
**Deciders:** Format author

## Context

[ADR-0007](0007-positions-and-anchors.md) gives every heading a name in
resource 4, and a link reaches it as `ch01.md#the-station`. Books in other
formats link to more than headings: EPUB and FB2 let any element carry an
identifier, and cross-references such as "see figure 3" or an index entry
lead to a paragraph, a figure or a row of a table.

A converter ([RFC 0003](../0003-mbook-converter-api.md)) could only send such
a link to the nearest heading above its target. The format itself did not
forbid other rows in resource 4, but that table is decoded whenever a file is
opened: a reference work with a name on every paragraph would pay for all of
them before showing its title.

## Decision

A link destination stays what it is: a chapter path and, after `#`, a name
inside that chapter. The names of headings stay in resource 4. Every other
named position of a chapter is stored in an *anchor table* of that chapter:
a resource of its own, placed immediately before the chapter.

## Options considered

| Dimension | A: a table before each chapter; headings stay in resource 4 | B: every anchor in resource 4 | C: every anchor in a table before its chapter; no resource 4 | D: no new anchors; links go to the nearest heading |
| --- | --- | --- | --- | --- |
| Reader complexity | One more table shape; a fragment is looked up in two places | Unchanged | One table shape; a fragment always needs a second frame | Unchanged |
| Cost of opening a file | Grows with headings only | Grows with every anchor of the book | Constant | Grows with headings only |
| Cost of following a link | One more small frame, next to the target chapter, for a name that is not a heading | None | One more small frame for every fragment | None |
| File size | A record and a table header per chapter that has such anchors, about 150 bytes | A few bytes per anchor | The same overhead for every chapter with a heading | 0 |
| What can be checked when a file is opened | Contents rows and links to headings | Everything | Nothing about fragments | Everything |
| Precision of converted links | Exact, to the line | Exact, to the line | Exact, to the line | The heading above the target |

**Chosen: A.**

## Trade-off analysis

- **D** needs no change and loses information: a link to a paragraph lands on
  the heading above it. Technical and scholarly books are mostly made of such
  links.
- **B** is the smallest change, and makes the cost of opening a file depend
  on how densely a converter names positions.
- **C** has one mechanism instead of two, but a reader could no longer check
  the contents against the anchors, or resolve a link to a heading, without
  reading a frame per chapter.
- **A** keeps what a reader needs at once (headings, for the contents and for
  most links) where it is read at once, and moves the open-ended rest next to
  the chapter it describes. The cost is two places to look a name up, and one
  more frame to decode when a link names a place that is not a heading.

A separate frame was chosen over a second output of the chapter's own frame,
so that a link can be resolved without decoding the text, and the text
without decoding the names.

## Proposed details

Decided by the format author: the two-part destination, the table in its own
frame before the chapter, and resource 4 kept for headings. Written into
RFC 0001 as proposals:

| Detail | Proposal |
| --- | --- |
| How the table is marked | A new `kind`, 3, in the resource table |
| How it is tied to its chapter | By position: the next resource is the chapter. No path, no extra column |
| Its columns | `offset` (`u32`), `name` (string) |
| Names | One namespace per chapter: a name is unique across resource 4 and the chapter's table |
| Contents rows | Still match a row of resource 4, so that they stay checkable when a file is opened |
| The start-of-line rule | Checked when the table and the text of the chapter are both decoded |

## Consequences

- Easier: a converter keeps the exact target of every internal link.
- Easier: opening a book costs the same however many places a chapter names.
- Easier: a chapter and the names inside it sit next to each other in the
  file, so one read fetches both.
- Harder: a reader resolves a fragment in two steps, and the second one does
  I/O.
- Harder: a reader that opens a file lazily cannot tell a link to a missing
  name from a link to a name of the chapter's table without reading that
  table.
- Open here, settled by [ADR-0009](0009-anchor-definitions.md): where a
  writer learns the names. They are written in the Markdown of the chapter.
- To revisit: whether a contents row may lead to a position of a chapter's
  table.

## Action items

1. [x] Reader: accept `kind` 3, tie the table to the next resource, and
   resolve fragments through it.
2. [x] Writer: emit the table before a chapter that has such anchors, and
   none otherwise.
3. [ ] Converter: name the targets of internal links
   ([RFC 0003](../0003-mbook-converter-api.md#links-and-footnotes)).
