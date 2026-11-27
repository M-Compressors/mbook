# The mbook format

What an mbook file is, in short: a summary of RFC 0001, version 1.0, a
draft. The tools of this server keep every rule below for you; read this to
know why a call answers as it does.

- The specification: <https://m-compressors.github.io/mbook/rfcs/0001-mbook-format.html>
- Why it is so, one decision at a time: <https://m-compressors.github.io/mbook/rfcs/adr/overview.html>

## What a book is

A book is Markdown chapters, AVIF pictures, Mermaid diagrams and KaTeX
formulas in one file. Unpacked, it would be a folder any Markdown viewer can
show:

```text
ch01.md
ch02.md
img/cover.avif
img/fig-3.avif
dia/route.mmd
math/speed.tex
```

Packed, it is one container of independent compressed frames with an index:
one frame per chapter, picture, diagram and formula, so a reader opens a
book without reading all of it and decodes only what is on screen. Text and
tables are compressed with OpenZL; pictures are AVIF files stored as they
are.

The first five frames are fixed:

| Frame | Holds |
| --- | --- |
| 0 | The format header: `MBOOK`, a zero byte, major version 1, minor 0 |
| 1 `meta` | The metadata, as key and value |
| 2 `resources` | One row per frame: its kind, its path, and for a picture its width and height |
| 3 `toc` | The contents: level, title, chapter, byte offset |
| 4 `anchors` | The names of the headings of every chapter |

Then, in this order: the landmarks when the book has any, the cover, the
chapters in reading order, the other pictures, the diagrams and the
formulas. A chapter that names lines other than headings is preceded by a
small table of those names.

## Kinds of resource

| Kind | What | Path, by custom |
| --- | --- | --- |
| 1 | A chapter: Markdown | `ch01.md` |
| 2 | A picture: an AVIF file | `img/fig-1.avif` |
| 3 | The names of lines of the next chapter | none |
| 4 | Landmarks | none |
| 5 | A diagram: Mermaid source | `dia/route.mmd` |
| 6 | A formula: KaTeX source, without `$` | `math/speed.tex` |

The reading order is the order of the chapters. The size of every picture is
in the table, so a reader lays out a page before it decodes a picture.

## Paths

- Segments joined by `/`. No segment is empty, `.` or `..`, and none holds
  `/`, `\` or a control character. A path never starts or ends with `/`.
- Paths are unique in a book. Lower case, digits, `.`, `_` and `-` are best;
  generated names are numbered: `img/0001.avif`.
- A chapter refers to another resource by a path relative to its own
  folder.

## Metadata

| Key | Count | Value |
| --- | --- | --- |
| `title` | exactly 1 | Not empty |
| `lang` | 1 or more | BCP 47; the first is the main language |
| `id` | exactly 1 | Stays the same across re-packings; a `urn:uuid:` |
| `author`, `translator` | 0 or more | A name as shown, in order |
| `series`, `series-index` | 0 or 1 | A name; a decimal number such as `2.5` |
| `publisher` | 0 or 1 | |
| `date` | 0 or 1 | `YYYY`, `YYYY-MM` or `YYYY-MM-DD` |
| `isbn` | 0 or more | Digits only |
| `genre` | 0 or more | Free-form |
| `annotation` | 0 or 1 | Plain text; paragraphs separated by an empty line |
| `cover` | 0 or 1 | The path of a picture |

A reader ignores keys it does not know; `x-` keys are for private use.

## A chapter

- UTF-8 without a byte order mark, lines ending in a line feed, no control
  character but tab and line feed. Under 256 KiB is best: split at
  top-level headings.
- CommonMark 0.31.2 with the extensions of GitHub: tables, task lists,
  strikethrough, extended autolinks and footnotes. A footnote is defined in
  the chapter that refers to it.
- Never in a chapter: raw HTML, pictures from the network, links with a
  scheme other than `http`, `https` and `mailto`.
- Not part of the dialect, so shown as plain text: math with `$`, alert
  blocks such as `> [!NOTE]`, heading attributes `{#id}`, wiki links, emoji
  shortcodes, front matter. A `mermaid` code block is just a code block.
  This server moves the Mermaid fences and `$…$` you write into resources
  for you (see `figures`).
- A paragraph that holds only one picture, diagram or formula is a figure;
  the title of the reference is its caption. A formula that is a figure is
  a display formula.
- The alternative text of a picture, a diagram or a formula is what a
  reader shows or speaks when it cannot draw it.

## Links and anchors

- `[text](ch02.md)` leads to the start of a chapter, `[text](ch02.md#name)`
  to a named place in it, `[text](#name)` to one in the same chapter.
- A heading is named the way GitHub names it: lower case, punctuation
  dropped, spaces as `-`, `-1`, `-2`… for repeats. `Chapter 3. The Seventh
  Proof` is `chapter-3-the-seventh-proof`.
- Any other line is named by an anchor definition on the line before it, a
  link reference definition that names itself: `[fig-1]: #fig-1`. It
  follows an empty line or starts a block.
- A reader finds a place through the tables of the file, never by searching
  the text; a link that leads nowhere is an error a writer must not leave.
- Places are byte offsets at the start of a line, so a book whose chapters
  change is a new edition: saved reading places may move.

## Contents and landmarks

- The contents are a tree of entries, listed in reading order. Each leads to
  a chapter or to a heading of it, never to another named line.
- Landmarks say where places of a kind are: `start` (where the text begins;
  at most one), `title-page`, `dedication`, `epigraph`, `foreword`,
  `preface`, `introduction`, `prologue`, `epilogue`, `conclusion`,
  `afterword`, `acknowledgements`, `appendix`, `notes`, `glossary`,
  `bibliography`, `index`, `list-of-figures`, `list-of-tables`,
  `about-the-author`, `copyright`, `colophon`, or an `x-` kind. A landmark
  may lead to any named place. A reader opens a new book at `start`.

## Diagrams and formulas

- Stored as their source, not as pictures of it: Mermaid 11 for a diagram,
  KaTeX 0.16 for a formula. A reader draws them in the size and the colours
  of its page.
- A chapter shows them with the syntax of a picture:
  `![From the station to the letter](dia/route.mmd "The way of the letter")`.
- A reader may draw none, some or all of them, and shows the alternative
  text for the rest. It never follows a `click` of Mermaid or `\href` of
  KaTeX, and never fetches anything for them.

## What a reader may rely on

- No scripts, no styles, no network: a chapter cannot carry active content,
  and pictures cannot name a network location.
- Fonts, sizes, margins and pagination belong to the reader; the format
  carries no style.
- Every frame has a checksum. Nothing authenticates the origin of a file.
- A reader skips what a newer minor version adds: unknown kinds, unknown
  keys, columns at the end of a table.
