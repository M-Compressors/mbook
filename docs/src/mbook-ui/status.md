# Status and verification

What the first version does, where it differs from the other pages of this
section, and how it was checked.

## What is implemented

The first version implements what the other pages of this section describe,
with the differences below.
The headless tests cover layout and interaction. The redesigned screens were
also exercised on macOS with an imported illustrated book. Screen-reader
use and native builds on Windows and Linux still need verification.

Seen in the pictures and left as it is: a notification stays over the right
end of the header for a while. A long word on a cover drawn from a title
can break in its middle.

| Part | State |
| --- | --- |
| Library | **New book…** creates a titled book and opens its first chapter in the editor. Cover tiles show a new badge, reading percentage or completion status below the cover. The card of the book being read shows how far it was read, not the name of its chapter: the catalogue stores the path of a chapter, not its title. A change of the catalogue that failed is followed by a reload, and a reload that succeeds clears the message, so that failure may pass unseen. |
| Export | The book menu exports the saved book in all eight converter output formats through a native save dialog. |
| Share | The book menu shows the share sheet of the platform with a copy of the mbook file named after the book. The headless tests check the copy, its name and the title it is given, and the announcement of a failure. The sheet was opened on macOS from the menu and offered AirDrop, Messages, Notes and Freeform for the named file; it has not been tried on Windows. On macOS it appears beside the window, not at the menu: `robius-share` takes no anchor. |
| Import | "Add a book…" and the Open key show a picker filtered to supported book extensions. Files can also be dropped on the window. Opening a file with the application from the system works through the formats it registers ([Opening files from the system](architecture.md#opening-files-from-the-system)). The tests cover the URLs and paths, the queue until the library is open, the registry entries of Windows and the formats the plist and the manifest declare; opening a file from Finder, Explorer or an Android file manager has not been tried, and the Windows code is compiled by continuous integration only. |
| Reader | Scrolling, optional pagination, whole-book search with return to the original passage, auto-hiding controls and optional progress. A book read for the first time opens at its `start` landmark, and the contents panel lists the landmarks under Go to. Pagination supplies whole-book page estimates and scales pictures to the page. Pictures and diagrams scale to fit a page. Mermaid diagrams and KaTeX formulas use the page tone and display alternative text when rendering fails. A picture, diagram or formula alone in its paragraph opens over the window with zoom; one in a line of text, a linked one and a reference-style picture do not. Other tall blocks can be clipped through a line. |
| Editor | Chapter search works on the current draft in Visual, Source, Split and Preview. Editing opens the first chapter in Visual. Chapters are listed and chosen by number and title. Visual supports tables, nested and task lists, quotes and code blocks, and draws the pictures, diagrams and formulas of the chapter in the text. A link is made from selected or newly entered words and can lead to a chapter or a heading chosen by title. A picture is chosen from the thumbnails of the book or taken from a file, with a description, and put at the caret. The Contents section shows the contents and where they come from. The entries of a stored list are edited, added, nested and removed one by one, are kept in the order of the book, and are marked when they lead nowhere or are out of order. They follow their chapter when it moves or its file is renamed, and are removed with it. The Landmarks section marks where the text begins in a block of its own and adds, edits and removes landmarks of every kind of RFC 0001, chosen from a grouped list that says what each gives a reader. The picture gallery shows thumbnails and picture import offers encoding settings. Diagrams and Formulas have syntax-highlighted source dialogs that take most of the window, a preview beside the source in a wide window that fits the drawing or shows it at its size, and removal controls. Visual inserts figure references, and Preview and Split render them. Footnotes and reference definitions are edited in Source. Source and Split highlight Markdown syntax. The platform Save shortcut works while editing. A picture cannot be put into the text from Source, and choosing a contents entry opens its chapter at the top. Dropping a file on a chapter has not been tried outside of tests, which call the handler directly. |
| Settings | As designed. The size of the interface is applied when the slider is released, because it moves the slider. |
| Languages | English and Russian, chosen from the system at start or in the settings. The tests check the choice of the language, the plural forms and that every key has both texts, and run in English. The Russian screens were looked at in the pictures of the visual test at every width: every text is Russian but the names of formats and typefaces, the text of the books and an error message a scene makes up. A change of language leaves the messages already shown, the placeholders of a dialog that is open and the footnote labels of an open chapter in the previous language. An error quoted from `mbook` or `mbook-converter` stays in English. |
| Updates | The banner sits below the window controls, and update notifications appear at the bottom. The check and the installation are written and tested against scripted releases and signed archives made by the tests. The installer is compiled on macOS and Windows. |
| Release workflow | The workflow produced installers and update archives for v0.1.0. The Trusted Publishing step awaits its first run. The macOS and Windows x64 jobs make NASM available before building. The Android job has not run: it needs the secrets `ANDROID_KEYSTORE` and `ANDROID_KEYSTORE_PASSWORD`, which are not set. |
| Android | `android.sh` built a signed APK on macOS with NDK 30, build-tools 35 and OpenJDK 21. It was installed with `adb` on one phone with Android 16 (arm64, Mali-G615): it starts in about 0.3 s on Vulkan, shows the library in the compact layout with the bar of the page under the status bar, and loads the bundled emoji font. Tried by hand there, since the phone refuses input from `adb`: adding a book through the document picker of the system works, and so do the bar of a page under the status bar, notifications that fit the screen, the reader's swipe between chapters, and the footer of a scrolling text, with the text fading into the page behind it. Sharing through `robius-share` showed no sheet; through the activity of `mbook-android` the share sheet of the system opened. Opening a book from another app was tried with a `VIEW` intent for an mbook file in Downloads: the book was added and opened. Export on Android has not been tried. Copying to the clipboard of the system and pasting from it, through the `clipboard` feature of gpui-mobile, has not been tried on the phone: before it, **Copy** kept the text inside the application. The navigation bar of gesture navigation is reported as 0 px there, so its handle is drawn over the bottom bar. No iOS build exists. |
| Typeface | Literata and Noto Sans are bundled for offline rendering. System and installed family choices use the platform’s font catalogue. |
| Transitions | Reading controls fade if motion is enabled. Page turns are immediate by default and can use a slide when enabled. |

Where assistive technology gets less than [Accessibility](appearance.md#accessibility)
asks:

- **The structure of a chapter is not reported.** The Markdown view of GPUI
  Kit gives its headings, lists and tables no roles. The text of a chapter
  is one element with the article role, named by the title of the chapter.
- **The language of a book is not reported**: GPUI has no property for it.
- **The link in the About dialog cannot be reached by keyboard**: the link
  of GPUI Kit takes no focus.
- **A picture cannot be opened larger from the keyboard**: the pictures of a
  chapter take no focus, as its links do not. Once open, the viewer is used
  with keys alone.
- A few parts of Kit controls are reported without a role: the frame of a
  text field, the search field inside a select, the track of a slider. The
  controls they belong to have roles and names.
- The Kit's minimize, maximize, restore and close buttons lack roles in
  the headless tree. On Windows, GPUI handles their hit testing in the
  window's non-client area.

## Testing

- Logic sits in models and in plain functions: the size class, the import
  state machine, the catalogue, progress, block offsets, the outline. Each
  has unit tests beside it.
- Models and views are tested in headless windows with the `test-support`
  feature of `gpui-kit`: a test imports a book built in a temporary root,
  opens it, edits it and saves it.
- The rule of [Threads](architecture.md#threads) is tested: with the background executor
  held back, a view must still draw, showing its loading state.
- In the unit tests `compute::spawn` does its work when its future is first
  polled, on the thread that polls it, so the test executor of GPUI orders
  all work as it did before the pool. The pool itself is tested in
  `shared::compute`. The visual test runs the real pool, and waits until it
  has ended the work of a step before it draws.
- Every layout is drawn at 390, 800 and 1280 px of width. With the Cargo
  feature `visual`, `cargo test -p mbook-ui --features visual --test visual`
  renders every screen without a window, in these widths and in a light
  and a dark page tone, into `target/visual/`: 160 pictures for a person to
  look at. With `MBOOK_VISUAL_LANGUAGE=ru` it draws them in Russian, into
  `target/visual/ru/`; the scenes reach their controls by the keys of their
  names, so every one is drawn in either language. The renderer exists on
  macOS; elsewhere the test says so and passes.
- Accessibility is tested, not reviewed:
  - a unit test computes the contrast of every pair of tokens that
    [Accessibility](appearance.md#accessibility) puts together;
  - for each screen a test reads the accessibility tree of a headless
    window and checks that every application control has a role and a
    name. The window buttons drawn by GPUI Kit are outside this check;
  - for each screen a test completes its main task with keys alone: add a
    book, open it, move to another chapter, edit and save.

## Reading verification

| Bundled font | Version | Source | License and shipped instances |
| --- | --- | --- | --- |
| Literata | 3.103 | [Google Fonts, Literata](https://github.com/google/fonts/tree/06bfe0d62a98375cb09c942fa18e68bd32cff057/ofl/literata) | SIL OFL 1.1, `Literata-OFL.txt`. TrueType, 12-point optical cut, upright and italic at 400/500/700. |
| Noto Sans | 2.015 | [Google Fonts, Noto Sans](https://github.com/google/fonts/tree/06bfe0d62a98375cb09c942fa18e68bd32cff057/ofl/notosans) | SIL OFL 1.1, `NotoSans-OFL.txt`. TrueType, standard width, upright and italic at 400/500/700. |

The original copyright and license metadata remains in the embedded files.
The static instances were generated with FontTools and use consistent
family names for native weight matching. The assets and notices are under
`crates/mbook-ui/assets/fonts`. Each face’s character map was checked for
the mixed-script sample, including combining acute accents and the ruble
sign.

| Tone | Body | Muted | Link | Selected body | Selected link | Search body | Search link | Focus accent on page |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Paper | 13.20 | 5.34 | 8.09 | 10.29 | 6.30 | 12.10 | 7.41 | 5.62 |
| Neutral | 14.48 | 5.84 | 7.86 | 11.04 | 5.99 | 12.62 | 6.85 | 5.91 |
| Sepia | 10.36 | 5.27 | 8.08 | 8.13 | 6.34 | 10.70 | 8.35 | 4.99 |
| Night | 11.33 | 6.86 | 7.77 | 7.48 | 5.13 | 8.66 | 5.94 | 6.01 |
| High contrast | 21.00 | 8.06 | 16.01 | 19.56 | 14.91 | 15.39 | 11.73 | 6.18 |

Ratios use the [WCAG 2.2](https://www.w3.org/TR/WCAG22/) relative
luminance calculation. Tests compare unrounded values. The first three
columns use the page background. Chrome tests also cover on-accent
labels, control outlines and focus against both chrome surfaces.

Run `cargo test -p mbook-ui --features visual --test visual --locked` to
capture `05c-reading-mode-*` and `05d-reading-mode-enlarged-*` in
`target/visual`. These open a Russian mbook fixture in the real reader.
The latter uses 44 logical px. The captures use a macOS headless renderer
on an Apple M5 Pro running macOS 26.6.2 at 2× density, with desktop and
phone-width viewports.
The first five full Russian prose lines inspected in the 1280 px Literata
capture contain 55–64 characters, including internal spaces. In the
390 px capture, the six full lines of the first paragraph contain
24–32 characters. The layout tests cover widths from 320 through 1920 logical px. The native font probe
checks distinct face IDs and the required glyphs for regular, medium, bold
and their italics. The visual command also writes
`target/visual/reading-latency.csv` with five first-frame samples each for
theme changes, 1 px text increases and page turns at 1280 × 780 logical px.
It measures dispatch and headless frame rendering on the CPU, rather than
display presentation. Five warm samples in a debug build gave median /
maximum times of 17.72 / 22.25 ms for themes, 17.80 / 21.34 ms for text
size, and 10.01 / 10.36 ms for page turns. The sample was the bundled
Russian fixture at 44 px, with page animation disabled. The timings do not
cover real-display latency or a cold open. The regression suite separately
exercises chapters of roughly 55,000 and 200,000 characters and retains a
test for switching layouts inside a long paragraph.

**Remaining checks and limitations**

- There are currently desktop shells only. The phone captures exercise the
  compact layout on macOS. Physical phone behavior, Dynamic Type, Android
  font scaling, Windows at 100% and 125%, and Linux are untested. Resize
  the native window through 320, 360, 390, 430, 768, 1280 and 1920 logical
  px, then repeat on each desktop backend at its system scale.
- VoiceOver and NVDA are untested. Open a book and use Tab, Shift+Tab, Escape,
  Home, End and Page Up/Down to read and operate Contents, Aa and Find.
  Check that focus remains visible, returns to its trigger and that the
  page is readable. The Markdown toolkit currently omits heading, list and
  book-language semantics. Native high-contrast preference mapping beyond
  the light/dark system preference is unverified. Browser zoom and CSS
  text-spacing overrides do not apply to the native app.
- Book-style first-line indent, language-aware hyphenation, and custom
  letter and word spacing are unimplemented. Footnote definitions render
  inline in the chapter. Bookmarks and user annotations are unimplemented.
- The chapter and source block survive reflow where the toolkit’s block
  map agrees with the book. The offset within a block is approximate, so
  text far into a very long paragraph and mixed-height blocks can move by
  more than one rendered line. In paged mode, oversized lists and tables
  can still split through a line. Test by switching layouts halfway through
  a long paragraph and comparing the first visible phrase before and after.
- The font assets are local. Test a missing family by selecting one on
  another system, removing it, and reopening the same catalogue offline.
  Slow network, platform font-registration failure and cold-start flash
  were not verified on physical displays.

**User-validation pilot**

Recruit 8–12 readers across ages, routine text-enlargement preferences
and desktop and phone use. When a phone build is available, compare Paper /
Literata with Neutral / Noto Sans in separate 20–30 minute sessions, with a
break. If Night is included, use a third session. Counterbalance order and
assign comparable unfamiliar Russian passages of similar length and
difficulty. Record device, scale, actual size, lighting, order and
baseline discomfort. Collect discomfort on a 0–10 scale before and after,
comfort and willingness to continue on a 1–7 scale after, several
comprehension questions, lost-line incidents, setting changes and open
comments. Treat reading speed as secondary. Use observations to refine
defaults while retaining individual settings.
