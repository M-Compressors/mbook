# What a chapter may contain

A chapter is GitHub Flavored Markdown, UTF-8, with lines ending in a line
feed.

## Allowed

- Headings `#` to `######`, paragraphs, emphasis, strong, strikethrough,
  code spans, fenced code blocks, block quotes, lists, task lists, tables,
  thematic breaks, footnotes.
- Hard line breaks: a backslash at the end of a line.
- Links to other chapters (`ch02.md`, `ch02.md#a-heading`), to places in the
  same chapter (`#a-heading`), and to `http`, `https` and `mailto`
  addresses.
- Pictures, diagrams and formulas, written with the syntax of a picture:
  `![alternative text](img/fig-1.avif "Caption")`.

## Not allowed

- Raw HTML of any kind: tags, comments, entities that stand for markup.
- Control characters other than tab and line feed.
- Links and pictures that lead nowhere: every path must be a chapter, a
  picture, a diagram or a formula of the book, and every `#name` an anchor
  of that chapter.
- Pictures from the network. A picture is always a resource of the book.

## Headings and anchors

Every heading names itself, the way GitHub names it: lower case, spaces as
`-`, punctuation dropped. `## The Seventh Proof` is `#the-seventh-proof`.
`get_book` lists the anchors each chapter has, so you do not have to work
them out.

Any other line can be named by an **anchor definition** on the line before
it, a link reference definition that names itself:

```markdown
[fig-1]: #fig-1
![The station at night](img/fig-1.avif "Fig. 1. The station")
```

Nothing is shown for the definition. Another chapter links to the figure as
`[the figure](ch01.md#fig-1)`. A name that a heading of the chapter already
has, or an earlier definition, adds nothing.

## Figures and captions

A paragraph that holds only one picture, diagram or formula is a figure. The
title of the link is its caption. A picture inside a line of text is set in
the line.

## Footnotes

```markdown
Text with a note[^1].

[^1]: The text of the note.
```

## Paths

- A path is segments joined by `/`. A segment is not empty, `.` or `..`, and
  holds no `/`, `\` or control character.
- Use lower case letters, digits, `.`, `_` and `-`: `ch01.md`,
  `img/fig-1.avif`.
- A path in a link is relative to the chapter that holds the link. A chapter
  `part-1/ch01.md` links to a picture `img/x.avif` as `../img/x.avif`.
- Chapters end in `.md`, pictures in `.avif`, diagrams in `.mmd`, formulas in
  `.tex`.
