---
name: mbook-authoring
description: Write and edit books in the mbook application through its MCP tools - chapters in Markdown, pictures, Mermaid diagrams, KaTeX formulas, metadata, contents and landmarks - without looking at the screen.
---

# Writing books in mbook

mbook is a format for books: chapters in Markdown, pictures in AVIF,
diagrams in Mermaid and formulas in KaTeX, packed in one file. The mbook
application keeps a library of such books, and its MCP server lets you read
and change them. Everything you need to know about a book comes back as
text, so you never need to see the screen.

## How a book is named

- **A book** is `book`, the key `list_books` and `create_book` give (a
  UUID). It is not the `id` in the metadata.
- **A chapter, a picture, a diagram or a formula** is its path in the book:
  `ch01.md`, `img/fig-1.avif`, `dia/0001.mmd`, `math/0001.tex`.
- **A place** is an `href`, written as a link of the book writes it:
  `ch01.md` or `ch01.md#the-station`. The part after `#` is the anchor of a
  heading, or a name an anchor definition gives (see `dialect`).
- Lines and columns count from 1.

## The order of work

1. `list_books` to find the book, or `create_book` for a new one.
2. `get_book` for its structure: metadata, chapters with their headings and
   anchors, contents, landmarks, pictures and figures with the lines that
   use them. It holds no text and no pictures.
3. `read_chapter` for the text you need. Long chapters can be read by
   section (`anchor`) or by `lines`.
4. Change the book: `edit_chapter` for a part of a chapter, `write_chapter`
   for all of it, `add_chapter`, `add_picture`, `put_figure`, `set_meta`,
   `set_contents`, `set_landmarks`.
5. `check_book`, and fix every problem it lists.
6. `save_book`. Nothing reaches the file before this, and a book with
   problems is not written.

Every change answers with the problems of the chapter it touched, with line
and column: fix a broken link at once rather than at the end.

## Rules you will need at once

- Chapters are GitHub Flavored Markdown **without raw HTML**. A tag such as
  `<br>` or `<div>` is a problem, not markup.
- A picture alone in its paragraph is a figure; the title of the link is its
  caption: `![The station at night](img/fig-1.avif "Fig. 1. The station")`.
- Write diagrams as a `mermaid` code fence and formulas as `$…$` or
  `$$…$$`. They are moved into resources of the book when the text is
  stored, and drawn first: one that cannot be drawn refuses the change.
  Start a diagram with `%% alt: …` (what it shows, in words) and, for a
  figure, `%% caption: …`.
- Every picture, diagram and formula needs an alternative text that says
  in words what it shows. It is all a reader without a screen gets.
- `edit_chapter` matches `old` against the stored text, in which diagrams
  and formulas are references such as `![…](dia/0001.mmd)`, not fences.
  Read before you edit.
- The person may be editing the same book in the application. When a call
  answers `user_typing`, wait a few seconds and try again. When
  `write_chapter` answers `conflict`, read the chapter again.
- Removing a chapter, a picture, a figure or a book, discarding changes,
  importing a file, adding a picture from a file and exporting to a file
  are shown to the person in a dialog. They may allow that kind of action
  for the rest of your session. When they decline, the call answers `declined`:
  do not ask again unless they tell you to. A picture you can give as SVG or
  base64 needs no dialog.

## More

Call `get_guide` with one of these topics, or read the files beside this
one:

| Topic | File | Says |
| --- | --- | --- |
| `dialect` | `references/dialect.md` | What a chapter may contain, anchors, footnotes, paths |
| `figures` | `references/figures.md` | Diagrams and formulas: how to write them, alternative text, previews |
| `pictures` | `references/pictures.md` | Adding pictures from a file, base64 or SVG; the cover; looking at a picture |
| `structure` | `references/structure.md` | Metadata, contents, landmarks, moving and naming chapters |
| `errors` | `references/errors.md` | Every error code and what to do about it |
