//! The diagram and formula sections and their chapter insertion controls.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, StatefulInteractiveElement as _, Styled as _, Task,
    TestSupportExt as _, Window, div, img, px,
};

use super::EditorPage;
use super::commands::Origin;
use super::markup::picture_markdown;
use super::prompts::{Question, confirm};
use super::sections::{control, list_item};
use super::session::{Control, Section, edit_error_words};
use crate::entities::figure::{Figure, FigureError, FigureKind, Palette, Setting, render};
use crate::features::appearance::Appearance;
use crate::features::edit_book::command::Command;
use crate::shared::ui::{announce, status_line};

impl EditorPage {
    pub(super) fn render_figures(
        &self,
        kind: FigureKind,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let figures = self
            .session
            .figures()
            .iter()
            .filter(|figure| figure.kind == kind);
        let noun = kind.noun();
        let mut panel = v_flex().p_4().gap_2().child(
            h_flex().child(
                control("new-figure", format!("New {noun}…"))
                    .primary()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.figure_edit = None;
                        let path = this.next_figure_path(kind);
                        ask_source(kind, path, None, window, cx);
                    })),
            ),
        );
        if let Some(error) = self.session.error_text(&Control::Figures(kind)) {
            panel = panel.child(status_line("figure-error", error).text_color(cx.theme().danger));
        }
        let mut figures = figures.peekable();
        if figures.peek().is_none() {
            panel = panel.child(status_line(
                "no-figures",
                format!(
                    "This book has no {} yet.",
                    Section::Figures(kind).label().to_lowercase()
                ),
            ));
        }
        panel.children(figures.map(|figure| {
            let edit_path = figure.path.clone();
            let remove_path = figure.path.clone();
            h_flex()
                .gap_2()
                .flex_wrap()
                .items_center()
                .child(div().flex_1().min_w_0().truncate().child(figure.path.clone()))
                .child(
                    control(format!("edit-figure-{}", figure.path), "Edit…")
                        .ghost()
                        .accessibility_label(format!("Edit {noun} {}", figure.path))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.edit_figure(kind, edit_path.clone(), window, cx);
                        })),
                )
                .child(
                    control(format!("remove-figure-{}", figure.path), "Remove…")
                        .ghost()
                        .accessibility_label(format!("Remove {noun} {}", figure.path))
                        .on_click(cx.listener(move |_, _, window, cx| {
                            let path = remove_path.clone();
                            confirm(
                                Question {
                                    title: format!("Remove {noun} {path}?"),
                                    detail: "References in chapters will need to be removed or updated before saving."
                                        .to_owned(),
                                    ok: "Remove",
                                },
                                window,
                                cx,
                                move |this, window, cx| {
                                    this.send(
                                        Command::RemoveFigure { kind, path: path.clone() },
                                        Origin::Figure(kind),
                                        window,
                                        cx,
                                    );
                                },
                            );
                        })),
                )
        }))
        .into_any_element()
    }

    fn next_figure_path(&self, kind: FigureKind) -> String {
        let (folder, extension) = kind.home();
        let used = |path: &str| {
            self.session
                .chapters()
                .iter()
                .any(|chapter| chapter.path == path)
                || self
                    .session
                    .pictures()
                    .iter()
                    .any(|picture| picture.path == path)
                || self
                    .session
                    .figures()
                    .iter()
                    .any(|figure| figure.path == path)
        };
        let stem = kind.noun();
        let mut path = format!("{folder}/{stem}.{extension}");
        let mut number = 1;
        while used(&path) {
            number += 1;
            path = format!("{folder}/{stem}-{number}.{extension}");
        }
        path
    }

    fn edit_figure(
        &mut self,
        kind: FigureKind,
        path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let reply = self.handle.figure(kind, &path);
        self.figure_edit = Some(cx.spawn_in(window, async move |this, cx| {
            let result = reply.await;
            this.update_in(cx, |this, window, cx| {
                if this.session.section != Section::Figures(kind) {
                    return;
                }
                match result {
                    Ok(source) => ask_source(kind, path, Some(&source), window, cx),
                    Err(error) => {
                        this.session
                            .set_error(Control::Figures(kind), edit_error_words(&error));
                        cx.notify();
                    }
                }
            })
            .ok();
        }));
    }

    /// Inserts a reference to an existing figure at the captured Visual selection.
    pub(super) fn ask_chapter_figure(
        &self,
        kind: FigureKind,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(chapter) = self.loaded.clone() else {
            return;
        };
        let Ok(selection) = self.visual.input.read(cx).capture_selection() else {
            return;
        };
        let paths: Rc<[String]> = self
            .session
            .figures()
            .iter()
            .filter(|figure| figure.kind == kind)
            .map(|figure| figure.path.clone())
            .collect();
        let noun = kind.noun();
        if paths.is_empty() {
            announce(
                format!(
                    "Add a {noun} in the {} section first.",
                    Section::Figures(kind).label()
                ),
                window,
                cx,
            );
            return;
        }
        let selected = Rc::new(Cell::new(0));
        let description = cx.new(|cx| InputState::new(window, cx));
        let first = Cell::new(true);
        let page = cx.weak_entity();
        window.open_alert_dialog(cx, move |alert, window, cx| {
            if first.replace(false) {
                description.focus_handle(cx).focus(window, cx);
            }
            let choices = v_flex()
                .id("figure-choices")
                .role(Role::Group)
                .aria_label(Section::Figures(kind).label())
                .max_h(px(170.0))
                .overflow_y_scroll()
                .children(paths.iter().enumerate().map(|(index, path)| {
                    let selected = selected.clone();
                    list_item(format!("figure-choice-{index}"), path.clone())
                        .ghost()
                        .selected(selected.get() == index)
                        .on_click(move |_, window, _| {
                            selected.set(index);
                            window.refresh();
                        })
                }));
            let form = v_flex()
                .gap_2()
                .child(choices)
                .child(div().child("Alternative text"))
                .child(Input::new(&description).aria_label("Alternative text"));
            let (page, paths, selected) = (page.clone(), paths.clone(), selected.clone());
            let (description, chapter, selection) =
                (description.clone(), chapter.clone(), selection.clone());
            alert
                .title(format!("Insert a {noun}"))
                .description(form)
                .confirm()
                .ok_text(format!("Insert {noun}"))
                .cancel_text("Cancel")
                .on_ok(move |_, window, cx| {
                    let markdown = picture_markdown(
                        &chapter,
                        &paths[selected.get()],
                        &description.read(cx).value(),
                    );
                    page.update(cx, |this, cx| {
                        if this.visual_is_editing(&chapter, cx)
                            && this.visual.input.read(cx).is_capture_current(&selection)
                        {
                            this.insert_markdown(markdown, window, cx);
                        }
                    })
                    .ok();
                    true
                })
        });
    }
}

/// A preview belongs to the exact source passed to the renderer.
#[derive(Default)]
struct SourcePreview {
    source: String,
    drawing: Option<Result<Arc<Figure>, FigureError>>,
    task: Option<Task<()>>,
}

fn ask_source(
    kind: FigureKind,
    path: String,
    initial: Option<&str>,
    window: &mut Window,
    cx: &mut Context<'_, EditorPage>,
) {
    let editing = initial.is_some();
    let path_input = cx.new(|cx| {
        let mut state = InputState::new(window, cx);
        state.set_value(path.as_str(), window, cx);
        state
    });
    let source_input = cx.new(|cx| {
        let mut state = TextareaState::new(window, cx).rows(5);
        if let Some(source) = initial {
            state.set_value(source, window, cx);
        }
        state
    });
    let preview = cx.new(|_| SourcePreview::default());
    let error = cx.new(|_| None::<&'static str>);
    let first = Cell::new(true);
    let page = cx.weak_entity();
    let noun = kind.noun();
    let source_label = match kind {
        FigureKind::Diagram => "Mermaid source",
        FigureKind::Formula => "KaTeX source",
    };
    window.open_alert_dialog(cx, move |alert, window, cx| {
        if first.replace(false) {
            source_input.focus_handle(cx).focus(window, cx);
        }
        let width = (window.viewport_size().width.as_f32() - 48.0).clamp(240.0, 570.0);
        let (preview_button, source_button) = (preview.clone(), source_input.clone());
        let tone = Appearance::global(cx).read(cx).page_tone(window);
        let source = source_input.read(cx).value();
        let form = v_flex()
            .gap_2()
            .children((!editing).then(|| {
                v_flex()
                    .gap_1()
                    .child(div().child("Book path"))
                    .child(Input::new(&path_input).aria_label("Book path"))
            }))
            .child(div().child(source_label))
            .child(Textarea::new(&source_input).aria_label(source_label))
            .child(
                h_flex().child(control("draw-figure", "Preview").ghost().on_click(
                    move |_, window, cx| {
                        let source = source_button.read(cx).value().to_string();
                        preview_button.update(cx, |preview, cx| {
                            preview.draw(kind, source, tone.into(), window, cx);
                        });
                        window.refresh();
                    },
                )),
            )
            .children(preview.read(cx).display(&source, width - 56.0, window, cx))
            .children(error.read(cx).map(|message| {
                status_line("figure-draft-error", message).text_color(cx.theme().danger)
            }));
        let (page, path_input, source_input, error) = (
            page.clone(),
            path_input.clone(),
            source_input.clone(),
            error.clone(),
        );
        alert
            .title(if editing {
                format!("Edit {path}")
            } else {
                format!("New {noun}")
            })
            .width(px(width))
            .description(form)
            .confirm()
            .ok_text(if editing { "Apply" } else { "Add" })
            .cancel_text("Cancel")
            .on_ok(move |_, window, cx| {
                let path = path_input.read(cx).value().trim().to_owned();
                let source = source_input.read(cx).value().to_string();
                let Ok(command) =
                    figure_command(kind, editing, path, source).inspect_err(|&message| {
                        error.update(cx, |error, cx| {
                            *error = Some(message);
                            cx.notify();
                        });
                        window.refresh();
                    })
                else {
                    return false;
                };
                page.update(cx, |this, cx| {
                    this.send(command, Origin::Figure(kind), window, cx);
                })
                .ok();
                true
            })
    });
}

fn figure_command(
    kind: FigureKind,
    editing: bool,
    path: String,
    source: String,
) -> Result<Command, &'static str> {
    if path.is_empty() {
        return Err("Enter a book path.");
    }
    if source.trim().is_empty() {
        return Err("Enter the source.");
    }
    if editing {
        Ok(Command::SetFigureSource { kind, path, source })
    } else {
        Ok(Command::AddFigure { kind, path, source })
    }
}

impl SourcePreview {
    fn draw(
        &mut self,
        kind: FigureKind,
        source: String,
        palette: Palette,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.source.clone_from(&source);
        self.drawing = None;
        let drawing =
            cx.background_spawn(async move { render(kind, &source, Setting::Block, palette) });
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let drawing = drawing.await;
            this.update_in(cx, |this, window, cx| {
                this.drawing = Some(drawing);
                cx.notify();
                window.refresh();
            })
            .ok();
        }));
    }

    fn display(
        &self,
        source: &str,
        max_width: f32,
        window: &Window,
        cx: &gpui_kit::App,
    ) -> Option<AnyElement> {
        if self.source != source || self.task.is_none() {
            return None;
        }
        let content = match &self.drawing {
            None => status_line("figure-preview-loading", "Rendering preview…").into_any_element(),
            Some(Err(error)) => status_line("figure-preview-error", error.to_string())
                .text_color(cx.theme().danger)
                .into_any_element(),
            Some(Ok(figure)) => {
                let scale = (max_width / figure.width)
                    .min(180.0 / figure.height)
                    .min(1.0);
                let (paper, _) = crate::shared::ui::page_colors(
                    Appearance::global(cx).read(cx).page_tone(window),
                );
                h_flex()
                    .id("figure-source-preview")
                    .role(Role::Image)
                    .aria_label("Figure preview")
                    .test_support()
                    .w_full()
                    .justify_center()
                    .bg(paper)
                    .child(
                        img(figure.image(scale * window.scale_factor()))
                            .w(px(figure.width * scale))
                            .h(px(figure.height * scale)),
                    )
                    .into_any_element()
            }
        };
        Some(content)
    }
}
