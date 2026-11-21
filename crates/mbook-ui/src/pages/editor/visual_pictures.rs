//! Loads images from the editable book for Visual and the picture gallery.

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, v_flex};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Context, ImageCacheError, ImageSource,
    InteractiveElement as _, ObjectFit, ParentElement as _, RenderImage, Role,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _, TestSupportExt as _, Window,
    div, img, px,
};
use percent_encoding::percent_decode_str;

use super::EditorPage;
use crate::entities::book::chapter::picture_destinations;
use crate::entities::picture::{PictureCache, decode_avif};
use crate::features::edit_book::actor::EditHandle;
use crate::features::edit_book::outline::PictureOutline;
use crate::shared::ui::announce;
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

impl EditorPage {
    pub(super) fn preview_chapter_pictures(&self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(chapter) = &self.loaded else { return };
        let source = self.text.read(cx).value();
        let mut seen = HashSet::new();
        let pictures: Rc<Vec<_>> = Rc::new(
            picture_destinations(&source)
                .iter()
                .filter_map(|uri| book_picture_path(chapter, uri))
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
            announce("This chapter has no book pictures.", window, cx);
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
                .aria_label(format!("Preview of {}", picture.path))
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
                        .label("Choose picture")
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
                .title("Chapter pictures")
                .width(px(width + 48.0))
                .description(content)
                .ok_text("Close")
        });
    }

    pub(super) fn picture_thumbnail(&self, picture: &PictureOutline) -> ImageSource {
        let key = picture_size((picture.width, picture.height), 280.0, Some(192.0))
            .map(|size| (picture.path.clone(), size));
        image_source(key, Arc::clone(&self.visual.pictures), self.handle.clone())
    }
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
            cx.background_spawn(async move {
                let bitmap = decode_avif(&encoded, MAX_DECODE_PIXELS)
                    .ok()?
                    .resize(width, height);
                Some((bitmap.byte_len(), bitmap.into_render_image()))
            })
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

/// Resolves a relative destination lexically. Only a catalogue match is read.
fn book_picture_path(chapter: &str, destination: &str) -> Option<String> {
    if destination.contains(['#', '?']) || destination.split('/').next()?.contains(':') {
        return None;
    }
    let path = percent_decode_str(destination).decode_utf8().ok()?;
    let mut segments = chapter.split('/').collect::<Vec<_>>();
    segments.pop();
    for name in path.split('/') {
        match name {
            "" => return None,
            "." => {}
            ".." => {
                segments.pop()?;
            }
            name => segments.push(name),
        }
    }
    Some(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::book_picture_path;

    #[test]
    fn a_chapter_picture_path_is_relative_and_percent_decoded() {
        assert_eq!(
            book_picture_path("part/ch01.md", "./../img/My%20pond.avif").as_deref(),
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
            assert!(book_picture_path("ch01.md", destination).is_none());
        }
    }
}
