//! The sections of the cover, the pictures and the problems.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, ImageSource, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, Role, StatefulInteractiveElement as _, Styled as _, StyledImage as _,
    TestSupportExt as _, Window, div, img, px,
};

use super::EditorPage;
use super::encode::Target;
use super::prompts::confirm_remove;
use super::sections::{control, list_item};
use super::session::{Control, ViewMode, is_contents_problem, problem_words, size_label};
use crate::shared::ui::{icon_button, status_line};

impl EditorPage {
    /// The Cover section: the picture, and the control that replaces it.
    pub(super) fn render_cover(&mut self, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let title = self.session.title().unwrap_or("this book").to_owned();
        let has_cover = self.session.meta().is_some_and(|meta| meta.cover.is_some());
        let picture = match (&self.cover.image, has_cover) {
            (Some(image), _) => div()
                .id("cover-image")
                .role(Role::Image)
                .aria_label(format!("Cover of {title}"))
                .test_support()
                .child(
                    img(ImageSource::Render(image.clone()))
                        .max_w(px(360.0))
                        .rounded_md(),
                )
                .into_any_element(),
            (None, true) if self.cover.failed => {
                status_line("cover-state", "The cover could not be shown.").into_any_element()
            }
            (None, true) => status_line("cover-state", "Loading the cover").into_any_element(),
            (None, false) => {
                status_line("cover-state", "This book has no cover.").into_any_element()
            }
        };
        let encoding = self.encoder.names(Target::Cover);
        v_flex()
            .id("cover")
            .gap_3()
            .p_4()
            .child(picture)
            .children(
                encoding
                    .into_iter()
                    .map(|name| status_line("cover-encoding", format!("Encoding {name}…"))),
            )
            .child(
                control(
                    "replace-cover",
                    if has_cover {
                        "Replace…"
                    } else {
                        "Add a cover…"
                    },
                )
                .w(px(180.0))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.choose_pictures(Target::Cover, window, cx);
                })),
            )
            .children(
                self.session
                    .error_text(&Control::Cover)
                    .map(|message| status_line("cover-error", message).text_color(theme.danger)),
            )
            .into_any_element()
    }

    /// The picture gallery and the control that adds a file.
    pub(super) fn render_pictures(&mut self, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let gallery = h_flex()
            .id("pictures-list")
            .role(Role::Group)
            .aria_label("Pictures")
            .flex_wrap()
            .gap_3()
            .children(
                self.session
                    .pictures()
                    .iter()
                    .map(|picture| self.render_picture_card(picture, cx)),
            );
        let encoding = self.encoder.names(Target::Picture);
        let empty = self.session.pictures().is_empty() && encoding.is_empty();
        v_flex()
            .id("pictures")
            .gap_3()
            .p_4()
            .child(gallery)
            .children(
                encoding.into_iter().map(|name| {
                    status_line(format!("encoding-{name}"), format!("Encoding {name}…"))
                }),
            )
            .children(empty.then(|| status_line("no-pictures", "This book has no pictures.")))
            .child(
                control("add-picture", "Add a picture…")
                    .w(px(180.0))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.choose_pictures(Target::Picture, window, cx);
                    })),
            )
            .children(
                self.session
                    .error_text(&Control::Pictures)
                    .map(|message| status_line("pictures-error", message).text_color(theme.danger)),
            )
            .into_any_element()
    }

    fn render_picture_card(
        &self,
        picture: &crate::features::edit_book::outline::PictureOutline,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let resolution = format!("{} × {} px", picture.width, picture.height);
        let file_size = size_label(picture.len);
        let preview_height = px(96.0);
        let path = picture.path.clone();
        let thumbnail = self.picture_thumbnail(picture);
        v_flex()
            .id(format!("picture-{}", picture.path))
            .role(Role::Group)
            .aria_label(format!("{}, {resolution}, {file_size}", picture.path))
            .w(px(156.0))
            .max_w_full()
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .id(format!("preview-{}", picture.path))
                    .role(Role::Image)
                    .aria_label(format!("Preview of {}", picture.path))
                    .test_support()
                    .relative()
                    .flex_none()
                    .w_full()
                    .h(preview_height)
                    .overflow_hidden()
                    .bg(theme.muted)
                    .rounded_md()
                    .child(
                        img(thumbnail)
                            .object_fit(ObjectFit::Contain)
                            .absolute()
                            .top_0()
                            .left_0()
                            .w_full()
                            .h(preview_height),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .child(picture.path.clone()),
                    )
                    .child(
                        icon_button(
                            format!("remove-picture-{}", picture.path),
                            IconName::Trash,
                            format!("Remove {}", picture.path),
                        )
                        .on_click(cx.listener(move |_, _, window, cx| {
                            let path = path.clone();
                            let title = path.clone();
                            confirm_remove(&title, window, cx, move |this, window, cx| {
                                this.remove_picture(path.clone(), window, cx);
                            });
                        })),
                    ),
            )
            .child(
                v_flex()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(resolution)
                    .child(file_size),
            )
    }

    /// The Problems section: what the last check or save found, and the
    /// controls that check again and settle the contents.
    pub(super) fn render_problems(&mut self, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let problems = self.session.problems().to_vec();
        let contents = problems.iter().any(is_contents_problem);
        let mut list = v_flex()
            .id("problems-list")
            .role(Role::Group)
            .aria_label("Problems")
            .gap_1();
        for (index, problem) in problems.iter().enumerate() {
            let place = problem
                .path
                .clone()
                .zip(problem.offset)
                .and_then(|key| self.session.places.get(&key).copied());
            list = list.child(
                list_item(format!("problem-{index}"), problem_words(problem, place))
                    .ghost()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_problem(index, window, cx);
                    })),
            );
        }
        let summary = if problems.is_empty() {
            "No problems were found. Check the book to look again.".to_owned()
        } else {
            let noun = if problems.len() == 1 {
                "problem"
            } else {
                "problems"
            };
            format!(
                "The book has {} {noun} and cannot be saved until they are fixed.",
                problems.len()
            )
        };
        v_flex()
            .id("problems")
            .gap_3()
            .p_4()
            .child(status_line("problems-summary", summary))
            .child(list)
            .child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        control("check", "Check the book")
                            .on_click(cx.listener(|this, _, window, cx| this.check(window, cx))),
                    )
                    .children(contents.then(|| {
                        control("rebuild-contents", "Rebuild the contents from the headings")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.rebuild_contents(window, cx);
                            }))
                    })),
            )
            .children(
                self.session
                    .error_text(&Control::Contents)
                    .map(|message| status_line("contents-error", message).text_color(theme.danger)),
            )
            .into_any_element()
    }

    /// Opens the chapter a problem is in, with the caret at its place.
    pub(super) fn open_problem(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(problem) = self.session.problems().get(index).cloned() else {
            return;
        };
        let Some(path) = problem.path else {
            return;
        };
        if matches!(self.session.mode, ViewMode::Preview | ViewMode::Visual) {
            self.set_mode(ViewMode::Source, window, cx);
        }
        let offset = problem
            .offset
            .and_then(|offset| usize::try_from(offset).ok());
        self.open_chapter(&path, window, cx);
        if let Some(offset) = offset {
            if self.loaded.as_deref() == Some(path.as_str()) {
                self.move_caret(offset, window, cx);
            } else {
                self.caret = Some(offset);
            }
        }
    }
}
