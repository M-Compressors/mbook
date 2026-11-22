//! A whole book in memory.

#[cfg(feature = "write")]
use crate::error::{Error, LandmarkError, Result, SourceError};
use crate::meta::Meta;
use crate::types::{Anchor, Size};
#[cfg(feature = "write")]
use crate::{analysis, avif, path, text};
use std::collections::HashMap;

#[cfg(feature = "read")]
mod load;
#[cfg(feature = "write")]
mod write;

/// A whole book in memory: metadata, chapters in reading order, pictures,
/// diagrams, formulas and the table of contents.
///
/// A `Book` is how a book is created and how an existing one is changed.
/// Chapters, pictures, diagrams and formulas are addressed by path. Nothing in it is a byte
/// offset the caller has to keep correct: anchors are computed from the text
/// of each chapter, and contents items name their target the way a link
/// does.
///
/// # Examples
///
/// Creating a book:
///
/// ```no_run
/// # #[cfg(feature = "write")]
/// # fn main() -> mbook::Result<()> {
/// use mbook::Book;
/// use std::fs;
///
/// let mut book = Book::new("The Example", "en")?;
/// book.meta_mut().add_author("A. Writer")?;
/// book.set_cover("img/cover.avif", fs::read("img/cover.avif")?)?;
/// book.add_chapter("ch01.md", fs::read_to_string("ch01.md")?)?;
/// book.add_picture("img/fig-1.avif", fs::read("img/fig-1.avif")?)?;
/// book.save("example.mbook")?;
/// # Ok(())
/// # }
/// # #[cfg(not(feature = "write"))]
/// # fn main() {}
/// ```
///
/// Changing an existing one:
///
/// ```no_run
/// # #[cfg(all(feature = "read", feature = "write"))]
/// # fn main() -> mbook::Result<()> {
/// use mbook::Book;
///
/// let mut book = Book::open("example.mbook")?;
/// book.meta_mut().set_title("The Example, 2nd edition")?;
/// if let Some(chapter) = book.chapter_mut("ch02.md") {
///     let fixed = chapter.text().replace("teh", "the");
///     chapter.set_text(fixed)?;
/// }
/// book.remove_picture("img/unused.avif");
/// book.save("example.mbook")?;
/// # Ok(())
/// # }
/// # #[cfg(not(all(feature = "read", feature = "write")))]
/// # fn main() {}
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Book {
    meta: Meta,
    chapters: Vec<Chapter>,
    pictures: Vec<Picture>,
    diagrams: Vec<Diagram>,
    formulas: Vec<Formula>,
    toc: Toc,
    /// The notable places of the book, in the order they are listed.
    landmarks: Vec<Landmark>,
    /// Where each path lives; paths are unique across chapters, pictures,
    /// diagrams and formulas.
    slots: HashMap<String, Slot>,
}

/// The place of a path in a `Book`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Chapter(usize),
    Picture(usize),
    Diagram(usize),
    Formula(usize),
}

/// One chapter of a [`Book`]: a path, Markdown text, the anchors of its
/// headings and the extra anchors of the lines it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chapter {
    path: String,
    text: String,
    /// The anchors of the headings, sorted by offset.
    anchors: Vec<Anchor>,
    /// The anchors of the chapter's own table, sorted by offset.
    extra_anchors: Vec<Anchor>,
}

/// One picture of a [`Book`]: a path and an AVIF file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    path: String,
    size: Size,
    data: Vec<u8>,
}

/// One diagram of a [`Book`]: a path and a description in Mermaid notation.
///
/// A chapter shows it the way it shows a picture:
/// `![The flow of a request](dia/flow.mmd)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagram {
    path: String,
    source: String,
}

/// One formula of a [`Book`]: a path and a formula in the notation of
/// `KaTeX`, without the `$` that surround one in running text.
///
/// A chapter shows it the way it shows a picture: `![E = mc^2](math/e.tex)`.
/// Alone in a paragraph it is set as a display formula, and inside a line as
/// a part of that line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formula {
    path: String,
    source: String,
}

/// The table of contents of a [`Book`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Toc {
    /// Built from the headings of every chapter when the book is written.
    FromHeadings {
        /// The deepest heading level to list; 6 lists every heading.
        max_level: u8,
    },
    /// Written as given. Every item must lead somewhere when the book is
    /// written.
    Explicit(Vec<TocItem>),
}

impl Default for Toc {
    /// Every heading of every chapter.
    fn default() -> Self {
        Self::FromHeadings { max_level: 6 }
    }
}

/// One item of an explicit table of contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TocItem {
    /// Depth in the tree, starting at 1.
    pub level: u8,
    /// The title, as plain text.
    pub title: String,
    /// The path of the chapter the item leads to.
    pub chapter: String,
    /// The anchor inside that chapter; `None` is the start of the chapter.
    pub anchor: Option<String>,
}

impl TocItem {
    /// An item that leads where a link with this destination would.
    ///
    /// `href` is a chapter path with an optional anchor after `#`. It is
    /// taken as written: nothing is percent-decoded.
    ///
    /// # Examples
    ///
    /// ```
    /// use mbook::TocItem;
    ///
    /// let item = TocItem::new(2, "The station", "ch01.md#the-station");
    /// assert_eq!(item.chapter, "ch01.md");
    /// assert_eq!(item.anchor.as_deref(), Some("the-station"));
    /// ```
    #[must_use]
    pub fn new(level: u8, title: impl Into<String>, href: &str) -> Self {
        let (chapter, anchor) = split_href(href);
        Self {
            level,
            title: title.into(),
            chapter,
            anchor,
        }
    }
}

/// One landmark of a [`Book`]: a notable place, such as the start of the
/// text, the glossary or the index.
///
/// A landmark names its target the way a link does: by the path of a chapter
/// and, for a place inside it, by an anchor. The kinds the format defines
/// are listed in RFC 0001; a kind that begins with `x-` is private.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Landmark {
    /// What the place is: `start`, `glossary`, `index`, …
    pub kind: String,
    /// The title to show, as plain text. An application names the kind
    /// itself when it is empty.
    pub title: String,
    /// The path of the chapter the landmark leads to.
    pub chapter: String,
    /// The anchor inside that chapter, of a heading or an extra one; `None`
    /// is the start of the chapter.
    pub anchor: Option<String>,
}

impl Landmark {
    /// A landmark that leads where a link with this destination would.
    ///
    /// `href` is a chapter path with an optional anchor after `#`. It is
    /// taken as written: nothing is percent-decoded.
    ///
    /// # Examples
    ///
    /// ```
    /// use mbook::Landmark;
    ///
    /// let landmark = Landmark::new("index", "", "back/index.md#index");
    /// assert_eq!(landmark.chapter, "back/index.md");
    /// assert_eq!(landmark.anchor.as_deref(), Some("index"));
    /// ```
    #[must_use]
    pub fn new(kind: impl Into<String>, title: impl Into<String>, href: &str) -> Self {
        let (chapter, anchor) = split_href(href);
        Self {
            kind: kind.into(),
            title: title.into(),
            chapter,
            anchor,
        }
    }
}

/// A link destination as a chapter path and an anchor; an empty anchor is
/// none.
fn split_href(href: &str) -> (String, Option<String>) {
    match href.split_once('#') {
        Some((chapter, anchor)) if !anchor.is_empty() => {
            (chapter.to_owned(), Some(anchor.to_owned()))
        }
        Some((chapter, _)) => (chapter.to_owned(), None),
        None => (href.to_owned(), None),
    }
}

impl Chapter {
    /// Normalises the text and computes its anchors.
    #[cfg(feature = "write")]
    fn new(path: String, text: String) -> Result<Self> {
        let (text, anchors, extra_anchors) = stored(text)?;
        Ok(Self {
            path,
            text,
            anchors,
            extra_anchors,
        })
    }

    /// The path of the chapter inside the book.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The Markdown text, in the form it is stored in.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The anchors of the headings of the chapter, sorted by offset.
    ///
    /// A chapter loaded from a file keeps the anchors stored in that file
    /// until its text is replaced.
    #[must_use]
    pub fn anchors(&self) -> &[Anchor] {
        &self.anchors
    }

    /// The anchors of the lines the chapter gives a name to, sorted by
    /// offset.
    ///
    /// A line gets a name from an *anchor definition* in front of it: a link
    /// reference definition that names itself, `[fig-1]: #fig-1`. A Markdown
    /// renderer shows nothing for it. A link reaches the line the same way
    /// as a heading: `ch01.md#fig-1`.
    ///
    /// A chapter loaded from a file keeps the extra anchors stored in that
    /// file until its text is replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(feature = "write")]
    /// # fn main() -> mbook::Result<()> {
    /// let mut book = mbook::Book::new("The Example", "en")?;
    /// let chapter = book.add_chapter(
    ///     "ch01.md",
    ///     "# One\n\n[remark]: #remark\nA remark worth a link.\n",
    /// )?;
    ///
    /// let extra: Vec<_> = chapter
    ///     .extra_anchors()
    ///     .iter()
    ///     .map(|anchor| (anchor.name.as_str(), anchor.offset))
    ///     .collect();
    /// assert_eq!(extra, [("remark", 7)]);
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "write"))]
    /// # fn main() {}
    /// ```
    #[must_use]
    pub fn extra_anchors(&self) -> &[Anchor] {
        &self.extra_anchors
    }

    /// Replaces the text and rebuilds the anchors from it: those of its
    /// headings and those of its anchor definitions.
    ///
    /// CRLF and lone CR become a line feed and a leading byte order mark is
    /// dropped. On error the chapter is unchanged.
    ///
    /// # Errors
    ///
    /// The text contains a control character other than line feed and tab.
    #[cfg(feature = "write")]
    pub fn set_text(&mut self, text: impl Into<String>) -> Result<()> {
        (self.text, self.anchors, self.extra_anchors) = stored(text.into())?;
        Ok(())
    }

    /// Tells whether a fragment with this name leads into the chapter.
    #[cfg(feature = "write")]
    fn has_anchor(&self, name: &str) -> bool {
        self.anchor_offset(name).is_some()
    }

    /// Where a fragment with this name leads: the offset of the anchor of a
    /// heading or of an extra one.
    #[cfg(feature = "write")]
    fn anchor_offset(&self, name: &str) -> Option<u32> {
        self.anchors
            .iter()
            .chain(&self.extra_anchors)
            .find(|anchor| anchor.name == name)
            .map(|anchor| anchor.offset)
    }
}

/// The stored form of chapter text, the anchors of its headings and its
/// extra anchors.
#[cfg(feature = "write")]
fn stored(text: String) -> Result<(String, Vec<Anchor>, Vec<Anchor>)> {
    let text = text::normalize(text)?;
    // Positions inside a chapter are 32-bit offsets.
    if u32::try_from(text.len()).is_err() {
        return Err(Error::Limit("a chapter must be smaller than 4 GiB"));
    }
    let analysis = analysis::analyse(&text);
    let anchors = analysis::anchors(&analysis.headings);
    let extra_anchors = analysis::extra_anchors(&analysis.named_lines, &anchors);
    Ok((text, anchors, extra_anchors))
}

impl Picture {
    /// Reads the pixel size from the AVIF file.
    #[cfg(feature = "write")]
    fn new(path: String, data: Vec<u8>) -> Result<Self> {
        let size = avif::displayed_size(&data).map_err(Error::InvalidPicture)?;
        Ok(Self { path, size, data })
    }

    /// The path of the picture inside the book.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The size in pixels, as displayed.
    #[must_use]
    pub fn size(&self) -> Size {
        self.size
    }

    /// The AVIF file.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

/// The stored form of the source of a diagram or of a formula.
///
/// The encoding is that of a chapter: CRLF and lone CR become a line feed and
/// a leading byte order mark is dropped. A source without a character that is
/// not white space describes nothing and is refused.
#[cfg(feature = "write")]
fn stored_source(source: String) -> Result<String, SourceError> {
    let source = text::normalize(source)?;
    if source.trim().is_empty() {
        return Err(SourceError::Empty);
    }
    Ok(source)
}

impl Diagram {
    /// Normalises the source.
    #[cfg(feature = "write")]
    fn new(path: String, source: String) -> Result<Self> {
        let source = stored_source(source).map_err(Error::InvalidDiagram)?;
        Ok(Self { path, source })
    }

    /// The path of the diagram inside the book.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The description of the diagram in Mermaid notation, in the form it is
    /// stored in.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Replaces the description of the diagram.
    ///
    /// CRLF and lone CR become a line feed and a leading byte order mark is
    /// dropped. On error the diagram is unchanged.
    ///
    /// # Errors
    ///
    /// The source is empty, or contains a control character other than line
    /// feed and tab.
    #[cfg(feature = "write")]
    pub fn set_source(&mut self, source: impl Into<String>) -> Result<()> {
        self.source = stored_source(source.into()).map_err(Error::InvalidDiagram)?;
        Ok(())
    }
}

impl Formula {
    /// Normalises the source.
    #[cfg(feature = "write")]
    fn new(path: String, source: String) -> Result<Self> {
        let source = stored_source(source).map_err(Error::InvalidFormula)?;
        Ok(Self { path, source })
    }

    /// The path of the formula inside the book.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The formula in the notation of `KaTeX`, in the form it is stored in.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Replaces the formula.
    ///
    /// CRLF and lone CR become a line feed and a leading byte order mark is
    /// dropped. On error the formula is unchanged.
    ///
    /// # Errors
    ///
    /// The source is empty, or contains a control character other than line
    /// feed and tab.
    #[cfg(feature = "write")]
    pub fn set_source(&mut self, source: impl Into<String>) -> Result<()> {
        self.source = stored_source(source.into()).map_err(Error::InvalidFormula)?;
        Ok(())
    }
}

/// Looking at a book.
impl Book {
    /// The metadata.
    #[must_use]
    pub fn meta(&self) -> &Meta {
        &self.meta
    }

    /// The chapters, in reading order.
    #[must_use]
    pub fn chapters(&self) -> &[Chapter] {
        &self.chapters
    }

    /// The chapter with this path.
    #[must_use]
    pub fn chapter(&self, path: &str) -> Option<&Chapter> {
        match self.slots.get(path)? {
            Slot::Chapter(index) => self.chapters.get(*index),
            _ => None,
        }
    }

    /// The pictures, in the order they were added.
    #[must_use]
    pub fn pictures(&self) -> &[Picture] {
        &self.pictures
    }

    /// The picture with this path.
    #[must_use]
    pub fn picture(&self, path: &str) -> Option<&Picture> {
        match self.slots.get(path)? {
            Slot::Picture(index) => self.pictures.get(*index),
            _ => None,
        }
    }

    /// The diagrams, in the order they were added.
    #[must_use]
    pub fn diagrams(&self) -> &[Diagram] {
        &self.diagrams
    }

    /// The diagram with this path.
    #[must_use]
    pub fn diagram(&self, path: &str) -> Option<&Diagram> {
        match self.slots.get(path)? {
            Slot::Diagram(index) => self.diagrams.get(*index),
            _ => None,
        }
    }

    /// The formulas, in the order they were added.
    #[must_use]
    pub fn formulas(&self) -> &[Formula] {
        &self.formulas
    }

    /// The formula with this path.
    #[must_use]
    pub fn formula(&self, path: &str) -> Option<&Formula> {
        match self.slots.get(path)? {
            Slot::Formula(index) => self.formulas.get(*index),
            _ => None,
        }
    }

    /// The table of contents.
    #[must_use]
    pub fn toc(&self) -> &Toc {
        &self.toc
    }

    /// The landmarks: the notable places of the book, in the order an
    /// application lists them. Empty for a book that has none.
    #[must_use]
    pub fn landmarks(&self) -> &[Landmark] {
        &self.landmarks
    }
}

/// Bookkeeping shared by loading and editing.
impl Book {
    /// A book that holds only its metadata.
    fn empty(meta: Meta) -> Self {
        Self {
            meta,
            chapters: Vec::new(),
            pictures: Vec::new(),
            diagrams: Vec::new(),
            formulas: Vec::new(),
            toc: Toc::default(),
            landmarks: Vec::new(),
            slots: HashMap::new(),
        }
    }

    /// Puts a chapter with a free path at a valid position.
    fn place_chapter(&mut self, index: usize, chapter: Chapter) -> &mut Chapter {
        self.slots
            .insert(chapter.path.clone(), Slot::Chapter(index));
        self.chapters.insert(index, chapter);
        self.renumber_chapters(index + 1);
        &mut self.chapters[index]
    }

    /// Puts a picture with a free path after the others.
    fn place_picture(&mut self, picture: Picture) -> &Picture {
        let index = self.pictures.len();
        self.slots
            .insert(picture.path.clone(), Slot::Picture(index));
        self.pictures.push(picture);
        &self.pictures[index]
    }

    /// Puts a diagram with a free path after the others.
    fn place_diagram(&mut self, diagram: Diagram) -> &mut Diagram {
        let index = self.diagrams.len();
        self.slots
            .insert(diagram.path.clone(), Slot::Diagram(index));
        self.diagrams.push(diagram);
        &mut self.diagrams[index]
    }

    /// Puts a formula with a free path after the others.
    fn place_formula(&mut self, formula: Formula) -> &mut Formula {
        let index = self.formulas.len();
        self.slots
            .insert(formula.path.clone(), Slot::Formula(index));
        self.formulas.push(formula);
        &mut self.formulas[index]
    }

    /// Updates the slots of the chapters from `from` on after a shift.
    fn renumber_chapters(&mut self, from: usize) {
        let paths = self.chapters.iter().map(|chapter| chapter.path.as_str());
        renumber(&mut self.slots, paths, from, Slot::Chapter);
    }
}

/// Points the slots of the items from `from` on at their places after a
/// shift. `paths` holds the path of every item, in order.
fn renumber<'a>(
    slots: &mut HashMap<String, Slot>,
    paths: impl Iterator<Item = &'a str>,
    from: usize,
    slot_of: impl Fn(usize) -> Slot,
) {
    for (index, path) in paths.enumerate().skip(from) {
        if let Some(slot) = slots.get_mut(path) {
            *slot = slot_of(index);
        }
    }
}

/// Creating, changing and saving a book.
#[cfg(feature = "write")]
impl Book {
    /// A book with a title and a main language, and nothing else.
    ///
    /// The book gets a new UUID version 7 as its identifier; replace it with
    /// [`Meta::set_id`] when the source has a stable one. The table of
    /// contents is built from the headings unless [`set_toc`](Self::set_toc)
    /// says otherwise.
    ///
    /// # Errors
    ///
    /// The title is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// let book = mbook::Book::new("The Example", "en")?;
    /// assert!(book.meta().id().starts_with("urn:uuid:"));
    /// # Ok::<(), mbook::Error>(())
    /// ```
    pub fn new(title: impl Into<String>, language: impl Into<String>) -> Result<Self> {
        let id = format!("urn:uuid:{}", uuid::Uuid::now_v7());
        Ok(Self::empty(Meta::new(title.into(), language.into(), id)?))
    }

    /// The metadata, for changing it.
    pub fn meta_mut(&mut self) -> &mut Meta {
        &mut self.meta
    }

    /// The chapter with this path, for changing its text.
    pub fn chapter_mut(&mut self, path: &str) -> Option<&mut Chapter> {
        match self.slots.get(path)? {
            Slot::Chapter(index) => self.chapters.get_mut(*index),
            _ => None,
        }
    }

    /// Adds a chapter at the end of the reading order.
    ///
    /// CRLF and lone CR in the text become a line feed and a leading byte
    /// order mark is dropped.
    ///
    /// # Errors
    ///
    /// The path is not valid or is already used, or the text contains a
    /// control character other than line feed and tab.
    pub fn add_chapter(
        &mut self,
        path: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<&mut Chapter> {
        self.insert_chapter(self.chapters.len(), path, text)
    }

    /// Adds a chapter at a position in the reading order; the chapters from
    /// that position on move one place later.
    ///
    /// # Errors
    ///
    /// As [`add_chapter`](Self::add_chapter), or `index` is greater than the
    /// number of chapters.
    pub fn insert_chapter(
        &mut self,
        index: usize,
        path: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<&mut Chapter> {
        self.check_position(index, self.chapters.len())?;
        let path = self.free_path(path.into())?;
        let chapter = Chapter::new(path, text.into())?;
        Ok(self.place_chapter(index, chapter))
    }

    /// Moves a chapter to another position in the reading order.
    ///
    /// # Errors
    ///
    /// No chapter has this path, or `index` is not less than the number of
    /// chapters.
    pub fn move_chapter(&mut self, path: &str, index: usize) -> Result<()> {
        let Some(Slot::Chapter(from)) = self.slots.get(path).copied() else {
            return Err(Error::NoSuchPath(path.to_owned()));
        };
        self.check_position(index, self.chapters.len() - 1)?;
        let chapter = self.chapters.remove(from);
        self.chapters.insert(index, chapter);
        self.renumber_chapters(from.min(index));
        Ok(())
    }

    /// Removes a chapter and returns it.
    ///
    /// Links to it from other chapters are reported when the book is written.
    pub fn remove_chapter(&mut self, path: &str) -> Option<Chapter> {
        let Slot::Chapter(index) = self.slots.get(path).copied()? else {
            return None;
        };
        self.slots.remove(path);
        let chapter = self.chapters.remove(index);
        self.renumber_chapters(index);
        Some(chapter)
    }

    /// Adds a picture. The pixel size is read from the AVIF file.
    ///
    /// # Errors
    ///
    /// The path is not valid or is already used, or the bytes are not an AVIF
    /// file with a primary image.
    pub fn add_picture(
        &mut self,
        path: impl Into<String>,
        avif: impl Into<Vec<u8>>,
    ) -> Result<&Picture> {
        let path = self.free_path(path.into())?;
        let picture = Picture::new(path, avif.into())?;
        Ok(self.place_picture(picture))
    }

    /// Removes a picture and returns it.
    ///
    /// References to it from chapters, and from the `cover` key, are reported
    /// when the book is written.
    pub fn remove_picture(&mut self, path: &str) -> Option<Picture> {
        let Slot::Picture(index) = self.slots.get(path).copied()? else {
            return None;
        };
        self.slots.remove(path);
        let picture = self.pictures.remove(index);
        let paths = self.pictures.iter().map(|picture| picture.path.as_str());
        renumber(&mut self.slots, paths, index, Slot::Picture);
        Some(picture)
    }

    /// The diagram with this path, for changing its source.
    pub fn diagram_mut(&mut self, path: &str) -> Option<&mut Diagram> {
        match self.slots.get(path)? {
            Slot::Diagram(index) => self.diagrams.get_mut(*index),
            _ => None,
        }
    }

    /// Adds a diagram, described in Mermaid notation.
    ///
    /// A chapter shows it the way it shows a picture, by its path:
    /// `![The flow of a request](dia/flow.mmd)`. CRLF and lone CR in the
    /// source become a line feed and a leading byte order mark is dropped.
    /// The notation itself is not checked: a reading application shows the
    /// alternative text of a diagram it cannot draw.
    ///
    /// # Errors
    ///
    /// The path is not valid or is already used, or the source is empty or
    /// contains a control character other than line feed and tab.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut book = mbook::Book::new("The Example", "en")?;
    /// book.add_diagram("dia/flow.mmd", "flowchart LR\n  A --> B\n")?;
    /// book.add_chapter("ch01.md", "![From A to B](dia/flow.mmd)\n")?;
    /// assert!(book.check().is_ok());
    /// # Ok::<(), mbook::Error>(())
    /// ```
    pub fn add_diagram(
        &mut self,
        path: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<&mut Diagram> {
        let path = self.free_path(path.into())?;
        let diagram = Diagram::new(path, source.into())?;
        Ok(self.place_diagram(diagram))
    }

    /// Removes a diagram and returns it.
    ///
    /// References to it from chapters are reported when the book is written.
    pub fn remove_diagram(&mut self, path: &str) -> Option<Diagram> {
        let Slot::Diagram(index) = self.slots.get(path).copied()? else {
            return None;
        };
        self.slots.remove(path);
        let diagram = self.diagrams.remove(index);
        let paths = self.diagrams.iter().map(|diagram| diagram.path.as_str());
        renumber(&mut self.slots, paths, index, Slot::Diagram);
        Some(diagram)
    }

    /// The formula with this path, for changing its source.
    pub fn formula_mut(&mut self, path: &str) -> Option<&mut Formula> {
        match self.slots.get(path)? {
            Slot::Formula(index) => self.formulas.get_mut(*index),
            _ => None,
        }
    }

    /// Adds a formula, written in the notation of `KaTeX` without the `$`
    /// that surround a formula in running text.
    ///
    /// A chapter shows it the way it shows a picture, by its path:
    /// `![E = mc^2](math/e.tex)`. CRLF and lone CR in the source become a
    /// line feed and a leading byte order mark is dropped. The notation
    /// itself is not checked: a reading application shows the alternative
    /// text of a formula it cannot set.
    ///
    /// # Errors
    ///
    /// The path is not valid or is already used, or the source is empty or
    /// contains a control character other than line feed and tab.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut book = mbook::Book::new("The Example", "en")?;
    /// book.add_formula("math/e.tex", "E = mc^2")?;
    /// book.add_chapter("ch01.md", "Mass is energy: ![E = mc^2](math/e.tex).\n")?;
    /// assert!(book.check().is_ok());
    /// # Ok::<(), mbook::Error>(())
    /// ```
    pub fn add_formula(
        &mut self,
        path: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<&mut Formula> {
        let path = self.free_path(path.into())?;
        let formula = Formula::new(path, source.into())?;
        Ok(self.place_formula(formula))
    }

    /// Removes a formula and returns it.
    ///
    /// References to it from chapters are reported when the book is written.
    pub fn remove_formula(&mut self, path: &str) -> Option<Formula> {
        let Slot::Formula(index) = self.slots.get(path).copied()? else {
            return None;
        };
        self.slots.remove(path);
        let formula = self.formulas.remove(index);
        let paths = self.formulas.iter().map(|formula| formula.path.as_str());
        renumber(&mut self.slots, paths, index, Slot::Formula);
        Some(formula)
    }

    /// Adds a picture and makes it the cover.
    ///
    /// A picture that was the cover before stays in the book.
    ///
    /// # Errors
    ///
    /// As [`add_picture`](Self::add_picture).
    pub fn set_cover(
        &mut self,
        path: impl Into<String>,
        avif: impl Into<Vec<u8>>,
    ) -> Result<&Picture> {
        let path = self.free_path(path.into())?;
        let picture = Picture::new(path.clone(), avif.into())?;
        self.meta.set_cover(path)?;
        Ok(self.place_picture(picture))
    }

    /// Replaces the table of contents.
    pub fn set_toc(&mut self, toc: Toc) {
        self.toc = toc;
    }

    /// Replaces the landmarks. An empty list removes them.
    ///
    /// Whether every landmark leads somewhere is checked when the book is
    /// written, like the items of the contents. On error the book is
    /// unchanged.
    ///
    /// # Errors
    ///
    /// A kind does not match `[a-z0-9]+(-[a-z0-9]+)*`, or more than one
    /// landmark has the kind `start`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mbook::{Book, Landmark};
    ///
    /// let mut book = Book::new("The Example", "en")?;
    /// book.add_chapter("ch01.md", "# Arrival\n")?;
    /// book.set_landmarks(vec![Landmark::new("start", "", "ch01.md")])?;
    /// assert_eq!(book.landmarks().len(), 1);
    /// # Ok::<(), mbook::Error>(())
    /// ```
    pub fn set_landmarks(&mut self, landmarks: Vec<Landmark>) -> Result<()> {
        let invalid = landmarks
            .iter()
            .find(|landmark| !crate::meta::is_key(&landmark.kind));
        if let Some(landmark) = invalid {
            return Err(LandmarkError::InvalidKind(landmark.kind.clone()).into());
        }
        if landmarks.iter().filter(|l| l.kind == "start").count() > 1 {
            return Err(LandmarkError::RepeatedStart.into());
        }
        self.landmarks = landmarks;
        Ok(())
    }

    /// An index into the reading order must not be greater than `max`.
    fn check_position(&self, index: usize, max: usize) -> Result<()> {
        if index > max {
            return Err(Error::IndexOutOfRange {
                index,
                len: self.chapters.len(),
            });
        }
        Ok(())
    }

    /// Checks that a path is valid and not used yet, and hands it back.
    fn free_path(&self, path: String) -> Result<String> {
        if let Err(reason) = path::validate(&path) {
            return Err(Error::InvalidPath { path, reason });
        }
        if self.slots.contains_key(&path) {
            return Err(Error::DuplicatePath(path));
        }
        Ok(path)
    }
}

#[cfg(test)]
#[cfg(feature = "write")]
mod tests {
    use super::{Book, Chapter, Diagram, Formula, Landmark, Picture, Toc, TocItem};
    use crate::error::{Error, LandmarkError, PathError, SourceError};
    use crate::types::Size;

    /// A chapter with one heading and two paragraphs, the first of them
    /// named.
    const TEXT: &str = "# One\n\n[remark]: #remark\nA remark.\n\nMore.\n";

    fn analysed(text: &str) -> Chapter {
        Chapter::new("ch01.md".to_owned(), text.to_owned()).unwrap()
    }

    fn extra(chapter: &Chapter) -> Vec<(&str, u32)> {
        let anchors = chapter.extra_anchors().iter();
        anchors
            .map(|anchor| (anchor.name.as_str(), anchor.offset))
            .collect()
    }

    #[test]
    fn an_anchor_definition_names_its_line() {
        assert_eq!(extra(&analysed(TEXT)), [("remark", 7)]);
    }

    #[test]
    fn an_anchor_definition_leaves_the_anchors_of_headings_alone() {
        let chapter = analysed(TEXT);
        let names: Vec<_> = chapter.anchors().iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["one"]);
    }

    #[test]
    fn a_chapter_without_anchor_definitions_has_no_extra_anchors() {
        assert_eq!(analysed("# One\n\nText.\n").extra_anchors(), []);
    }

    #[test]
    fn set_text_moves_an_extra_anchor_with_its_line() {
        let mut chapter = analysed(TEXT);
        let edited = chapter.text().replace("# One", "# One and a half");
        chapter.set_text(edited).unwrap();
        assert_eq!(extra(&chapter), [("remark", 18)]);
    }

    #[test]
    fn set_text_drops_an_extra_anchor_whose_definition_is_gone() {
        let mut chapter = analysed(TEXT);
        chapter.set_text("# One\n\nA remark.\n").unwrap();
        assert_eq!(chapter.extra_anchors(), []);
    }

    #[test]
    fn set_text_finds_a_new_anchor_definition() {
        let mut chapter = analysed("# One\n\nMore.\n");
        chapter.set_text("# One\n\n[more]: #more\nMore.\n").unwrap();
        assert_eq!(extra(&chapter), [("more", 7)]);
    }

    #[test]
    fn a_text_that_is_refused_keeps_the_extra_anchors() {
        let mut chapter = analysed(TEXT);
        assert!(chapter.set_text("bad \u{1b} text").is_err());
        assert_eq!(extra(&chapter), [("remark", 7)]);
    }

    /// A chapter built by hand, for the tests that only move chapters.
    fn chapter(path: &str) -> Chapter {
        Chapter {
            path: path.to_owned(),
            text: format!("# {path}\n"),
            anchors: Vec::new(),
            extra_anchors: Vec::new(),
        }
    }

    fn picture(path: &str) -> Picture {
        Picture {
            path: path.to_owned(),
            size: Size {
                width: 1,
                height: 1,
            },
            data: Vec::new(),
        }
    }

    /// A book with three chapters and two pictures.
    fn book() -> Book {
        let mut book = Book::new("The Example", "en").unwrap();
        for path in ["ch01.md", "ch02.md", "ch03.md"] {
            book.place_chapter(book.chapters.len(), chapter(path));
        }
        for path in ["img/a.avif", "img/b.avif"] {
            book.place_picture(picture(path));
        }
        book
    }

    fn reading_order(book: &Book) -> Vec<&str> {
        book.chapters().iter().map(Chapter::path).collect()
    }

    /// Every path finds the item that carries it.
    fn slots_agree(book: &Book) -> bool {
        let chapters = book
            .chapters()
            .iter()
            .all(|c| book.chapter(c.path()) == Some(c));
        let pictures = book
            .pictures()
            .iter()
            .all(|p| book.picture(p.path()) == Some(p));
        let diagrams = book
            .diagrams()
            .iter()
            .all(|d| book.diagram(d.path()) == Some(d));
        let formulas = book
            .formulas()
            .iter()
            .all(|f| book.formula(f.path()) == Some(f));
        let items =
            book.chapters.len() + book.pictures.len() + book.diagrams.len() + book.formulas.len();
        chapters && pictures && diagrams && formulas && book.slots.len() == items
    }

    /// The example with two diagrams and two formulas.
    fn illustrated() -> Book {
        let mut book = book();
        for path in ["dia/a.mmd", "dia/b.mmd"] {
            book.add_diagram(path, "flowchart LR\n  A --> B\n").unwrap();
        }
        for path in ["math/a.tex", "math/b.tex"] {
            book.add_formula(path, "E = mc^2").unwrap();
        }
        book
    }

    #[test]
    fn a_diagram_exposes_its_path_and_source() {
        let book = illustrated();
        let diagram = book.diagram("dia/a.mmd").unwrap();
        assert_eq!(
            (diagram.path(), diagram.source()),
            ("dia/a.mmd", "flowchart LR\n  A --> B\n")
        );
    }

    #[test]
    fn a_formula_exposes_its_path_and_source() {
        let book = illustrated();
        let formula = book.formula("math/b.tex").unwrap();
        assert_eq!(
            (formula.path(), formula.source()),
            ("math/b.tex", "E = mc^2")
        );
    }

    #[test]
    fn diagrams_and_formulas_are_listed_in_the_order_they_were_added() {
        let book = illustrated();
        let diagrams: Vec<_> = book.diagrams().iter().map(Diagram::path).collect();
        let formulas: Vec<_> = book.formulas().iter().map(Formula::path).collect();
        assert_eq!(
            (diagrams, formulas),
            (
                vec!["dia/a.mmd", "dia/b.mmd"],
                vec!["math/a.tex", "math/b.tex"]
            )
        );
        assert!(slots_agree(&book));
    }

    #[test]
    fn a_path_of_another_kind_is_neither_a_diagram_nor_a_formula() {
        let mut book = illustrated();
        assert!(book.diagram("math/a.tex").is_none());
        assert!(book.diagram("ch01.md").is_none());
        assert!(book.formula("dia/a.mmd").is_none());
        assert!(book.formula("img/a.avif").is_none());
        assert!(book.diagram_mut("math/a.tex").is_none());
        assert!(book.formula_mut("dia/a.mmd").is_none());
        assert!(book.picture("dia/a.mmd").is_none());
        assert!(book.chapter("math/a.tex").is_none());
        assert!(book.chapter_mut("dia/a.mmd").is_none());
    }

    #[test]
    fn a_source_is_stored_with_line_feeds_and_without_a_byte_order_mark() {
        let mut book = book();
        let diagram = book.add_diagram("d.mmd", "\u{feff}flowchart LR\r\n  A --> B\r");
        assert_eq!(
            diagram.map(|diagram| diagram.source().to_owned()).unwrap(),
            "flowchart LR\n  A --> B\n"
        );
        let formula = book.add_formula("f.tex", "a\r\n+ b");
        assert_eq!(
            formula.map(|formula| formula.source().to_owned()).unwrap(),
            "a\n+ b"
        );
    }

    #[test]
    fn a_source_without_a_visible_character_is_refused() {
        let mut book = book();
        let diagram = book.add_diagram("d.mmd", " \n\t\n");
        assert!(matches!(
            diagram,
            Err(Error::InvalidDiagram(SourceError::Empty))
        ));
        let formula = book.add_formula("f.tex", "");
        assert!(matches!(
            formula,
            Err(Error::InvalidFormula(SourceError::Empty))
        ));
        assert!(slots_agree(&book));
    }

    #[test]
    fn a_source_with_a_control_character_is_refused_with_its_place() {
        let mut book = book();
        let diagram = book.add_diagram("d.mmd", "flow\u{1b}chart");
        assert!(matches!(
            diagram,
            Err(Error::InvalidDiagram(SourceError::ForbiddenCharacter(found)))
                if (found.offset, found.character) == (4, '\u{1b}')
        ));
        let formula = book.add_formula("f.tex", "\0");
        assert!(matches!(
            formula,
            Err(Error::InvalidFormula(SourceError::ForbiddenCharacter(found)))
                if found.offset == 0
        ));
    }

    #[test]
    fn a_diagram_and_a_formula_need_a_free_and_valid_path() {
        let mut book = illustrated();
        assert!(matches!(
            book.add_diagram("ch01.md", "flowchart LR"),
            Err(Error::DuplicatePath(path)) if path == "ch01.md"
        ));
        assert!(matches!(
            book.add_formula("dia/a.mmd", "x"),
            Err(Error::DuplicatePath(path)) if path == "dia/a.mmd"
        ));
        assert!(matches!(
            book.add_diagram("/d.mmd", "flowchart LR"),
            Err(Error::InvalidPath {
                reason: PathError::EmptySegment,
                ..
            })
        ));
        assert!(matches!(
            book.add_formula("", "x"),
            Err(Error::InvalidPath {
                reason: PathError::Empty,
                ..
            })
        ));
        assert!(matches!(
            book.add_chapter("math/a.tex", "text"),
            Err(Error::DuplicatePath(path)) if path == "math/a.tex"
        ));
    }

    #[test]
    fn set_source_replaces_the_source_of_a_diagram_and_of_a_formula() {
        let mut book = illustrated();
        let diagram = book.diagram_mut("dia/b.mmd").unwrap();
        diagram
            .set_source("sequenceDiagram\r\n  A->>B: hi")
            .unwrap();
        assert_eq!(diagram.source(), "sequenceDiagram\n  A->>B: hi");
        let formula = book.formula_mut("math/a.tex").unwrap();
        formula.set_source("a^2 + b^2 = c^2").unwrap();
        assert_eq!(formula.source(), "a^2 + b^2 = c^2");
    }

    #[test]
    fn a_source_that_is_refused_keeps_the_one_before() {
        let mut book = illustrated();
        let diagram = book.diagram_mut("dia/a.mmd").unwrap();
        assert!(matches!(
            diagram.set_source("  "),
            Err(Error::InvalidDiagram(SourceError::Empty))
        ));
        assert_eq!(diagram.source(), "flowchart LR\n  A --> B\n");
        let formula = book.formula_mut("math/a.tex").unwrap();
        assert!(matches!(
            formula.set_source("x\u{7}"),
            Err(Error::InvalidFormula(SourceError::ForbiddenCharacter(_)))
        ));
        assert_eq!(formula.source(), "E = mc^2");
    }

    #[test]
    fn slots_follow_a_removed_diagram_and_a_removed_formula() {
        let mut book = illustrated();
        let diagram = book.remove_diagram("dia/a.mmd");
        let formula = book.remove_formula("math/a.tex");
        assert_eq!(diagram.as_ref().map(Diagram::path), Some("dia/a.mmd"));
        assert_eq!(formula.as_ref().map(Formula::path), Some("math/a.tex"));
        assert_eq!(
            book.diagram("dia/b.mmd").map(Diagram::path),
            Some("dia/b.mmd")
        );
        assert_eq!(
            book.formula("math/b.tex").map(Formula::path),
            Some("math/b.tex")
        );
        assert!(slots_agree(&book));
    }

    #[test]
    fn removing_ignores_a_path_of_another_kind() {
        let mut book = illustrated();
        assert!(book.remove_diagram("math/a.tex").is_none());
        assert!(book.remove_diagram("nothing.mmd").is_none());
        assert!(book.remove_formula("dia/a.mmd").is_none());
        assert!(book.remove_picture("dia/a.mmd").is_none());
        assert!(book.remove_chapter("math/a.tex").is_none());
        assert!(slots_agree(&book));
    }

    #[test]
    fn a_chapter_exposes_its_path_text_and_anchors() {
        let chapter = chapter("ch01.md");
        assert_eq!(
            (chapter.path(), chapter.text(), chapter.anchors().len()),
            ("ch01.md", "# ch01.md\n", 0)
        );
    }

    #[test]
    fn a_picture_exposes_its_path_size_and_data() {
        let picture = picture("img/a.avif");
        let size = Size {
            width: 1,
            height: 1,
        };
        assert_eq!(
            (picture.path(), picture.size(), picture.data()),
            ("img/a.avif", size, &[][..])
        );
    }

    #[test]
    fn pictures_are_found_by_path() {
        let book = book();
        assert_eq!(
            book.picture("img/b.avif").map(Picture::path),
            Some("img/b.avif")
        );
    }

    #[test]
    fn a_chapter_path_is_not_a_picture() {
        assert!(book().picture("ch01.md").is_none());
    }

    #[test]
    fn chapter_mut_finds_the_chapter_to_change() {
        let mut book = book();
        assert_eq!(
            book.chapter_mut("ch03.md").map(|c| c.path().to_owned()),
            Some("ch03.md".to_owned())
        );
    }

    #[test]
    fn chapter_mut_ignores_a_picture_path() {
        assert!(book().chapter_mut("img/a.avif").is_none());
    }

    #[test]
    fn remove_picture_returns_the_picture() {
        let removed = book().remove_picture("img/a.avif");
        assert_eq!(removed.as_ref().map(Picture::path), Some("img/a.avif"));
    }

    #[test]
    fn remove_picture_ignores_a_chapter_path() {
        assert!(book().remove_picture("ch01.md").is_none());
    }

    #[test]
    fn new_book_lists_every_heading_in_its_contents() {
        let book = Book::new("The Example", "en").unwrap();
        assert_eq!(book.toc(), &Toc::FromHeadings { max_level: 6 });
    }

    #[test]
    fn new_book_gets_a_version_7_uuid() {
        let book = Book::new("The Example", "en").unwrap();
        let id = book.meta().id().strip_prefix("urn:uuid:").unwrap();
        assert_eq!(id.as_bytes()[14], b'7');
    }

    #[test]
    fn new_rejects_an_empty_title() {
        assert!(matches!(Book::new("", "en"), Err(Error::InvalidMeta(_))));
    }

    #[test]
    fn chapters_are_found_by_path() {
        let book = book();
        assert_eq!(book.chapter("ch02.md").map(Chapter::path), Some("ch02.md"));
    }

    #[test]
    fn a_picture_path_is_not_a_chapter() {
        assert!(book().chapter("img/a.avif").is_none());
    }

    #[test]
    fn placing_a_chapter_in_the_middle_shifts_the_later_ones() {
        let mut book = book();
        book.place_chapter(1, chapter("intro.md"));
        assert_eq!(
            reading_order(&book),
            ["ch01.md", "intro.md", "ch02.md", "ch03.md"]
        );
    }

    #[test]
    fn slots_follow_a_chapter_placed_in_the_middle() {
        let mut book = book();
        book.place_chapter(1, chapter("intro.md"));
        assert!(slots_agree(&book));
    }

    #[test]
    fn remove_chapter_returns_the_chapter() {
        let removed = book().remove_chapter("ch02.md");
        assert_eq!(removed.as_ref().map(Chapter::path), Some("ch02.md"));
    }

    #[test]
    fn slots_follow_a_removed_chapter() {
        let mut book = book();
        book.remove_chapter("ch01.md");
        assert!(slots_agree(&book));
    }

    #[test]
    fn remove_chapter_ignores_a_picture_path() {
        let mut book = book();
        assert!(book.remove_chapter("img/a.avif").is_none());
    }

    #[test]
    fn move_chapter_to_the_end_changes_the_reading_order() {
        let mut book = book();
        book.move_chapter("ch01.md", 2).unwrap();
        assert_eq!(reading_order(&book), ["ch02.md", "ch03.md", "ch01.md"]);
    }

    #[test]
    fn move_chapter_to_the_start_changes_the_reading_order() {
        let mut book = book();
        book.move_chapter("ch03.md", 0).unwrap();
        assert_eq!(reading_order(&book), ["ch03.md", "ch01.md", "ch02.md"]);
    }

    #[test]
    fn slots_follow_a_moved_chapter() {
        let mut book = book();
        book.move_chapter("ch03.md", 0).unwrap();
        assert!(slots_agree(&book));
    }

    #[test]
    fn move_chapter_rejects_a_position_past_the_end() {
        let result = book().move_chapter("ch01.md", 3);
        assert!(matches!(
            result,
            Err(Error::IndexOutOfRange { index: 3, len: 3 })
        ));
    }

    #[test]
    fn move_chapter_rejects_an_unknown_path() {
        let result = book().move_chapter("nope.md", 0);
        assert!(matches!(result, Err(Error::NoSuchPath(path)) if path == "nope.md"));
    }

    #[test]
    fn slots_follow_a_removed_picture() {
        let mut book = book();
        book.remove_picture("img/a.avif");
        assert!(slots_agree(&book));
    }

    #[test]
    fn add_chapter_rejects_a_path_used_by_a_picture() {
        let mut book = book();
        let result = book.add_chapter("img/a.avif", "text");
        assert!(matches!(result, Err(Error::DuplicatePath(path)) if path == "img/a.avif"));
    }

    #[test]
    fn add_picture_rejects_an_invalid_path() {
        let mut book = book();
        let result = book.add_picture("../cover.avif", Vec::new());
        let reason = PathError::DotSegment;
        assert!(matches!(result, Err(Error::InvalidPath { reason: r, .. }) if r == reason));
    }

    #[test]
    fn insert_chapter_rejects_a_position_past_the_end() {
        let mut book = book();
        let result = book.insert_chapter(4, "ch04.md", "text");
        assert!(matches!(
            result,
            Err(Error::IndexOutOfRange { index: 4, len: 3 })
        ));
    }

    #[test]
    fn a_landmark_is_built_from_a_link_destination() {
        let whole = Landmark::new("start", "", "ch01.md");
        let inside = Landmark::new("index", "The Index", "back.md#index");
        assert_eq!(
            (whole.kind.as_str(), whole.chapter.as_str(), whole.anchor),
            ("start", "ch01.md", None)
        );
        assert_eq!(
            (inside.title.as_str(), inside.anchor.as_deref()),
            ("The Index", Some("index"))
        );
    }

    #[test]
    fn set_landmarks_replaces_the_landmarks() {
        let mut book = book();
        book.set_landmarks(vec![Landmark::new("start", "", "ch01.md")])
            .unwrap();
        book.set_landmarks(vec![
            Landmark::new("appendix", "A", "ch02.md"),
            Landmark::new("appendix", "B", "ch03.md"),
            Landmark::new("x-maps", "Maps", "ch03.md"),
        ])
        .unwrap();
        let kinds: Vec<_> = book.landmarks().iter().map(|l| l.kind.as_str()).collect();
        assert_eq!(kinds, ["appendix", "appendix", "x-maps"]);
    }

    #[test]
    fn set_landmarks_rejects_a_kind_that_is_not_a_key() {
        let mut book = book();
        let result = book.set_landmarks(vec![Landmark::new("The Index", "", "ch01.md")]);
        assert!(matches!(
            result,
            Err(Error::InvalidLandmark(LandmarkError::InvalidKind(kind))) if kind == "The Index"
        ));
    }

    #[test]
    fn set_landmarks_rejects_a_second_start() {
        let mut book = book();
        let result = book.set_landmarks(vec![
            Landmark::new("start", "", "ch01.md"),
            Landmark::new("start", "", "ch02.md"),
        ]);
        assert!(matches!(
            result,
            Err(Error::InvalidLandmark(LandmarkError::RepeatedStart))
        ));
    }

    #[test]
    fn refused_landmarks_leave_the_book_as_it_was() {
        let mut book = book();
        book.set_landmarks(vec![Landmark::new("start", "", "ch01.md")])
            .unwrap();
        assert!(
            book.set_landmarks(vec![Landmark::new("", "", "ch01.md")])
                .is_err()
        );
        assert_eq!(book.landmarks().len(), 1);
    }

    #[test]
    fn a_new_book_has_no_landmarks() {
        assert_eq!(Book::new("The Example", "en").unwrap().landmarks(), []);
    }

    #[test]
    fn toc_item_without_a_fragment_leads_to_the_chapter_start() {
        assert_eq!(TocItem::new(1, "Arrival", "ch01.md").anchor, None);
    }

    #[test]
    fn toc_item_with_an_empty_fragment_leads_to_the_chapter_start() {
        assert_eq!(TocItem::new(1, "Arrival", "ch01.md#").anchor, None);
    }

    #[test]
    fn toc_item_splits_at_the_first_hash() {
        let item = TocItem::new(2, "Odd", "ch01.md#a#b");
        assert_eq!(
            (item.chapter.as_str(), item.anchor.as_deref()),
            ("ch01.md", Some("a#b"))
        );
    }
}
