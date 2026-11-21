//! A whole book in memory.

#[cfg(feature = "write")]
use crate::error::Error;
#[cfg(feature = "write")]
use crate::error::Problems;
#[cfg(any(feature = "read", feature = "write"))]
use crate::error::Result;
use crate::meta::Meta;
#[cfg(feature = "read")]
use crate::options::ReadOptions;
#[cfg(feature = "write")]
use crate::options::WriteOptions;
use crate::types::{Anchor, Size};
#[cfg(feature = "write")]
use crate::{analysis, avif, path, text};
use std::collections::HashMap;
#[cfg(feature = "read")]
use std::io::Read;
#[cfg(feature = "write")]
use std::io::Write;
#[cfg(any(feature = "read", feature = "write"))]
use std::path::Path;

/// A whole book in memory: metadata, chapters in reading order, pictures and
/// the table of contents.
///
/// A `Book` is how a book is created and how an existing one is changed.
/// Chapters and pictures are addressed by path. Nothing in it is a byte
/// offset the caller has to keep correct: anchors are computed from the
/// headings of each chapter, and contents items name their target the way a
/// link does.
///
/// # Examples
///
/// Creating a book:
///
/// ```no_run
/// # #[cfg(feature = "write")]
/// # fn main() -> mbook_core::Result<()> {
/// use mbook_core::Book;
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
/// # fn main() -> mbook_core::Result<()> {
/// use mbook_core::Book;
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
#[derive(Clone, Debug)]
pub struct Book {
    meta: Meta,
    chapters: Vec<Chapter>,
    pictures: Vec<Picture>,
    toc: Toc,
    /// Where each path lives; paths are unique across chapters and pictures.
    slots: HashMap<String, Slot>,
}

/// The place of a path in a `Book`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    not(feature = "write"),
    expect(
        dead_code,
        reason = "`Book::read` fills the slots once it decodes containers"
    )
)]
enum Slot {
    Chapter(usize),
    Picture(usize),
}

/// One chapter of a [`Book`]: a path, Markdown text and the anchors of its
/// headings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chapter {
    path: String,
    text: String,
    anchors: Vec<Anchor>,
}

/// One picture of a [`Book`]: a path and an AVIF file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    path: String,
    size: Size,
    data: Vec<u8>,
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
    /// use mbook_core::TocItem;
    ///
    /// let item = TocItem::new(2, "The station", "ch01.md#the-station");
    /// assert_eq!(item.chapter, "ch01.md");
    /// assert_eq!(item.anchor.as_deref(), Some("the-station"));
    /// ```
    #[must_use]
    pub fn new(level: u8, title: impl Into<String>, href: &str) -> Self {
        let (chapter, anchor) = match href.split_once('#') {
            Some((chapter, anchor)) if !anchor.is_empty() => (chapter, Some(anchor.to_owned())),
            Some((chapter, _)) => (chapter, None),
            None => (href, None),
        };
        Self {
            level,
            title: title.into(),
            chapter: chapter.to_owned(),
            anchor,
        }
    }
}

impl Chapter {
    /// Normalises the text and computes its anchors.
    #[cfg(feature = "write")]
    fn new(path: String, text: String) -> Result<Self> {
        let text = text::normalize(text)?;
        let anchors = analysis::anchors(&text);
        Ok(Self {
            path,
            text,
            anchors,
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

    /// The anchors of the chapter, sorted by offset.
    ///
    /// A chapter loaded from a file keeps the anchors stored in that file
    /// until its text is replaced.
    #[must_use]
    pub fn anchors(&self) -> &[Anchor] {
        &self.anchors
    }

    /// Replaces the text and rebuilds the anchors from its headings.
    ///
    /// CRLF and lone CR become a line feed and a leading byte order mark is
    /// dropped. On error the chapter is unchanged.
    ///
    /// # Errors
    ///
    /// The text contains a control character other than line feed and tab.
    #[cfg(feature = "write")]
    pub fn set_text(&mut self, text: impl Into<String>) -> Result<()> {
        let text = text::normalize(text.into())?;
        self.anchors = analysis::anchors(&text);
        self.text = text;
        Ok(())
    }
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
            Slot::Picture(_) => None,
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
            Slot::Chapter(_) => None,
        }
    }

    /// The table of contents.
    #[must_use]
    pub fn toc(&self) -> &Toc {
        &self.toc
    }
}

/// Loading a book.
#[cfg(feature = "read")]
impl Book {
    /// Loads the book stored in a file.
    ///
    /// # Errors
    ///
    /// As [`Book::read`].
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::read(std::fs::File::open(path)?)
    }

    /// Loads a book from a reader, decoding everything in one pass. The
    /// reader does not have to seek.
    ///
    /// Loading is lenient where the format tells readers to be: a chapter
    /// with a broken link loads, and the problem is reported when the book is
    /// written.
    ///
    /// # Errors
    ///
    /// The source fails, the data is not a valid mbook file, or a default
    /// limit is exceeded.
    pub fn read(reader: impl Read) -> Result<Self> {
        Self::read_with(reader, &ReadOptions::default())
    }

    /// Loads a book from a reader with the given options.
    ///
    /// # Errors
    ///
    /// As [`Book::read`], with the limits of `options`.
    pub fn read_with(_reader: impl Read, _options: &ReadOptions) -> Result<Self> {
        todo!("decode a container sequentially and build the model")
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
    /// let book = mbook_core::Book::new("The Example", "en")?;
    /// assert!(book.meta().id().starts_with("urn:uuid:"));
    /// # Ok::<(), mbook_core::Error>(())
    /// ```
    pub fn new(title: impl Into<String>, language: impl Into<String>) -> Result<Self> {
        let id = format!("urn:uuid:{}", uuid::Uuid::now_v7());
        Ok(Self {
            meta: Meta::new(title.into(), language.into(), id)?,
            chapters: Vec::new(),
            pictures: Vec::new(),
            toc: Toc::default(),
            slots: HashMap::new(),
        })
    }

    /// The metadata, for changing it.
    pub fn meta_mut(&mut self) -> &mut Meta {
        &mut self.meta
    }

    /// The chapter with this path, for changing its text.
    pub fn chapter_mut(&mut self, path: &str) -> Option<&mut Chapter> {
        match self.slots.get(path)? {
            Slot::Chapter(index) => self.chapters.get_mut(*index),
            Slot::Picture(_) => None,
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
        self.renumber_pictures(index);
        Some(picture)
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

    /// Checks the rules that connect the parts of a book, without writing it.
    ///
    /// # Errors
    ///
    /// Every problem found, not only the first: links and pictures that lead
    /// nowhere, raw HTML, a cover that is not in the book, contents items
    /// that do not resolve.
    pub fn check(&self) -> Result<(), Problems> {
        todo!("parse every chapter and resolve its references")
    }

    /// Writes the book with the default options.
    ///
    /// # Errors
    ///
    /// As [`write_with`](Self::write_with).
    pub fn write(&self, writer: impl Write) -> Result<()> {
        self.write_with(writer, &WriteOptions::default())
    }

    /// Writes the book as one mbook file.
    ///
    /// The book is checked first, and nothing is written if the check fails.
    /// Writing the same book twice with the same options gives the same
    /// bytes.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] with every problem [`check`](Self::check) finds;
    /// the sink fails; a limit of `options` is exceeded.
    pub fn write_with(&self, _writer: impl Write, _options: &WriteOptions) -> Result<()> {
        todo!("check the book, build the four tables and write the container")
    }

    /// Writes the book to a file.
    ///
    /// The book is written to a temporary file next to `path`, which is then
    /// renamed into place, so an interrupted save leaves the previous file
    /// intact. `path` may be the file the book was loaded from.
    ///
    /// # Errors
    ///
    /// As [`write`](Self::write).
    pub fn save(&self, _path: impl AsRef<Path>) -> Result<()> {
        todo!("write to a temporary file next to the target and rename it")
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

    /// Updates the slots of the chapters from `from` on after a shift.
    fn renumber_chapters(&mut self, from: usize) {
        for (index, chapter) in self.chapters.iter().enumerate().skip(from) {
            if let Some(slot) = self.slots.get_mut(chapter.path.as_str()) {
                *slot = Slot::Chapter(index);
            }
        }
    }

    /// Updates the slots of the pictures from `from` on after a shift.
    fn renumber_pictures(&mut self, from: usize) {
        for (index, picture) in self.pictures.iter().enumerate().skip(from) {
            if let Some(slot) = self.slots.get_mut(picture.path.as_str()) {
                *slot = Slot::Picture(index);
            }
        }
    }
}

#[cfg(all(test, feature = "write"))]
mod tests {
    use super::{Book, Chapter, Picture, Toc, TocItem};
    use crate::error::{Error, PathError};
    use crate::types::Size;

    /// A chapter built without the Markdown analysis, which is not written
    /// yet.
    fn chapter(path: &str) -> Chapter {
        Chapter {
            path: path.to_owned(),
            text: format!("# {path}\n"),
            anchors: Vec::new(),
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
        chapters && pictures && book.slots.len() == book.chapters.len() + book.pictures.len()
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
