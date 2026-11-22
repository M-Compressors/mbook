//! Typed resource numbers handed out by a `BookReader`.

/// Identifies a chapter inside one mbook file.
///
/// It wraps the number of a text resource. There is no public constructor: a
/// `BookReader` hands one out only for a resource that is a chapter, so a
/// picture cannot be read as text.
///
/// An id is a number inside one file. It does not survive re-packing a book;
/// the stable name of a chapter is its path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChapterId(u32);

/// Identifies a picture inside one mbook file.
///
/// It wraps the number of a picture resource. See [`ChapterId`] for how ids
/// are obtained and how long they stay valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PictureId(u32);

/// Identifies a diagram inside one mbook file.
///
/// It wraps the number of a diagram resource. See [`ChapterId`] for how ids
/// are obtained and how long they stay valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DiagramId(u32);

/// Identifies a formula inside one mbook file.
///
/// It wraps the number of a formula resource. See [`ChapterId`] for how ids
/// are obtained and how long they stay valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FormulaId(u32);

impl ChapterId {
    #[cfg(feature = "read")]
    pub(crate) const fn new(number: u32) -> Self {
        Self(number)
    }

    /// The number of the resource in the file.
    ///
    /// Store it to keep an id outside the program, and turn it back into an
    /// id with `BookReader::chapter_id`.
    #[must_use]
    pub const fn number(self) -> u32 {
        self.0
    }
}

impl PictureId {
    #[cfg(feature = "read")]
    pub(crate) const fn new(number: u32) -> Self {
        Self(number)
    }

    /// The number of the resource in the file.
    ///
    /// Turn it back into an id with `BookReader::picture_id`.
    #[must_use]
    pub const fn number(self) -> u32 {
        self.0
    }
}

impl DiagramId {
    #[cfg(feature = "read")]
    pub(crate) const fn new(number: u32) -> Self {
        Self(number)
    }

    /// The number of the resource in the file.
    ///
    /// Turn it back into an id with `BookReader::diagram_id`.
    #[must_use]
    pub const fn number(self) -> u32 {
        self.0
    }
}

impl FormulaId {
    #[cfg(feature = "read")]
    pub(crate) const fn new(number: u32) -> Self {
        Self(number)
    }

    /// The number of the resource in the file.
    ///
    /// Turn it back into an id with `BookReader::formula_id`.
    #[must_use]
    pub const fn number(self) -> u32 {
        self.0
    }
}
