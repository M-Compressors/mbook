# ADR-0009: A line is named by a link reference definition in its chapter

**Status:** Accepted; the points under "Proposed details" are proposed\
**Date:** 2026-11-21\
**Deciders:** Format author

## Context

[ADR-0008](0008-chapter-anchor-tables.md) lets a link lead to a line that is
not a heading, through a table of names and byte offsets in front of the
chapter. It did not say where a writer learns those names.

The first implementation took them from the caller: a name and a byte offset
for a text already given. An offset belongs to one version of a text, so
replacing the text had to drop every name of the chapter. Correcting one
letter of a converted book removed all its link targets, and nothing in the
new text could bring them back: the dialect
([ADR-0006](0006-markdown-dialect.md)) has no way to mark a place. A book
unpacked into a folder lost the names as well, because they existed only in
the table.

## Decision

A chapter names a line in its own Markdown, with a link reference definition
that names itself, placed in front of the line:

```markdown
[fig-1]: #fig-1
![View of the pond](img/fig-3.avif "Fig. 3. Patriarch's Ponds")
```

A writer finds these *anchor definitions* when it parses the chapter and
fills the chapter's anchor table from them, as it fills resource 4 from the
headings. Readers keep resolving fragments through the tables only.

## Options considered

| Dimension | A: a link reference definition that names itself | B: `{#id}` attributes kept in the chapter | C: `{#id}` accepted by the writer and removed from the stored text | D: names and offsets given by the caller | E: as D, with offsets carried across an edit by comparing lines |
| --- | --- | --- | --- | --- | --- |
| Reader complexity | None: CommonMark renders nothing for a definition | Every reader must find and hide the attribute | None | None | None |
| Compatibility | Plain GFM; invisible on GitHub and in any viewer | Literal `{#id}` text everywhere but in mbook readers | Plain GFM | Plain GFM | Plain GFM |
| Survives an edit of the chapter | Yes: the name is part of the text | Yes | Only through a second form of the text that still has the marks | No | Mostly; a rewritten line is a guess |
| Survives unpacking into a folder | Yes | Yes | No | No | No |
| File size | One short line per name, besides the table row | A few bytes per name, besides the table row | The table row | The table row | The table row |
| Risk | A definition written for another purpose is taken for a name | A second dialect to specify | Two texts of one chapter that can disagree | An offset that outlives its text | A name that silently moves to another line |

**Chosen: A.**

## Trade-off analysis

- **B** is the syntax of the first draft of the format. It was dropped with
  ADR-0006 because it is not GFM: a chapter would show `{#fig-1}` wherever it
  is viewed outside an mbook reader, and every reader would need a parser
  extension to hide it.
- **C** keeps the stored text clean and moves the problem into the library:
  the text a program reads back has no marks, so editing it loses the names
  unless a second accessor returns the marked text.
- **D** was implemented first. It works for a converter, which builds a text
  once, and fails for any later edit.
- **E** repairs D with a heuristic. A name follows its line exactly when the
  line is unchanged, and by position when it was rewritten, which can be
  wrong without any error.
- **A** uses a construct every CommonMark parser already has and every
  renderer already hides. The name travels with the text through edits,
  through a folder of Markdown files and through any tool that handles GFM.
  The table of ADR-0008 stays what a reader uses; it is derived data, like
  the anchors of headings.

A link reference definition can also be used as intended: `[the figure][fig-1]`
in the same chapter is an ordinary link to `#fig-1`, and resolves to the
line the definition names.

## Proposed details

Decided by the format author: names are written in the Markdown of the
chapter, as link reference definitions. Written into RFC 0001 as proposals:

| Detail | Proposal |
| --- | --- |
| Which definitions are anchor definitions | Those whose destination is `#` followed by their own label, compared byte for byte after percent-decoding the destination |
| The position named | The first byte of the line on which the definition starts |
| A name that a heading of the chapter already has | The definition is an ordinary one and names nothing |
| A name defined twice | The first definition names its line; later ones name nothing |
| Rows of the table without a definition | Allowed; a reader does not compare the table with the text |
| A title on the definition | Allowed and ignored |

## Consequences

- Easier: an editing library computes every offset from the text again, and
  an edit keeps the names it does not delete
  ([RFC 0002](../0002-mbook-core-api.md#extra-anchors)).
- Easier: unpacking a book into a folder and packing it again keeps every
  link target.
- Easier: a converter writes one line of Markdown per link target instead of
  tracking byte offsets ([RFC 0003](../0003-mbook-converter-api.md#links-and-footnotes)).
- Harder: a writer must know two rules of CommonMark. A definition cannot
  interrupt a paragraph, so it needs an empty line or the start of a block
  before it; and a following line that is only a quoted or parenthesised
  string would be read as its title, so such a line needs an empty line
  before it.
- Harder: a row of a table cannot be named, because a definition cannot
  stand inside a GFM table. The table as a whole can.
- To revisit: whether the self-naming rule is too easy to meet by accident.

## Action items

1. [x] Writer: collect anchor definitions when a chapter is parsed and fill
   its anchor table from them.
2. [x] Library: remove the calls that took a name and an offset.
3. [ ] Converter: emit an anchor definition in front of every link target
   that is not a heading.
