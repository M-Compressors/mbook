//! Loads images from the editable book for Visual and the picture gallery.

use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Context, Focusable as _, FontWeight, ImageCacheError,
    ImageSource, InteractiveElement as _, ObjectFit, ParentElement as _, RenderImage, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, StyledImage as _,
    TestSupportExt as _, Window, div, img, px,
};
use rust_i18n::t;

use super::EditorPage;
use super::encode::Target;
use super::markup::picture_markdown;
use super::picture_prompt::{description_hint, description_label};
use crate::entities::book::chapter::{image_path, picture_destinations};
use crate::entities::picture::{PictureCache, decode_avif};
use crate::features::edit_book::actor::EditHandle;
use crate::features::edit_book::outline::PictureOutline;
use crate::shared::compute;
use crate::shared::ui::announce;
use crate::shared::ui::{status_line, target_size};
use crate::widgets::chapter_view::picture_size;

const MAX_DECODE_PIXELS: u64 = 40_000_000;
const CACHE_BYTES: usize = 32 * 1024 * 1024;
type PictureKey = (String, (u32, u32));

pub(super) struct VisualPictures {
    ready: PictureCache<PictureKey, RenderImage>,
    loading: HashSet<PictureKey>,
    failed: HashSet<String>,
}

impl Default for VisualPictures {
    fn default() -> Self {
        Self {
            ready: PictureCache::new(CACHE_BYTES),
            loading: HashSet::new(),
            failed: HashSet::new(),
        }
    }
}

/// The control of the dialog that takes a picture from a file.
pub(super) fn from_computer() -> Cow<'static, str> {
    t!("visual.pictures.from_computer")
}

/// The control of the dialog that puts the chosen picture into the chapter.
pub(super) fn insert() -> Cow<'static, str> {
    t!("visual.pictures.insert")
}

/// What the dialog says when it is confirmed with no picture chosen.
pub(super) fn choose_first() -> Cow<'static, str> {
    t!("visual.pictures.choose_first")
}

/// What the dialog says about a book without pictures.
pub(super) fn no_pictures() -> Cow<'static, str> {
    t!("visual.pictures.none")
}

/// The sides of a picture in the dialog that offers them, in pixels.
const CHOICE: (f32, f32) = (132.0, 96.0);

impl EditorPage {
    /// The dialog that puts a picture where the caret is: one the book has
    /// already, chosen by its thumbnail, or a new one from a file, which is
    /// added to the book on the way.
    pub(super) fn ask_chapter_picture(&self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(chapter) = self.loaded.clone() else {
            return;
        };
        let pictures: Rc<Vec<PictureOutline>> = Rc::new(self.session.pictures().to_vec());
        let state = Arc::clone(&self.visual.pictures);
        let handle = self.handle.clone();
        let selected = Rc::new(Cell::new(None::<usize>));
        let description = cx.new(|cx| InputState::new(window, cx).placeholder(description_hint()));
        let error = cx.new(|_| None::<SharedString>);
        let page = cx.weak_entity();
        let first = Cell::new(true);
        window.open_alert_dialog(cx, move |alert, window, cx| {
            if first.replace(false) {
                description.focus_handle(cx).focus(window, cx);
            }
            let theme = cx.theme().clone();
            let width = (window.viewport_size().width.as_f32() - 96.0).clamp(240.0, 600.0);
            let choices = h_flex()
                .id("picture-choices")
                .role(Role::Group)
                .aria_label(t!("visual.pictures.of_the_book"))
                .test_support()
                .flex_wrap()
                .gap_2()
                .max_h(px(280.0))
                .overflow_y_scroll()
                .children(pictures.iter().enumerate().map(|(index, picture)| {
                    let key = picture_size(
                        (picture.width, picture.height),
                        CHOICE.0 * 2.0,
                        Some(CHOICE.1 * 2.0),
                    )
                    .map(|size| (picture.path.clone(), size));
                    let image = image_source(key, Arc::clone(&state), handle.clone());
                    let (chosen, error) = (selected.clone(), error.clone());
                    picture_choice(
                        index,
                        &picture.path,
                        selected.get() == Some(index),
                        image,
                        cx,
                    )
                    .on_click(move |_, window, cx| {
                        chosen.set(Some(index));
                        error.update(cx, |error, cx| {
                            *error = None;
                            cx.notify();
                        });
                        window.refresh();
                    })
                }));
            let upload = upload_button(page.clone());
            let form = v_flex()
                .gap_3()
                .children(pictures.is_empty().then(|| {
                    status_line("no-book-pictures", no_pictures())
                        .text_color(theme.muted_foreground)
                }))
                .children((!pictures.is_empty()).then_some(choices))
                .child(h_flex().child(upload))
                .children((!pictures.is_empty()).then(|| {
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .child(SharedString::from(description_label())),
                        )
                        .child(Input::new(&description).aria_label(description_label()))
                }))
                .children(error.read(cx).clone().map(|message| {
                    status_line("picture-choice-error", message).text_color(theme.danger)
                }));
            let (page, pictures, chapter) = (page.clone(), pictures.clone(), chapter.clone());
            let (selected, description, error) =
                (selected.clone(), description.clone(), error.clone());
            alert
                .title(t!("visual.pictures.insert_title"))
                .width(px(width))
                .description(form)
                .confirm()
                .ok_text(insert())
                .cancel_text(t!("prompt.cancel"))
                .on_ok(move |_, window, cx| {
                    let Some(picture) = selected.get().and_then(|index| pictures.get(index)) else {
                        error.update(cx, |error, cx| {
                            *error = Some(choose_first().into());
                            cx.notify();
                        });
                        window.refresh();
                        return false;
                    };
                    let markdown =
                        picture_markdown(&chapter, &picture.path, &description.read(cx).value());
                    page.update(cx, |this, cx| this.insert_markdown(markdown, window, cx))
                        .ok();
                    true
                })
        });
    }

    pub(super) fn preview_chapter_pictures(&self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(chapter) = &self.loaded else { return };
        let source = self.text.read(cx).value();
        let mut seen = HashSet::new();
        let pictures: Rc<Vec<_>> = Rc::new(
            picture_destinations(&source)
                .iter()
                .filter_map(|uri| image_path(chapter, uri))
                .filter_map(|path| {
                    self.session
                        .pictures()
                        .iter()
                        .find(|picture| picture.path == path)
                })
                .filter(|picture| seen.insert(picture.path.as_str()))
                .cloned()
                .collect(),
        );
        if pictures.is_empty() {
            announce(t!("visual.pictures.none_in_chapter"), window, cx);
            return;
        }
        let state = Arc::clone(&self.visual.pictures);
        let handle = self.handle.clone();
        let selected = Rc::new(Cell::new(0));
        window.open_alert_dialog(cx, move |alert, window, cx| {
            let picture = &pictures[selected.get()];
            let viewport = window.viewport_size();
            let width = (viewport.width.as_f32() - 124.0).clamp(120.0, 520.0);
            let height = (viewport.height.as_f32() - 190.0).clamp(80.0, 400.0);
            let size = picture_size((picture.width, picture.height), width, Some(height));
            let key = size.map(|size| (picture.path.clone(), size));
            let image = image_source(key, Arc::clone(&state), handle.clone());
            let frame = div()
                .id("chapter-picture-preview")
                .role(Role::Image)
                .aria_label(t!("visual.pictures.preview_of", path = picture.path))
                .test_support()
                .relative()
                .flex_none()
                .w_full()
                .h(px(height))
                .overflow_hidden()
                .rounded_md()
                .bg(cx.theme().muted)
                .child(
                    img(image)
                        .object_fit(ObjectFit::Contain)
                        .absolute()
                        .top_0()
                        .left_0()
                        .w_full()
                        .h(px(height)),
                );
            let content = v_flex()
                .gap_2()
                .child(frame)
                .child(div().text_sm().child(picture.path.clone()))
                .children((pictures.len() > 1).then(|| {
                    let pictures = pictures.clone();
                    let selected = selected.clone();
                    Button::new("chapter-picture-select")
                        .label(t!("visual.pictures.choose"))
                        .dropdown_caret(true)
                        .dropdown_menu(move |menu, _, _| {
                            pictures
                                .iter()
                                .enumerate()
                                .fold(menu, |menu, (index, picture)| {
                                    let selected = selected.clone();
                                    menu.item(
                                        PopupMenuItem::new(picture.path.clone())
                                            .checked(index == selected.get())
                                            .on_click(move |_, window, _| {
                                                selected.set(index);
                                                window.refresh();
                                            }),
                                    )
                                })
                        })
                }));
            alert
                .title(t!("visual.pictures.of_the_chapter"))
                .width(px(width + 48.0))
                .description(content)
                .ok_text(t!("visual.pictures.close"))
        });
    }

    pub(super) fn picture_thumbnail(&self, picture: &PictureOutline) -> ImageSource {
        let key = picture_size((picture.width, picture.height), 280.0, Some(192.0))
            .map(|size| (picture.path.clone(), size));
        image_source(key, Arc::clone(&self.visual.pictures), self.handle.clone())
    }
}

/// The control that closes the dialog and takes the picture from a file.
fn upload_button(page: gpui_kit::WeakEntity<EditorPage>) -> Button {
    Button::new("picture-upload")
        .label(from_computer())
        .min_h(px(target_size()))
        .cursor_pointer()
        .on_click(move |_, window, cx| {
            window.close_dialog(cx);
            page.update(cx, |this, cx| {
                this.choose_pictures(Target::Chapter, window, cx);
            })
            .ok();
        })
}

/// One picture the dialog offers: its thumbnail over the name of its file.
/// What assistive technology hears is its whole path, and whether it is the
/// one chosen.
fn picture_choice(index: usize, path: &str, chosen: bool, image: ImageSource, cx: &App) -> Button {
    let name = path.rsplit('/').next().unwrap_or_default().to_owned();
    Button::new(("picture-choice", index))
        .ghost()
        .selected(chosen)
        .accessibility_label(if chosen {
            t!("visual.pictures.chosen", path = path)
        } else {
            Cow::Owned(path.to_owned())
        })
        .cursor_pointer()
        .w(px(CHOICE.0 + 16.0))
        .h(px(CHOICE.1 + 44.0))
        .p_2()
        .child(
            v_flex()
                .w(px(CHOICE.0))
                .gap_1()
                .child(
                    div()
                        .relative()
                        .flex_none()
                        .w_full()
                        .h(px(CHOICE.1))
                        .overflow_hidden()
                        .rounded_md()
                        .bg(cx.theme().muted)
                        .child(
                            img(image)
                                .object_fit(ObjectFit::Contain)
                                .absolute()
                                .top_0()
                                .left_0()
                                .w_full()
                                .h(px(CHOICE.1)),
                        ),
                )
                .child(div().w_full().truncate().text_sm().child(name)),
        )
}

fn image_source(
    key: Option<PictureKey>,
    state: Arc<Mutex<VisualPictures>>,
    handle: EditHandle,
) -> ImageSource {
    ImageSource::Custom(Arc::new(move |window, cx| {
        let Some(key) = &key else {
            return Some(Err(ImageCacheError::Asset(
                "the picture is not part of the book".into(),
            )));
        };
        resolve(&state, key, &handle, window.window_handle(), cx)
    }))
}

fn resolve(
    cache: &Arc<Mutex<VisualPictures>>,
    key: &PictureKey,
    handle: &EditHandle,
    window: AnyWindowHandle,
    cx: &mut App,
) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
    let mut pictures = cache.lock().ok()?;
    if let Some(image) = pictures.ready.get(key) {
        return Some(Ok(image));
    }
    if pictures.failed.contains(&key.0) {
        return Some(Err(ImageCacheError::Asset(
            "the picture cannot be shown".into(),
        )));
    }
    if pictures.loading.len() >= 2 || !pictures.loading.insert(key.clone()) {
        return None;
    }
    drop(pictures);
    let key = key.clone();
    let cache = Arc::clone(cache);
    let handle = handle.clone();
    cx.spawn(async move |cx| {
        let encoded = handle.picture(&key.0).await;
        let decoded = if let Ok(encoded) = encoded {
            let (width, height) = key.1;
            cx.background_spawn(compute::spawn(move || {
                let bitmap = decode_avif(&encoded, MAX_DECODE_PIXELS)
                    .ok()?
                    .resize(width, height);
                Some((bitmap.byte_len(), bitmap.into_render_image()))
            }))
            .await
        } else {
            None
        };
        if let Ok(mut pictures) = cache.lock() {
            pictures.loading.remove(&key);
            if let Some((bytes, image)) = decoded {
                pictures.ready.insert(key, image, bytes);
            } else {
                pictures.failed.insert(key.0);
            }
        }
        cx.update(|cx| {
            window.update(cx, |_, window, _| window.refresh()).ok();
        });
    })
    .detach();
    None
}

#[cfg(test)]
impl VisualPictures {
    pub(super) fn has_cached(&mut self, path: &str, size: (u32, u32)) -> bool {
        self.ready.get(&(path.to_owned(), size)).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::image_path;

    #[test]
    fn a_chapter_picture_path_is_relative_and_percent_decoded() {
        assert_eq!(
            image_path("part/ch01.md", "./../img/My%20pond.avif").as_deref(),
            Some("img/My pond.avif")
        );
    }

    #[test]
    fn picture_lookup_refuses_absolute_external_and_outside_paths() {
        for destination in [
            "https://example.com/x.avif",
            "file:///tmp/x.avif",
            "/x.avif",
            "../x.avif",
            "%2Fx.avif",
        ] {
            assert!(image_path("ch01.md", destination).is_none());
        }
    }
}
