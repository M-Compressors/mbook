//! A picture or a figure of a book over the whole window, to look at it
//! closely.
//!
//! [`PictureViewer`] shows what it is given fitted to the window and lets the
//! reader enlarge and reduce it with buttons, keys, a pinch, or the wheel
//! with Command or Control held; an enlarged picture is moved with a drag or
//! by scrolling. It knows nothing of books: the owner hands it a decoded
//! picture, or a drawn figure, and closes it when it asks to be closed
//! ([`ViewerEvent::Close`]).
//!
//! A picture is shown at its pixels as logical pixels, as the text shows it;
//! a figure at the size of the text. The viewer fits either to the window,
//! enlarging it at most four times to do so, and draws a figure again for its
//! new size each time it is twice as large or as small.

mod zoom;

use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, AnyElement, App, ClickEvent, Context, CursorStyle, DispatchPhase, EventEmitter,
    FocusHandle, Focusable, Global, HitboxBehavior, Image, ImageSource, InteractiveElement as _,
    IntoElement, KeyBinding, KeyBindingContextPredicate, MouseButton, MouseDownEvent,
    MouseMoveEvent, ObjectFit, ParentElement as _, PinchEvent, Pixels, Point, Render, RenderImage,
    Role, ScrollEvent, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, TestSupportExt as _, Window, actions, canvas, div, img, point, px,
};

use self::zoom::{Scales, anchored, dragged, raster_density, scales, step};
use crate::entities::figure::Figure;
use crate::shared::actions::{Find, GoBack};
use crate::shared::ui::{icon_button, target_size};

pub(crate) use zoom::decoded_size;

/// The key context of the viewer.
pub(crate) const CONTEXT: &str = "PictureViewer";
/// The room kept around a fitted picture, in logical pixels.
const MARGIN: f32 = 24.0;
/// The room the controls take under the picture, in logical pixels.
const CONTROLS: f32 = 64.0;
/// How far the pointer moves before a press is a drag, in logical pixels.
const DRAG_SLOP: f32 = 4.0;
/// How much one line of the wheel enlarges or reduces the picture.
const WHEEL_LINE: f32 = 1.1;
/// How much one pixel of a precise wheel enlarges or reduces the picture.
const WHEEL_PIXEL: f32 = 1.01;

actions!(
    picture_viewer,
    [
        /// Enlarges the picture one step.
        ZoomIn,
        /// Reduces the picture one step.
        ZoomOut,
        /// Fits the whole picture to the window.
        ZoomToFit,
    ]
);

/// What the viewer shows.
#[derive(Clone)]
pub(crate) enum ViewerContent {
    /// A picture of these pixels that is being decoded.
    Loading((u32, u32)),
    /// A decoded picture. `pixels` is its size in the book, which the image
    /// may have fewer of ([`decoded_size`]).
    Picture {
        image: Arc<RenderImage>,
        pixels: (u32, u32),
    },
    /// A diagram or a formula, drawn for text of `text_size` pixels.
    Figure { figure: Arc<Figure>, text_size: f32 },
    /// The picture could not be decoded.
    Failed,
}

/// What the viewer asks of its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewerEvent {
    /// Close the viewer.
    Close,
}

/// A press of the pointer that may become a drag.
#[derive(Debug, Clone, Copy)]
struct Press {
    from: Point<Pixels>,
    offset: Point<Pixels>,
    moved: bool,
}

/// The images a figure was drawn as for the viewer: the one shown and the
/// one before it, which stands in while the new one is rasterised.
#[derive(Default)]
struct Rasters {
    current: Option<(u32, Arc<Image>)>,
    previous: Option<Arc<Image>>,
}

/// A picture or a figure over the whole window.
pub(crate) struct PictureViewer {
    alt: SharedString,
    content: ViewerContent,
    /// The scale as a part of the own size; `None` while it fits the window
    /// and follows its size.
    zoom: Option<f32>,
    /// The scales of the last frame.
    scales: Option<Scales>,
    scroll: ScrollHandle,
    focus_handle: FocusHandle,
    press: Option<Press>,
    /// Whether the last press was a drag, so that its release closes
    /// nothing.
    dragged: bool,
    rasters: Rasters,
    /// The viewer takes the focus when it is first drawn: the click that
    /// opened it may still be on its way and give the focus to the text.
    focus_on_draw: bool,
}

impl EventEmitter<ViewerEvent> for PictureViewer {}

impl PictureViewer {
    /// A viewer of `content`, described by `alt`.
    pub(crate) fn new(
        alt: impl Into<SharedString>,
        content: ViewerContent,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        bind(cx);
        Self {
            alt: alt.into(),
            content,
            zoom: None,
            scales: None,
            scroll: ScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            press: None,
            dragged: false,
            rasters: Rasters::default(),
            focus_on_draw: true,
        }
    }

    /// Replaces what is shown: the decoded picture arrived, or it failed.
    pub(crate) fn set_content(&mut self, content: ViewerContent, cx: &mut Context<'_, Self>) {
        self.release(cx);
        self.content = content;
        cx.notify();
    }

    /// What is shown.
    #[cfg(test)]
    pub(crate) fn content(&self) -> &ViewerContent {
        &self.content
    }

    /// The scale as a part of the own size, once the viewer was drawn.
    pub(crate) fn zoom(&self) -> Option<f32> {
        self.zoom.or_else(|| self.scales.map(|scales| scales.fit))
    }

    /// Frees the images the viewer made. The owner calls it before it drops
    /// the viewer: GPUI keeps an image until it is told to let it go.
    pub(crate) fn release(&mut self, cx: &mut App) {
        if let ViewerContent::Picture { image, .. } = &self.content {
            cx.drop_image(Arc::clone(image), None);
        }
        let rasters = std::mem::take(&mut self.rasters);
        for image in rasters
            .current
            .map(|(_, image)| image)
            .into_iter()
            .chain(rasters.previous)
        {
            ImageSource::from(image).remove_asset(cx);
        }
    }

    /// The size of the picture at its own scale, in logical pixels.
    fn natural(&self) -> Option<(f32, f32)> {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a size on the screen; pixels of a picture are far below 2^24"
        )]
        let pixels = |(width, height): (u32, u32)| (width as f32, height as f32);
        match &self.content {
            ViewerContent::Loading(size) | ViewerContent::Picture { pixels: size, .. } => {
                Some(pixels(*size))
            }
            ViewerContent::Figure { figure, text_size } => {
                let scale = text_size / figure.em;
                Some((figure.width * scale, figure.height * scale))
            }
            ViewerContent::Failed => None,
        }
    }

    /// Changes the scale to `zoom`, keeping the part of the picture at the
    /// point `at` of the window where it is; the middle without one.
    fn zoom_to(&mut self, zoom: f32, at: Option<Point<Pixels>>, cx: &mut Context<'_, Self>) {
        let (Some(scales), Some(before)) = (self.scales, self.zoom()) else {
            return;
        };
        let after = scales.clamp(zoom);
        if (after - before).abs() < f32::EPSILON {
            return;
        }
        let bounds = self.scroll.bounds();
        let room = bounds.size;
        let at = at.map_or_else(
            || point(room.width / 2.0, room.height / 2.0),
            |at| at - bounds.origin,
        );
        if let Some((width, height)) = self.shown_size(before)
            && let Some((new_width, new_height)) = self.shown_size(after)
        {
            let offset = self.scroll.offset();
            self.scroll.set_offset(point(
                px(anchored(
                    offset.x.as_f32(),
                    room.width.as_f32(),
                    width,
                    new_width,
                    at.x.as_f32(),
                )),
                px(anchored(
                    offset.y.as_f32(),
                    room.height.as_f32(),
                    height,
                    new_height,
                    at.y.as_f32(),
                )),
            ));
        }
        self.zoom = Some(after);
        cx.notify();
    }

    /// The size the picture takes at `zoom`, as the last frame measured it.
    fn shown_size(&self, zoom: f32) -> Option<(f32, f32)> {
        let (width, height) = self.natural()?;
        Some((width * zoom, height * zoom))
    }

    /// One step larger or smaller.
    fn step(&mut self, larger: bool, cx: &mut Context<'_, Self>) {
        if let (Some(scales), Some(zoom)) = (self.scales, self.zoom()) {
            self.zoom_to(step(zoom, scales, larger), None, cx);
        }
    }

    /// Back to the whole picture in the window.
    fn fit(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(scales) = self.scales {
            self.zoom_to(scales.fit, None, cx);
        }
        self.zoom = None;
        self.scroll.set_offset(Point::default());
        cx.notify();
    }

    /// What the viewer and its picture are called: the alternative text,
    /// or a word for a picture that has none.
    fn name(&self) -> SharedString {
        if self.alt.is_empty() {
            "Picture".into()
        } else {
            self.alt.clone()
        }
    }
}

/// Marks that the keys are bound, so that a second viewer does not bind them
/// again.
struct Bound;

impl Global for Bound {}

/// Binds the keys of the viewer, once. `secondary-` is Command on macOS and
/// Control elsewhere. Escape closes it through [`GoBack`].
fn bind(cx: &mut App) {
    if cx.has_global::<Bound>() {
        return;
    }
    cx.set_global(Bound);
    let keys: [(&str, Box<dyn Action>); 9] = [
        ("=", Box::new(ZoomIn)),
        ("+", Box::new(ZoomIn)),
        ("secondary-=", Box::new(ZoomIn)),
        ("secondary-+", Box::new(ZoomIn)),
        ("-", Box::new(ZoomOut)),
        ("secondary--", Box::new(ZoomOut)),
        ("0", Box::new(ZoomToFit)),
        ("secondary-0", Box::new(ZoomToFit)),
        ("f", Box::new(ZoomToFit)),
    ];
    let context = KeyBindingContextPredicate::parse(CONTEXT).ok().map(Rc::new);
    // A key the platform cannot parse is left out; the buttons still reach
    // every action.
    let bindings: Vec<KeyBinding> = keys
        .into_iter()
        .filter_map(|(keys, action)| {
            KeyBinding::load(keys, action, context.clone(), false, None, cx).ok()
        })
        .collect();
    cx.bind_keys(bindings);
}

impl Focusable for PictureViewer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PictureViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        if std::mem::take(&mut self.focus_on_draw) {
            let handle = self.focus_handle.clone();
            window.defer(cx, move |window, cx| handle.focus(window, cx));
        }
        let viewport = window.viewport_size();
        let insets = crate::shared::safe_area::insets(window);
        let room = (
            viewport.width.as_f32() - insets.left.as_f32() - insets.right.as_f32() - 2.0 * MARGIN,
            viewport.height.as_f32()
                - insets.top.as_f32()
                - insets.bottom.as_f32()
                - 2.0 * MARGIN
                - 2.0 * CONTROLS,
        );
        self.scales = self.natural().and_then(|natural| scales(natural, room));
        if let (Some(scales), Some(zoom)) = (self.scales, self.zoom) {
            // The window changed and the old scale is out of reach.
            self.zoom = Some(scales.clamp(zoom));
        }
        let zoom = self.zoom();
        let picture = self.picture(zoom, window, cx);
        let larger = self
            .shown_size(zoom.unwrap_or(1.0))
            .zip(Some(self.scroll.bounds().size))
            .is_some_and(|((width, height), room)| {
                width > room.width.as_f32() + 0.5 || height > room.height.as_f32() + 0.5
            });
        let backdrop = gpui_kit::black().opacity(0.94);

        div()
            .id("picture-viewer")
            .role(Role::Dialog)
            .aria_label(self.name())
            .test_support()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .absolute()
            .inset_0()
            .occlude()
            .bg(backdrop)
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| this.step(true, cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| this.step(false, cx)))
            .on_action(cx.listener(|this, _: &ZoomToFit, _, cx| this.fit(cx)))
            .on_action(cx.listener(|_, _: &GoBack, _, cx| cx.emit(ViewerEvent::Close)))
            // The page behind has a bar of its own to find in; it is not
            // opened under the viewer.
            .on_action(|_: &Find, _, _| {})
            .child(Self::wheel_zoom(cx))
            .child(self.area(picture, larger, cx))
            .child(
                div()
                    .absolute()
                    .top(insets.top + px(12.0))
                    .right(insets.right + px(12.0))
                    .occlude()
                    .p_1()
                    .rounded_full()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().popover)
                    .text_color(cx.theme().popover_foreground)
                    .child(
                        icon_button("picture-viewer-close", IconName::Close, "Close")
                            .rounded_full()
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(ViewerEvent::Close))),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(insets.bottom + px(16.0))
                    .flex()
                    .justify_center()
                    .child(self.controls(zoom, cx)),
            )
    }
}

impl PictureViewer {
    /// What stands in the middle: the picture at `zoom`, or why there is
    /// none.
    fn picture(
        &mut self,
        zoom: Option<f32>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let size = zoom.and_then(|zoom| self.shown_size(zoom));
        let ink = gpui_kit::white();
        let name = self.name();
        match (&self.content, size) {
            (ViewerContent::Picture { image, .. }, Some((width, height))) => shown_at(
                img(Arc::clone(image)).object_fit(ObjectFit::Fill),
                (width, height),
                name,
            ),
            (ViewerContent::Figure { figure, text_size }, Some((width, height))) => {
                let figure = Arc::clone(figure);
                let units_scale = zoom.unwrap_or(1.0) * text_size / figure.em;
                let Some(density) = raster_density(
                    units_scale * window.scale_factor(),
                    (figure.width, figure.height),
                ) else {
                    return div().into_any_element();
                };
                let (image, previous) = self.raster(&figure, density, cx);
                let mut shown = img(image).object_fit(ObjectFit::Fill);
                if let Some(previous) = previous {
                    shown = shown.with_loading(move || {
                        img(Arc::clone(&previous)).size_full().into_any_element()
                    });
                }
                shown_at(shown, (width, height), name)
            }
            (ViewerContent::Failed, _) => div()
                .text_color(ink)
                .child("This picture could not be shown.")
                .into_any_element(),
            _ => {
                let indicator = if cx.reduce_motion() {
                    gpui_kit::component::Icon::new(IconName::Loader)
                        .large()
                        .into_any_element()
                } else {
                    gpui_kit::component::spinner::Spinner::new()
                        .large()
                        .into_any_element()
                };
                div().text_color(ink).child(indicator).into_any_element()
            }
        }
    }

    /// The image of `figure` at `density`, drawn when it is not yet, and the
    /// image before it while that one is rasterised.
    fn raster(
        &mut self,
        figure: &Figure,
        density: f32,
        cx: &mut App,
    ) -> (Arc<Image>, Option<Arc<Image>>) {
        let key = density.to_bits();
        if let Some((made_for, image)) = &self.rasters.current
            && *made_for == key
        {
            return (Arc::clone(image), self.rasters.previous.clone());
        }
        let image = figure.draw(density);
        if let Some(gone) = self.rasters.previous.take() {
            ImageSource::from(gone).remove_asset(cx);
        }
        self.rasters.previous = self
            .rasters
            .current
            .replace((key, Arc::clone(&image)))
            .map(|(_, image)| image);
        (image, self.rasters.previous.clone())
    }

    /// The room the picture is scrolled and dragged in. A click beside the
    /// picture closes the viewer.
    fn area(&self, picture: AnyElement, larger: bool, cx: &mut Context<'_, Self>) -> AnyElement {
        // As large as the room, or as the picture when it is larger, so that
        // all of an enlarged picture can be scrolled to.
        let centred = div()
            .flex_none()
            .min_w_full()
            .min_h_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("picture-viewer-frame")
                    .flex_none()
                    // A click on the picture is no click beside it.
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(picture),
            );
        div()
            .id("picture-viewer-area")
            .role(Role::ScrollView)
            .aria_label(self.name())
            .test_support()
            .size_full()
            .flex()
            .items_start()
            .overflow_scroll()
            .track_scroll(&self.scroll)
            .when(larger, |area| {
                area.cursor(if self.press.is_some() {
                    CursorStyle::ClosedHand
                } else {
                    CursorStyle::OpenHand
                })
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.press = Some(Press {
                        from: event.position,
                        offset: this.scroll.offset(),
                        moved: false,
                    });
                    this.dragged = false;
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(Self::on_drag))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.dragged = this.press.take().is_some_and(|press| press.moved);
                    cx.notify();
                }),
            )
            .on_pinch(cx.listener(|this, event: &PinchEvent, _, cx| {
                if let Some(zoom) = this.zoom() {
                    this.zoom_to(zoom * (1.0 + event.delta), Some(event.position), cx);
                }
            }))
            .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                if !std::mem::take(&mut this.dragged) && !matches!(event, ClickEvent::Keyboard(_)) {
                    cx.emit(ViewerEvent::Close);
                }
            }))
            .child(centred)
            .into_any_element()
    }

    /// Moves an enlarged picture with the pointer that holds it.
    fn on_drag(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(press) = self.press.as_mut() else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.press = None;
            return;
        }
        let moved = event.position - press.from;
        if !press.moved && moved.x.as_f32().hypot(moved.y.as_f32()) < DRAG_SLOP {
            return;
        }
        press.moved = true;
        let press = *press;
        let Some((width, height)) = self.zoom().and_then(|zoom| self.shown_size(zoom)) else {
            return;
        };
        let room = self.scroll.bounds().size;
        self.scroll.set_offset(point(
            px(dragged(
                press.offset.x.as_f32(),
                moved.x.as_f32(),
                room.width.as_f32(),
                width,
            )),
            px(dragged(
                press.offset.y.as_f32(),
                moved.y.as_f32(),
                room.height.as_f32(),
                height,
            )),
        ));
        cx.notify();
    }

    /// The wheel with Command or Control held enlarges and reduces the
    /// picture around the pointer. The view claims the event before the room
    /// scrolls with it: a listener of an element painted first is asked
    /// first.
    fn wheel_zoom(cx: &mut Context<'_, Self>) -> AnyElement {
        let viewer = cx.entity().downgrade();
        canvas(
            |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |_, hitbox, window, _| {
                let viewer = viewer.clone();
                window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture
                        || !event.modifiers.secondary(cx)
                        || !hitbox.should_handle_scroll(window)
                    {
                        return;
                    }
                    cx.stop_propagation();
                    let factor = wheel_factor(event);
                    viewer
                        .update(cx, |this, cx| {
                            if let Some(zoom) = this.zoom() {
                                this.zoom_to(zoom * factor, Some(event.position), cx);
                            }
                        })
                        .ok();
                });
            },
        )
        .absolute()
        .inset_0()
        .into_any_element()
    }

    /// The buttons under the picture.
    fn controls(&self, zoom: Option<f32>, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme();
        let scales = self.scales;
        let (smaller, larger) = match (scales, zoom) {
            (Some(scales), Some(zoom)) => (zoom > scales.least * 1.001, zoom < scales.most / 1.001),
            _ => (false, false),
        };
        let percent: SharedString =
            zoom.map_or_else(|| "–".into(), |zoom| format!("{:.0}%", zoom * 100.0).into());
        h_flex()
            .id("picture-viewer-controls")
            .occlude()
            .gap_1()
            .p_1()
            .rounded_full()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .child(
                icon_button("picture-viewer-zoom-out", IconName::ZoomOut, "Zoom out")
                    .rounded_full()
                    .disabled(!smaller)
                    .on_click(cx.listener(|this, _, _, cx| this.step(false, cx))),
            )
            .child(
                div()
                    .id("picture-viewer-zoom")
                    .role(Role::Status)
                    .aria_label(percent.clone())
                    .test_support()
                    .min_w(px(target_size() + 20.0))
                    .text_center()
                    .text_sm()
                    .child(percent),
            )
            .child(
                icon_button("picture-viewer-zoom-in", IconName::ZoomIn, "Zoom in")
                    .rounded_full()
                    .disabled(!larger)
                    .on_click(cx.listener(|this, _, _, cx| this.step(true, cx))),
            )
            .child(
                icon_button("picture-viewer-fit", IconName::Scan, "Fit to window")
                    .rounded_full()
                    .disabled(self.zoom.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.fit(cx))),
            )
    }
}

/// An image `width` by `height` logical pixels, reported as a picture
/// called `name`. GPUI reports no role of an image element itself.
fn shown_at(image: gpui_kit::Img, (width, height): (f32, f32), name: SharedString) -> AnyElement {
    div()
        .id("picture-viewer-image")
        .role(Role::Image)
        .aria_label(name)
        .test_support()
        .flex_none()
        .w(px(width))
        .h(px(height))
        .child(image.size_full())
        .into_any_element()
}

/// How much one turn of the wheel enlarges the picture: more than one when
/// it is turned away from the reader.
fn wheel_factor(event: &ScrollEvent) -> f32 {
    use gpui_kit::ScrollDelta;
    match event.delta {
        ScrollDelta::Lines(lines) => WHEEL_LINE.powf(lines.y),
        ScrollDelta::Pixels(pixels) => WHEEL_PIXEL.powf(pixels.y.as_f32()),
    }
}

#[cfg(test)]
mod tests;
