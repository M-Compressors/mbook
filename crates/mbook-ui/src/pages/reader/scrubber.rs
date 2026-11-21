//! The whole-book position slider.

use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::{AppContext as _, Context, Entity, Window};

use super::{Phase, ReaderPage, controls::slider_progress};

/// The slider's thumb can preview a location before the reader moves there.
pub(super) struct Scrubber {
    /// The Kit control's state.
    pub(super) slider: Entity<SliderState>,
    /// The fraction under the pointer during a gesture.
    pub(super) preview: Option<u16>,
    /// The latest actual position reconciled with the thumb.
    shown: Option<u16>,
}

impl Scrubber {
    /// Builds a slider with half-percent keyboard steps.
    pub(super) fn new(cx: &mut Context<'_, ReaderPage>) -> Self {
        Self {
            slider: cx.new(|_| SliderState::new().max(100.0).step(0.5)),
            preview: None,
            shown: None,
        }
    }

    /// Updates the thumb after a page turn, without moving it during a drag.
    pub(super) fn sync(
        &mut self,
        progress: u16,
        window: &mut Window,
        cx: &mut Context<'_, ReaderPage>,
    ) {
        if self.preview.is_none() && self.shown != Some(progress) {
            self.shown = Some(progress);
            self.slider.update(cx, |slider, cx| {
                slider.set_value(f32::from(progress) / 10.0, window, cx);
            });
        }
    }

    /// Starts over when the reader opens a different snapshot.
    pub(super) fn reset(&mut self) {
        self.preview = None;
        self.shown = None;
    }
}

impl ReaderPage {
    /// Previews the chapter while dragging and navigates when the gesture commits.
    pub(super) fn on_scrub(&mut self, event: &SliderEvent, cx: &mut Context<'_, Self>) {
        match event {
            SliderEvent::Change(value) => {
                self.scrubber.preview = Some(slider_progress(value.end()));
            }
            SliderEvent::Release(value) => {
                self.scrubber.preview = None;
                let position = slider_progress(value.end());
                if position != self.progress
                    && let Phase::Ready(reading) = &self.phase
                    && reading.source.is_some()
                    && let Some((chapter, offset)) = reading.snapshot.at_progress(position)
                {
                    self.jump(chapter, offset, cx);
                }
            }
        }
        self.touch(cx);
        cx.notify();
    }
}
