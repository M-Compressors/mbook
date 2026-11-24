# Using the application

What a person sees and does on each screen. How the screens are built is on
[Architecture](architecture.md).

## Size classes

The layout follows the width of the window. A narrow desktop window gets the
phone layout, and a tablet in landscape gets the desktop one.

| Class | Window width | Navigation | Typical device |
| --- | --- | --- | --- |
| Compact | below 600 px | A stack of screens; a bar at the bottom | Phone; a narrow window |
| Medium | 600 to 959 px | A stack; panels slide over the content | Tablet in portrait; half a screen |
| Expanded | 960 px and more | A sidebar beside the content; panels dock | Desktop; tablet in landscape |

## Library

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
  Edit, Show the file, Export, Share, Remove.
- **Share** opens the share sheet of the system with the mbook file of the
  book: AirDrop, Messages and Mail on macOS, Nearby Sharing and installed
  apps on Windows, and the messengers and social networks installed there
  that accept a file. What happens next is up to the app chosen.
- Files are added with `+`, by dropping them on the window, or by opening
  them with mbook from a file manager or another app, which offers mbook for
  every format it reads; one file opened this way opens its book. The system
  picker lists mbook and the extensions accepted by the converter.
- **New book…** asks for a title. Confirming creates a book in the library
  and opens its empty first chapter in the Markdown editor. The title and
  language can be changed in **Details**.

## Reader

- Read in one scrolling column. Page Up and Page Down move by a screen;
  Home and End move to the chapter boundaries. The footer and chapter
  shortcuts can advance to the next chapter, and so does a swipe to the
  left, with a finger or two on a trackpad; a swipe to the right goes to
  the previous one. **Read one page at a time** adds page turns and
  book-wide page numbers; there a swipe turns a page. A number is
  approximate until the reader has measured every chapter at the current
  typography.
- The reading slider previews the chapter and progress while seeking.
  It appears on hover, while in use, or on keyboard focus. Progress can
  be hidden from the appearance panel. While the controls are hidden, the
  header keeps its space, and so does the footer of one page at a time. A
  scrolling text runs to the bottom of the window under its footer, which
  stays; the text fades into the page behind its buttons and status, and
  scrolled to the end of a chapter, its last line stops above them. Move the
  pointer, tap the middle of the text, scroll back or press a key to show
  the controls; they also come back at the end of a chapter. A tap or a
  swipe on a notification is the notification's, not the text's.
- A book opened for the first time opens where its text begins, past the
  cover and the title page, when the book marks that place. After that it
  reopens where it was left.
- **Contents** marks the current entry. When the book marks notable places,
  such as its notes, glossary or index, the panel lists them first under
  **Go to**, by the name the book gives them or the name of their kind, and
  the entries follow under **Contents**. Choosing one goes there, and Escape
  or **Return to previous passage** comes back. Contents and **Aa** open in
  popovers on expanded windows and bottom sheets on smaller windows.
- **Aa** offers typeface, size and theme. Advanced appearance includes line
  height, width, paragraph spacing, heavier text, paging, motion and
  progress visibility. Reset appearance keeps the current reading place.
- **Find in book** (Command+F or Control+F) searches visible chapter text
  without regard to case. Matches wrap across chapters. The bar shows the
  chapter and excerpt. For supported Markdown spans the text is highlighted.
  Use the arrows, Return or Shift+Return to move between results. Closing
  the bar takes the reader back to the passage where they opened it.
- Click or tap a picture, a diagram or a formula that stands alone in its
  paragraph to see it over the whole window. It opens fitted to the window.
  **Zoom in** and **Zoom out** change its size a step at a time, as do `+`
  and `-` (with or without Command or Control), a pinch, or the wheel with
  Command or Control held. A step stops at the size the picture has in the
  book and at the fitted size; **Fit to window** or `0` goes back to the
  fitted size. An enlarged picture is moved by dragging it or by scrolling.
  Escape, **Close** or a click beside the picture closes it. A picture inside
  a line of text, a picture that is a link and a reference-style picture
  `![…][name]` do not open.
- Links, GFM footnote references and contents entries can be followed and
  revisited with Escape or **Return to previous passage**.
  A web link opens in the system browser. A book reopens at its saved
  chapter and source offset.

## Editor

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
Compact: it opens in the chapter; the menu button beside Back slides the
same list over it. Back leaves the editor.
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
- **Landmarks** marks the places a reading application can take a reader
  to ([RFC 0001](../rfcs/0001-mbook-format.md#landmarks-1)). The section
  opens with what that gives a reader, then shows two parts:
  - **Where the text begins**, in a block of its own with a band of the
    accent: where a new book opens instead of at its cover or title page.
    The block says whether the place is marked, and **Mark where the text
    begins…**, **Change…** and **Remove** set or clear it. A book has one.
  - **Other places**: every other landmark, listed by the name a reader is
    shown, with its kind beside a title that stands in for it and where it
    leads under it. **Add a landmark…** asks what the place is, the name
    readers see and where it is. The kinds are grouped by what a reader
    looks for: looked up while reading (notes, glossary, index and the
    like), parts of the text, and pages about the book. Under the choice a
    line says what a reader gets from it. The name is optional, and the
    name of the kind is its placeholder; a name that only repeats the kind
    is not stored. A new landmark is put among the others in the order of
    the book. A kind the editor does not know, such as one for private use,
    is listed as the book writes it and kept when its place is changed.

  Each landmark leads to a place chosen like the destination of a link: a
  chapter, and its start or a named place in it. A landmark keeps leading to
  its chapter when the file of the chapter is renamed, and is removed with
  the chapter.
- **Problems** lists what `Book::check` reports, each with its chapter and
  place; choosing one opens that place. A book with problems is not saved.
- **Save** replaces the book in the library. For a book with automatic
  contents, Save builds the contents from the current chapter headings.
  Use Command+S on macOS or Ctrl+S on other platforms. On macOS the File
  menu also offers Save.

## Settings

One screen, reached from the library: a page on an expanded window, a pushed
screen on a compact one.

| Setting | Choices | Default |
| --- | --- | --- |
| Typeface of books | Literata, Noto Sans, System and installed families | Literata |
| Size of the text | 16 to 80 logical px in Aa, with a shortcut to 44 px | 20 compact / 22 wider at first opening |
| Line height of the text | 1.30 to 2.00 in 0.05 steps | 1.60 |
| Page tone | Paper, Neutral, Sepia, Night, High contrast, or Follow the system | Follows the system |
| Wide-window column | 45 to 80 zero-glyph advances | 60 |
| Paragraph gap | 0 to 2 em | 0.7 em |
| Heavier text | 400 or 500 where the family supplies it | 400 |
| Reading layout | Scroll or pages | Scroll |
| Page-turn animation | Off or on, subject to reduced motion | Off |
| Reading progress | Visible or hidden | Visible |
| Language | System language, English, Русский | System language |
| Size of the interface | 100% to 200% | 100% |
| Controls of the reader | Hide while reading, or always shown | Hide while reading |
| Motion | As the system says, or reduced | As the system says |
| Check for updates | On, off | On |

A change shows at once in an open book. A page tone that follows the system
is Paper while the system is light and Night while it is dark.

The interface is in English and in Russian. With **System language** it is
in the first language of the system it is translated into, and in English
when the system prefers none of them; the choice names the language it
stands for, and each language is named in itself, so that a person who
cannot read the current one finds theirs. A new language shows at once,
the menu bar of macOS included; a message already on the screen keeps its
language ([Languages](architecture.md#languages)).

## Using the application without a pointer or without sight

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
