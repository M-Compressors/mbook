//! The libraries the screens are drawn over: books written to a temporary
//! root, with covers of several proportions.

use std::fs;
use std::io;
use std::path::Path;

use image::{ImageFormat, Rgb, RgbImage};
use tempfile::TempDir;

use crate::entities::catalogue::{BookRow, Catalogue, ReadingState};
use crate::entities::picture::{EncodeOptions, encode_avif};
use crate::error::{Error, Result};
use crate::features::import_book::refresh_thumbnail;
use crate::shared::model::{BookKey, Place};
use crate::shared::roots::Roots;

/// How far a book has been read.
#[derive(Debug, Clone, Copy)]
enum Progress {
    /// Never opened.
    Unopened,
    /// Opened, with this many thousandths behind the place.
    Reading(u16),
    /// Marked as finished.
    Finished,
}

/// One book of a library.
struct Spec {
    title: &'static str,
    authors: &'static [&'static str],
    language: &'static str,
    /// The size of the cover in pixels, and its colour as a hue in degrees.
    cover: Option<(u32, u32, f32)>,
    progress: Progress,
}

/// The books of the library with books in it. The first one has the text the
/// reader and the editor are shown over.
const BOOKS: [Spec; 9] = [
    Spec {
        title: "The Quiet Orchard",
        authors: &["Ann Ledger"],
        language: "en",
        cover: Some((120, 180, 150.0)),
        progress: Progress::Reading(380),
    },
    Spec {
        title: "Война и мир",
        authors: &["Лев Толстой"],
        language: "ru",
        cover: Some((120, 180, 20.0)),
        progress: Progress::Finished,
    },
    Spec {
        title: "A Very Long Title That Keeps Going: An Illustrated Guide to the Birds, Beasts and Other Curiosities of the Northern Marshlands, Volume Two",
        authors: &["Eleanor Whitcombe-Hargreaves"],
        language: "en",
        cover: None,
        progress: Progress::Unopened,
    },
    Spec {
        title: "Dune",
        authors: &["Frank Herbert"],
        language: "en",
        cover: Some((140, 140, 40.0)),
        progress: Progress::Unopened,
    },
    Spec {
        title: "Зима в Простоквашино",
        authors: &["Эдуард Успенский"],
        language: "ru",
        cover: None,
        progress: Progress::Reading(620),
    },
    Spec {
        title: "Ink",
        authors: &["Mo"],
        language: "en",
        cover: None,
        progress: Progress::Unopened,
    },
    Spec {
        title: "Pocket Atlas",
        authors: &["Ruth Carver", "Idris Bell"],
        language: "en",
        cover: Some((180, 120, 210.0)),
        progress: Progress::Unopened,
    },
    Spec {
        title: "Тонкая книга о длинных названиях и коротких авторах",
        authors: &["И. Н. Иванов"],
        language: "ru",
        cover: Some((80, 200, 280.0)),
        progress: Progress::Reading(120),
    },
    Spec {
        title: "Moby-Dick; or, The Whale",
        authors: &["Herman Melville"],
        language: "en",
        cover: None,
        progress: Progress::Finished,
    },
];

/// The text of the first chapter of the book that is read.
const CHAPTER_ONE: &str = "# The gate\n\nThe orchard lay behind a low stone wall, and nobody remembered who had built it. \
The gate was never locked; it only stuck, and every visitor learned the trick of lifting it by its latch.\n\n\
Мы пришли в сад ранним утром, когда трава ещё была мокрой от росы, и первое, что бросилось в глаза, \
была старая яблоня с табличкой на стволе.\n";

/// The text of the chapter the reader opens in the middle of.
const CHAPTER_TWO: &str = "# The harvest\n\n\
The apples came late that year. Ann counted the crates twice, and then a third time, because the number \
would not agree with the ledger, and a ledger in that family was a thing one argued with only in private.\n\n\
Яблоки в тот год поспели поздно. Анна дважды пересчитала ящики, а потом в третий раз, потому что число \
не сходилось с записями, а с записями в этой семье спорили только наедине.\n\n\
## What was picked\n\n\
The pickers brought in, in order of ripeness:\n\n\
- Early russets, small and rough, for the cider press\n\
- Cox's pippins, which kept best through the winter\n\
- Антоновка, sour enough to make a visitor wince\n\
- A single crate of something nobody could name\n\n\
> An orchard is a slow argument between a family and its weather. \
Neither side is ever quite right, and both go on arguing for generations.\n\n\
## The ledger\n\n\
| Variety | Crates | Weight |\n| --- | --- | --- |\n| Russet | 14 | 310 kg |\n| Cox's pippin | 22 | 480 kg |\n| Антоновка | 9 | 200 kg |\n\n\
![The orchard at dusk, seen from the gate](img/orchard.avif)\n\n\
A longer paragraph follows the figure, so that the text goes on below it: the light had gone amber by the time the last \
crate was carried in, and the wall threw a long shadow across the grass. \
Read the [next chapter](ch03.md) or the [notes of the orchard society](https://example.org/orchard) for more.\n\n\
Ещё один абзац после рисунка, чтобы текст продолжался и ниже: свет стал янтарным, когда внесли последний ящик, \
и стена бросила длинную тень на траву.\n\n\
## After dark\n\n\
The kitchen smelled of cloves. Nobody spoke of the count again until the morning.\n";

/// The text of the last chapter.
const CHAPTER_THREE: &str = "# Notes\n\nThe notes are kept in the cellar, with the cider.\n";

/// A picture of `width` by `height` pixels with a gradient of one hue and a
/// few shapes, so that scaling and cropping show.
fn painting(width: u32, height: u32, hue: f32) -> RgbImage {
    let top = hsl(hue, 0.55, 0.38);
    let bottom = hsl(hue + 30.0, 0.65, 0.68);
    let ink = hsl(hue + 180.0, 0.45, 0.92);
    let (w, h) = (f64::from(width), f64::from(height));
    RgbImage::from_fn(width, height, |x, y| {
        let t = f64::from(y) / h;
        let mix = |a: u8, b: u8| {
            // Both channels are 0..=255 and `t` is within 0..=1.
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the mix of two bytes is a byte"
            )]
            let value = (f64::from(a) * (1.0 - t) + f64::from(b) * t) as u8;
            value
        };
        let cx = f64::from(x) - w / 2.0;
        let cy = f64::from(y) - h * 0.42;
        let in_disc = (cx * cx + cy * cy).sqrt() < w.min(h) * 0.22;
        let in_band = f64::from(y) > h * 0.78 && f64::from(y) < h * 0.82;
        if in_disc || in_band {
            ink
        } else {
            Rgb([
                mix(top.0[0], bottom.0[0]),
                mix(top.0[1], bottom.0[1]),
                mix(top.0[2], bottom.0[2]),
            ])
        }
    })
}

/// A colour from hue (degrees), saturation and lightness.
fn hsl(hue: f32, saturation: f32, lightness: f32) -> Rgb<u8> {
    let hue = hue.rem_euclid(360.0);
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let second = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let (red, green, blue) = match hue {
        h if h < 60.0 => (chroma, second, 0.0),
        h if h < 120.0 => (second, chroma, 0.0),
        h if h < 180.0 => (0.0, chroma, second),
        h if h < 240.0 => (0.0, second, chroma),
        h if h < 300.0 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let shift = lightness - chroma / 2.0;
    let byte = |value: f32| {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a channel of a colour is between 0 and 1 before it is scaled to a byte"
        )]
        let byte = ((value + shift).clamp(0.0, 1.0) * 255.0) as u8;
        byte
    };
    Rgb([byte(red), byte(green), byte(blue)])
}

/// The AVIF bytes of a picture, made small and fast.
fn avif(width: u32, height: u32, hue: f32) -> Result<Vec<u8>> {
    let mut png = io::Cursor::new(Vec::new());
    painting(width, height, hue)
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|error| Error::Render(error.to_string()))?;
    let options = EncodeOptions {
        quality: 55,
        speed: 10,
        ..EncodeOptions::default()
    };
    encode_avif(png.get_ref(), options).map_err(|error| Error::Render(error.to_string()))
}

/// A library written to a temporary folder.
pub(super) struct World {
    /// Keeps the folder until the world is dropped.
    _dir: TempDir,
    roots: Roots,
    /// The books, in the order they were added.
    keys: Vec<BookKey>,
}

impl World {
    /// A library without a book.
    pub(super) fn empty() -> Result<Self> {
        Self::build(&[])
    }

    /// A library with nine books.
    pub(super) fn with_books() -> Result<Self> {
        Self::build(&BOOKS)
    }

    fn build(specs: &[Spec]) -> Result<Self> {
        let dir = tempfile::tempdir()?;
        let roots = Roots::new(dir.path().join("data"), dir.path().join("cache"));
        roots.prepare()?;
        let catalogue = Catalogue::open(&roots.catalogue())
            .map_err(|error| Error::Render(error.to_string()))?;
        let mut keys = Vec::new();
        for (at, spec) in specs.iter().enumerate() {
            keys.push(write_one(&roots, &catalogue, spec, at)?);
        }
        Ok(Self {
            _dir: dir,
            roots,
            keys,
        })
    }

    /// The books, in the order they were added.
    pub(super) fn keys(&self) -> &[BookKey] {
        &self.keys
    }

    /// A copy of the library in a folder of its own, which a scene may change.
    pub(super) fn copy(&self) -> Result<(TempDir, Roots)> {
        let dir = tempfile::tempdir()?;
        let data = dir.path().join("data");
        let cache = dir.path().join("cache");
        copy_tree(self.roots.data(), &data)?;
        copy_tree(self.roots.cache(), &cache)?;
        Ok((dir, Roots::new(data, cache)))
    }
}

/// Writes one book, records it in the catalogue and makes its thumbnail.
fn write_one(roots: &Roots, catalogue: &Catalogue, spec: &Spec, at: usize) -> Result<BookKey> {
    let key = BookKey::generate();
    let mut book = mbook::Book::new(spec.title, spec.language)?;
    for author in spec.authors {
        book.meta_mut().add_author(*author)?;
    }
    let cover = match spec.cover {
        Some((width, height, hue)) => {
            let bytes = avif(width, height, hue)?;
            book.set_cover("cover.avif", bytes.clone())?;
            Some(bytes)
        }
        None => None,
    };
    if at == 0 {
        book.add_picture("img/orchard.avif", avif(320, 200, 195.0)?)?;
        book.add_chapter("ch01.md", CHAPTER_ONE)?;
        book.add_chapter("ch02.md", CHAPTER_TWO)?;
        book.add_chapter("ch03.md", CHAPTER_THREE)?;
    } else {
        book.add_chapter("ch01.md", "# One\n\nA short text.\n")?;
    }
    let path = roots.book(key);
    book.save(&path)?;
    let file_len = fs::metadata(&path)?.len();
    let reader = mbook::BookReader::open(&path)?;
    let added = 1_700_000_000_000 + u64::try_from(at).unwrap_or(0) * 86_400_000;
    let row = BookRow::from_reader(key, &reader, file_len, added, None);
    drop(reader);
    catalogue
        .put_book(&row)
        .map_err(|error| Error::Render(error.to_string()))?;
    refresh_thumbnail(roots, key, cover.as_deref());
    let opened = Some(added + 3_600_000);
    let state = match spec.progress {
        Progress::Unopened => ReadingState::default(),
        Progress::Reading(permille) => ReadingState {
            place: Some(Place {
                chapter: "ch01.md".to_owned(),
                offset: if at == 0 { place_in_chapter_two() } else { 0 },
            }),
            progress_permille: permille,
            opened_ms: opened,
            finished: false,
        },
        Progress::Finished => ReadingState {
            place: Some(Place {
                chapter: "ch01.md".to_owned(),
                offset: 0,
            }),
            progress_permille: 1000,
            opened_ms: opened,
            finished: true,
        },
    };
    let state = if at == 0 {
        // The place is in the second chapter, where the text has everything.
        ReadingState {
            place: state.place.map(|place| Place {
                chapter: "ch02.md".to_owned(),
                ..place
            }),
            ..state
        }
    } else {
        state
    };
    if state != ReadingState::default() {
        catalogue
            .put_state(key, &state)
            .map_err(|error| Error::Render(error.to_string()))?;
    }
    Ok(key)
}

/// Where the reader is put in the second chapter: at the heading of the table.
fn place_in_chapter_two() -> u32 {
    CHAPTER_TWO
        .find("## What was picked")
        .and_then(|at| u32::try_from(at).ok())
        .unwrap_or(0)
}

/// Copies a folder and what is in it.
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
