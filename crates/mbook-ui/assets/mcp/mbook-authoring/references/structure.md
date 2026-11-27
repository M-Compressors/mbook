# Metadata, contents and landmarks

## Metadata

`set_meta` takes a patch. A field you leave out stays as it is; `null` or an
empty list removes it.

| Field | Holds |
| --- | --- |
| `title` | The title; never empty |
| `authors`, `translators` | Names as they are shown, in order |
| `language` | The main language, a BCP 47 tag such as `en` or `ru` |
| `series` | `{"name": "…", "index": "2"}`; the index is a decimal number or `null` |
| `publisher` | The publisher |
| `date` | `YYYY`, `YYYY-MM` or `YYYY-MM-DD` |
| `isbn` | ISBN-13 or ISBN-10, digits only |
| `genres` | Free-form genres or tags |
| `annotation` | A description in plain text; paragraphs are separated by an empty line |

The cover is set with `add_picture` and `cover: true`.

## Contents

The contents a reader sees come either from the headings of the chapters or
from a list the book keeps.

- `set_contents` with `{"automatic": true}` follows the headings. This is
  what a new book does, and usually what you want.
- `set_contents` with `{"entries": [{"level": 1, "title": "Arrival", "href":
  "ch01.md"}, {"level": 2, "title": "The station", "href":
  "ch01.md#the-station"}]}` keeps that list. An entry leads to a chapter or
  to a heading of it, not to an anchor definition. Levels start at 1, and
  each is at most one deeper than the entry before.

An entry that leads nowhere is a problem `check_book` lists.

## Landmarks

Landmarks say where places of a certain kind are, so a reader can offer "go
to the beginning" or "index".

```json
{"landmarks": [
  {"kind": "start", "title": "", "href": "ch01.md"},
  {"kind": "glossary", "title": "Glossary of terms", "href": "back/glossary.md"}
]}
```

- `start` is where the text begins; a book has at most one. The other
  kinds are `title-page`, `dedication`, `epigraph`, `foreword`, `preface`,
  `introduction`, `prologue`, `epilogue`, `conclusion`, `afterword`,
  `acknowledgements`, `appendix`, `notes`, `glossary`, `bibliography`,
  `index`, `list-of-figures`, `list-of-tables`, `about-the-author`,
  `copyright` and `colophon`; they may repeat. A kind
  of your own begins with `x-`.
- A landmark may lead to a heading or to an anchor definition.
- `set_landmarks` replaces the whole list; an empty list removes them.

## Chapters

- `add_chapter` puts a chapter after the last one, or at `position`
  (counted from 0). Without `path` it gets the next free `chNN.md`. Start
  the text with a heading of level 1: it is the title of the chapter.
- `move_chapter` changes the position, the path, or both. The contents and
  the landmarks follow the chapter; links in other chapters do not, and are
  then problems that `check_book` lists.
- `remove_chapter` removes it after the person agrees, with the entries of
  the contents and the landmarks that led into it.
