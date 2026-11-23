# mbook-ui

A reading application for [mbook](https://github.com/M-Compressors/mbook)
files: a library of books, a reader and an editor, built with GPUI Kit.

- **Your books, smaller.** Import an EPUB, FB2, PDF, comic archive, Word,
  HTML, Markdown or text file and it becomes an mbook file; an illustrated
  7.0 MB EPUB [became 1.6 MB][measured]. Export gives any of the same
  formats back.
- **No waiting.** A book opens from five small tables and each chapter is
  decoded on its own, off the main thread.
- **No web engine inside.** Chapters are Markdown, pictures are AVIF,
  formulas and diagrams are drawn from their source in the size and tone of
  the page.
- **Reader and editor in one.** What the editor previews is what the reader
  shows.

**[Using the application][guide]** · [Architecture][architecture] · [Appearance and accessibility][appearance] · [Status][status] · [Design][design]

## Install

Installers for macOS (arm64 and x86-64) and Windows (x86-64) are on
[GitHub Releases](https://github.com/M-Compressors/mbook/releases). They are
signed with a self-signed certificate, so Gatekeeper and SmartScreen warn at
the first start. There is no Linux build yet.

## Run from source

```sh
cargo run -p mbook-ui
```

Requires Rust 1.100 or later. The AVIF decoder requires NASM at build time
on x86 and x86-64. The executable is called `mbook`.

| Cargo feature | Default | Enables |
| --- | --- | --- |
| `self-update` | yes | Checking the releases of the repository for a newer version and installing it; a build for a store or a package manager leaves it off |
| `visual` | no | Rendering every screen to a PNG file in a headless window, for `tests/visual.rs` |

## What it does

| Screen | What you do there |
| --- | --- |
| **Library** | Keep books as covers on a shelf, search and sort them, add files with `+` or by dropping them on the window, create a new book, export one in any of eight formats, send its file through the share sheet of the system |
| **Reader** | Read one scrolling column or one page at a time from where the text begins, follow links and footnotes and return, go to the notes, the glossary or the index the book marks, open a picture or a diagram over the whole window and zoom it, search the whole book, choose typeface, size and one of five page tones |
| **Editor** | Write chapters as blocks, as Markdown source or side by side with a preview; put in pictures, links, diagrams and formulas; edit the details, the cover, the contents and the landmarks; see what `Book::check` reports |
| **Settings** | Typography, page tone, the size of the interface, motion and updates |

The layout follows the width of the window, not the platform: below 600 px
it is a stack of screens with a bar at the bottom, from 960 px a sidebar
beside the content.

| Key | Action |
| --- | --- |
| The platform's Open, Save, Find, Settings | The same in the application |
| Page Up, Page Down, Home, End | Move in a chapter |
| The platform's bracket keys | Previous and next chapter |
| Space | Turn a page |
| `+`, `-`, `0` | Zoom in, zoom out and fit a picture opened over the window |
| Escape | Close a picture or a panel; return to the previous passage; go back |

## How it is built

- **The interface never waits for a book.** Everything that reads a file,
  decodes, parses, converts or encodes runs on the background executor of
  GPUI; the main thread only draws and reacts.
- **Reading is lazy.** The reader holds a `BookReader` and decodes the
  chapter and the pictures that are on screen. The editor holds a whole
  `Book`, owned by one background task that applies commands in order.
- **The mbook files are the source of truth.** The catalogue is a `redb`
  database that can be rebuilt from the files; a lost catalogue costs
  reading places and settings, not books.
- **A failed import or save loses nothing.** A file enters the library, and
  replaces a book, only by a rename.
- **The modules follow Feature-Sliced Design**: `app`, `pages`, `widgets`,
  `features`, `entities`, `shared`. The binary is a thin shell around
  `mbook_ui::run`.

```text
<data dir>/mbook/
    catalog.redb            the catalogue: books, reading places, settings
    books/<key>.mbook       one file per book
<cache dir>/mbook/          may be deleted at any time
    covers/<key>.jpg        thumbnails of covers
    share/                  books handed to the share sheet, until the next start
```

## Testing

```sh
cargo test -p mbook-ui --locked
cargo test -p mbook-ui --features visual --test visual --locked
```

The second command renders every screen into `target/visual/` for a person
to look at; the renderer exists on macOS only.

## Status

A first version. Screen-reader use and native builds on Windows and Linux
still need verification; [Status and verification][status] lists what
exists, where it falls short of the design and how it was checked.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

[guide]: https://m-compressors.github.io/mbook/mbook-ui/guide.html
[architecture]: https://m-compressors.github.io/mbook/mbook-ui/architecture.html
[appearance]: https://m-compressors.github.io/mbook/mbook-ui/appearance.html
[status]: https://m-compressors.github.io/mbook/mbook-ui/status.html
[design]: https://m-compressors.github.io/mbook/mbook-ui/design.html
[measured]: https://m-compressors.github.io/mbook/rfcs/0001-mbook-format.html#a-converted-book
