# API reference

The signatures below are the public surface of the crate as implemented,
grouped by type, with what each call reads and when it fails. The item-by-item
documentation, with an example for every call, is on
[docs.rs](https://docs.rs/mbook).

## Shared value types

```rust
pub struct ChapterId(/* private */);         // a resource of kind 1
pub struct PictureId(/* private */);         // a resource of kind 2
pub struct DiagramId(/* private */);         // a resource of kind 5
pub struct FormulaId(/* private */);         // a resource of kind 6
impl ChapterId { pub fn number(self) -> u32; }   // its number in the file
impl PictureId { pub fn number(self) -> u32; }
impl DiagramId { pub fn number(self) -> u32; }
impl FormulaId { pub fn number(self) -> u32; }

pub struct Size { pub width: u32, pub height: u32 }
pub struct Position { pub chapter: ChapterId, pub offset: u32 }
pub struct Version { pub major: u8, pub minor: u8 }

pub struct ChapterEntry<'a> {
    pub id: ChapterId,
    pub path: &'a str,
    pub decoded_len: u64,                    // from the container directory
}
pub struct PictureEntry<'a> {
    pub id: PictureId,
    pub path: &'a str,
    pub size: Size,
    pub decoded_len: u64,
}
pub struct DiagramEntry<'a> { pub id: DiagramId, pub path: &'a str, pub decoded_len: u64 }
pub struct FormulaEntry<'a> { pub id: FormulaId, pub path: &'a str, pub decoded_len: u64 }

pub struct TocEntry { pub level: u8, pub title: String, pub position: Position }
pub struct LandmarkEntry { pub kind: String, pub title: String, pub position: Position }
pub struct Anchor { pub offset: u32, pub name: String }
pub enum Destination {
    External,
    Chapter(Position),
    Picture(PictureId),
    Diagram(DiagramId),
    Formula(FormulaId),
    ExtraAnchor(ChapterId),                  // only from BookReader::resolve
}

// The raw resource table, for tools that list a file.
pub enum ResourceKind {                      // non-exhaustive
    Service, Text, Picture, ChapterAnchors, Landmarks, Diagram, Formula, Unknown(u8),
}
pub struct Resource<'a> {
    pub number: u32,
    pub kind: ResourceKind,
    pub path: &'a str,
    pub size: Option<Size>,
    pub decoded_len: u64,
}

/// The detection rule of RFC 0001; needs the first 53 bytes of a file.
pub fn is_mbook(prefix: &[u8]) -> bool;
```

- The four id types are `Copy`, ordered and hashable. They have no public
  constructor: an id is obtained from a `BookReader`, which hands out a
  `ChapterId` only for a text resource, a `PictureId` only for a picture, a
  `DiagramId` only for a diagram and a `FormulaId` only for a formula.
- An id is a number inside one file. It is not stable across re-packings of a
  book; the stable name of a resource is its path.

## `Meta`

One type serves both `BookReader` (through `&Meta`) and `Book` (through
`&mut Meta`).

```rust
impl Meta {
    pub fn title(&self) -> &str;
    pub fn language(&self) -> &str;                          // the main language
    pub fn languages(&self) -> impl Iterator<Item = &str>;
    pub fn id(&self) -> &str;
    pub fn authors(&self) -> impl Iterator<Item = &str>;
    pub fn translators(&self) -> impl Iterator<Item = &str>;
    pub fn series(&self) -> Option<Series<'_>>;              // name and index
    pub fn publisher(&self) -> Option<&str>;
    pub fn date(&self) -> Option<&str>;
    pub fn isbns(&self) -> impl Iterator<Item = &str>;
    pub fn genres(&self) -> impl Iterator<Item = &str>;
    pub fn annotation(&self) -> Option<&str>;
    pub fn cover(&self) -> Option<&str>;                     // a path

    pub fn get(&self, key: &str) -> Option<&str>;            // any key, first value
    pub fn get_all(&self, key: &str) -> impl Iterator<Item = &str>;
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)>;

    pub fn set_title(&mut self, title: impl Into<String>) -> Result<()>;
    pub fn set_language(&mut self, tag: impl Into<String>) -> Result<()>;
    pub fn set_id(&mut self, id: impl Into<String>) -> Result<()>;
    pub fn add_author(&mut self, name: impl Into<String>) -> Result<()>;
    pub fn set_series(&mut self, name: impl Into<String>, index: Option<&str>) -> Result<()>;
    pub fn set_cover(&mut self, path: impl Into<String>) -> Result<()>;
    // … one setter or adder per key defined by RFC 0001

    pub fn set(&mut self, key: &str, value: impl Into<String>) -> Result<()>;
    pub fn add(&mut self, key: &str, value: impl Into<String>) -> Result<()>;
    pub fn remove(&mut self, key: &str) -> Result<()>;
}
```

- `set` replaces every row of a key with one row; `add` appends a row.
- A `Meta` always satisfies the counts of RFC 0001: `remove` fails for
  `title`, `lang` and `id`, and `add` fails for a key that allows one value
  and already has it.
- Unknown keys, including `x-` keys, are kept and written back unchanged.

## `BookReader<R>`

```rust
impl BookReader<File> {
    pub fn open(path: impl AsRef<Path>) -> Result<Self>;
}
impl BookReader<Cursor<Vec<u8>>> {
    pub fn buffered(reader: impl Read) -> Result<Self>;
}
impl<R: Read + Seek> BookReader<R> {
    pub fn new(source: R) -> Result<Self>;
    pub fn with_options(source: R, options: &ReadOptions) -> Result<Self>;

    // From the tables; no I/O.
    pub fn version(&self) -> Version;
    pub fn meta(&self) -> &Meta;
    pub fn toc(&self) -> &[TocEntry];
    pub fn chapters(&self) -> impl ExactSizeIterator<Item = ChapterEntry<'_>>;  // reading order
    pub fn pictures(&self) -> impl ExactSizeIterator<Item = PictureEntry<'_>>;
    pub fn diagrams(&self) -> impl ExactSizeIterator<Item = DiagramEntry<'_>>;
    pub fn formulas(&self) -> impl ExactSizeIterator<Item = FormulaEntry<'_>>;
    pub fn chapter(&self, id: ChapterId) -> Option<ChapterEntry<'_>>;
    pub fn picture(&self, id: PictureId) -> Option<PictureEntry<'_>>;
    pub fn diagram(&self, id: DiagramId) -> Option<DiagramEntry<'_>>;
    pub fn formula(&self, id: FormulaId) -> Option<FormulaEntry<'_>>;
    pub fn find_chapter(&self, path: &str) -> Option<ChapterEntry<'_>>;
    pub fn find_picture(&self, path: &str) -> Option<PictureEntry<'_>>;
    pub fn find_diagram(&self, path: &str) -> Option<DiagramEntry<'_>>;
    pub fn find_formula(&self, path: &str) -> Option<FormulaEntry<'_>>;
    pub fn cover(&self) -> Option<PictureEntry<'_>>;
    pub fn chapter_id(&self, number: u32) -> Option<ChapterId>;     // checked
    pub fn picture_id(&self, number: u32) -> Option<PictureId>;     // checked
    pub fn diagram_id(&self, number: u32) -> Option<DiagramId>;     // checked
    pub fn formula_id(&self, number: u32) -> Option<FormulaId>;     // checked
    pub fn anchors(&self, chapter: ChapterId) -> &[Anchor];         // of headings
    pub fn resolve(&self, from: ChapterId, destination: &str)
        -> Result<Destination, ResolveError>;
    pub fn resources(&self) -> impl ExactSizeIterator<Item = Resource<'_>>;  // every row

    // Seek, read and decode one resource.
    pub fn read_chapter(&mut self, id: ChapterId) -> Result<String>;
    pub fn read_extra_anchors(&mut self, chapter: ChapterId) -> Result<Vec<Anchor>>;
    pub fn read_landmarks(&mut self) -> Result<Vec<LandmarkEntry>>;
    pub fn read_resolve(&mut self, from: ChapterId, destination: &str)
        -> Result<Destination>;
    pub fn read_picture(&mut self, id: PictureId) -> Result<Vec<u8>>;
    pub fn read_diagram(&mut self, id: DiagramId) -> Result<String>;    // Mermaid
    pub fn read_formula(&mut self, id: FormulaId) -> Result<String>;    // KaTeX
    pub fn read_cover(&mut self) -> Result<Option<Vec<u8>>>;

    pub fn load(&mut self) -> Result<Book>;      // decode everything
    pub fn into_inner(self) -> R;
}
```

| Call | Reads from the source | Fails when |
| --- | --- | --- |
| `open`, `new`, `buffered` | Footer, directory, resources 0 to 4 | The file must be rejected under [Conformance](../rfcs/0001-mbook-format.md#conformance), or a limit is exceeded. |
| Table accessors | Nothing | Never; unknown ids give `None` or an empty slice. |
| `resolve` | Nothing | The destination is unresolved under [Resolving destinations](../rfcs/0001-mbook-format.md#resolving-destinations), as far as resources 2 and 4 can tell. |
| `read_extra_anchors` | The anchor table of one chapter; nothing when it has none | I/O; a damaged frame; the table is not of the defined shape or breaks a rule of [Chapter anchor tables](../rfcs/0001-mbook-format.md#chapter-anchor-tables); the id came from another book. |
| `read_landmarks` | The landmarks table; nothing when the book has none | I/O; a damaged frame; the table is not of the defined shape or breaks a rule of [Landmarks](../rfcs/0001-mbook-format.md#landmarks-1). |
| `read_resolve` | That table, and only when the fragment is not the anchor of a heading | `Error::Unresolved` when the destination is unresolved; otherwise as `read_extra_anchors`. |
| `read_chapter`, `read_picture` | One record | I/O; a damaged frame; the resource exceeds its limit; the text is not UTF-8, or a stored position in it is not at the start of a line; the id came from another book. |
| `read_diagram`, `read_formula` | One record | I/O; a damaged frame; the resource exceeds its limit; the source is not UTF-8; the id came from another book. |
| `load` | Every text, picture, diagram, formula and anchor table record, and the landmarks | As above, for any resource; an extra anchor is not at the start of a line; or the decoded book exceeds the file limit. |

- An error in one resource does not poison the reader; other resources can
  still be read.
- `resolve` takes the destination after the Markdown parser has processed
  escapes and entities, and applies the steps of RFC 0001. With
  `read_resolve` it is all a reading application needs for links and
  pictures.
- **A fragment is resolved in two steps**, because RFC 0001 stores names in
  two places. `resolve` knows the anchors of headings. For any other
  fragment it looks at the resource table: when the target chapter has an
  anchor table it returns `Destination::ExtraAnchor`, and
  `ResolveError::UnknownAnchor` when it has none. `read_resolve` takes the
  second step and never returns `ExtraAnchor`.
- The anchor table of a chapter cannot show whether an offset is at the
  start of a line. `read_extra_anchors` and `read_resolve` check everything
  else; `load` and `Book::read`, which have the text as well, check this too.
- An anchor table that breaks a rule fails the calls that read it, and
  nothing else: its chapter is still read by `read_chapter`.
- **Landmarks are read on request**, by `read_landmarks`; opening a book
  does not decode them. A landmarks table that breaks a rule fails that call
  and `load`, and nothing else. A landmark inside a chapter needs an anchor
  at its offset: `read_landmarks` checks that against the anchors of
  headings and, when the chapter has an anchor table of its own, leaves the
  rest to `load`, which has that table.
- A resource of an unknown `kind` is listed by `resources` only. It has no id
  type, so it cannot be decoded, and `load` and `Book::read` leave it behind:
  a file of a later minor version loses such resources when it is edited.
- The tables cannot show whether a stored position is at the start of a line;
  that needs the text. `read_chapter` checks the positions of the chapter it
  decodes, and fails for that chapter only.
- **A diagram and a formula are read as text and not looked into.** The
  crate does not parse either notation, so `read_diagram` succeeds for a
  source no renderer accepts, and for an empty one, which RFC 0001 forbids
  writers to store: the application shows the alternative text.
- The types rule out reading a picture as a chapter. They cannot rule out an
  id taken from another book: such an id gives `None` from the table
  accessors and `Error::NoSuchResource` from the `read_` calls.
- A `BookReader` is `Send` when `R` is. To decode on several threads, open one
  reader per thread.

## `Book`

```rust
impl Book {
    pub fn new(title: impl Into<String>, language: impl Into<String>) -> Result<Self>;
    pub fn open(path: impl AsRef<Path>) -> Result<Self>;
    pub fn read(reader: impl Read) -> Result<Self>;
    pub fn read_with(reader: impl Read, options: &ReadOptions) -> Result<Self>;

    pub fn check(&self) -> Result<(), Problems>;
    pub fn write(&self, writer: impl Write) -> Result<()>;
    pub fn write_with(&self, writer: impl Write, options: &WriteOptions) -> Result<()>;
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()>;

    pub fn meta(&self) -> &Meta;
    pub fn meta_mut(&mut self) -> &mut Meta;

    pub fn chapters(&self) -> &[Chapter];                    // reading order
    pub fn chapter(&self, path: &str) -> Option<&Chapter>;
    pub fn chapter_mut(&mut self, path: &str) -> Option<&mut Chapter>;
    pub fn add_chapter(&mut self, path: impl Into<String>, text: impl Into<String>)
        -> Result<&mut Chapter>;
    pub fn insert_chapter(&mut self, index: usize, path: impl Into<String>,
        text: impl Into<String>) -> Result<&mut Chapter>;
    pub fn move_chapter(&mut self, path: &str, index: usize) -> Result<()>;
    pub fn remove_chapter(&mut self, path: &str) -> Option<Chapter>;

    pub fn pictures(&self) -> &[Picture];
    pub fn picture(&self, path: &str) -> Option<&Picture>;
    pub fn add_picture(&mut self, path: impl Into<String>, avif: impl Into<Vec<u8>>)
        -> Result<&Picture>;
    pub fn remove_picture(&mut self, path: &str) -> Option<Picture>;
    pub fn set_cover(&mut self, path: impl Into<String>, avif: impl Into<Vec<u8>>)
        -> Result<&Picture>;                                 // add_picture + meta cover

    pub fn diagrams(&self) -> &[Diagram];
    pub fn diagram(&self, path: &str) -> Option<&Diagram>;
    pub fn diagram_mut(&mut self, path: &str) -> Option<&mut Diagram>;
    pub fn add_diagram(&mut self, path: impl Into<String>, source: impl Into<String>)
        -> Result<&mut Diagram>;
    pub fn remove_diagram(&mut self, path: &str) -> Option<Diagram>;

    pub fn formulas(&self) -> &[Formula];
    pub fn formula(&self, path: &str) -> Option<&Formula>;
    pub fn formula_mut(&mut self, path: &str) -> Option<&mut Formula>;
    pub fn add_formula(&mut self, path: impl Into<String>, source: impl Into<String>)
        -> Result<&mut Formula>;
    pub fn remove_formula(&mut self, path: &str) -> Option<Formula>;

    pub fn toc(&self) -> &Toc;
    pub fn set_toc(&mut self, toc: Toc);

    pub fn landmarks(&self) -> &[Landmark];
    pub fn set_landmarks(&mut self, landmarks: Vec<Landmark>) -> Result<()>;
}

impl Chapter {
    pub fn path(&self) -> &str;
    pub fn text(&self) -> &str;
    pub fn anchors(&self) -> &[Anchor];                  // of headings
    pub fn extra_anchors(&self) -> &[Anchor];            // of lines it names
    pub fn set_text(&mut self, text: impl Into<String>) -> Result<()>;
}

impl Picture {
    pub fn path(&self) -> &str;
    pub fn size(&self) -> Size;
    pub fn data(&self) -> &[u8];
}

impl Diagram {                                           // Formula is the same
    pub fn path(&self) -> &str;
    pub fn source(&self) -> &str;
    pub fn set_source(&mut self, source: impl Into<String>) -> Result<()>;
}
```

`Book::new` creates the three required `meta` rows and fails only for an
empty title. `id` is `urn:uuid:` followed by a UUID version 7 generated by the
`uuid` crate. A converter that has a stable identifier replaces it with
`meta_mut().set_id`.

`add_chapter`, `insert_chapter` and `set_text` normalise the text before
storing it: CRLF and lone CR become a line feed, and a leading byte order mark
is removed. Nothing else is changed.

`add_diagram`, `add_formula` and `set_source` normalise a source the same
way, and refuse one without a character that is not white space. A diagram
and a formula can be changed in place, unlike a picture: their source is
text an editor works on, and removing and adding one again would move it to
the end of its list.

### Navigation

```rust
pub enum Toc {
    FromHeadings { max_level: u8 },     // Book::new uses 6: every heading
    Explicit(Vec<TocItem>),             // what a loaded book has
}
pub struct TocItem {
    pub level: u8,
    pub title: String,
    pub chapter: String,                // a path
    pub anchor: Option<String>,         // None is the start of the chapter
}

impl TocItem {
    /// `href` is a link destination: `ch01.md` or `ch01.md#the-station`.
    pub fn new(level: u8, title: impl Into<String>, href: &str) -> Self;
}
```

A `Book` never stores a byte offset that the caller has to keep correct:

- **Anchors belong to their chapter.** `add_chapter` and `set_text` compute
  them from the headings with the naming rule of RFC 0001. The rule is
  defined by GitHub's implementation, so the names come from a port of it,
  the `github-slugger` crate. A heading whose name would be empty, such as
  `# ***`, gets no anchor, because the format does not allow an empty name. A
  chapter loaded from a file keeps the anchors stored in that file until its
  text is replaced, so links written against another writer's names keep
  working.
- **Contents entries name their target** by chapter path and anchor, exactly
  as a link does. The offset is looked up when the book is written.
- **Every stored contents row can be named this way**, because RFC 0001
  requires a contents row with a non-zero offset to match an anchor row
  ([Resource 3: `toc`](../rfcs/0001-mbook-format.md#resource-3-toc)). A loaded row
  becomes the chapter's path and the name of the first anchor at that
  position.

### Landmarks

```rust
pub struct Landmark {
    pub kind: String,                   // `start`, `glossary`, `index`, …
    pub title: String,                  // may be empty
    pub chapter: String,                // a path
    pub anchor: Option<String>,         // None is the start of the chapter
}

impl Landmark {
    /// `href` is a link destination: `back/index.md` or `ch01.md#fig-1`.
    pub fn new(kind: impl Into<String>, title: impl Into<String>, href: &str) -> Self;
}
```

The [landmarks](../rfcs/0001-mbook-format.md#landmarks-1) of a book are a list, set
as a whole with `set_landmarks`, like the contents.

- **A landmark names its target the way a link does**, by chapter path and
  anchor; its offset is looked up when the book is written. Unlike a
  contents item it may lead to an extra anchor as well as to a heading,
  because RFC 0001 lets a landmark match a row of either table.
- `set_landmarks` checks what concerns the list alone: the form of every
  kind, and that at most one landmark is the `start`. Whether a landmark
  leads somewhere connects it with the chapters and is checked by `check`
  and `write`, as `DanglingLandmark`.
- An empty list removes the landmarks; a book without them is written
  without the table.
- A loaded landmark becomes the chapter's path and the name of the first
  anchor at its position, of a heading before an extra one.

### Extra anchors

A heading names itself. Any other line is named by an
[anchor definition](../rfcs/0001-mbook-format.md#anchor-definitions) in the text of
its chapter: `[fig-1]: #fig-1`. The extra anchors of a chapter are written as
its [anchor table](../rfcs/0001-mbook-format.md#chapter-anchor-tables), and a chapter
without them gets no table.

- **Extra anchors are computed from the text**, by the same parse that finds
  the headings. `add_chapter`, `insert_chapter` and `set_text` compute them;
  no call takes a name or an offset. An edit therefore keeps every name
  whose definition it keeps, and removes a name by removing its line.
- A definition that repeats the name of a heading of the chapter, or of an
  earlier anchor definition, adds no anchor, as RFC 0001 says.
- The definition stays in the text: `chapter.text()` returns it, and a
  reading application gets it from `read_chapter` and renders nothing for
  it.
- **A contents item leads to a chapter or to a heading**, not to an extra
  anchor: RFC 0001 wants a contents row inside a chapter to match a row of
  `anchors`. An item that names an extra anchor is reported as
  `DanglingTocItem`.
- A loaded chapter keeps the extra anchors stored in its file, under the same
  rule as its heading anchors: until its text is replaced. A file from
  another writer may name a line without an anchor definition; that name is
  lost when the text is replaced.

### Where each rule is checked

| Rule of RFC 0001 | Checked by | Reported as |
| --- | --- | --- |
| Path syntax, path uniqueness | `add_chapter`, `insert_chapter`, `add_picture`, `set_cover` | `Error::InvalidPath`, `Error::DuplicatePath` |
| A position in the reading order exists | `insert_chapter`, `move_chapter` | `Error::IndexOutOfRange` |
| The chapter to move exists | `move_chapter` | `Error::NoSuchPath` |
| Text encoding: no control characters other than line feed and tab | `add_chapter`, `insert_chapter`, `set_text`, after normalising | `Error::InvalidText`, with the byte offset |
| The same rule, for a chapter that was loaded and never replaced | `check`, `write` | `Problems` |
| The picture is an AVIF file with a primary item | `add_picture` | `Error::InvalidPicture` |
| The source of a diagram or a formula: the encoding of a chapter, and not empty | `add_diagram`, `add_formula`, `set_source`, after normalising | `Error::InvalidDiagram`, `Error::InvalidFormula`, with a `SourceError` as its source |
| The same rules, for a diagram or a formula that was loaded and never replaced | `check`, `write` | `Problems`, with the path of the diagram or the formula |
| `meta` key syntax and counts | `Meta` setters | `Error::InvalidMeta` |
| The form of a landmark's kind; one `start` at most | `set_landmarks` | `Error::InvalidLandmark`, with a `LandmarkError` as its source |
| Every landmark leads to a chapter or to an anchor of it | `check`, `write` | `Problems` |
| No raw HTML, allowed URI schemes | `check`, `write` | `Problems` |
| Every link and picture reference resolves, a fragment against the anchors of headings and the extra anchors of its chapter; a picture reference names a picture, a diagram or a formula | `check`, `write` | `Problems` |
| `cover` names a picture | `check`, `write` | `Problems` |
| Contents entries resolve; levels form a tree | `check`, `write` | `Problems` |

- `write` runs `check` first and writes nothing if it fails.
- Loading is lenient where RFC 0001 tells readers to be: a chapter with a
  broken link loads. The problem is reported when that book is written,
  because a writer **MUST NOT** produce such a file.
- Line endings and a byte order mark are the only things the crate repairs.
  Raw HTML and control characters are the converter's job; the error or the
  problem says where the offence is.

### What `write` produces

Resources 0 to 4, then the landmarks when the book has any, then the cover,
then chapters in reading order, each directly after its anchor table when it
has extra anchors, then the remaining pictures, the diagrams and the
formulas, as RFC 0001 recommends. Pictures use the *store* graph; diagrams
and formulas are compressed like chapters.
The same `Book` written twice with the same options gives identical bytes.

## Options and limits

```rust
#[non_exhaustive]
pub struct Limits {
    pub max_resources: usize,
    pub max_resource_bytes: u64,    // decoded size of one resource
    pub max_file_bytes: u64,
}
#[non_exhaustive] pub struct ReadOptions  { pub limits: Limits }
#[non_exhaustive] pub struct WriteOptions { pub level: Option<i32>, pub limits: Limits }
```

`Limits` are applied before allocating, as RFC 0001 requires of readers. They
replace the mopenzl defaults, which are too small for picture-heavy books.

| Limit | `Limits::default()` | Reason |
| --- | ---: | --- |
| `max_resources` | 1,000,000 | Ten times the largest book measured in RFC 0001; the directory is then 24 MB. |
| `max_resource_bytes` | 64 MiB | The default frame limit of mopenzl. |
| `max_file_bytes` | 16 GiB | Room for a comic of several thousand large pages. |

- A lazy reader decodes one resource at a time, so `max_resource_bytes` is
  all that bounds its memory. A resource above the limit fails when it is
  read, not when the file is opened.
- A `Book` holds everything decoded. `max_file_bytes` therefore also bounds
  the decoded size of a book that `Book::read` or `BookReader::load` builds.
- `WriteOptions::level` outside the range of the compressor is brought into
  that range.

## Errors

One `Error` enum, non-exhaustive, with `Result<T> = Result<T, Error>`.

| Variant | Meaning |
| --- | --- |
| `Io` | The source or the sink failed. |
| `NotMbook` | No valid format header. |
| `UnsupportedVersion(Version)` | `major` is newer than this crate. |
| `Container` | The container or a frame is damaged. The mopenzl error is the `source()`, as a boxed `dyn Error`. |
| `Malformed { resource, reason }` | A table breaks a **MUST** of RFC 0001. |
| `Limit` | A configured limit was exceeded. |
| `NoSuchResource` | A `read_` call was given an id from another book. |
| `InvalidDiagram`, `InvalidFormula` | The source given for a diagram or a formula is empty or holds a control character. The `SourceError` is the `source()`. |
| `Unresolved` | `read_resolve` was given a destination that leads nowhere. The `ResolveError` is the `source()`. |
| `InvalidPath`, `DuplicatePath`, `InvalidText`, `InvalidPicture`, `InvalidMeta`, `InvalidLandmark` | An argument to a `Book` or `Meta` method broke a rule. |
| `NoSuchPath`, `IndexOutOfRange` | `move_chapter` or `insert_chapter` named a chapter or a position that does not exist. |
| `Invalid(Problems)` | `write` or `save` refused the book. |
