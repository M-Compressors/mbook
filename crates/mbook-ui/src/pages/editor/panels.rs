//! The sections of the cover, the pictures and the problems.

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, ImageSource, InteractiveElement as _, IntoElement, ParentElement as _,
    Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, img, px,
};

use super::EditorPage;
use super::encode::Target;
use super::prompts::confirm_remove;
use super::sections::{control, list_item};
use super::session::{Control, ViewMode, is_contents_problem, problem_words, size_label};
use crate::shared::ui::status_line;

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

    /// The Pictures section: the list, and the control that adds one.
    pub(super) fn render_pictures(&mut self, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let mut list = v_flex()
            .id("pictures-list")
            .role(Role::Group)
            .aria_label("Pictures")
            .gap_1();
        for picture in self.session.pictures().to_vec() {
            let summary = format!(
                "{}, {} by {} pixels, {}",
                picture.path,
                picture.width,
                picture.height,
                size_label(picture.len)
            );
            let path = picture.path.clone();
            list = list.child(
                h_flex()
                    .id(format!("picture-{}", picture.path))
                    .gap_2()
                    .items_center()
                    .child(div().flex_1().min_w_0().truncate().child(summary))
                    .child(
                        control(format!("remove-picture-{}", picture.path), "Remove")
                            .ghost()
                            .accessibility_label(format!("Remove {}", picture.path))
                            .on_click(cx.listener(move |_, _, window, cx| {
                                let path = path.clone();
                                confirm_remove(
                                    &path.clone(),
                                    window,
                                    cx,
                                    move |this, window, cx| {
                                        this.remove_picture(path.clone(), window, cx);
                                    },
                                );
                            })),
                    ),
            );
        }
        for name in self.encoder.names(Target::Picture) {
            list = list.child(status_line(
                format!("encoding-{name}"),
                format!("Encoding {name}…"),
            ));
        }
        if self.session.pictures().is_empty() && self.encoder.names(Target::Picture).is_empty() {
            list = list.child(status_line("no-pictures", "This book has no pictures."));
        }
        v_flex()
            .id("pictures")
            .gap_3()
            .p_4()
            .child(list)
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
