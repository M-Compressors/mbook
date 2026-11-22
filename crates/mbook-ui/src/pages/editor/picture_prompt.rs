//! The output settings for adding a picture to a book.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, Focusable as _, ParentElement as _, SharedString,
    Styled as _, Window, div,
};

use super::EditorPage;
use super::session::clean_path;
use crate::entities::picture::EncodeOptions;
use crate::shared::ui::status_line;

/// The label of the field that describes a picture for those who cannot see
/// it, wherever a picture is put into a chapter.
pub(super) const DESCRIPTION_LABEL: &str = "Description, read to those who cannot see the picture";
/// What the empty description field shows.
pub(super) const DESCRIPTION_HINT: &str = "What the picture shows";
/// The label of the field that names the picture inside the book.
pub(super) const NAME_LABEL: &str = "File name in the book";

pub(super) struct PictureSettings {
    pub(super) path: String,
    pub(super) options: EncodeOptions,
    /// What the description field held; empty when it was not asked for.
    pub(super) description: String,
}

/// Asks how a chosen file joins the book. With `describe` the dialog also
/// asks for the description of the picture, because it goes into a chapter.
#[expect(
    clippy::too_many_arguments,
    reason = "the dialog takes its words and both of its outcomes"
)]
pub(super) fn ask_picture(
    title: &'static str,
    name: String,
    proposal: String,
    describe: bool,
    window: &mut Window,
    cx: &mut Context<'_, EditorPage>,
    accept: impl Fn(&mut EditorPage, PictureSettings, &mut Window, &mut Context<'_, EditorPage>)
    + 'static,
    cancel: impl Fn(&mut EditorPage, &mut Window, &mut Context<'_, EditorPage>) + 'static,
) {
    let page = cx.weak_entity();
    let path = cx.new(|cx| InputState::new(window, cx));
    path.update(cx, |field, cx| field.set_value(proposal, window, cx));
    let quality = cx.new(|cx| InputState::new(window, cx));
    quality.update(cx, |field, cx| {
        field.set_value(EncodeOptions::default().quality.to_string(), window, cx);
    });
    let description = cx.new(|cx| InputState::new(window, cx).placeholder(DESCRIPTION_HINT));
    let width = cx.new(|cx| InputState::new(window, cx).placeholder("Original"));
    let height = cx.new(|cx| InputState::new(window, cx).placeholder("Original"));
    let error = cx.new(|_| None::<SharedString>);
    let (accept, cancel) = (Rc::new(accept), Rc::new(cancel));
    let first = Cell::new(true);
    window.open_alert_dialog(cx, move |alert, window, cx| {
        if first.replace(false) {
            let field = if describe { &description } else { &path };
            field.focus_handle(cx).focus(window, cx);
        }
        let danger = cx.theme().danger;
        let muted = cx.theme().muted_foreground;
        let form = v_flex()
            .gap_2()
            .child(div().child(name.clone()))
            .children(describe.then(|| labeled(DESCRIPTION_LABEL, &description)))
            .child(labeled(NAME_LABEL, &path))
            .child(labeled("Quality (1–100)", &quality))
            .child(
                h_flex()
                    .gap_2()
                    .child(labeled("Max width (px)", &width).flex_1())
                    .child(labeled("Max height (px)", &height).flex_1()),
            )
            .child(div().text_sm().text_color(muted).child(
                "Quality 80 suits most pictures. Leave the sizes empty to keep the original size.",
            ))
            .children(
                error
                    .read(cx)
                    .clone()
                    .map(|text| status_line("picture-settings-error", text).text_color(danger)),
            );
        let description = description.clone();
        let (path, quality, width, height, error) = (
            path.clone(),
            quality.clone(),
            width.clone(),
            height.clone(),
            error.clone(),
        );
        let (page_ok, page_cancel) = (page.clone(), page.clone());
        let (accept, cancel) = (accept.clone(), cancel.clone());
        alert
            .title(title)
            .description(form)
            .confirm()
            .ok_text("Add")
            .cancel_text("Cancel")
            .on_ok(move |_, window, cx| {
                match settings(
                    &path.read(cx).value(),
                    &quality.read(cx).value(),
                    &width.read(cx).value(),
                    &height.read(cx).value(),
                ) {
                    Ok(mut settings) => {
                        settings.description = description.read(cx).value().to_string();
                        page_ok
                            .update(cx, |this, cx| accept(this, settings, window, cx))
                            .ok();
                        true
                    }
                    Err(message) => {
                        error.update(cx, |value, cx| {
                            *value = Some(message.into());
                            cx.notify();
                        });
                        window.refresh();
                        false
                    }
                }
            })
            .on_cancel(move |_, window, cx| {
                page_cancel
                    .update(cx, |this, cx| cancel(this, window, cx))
                    .ok();
                true
            })
    });
}

fn labeled(label: &'static str, input: &Entity<InputState>) -> gpui_kit::Div {
    v_flex()
        .min_w_0()
        .gap_1()
        .child(div().child(label))
        .child(Input::new(input).aria_label(label))
}

fn settings(
    path: &str,
    quality: &str,
    width: &str,
    height: &str,
) -> Result<PictureSettings, &'static str> {
    let path = clean_path(path)?;
    let quality = quality
        .trim()
        .parse::<u8>()
        .ok()
        .filter(|value| (1..=100).contains(value))
        .ok_or("Enter a quality from 1 to 100.")?;
    let resolution = (dimension(width)?, dimension(height)?);
    Ok(PictureSettings {
        path,
        options: EncodeOptions {
            quality,
            resolution,
            recompress_avif: true,
            ..EncodeOptions::default()
        },
        description: String::new(),
    })
}

fn dimension(value: &str) -> Result<Option<u32>, &'static str> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse::<std::num::NonZeroU32>()
        .map(|value| Some(value.get()))
        .map_err(
            |_| "Enter a positive number of pixels or leave the field empty for original size.",
        )
}

#[cfg(test)]
mod tests {
    use super::settings;

    #[test]
    fn empty_dimensions_keep_the_original_size() {
        let settings = settings("img/pond.avif", "80", "", " ").unwrap();
        assert_eq!(settings.options.resolution, (None, None));
        assert_eq!(settings.options.quality, 80);
    }

    #[test]
    fn invalid_picture_settings_are_refused() {
        assert!(settings("img/pond.avif", "101", "", "").is_err());
        assert!(settings("img/pond.avif", "80", "0", "").is_err());
    }
}
