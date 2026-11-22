//! What was drawn, kept by its source and the way it was drawn.

use std::sync::{Arc, LazyLock, Mutex};

use super::{Figure, FigureError, FigureKind, Palette, Setting};
use crate::entities::picture::PictureCache;

/// The bytes of SVG the cache keeps. A formula takes a few kilobytes and a
/// diagram some tens, so this is the figures of several chapters.
const BUDGET: usize = 48 * 1024 * 1024;
/// What a remembered failure is counted as.
const FAILURE_BYTES: usize = 256;

/// How a figure was drawn, apart from its source.
pub(super) type Manner = (FigureKind, Setting, Palette);
type Key = (Manner, Box<str>);
type Drawn = Result<Arc<Figure>, FigureError>;

/// One cache for the application: a figure is a function of its source and
/// of the manner it is drawn in, whoever asks for it.
static CACHE: LazyLock<Mutex<PictureCache<Key, Drawn>>> =
    LazyLock::new(|| Mutex::new(PictureCache::new(BUDGET)));

/// The figure of `source` drawn in `manner`: from the cache, or drawn now
/// and kept. Two tasks that ask for the same figure at once may both draw
/// it. They use the first drawing published to the cache.
pub(super) fn get_or_draw(
    manner: Manner,
    source: &str,
    draw: impl FnOnce() -> Result<Figure, FigureError>,
) -> Drawn {
    let key: Key = (manner, source.into());
    if let Some(kept) = CACHE.lock().ok().and_then(|mut cache| cache.get(&key)) {
        return (*kept).clone();
    }
    let drawn: Drawn = draw().map(Arc::new);
    let bytes = drawn
        .as_ref()
        .map_or(FAILURE_BYTES, |figure| figure.byte_len())
        + source.len();
    if let Ok(mut cache) = CACHE.lock() {
        if let Some(kept) = cache.get(&key) {
            return (*kept).clone();
        }
        cache.insert(key, Arc::new(drawn.clone()), bytes);
    }
    drawn
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manner() -> Manner {
        (FigureKind::Formula, Setting::Line, Palette::PAPER)
    }

    #[test]
    fn what_is_kept_is_not_drawn_again() {
        let source = "kept-by-the-cache-test";
        let first = get_or_draw(manner(), source, || Err(FigureError::Empty));
        let again = get_or_draw(manner(), source, || Err(FigureError::Renderer));
        assert_eq!(first.err(), Some(FigureError::Empty));
        assert_eq!(again.err(), Some(FigureError::Empty));
    }

    #[test]
    fn another_manner_is_another_figure() {
        let source = "manner-of-the-cache-test";
        let line = get_or_draw(manner(), source, || Err(FigureError::Empty));
        let block = get_or_draw(
            (FigureKind::Formula, Setting::Block, Palette::PAPER),
            source,
            || Err(FigureError::Renderer),
        );
        assert_ne!(line.err(), block.err());
    }

    #[test]
    fn a_drawing_published_during_another_render_is_reused() {
        let source = "same-source-published-during-render";
        let drawn = get_or_draw(manner(), source, || {
            let published = get_or_draw(manner(), source, || Err(FigureError::Empty));
            assert_eq!(published.err(), Some(FigureError::Empty));
            Err(FigureError::Renderer)
        });
        assert_eq!(drawn.err(), Some(FigureError::Empty));
    }
}
