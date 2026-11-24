//! The pictures of a draft on their way into the book: each is stored once,
//! when a chapter first shows it, and gets its path in that order.
//!
//! With the `rayon` feature the pictures the chapters show are encoded
//! beforehand, on all cores at once ([`Pictures::prepare`]); storing then
//! takes the finished file. The paths and the bytes stay the same.

use crate::draft::{PictureData, SourcePicture};
use crate::options::ConvertOptions;
use crate::picture;
use mbook::Book;
use std::collections::HashMap;
#[cfg(parallel_pictures)]
use {
    super::rewrite::Scan, percent_encoding::percent_decode_str, rayon::prelude::*,
    std::collections::HashSet, std::ops::Range,
};

/// The path of the cover.
const COVER: &str = "img/cover.avif";

/// How many pixels the pictures encoded at the same time may hold together.
/// A picture with more is encoded alone, as it would be without `rayon`.
#[cfg(parallel_pictures)]
const PARALLEL_PIXELS: u64 = 32_000_000;

/// A picture, as far as encoding is concerned: two names for the same bytes
/// are one picture.
#[cfg(parallel_pictures)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Key<'a> {
    /// A file of the source, by its bytes.
    Content(&'a [u8]),
    /// Pixels a front end rendered, by their name.
    #[cfg(feature = "pdf")]
    Name(&'a str),
}

#[cfg(parallel_pictures)]
impl<'a> Key<'a> {
    #[cfg_attr(
        not(feature = "pdf"),
        expect(unused_variables, reason = "only rendered pixels are known by name")
    )]
    fn of(name: &'a str, data: &'a PictureData) -> Self {
        match data {
            PictureData::Encoded(bytes) => Self::Content(bytes),
            #[cfg(feature = "pdf")]
            PictureData::Rgba { .. } => Self::Name(name),
        }
    }
}

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
    /// The AVIF files encoded ahead of their first use; `None` for a picture
    /// that cannot be encoded.
    #[cfg(parallel_pictures)]
    ready: HashMap<Key<'a>, Option<Vec<u8>>>,
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
            #[cfg(parallel_pictures)]
            ready: HashMap::new(),
        }
    }

    /// Encodes the cover and the pictures the chapters show on all cores, so
    /// that storing them later takes no time. A destination is decoded as the
    /// chapters decode it when they ask for its path.
    #[cfg(parallel_pictures)]
    pub(crate) fn prepare_shown(&mut self, cover: Option<&str>, scans: &[Scan]) {
        let shown: Vec<_> = scans
            .iter()
            .flat_map(|scan| &scan.pictures)
            .filter_map(|destination| percent_decode_str(destination).decode_utf8().ok())
            .collect();
        self.prepare(cover.into_iter().chain(shown.iter().map(AsRef::as_ref)));
    }

    /// Encodes the pictures with these names on all cores, so that storing
    /// them later takes no time. A name the source has no picture for, a
    /// picture stored or encoded already and a second name for the same
    /// bytes are passed over. The pictures go in runs whose pixels together
    /// stay within [`PARALLEL_PIXELS`].
    #[cfg(parallel_pictures)]
    pub(crate) fn prepare<'n>(&mut self, names: impl IntoIterator<Item = &'n str>) {
        let mut seen = HashSet::new();
        let mut work = Vec::new();
        for name in names {
            let Some((&name, &data)) = self.by_name.get_key_value(name) else {
                continue;
            };
            let key = Key::of(name, data);
            if !self.stored.contains_key(name) && !self.ready.contains_key(&key) && seen.insert(key)
            {
                work.push((key, data));
            }
        }
        let pixels: Vec<u64> = work.iter().map(|(_, data)| picture::pixels(data)).collect();
        let options = self.options;
        for run in runs(&pixels, PARALLEL_PIXELS) {
            let encoded: Vec<_> = work[run]
                .par_iter()
                .map(|&(key, data)| {
                    let avif = picture::to_avif(data, options.pictures, &options.limits);
                    (key, avif)
                })
                .collect();
            self.ready.extend(encoded);
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
        let path = same_file.or_else(|| self.encode(book, name, data, as_cover));
        if let (Some(bytes), Some(path)) = (content, &path) {
            self.by_content.entry(bytes).or_insert_with(|| path.clone());
        }
        self.stored.insert(name, path.clone());
        path
    }

    /// The AVIF file of a picture: the one prepared for it, or else one
    /// encoded now.
    #[cfg_attr(
        not(parallel_pictures),
        expect(unused_variables, reason = "the name finds a prepared picture")
    )]
    fn avif(&mut self, name: &'a str, data: &'a PictureData) -> Option<Vec<u8>> {
        #[cfg(parallel_pictures)]
        if let Some(prepared) = self.ready.remove(&Key::of(name, data)) {
            return prepared;
        }
        picture::to_avif(data, self.options.pictures, &self.options.limits)
    }

    /// Encodes a picture and adds it to the book. `mbook` has the last
    /// word on whether the result is a picture it can store.
    fn encode(
        &mut self,
        book: &mut Book,
        name: &'a str,
        data: &'a PictureData,
        as_cover: bool,
    ) -> Option<String> {
        let avif = self.avif(name, data)?;
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

/// Splits pictures, in their order, into runs whose pixels together are
/// within the budget. A picture above the budget is a run of its own.
#[cfg(parallel_pictures)]
fn runs(pixels: &[u64], budget: u64) -> Vec<Range<usize>> {
    let mut runs = Vec::new();
    let (mut start, mut sum) = (0, 0_u64);
    for (index, &count) in pixels.iter().enumerate() {
        if index > start && sum.saturating_add(count) > budget {
            runs.push(start..index);
            (start, sum) = (index, 0);
        }
        sum = sum.saturating_add(count);
    }
    if start < pixels.len() {
        runs.push(start..pixels.len());
    }
    runs
}

#[cfg(test)]
#[cfg(parallel_pictures)]
mod tests {
    use super::{Key, Pictures, runs};
    use crate::draft::{PictureData, SourcePicture};
    use crate::options::ConvertOptions;
    use crate::picture;
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    use mbook::Book;
    use std::io::Cursor;

    /// A PNG file of one colour.
    fn png(shade: u8) -> Vec<u8> {
        let picture = RgbImage::from_pixel(12, 8, Rgb([shade, 40, 200]));
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(picture)
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    fn sources(pictures: &[(&str, Vec<u8>)]) -> Vec<SourcePicture> {
        pictures
            .iter()
            .map(|(name, bytes)| SourcePicture {
                name: (*name).to_owned(),
                data: PictureData::Encoded(bytes.clone()),
            })
            .collect()
    }

    /// The first and the end of each run.
    fn bounds(pixels: &[u64], budget: u64) -> Vec<(usize, usize)> {
        runs(pixels, budget)
            .into_iter()
            .map(|run| (run.start, run.end))
            .collect()
    }

    #[test]
    fn no_pictures_make_no_runs() {
        assert_eq!(bounds(&[], 10), Vec::<(usize, usize)>::new());
    }

    #[test]
    fn pictures_within_the_budget_make_one_run() {
        assert_eq!(bounds(&[3, 3, 4], 10), [(0, 3)]);
    }

    #[test]
    fn a_run_ends_before_the_picture_that_would_exceed_the_budget() {
        assert_eq!(bounds(&[4, 4, 4, 4, 4], 10), [(0, 2), (2, 4), (4, 5)]);
    }

    #[test]
    fn a_picture_above_the_budget_is_a_run_of_its_own() {
        assert_eq!(bounds(&[2, 50, 2], 10), [(0, 1), (1, 2), (2, 3)]);
        assert_eq!(bounds(&[50], 10), [(0, 1)]);
    }

    #[test]
    fn pictures_without_pixels_join_any_run() {
        assert_eq!(bounds(&[10, 0, 0, 1], 10), [(0, 3), (3, 4)]);
    }

    #[test]
    fn counts_near_the_largest_number_do_not_overflow() {
        assert_eq!(
            bounds(&[u64::MAX, u64::MAX], u64::MAX - 1),
            [(0, 1), (1, 2)]
        );
    }

    #[test]
    fn only_named_pictures_of_the_source_are_prepared_once_per_content() {
        let pictures = sources(&[("a.png", png(1)), ("same.png", png(1)), ("b.png", png(2))]);
        let options = ConvertOptions::default();
        let mut store = Pictures::new(&pictures, &options);
        store.prepare(["a.png", "same.png", "a.png", "missing.png"]);
        assert_eq!(store.ready.len(), 1);
        assert!(store.ready.contains_key(&Key::Content(&png(1))));
    }

    #[test]
    fn a_prepared_picture_is_the_one_encoded_alone() {
        let pictures = sources(&[("a.png", png(1)), ("b.png", png(2)), ("c.png", png(3))]);
        let options = ConvertOptions::default();
        let mut store = Pictures::new(&pictures, &options);
        store.prepare(["a.png", "b.png", "c.png"]);
        for source in &pictures {
            let alone = picture::to_avif(&source.data, options.pictures, &options.limits);
            assert_eq!(store.ready[&Key::of(&source.name, &source.data)], alone);
        }
    }

    #[test]
    fn storing_takes_the_prepared_picture() {
        let pictures = sources(&[("a.png", png(1))]);
        let options = ConvertOptions::default();
        let mut store = Pictures::new(&pictures, &options);
        store.prepare(["a.png"]);
        let mut book = Book::new("T", "en").unwrap();
        assert_eq!(
            store.path(&mut book, "a.png").as_deref(),
            Some("img/0001.avif")
        );
        assert!(store.ready.is_empty());
        assert_eq!(book.pictures().len(), 1);
    }

    #[test]
    fn a_picture_that_was_not_prepared_is_encoded_when_it_is_stored() {
        let pictures = sources(&[("a.png", png(1)), ("b.png", png(2))]);
        let options = ConvertOptions::default();
        let mut store = Pictures::new(&pictures, &options);
        store.prepare(["b.png"]);
        let mut book = Book::new("T", "en").unwrap();
        assert_eq!(
            store.path(&mut book, "a.png").as_deref(),
            Some("img/0001.avif")
        );
        assert_eq!(
            store.path(&mut book, "b.png").as_deref(),
            Some("img/0002.avif")
        );
        assert_eq!(book.pictures().len(), 2);
    }

    #[test]
    fn a_stored_picture_is_not_prepared_again() {
        let pictures = sources(&[("a.png", png(1))]);
        let options = ConvertOptions::default();
        let mut store = Pictures::new(&pictures, &options);
        let mut book = Book::new("T", "en").unwrap();
        store.path(&mut book, "a.png");
        store.prepare(["a.png"]);
        assert!(store.ready.is_empty());
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn rendered_pixels_are_prepared_by_their_name() {
        let page = |name: &str| SourcePicture {
            name: name.to_owned(),
            data: PictureData::Rgba {
                width: 2,
                height: 2,
                pixels: vec![255; 16],
            },
        };
        let pictures = vec![page("p1"), page("p2")];
        let options = ConvertOptions::default();
        let mut store = Pictures::new(&pictures, &options);
        store.prepare(["p1", "p2"]);
        assert_eq!(store.ready.len(), 2);
        assert!(store.ready.contains_key(&Key::Name("p2")));
        let mut book = Book::new("T", "en").unwrap();
        assert_eq!(
            store.path(&mut book, "p2").as_deref(),
            Some("img/0001.avif")
        );
        assert_eq!(store.ready.len(), 1);
    }

    #[test]
    fn a_picture_that_cannot_be_encoded_is_prepared_as_nothing() {
        let pictures = sources(&[("a.svg", b"<svg/>".to_vec())]);
        let options = ConvertOptions::default();
        let mut store = Pictures::new(&pictures, &options);
        store.prepare(["a.svg"]);
        assert_eq!(store.ready.get(&Key::Content(b"<svg/>")), Some(&None));
        let mut book = Book::new("T", "en").unwrap();
        assert_eq!(store.path(&mut book, "a.svg"), None);
    }
}
