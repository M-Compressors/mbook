# Design

Which decisions are confirmed, which crates the application is built on, what
the design costs, and the choices behind it.

## What is settled

The format author settled the points below on 2026-11-21. Every other detail
of this section is *proposed*, and listed under
[Settled and open questions](#settled-and-open-questions) where a choice is
open.

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
| Literata is the default reading choice and the typeface is configurable | Confirmed. Literata and Noto Sans are bundled. |
| The modules follow Feature-Sliced Design | Confirmed |
| A release is a tag on GitHub; its binaries are built by a workflow started by hand for that tag | Confirmed |
| Releases are signed with keys generated for the project and kept as secrets of the repository | Confirmed |
| The icon is an open book on the navy of the design documents | Confirmed |
| Saving an edit replaces the book in the library | Confirmed |
| The editor accepts pictures that are not AVIF and encodes them with the `image` crate | Confirmed |
| The catalogue is a `redb` database | Confirmed |

## Dependencies

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
The font sources and notices are listed in
[Reading verification](status.md#reading-verification).

Ruled out:

| Crate | Reason |
| --- | --- |
| `image` with `avif-native` | Needs the C library dav1d on the machine that builds and on the one that runs |
| `zenavif`, `rav1d-safe` | AGPL-3.0, or a commercial license |
| `rusqlite` | See [Rationale and alternatives](#rationale-and-alternatives) |

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
  [Settled and open questions](#settled-and-open-questions).

## Rationale and alternatives

| Decision | Chosen | Rejected | Why |
| --- | --- | --- | --- |
| Toolkit | GPUI Kit on GPUI Fast | | Decided by the format author. |
| File picker filters | Native dialogs through `rfd` | GPUI file prompts | GPUI Kit 0.10 cannot pass extension filters to its platform picker. |
| Export destination | Native save dialog and temporary file in the destination directory | Convert directly into the chosen file | The completed file is published in one operation and an existing file cannot be overwritten. |
| Where CPU-bound work runs | The background executor of GPUI | A separate thread pool | Decided by the format author. One scheduler, with priorities, whose tasks are cancelled by dropping them. |
| What decides a layout | The width of the window | The platform | A narrow desktop window and a phone need the same layout; a platform test gives two code paths that drift apart. |
| Primary flow | Continuous vertical scrolling per chapter, with saved pagination as an alternative | Paged-only navigation | Scroll uses the native gesture path and leaves ordinary text free of page masks. |
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
| The places a link can lead to | Carried in the outline, from the anchors `Book` keeps | A public call of `mbook` that lists the contents | The API of `mbook` stays as it is. The price is that the rule which lists a heading is written here a second time, with a test. |
| Escaping the words the editor writes into Markdown | A function of this crate, tested by reading the result back with `markdown` | `mdast_util_to_markdown` 0.0.2 | That crate is a pre-release without the GFM extensions. `mbook-converter` escapes with a function of its own for the same reason ([Dependencies](../mbook-converter/design.md#dependencies)); it is not public, and the API of `mbook-converter` stays as it is. A link needs none of it: the block editor types and links the words. |
| Where an edit is saved | Over the book in the library, by a rename | Always a new copy | Decided by the format author. The library is the working copy; the original file was never touched. |
| Catalogue | `redb` | SQLite through `rusqlite` | Decided by the format author. Pure Rust: nothing to cross-compile for a phone. The price is no SQL; a library is filtered in memory. |
| | | No catalogue; open every file at start | Reading places and settings need a store anyway, and a shelf of thousands should not wait for thousands of files. |
| Source of truth for metadata | The mbook files | The catalogue | An edit changes the file; a lost catalogue then costs no book. |
| Key of a book | A UUID made at import | The `id` of the metadata | That `id` is any string and may be edited. |
| Directories | `dirs`, handed in by the shell | Paths built in the library | Decided by the format author; handing them in keeps tests and mobile shells possible. |
| Installing updates | `self_update`, blocking transport on the background | Its asynchronous transport | Decided by the format author to use the crate. The blocking transport needs no Tokio runtime beside GPUI. |
| Update banner placement | Below the title bar | Above the whole page | Native macOS window controls stay at the top of the window when the banner appears. |
| AVIF decoder | `avif-decode` on `rav1d` | dav1d in C | Decided by the format author. No C toolchain; the same reason as for the encoder of `mbook-converter`. |
| Minimum Rust version | 1.100 for this package | The 1.89 of the workspace | Decided by the format author. `avif-decode` needs 1.98 and `redb` 1.90; an application is built by its maintainers, not by users of a library. |
| Typeface of books | Bundled Literata and Noto Sans, plus System and installed choices | Only installed families | Rendering is available offline on a fresh installation. |
| Building a release | A workflow started by hand for a tag | Building on every tag | Decided by the format author. The workflow publishes the libraries for the tag before building the application. |
| Drawing diagrams and formulas | Render source with merman and RaTeX, then give SVG to GPUI | Convert sources to AVIF; render with a web view | SVG lets the reader choose its own page colours and sizes. The source remains editable and rendering stays local. |
| Pictures added in the editor | Encoded here, with `image` | A public encoder in `mbook-converter` | Decided by the format author. The API of `mbook-converter` stays as it is; the price is that quality and speed are written in two crates. |
| Size class under a scaled interface | Width divided by the scale | Width in pixels | At 200% a desktop window has room for the compact layout only; clipping or a horizontal scroll bar fails WCAG 1.4.10. |
| Reordering chapters | A drag, and "Move up" and "Move down" | A drag alone | WCAG 2.5.7: a drag needs an alternative. |
| Thumbnails | JPEG files in the cache | Decoding every cover at start | The shelf then needs no AVIF decoder to appear. |

## Prior art

| Application | Taken from it | Left behind |
| --- | --- | --- |
| Apple Books for the Mac | Cover-first shelves, single-page reading with quiet controls, an always-visible page number, a reading position slider, and page tones | Store, two-page spreads, collections, and annotations |
| Calibre | A library that owns copies of the books; converting on import; an editor with source and preview | A database as the source of truth; a window per tool |
| Zed | Entities as models; tasks owned by their entity; nothing blocking on the main thread | |

## Settled and open questions

*Proposed* and not confirmed by the format author:

| Question | Proposed |
| --- | --- |
| How a row of the catalogue is encoded | The value types of `redb`, without a serialisation crate |
| Notarising the macOS build, and a certificate of an authority for Windows | Not done: the certificate is self-signed |
| A release for Linux | Not built |
| The limits of conversion and of the picture cache on a compact device | Not chosen |

Needs a change outside this crate:

- **Progress and cancellation of a conversion** are
  [future possibilities of `mbook-converter`](../mbook-converter/design.md#future-possibilities);
  until then an import shows no percentage. The same holds for a
  picture being encoded in the editor.

The first version answered the questions a spike was to answer:

| Question | Answer |
| --- | --- |
| Do the macros of GPUI compile under the lints of the workspace? | Yes, with `unsafe_code = "forbid"` and Clippy's `pedantic` denied. |
| Does the Markdown view of GPUI Kit tell which block is at the top? | It lays out one item per top-level block, in the order of the source, but does not give its offset. The application keeps the offsets `markdown` finds and maps one to the other. |
| Does it reserve the size of a picture that is loading? | No: a pending picture is measured as a small square. The application hands it a transparent picture of the final size first. |
| Can GPUI announce a message, and give an element a language? | It has no live regions and no language. See [Accessibility](appearance.md#accessibility) and [What is implemented](status.md#what-is-implemented). |
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
- **Conversion progress and cancellation**, when `mbook-converter` has them.
