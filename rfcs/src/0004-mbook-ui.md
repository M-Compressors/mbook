# RFC 0004: The mbook application

| | |
| --- | --- |
| **Status** | Draft |
| **Start date** | 2026-11-21 |
| **Crate** | `mbook-ui` |
| **Builds on** | [RFC 0002](0002-mbook-api.md); [RFC 0003](0003-mbook-converter-api.md) |

## Summary

`mbook-ui` is a reading application built on `mbook` and `mbook-converter`.
It has three screens:

| Screen | What it does | Built on |
| --- | --- | --- |
| **Library** | Shows the books the application keeps, as covers | The catalogue; `BookReader::open` and `read_cover` |
| **Reader** | Shows one page of a chapter at a time | `BookReader`, read lazily |
| **Editor** | Changes the metadata, chapters, notes landmarks and pictures of a book | `Book`, in memory |

Opening a file that is not yet in the library adds it: an mbook file is
copied, a file in any format of RFC 0003 is converted first. Everything that
reads a file, decodes, parses, converts or encodes runs on the background
executor of GPUI; the main thread only draws and reacts.

The interface is built with GPUI Kit on the GPUI Fast engine. One layout
serves a phone, a tablet and a desktop window: it is chosen from the width of
the window, never from the platform.

A first version exists. [What is implemented](#what-is-implemented) says
what it does, where it differs from this document, and what has not been
tried outside of tests.

## Motivation

The repository has a format, a library and a converter, and no way to look at
a book. A reader is also the first real consumer of `BookReader`: the claims
of [RFC 0001](0001-mbook-format.md) (a book opens without being read in full;
only what is on screen is decoded) are only shown to hold by an application
that depends on them.

| Task | Screen |
| --- | --- |
| Keep books in one place and find one | Library |
| Open an EPUB, FB2 or PDF without thinking about formats | Library, through import |
| Read, and come back to the same place | Reader |
| Correct a title, a cover or a typo after a conversion | Editor |

### Goals

| Goal | How the design meets it |
| --- | --- |
| The interface never waits for a book | No call that touches a file or parses runs on the main thread ([Threads](#threads)). |
| One code base for desktop and mobile | Layout depends on a size class of the window; platform code stays in a thin shell ([Layout](#layout)). |
| The library survives its index | The mbook files hold the metadata; the catalogue can be rebuilt from them ([The catalogue](#the-catalogue)). |
| A failed import or save loses nothing | A file enters the library, and replaces a book, only by a rename ([Import](#import), [Editing](#editing)). |
| Little own code | Components, the Markdown view, the text editor, dialogs and lists come from GPUI Kit; paths, updates and the store from established crates. |
| What is read is what was edited | The editor's preview and the reader are the same view with the same style. |
| Usable without sight, without a pointer and without fine motor control | Every screen meets WCAG 2.2 at level AA where it applies to an application ([Accessibility](#accessibility)). |

### Non-goals

- **A store, an account or synchronisation.** The library is a folder on one
  device.
- **Two-page spreads.** See [Future possibilities](#future-possibilities).
- **Annotations**: highlights, notes and bookmarks.
- **Converting from mbook**, as in RFC 0003.
- **A streaming editor.** [RFC 0002](0002-mbook-api.md#non-goals) has none;
  a book is edited as a whole `Book`.
- **Mobile shells.** The design keeps them possible; the first version ships
  for desktop.

## Guide-level explanation

### The three screens

```mermaid
flowchart LR
    file["A file:<br/>mbook, EPUB, FB2,<br/>PDF, CBZ, DOCX, …"] -- "open, drop" --> imp["Import"]
    imp -- "copy or convert" --> lib["Library"]
    lib -- "open" --> reader["Reader<br/>BookReader, lazy"]
    lib -- "edit" --> editor["Editor<br/>Book, in memory"]
    reader -- "edit this book" --> editor
    editor -- "save" --> lib
```

The model is Apple Books for the Mac: a quiet library where the covers carry
the colour, and a reader whose controls step aside. What is taken from it and
what is left out is listed under [Prior art](#prior-art).

### Size classes

The layout follows the width of the window. A narrow desktop window gets the
phone layout, and a tablet in landscape gets the desktop one.

| Class | Window width | Navigation | Typical device |
| --- | --- | --- | --- |
| Compact | below 600 px | A stack of screens; a bar at the bottom | Phone; a narrow window |
| Medium | 600 to 959 px | A stack; panels slide over the content | Tablet in portrait; half a screen |
| Expanded | 960 px and more | A sidebar beside the content; panels dock | Desktop; tablet in landscape |

### Library

```text
Expanded
+----------------+----------------------------------------------------------+
| [ Search     ] |  Continue reading                                        |
|                |  +------+  The Master and Margarita                      |
| Library        |  |cover |  Mikhail Bulgakov                              |
|  [L] All books |  |      |  Chapter 7, 38%                     [ Read ]   |
|  [B] Reading   |  +------+                                                |
|  [✓] Finished  | -------------------------------------------------------- |
|                |  All books                           [Sort]       [+]   |
|                |  +-----+  +-----+  +-----+  +-----+  +-----+  +-----+    |
|                |  |     |  |     |  |     |  |     |  |     |  |     |    |
|                |  |     |  |     |  |     |  |     |  | ... |  |     |    |
|                |  +-----+  +-----+  +-----+  +-----+  +-----+  +-----+    |
|                |  38%  …   NEW  …   72%  …            Converting         |
| [ Import  ^2 ] |                                                           |
+----------------+----------------------------------------------------------+

Compact
+---------------------------+
| Library             [+]   |
| [ Search               ]  |
| +-----+ +-----+ +-----+   |
| |     | |     | |     |   |
| +-----+ +-----+ +-----+   |
| 38% …   NEW …    72% …   |
| +-----+ +-----+ +-----+   |
| |     | |     | |     |   |
+---------------------------+
| Reading   Library  Search |
+---------------------------+
```

- The sidebar uses icons for All books, Reading and Finished. The sort
  icon opens a menu with the current order marked.
- A cover keeps its own proportions; covers are aligned at the bottom of a
  row, as on a shelf. A book without a cover gets one drawn from its title.
- Under a cover: the percentage read, `Finished`, or a `NEW` badge.
  A neutral hover highlight leaves a small inset around the cover and
  reading status.
  The ellipsis opens the book menu.
- A book that is still being converted is already on the shelf, with its
  file name and the state of the import.
- The menu of a book (right click, the `…` button, or a long press): Open,
  Edit, Show the file, Remove.
- Files are added with `+` or by dropping them on the window. The system
  picker lists mbook and the extensions accepted by the converter.
- **New book…** asks for a title. Confirming creates a book in the library
  and opens its empty first chapter in the Markdown editor. The title and
  language can be changed in **Details**.

### Reader

```text
Pointer over the slider                  Controls hidden
+-------------------------------------+  +-------------------------------------+
| [ Back  Contents ]  Title  [ A Edit ]|  |                                     |
|                                     |  |                                     |
|         Chapter 7                   |  |         Chapter 7                   |
|                                     |  |                                     |
|         It was then that the        |  |         It was then that the        |
|         second act began.           |  |         second act began.           |
|                                     |  |                                     |
| Chapter 7, 38%   < 48 of 126 >        |  |           48 of 126                 |
| ------------●---------------------- |  |                                     |
+-------------------------------------+  +-------------------------------------+
```

- A page fills the text area. The current page and the total are numbered
  across the whole book. An approximation sign appears until all chapters
  have been measured for the current layout.
- The reading slider appears when the pointer is over the bottom strip,
  during a seek gesture, or when the slider has keyboard focus. Drag or
  click to seek across the book. The status previews the destination as
  the thumb moves, and releasing opens the destination page. Assistive
  technology can operate the slider while its track is hidden.
- Move the pointer, press a key or tap in the middle of the text to show the
  controls. They fade away after a few seconds of inactivity.
- Choose **Find in book**, or press Command+F on macOS or Control+F on
  Windows and Linux. Search matches text without regard to case and opens
  the page of the match. The bar shows the chapter and surrounding text.
  Use the arrow buttons, or Return and Shift+Return in the search field, to
  move between matches. Search wraps across the book. Escape closes the bar.
- **Contents** lists the entries of the book and marks the current one.
  Contents and appearance open as popovers in an expanded window and as
  bottom sheets in a medium or compact window.
- **Text and page appearance** offers page-tone swatches, text size and
  line-height controls, and the recently used typefaces. Settings lists
  every available typeface.
- Turn pages with Space, Page Up, Page Down, the arrows, the wheel, a
  two-finger trackpad gesture, or a click or tap in the margins. The
  bracket shortcuts still move by chapter.
- A link to another place of the book jumps there, and Back returns. A link
  to the web opens in the browser when it is followed.
- The book reopens on the page containing its saved place.

### Editor

The editor speaks of a book the way its author does: chapters have a number
and a title, a picture has a description, a link leads to a place. The paths
of files stay visible beside the titles, because links use them, but nothing
is asked for by path unless the path itself is being changed.

```text
Expanded
+------------------+--------------------------------------------------------+
| <  The Quiet Orchard                          Unsaved changes   [ Save ]  |
+------------------+--------------------------------------------------------+
| Book             | 2 The harvest  ch02.md     Visual Source Split Preview … |
|  Details         +--------------------------------------------------------+
|  Cover           | Undo Redo | Heading 1  B I S  | Link  Picture  Insert  |
|  Contents        +--------------------------------------------------------+
|  Pictures        | The harvest                                            |
|  Landmarks       |                                                        |
|  Problems (2)    | The apples came late that year.                        |
| Chapters  [+ New]|                                                        |
|  1 The gate      |                                                        |
|  2 The harvest   |                                                        |
|  3 Notes         |                                                        |
+------------------+--------------------------------------------------------+

Medium: the list slides over the content from "Book and chapters".
Compact: a stack. It opens in the chapter; Back reaches the same list.
```

**Finding the way.** One list holds everything: the sections about the whole
book under *Book*, then the chapters in reading order under *Chapters*.

- A chapter is listed by its number and the title its first heading gives
  it. A chapter without a heading is an *Untitled chapter*. A chapter with
  changes that are not saved says *edited*.
- The bar over a chapter repeats its number and title, and shows the path of
  its file.

**Writing a chapter.**

- **New chapter…** stands over the list of chapters, so that a long book
  does not push it out of sight. It asks for a title. The chapter is added after the last
  one, starts with that title as a heading of the first level, and is
  opened. Its file gets the next free name of the form `chNN.md`. A chapter
  may be added without a title.
- The bar offers Visual, Source, Split and Preview, by name in an expanded or
  medium window and by icon in a compact one. Split is available in expanded
  windows. Editing opens the first chapter in Visual for both new and
  existing books.
- **Find in chapter** and the platform's Find shortcut search the open
  chapter, including unsaved edits. Visual selects the matching text.
  Source and Split search the Markdown source and select the match on the
  source side. Preview opens the matching block. The same navigation keys
  and buttons as in the reader move between matches.
- **Visual** edits Markdown as blocks. One toolbar holds undo and redo, the
  heading level and inline formatting, quotes and list indents, then what is
  put into the text: **Link**, **Picture** and the **Insert** menu with
  tables, code blocks, rules and lists. Code blocks offer a language picker.
- Click or Tab into a table cell to edit it. Row and column grips open
  controls for insertion, deletion and alignment. Tables and code blocks can
  move between top-level blocks by dragging their handles. Task items can
  be checked in the editor.
- The menu at the end of the bar (*Chapter actions*) moves the chapter up or
  down in the reading order, renames its file and removes it. Renaming and
  removing ask first.
- **Source** and the source side of **Split** highlight Markdown syntax.
  Source can edit picture alternative text and destinations, and footnote
  and reference definitions.

**Putting a picture into a chapter.** **Picture** opens one dialog.

- It shows the pictures of the book as thumbnails. Choosing one, describing
  it and confirming puts it where the caret is, with a path relative to the
  chapter.
- **Add from your computer…** takes a file instead. The file is encoded,
  added to the pictures of the book and put where the caret is, in one go.
  Dropping picture files on the chapter does the same.
- The description is what is read to someone who cannot see the picture. It
  may be left empty.
- **Pictures in this chapter** previews the pictures the chapter refers to.

**Diagrams and formulas.** The **Diagrams** and **Formulas** sections add,
edit and remove resources. The source field highlights Mermaid or LaTeX
syntax as you type. Choose **Preview** to see the result or a renderer error.
Editing a source keeps its book path and the chapter references that use it.
In Visual, **Diagram** or **Formula** inserts an existing resource at the
caret. An empty alternative text field uses the resource kind as the
description. The chapter's Preview shows the same rendering as the reader.

**Linking.** **Link** opens one dialog.

- With words selected, they become the link. With nothing selected, the
  dialog also asks for the words and inserts them. With the caret in a link,
  the dialog edits it and offers **Remove the link**.
- Where the link leads is typed as a web address, or chosen as a place of
  the book: a chapter by number and title, then its start, one of its
  headings or a line it names. A choice fills in the relative destination,
  which stays editable.

**The contents.** The **Contents** section shows what a reader is shown as
the contents, and says where it comes from.

- Contents that follow the headings list every heading of every chapter. To
  rename an entry, its heading is changed; a new heading adds one.
- A book made from another format keeps the contents of its source as a list
  of its own. The section says so, and **Build the contents from the
  headings** replaces the list after a question.
- An entry of such a list ends with a menu. **Edit…** asks for its title and
  for where it leads: a chapter, and its start or one of its headings,
  chosen by title. **Nest under the entry above** and **Move out one level**
  change its level, and the entries under it move with it. **Remove…** asks
  first. **Add an entry…** opens the same dialog; choosing a heading fills
  in its words as the title.
- Nobody orders the list by hand. An entry sits where its place is in the
  book, and one that is given another place moves there. The entries of a
  chapter go with it when it moves up or down, and keep leading to it when
  its file is renamed. They are removed with it, and the question before a
  chapter is removed says how many there are. Landmarks do the same.
- An entry whose heading is gone is marked "leads nowhere", and
  one whose heading was moved past another in the text "out of order". The
  section counts both, and **Put the entries in the order of the book**
  sorts the list. The Problems section leads here with **Open Contents**.
- Choosing an entry opens its chapter.

**The whole book.**

- **Details** is a form over the metadata of RFC 0001: title, authors,
  language, series, publisher, date, annotation.
- **Pictures** shows a gallery with a preview, dimensions, size and a remove
  control for each picture. Pictures and the **Cover** accept AVIF, JPEG,
  PNG, GIF, WebP and BMP. Adding one offers the name of its file in the
  book, a quality and maximum output dimensions. It starts at quality 80 and
  the original dimensions.
- **Landmarks** says what a landmark is, lists the book's
  [`notes` landmarks](0001-mbook-format.md#landmarks-1) and lets the editor
  add, edit or remove them. Each entry points to a place chosen like the
  destination of a link: a chapter, and its start or a named place in it.
  Its title is optional. The chapter editor is where the text of the notes
  is written. A landmark keeps leading to its chapter when the file of the
  chapter is renamed, and is removed with the chapter.
- **Problems** lists what `Book::check` reports, each with its chapter and
  place; choosing one opens that place. A book with problems is not saved.
- **Save** replaces the book in the library. For a book with automatic
  contents, Save builds the contents from the current chapter headings.
  Use Command+S on macOS or Ctrl+S on other platforms. On macOS the File
  menu also offers Save.

### Settings

One screen, reached from the library: a page on an expanded window, a pushed
screen on a compact one.

| Setting | Choices | Default |
| --- | --- | --- |
| Typeface of books | Literata, or any font family installed on the device | Literata |
| Size of the text | 12 to 32 px | 20 px |
| Line height of the text | 1.2 to 2.0 | 1.6 |
| Page tone | Paper, Sepia, Dusk, Night, or the one that follows the system | Follows the system |
| Size of the interface | 100% to 200% | 100% |
| Controls of the reader | Hide while reading, or always shown | Hide while reading |
| Motion | As the system says, or reduced | As the system says |
| Check for updates | On, off | On |

A change shows at once in an open book. A page tone that follows the system
is Paper while the system is light and Night while it is dark.

### Using the application without a pointer or without sight

- **Keyboard.** Every action has a key or is reached with Tab and the arrow
  keys; what has the focus is outlined. The shelf is a grid the arrows move
  through; Return opens a book, and the menu key opens its menu.
- **Screen readers.** A cover is announced as "The Master and Margarita, by
  Mikhail Bulgakov, 38 percent read". A chapter is read as headings,
  paragraphs, lists and tables; a picture is read by its alternative text.
  The end of an import, and a failed one, are spoken.
- **Low vision.** The interface scales to 200%, at which a desktop window
  takes the compact layout instead of cutting anything off. Text and
  controls keep the contrast of level AA in every page tone.
- **Touch and tremor.** Nothing needs a drag, a hover or a timed gesture: a
  chapter is moved with "Move up" and "Move down", a picture is added from a
  dialog as well as by dropping it, and the controls of the reader can be
  kept on screen.

### Visual direction

The covers are the only saturated colour of the library; the reader shows
nothing but the page. One accent marks what is selected and how far a book
has been read.

| Token | Light | Dark | Used for |
| --- | --- | --- | --- |
| Canvas | `#F4F5F7` | `#15161A` | The background of the library |
| Surface | `#FFFFFF` | `#1E2026` | Sidebars, sheets, menus |
| Ink | `#16181D` | `#ECEDEF` | Text |
| Ink, muted | `#5F6673` | `#9AA0AB` | Authors, captions |
| Line | `#E3E5EA` | `#2C2F37` | Separators, which carry no meaning |
| Outline | `#7C8390` | `#747B89` | The border of a field, the track of a progress line, the edge of a control |
| Iris (accent) | `#4650D8` | `#8C93FF` | Selection, progress, the primary button |

| Page tone | Page | Text |
| --- | --- | --- |
| Paper | `#FFFFFF` | `#1B1B1B` |
| Sepia | `#F4ECD8` | `#3A2E1F` |
| Dusk | `#2A2C31` | `#D5D7DB` |
| Night | `#000000` | `#B8B8B8` |

- **Type.** The interface uses the system font of the platform. The text of
  a book uses the chosen family when available and falls back through
  installed serif families. The library section heading uses the reading
  typeface as well.
- **Measure.** A line of a book is at most 38 em, about 70 characters, at a
  line height of 1.6.
- **Icon.** An open book on the navy of these design documents:
  `crates/mbook-ui/assets/icon.svg`, drawn once and rendered to the PNG the
  bundle and the Windows executable take.
- **Motion** answers an action: a screen is pushed, a sheet rises, the
  controls of the reader fade and page turns slide and fade. The system
  setting that reduces motion turns the transitions off.

## Reference-level explanation

### What is settled

The format author settled the points below on 2026-11-21. Every other detail
of this document is *proposed*, and listed under
[Unresolved questions](#unresolved-questions) where a choice is open.

| Decision | Status |
| --- | --- |
| The interface is built with `gpui-kit` on the `gpui-fast` engine | Confirmed |
| The allocator is `mimalloc3` | Confirmed |
| The application has a library of books | Confirmed |
| It opens every format `mbook-converter` converts | Confirmed |
| A book that is not mbook is converted when it is opened, and the result is kept in the library | Confirmed |
| A book can be edited | Confirmed |
| All CPU-bound work runs on the background executor | Confirmed |
| The prototype is modelled on Apple Books for the Mac | Confirmed |
| The layout is responsive, so that desktop and mobile share it | Confirmed |
| The directories of the application come from the `dirs` crate | Confirmed |
| Updates are installed from GitHub releases with the `self_update` crate | Confirmed |
| The crate is `mbook-ui`, and the application is called mbook | Confirmed |
| An import leaves the original file where it is; the library gets a copy | Confirmed |
| A book whose `id` is already in the library is not added again; the existing one is opened | Confirmed |
| Pictures are decoded by `avif-decode`; the package requires Rust 1.100 | Confirmed |
| Literata is the default reading choice and the typeface is configurable | Confirmed. Bundling Literata is pending. |
| The modules follow Feature-Sliced Design | Confirmed |
| A release is a tag on GitHub; its binaries are built by a workflow started by hand for that tag | Confirmed |
| Releases are signed with keys generated for the project and kept as secrets of the repository | Confirmed |
| The icon is an open book on the navy of the design documents | Confirmed |
| Saving an edit replaces the book in the library | Confirmed |
| The editor accepts pictures that are not AVIF and encodes them with the `image` crate | Confirmed |
| The catalogue is a `redb` database | Confirmed |

### Crate and workspace

`crates/mbook-ui` is one package with a library and a binary.

| Target | Contains |
| --- | --- |
| Library `mbook_ui` | Every model and view, and `run(Application, Roots)`. It names no platform. |
| Binary | The allocator, the platform of the desktop, the directories from `dirs`, a call to `run`. |

- The binary **MUST** stay a shell: it sets the global allocator, builds the
  `Application`, and passes it on. A mobile shell is a second small crate
  that supplies its own platform to the same `run`.
- `gpui-kit` is used with the feature `gpui-fast`; application code imports
  `gpui_kit::*` only and never names `gpui` or `gpui-fast`.
- The allocator is set in the binary, not in the library, so that tests and
  other shells choose their own.
- The package sets its own `rust-version`, 1.100, and is not published to
  crates.io. `mbook` and `mbook-converter` keep the 1.89 of the workspace.
- The lints of the workspace apply unchanged.

The modules follow the layers of Feature-Sliced Design. A layer uses only
the layers below it.

| Layer | Slices |
| --- | --- |
| `app` | `run`, the window, the root view, menus and keys |
| `pages` | `library`, `reader`, `editor`, `settings` |
| `widgets` | `chapter_view`, the text of a chapter, which the reader and the preview of the editor share. Every other part of a screen belongs to its page until a second page needs it. |
| `features` | `import_book`, `reading_position`, `appearance`, `edit_book`, `search_library`, `search_text`, `auto_update` |
| `entities` | `book` (an open book), `catalogue` (the store), `picture` (decoding and its cache), `figure` (diagram and formula rendering) |
| `shared` | `roots` (directories), `model` (the key of a book, a place in one), `layout` (size classes), `theme` (colour tokens), `actions`, `ui` (presets over Kit components), `assets` |

State lives in model entities; a view observes a model and calls its
methods. A view holds no `BookReader`, no `Book` and no path.

### Threads

| Work | Runs on | Priority |
| --- | --- | --- |
| Drawing; changing entities; reading snapshots | Main thread | |
| Opening the catalogue; its reads and writes | Background | Medium |
| `BookReader::open`; building the snapshot of a book | Background | High |
| `read_chapter`; resolving its links and pictures | Background | High |
| Parsing Markdown for display | Background, by the text view of GPUI Kit | |
| `read_picture`; decoding AVIF; scaling | Background | Medium |
| Reading diagram and formula sources and drawing SVG | Background | High for the current chapter |
| Detecting a format; copying a file | Background | Medium |
| `Converter::convert` | Background, one conversion at a time | Low |
| Making the thumbnail of a cover | Background | Low |
| `Book::open`, every change of a `Book`, `check`, `save` | Background, by the task that owns the book | Medium |
| Decoding a picture added in the editor and encoding it as AVIF | Background, one at a time | Low |
| Filtering the library | Background | High |
| Matching text in the reader and chapter editor | Background | |
| Checking for an update; downloading it | Background | Low |

- Code on the main thread **MUST NOT** call a `read_` method of
  `BookReader`, any method of `Book` or `Converter`, a function of
  `std::fs`, the catalogue, or a decoder.
- Background work is started with the background executor of GPUI and
  returns a `Task`. The entity that asked keeps the `Task`; dropping the
  entity cancels it.
- Cancelling takes effect between steps. A conversion or a decode that has
  started runs to its end, and its result is discarded: the converter has no
  cancellation ([RFC 0003](0003-mbook-converter-api.md#future-possibilities)).
- A result that arrives for a state the user has left **MUST** be dropped:
  every request carries the generation of the view that made it.
- Conversions run one at a time. The AVIF encoder already uses every core,
  and each conversion holds its source and its book in memory
  ([RFC 0003](0003-mbook-converter-api.md#sources-and-memory)).

### Storage

The shell hands the library two directories, the *roots*. On desktop they
come from `dirs`:

```text
dirs::data_dir()/mbook/            kept; backed up with the user's data
    catalog.redb                   the catalogue
    books/<key>.mbook              one file per book
    books/.incoming/               files of imports that have not finished
dirs::cache_dir()/mbook/           may be deleted at any time
    covers/<key>.jpg               thumbnails of covers
    update/                        a downloaded update
```

- `<key>` is a UUID version 7 made when the book is added. It is not the
  `id` of the book's metadata: that is any string, and an edit may change it.
- A file **MUST** enter `books/` by a rename from `books/.incoming/`, which
  is on the same volume. A reader therefore never sees half a file.
- `books/.incoming/` is emptied when the application starts.
- Everything under the cache root **MUST** be reproducible from the data
  root.
- When `dirs` knows no directory for the platform, the shell supplies the
  roots itself. Tests pass temporary directories.

### The catalogue

The catalogue is one `redb` database. It holds two kinds of data:

| Table | Key | Holds | Can be rebuilt |
| --- | --- | --- | --- |
| `books` | key | `id`, title, authors, language, series, the size of the file and of the cover, the time added, the format it was converted from | Yes, from the files in `books/` |
| `state` | key | The last place read, the time of the last opening, whether the book is finished | No |
| `settings` | name | Page tone, typeface, text size, the size of the window, whether updates are checked | No |

- **The mbook files are the source of truth for metadata.** A row of `books`
  is a copy that spares the application from opening every file at start.
- When the database is missing or cannot be opened, the application
  **MUST** rebuild `books` by opening every file of `books/` with
  `BookReader::open`, which reads the tables and nothing else. Reading
  places and settings are then lost; the books are not. The file that
  could not be opened is kept beside the new one as `catalog.redb.damaged`,
  and the user is told once.
- Two cases are not damage, and the file **MUST NOT** be replaced: a
  database written by a newer version of the application, and one that
  another running copy of the application holds open. Both end in an error
  screen that says which.
- A place is stored as the **path** of a chapter and a byte offset, not as a
  resource number: numbers change when a book is saved again
  ([RFC 0002](0002-mbook-api.md#shared-value-types)), paths do not. When the
  offset is no longer at the start of a block, the reader opens the block
  that contains it; when the chapter is gone, the start of the book.
- Progress is the decoded length of the chapters before the place, plus the
  offset, over the decoded length of all chapters. Both come from the
  directory of the file; nothing is decoded for it.
- Rows are encoded with the value types of `redb`; the encoding is
  *proposed*. The database carries a schema version.

### Import

```mermaid
stateDiagram-v2
    [*] --> Queued
    Queued --> Probing
    Probing --> Copying: is_mbook
    Probing --> Converting: any other format
    Copying --> Indexing
    Converting --> Indexing
    Indexing --> Done
    Probing --> Failed
    Copying --> Failed
    Converting --> Failed
    Indexing --> Failed
    Done --> [*]
```

| Step | What happens |
| --- | --- |
| Probing | The first 53 bytes are given to `mbook::is_mbook`. |
| Copying | An mbook file is copied into `books/.incoming/` and opened with `BookReader::open`, which rejects a file that is not valid. |
| Converting | `Converter::convert` reads the file and writes into `books/.incoming/`. The format is detected from the content; for Markdown and plain text, which are never detected, it is taken from the extension of the file name with `Format::from_str`. |
| Indexing | The file is renamed into `books/`. Its metadata go into `books`, and its cover, decoded and scaled, into `covers/`. |

- The native picker filters by the book extensions, including ZIP
  containers for supported sources. The application checks the extension
  again after selection. Conversion and drag-and-drop still inspect the
  source content.
- The original file is neither moved nor changed.
- A book whose `id` is already in the library is not added again; the
  existing book is opened. The converter gives a source without
  an identifier an `id` computed from its bytes, so the same file is
  recognised.
- A failed import leaves nothing in `books/` and no row. The error is shown
  on the entry of the import, in the words of the converter's `Error`.
- The warnings of a `Report` are counted on the entry and listed on request.
  They are not kept after the application closes.
- Opening one file opens the reader when its import is done. Adding several
  files only adds them.
- A phone cannot hold a source of 2 GiB and its book. On a compact device
  the shell lowers `ConvertLimits` (*proposed*; the values are not chosen).

### Reading

An open book is an entity with two parts.

| Part | Lives | Contains |
| --- | --- | --- |
| Snapshot | Main thread, immutable | Title and authors; the contents; for each chapter its number, path and decoded length |
| Source | Behind a mutex that only background tasks lock | The `BookReader<File>` |

- The snapshot is built once, on the background, when the book is opened.
  The main thread **MUST NOT** lock the source, not even for a table lookup.
- A chapter is prepared on the background in one task: `read_chapter`, then
  `resolve` for every link and picture destination of the text, then the
  byte offset of every top-level block, found with `markdown`, the parser
  `mbook` itself uses. The main thread receives the text, a map from
  destination to `Destination`, the figure sources, and the offsets.
- The reader uses the scrolling Markdown view of GPUI Kit to measure the
  top-level blocks of the chapter. It displays one measured page at a time
  by moving the list to that page's start and masking the content below
  its end. The editor preview uses the same view with scrolling.
- Paragraphs split at rendered line boundaries, avoiding a single line at
  either end of a break when there is room. A heading stays with the content
  that follows when they fit together. Other blocks move intact to the next
  page if they fit there. A block taller than a page splits on the body text
  line grid because the Kit does not expose its own internal line boundaries.
- Each chapter contributes its measured page count to the book total. Until
  a chapter has been laid out, its count is estimated from its decoded
  length and the observed density of measured, multi-page chapters. With
  no such chapter, the column size and typography supply the initial
  density. The counts are kept for the current window and typography.
- The reading slider maps its fraction back into the decoded lengths in the
  snapshot, then opens the page containing the resulting byte offset. The
  start and end include empty first or last chapters. The bottom strip
  keeps its dimensions as the slider appears and disappears.
- The stored place remains the chapter path and a byte offset. For a page
  that begins inside a block, the reader estimates the byte offset from
  the vertical fraction of that block. It writes the position at most once
  a second and when the book closes.
- Find starts with the current chapter and reads one chapter at a time on the
  background. It searches words shown by Markdown, including headings, link
  labels, code and alternative text. It keeps the current match and its source
  offset for navigation. Only the chapter shown by a match is laid out.
  Changing the query or closing the bar cancels the remaining chapter reads.
- After a change to the window or typography, the reader recalculates the
  pages and keeps the page containing the reading anchor. A contents
  entry, link or saved place does the same with its destination offset.
  A destination that resolved to `Destination::ExtraAnchor` is completed
  with `read_resolve` on the background when the link is followed.
- Turning past the last page opens the first page of the next chapter.
  Turning back from the first page opens the last page of the previous
  chapter. Page turns within a chapter slide and fade. With reduced motion,
  they take effect immediately.
- Editing a book closes its source first, so that the file can be replaced
  on every platform; the reader reopens the book afterwards.

### Pictures

GPUI decodes no AVIF, so the application does:

1. `read_picture` gives the bytes of the file.
2. `avif-decode` decodes them to 8-bit RGBA. It does not apply the crop,
   the rotation and the mirroring an AVIF file may ask for, so the
   application does, by the rules `mbook` reads the size of a picture with:
   the decoded picture then has the size the tables of the book give.
3. The picture is scaled down to fit the text column and the available page
   height, keeping its proportions.
4. The result is given to GPUI as a rendered image.

- A picture is requested when the Markdown view lays it out. The three
  steps run off the main thread. The AVIF decoder runs on a scoped thread
  with its own stack because native executor threads can have small stacks.
  The view reserves the picture's space from its size in the book tables.
- Decoded pictures are cached by their fitted dimensions. After a resize,
  the placeholder and the decode use the new size. The cache is bounded by
  bytes, with the least recently shown leaving first. The bound depends on the size class
  (*proposed*: 256 MiB expanded, 64 MiB compact).
- The thumbnail of a cover is made once, at import or after a save, 400 px
  wide, and stored as a JPEG file that GPUI loads by its path.
- A picture that cannot be decoded shows its alternative text. It never
  fails the chapter.

### Diagrams and formulas

References to diagrams and formulas use the
[Markdown image syntax of RFC 0001](0001-mbook-format.md#diagram-and-formula-resources).
The source dialogs use GPUI Kit's code editor with Tree-sitter grammars for
Mermaid and LaTeX. The highlighting is independent of rendering.
The application uses `merman` for Mermaid and RaTeX for KaTeX. It renders
the source on a background task and passes SVG to GPUI. A bounded cache
is keyed by source, page colours and formula style.

The Markdown view centres a block figure, fits its drawing to the column
and page and displays its caption when supplied. Inline formulas follow
the baseline of the surrounding text. Unavailable drawings show the
alternative text. Figures use the page tone and scale with the text.
All drawing is local. The application does not follow links or execute
commands from figure sources.

### Accessibility

The application conforms to WCAG 2.2 at level AA, read for a native
application. GPUI reports its elements to the platform through AccessKit; an
element is reported when it has an identifier and a role.

**What assistive technology is told**

- Every control **MUST** have a role and a name. A control that shows only
  an icon gets its name as a label, and shows the same words as a tooltip.
- The name of a book on the shelf is its title, its authors and its state:
  the percentage read, `new`, or the step of its import. The progress line
  under a cover is not a separate element.
- A state that colour shows **MUST** also be in the name or in text beside
  it: selected, current chapter, unsaved, failed, has problems.
- The text of a chapter is reported with its structure, which the Markdown
  view of GPUI Kit provides: headings with their level, lists, tables,
  links, code.
- A picture of a book is reported as an image named by its alternative
  text. A picture with an empty alternative text is not reported.
- The language of a book, from its metadata, is given to the elements that
  show its text, so that a screen reader speaks it in the right voice.
- A change the user did not cause in the focused element **MUST** be
  announced: an import that finished or failed, problems found by a save, an
  update that is ready. A change that follows from an action is announced by
  the element that changed. GPUI has no live regions: an announcement is a
  notification with the alert role, and text that changes in place has the
  status role.
- An identifier of an element **MUST** stay the same while the element
  means the same thing: the identifier of a book is its key, of a chapter
  its path. Assistive technology then does not hear a list as replaced when
  it is redrawn.

**Keyboard and focus**

- Every action **MUST** be reachable from the keyboard alone, and no
  element may hold the focus without a way to leave it by key.
- The order of focus follows the order of reading: sidebar, then content,
  then panels.
- The focused element **MUST** show an outline of at least 2 px in the
  accent colour, with a contrast of 3:1 against what is beside it. The
  outline appears for focus that came from the keyboard.
- A dialog, a sheet and a popover keep the focus inside while they are
  open, close with Escape, and return the focus to the control that opened
  them.
- In the Markdown editor Tab indents. Escape followed by Tab leaves the
  editor, and the editor says so in its description.
- Opening a screen moves the focus to its heading or its first control;
  going back returns it to the control that led there.
- Keys are bound through the key map of GPUI, so that they are listed in the
  menus and can be changed in one place.

| Key | Action |
| --- | --- |
| Tab, Shift+Tab | Next and previous control |
| Arrows | Move inside a grid, a list or a menu |
| Return, Space | Activate; in the reader, Space turns a page |
| Escape | Close a panel; leave the editor; go back |
| The platform's Open, Save, Find, Settings | The same in the application |
| Page Up, Page Down, Home, End | Move in a chapter |
| The platform's bracket keys | Previous and next chapter |

**Seeing**

- Text **MUST** have a contrast of at least 4.5:1 against its background,
  and the border or glyph that identifies a control at least 3:1. The
  tokens of [Visual direction](#visual-direction) meet this in both
  appearances and in every page tone; a test computes it.
- The colour `Line` is for decoration only. Whatever must be seen to use a
  control uses `Outline`.
- The size of the interface is one setting that scales every length of the
  interface. The size class is computed from the width of the window
  divided by that scale, so that a scaled window reflows instead of
  clipping.
- The text of a book is never justified and never set narrower than the
  reader chose. Its size, line height and typeface are settings; no text is
  drawn as a picture.
- Nothing flashes.

**Pointer and touch**

- A control is at least 24 px in both directions for a pointer and 44 px on
  a touch device.
- Whatever a drag does **MUST** also be possible without one: a chapter has
  "Move up" and "Move down".
- Whatever a hover or a long press reveals **MUST** also have a visible
  control or a menu entry.

**Time and motion**

- The controls of the reader hide only while nothing inside them has the
  focus and the pointer is not over them, and never when the setting keeps
  them shown. They return on any key. The fade and the page turn animation
  are skipped when reduced motion is active.
- No message disappears by itself while it still asks for an action.
- When the system, or the setting, asks for reduced motion, every
  transition is replaced by an immediate change. GPUI Kit reads the system
  setting on macOS, Windows and Linux.

**Words**

- An error names what went wrong and what can be done, in text, at the
  control it belongs to.
- A field of a form has a visible label, which is also its name.

### Appearance

- The typeface, the size and line height of the text and the page tone are
  rows of `settings`. A change is applied to every open reader and to the preview of
  the editor without reopening the book.
- The choices are the families the GPUI text system reports for the device.
  The list is read once on the background.
- A stored family that is unavailable **MUST** fall back to the first
  installed family in this order: Literata, Charter, Iowan Old Style,
  Georgia, Times New Roman. The interface font is used until enumeration
  finishes or if none is available. The stored choice is kept.
- A typeface without the letters of a book is not refused: the text system
  substitutes another font for them.
- The setting changes the text of books and the library section heading.
  Other interface text uses the system font, and code remains monospaced.

### Editing

- The library creates new books with `Book::new`, the entered title and
  the language tag `und`. It adds an empty `ch01.md`, writes the book into
  `books/.incoming/` and uses the same install and catalogue step as an
  import. The editor opens after the file and its catalogue row are in place.

```mermaid
sequenceDiagram
    participant V as Views (main thread)
    participant S as EditSession (main thread)
    participant T as Book task (background)
    V->>S: change, check, save
    S->>T: command, in order
    T->>T: Book::set_text, check, save
    T-->>S: outline, problems, or error
    S-->>V: notify
```

- A session starts with `Book::open` on the background. The `Book` is then
  owned by one background task for as long as the session lives.
- The session sends that task commands over a channel, and the task applies
  them **in the order they were sent**. A save therefore always includes the
  edits made before it.
- The main thread holds an *outline*: the metadata, the paths and sizes of
  chapters and pictures, the paths of diagrams and formulas, the landmarks
  and the problems last reported.
  The task sends a new outline after each command that changes it.
- The Markdown text of the chapter is in the source editor of GPUI Kit.
  Source and Split use its Tree-sitter Markdown grammar.
- Visual uses `gpui-kit-markdown-editor` with the GPUI Fast engine. Enter
  starts a paragraph, and the block editor owns selections, structured
  edits, and undo and redo. Committed transactions update the chapter's
  Markdown buffer. Opening or closing Visual leaves the source intact.
- The application compares the editor's GFM blocks before and after edits
  with `similar`. Unchanged blocks keep their original Markdown whenever
  they can be put back without changing the block structure. If the
  initial import changes the top-level block structure, the chapter is
  available in Source. The Visual picture preview and the gallery use the
  same bounded cache and load pictures by book path through the editing task.
- The source text is
  sent to the task as `Chapter::set_text` when typing pauses for 400 ms,
  when another chapter is opened, and before a check or a save.
- The preview is the Markdown view of the reader, given the text of the
  editor. The text updates immediately. The editing task resolves figure
  references from the current draft after a short pause, including
  reference-style images. The view then draws them using the same code as
  the reader.
- An error of a call (`InvalidText`, `InvalidPath`, `DuplicatePath`,
  `InvalidPicture`, `InvalidDiagram`, `InvalidFormula`, `InvalidMeta`) is shown at the control that caused it,
  with the byte offset when the error has one.
- **Save** runs `Book::check`, then `Book::save` onto the file of the
  library, which writes a temporary file beside it and renames it into
  place. A book with problems is not written, and the problems are shown.
  Save sends chapter text and pending Details edits before the save command.
  After a save the row of `books` and the thumbnail are made again.
- Undo inside a chapter is the editor's own. Removing a diagram or formula
  asks for confirmation. References to a removed resource are reported by
  `Book::check` and block saving until they are updated or removed.
- Removing a chapter or a picture
  asks first and cannot be undone.
- Changes in Landmarks replace the landmark list through `Book::set_landmarks`.
  The editor keeps the other landmarks and their order. If a chapter or
  anchor no longer exists, the next check or save reports the dangling
  landmark. The Landmarks section can retarget or remove it.
- A chapter moves to another path by being removed and inserted at the same
  position, since `Book` has no rename; the links that broke are then among
  the problems.
- A new chapter gets the first free path of the form `chNN.md` and the text
  `# Title`, followed by an empty line, when a title was entered. The title
  is escaped so that it is read back as the same words.
- The outline carries, for every chapter, the places a link or a landmark
  can name: its headings, with their level and their text without markup,
  and the lines an anchor definition names. They are found from the anchors
  `Book` keeps; only the block of each heading is parsed again, and the
  whole chapter when that block has a bracket, which may refer to a
  definition elsewhere. The list of
  chapters, the dialogs of a link and of a landmark and the Contents section
  all read them from there.
- The outline carries the contents as a reader is shown them. For automatic
  contents they are computed from those headings by the rule `Book` writes
  them with ([RFC 0002](0002-mbook-api.md)); for a stored list they are its
  items.
- A link made with nothing selected is typed by the block editor as plain
  text, selected and then linked, so that the editor escapes the words and
  writes the destination.
- A picture is put into a chapter as `![description](path)`, pasted into the
  block editor as Markdown. The path is relative to the chapter and
  percent-encoded; the description is escaped.
  - A picture chosen from a file for a chapter remembers that chapter. When
    the book has taken the picture and Visual still shows that chapter, the
    picture is put at the caret. Otherwise it stays among the pictures of
    the book and a notification says how to place it.
  - A drop on the chapter takes the files with the extension of a picture
    and leaves the others to the window, which imports books.
- A book with an empty contents list uses automatic contents when opened
  in the editor. Save builds the list from the current chapter headings
  and records the mode in the private metadata key
  `x-mbook-ui-contents` with value `headings`. Reopening such a book in
  the editor restores the mode for subsequent edits.
- A book with entries and no automatic mode keeps the contents of its file,
  item by item. Removing a heading can make an item lead nowhere, and
  moving a heading past another in the text can make one fall out of order;
  either blocks the save. The outline marks each such item
  by the walk `Book` writes the contents with, so the Contents section
  shows which entries to change. The editor also offers to switch to
  automatic contents from headings.
- The items of such a list follow their chapter.
  - Moving a chapter sorts the list again, by the rule of the sorting
    control below, when the list was in the order of the book before the
    move. A list that was out of order already is left as it is: the editor
    does not rearrange what the author has not sorted.
  - A chapter that gets another path takes the items that led into it
    along, and so do the landmarks, of every kind. The links of other
    chapters do not follow; they are reported by the check.
  - Removing a chapter removes the items that led into it, and the items
    under them move up as they do when one entry is removed. A list left
    without items gives way to automatic contents. The landmarks that led
    into the chapter are removed too, of every kind: the Landmarks section
    shows only `notes`, so a landmark of another kind that led nowhere
    could not be repaired there.
- A list the book keeps is changed by commands of the editing task that
  name an entry by its position in the last outline. The section takes no
  second change until the first is answered.
  - An entry needs a title and a place the contents can lead to: a chapter,
    or one of its headings. A line that an anchor definition names is not
    offered, because the format does not allow it
    ([RFC 0001](0001-mbook-format.md#resource-3-toc)).
  - A new entry is put after the last entry that leads to the same place or
    an earlier one, at the level of the entry above it. An entry given
    another place is taken out and put back by the same rule with the level
    it had.
  - A level is kept between what the entry above allows and what the entry
    below needs. Moving an entry in or out moves the entries under it too;
    removing one, or moving it elsewhere, brings them one level up. A list
    that follows the rules of the format therefore still follows them after
    any of these changes.
  - Sorting is stable by chapter and offset. An entry that leads nowhere
    stays behind the entry it followed, and a level the new order does not
    allow is brought up.
  - A list that loses its last entry gives way to automatic contents, as an
    empty list does when a book is opened.
- Text that `Chapter::set_text` refused stays in the editor as a draft, and
  the book is not saved while a draft exists: nothing typed is dropped.
- A picture, or a cover, is added from a file in AVIF, JPEG, PNG, GIF, WebP
  or BMP, the formats `mbook-converter` reads
  ([RFC 0003](0003-mbook-converter-api.md#pictures)). The native picker and
  its selection check use the extensions of these formats.
  - The application decodes and orients every selected file, takes the first
    frame of an animation, and encodes AVIF at the chosen quality and speed
    6. Optional width and height limits fit the result without upscaling
    or changing its proportions. The encoding runs on a separate task.
    The command that adds the picture is sent when the bytes are ready.
  - The path the user gives ends in `.avif`; the editor proposes the name of
    the file with that extension.
  - A picture of more than 100,000,000 pixels, the limit of the converter,
    **MUST** be refused before it is decoded in full.
  - A save waits for the pictures that are still being encoded.
  - A file that cannot be decoded is reported at the control that added it,
    and nothing is added.
- Leaving the editor with unsaved changes asks whether to save them.
  Unsaved changes do not survive the application closing.

### Layout

- A view **MUST** take its layout from the size class of its window and
  from nothing else. It **MUST NOT** test the operating system.
- The size class is computed from the width of the window and the scale
  of the interface, by the entity that holds the settings; a view asks it
  while it draws.
- What a pointer reveals by hovering **MUST** also be reachable without
  one: by a tap, a long press or a visible control.
- On a touch device every control is at least 44 px in both directions.
- Views keep clear of the safe-area insets the window reports.

| Element | Compact | Medium | Expanded |
| --- | --- | --- | --- |
| Sections of the library | A bar at the bottom | A sidebar that slides over | A fixed sidebar |
| Columns of the shelf | As many covers of 104 px as fit | Covers of 132 px | Covers of 132 px |
| Contents of a book | A sheet | A sheet | A popover |
| Paged text column | Window width less 20 px on each side | At most 38 em with at least 32 px on each side | At most 38 em with at least 32 px on each side |
| Editor | A stack; the list of sections and chapters, then Visual, Source or Preview | Visual, Source or Preview; the list slides over | The list beside Visual, Source, Split or Preview |
| Menu of a book | A long press | A long press, or `…` | A right click, or `…` |

### Updates

Updates exist on desktop only, behind the Cargo feature `self-update`, which
a build for a store or a package manager leaves off.

- A release is a tag of the repository on GitHub, `vX.Y.Z`, where `X.Y.Z` is
  the version of the workspace. A maintainer starts
  `.github/workflows/release.yml` by hand with the tag selected as the ref.
  The tag must contain the workflow version to run. The workflow checks the
  selected tag against the workspace version and publishes `mbook` and
  `mbook-converter` to crates.io through Trusted Publishing. It skips crate
  versions already present in the registry. Once both libraries are
  published, it creates the GitHub release if needed and builds and uploads
  the application installers and update archives for that tag.
- The workflow builds for macOS on arm64 and x86-64 and for Windows on
  x86-64. There is no Linux build.
- Each platform gets two files: an installer for a person (`.dmg`, `.msi`)
  and an archive `mbook-<tag>-<target>.tar.gz` for the application to update
  itself from. The archive holds the application bundle on macOS and the
  installer on Windows.
- The bundle, the executable and the installer are signed with a
  self-signed certificate, so that the identity of the application stays
  the same from one version to the next. The systems do not trust such a
  certificate: Gatekeeper and SmartScreen warn at the first start.
- The archive is signed with zipsign. The private key and the certificate
  are secrets of the repository (`ZIPSIGN_KEY`, `BINARY_SIGN`,
  `BINARY_SIGN_PASSWORD`); the public key is
  `crates/mbook-ui/assets/update.pub`, built into the application.
- The check asks the GitHub releases of the repository for the latest
  version and compares it with the version of the package. An update is
  offered once that release has the archive for the running platform. The
  check runs at start and once a day in release builds and can be turned off
  in the settings.
- `self_update` is used with its blocking `ureq` transport inside a
  background task. The application has no second runtime.
- An update is offered, never installed unasked. It is downloaded to
  `update/` under the cache root.
- A downloaded archive **MUST** be verified against a public key built into
  the application before anything is replaced (the `signatures` feature).
- Installing waits while an import is running or a book is open in the
  editor, and so does the restart that follows it: it must not replace the
  application under a conversion, nor restart it over unsaved changes.

| Platform | What is replaced |
| --- | --- |
| macOS | The whole application bundle, from a `.tar.gz`, so that its signature stays valid |
| Windows | Nothing by the application: the installer from the verified archive is started, and replaces the version itself |
| Linux | Nothing: no release is built for it |

### Errors

- The library has one `Error` enum, non-exhaustive, built with `thiserror`.
  The errors of `mbook`, `mbook-converter`, the store and the decoder are its
  sources.
- An error is turned into a sentence for the user in the view that shows it,
  not where it arises.
- A failure of one book, chapter or picture **MUST NOT** close the
  application or another book.
- A failure is never kept to itself: a catalogue that cannot be read or
  changed is reported on the library, with a way to try again, and settings
  that cannot be saved on the settings screen.

### Dependencies

Checked on 2026-11-21 against crates.io. A crate only this package uses is
declared in its own manifest; the manifest of the workspace keeps what
several packages share.

| Crate | Version, last release | License | Used for |
| --- | --- | --- | --- |
| `gpui-kit`, feature `gpui-fast` | 0.10.0, 2026-11-16 | Apache-2.0 | The interface; brings GPUI Fast 0.1 |
| `rfd` | 0.17.2 | MIT | Native book and picture pickers with extension filters |
| `mimalloc3` | 0.1.61, 2026-11-17 | MIT | The allocator of the binary |
| `dirs` | 7.0.0, 2026-09-05 | MIT OR Apache-2.0 | The roots on desktop |
| `self_update` | 1.3.3, 2026-10-27 | MIT | Updates; Rust 1.88 |
| `redb` | 5.0.0, 2026-11-18 | MIT OR Apache-2.0 | The catalogue; Rust 1.90 |
| `avif-decode` | 3.0.0, 2026-09-20 | BSD-3-Clause | Decoding pictures; Rust 1.98 |
| `image` | The version and features of the workspace; GPUI brings the same crate | MIT OR Apache-2.0 | Decoding and AVIF encoding of pictures added in the editor; scaling |
| `futures` | already in the tree of GPUI | MIT OR Apache-2.0 | The channel of the editing task |
| `merman` | 0.11.0 | MIT OR Apache-2.0 | Draws Mermaid as SVG |
| `tree-sitter-mermaid`, `codebook-tree-sitter-latex` | 0.3.5 and 0.6.1 | MIT | Syntax highlighting in the Mermaid and KaTeX source dialogs |
| `ratex-parser`, `ratex-layout`, `ratex-types`, `ratex-svg`, `ratex-font`, `ratex-unicode-font` | 0.1.16 | MIT | Parses and sets KaTeX as SVG with embedded glyph outlines |
| `ab_glyph`, `serde_json` | 0.2 and 1 | Apache-2.0 and MIT OR Apache-2.0, respectively | System glyph metrics, Mermaid configuration and traversing the formula tree |
| `uuid`, `tempfile`, `thiserror`, `markdown` | The versions of the workspace | | Keys; files of imports; errors; block offsets |

`avif-decode` brings `rav1d` (BSD-2-Clause, a port of dav1d to Rust; last
release 1.1.0 of 2025-05-07) and `avif-parse` (MPL-2.0). The assembly
enabled by `avif-decode` for x86 and x86-64 requires NASM at build time.
Literata's font files are added to the repository under the SIL Open Font
License 1.1.

Ruled out:

| Crate | Reason |
| --- | --- |
| `image` with `avif-native` | Needs the C library dav1d on the machine that builds and on the one that runs |
| `zenavif`, `rav1d-safe` | AGPL-3.0, or a commercial license |
| `rusqlite` | See [Rationale and alternatives](#rationale-and-alternatives) |

### Testing

- Logic sits in models and in plain functions: the size class, the import
  state machine, the catalogue, progress, block offsets, the outline. Each
  has unit tests beside it.
- Models and views are tested in headless windows with the `test-support`
  feature of `gpui-kit`: a test imports a book built in a temporary root,
  opens it, edits it and saves it.
- The rule of [Threads](#threads) is tested: with the background executor
  held back, a view **MUST** still draw, showing its loading state.
- Every layout is drawn at 390, 800 and 1280 px of width. With the Cargo
  feature `visual`, `cargo test -p mbook-ui --features visual --test visual`
  renders every screen without a window, in these widths and in a light
  and a dark page tone, into `target/visual/`: 136 pictures for a person to
  look at. The renderer exists on macOS; elsewhere the test says so and
  passes.
- Accessibility is tested, not reviewed:
  - a unit test computes the contrast of every pair of tokens that
    [Accessibility](#accessibility) puts together;
  - for each screen a test reads the accessibility tree of a headless
    window and checks that every application control has a role and a
    name. The window buttons drawn by GPUI Kit are outside this check;
  - for each screen a test completes its main task with keys alone: add a
    book, open it, move to another chapter, edit and save.

### What is implemented

The first version implements the sections above with the differences below.
The headless tests cover layout and interaction. The redesigned screens were
also exercised on macOS with an imported illustrated book. Screen-reader
use and native builds on Windows and Linux still need verification.

Seen in the pictures and left as it is: a notification stays over the right
end of the header for a while. A long word on a cover drawn from a title
can break in its middle.

| Part | State |
| --- | --- |
| Library | **New book…** creates a titled book and opens its first chapter in the editor. Cover tiles show a new badge, reading percentage or completion status below the cover. The card of the book being read shows how far it was read, not the name of its chapter: the catalogue stores the path of a chapter, not its title. A change of the catalogue that failed is followed by a reload, and a reload that succeeds clears the message, so that failure may pass unseen. |
| Import | "Add a book…" and the Open key show a picker filtered to supported book extensions. Files can also be dropped on the window. Opening a file with the application from the system, and a file named on the command line, are not handled. |
| Reader | Whole-book text search, single-page reading, whole-book page numbers with estimates until all chapters are measured, and auto-hiding controls with a position slider that appears on hover or while in use. Pictures and diagrams scale to fit a page. Mermaid diagrams and KaTeX formulas use the page tone and display alternative text when rendering fails. Other tall blocks can be clipped through a line. |
| Editor | Chapter search works on the current draft in Visual, Source, Split and Preview. Editing opens the first chapter in Visual. Chapters are listed and chosen by number and title. Visual supports tables, nested and task lists, quotes and code blocks. A link is made from selected or newly entered words and can lead to a chapter or a heading chosen by title. A picture is chosen from the thumbnails of the book or taken from a file, with a description, and put at the caret. The Contents section shows the contents and where they come from. The entries of a stored list are edited, added, nested and removed one by one, are kept in the order of the book, and are marked when they lead nowhere or are out of order. They follow their chapter when it moves or its file is renamed, and are removed with it. The Landmarks section adds, edits and removes `notes` landmarks. The picture gallery shows thumbnails and picture import offers encoding settings. Diagrams and Formulas have syntax-highlighted source dialogs, a preview and removal controls. Visual inserts figure references, and Preview and Split render them. Footnotes and reference definitions are edited in Source. Source and Split highlight Markdown syntax. The platform Save shortcut works while editing. A picture cannot be put into the text from Source, and choosing a contents entry opens its chapter at the top. Dropping a file on a chapter has not been tried outside of tests, which call the handler directly. |
| Settings | As designed. The size of the interface is applied when the slider is released, because it moves the slider. |
| Updates | The check and the installation are written and tested against scripted releases and signed archives made by the tests. The installer is compiled on macOS and Windows. |
| Release workflow | The workflow produced installers and update archives for v0.1.0. The Trusted Publishing step awaits its first run. The macOS and Windows x64 jobs make NASM available before building. |
| Typeface | Literata is available when installed locally. The fallback chain supplies another installed serif family when possible. Bundled fonts are unimplemented. |
| Transitions | Page turns within a chapter slide and fade, and the reading controls fade. Screen navigation and chapter changes are immediate. Reduced motion uses immediate transitions. |

Where assistive technology gets less than [Accessibility](#accessibility)
asks:

- **The structure of a chapter is not reported.** The Markdown view of GPUI
  Kit gives its headings, lists and tables no roles. The text of a chapter
  is one element with the article role, named by the title of the chapter.
- **The language of a book is not reported**: GPUI has no property for it.
- **The link in the About dialog cannot be reached by keyboard**: the link
  of GPUI Kit takes no focus.
- A few parts of Kit controls are reported without a role: the frame of a
  text field, the search field inside a select, the track of a slider. The
  controls they belong to have roles and names.
- The Kit's minimize, maximize, restore and close buttons lack roles in
  the headless tree. On Windows, GPUI handles their hit testing in the
  window's non-client area.

### Security considerations

- A book is untrusted input. Its limits are those of `mbook` and
  `mbook-converter`; the application adds none of its own parsers.
- Markdown of a book has no raw HTML and no scripts
  ([ADR-0006](adr/0006-markdown-dialect.md)). A picture is only ever taken
  from the book; a network address in a picture is not fetched.
- An external link is opened by the system browser, and only when the user
  follows it.
- An update replaces the program. It is accepted only with a valid
  signature, over TLS, from the releases of this repository.

## Drawbacks

- **A large dependency.** GPUI and its component library are far larger
  than the rest of the workspace together, and the engine is young: GPUI Fast
  is at version 0.1.
- **The checks of the repository grow.** Clippy, tests and documentation of
  the workspace now build GPUI, and continuous integration needs a display
  server's libraries on Linux.
- **An editing session holds the whole book decoded**, as `Book` does. A
  comic of several hundred pages may not fit on a phone.
- **Accessibility depends on a young integration.** What a screen reader
  hears is what GPUI and AccessKit pass on; a gap there is a gap here.
- **A running conversion cannot be stopped**, only ignored. Neither can the
  encoding of a picture, which takes seconds for a large one.
- **Large blocks.** A page boundary through an oversized code block, list or
  table is limited by the geometry the Markdown view exposes.
- **Two copies of a book** after import: the original and the one in the
  library.
- **A stored place is approximate after an edit** of its chapter.
- **A second minimum Rust version** in the workspace.
- **Android has no backend** in this stack today; see
  [Unresolved questions](#unresolved-questions).

## Rationale and alternatives

| Decision | Chosen | Rejected | Why |
| --- | --- | --- | --- |
| Toolkit | GPUI Kit on GPUI Fast | | Decided by the format author. |
| File picker filters | Native dialogs through `rfd` | GPUI file prompts | GPUI Kit 0.10 cannot pass extension filters to its platform picker. |
| Where CPU-bound work runs | The background executor of GPUI | A separate thread pool | Decided by the format author. One scheduler, with priorities, whose tasks are cancelled by dropping them. |
| What decides a layout | The width of the window | The platform | A narrow desktop window and a phone need the same layout; a platform test gives two code paths that drift apart. |
| Reading | `BookReader`, lazily | `Book` for everything | A `Book` decodes every chapter and picture before the first line is shown. |
| Finding text in a book | Scan chapter text on demand | Index the whole book at open | Opening a book stays lazy. Search retains its current match and reads no pictures, with a chapter scan for each step. |
| Access to the reader | A snapshot on the main thread; the source behind a mutex for background tasks | Table lookups on the main thread | A lookup would wait for a mutex that a read holds during I/O. |
| Rendering Markdown | The Markdown view of GPUI Kit | A renderer of this crate | It virtualises, selects text, parses off the main thread, and is built on the same `markdown` crate as `mbook`. |
| Paginating a chapter | Break the chapter using the measured GPUI Kit blocks | A custom Markdown renderer or a fixed `max_lines` value | Measurements include proportional wrapping, headings and images while chapter-wide references stay with the Kit. |
| Whole-book page numbers | Combine measured chapters and estimates for the current layout | A fixed count in the file or a full-book layout pass on open | Reflow changes the rendered count. Estimates let the book open lazily, and each measured chapter replaces its estimate. |
| Seeking within the book | Use the decoded chapter lengths and an offset | Index every page before allowing a seek | Chapter lengths are already in the snapshot and locate a destination even before its chapter is measured. |
| Stored place | Chapter path and byte offset | Resource number and offset | Numbers change when a book is written again. |
| Creating a book | Create a titled book with one empty chapter and open it in the editor | Ask for chapter paths before opening the editor | A writer can start with the title and go straight to writing. The title and chapter paths remain editable. |
| Editing | One background task owns the `Book`; commands in order | A `Book` behind a mutex, one task per change | Separate tasks may run out of order: a save could miss the edit before it. |
| Visual chapter editing | Use the GPUI Kit block editor and restore intact source blocks with `similar` | Extend the inline Markdown projection | The block editor provides table and list structure, selection and undo. Restoring unchanged blocks protects source spelling and heading anchors. |
| How a chapter is named in the editor | Its number and the title of its first heading; the path beside it | The path alone | An author knows a chapter by its title. The path stays in sight because links and problems name it. |
| Adding a chapter | Ask for a title; propose the path | Ask for a path | The title is what the author has in mind, and it becomes the heading the contents are built from. The path can be changed afterwards. |
| Putting a picture into a chapter | One dialog in the chapter: the pictures of the book, or a file that is added on the way | Add the picture in Pictures first, then pick its path from a menu in the chapter | The second way takes three screens and needs the name of a file to be remembered. |
| Where chapter actions are | A menu with words: Move up, Move down, Rename the file, Remove the chapter | A row of icons beside the views | Arrows beside a title read as "previous" and "next". The actions are rare and one of them cannot be undone. |
| Where the contents are explained | A section of their own that shows them | Only the control that rebuilds them among the problems | An author who asks how to change the contents finds neither a list nor a rule otherwise. |
| The order of a stored contents list | The editor keeps it: an entry sits where its place is in the book | Controls that move an entry up and down | The format refuses a list that is not in reading order, so almost every move by hand would be a mistake to undo. A moved chapter takes its entries along, a removed one takes them away, and one control sorts a list that an edit of the text left out of order. |
| The level of a contents entry | Two menu items that move it in or out with the entries under it | A level number in the dialog | The level that is allowed depends on the entries around it. Moving in and out, as an outliner does, cannot produce a list the format refuses. |
| The places a link can lead to | Carried in the outline, from the anchors `Book` keeps | A public call of `mbook` that lists the contents | RFC 0002 stays as it is. The price is that the rule which lists a heading is written here a second time, with a test. |
| Escaping the words the editor writes into Markdown | A function of this crate, tested by reading the result back with `markdown` | `mdast_util_to_markdown` 0.0.2 | That crate is a pre-release without the GFM extensions. `mbook-converter` escapes with a function of its own for the same reason ([RFC 0003](0003-mbook-converter-api.md)); it is not public, and RFC 0003 stays as it is. A link needs none of it: the block editor types and links the words. |
| Where an edit is saved | Over the book in the library, by a rename | Always a new copy | Decided by the format author. The library is the working copy; the original file was never touched. |
| Catalogue | `redb` | SQLite through `rusqlite` | Decided by the format author. Pure Rust: nothing to cross-compile for a phone. The price is no SQL; a library is filtered in memory. |
| | | No catalogue; open every file at start | Reading places and settings need a store anyway, and a shelf of thousands should not wait for thousands of files. |
| Source of truth for metadata | The mbook files | The catalogue | An edit changes the file; a lost catalogue then costs no book. |
| Key of a book | A UUID made at import | The `id` of the metadata | That `id` is any string and may be edited. |
| Directories | `dirs`, handed in by the shell | Paths built in the library | Decided by the format author; handing them in keeps tests and mobile shells possible. |
| Installing updates | `self_update`, blocking transport on the background | Its asynchronous transport | Decided by the format author to use the crate. The blocking transport needs no Tokio runtime beside GPUI. |
| AVIF decoder | `avif-decode` on `rav1d` | dav1d in C | Decided by the format author. No C toolchain; the same reason as for the encoder in RFC 0003. |
| Minimum Rust version | 1.100 for this package | The 1.89 of the workspace | Decided by the format author. `avif-decode` needs 1.98 and `redb` 1.90; an application is built by its maintainers, not by users of a library. |
| Typeface of books | Literata when installed, then local serif fallbacks; any installed family in settings | A fixed set of bundled fonts | The fallback chain keeps book typography available without font assets in the repository. |
| Building a release | A workflow started by hand for a tag | Building on every tag | Decided by the format author. The workflow publishes the libraries for the tag before building the application. |
| Drawing diagrams and formulas | Render source with merman and RaTeX, then give SVG to GPUI | Convert sources to AVIF; render with a web view | SVG lets the reader choose its own page colours and sizes. The source remains editable and rendering stays local. |
| Pictures added in the editor | Encoded here, with `image` | A public encoder in `mbook-converter` | Decided by the format author. RFC 0003 stays as it is; the price is that quality and speed are written in two crates. |
| Size class under a scaled interface | Width divided by the scale | Width in pixels | At 200% a desktop window has room for the compact layout only; clipping or a horizontal scroll bar fails WCAG 1.4.10. |
| Reordering chapters | A drag, and "Move up" and "Move down" | A drag alone | WCAG 2.5.7: a drag needs an alternative. |
| Thumbnails | JPEG files in the cache | Decoding every cover at start | The shelf then needs no AVIF decoder to appear. |

## Prior art

| Application | Taken from it | Left behind |
| --- | --- | --- |
| Apple Books for the Mac | Cover-first shelves, single-page reading with quiet controls, an always-visible page number, a reading position slider, and page tones | Store, two-page spreads, collections, and annotations |
| Calibre | A library that owns copies of the books; converting on import; an editor with source and preview | A database as the source of truth; a window per tool |
| Zed | Entities as models; tasks owned by their entity; nothing blocking on the main thread | |

## Unresolved questions

*Proposed* by this draft and not confirmed by the format author:

| Question | Proposed |
| --- | --- |
| How a row of the catalogue is encoded | The value types of `redb`, without a serialisation crate |
| Notarising the macOS build, and a certificate of an authority for Windows | Not done: the certificate is self-signed |
| A release for Linux | Not built |
| The typefaces offered beside Literata | Every family the device has |
| The limits of conversion and of the picture cache on a compact device | Not chosen |

Needs a change outside this crate:

- **Progress and cancellation of a conversion** are future possibilities of
  RFC 0003; until then an import shows no percentage. The same holds for a
  picture being encoded in the editor.

The first version answered the questions a spike was to answer:

| Question | Answer |
| --- | --- |
| Do the macros of GPUI compile under the lints of the workspace? | Yes, with `unsafe_code = "forbid"` and Clippy's `pedantic` denied. |
| Does the Markdown view of GPUI Kit tell which block is at the top? | It lays out one item per top-level block, in the order of the source, but does not give its offset. The application keeps the offsets `markdown` finds and maps one to the other. |
| Does it reserve the size of a picture that is loading? | No: a pending picture is measured as a small square. The application hands it a transparent picture of the final size first. |
| Can GPUI announce a message, and give an element a language? | It has no live regions and no language. See [Accessibility](#accessibility) and [What is implemented](#what-is-implemented). |
| Is there a GPUI backend for Android? | Not looked at: the first version is for desktop. |

Not answered: how the Markdown view behaves with one chapter of several
megabytes, and whether the system's setting for more contrast reaches the
application.

## Future possibilities

- **Two-page spreads**, which need two independently positioned columns.
- **Scrolling from one chapter into the next.**
- **Shells for iOS and Android.**
- **Highlights, notes and bookmarks**, stored beside the reading place.
- **Collections** in the library.
- **A journal of unsaved edits**, replayed after a crash.
- **Undo for changes of structure.**
- **Links that follow a renamed chapter**: the links of other chapters
  into it would be rewritten.
- **Turning automatic contents into a list**, for an author who wants to
  leave headings out of the contents.
- **Showing where a picture is used**, and offering to remove the ones no
  chapter refers to.
- **Exporting** a book of the library as a file.
- **Conversion progress and cancellation**, when RFC 0003 has them.
