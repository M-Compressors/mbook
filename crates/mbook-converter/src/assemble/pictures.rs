//! The pictures of a draft on their way into the book: each is encoded when
//! a chapter first shows it, and stored once.

use crate::draft::{PictureData, SourcePicture};
use crate::options::ConvertOptions;
use crate::picture;
use mbook::Book;
use std::collections::HashMap;

/// The path of the cover.
const COVER: &str = "img/cover.avif";

/// The pictures of a source, and the paths of those already in the book.
pub(crate) struct Pictures<'a> {
    options: &'a ConvertOptions,
    /// The pictures by the name the source has for them.
    by_name: HashMap<&'a str, &'a PictureData>,
    /// What became of a name: its path, or `None` when it cannot be stored.
    stored: HashMap<&'a str, Option<String>>,
    /// The paths of the files already stored, by their bytes in the source.
    by_content: HashMap<&'a [u8], String>,
    /// How many pictures got a numbered path.
    count: usize,
}

impl<'a> Pictures<'a> {
    pub(crate) fn new(pictures: &'a [SourcePicture], options: &'a ConvertOptions) -> Self {
        let mut by_name = HashMap::new();
        for picture in pictures {
            // The first picture with a name is the one a reference means.
            by_name
                .entry(picture.name.as_str())
                .or_insert(&picture.data);
        }
        Self {
            options,
            by_name,
            stored: HashMap::new(),
            by_content: HashMap::new(),
            count: 0,
        }
    }

    /// Stores the cover. `false` when the source has no such picture or it
    /// cannot be encoded.
    pub(crate) fn store_cover(&mut self, book: &mut Book, name: &str) -> bool {
        self.store(book, name, true).is_some()
    }

    /// The path of a picture in the book, storing it first when this is its
    /// first use. `None` when the source has no such picture or it cannot be
    /// encoded.
    pub(crate) fn path(&mut self, book: &mut Book, name: &str) -> Option<String> {
        self.store(book, name, false)
    }

    fn store(&mut self, book: &mut Book, name: &str, as_cover: bool) -> Option<String> {
        let (&name, &data) = self.by_name.get_key_value(name)?;
        if let Some(known) = self.stored.get(name) {
            return known.clone();
        }
        let content = match data {
            PictureData::Encoded(bytes) => Some(bytes.as_slice()),
            #[cfg(feature = "pdf")]
            PictureData::Rgba { .. } => None,
        };
        let same_file = content.and_then(|bytes| self.by_content.get(bytes).cloned());
        let path = same_file.or_else(|| self.encode(book, data, as_cover));
        if let (Some(bytes), Some(path)) = (content, &path) {
            self.by_content.entry(bytes).or_insert_with(|| path.clone());
        }
        self.stored.insert(name, path.clone());
        path
    }

    /// Encodes a picture and adds it to the book. `mbook` has the last
    /// word on whether the result is a picture it can store.
    fn encode(&mut self, book: &mut Book, data: &PictureData, as_cover: bool) -> Option<String> {
        let avif = picture::to_avif(data, self.options.pictures, &self.options.limits)?;
        let path = if as_cover {
            COVER.to_owned()
        } else {
            format!("img/{:04}.avif", self.count + 1)
        };
        let added = if as_cover {
            book.set_cover(path.as_str(), avif)
        } else {
            book.add_picture(path.as_str(), avif)
        };
        added.ok()?;
        self.count += usize::from(!as_cover);
        Some(path)
    }
}
