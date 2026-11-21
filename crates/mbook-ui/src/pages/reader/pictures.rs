//! The pictures of the chapter on the screen: who is asked for, who is
//! decoded, who is kept.
//!
//! The view of the text asks for a picture each time it draws it, from the
//! main thread, through [`Pictures::resolve`]; the first ask puts the picture
//! in a queue and wakes the page, which decodes it on the background and
//! gives the result back with [`Pictures::finish`]. Decoded pictures are
//! kept in a cache bounded by their bytes; a slot holds no image itself, so
//! the bound is the memory the pictures take.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use futures::channel::mpsc::UnboundedSender;
use gpui_kit::RenderImage;
use std::sync::Arc;

use crate::entities::book::chapter::PictureRef;
use crate::entities::book::error::BookError;
use crate::entities::book::source::BookSource;
use crate::entities::picture::{PictureCache, PictureError, decode_avif};
use crate::shared::layout::SizeClass;
use crate::widgets::chapter_view::{PictureState, device_width};

/// The most pixels a picture may have. A picture with more is not decoded.
const MAX_PIXELS: u64 = 40_000_000;
/// The bytes of decoded pictures kept on an expanded window.
const EXPANDED_BUDGET: usize = 256 * 1024 * 1024;
/// The bytes of decoded pictures kept on a smaller window.
const SMALL_BUDGET: usize = 64 * 1024 * 1024;
/// How many pictures are decoded at the same time.
pub(super) const CONCURRENCY: usize = 2;

/// The bytes the cache keeps for a window of this class.
pub(super) fn budget(class: SizeClass) -> usize {
    match class {
        SizeClass::Expanded => EXPANDED_BUDGET,
        SizeClass::Compact | SizeClass::Medium => SMALL_BUDGET,
    }
}

/// Why a picture could not be made ready.
#[derive(Debug, thiserror::Error)]
pub(super) enum LoadError {
    /// The file of the book did not give the bytes.
    #[error("the picture could not be read from the book")]
    Read(#[source] BookError),
    /// The bytes are not a picture that can be shown.
    #[error("the picture could not be decoded")]
    Decode(#[source] PictureError),
}

/// What a job gives back: the image and the bytes it takes.
pub(super) type Loaded = (Arc<RenderImage>, usize);

/// Reads, decodes and scales a picture to `width` device pixels. Runs on the
/// background.
pub(super) fn load(
    source: &BookSource,
    picture: PictureRef,
    width: u32,
) -> Result<Loaded, LoadError> {
    let bytes = source.read_picture(picture).map_err(LoadError::Read)?;
    let bitmap = decode_avif(&bytes, MAX_PIXELS)
        .map_err(LoadError::Decode)?
        .fit_width(width);
    let bytes = bitmap.byte_len();
    Ok((bitmap.into_render_image(), bytes))
}

/// A picture to load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Job {
    /// The destination as written in the text.
    pub(super) destination: String,
    pub(super) picture: PictureRef,
    /// The width the picture is made for, in device pixels.
    pub(super) width: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Nothing asked for.
    Idle,
    /// In the queue.
    Queued,
    /// Being decoded.
    Loading,
    /// In the cache, made for this width.
    Loaded(u32),
    /// It cannot be shown.
    Failed,
}

struct Slot {
    picture: PictureRef,
    phase: Phase,
}

struct Inner {
    slots: HashMap<String, Slot>,
    cache: PictureCache<(PictureRef, u32), RenderImage>,
    queue: VecDeque<Job>,
}

/// The pictures of one chapter, shared between the page and the view of the
/// text.
pub(super) struct Pictures {
    inner: Mutex<Inner>,
    wake: UnboundedSender<()>,
}

impl Pictures {
    /// An empty table whose cache holds `budget` bytes. `wake` is told when
    /// a picture is queued.
    pub(super) fn new(budget: usize, wake: UnboundedSender<()>) -> Self {
        Self {
            inner: Mutex::new(Inner {
                slots: HashMap::new(),
                cache: PictureCache::new(budget),
                queue: VecDeque::new(),
            }),
            wake,
        }
    }

    /// Starts a chapter: forgets what was asked and knows the pictures of the
    /// new text. The cache stays, so a picture seen again is not decoded
    /// again.
    pub(super) fn reset(&self, pictures: &HashMap<String, PictureRef>) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        inner.queue.clear();
        inner.slots = pictures
            .iter()
            .map(|(destination, picture)| {
                (
                    destination.clone(),
                    Slot {
                        picture: *picture,
                        phase: Phase::Idle,
                    },
                )
            })
            .collect();
    }

    /// Changes how many bytes the cache may hold.
    pub(super) fn set_budget(&self, bytes: usize) {
        if let Ok(mut inner) = self.inner.lock() {
            let mut cache = PictureCache::new(bytes);
            std::mem::swap(&mut inner.cache, &mut cache);
        }
    }

    /// What the view draws for `destination` in a column of `column` logical
    /// pixels on a window with the scale factor `scale`. The first ask for a
    /// picture queues it.
    pub(super) fn resolve(
        &self,
        destination: &str,
        column: f32,
        scale: f32,
    ) -> Option<PictureState> {
        let mut inner = self.inner.lock().ok()?;
        let Inner {
            slots,
            cache,
            queue,
        } = &mut *inner;
        let slot = slots.get_mut(destination)?;
        let known = (slot.picture.width, slot.picture.height);
        let width = device_width(column, scale);
        let wanted = (slot.picture, width);
        let shown = match slot.phase {
            Phase::Failed => return Some(PictureState::Failed),
            Phase::Queued | Phase::Loading => return Some(PictureState::Pending(known)),
            Phase::Loaded(made) => cache.get(&(slot.picture, made)),
            Phase::Idle => cache.get(&wanted),
        };
        if let Some(image) = shown {
            if slot.phase == Phase::Idle {
                slot.phase = Phase::Loaded(width);
            }
            return Some(PictureState::Ready(image));
        }
        // Never loaded, or pushed out of the cache.
        slot.phase = Phase::Queued;
        queue.push_back(Job {
            destination: destination.to_owned(),
            picture: slot.picture,
            width,
        });
        // The page ends with its task; a failed send then means nobody is
        // left to load for.
        self.wake.unbounded_send(()).ok();
        Some(PictureState::Pending(known))
    }

    /// The next picture to load, now marked as being loaded.
    pub(super) fn next_job(&self) -> Option<Job> {
        let mut inner = self.inner.lock().ok()?;
        while let Some(job) = inner.queue.pop_front() {
            if let Some(slot) = inner.slots.get_mut(&job.destination)
                && slot.picture == job.picture
                && slot.phase == Phase::Queued
            {
                slot.phase = Phase::Loading;
                return Some(job);
            }
        }
        None
    }

    /// Takes the result of a job. A result for a picture that is no longer
    /// asked for is dropped. Returns whether the view has anything new to
    /// draw.
    pub(super) fn finish(&self, job: &Job, result: Result<Loaded, LoadError>) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };
        let Inner { slots, cache, .. } = &mut *inner;
        let Some(slot) = slots.get_mut(&job.destination) else {
            return false;
        };
        if slot.picture != job.picture || slot.phase != Phase::Loading {
            return false;
        }
        match result {
            Ok((image, bytes)) => {
                cache.insert((job.picture, job.width), image, bytes);
                slot.phase = Phase::Loaded(job.width);
            }
            Err(_) => slot.phase = Phase::Failed,
        }
        true
    }

    /// The bytes the cache holds.
    #[cfg(test)]
    pub(super) fn cached_bytes(&self) -> usize {
        self.inner.lock().map_or(0, |inner| inner.cache.bytes())
    }
}

#[cfg(test)]
mod tests {
    use futures::channel::mpsc::{UnboundedReceiver, unbounded};

    use super::*;
    use crate::entities::book::source::open;
    use crate::pages::reader::support::{FIGURE, TestResult, write_book};

    /// A table over a chapter that has one picture, with the real reference
    /// the book makes of it.
    struct Sample {
        pictures: Pictures,
        wake: UnboundedReceiver<()>,
        known: HashMap<String, PictureRef>,
        source: BookSource,
        dir: tempfile::TempDir,
    }

    fn sample(budget: usize) -> TestResult<Sample> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("b.mbook");
        write_book(
            &path,
            "T",
            "en",
            &[("a.md", "![fig](img/fig.avif)\n")],
            Some("rotated.avif"),
        )?;
        let (_, source) = open(&path)?;
        let known = source.prepare_chapter(0)?.pictures;
        let (wake_sender, wake) = unbounded();
        let pictures = Pictures::new(budget, wake_sender);
        pictures.reset(&known);
        Ok(Sample {
            pictures,
            wake,
            known,
            source,
            dir,
        })
    }

    fn image() -> Loaded {
        (Arc::new(RenderImage::new(Vec::new())), 100)
    }

    #[test]
    fn the_budget_is_larger_on_an_expanded_window() {
        assert_eq!(budget(SizeClass::Expanded), 256 * 1024 * 1024);
        assert_eq!(budget(SizeClass::Medium), 64 * 1024 * 1024);
        assert_eq!(budget(SizeClass::Compact), 64 * 1024 * 1024);
    }

    #[test]
    fn a_destination_that_is_no_picture_of_the_book_is_unknown() -> TestResult {
        let s = sample(10_000)?;
        assert!(
            s.pictures
                .resolve("https://example.org/x.png", 300.0, 1.0)
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn the_first_ask_queues_the_picture_and_says_how_big_it_will_be() -> TestResult {
        let mut s = sample(10_000)?;
        let size = (s.known[FIGURE].width, s.known[FIGURE].height);
        let state = s.pictures.resolve(FIGURE, 300.0, 2.0);
        assert!(matches!(state, Some(PictureState::Pending(known)) if known == size));
        assert!(s.wake.try_recv().is_ok(), "the page is woken");
        let job = s.pictures.next_job().ok_or("no job")?;
        assert_eq!(job.destination, FIGURE);
        assert_eq!(job.width, 600);
        Ok(())
    }

    #[test]
    fn asking_again_does_not_queue_it_twice() -> TestResult {
        let s = sample(10_000)?;
        s.pictures.resolve(FIGURE, 300.0, 1.0);
        s.pictures.resolve(FIGURE, 300.0, 1.0);
        assert!(s.pictures.next_job().is_some());
        assert!(s.pictures.next_job().is_none());
        Ok(())
    }

    #[test]
    fn a_loaded_picture_is_ready_and_is_not_queued_again() -> TestResult {
        let s = sample(10_000)?;
        s.pictures.resolve(FIGURE, 300.0, 1.0);
        let job = s.pictures.next_job().ok_or("no job")?;
        assert!(s.pictures.finish(&job, Ok(image())));
        assert!(matches!(
            s.pictures.resolve(FIGURE, 300.0, 1.0),
            Some(PictureState::Ready(_))
        ));
        assert!(s.pictures.next_job().is_none());
        assert_eq!(s.pictures.cached_bytes(), 100);
        Ok(())
    }

    #[test]
    fn a_picture_that_cannot_be_decoded_is_failed_for_good() -> TestResult {
        let s = sample(10_000)?;
        s.pictures.resolve(FIGURE, 300.0, 1.0);
        let job = s.pictures.next_job().ok_or("no job")?;
        let error = LoadError::Decode(PictureError::UnsupportedFormat);
        assert!(s.pictures.finish(&job, Err(error)));
        assert!(matches!(
            s.pictures.resolve(FIGURE, 300.0, 1.0),
            Some(PictureState::Failed)
        ));
        assert!(s.pictures.next_job().is_none());
        Ok(())
    }

    #[test]
    fn a_result_after_the_chapter_changed_is_dropped() -> TestResult {
        let s = sample(10_000)?;
        s.pictures.resolve(FIGURE, 300.0, 1.0);
        let job = s.pictures.next_job().ok_or("no job")?;
        s.pictures.reset(&s.known);
        assert!(!s.pictures.finish(&job, Ok(image())));
        assert_eq!(s.pictures.cached_bytes(), 0);
        Ok(())
    }

    #[test]
    fn a_picture_pushed_out_of_the_cache_is_asked_for_again() -> TestResult {
        let s = sample(10_000)?;
        s.pictures.resolve(FIGURE, 300.0, 1.0);
        let job = s.pictures.next_job().ok_or("no job")?;
        s.pictures.finish(&job, Ok(image()));
        s.pictures.set_budget(10);
        assert!(matches!(
            s.pictures.resolve(FIGURE, 300.0, 1.0),
            Some(PictureState::Pending(_))
        ));
        assert!(s.pictures.next_job().is_some());
        Ok(())
    }

    #[test]
    fn a_picture_kept_in_the_cache_is_ready_in_the_next_chapter_without_a_job() -> TestResult {
        let s = sample(10_000)?;
        s.pictures.resolve(FIGURE, 300.0, 1.0);
        let job = s.pictures.next_job().ok_or("no job")?;
        s.pictures.finish(&job, Ok(image()));
        s.pictures.reset(&s.known);
        assert!(matches!(
            s.pictures.resolve(FIGURE, 300.0, 1.0),
            Some(PictureState::Ready(_))
        ));
        Ok(())
    }

    #[test]
    fn a_job_for_a_slot_that_is_gone_is_skipped() -> TestResult {
        let s = sample(10_000)?;
        s.pictures.resolve(FIGURE, 300.0, 1.0);
        s.pictures.reset(&HashMap::new());
        assert!(s.pictures.next_job().is_none());
        Ok(())
    }

    #[test]
    fn a_picture_of_the_book_is_read_decoded_and_scaled_to_the_width() -> TestResult {
        let s = sample(10_000)?;
        let picture = s.known[FIGURE];
        let (image, bytes) = load(&s.source, picture, 4)?;
        assert!(bytes > 0 && bytes <= 4 * picture.height as usize * 4 * 4);
        assert!(image.size(0).width.0 <= 4);
        Ok(())
    }

    #[test]
    fn a_picture_the_book_does_not_have_fails_to_load() -> TestResult {
        let s = sample(10_000)?;
        let picture = s.known[FIGURE];
        let other = write_other(&s)?;
        assert!(matches!(load(&other, picture, 4), Err(LoadError::Read(_))));
        Ok(())
    }

    /// A book without pictures, opened.
    fn write_other(s: &Sample) -> TestResult<BookSource> {
        let path = s.dir.path().join("other.mbook");
        write_book(&path, "O", "en", &[("a.md", "Text\n")], None)?;
        Ok(open(&path)?.1)
    }

    #[test]
    fn a_load_error_keeps_its_cause_as_the_source() {
        use std::error::Error as _;
        let error = LoadError::Decode(PictureError::UnsupportedFormat);
        assert_eq!(error.to_string(), "the picture could not be decoded");
        assert!(error.source().is_some());
    }
}
