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
| CPU-bound work runs on a pool of `rayon` with half the threads of the system, which the background executor awaits | Confirmed on 2026-11-24 |
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
| The application is built for Android; there it runs on gpui-pre with the platform of gpui-mobile, while the desktop stays on GPUI Fast | Confirmed on 2026-11-23 |
| The release workflow builds an APK | Confirmed on 2026-11-23 |

## Dependencies

Checked on 2026-11-21 against crates.io. A crate only this package uses is
declared in its own manifest; the manifest of the workspace keeps what
several packages share.

| Crate | Version, last release | License | Used for |
| --- | --- | --- | --- |
| `gpui-kit`, feature `gpui-fast` | 0.11.0, 2026-11-23 | Apache-2.0 | The interface; brings GPUI Fast 0.1 |
| `rfd` | 0.17.2 | MIT | Native book and picture pickers with extension filters |
| `robius-share`, desktop only | 0.3.0, 2026-07-18; Project Robius, repository pushed 2026-11-13 | MIT | The share sheet of the platform; Rust 1.77. On Windows it brings `windows` 0.56 beside the versions GPUI uses |
| `mimalloc3` | 0.1.61, 2026-11-17 | MIT | The allocator of the binary |
| `dirs` | 7.0.0, 2026-09-05 | MIT OR Apache-2.0 | The roots on desktop |
| `self_update` | 1.3.3, 2026-10-27 | MIT | Updates; Rust 1.88 |
| `redb` | 5.0.0, 2026-11-18 | MIT OR Apache-2.0 | The catalogue; Rust 1.90 |
| `avif-decode` | 3.0.0, 2026-09-20 | BSD-3-Clause | Decoding pictures; Rust 1.98 |
| `image` | The version and features of the workspace, with `nasm` on ARM; GPUI brings the same crate | MIT OR Apache-2.0 | Decoding and AVIF encoding of pictures added in the editor; scaling |
| `futures` | already in the tree of GPUI | MIT OR Apache-2.0 | The channel of the editing task; the future of work on the pool |
| `rayon` | The version of the workspace; already in the tree of `image`, `ravif` and `rav1e` | MIT OR Apache-2.0 | The pool of CPU-bound work |
| `tree-sitter-mermaid`, `codebook-tree-sitter-latex` | 0.3.5 and 0.6.1 | MIT | Syntax highlighting in the Mermaid and KaTeX source dialogs |
| `rust-i18n` | 4.2.4, 2026-09-29; Longbridge, the authors of GPUI Component | MIT | The texts of `locales/` and the language of the process; GPUI Component already uses it, so its controls and the application share one language |
| `sys-locale` | 0.3.2, 2024-11-01; 1Password | MIT OR Apache-2.0 | The languages of the system in order of preference, on every platform including Android |
| `icu_plurals`, `icu_locale_core` | 2.4.0, 2026-10-22; ICU4X of Unicode; `icu_locale_core` already in the tree | Unicode-3.0 | The plural category of a count |
| `uuid`, `tempfile`, `thiserror`, `markdown` | The versions of the workspace | | Keys; files of imports; errors; block offsets |
| `gpui-pre-mobile` (gpui-mobile), Android only | Commit `d3eddb8` of the fork [Mnwa/gpui-mobile](https://github.com/Mnwa/gpui-mobile), branch `gpui-pre-0.3.14`, 2026-11-27; upstream [longbridge/gpui-mobile](https://github.com/longbridge/gpui-mobile) pushed 2026-11-03 | GPL-3.0-or-later OR AGPL-3.0-or-later OR Apache-2.0, used under Apache-2.0 | The GPUI platform of Android: window, input, renderer, document picker and, with its `clipboard` feature, the clipboard of the system; its Java activities and emoji font go into the APK |
| `android-activity`, `android_logger`, `log` (in `mbook-android`) | 0.6, 0.15, 0.4 | MIT OR Apache-2.0 | `NativeActivity` glue that calls `android_main`; logcat |
| `jni`, `log`, Android only | 0.22, the version gpui-mobile uses; 0.4 | MIT OR Apache-2.0 | Asking the activity for the documents it received; logging a Java exception |
| `windows-registry`, Windows only | 0.100.0, of windows-rs; Rust 1.95. GPUI keeps 0.6.1, so the tree holds both | MIT OR Apache-2.0 | Registering the book formats for the current user; a safe API, so the crate keeps `unsafe_code` forbidden |

gpui-mobile is taken from a fork, pinned to a commit: its only release on
crates.io, 0.1.0, is built for gpui-pre 0.3.5, its `main` is on 0.3.10, and
GPUI Kit 0.11 pins gpui-pre 0.3.14. Cargo keeps one gpui-pre 0.3 in a build,
so the platform must be on the same one as the Kit. The fork starts from the
commit of [longbridge/gpui-mobile#56](https://github.com/longbridge/gpui-mobile/pull/56),
which moved it to 0.3.12, and moves it to 0.3.14: the pins, and the
`gpui-pre-wgpu` it vendors for its `offscreen` feature, which mbook does not
enable. Its default features bring about thirty platform modules (camera,
contacts, location, notifications…); they are off. Its README calls it an
experimental platform release: composition through the keyboard of the
system and the lifecycle of an activity are incomplete.

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
| `kael_share` 0.4.1 | Calls a named sharing service directly instead of showing the picker, and shares no file on Windows |
| `istmo-share` 0.1.1-alpha | An alpha whose API changes between releases; needs the window registry and runtime of the istmo framework |
| `gpui-mobile` 0.1.0 on crates.io (itsbalamurali/gpui-mobile) | Declares no dependency, not even on GPUI: it implements no platform for the GPUI of the Kit |
| `cargo-apk`, `xbuild` | `cargo-apk` is deprecated in favour of `xbuild` and packs no Java code, which the activities of gpui-mobile are; `xbuild` packs it through Gradle. `android.sh` compiles them with the tools of the SDK |
| `fluent-bundle` | A second system of translation beside the `rust-i18n` of GPUI Component, with a language of its own to keep in two places; its plurals are what `icu_plurals` gives |
| `gettext-rs` | Binds the C library gettext, which a build for Windows and Android must bring |
| `intl_pluralrules` 7.0.2 | No release since 2022-10-19; ICU4X, which follows the CLDR, is already in the tree |
| `cargo-ndk` | Sets the same four variables `android.sh` sets; a tool to install on every machine for no other gain |
| A Gradle project | Gradle and the Android Gradle plugin are downloaded on every clean build, for an APK of one activity without AndroidX |

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
- **Two engines.** Android runs on gpui-pre, the desktop on GPUI Fast. The
  code is the same and the tests run on GPUI Fast only; a difference of the
  engines shows on a phone first.
- **A platform from a pull request.** gpui-mobile is a pinned commit of an
  open pull request, an experimental release by its own account, mostly the
  work of one author. Its Java activities and emoji font are taken from its
  example application, whose layout may change.
- **Android limits** listed under [Android](android.md#limits): arm64 only, a
  theme that does not work for API 36, a black screen after the activity is
  recreated.
- **A content provider of this repository** lends shared files on Android:
  a small amount of Java to keep, because the FileProvider of AndroidX
  needs Gradle.

## Rationale and alternatives

| Decision | Chosen | Rejected | Why |
| --- | --- | --- | --- |
| Toolkit | GPUI Kit on GPUI Fast | | Decided by the format author. |
| Engine on Android | gpui-pre, with the platform of gpui-mobile | GPUI Fast with a platform of this repository | GPUI Fast has no Android platform; for iOS its platform crate takes the iOS crate of gpui-pre. gpui-mobile is the only Android platform for GPUI and is written for gpui-pre; writing one is most of gpui-mobile again. The application compiles and its tests pass on gpui-pre unchanged. |
| Source of gpui-mobile | A commit of the fork Mnwa/gpui-mobile on the gpui-pre of GPUI Kit 0.11 | The commit of longbridge/gpui-mobile#56 with GPUI Kit 0.10; waiting for upstream to reach gpui-pre 0.3.14 | The Kit and the platform must share one gpui-pre, and upstream has no commit on 0.3.14. The move needed no change to the code of gpui-mobile, only to the renderer it vendors for a feature mbook leaves off, so the fork is a small difference to give back upstream. |
| The Android entry point | A crate of its own, `mbook-android`, with the lints of the workspace but `unsafe_code` denied, and one `#[expect]` on `android_main` | `android_main` in `mbook-ui`; the host-driven entry point of gpui-mobile | `#[unsafe(no_mangle)]` is unsafe code, which `mbook-ui` forbids, and a forbidden lint cannot be expected. The host-driven entry point survives a recreated activity but needs JNI functions of this crate and an activity of its own. |
| Packing the APK | `android.sh`: `javac`, `d8`, `aapt2`, `zipalign` and `apksigner` of the SDK | Gradle; `cargo-apk`; `xbuild` | See Ruled out under [Dependencies](#dependencies). |
| System bars on Android | Opt out of the edge-to-edge layout of API 35, and pad views by the bars gpui-mobile measures | `targetSdkVersion` 34; JNI code of this crate that reads `WindowInsets` | gpui-mobile reads the bars from the content view, which edge-to-edge makes the whole screen. API 34 leaves the current target of Google Play behind; JNI code needs a second Java activity. |
| Export on Android | The converted file goes to the share sheet | `prompt_for_new_path`; the save picker of gpui-mobile | gpui-mobile answers `prompt_for_new_path` with no path, and its save picker returns a `content:` URI, which `std::fs` cannot write. |
| Clipboard on Android | The `clipboard` feature of gpui-mobile, with its `GpuiClipboard` packed into the APK; the edit menu of GPUI Kit | gpui-mobile without the feature; a clipboard class of this repository; the floating text toolbar of Android (`ActionMode`) | Without the feature gpui-mobile keeps copied text in a string of the process, so **Copy** reached no other app. The class of gpui-mobile is the twenty lines a class of this repository would be. The toolbar of Android belongs to a `View` that owns the selection; GPUI draws the text itself, so it would need the bounds of the selection and every command passed through JNI, for the same four commands the menu of the Kit already offers on every platform. |
| Share on Android | `MbookActivity.share` with `MbookFiles`, a read-only content provider of this repository, started on the thread of the activity without waiting | `robius-share`, as on the desktop; the share module of gpui-mobile; the FileProvider of AndroidX | `robius-share` showed no sheet on the phone it was tried on: it copied the file to `Download/Robius Share/`, left it there, and blocked the thread of the interface on a latch until the thread of the activity had run, which android-activity's handshakes can make wait for the interface. It also never clears a Java exception it causes. The share module of gpui-mobile shares text only, and the FileProvider of AndroidX needs Gradle. |
| A sideways swipe over a scrolling text | Steps to the next or the previous chapter; one swipe steps once, and a movement mostly up or down never does | Nothing; a tap at the edge as for a page | A phone reader expects a swipe to turn; the chapter is the page of a scrolling text. GPUI reports a pan as scrolling, so the same code serves a trackpad. |
| The footer of a scrolling text | Fixed over the text, which runs under it and fades into the paper behind the controls | Its room kept below the text; over the text only while the controls are shown; fully transparent | On a phone the kept room was a sixth of the screen with nothing in it, and a footer that hid left no way to the next chapter while scrolling. Fully transparent, its arrows and status were lost in the lines under them. The text view of the Kit cannot pad the end of its list, so the text is followed by an empty block as tall as the footer: scrolled to its end, the last line stops above the controls. A paged text keeps the room, since its pages are cut to the height above the footer. |
| Bringing the hidden controls back while scrolling | Scrolling back, and the end of a chapter | Only a tap or the pointer | A reader who wants the controls scrolls back, as in other readers; at the end of a chapter the next one is what a reader wants. |
| A tap or a swipe on something over the text | Belongs to what is on top: the reader checks the hitbox of its text at the point | The bounds of the text | A notification lies over the text; a tap on it turned the page and a swipe over it changed the chapter. |
| Dismissing an announcement on a touch screen | A tap, or a sideways swipe over it | Its close button; hiding it after a time | The close button of the Kit shows on hover only, which a touch screen has not. A time limit would fail WCAG 2.2.1 for a message the user did not cause. |
| Registering the formats on Windows | The application writes them to the registry of the current user at each start | Entries of the installer; nothing | The installer of cargo-bundle cannot hold file types, and writing the WiX source of the installer by hand is a second build to keep. The price is that uninstalling leaves the entries, which then point at a missing program. |
| Registering the formats other than mbook | **Open with** and an alternative handler, never the default | Making mbook the default for EPUB, PDF and the rest | The user chose a program for those; mbook converts them on the way in. |
| Documents handed to the Android activity | Copied into the cache by an activity of this repository, and taken by the interface once a second | A JNI function the activity calls; the deep links of gpui-mobile | A JNI function is a second unmangled entry point in `mbook-android`; the deep links of gpui-mobile go to its example activity and carry URLs, not readable files. |
| Where another app's document is received on Android | `OpenActivity`, without a window or native code, which starts `MbookActivity` in its own task | The intent filters on `MbookActivity` | An app that starts the receiver for a result, as Telegram does, gets a second `MbookActivity` in its own task, which cannot run the interface and closes; the document was imported, but mbook did not come up. |
| A destroyed activity on Android | Ends the process | Keeping the process, as Android does | GPUI cannot start again in a process on the thread of a new activity, and the window of that activity stays black. The price is a fresh start after the language of the system changes. |
| The sections of the editor in a compact window | A sheet from a menu button beside Back, as in a medium window | A screen of its own that Back returns to | Back that first opened the list and only then left the editor read as a bug; a menu button says what it opens. |
| Translations | `rust-i18n` with a YAML file per part of the application, keys named by part and purpose; counts by the plural rules of the CLDR | Fluent; gettext; the English text as key | GPUI Component translates its controls with `rust-i18n` in a language of the process; a second system would need the language set twice. A key by purpose keeps a reworded English text from breaking its translations. |
| The language at start | The first language of the system the interface has, else English | The first language of the system, else English; English always | A Russian speaker whose system lists German first still gets Russian; a language without a translation would be all fallback. |
| The language setting | System language, then every language named in itself; stored by its code, no row while it follows the system | The names in the current language | A person who cannot read the current language finds theirs. No row keeps a change of the system's language working, as for the page tone. |
| The language in tests | Never set: the tests run in English and a test checks the language `Appearance` chooses, not the text | Setting it per test | The language of `rust-i18n` belongs to the process, and the tests of a crate run on several threads at once. |
| File picker filters | Native dialogs through `rfd` | GPUI file prompts | GPUI Kit 0.11 cannot pass extension filters to its platform picker. |
| Sending a book to other people | The share sheet of the platform through `robius-share`, with the mbook file | Share links of social networks; code of this crate on AppKit and WinRT | A share link carries text and a web address, and a library book has no address. Calling the platform directly needs `unsafe`, which the workspace forbids; `robius-share` keeps it in a maintained crate. |
| Export destination | Native save dialog and temporary file in the destination directory | Convert directly into the chosen file | The completed file is published in one operation and an existing file cannot be overwritten. |
| Landmarks section | The start in a block of its own, then the other landmarks as a list | One row for every kind, filled or empty; one list with the start among the rest | The start is the one landmark a reader meets without asking and a book has one. Twenty-two rows would bury the few places a book has. |
| Choosing the kind of a landmark | One menu grouped by what a reader looks for, the places looked up while reading first, with a line that says what the choice gives | An alphabetical menu; the identifiers of the format | Authors choose by purpose, and the line answers why a place is worth marking. |
| Landmarks in the reader | Listed under Go to above the entries in the contents panel, in the order of the book | A control of their own in the header; mixed among the entries | The header already holds five controls; the order of the book is the order RFC 0001 asks an application to list them in, and mixed in they would read as chapters. |
| Where a new book opens | The `start` landmark, only while no place is stored | Always at `start`; at the first chapter as before | A reader who has turned back to the title page expects to come back to it. |
| Position of a new landmark | In the order of the book among the others | At the end of the list | RFC 0001 makes the stored order the order an application lists them in. |
| Where CPU-bound work runs | The global pool of `rayon` with half the threads of the system and a stack of 16 MiB, through `compute::spawn`, whose future the background executor awaits; files, the catalogue and the network stay on the executor | Everything on the background executor of GPUI, the choice until 2026-11-24; a pool with every thread | Decided by the format author on 2026-11-24. The converter and `rav1e` compute on the pool of `rayon` anyway, so the work of the application joins them there instead of competing with them from the threads of the executor; half the threads stay for the executor, drawing and the system, so a long import does not slow the interface. The stack is larger than the 2 MiB of a thread of the standard library because parsing nested Markdown, laying out a diagram and encoding a picture recurse; it is reserved, not used. The price: the pool has no priorities, cancelling reaches work only before it starts, and an import has half the cores (the EPUB of the record below: 4.3 s on four threads, 2.6 s on eight). |
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
| Drawing diagrams and formulas | Render source with the `figure` module of `mbook-converter` (merman and RaTeX), then give SVG to GPUI | Convert sources to AVIF; render with a web view | SVG lets the reader choose its own page colours and sizes. The source remains editable and rendering stays local. |
| Encoding the pictures of an import | `mbook-converter` with its `rayon` feature | Its default, one picture after another | Decided by the format author. An import of a 7 MB EPUB with 45 pictures took 2.6 s instead of 3.9 s on eight threads of an Apple M-series processor; the converter's own record has the details ([Rationale](../mbook-converter/design.md#rationale-and-alternatives)). |
| Assembly of the AVIF encoder | The NEON assembly of `rav1e` on ARM (`aarch64`), through the `nasm` feature of `image`; pure Rust on x86 | Pure Rust everywhere; the assembly on x86 as well | *Proposed*, not yet confirmed by the format author. Importing a 7 MB EPUB with 45 pictures took 10.5 s on four cores of an Apple M-series processor without it and 5.5 s with it; a phone took about a minute. On ARM the feature needs only the C compiler, which the Android build has for its C dependencies already; on x86 it needs NASM, which the release builds install for `rav1d` but a contributor may not have. The price is an assembler step in the ARM builds, beside the "no C toolchain" reason of the AVIF decoder. The library `mbook-converter` stays pure Rust. |
| Pictures added in the editor | Encoded here, with `image` | A public encoder in `mbook-converter` | Decided by the format author. The API of `mbook-converter` stays as it is; the price is that quality and speed are written in two crates. |
| Size class under a scaled interface | Width divided by the scale | Width in pixels | At 200% a desktop window has room for the compact layout only; clipping or a horizontal scroll bar fails WCAG 1.4.10. |
| Reordering chapters | A drag, and "Move up" and "Move down" | A drag alone | WCAG 2.5.7: a drag needs an alternative. |
| Thumbnails | JPEG files in the cache | Decoding every cover at start | The shelf then needs no AVIF decoder to appear. |
| Making a picture of the text open larger | A block plugin of the chapter view that draws a picture alone in its paragraph as the Kit does, and answers a click | Wrapping each picture in a link of the text; a hit test over the whole text | GPUI Kit 0.11 lets no click reach an image of its Markdown view. A link would change the text that offsets, search and copying use; a hit test would need the bounds of the images, which the Kit does not give. The price is that a picture in a line of text does not open. |
| Pictures in Visual | The block editor loads images through a resolver of the page, which finds pictures of the book only and decodes each at the width of the text as last laid out, with a transparent stand-in of that size | The placeholders of the block editor; its built-in loader; decoding at the width of the window | Placeholders showed no picture while the text around it was edited. The built-in loader takes a destination as an address or a file to fetch, while a book's are relative to the chapter. The width of the window is known on the first frame but wider than the text, and a second decode would follow once the text is laid out; waiting for that layout costs one 200 ms delay the first time Visual opens. Without the stand-in the editor holds a small square until the picture arrives, and the text below it moves. |
| Diagrams and formulas in Visual | The same resolver answers with the drawing of the figure, from the sources Preview asks for, as an SVG image declared at its size on the screen | A plugin of the block editor; a raster of this crate at the density of the screen; resolving the figures a second time for Visual | The block editor has no plugin for an image: it asks for an image source and sizes the image by it. GPUI draws an SVG image at twice the size it declares, so declaring the size on the screen is enough for a sharp drawing, where a raster would have to follow the density of each screen. Preview already asks for the figures of every draft; Visual takes the same answer. The cost is that a formula in a line is centred on the line instead of set on its baseline, and that the editor tells a display formula from the paragraph it is alone in, as the reader does. |
| The preview in the dialog of a diagram or a formula | The drawing beside the source in a wide window, fitted into the room left by the source and enlarged up to twice, with **Actual size** to see a large one at its size in the text and scroll it | A fixed dialog of 570 pixels with the drawing under the source, at most 180 pixels high; the full-window viewer of the reader | A large diagram fitted into 180 pixels could not be read, and the source scrolled in a field of six lines. Beside the source, the drawing gets the height of the window while both stay in sight. A dialog cannot open the viewer of the reader over itself. |
| What the viewer shows of a picture | The picture decoded again at its own size, up to 16,000,000 pixels | The bitmap fitted to the column, scaled up | The point of the viewer is to see more than the column shows. |
| Where the viewer lives | In the reader page | In `widgets` | Only the reader opens it; a part of a screen moves to `widgets` when a second page needs it. |
| How often a figure is drawn for the viewer | At densities that are powers of two | At every zoom; once at the largest zoom | Every step would rasterise and keep a new image; one large raster costs memory at every zoom and blurs when reduced far. |

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
| The Android package name | `io.github.m_compressors.mbook`: the identifier of the bundle, with `_`, since a package name has no `-` |
| The oldest Android | API 26, where Vulkan is part of the platform, as gpui-mobile chose |
| The architectures of the APK | arm64-v8a only |
| The keystore of the APK | A PKCS #12 keystore generated for the project, kept as the secret `ANDROID_KEYSTORE`; not yet generated |
| A pre-release on Android | Gets the `versionCode` of its release, so the release does not replace it; releases are tagged `vX.Y.Z` only |
| The formats registered | All of `BOOK_FILE_EXTENSIONS`, `.zip` and `.txt` among them; mbook owns `.mbook` only |

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
| Is there a GPUI backend for Android? | Not for GPUI Fast. gpui-mobile is one for gpui-pre, and the application runs on it: checked on 2026-11-23 on a phone with Android 16. |

Not answered: how the Markdown view behaves with one chapter of several
megabytes, and whether the system's setting for more contrast reaches the
application.

## Future possibilities

- **Two-page spreads**, which need two independently positioned columns.
- **Scrolling from one chapter into the next.**
- **A shell for iOS.** gpui-mobile has an iOS platform too; it needs an
  Xcode project and the scenes the iOS 27 SDK requires
  ([longbridge/gpui-mobile#44](https://github.com/longbridge/gpui-mobile/pull/44)).
- **The insets of the system from gpui-mobile**, which would lift the API 35
  limit of the theme, and **its host-driven entry point**, which survives a
  recreated activity.
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
