# Conversion rules

What the converter does with a source, from the first byte to the finished
`Book`. The rules of [Assembling the book](#assembling-the-book) are the same
for every format; [Rules per format](#rules-per-format) says what each front
end hands over, and [What is implemented](#what-is-implemented) what it cannot
do yet.

## Sources and memory

The converter accepts a forward-only `Read` and a slice.

- **A `Read` is read to its end into memory** before anything else happens,
  bounded by `ConvertLimits::max_source_bytes`. Three things need all of it:
  the format is detected from the content, an `id` may be computed from every
  byte, and most front-end crates want the whole source.
- **A slice is used in place.** No copy of the source is made by this crate.
  A front-end crate may still make one: `rbook` wants an owned source.
- After that, every front end starts from the same thing, the bytes of the
  source.

Consequences that hold for every format:

- **Peak memory is about the source plus the converted book.** The result is
  a `Book`, and a `Book` holds every chapter and picture
  ([`mbook`, Drawbacks](../mbook/design.md#drawbacks)). Conversion is streaming at
  its edges, a `Read` in and a `Write` out; it is not a constant-memory
  pipeline.
- The pictures of the source are held, as the front end extracted them,
  until the book is assembled.

## Detection

When `ConvertOptions::format` is `None`, the format is taken from the
content.

| Format | Recognised by |
| --- | --- |
| PDF | `%PDF-` within the first 1,024 bytes |
| EPUB | A ZIP whose `mimetype` entry is `application/epub+zip`, or that contains `META-INF/container.xml` |
| DOCX | A ZIP that contains `word/document.xml` |
| FB2 in a ZIP | A ZIP with exactly one entry whose name ends in `.fb2` |
| CBZ | Any other ZIP in which every entry is a picture, a folder or `ComicInfo.xml` |
| FB2 | An XML document whose root element is `FictionBook` |
| HTML | A document that starts with `<!DOCTYPE html` or `<html`, in any letter case |
| Markdown, text | Never detected; only when named |

- Rows are tried in this order; the first match wins.
- A file name is never used: a stream has none.
- A source that matches no row fails with `Error::UnknownFormat`. A source
  that matches a format whose feature is off fails with
  `Error::FormatDisabled`.

## The draft

A front end is private code. It produces the same things for every format,
and nothing else crosses into the assembler:

| Part | Content |
| --- | --- |
| Metadata | Values for the keys of [`meta`](../rfcs/0001-mbook-format.md#resource-1-meta), unchecked |
| Sections | Markdown in reading order; each section has the source's name for it (a file of an EPUB, a section of an FB2 document) and may contain raw HTML and destinations in the source's own terms |
| Marks | An element of the source that a link may lead to is marked in the Markdown of its section with an [anchor definition](../rfcs/0001-mbook-format.md#anchor-definitions) in front of its block, named by the identifier it has in the source |
| Pictures | For each source name: the picture as the source has it |
| Cover | The source name of the picture that is the cover |
| Outline | Optional: level, title and a destination in the source's own terms |
| Landmarks | Optional: a kind of RFC 0001, a title and a destination in the source's own terms |
| Warnings | What the front end itself could not keep |

A front end must not build chapters, paths or tables itself.

## Assembling the book

The steps below are the conversion rules of this crate. They do not depend on
the source format.

### Chapters and paths

- One section becomes one chapter. A front end chooses its sections so that
  each starts at a top-level division of the source
  ([Rules per format](#rules-per-format)).
- A chapter above 256 KiB must be split, as RFC 0001
  [recommends](../rfcs/0001-mbook-format.md#encoding): at its top-level headings
  first, then at the paragraph boundary nearest to the limit.
- A section without text and without a picture produces no chapter.
- Chapters are named `ch0001.md`, `ch0002.md`, … in reading order. Pictures
  are named `img/0001.avif`, `img/0002.avif`, … in the order of their first
  use, and the cover `img/cover.avif`.

### Markdown

Every chapter is parsed with `markdown` 1.0.0, configured for the
[dialect](../rfcs/0001-mbook-format.md#dialect) as in `mbook`, and changed at the
positions the parser reports:

| Found | Becomes | Warning |
| --- | --- | --- |
| Inline `<br>` | A hard line break, written `\` and a line feed | No |
| Any other raw inline HTML tag | Nothing; the text between the tags stays | `MarkupDropped`, once per tag name |
| An HTML block | Nothing | `MarkupDropped` |
| A picture whose source name the draft knows | A picture with the new path | No |
| A picture the draft does not have, cannot decode, or that names a network address | Its alternative text | `PictureDropped` |
| A link with scheme `http`, `https` or `mailto` | Unchanged | No |
| A link with any other scheme | Its text | `LinkDropped` |
| A link into the book | See [Links and footnotes](#links-and-footnotes) | |
| A character below U+0020 other than line feed and tab | Nothing | No |

Text is normalised to Unicode NFC.

### Links and footnotes

Source formats link to any element. mbook names a heading by itself and any
other line through an
[anchor definition](../rfcs/0001-mbook-format.md#anchor-definitions) in front of it
([ADR-0009](../rfcs/adr/0009-anchor-definitions.md)), so the target of a link is kept
to the line.

| Target in the source | Destination after conversion | Warning |
| --- | --- | --- |
| A file, a section or a page | The chapter that starts there | No |
| A heading | That heading: the chapter's path and the anchor `mbook` computed | No |
| Any other block: a paragraph, a figure, a list item, a block quote, a table | That block, named by an anchor definition in front of it | No |
| An element inside a block, such as a word of a paragraph or a row of a table | The block that contains it, named the same way | No |
| Nothing that was converted | The link becomes its text | `LinkDropped` |

- A front end marks every element that has an identifier. The assembler
  keeps a mark only when a link of the book, or an entry of the contents,
  leads to it, and removes the others. `mbook` finds the definitions
  that are kept and fills the anchor table of the chapter; the converter
  handles no byte offsets.
- The name is the identifier of the source, with everything but letters,
  digits, `-`, `_` and `.` replaced by `-`. When the chapter already has an
  anchor with that name, of a heading for example, `-2`, `-3`, … is
  appended; every link is rewritten anyway, so a name need not survive.
- When an identifier is used twice in a section, the first element that
  carries it is the target.
- Links are re-targeted after every chapter has its final text: the anchor
  of a heading is known only then.
- A link that the source marks as a note reference (`epub:type="noteref"` in
  EPUB, `<a type="note">` in FB2) becomes a footnote: `[^n]` at the
  reference and `[^n]: text` at the end of the same chapter, as RFC 0001
  [requires](../rfcs/0001-mbook-format.md#dialect). The text of the note is copied
  into every chapter that refers to it and is removed from where it stood.

### Pictures

| Source picture | Stored as |
| --- | --- |
| An AVIF file that `Book::add_picture` accepts | The same bytes |
| JPEG, PNG, GIF, WebP, BMP | Decoded, turned upright by its orientation tag, encoded as AVIF |
| An animated GIF or WebP | Its first frame, encoded the same way |
| Anything else (SVG, JPEG 2000, TIFF, WMF) | Not stored; see [Markdown](#markdown) |

- A picture is stored once, however often it is used. Two source names with
  the same bytes share one resource.
- A picture that no chapter shows and that is not the cover is not stored.
- Before decoding, the pixel count declared by the picture must be
  compared with `ConvertLimits::max_picture_pixels`.
- Encoding uses `PictureOptions::quality` and `PictureOptions::speed`.

### Metadata

Values come from the front end and are passed to the setters of
[`Meta`](../mbook/api.md#meta). A value that a setter refuses is
dropped with a `MetadataDropped` warning; it does not fail the conversion.

| Key | Rule |
| --- | --- |
| `title` | The source's title; otherwise the text of the first top-level heading; otherwise `Untitled`, with a `MetadataMissing` warning |
| `lang` | The source's language tags; otherwise `und`, with a `MetadataMissing` warning |
| `id` | The source's identifier: a UUID becomes `urn:uuid:` and the UUID in lower case, anything else is kept. Without one: a UUID version 5 computed from the source bytes, so that converting the same file twice gives the same `id` |
| `date` | Shortened to `YYYY`, `YYYY-MM` or `YYYY-MM-DD` |
| `isbn` | Digits only; an identifier of the source that is a valid ISBN also fills this key |
| `annotation` | Plain text: markup is removed and paragraphs are separated by an empty line |
| `cover` | Set by `Book::set_cover` when the source names a cover picture that could be stored |

Where each key comes from:

| Key | EPUB | FB2 | PDF | CBZ (`ComicInfo.xml`) | DOCX | HTML |
| --- | --- | --- | --- | --- | --- | --- |
| `title` | `dc:title` | `book-title` | `Title` | `Title` | Title | `title` |
| `lang` | `dc:language` | `lang` | | `LanguageISO` | `dc:language` | `lang` of `html` |
| `id` | The unique `dc:identifier` | `document-info/id` | | | | |
| `author` | `dc:creator` | `title-info/author` | `Author` | `Writer` | Creator | `meta` `author` |
| `translator` | `dc:contributor` with role `trl` | `translator` | | `Translator` | | |
| `series`, `series-index` | `belongs-to-collection` and `group-position`, or `calibre:series` | `sequence` | | `Series`, `Number` | | |
| `publisher` | `dc:publisher` | `publish-info/publisher` | | `Publisher` | | |
| `date` | `dc:date` | `publish-info/year`, otherwise `title-info/date` | | `Year`, `Month`, `Day` | | |
| `isbn` | An identifier that is an ISBN | `publish-info/isbn` | | | | |
| `genre` | `dc:subject` | `genre` | `Keywords` | `Genre` | Keywords | `meta` `keywords` |
| `annotation` | `dc:description` | `annotation` | `Subject` | `Summary` | Subject and description | `meta` `description` |
| `cover` | The manifest's cover image | `coverpage` | The first page of a scan | The first page | | |

Markdown and plain text carry no metadata that is read.

A cell is filled only as far as the front-end crate exposes the field.

### Contents

- With an outline in the draft, the contents are `Toc::Explicit`: one item
  per outline entry. An entry leads to a chapter or to a heading, because a
  contents item cannot name an extra anchor
  ([`mbook`, Extra anchors](../mbook/api.md#extra-anchors)): an entry whose target
  is another element leads to the nearest heading above it, or to the start
  of its chapter, with a `LinkRetargeted` warning.
- An entry whose target was not converted is left out with a `LinkDropped`
  warning.
- Without one, the contents are `Toc::FromHeadings`.

### Landmarks

A [landmark](../rfcs/0001-mbook-format.md#landmarks-1) of the draft becomes one of
the book.

- Its destination is re-targeted like a link, to the line: a landmark may
  lead to an extra anchor, so nothing is moved to a heading.
- A landmark whose target was not converted is left out with a `LinkDropped`
  warning, and so is a second `start`.

Only EPUB has landmarks of its own: the landmarks navigation of EPUB 3,
otherwise the guide of EPUB 2. Their types are mapped to the kinds of
RFC 0001 (`bodymatter` becomes `start`, `loi` becomes `list-of-figures`,
`copyright-page` becomes `copyright`, and so on). Three sorts of entry
produce no landmark: the cover, which is a picture in mbook; a table of
contents set in the text (`toc`), which the contents of the book replace;
and types without a counterpart.

### The final check

The assembled book must pass `Book::check` before it is returned or
written. A failure is reported as `Error::Book` and means a defect of this
crate, not of the source.

## Rules per format

| Format | Sections | Outline | Notes |
| --- | --- | --- | --- |
| EPUB | One per spine item, in spine order | The navigation document, otherwise the NCX | A page that only shows the cover picture is skipped. A picture in the spine is a figure. The landmarks of the book are carried over. |
| FB2 | One per top-level `section` of the main body; what stands between them gets sections of its own | None; headings are used | See the table below. The main body is the first one not named `notes` or `comments`. |
| PDF | One per first-level entry of the document outline, rendered from its pages; otherwise cut at top-level headings | The document outline | Markdown is rendered by `unpdf` with Markdown tables, without front matter and page markers. A document without a text layer becomes one section in which every page is a figure, rendered at 150 dpi or less. |
| CBZ | One per folder of the archive, headed by the name of the folder; one for an archive without folders | None; headings are used | Entries in natural order of their names; every page is a figure, and the first page is the cover. `ComicInfo.xml` fills the metadata. |
| DOCX | Cut at top-level headings | None; headings are used | Markdown is rendered by `undoc`. A footnote is moved into the section that refers to it. |
| HTML | Cut at top-level headings | None; headings are used | The encoding is taken from a byte order mark, from the document, or guessed. Only pictures given as `data:` addresses can be stored. |
| Markdown | Cut at top-level headings | None; headings are used | The encoding is detected. Every heading is marked with the name GitHub derives from it, so that `[see](#the-heading)` keeps working across chapters. A metadata block at the start (front matter) is removed with `MarkupDropped`. Pictures cannot be found from a stream and become their alternative text. |
| Text | One | None | The encoding is detected; paragraphs are separated by empty lines; Markdown punctuation is escaped. |

HTML, and the HTML inside an EPUB, is converted the same way:
`script`, `style`, forms and embedded objects are dropped; `sub`, `sup` and
`u` keep their text and are reported once as `MarkupDropped`; a table with
merged cells is flattened and reported; every element with an `id` is marked;
a note reference of EPUB (`epub:type="noteref"`) whose note is in the same
document becomes a footnote.

FictionBook is converted element by element by this crate:

| FB2 | Markdown |
| --- | --- |
| `title` of a section at depth *n* | A heading of level *n*, at most 6 |
| `subtitle` | A heading one level below its section's |
| `p` | A paragraph |
| `emphasis`, `strong`, `strikethrough`, `code` | The matching inline markup |
| `sub`, `sup`, `style` | The text alone, with `MarkupDropped` |
| `epigraph`, `cite`, the `annotation` of a section | A block quote; `text-author` is its last paragraph |
| `poem` | One paragraph per `stanza`, with a hard line break after every `v`; its title is a bold line |
| `empty-line` | A paragraph boundary |
| `table` | A table; merged cells are split, with `MarkupDropped` |
| `image` | A picture; alone in a paragraph, it is a figure |
| `a` | A link, or a footnote when its type is `note` or its target is in the notes body |
| `body` named `notes` or `comments` | The source of footnote texts; it produces no section |
| `binary` | A picture |
| An element the format does not define | Its text |

A source that is protected, with a password or by rights management, fails
with `Error::Protected` for every format. A source with no text and no
picture that could be stored fails with `Error::Empty`.

## What is implemented

Every format of [Formats and features](api.md#formats-and-features) is implemented
and tested against sources built in memory by its tests. What each front end
does not do:

| Format | Limits of the front end |
| --- | --- |
| EPUB | XHTML in an encoding other than UTF-8 and UTF-16 is read as UTF-8. A plain-text description loses its paragraph breaks, because `rbook` joins its lines. A note in another file than its reference stays a link. An archive entry declared larger than the resource limit refuses the book, even an entry the book does not use. |
| FB2 | A table keeps no alignment. The `style` and `lang` attributes are ignored. The document is decoded as a whole before it is parsed. Elements nested deeper than 256 levels are refused with `Error::Limit`. |
| PDF | Headings are what `unpdf` finds: per page, by font size, at most four levels. A picture stands at the end of its page, not where it is drawn, and one narrower or lower than 64 pixels is left out. An outline entry with a named destination has no page and joins the entry before it. A scanned document has no contents. Every page of a scan is held as pixels until the book is assembled. Whether `unpdf` escapes a lone `~` was not verified. |
| CBZ | `ComicInfo.xml` is read in UTF-8 only; of its fields the title, series, number, writer, translator, publisher, date, language, genre and summary are used. |
| DOCX | A link to a bookmark of the document becomes its text: `undoc` does not read bookmarks. A heading is found only when its style has an outline level. A note is plain text. Underline, superscript and subscript reach the assembler as raw HTML and are removed with `MarkupDropped`. The language is read from the core properties only. `undoc` unpacks entries under its own bound, 100 times their packed size, not under the limits of this crate. |
| HTML | A `form` and what it contains is dropped. CDATA sections of XHTML are not read. A document nested deeper than 128 levels becomes one paragraph of text, with `MarkupDropped`: `htmd` walks the tree recursively. An element with a name HTML does not define cannot be a link target. |
| Markdown | Front matter is removed, not read. Pictures are dropped: a stream has no folder to find them in. |
| Text | No structure is recognised: no headings, no lists. |

The assembler implements every rule of
[Assembling the book](#assembling-the-book).

## Security considerations

- **A source is untrusted input** to this crate, to its front-end crate and
  to the picture decoders. The converter has no `unsafe` code of its own;
  its dependencies are not covered by that statement.
- **Limits cover what the converter allocates**: the buffered source,
  unpacked entries, decoded pictures and the book. Memory that a front-end
  crate allocates while it parses is bounded only through the size of the
  source handed to it.
- **Archives.** An entry is unpacked only up to its limit, whatever size its
  header declares.
- **Nothing is fetched.** Network addresses in a source are never opened.
- **No path of the source reaches the book.** Chapters and pictures get
  generated names, so a hostile entry name cannot become a resource path.
- **No active content survives.** Scripts and styles are dropped with the raw
  HTML, and the final check rejects a book that still contains any.
