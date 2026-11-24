//! The diagram and formula sections and their chapter insertion controls.

use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Once};

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::highlighter::{GrammarConfig, LanguageRegistry};
use gpui_kit::component::input::{Editor, EditorState, Input, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Selectable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, Task, TestSupportExt as _, Window, div, img, px,
};
use rust_i18n::t;

use super::EditorPage;
use super::commands::Origin;
use super::markup::picture_markdown;
use super::prompts::{Question, confirm};
use super::sections::{control, list_item};
use super::session::{Control, Section, edit_error_words};
use crate::entities::figure::{Figure, FigureError, FigureKind, Palette, Setting, render};
use crate::features::appearance::Appearance;
use crate::features::edit_book::command::Command;
use crate::shared::compute;
use crate::shared::theme::PageTone;
use crate::shared::ui::{announce, status_line, target_size};

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
        let words = FigureWords::of(kind);
        let mut panel =
            v_flex().p_4().gap_2().child(
                h_flex().child(control("new-figure", words.new_more).primary().on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.figure_edit = None;
                        let path = this.next_figure_path(kind);
                        ask_source(kind, &path, None, window, cx);
                    }),
                )),
            );
        if let Some(error) = self.session.error_text(&Control::Figures(kind)) {
            panel = panel.child(status_line("figure-error", error).text_color(cx.theme().danger));
        }
        let mut figures = figures.peekable();
        if figures.peek().is_none() {
            panel = panel.child(status_line("no-figures", words.none));
        }
        panel
            .children(figures.map(|figure| {
                let edit_path = figure.path.clone();
                let remove_path = figure.path.clone();
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(figure.path.clone()),
                    )
                    .child(
                        control(
                            format!("edit-figure-{}", figure.path),
                            t!("prompt.figure.edit"),
                        )
                        .ghost()
                        .accessibility_label(FigureWords::edit_named(kind, &figure.path))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.edit_figure(kind, edit_path.clone(), window, cx);
                            },
                        )),
                    )
                    .child(
                        control(
                            format!("remove-figure-{}", figure.path),
                            t!("prompt.figure.remove"),
                        )
                        .ghost()
                        .accessibility_label(FigureWords::remove_named(kind, &figure.path))
                        .on_click(cx.listener(move |_, _, window, cx| {
                            let path = remove_path.clone();
                            confirm(
                                Question {
                                    title: FigureWords::remove_question(kind, &path),
                                    detail: t!("prompt.figure.remove_detail").into_owned(),
                                    ok: t!("prompt.remove.ok").into(),
                                },
                                window,
                                cx,
                                move |this, window, cx| {
                                    this.send(
                                        Command::RemoveFigure {
                                            kind,
                                            path: path.clone(),
                                        },
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
                    Ok(source) => ask_source(kind, &path, Some(&source), window, cx),
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
        let words = FigureWords::of(kind);
        if paths.is_empty() {
            announce(words.add_first, window, cx);
            return;
        }
        let selected = Rc::new(Cell::new(0));
        let default_description = SharedString::from(words.name);
        let description =
            cx.new(|cx| InputState::new(window, cx).placeholder(default_description.clone()));
        let (section, insert_title, insert) = (
            SharedString::from(words.section),
            SharedString::from(words.insert_title),
            SharedString::from(words.insert),
        );
        let first = Cell::new(true);
        let page = cx.weak_entity();
        window.open_alert_dialog(cx, move |alert, window, cx| {
            if first.replace(false) {
                description.focus_handle(cx).focus(window, cx);
            }
            let choices = v_flex()
                .id("figure-choices")
                .role(Role::Group)
                .aria_label(section.clone())
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
                .child(div().child(SharedString::from(t!("prompt.figure.alt_text"))))
                .child(Input::new(&description).aria_label(t!("prompt.figure.alt_text")));
            let (page, paths, selected) = (page.clone(), paths.clone(), selected.clone());
            let (description, chapter, selection) =
                (description.clone(), chapter.clone(), selection.clone());
            let default_description = default_description.clone();
            alert
                .title(insert_title.clone())
                .description(form)
                .confirm()
                .ok_text(insert.clone())
                .cancel_text(t!("prompt.cancel"))
                .on_ok(move |_, window, cx| {
                    let description = description.read(cx).value();
                    let alt = if description.trim().is_empty() {
                        &default_description
                    } else {
                        &description
                    };
                    let markdown = picture_markdown(&chapter, &paths[selected.get()], alt);
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

/// The words of the figures of one kind, which a language may inflect
/// differently for a diagram and a formula.
struct FigureWords {
    /// The control that makes a new one, which goes on in a dialog.
    new_more: Cow<'static, str>,
    /// The title of the dialog that makes a new one.
    new_title: Cow<'static, str>,
    /// What the section says while the book has none.
    none: Cow<'static, str>,
    /// What the editor says when one is to be inserted and the book has none.
    add_first: Cow<'static, str>,
    /// The name of the section, over the list of them.
    section: Cow<'static, str>,
    /// The alternative text of one inserted without a description.
    name: Cow<'static, str>,
    /// The title of the dialog that inserts one into a chapter.
    insert_title: Cow<'static, str>,
    /// The control that inserts it.
    insert: Cow<'static, str>,
}

impl FigureWords {
    fn of(kind: FigureKind) -> Self {
        match kind {
            FigureKind::Diagram => Self {
                new_more: t!("prompt.figure.diagram.new_more"),
                new_title: t!("prompt.figure.diagram.new_title"),
                none: t!("prompt.figure.diagram.none"),
                add_first: t!("prompt.figure.diagram.add_first"),
                section: t!("prompt.figure.diagram.section"),
                name: t!("prompt.figure.diagram.name"),
                insert_title: t!("prompt.figure.diagram.insert_title"),
                insert: t!("prompt.figure.diagram.insert"),
            },
            FigureKind::Formula => Self {
                new_more: t!("prompt.figure.formula.new_more"),
                new_title: t!("prompt.figure.formula.new_title"),
                none: t!("prompt.figure.formula.none"),
                add_first: t!("prompt.figure.formula.add_first"),
                section: t!("prompt.figure.formula.section"),
                name: t!("prompt.figure.formula.name"),
                insert_title: t!("prompt.figure.formula.insert_title"),
                insert: t!("prompt.figure.formula.insert"),
            },
        }
    }

    /// What assistive technology hears for the control that edits one.
    fn edit_named(kind: FigureKind, path: &str) -> Cow<'static, str> {
        match kind {
            FigureKind::Diagram => t!("prompt.figure.diagram.edit_named", path = path),
            FigureKind::Formula => t!("prompt.figure.formula.edit_named", path = path),
        }
    }

    /// What assistive technology hears for the control that removes one.
    fn remove_named(kind: FigureKind, path: &str) -> Cow<'static, str> {
        match kind {
            FigureKind::Diagram => t!("prompt.figure.diagram.remove_named", path = path),
            FigureKind::Formula => t!("prompt.figure.formula.remove_named", path = path),
        }
    }

    /// The question before one is removed.
    fn remove_question(kind: FigureKind, path: &str) -> String {
        match kind {
            FigureKind::Diagram => t!("prompt.figure.diagram.remove_question", path = path),
            FigureKind::Formula => t!("prompt.figure.formula.remove_question", path = path),
        }
        .into_owned()
    }
}

/// A preview belongs to the exact source passed to the renderer.
#[derive(Default)]
struct SourcePreview {
    source: String,
    drawing: Option<Result<Arc<Figure>, FigureError>>,
    task: Option<Task<()>>,
    /// The drawing is shown at its size in the text, and scrolls, instead of
    /// being fitted to the preview.
    actual: bool,
}

/// Where the parts of the dialog that makes or edits a figure go in a
/// window: the dialog takes most of it, and in a wide one the drawing stands
/// beside the source, so that a large diagram can be read while it is
/// written.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SourceLayout {
    /// The width of the dialog.
    width: f32,
    /// The drawing stands beside the source instead of under it.
    beside: bool,
    /// The height of the source field.
    source_height: f32,
    /// The size of the box the drawing is shown in.
    preview_width: f32,
    preview_height: f32,
}

impl SourceLayout {
    /// The narrowest and the widest dialog.
    const MIN_WIDTH: f32 = 240.0;
    const MAX_WIDTH: f32 = 1280.0;
    /// The width of the window the dialog leaves free, on both sides
    /// together, and the height it leaves free above and below.
    const EDGE: f32 = 48.0;
    /// The padding of the dialog on both sides together.
    const INSET: f32 = 32.0;
    /// From this dialog width on the drawing stands beside the source.
    const BESIDE_FROM: f32 = 800.0;
    /// The share of the width the drawing takes beside the source.
    const PREVIEW_SHARE: f32 = 0.6;
    /// The space between the source and the drawing.
    const GAP: f32 = 12.0;
    /// The height of the title, the label of the source, the footer and the
    /// paddings and gaps between them, without the controls.
    const CHROME: f32 = 168.0;
    /// The height of the label and the field of the book path, without the
    /// field's control height.
    const PATH_LABEL: f32 = 32.0;
    /// The smallest source field and the smallest drawing.
    const MIN_SOURCE: f32 = 120.0;
    const MIN_PREVIEW: f32 = 120.0;
    /// The largest share of the height the source takes over the drawing.
    const SOURCE_SHARE: f32 = 0.35;
    /// How much larger than in the text a small drawing is shown.
    const MAX_ENLARGE: f32 = 2.0;
    /// The space around the drawing inside its box.
    const PADDING: f32 = 8.0;

    /// The layout for a window of `width` by `height`, for a dialog that
    /// asks for the book path when `with_path`, and whose controls are
    /// `control` high.
    fn of(width: f32, height: f32, with_path: bool, control: f32) -> Self {
        let dialog = (width - Self::EDGE).clamp(Self::MIN_WIDTH, Self::MAX_WIDTH);
        let inner = dialog - Self::INSET;
        let path = if with_path {
            Self::PATH_LABEL + control
        } else {
            0.0
        };
        // The source field and the row of controls under it.
        let body = (height - Self::EDGE * 2.0 - Self::CHROME - control * 2.0 - path)
            .max(Self::MIN_SOURCE + Self::MIN_PREVIEW);
        if dialog >= Self::BESIDE_FROM {
            let preview_width = ((inner - Self::GAP) * Self::PREVIEW_SHARE).floor();
            Self {
                width: dialog,
                beside: true,
                source_height: body,
                preview_width,
                // Level with the label, the field and the controls.
                preview_height: body + control + 28.0,
            }
        } else {
            let source_height = (body * Self::SOURCE_SHARE)
                .max(Self::MIN_SOURCE)
                .min(body - Self::MIN_PREVIEW);
            Self {
                width: dialog,
                beside: false,
                source_height,
                preview_width: inner,
                preview_height: body - source_height,
            }
        }
    }

    /// The scale at which a drawing of `width` by `height` fills the box of
    /// the preview: smaller for a large one, up to twice the size for a
    /// small one.
    fn fit(&self, width: f32, height: f32) -> f32 {
        let room_width = self.preview_width - Self::PADDING * 2.0;
        let room_height = self.preview_height - Self::PADDING * 2.0;
        (room_width / width)
            .min(room_height / height)
            .clamp(0.0, Self::MAX_ENLARGE)
    }

    /// The fitted scale of a drawing that is shown smaller than it is, the
    /// one that can be seen at its own size instead.
    fn zoom(&self, width: f32, height: f32) -> Option<f32> {
        let fit = self.fit(width, height);
        (fit < 1.0).then_some(fit)
    }
}

const MERMAID: &str = "mermaid";
const LATEX: &str = "latex";

/// Registers the source grammars before the first dialog needs them.
fn register_source_grammars() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let registry = LanguageRegistry::singleton();
        registry.register(
            MERMAID,
            &GrammarConfig::new(
                MERMAID,
                tree_sitter_mermaid::LANGUAGE.into(),
                Vec::new(),
                tree_sitter_mermaid::HIGHLIGHTS_QUERY,
                "",
                "",
            ),
        );
        registry.register(
            LATEX,
            &GrammarConfig::new(
                LATEX,
                codebook_tree_sitter_latex::LANGUAGE.into(),
                Vec::new(),
                r#"
                    (command_name) @function
                    [(line_comment) (block_comment)] @comment
                    [(operator) "_" "^"] @operator
                    ["{" "}" "[" "]"] @punctuation.bracket
                "#,
                "",
                "",
            ),
        );
    });
}

/// The grammar of the source of a figure, the label of its field, and the
/// title of the dialog that makes or edits the figure at `path`.
fn source_words(
    kind: FigureKind,
    path: &str,
    editing: bool,
) -> (&'static str, SharedString, SharedString) {
    let (language, label) = match kind {
        FigureKind::Diagram => (MERMAID, t!("prompt.figure.mermaid_source")),
        FigureKind::Formula => (LATEX, t!("prompt.figure.katex_source")),
    };
    let title = if editing {
        t!("prompt.figure.edit_title", path = path)
    } else {
        FigureWords::of(kind).new_title
    };
    (language, label.into(), title.into())
}

fn ask_source(
    kind: FigureKind,
    path: &str,
    initial: Option<&str>,
    window: &mut Window,
    cx: &mut Context<'_, EditorPage>,
) {
    register_source_grammars();
    let editing = initial.is_some();
    let initial = initial.map(str::to_owned);
    let (language, source_label, title) = source_words(kind, path, editing);
    let path_input = cx.new(|cx| {
        let mut state = InputState::new(window, cx);
        state.set_value(path, window, cx);
        state
    });
    let source_input = cx.new(|cx| {
        EditorState::new(window, cx)
            .language(language)
            .default_value(initial.clone().unwrap_or_default())
            .searchable(false)
            .folding(false)
            .line_number(false)
            .soft_wrap(true)
    });
    let preview = cx.new(|_| SourcePreview::default());
    let error = cx.new(|_| None::<SharedString>);
    let first = Cell::new(true);
    let page = cx.weak_entity();
    window.open_alert_dialog(cx, move |alert, window, cx| {
        let tone = Appearance::global(cx).read(cx).page_tone(window);
        if first.replace(false) {
            source_input.focus_handle(cx).focus(window, cx);
            // A figure that is edited is drawn at once: its source is known.
            if let Some(initial) = initial.as_ref() {
                preview.update(cx, |preview, cx| {
                    preview.draw(kind, initial.clone(), tone.into(), window, cx);
                });
            }
        }
        let viewport = window.viewport_size();
        let layout = SourceLayout::of(
            viewport.width.as_f32(),
            viewport.height.as_f32(),
            !editing,
            target_size(),
        );
        let source = source_input.read(cx).value();
        let buttons = preview_controls(kind, tone, &layout, &preview, &source_input, cx);
        let drawing = preview.read(cx).display(&source, &layout, window, cx);
        let body = source_body(&layout, &source_label, &source_input, buttons, drawing);
        let form = v_flex()
            .gap_2()
            .children((!editing).then(|| {
                v_flex()
                    .gap_1()
                    .child(div().child(SharedString::from(t!("prompt.figure.book_path"))))
                    .child(Input::new(&path_input).aria_label(t!("prompt.figure.book_path")))
            }))
            .child(body)
            .children(error.read(cx).clone().map(|message| {
                status_line("figure-draft-error", message).text_color(cx.theme().danger)
            }));
        let (page, path_input, source_input, error) = (
            page.clone(),
            path_input.clone(),
            source_input.clone(),
            error.clone(),
        );
        alert
            .title(title.clone())
            .width(px(layout.width))
            .description(form)
            .confirm()
            .ok_text(if editing {
                t!("prompt.apply")
            } else {
                t!("prompt.add")
            })
            .cancel_text(t!("prompt.cancel"))
            .on_ok(move |_, window, cx| {
                let path = path_input.read(cx).value().trim().to_owned();
                let source = source_input.read(cx).value().to_string();
                let Ok(command) =
                    figure_command(kind, editing, path, source).inspect_err(|message| {
                        error.update(cx, |error, cx| {
                            *error = Some(message.clone().into());
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

/// The source field with its controls, and the drawing beside or under it.
fn source_body(
    layout: &SourceLayout,
    label: &SharedString,
    source_input: &Entity<EditorState>,
    buttons: gpui_kit::Div,
    drawing: AnyElement,
) -> AnyElement {
    let source_column = v_flex()
        .gap_2()
        .min_w_0()
        .when(layout.beside, gpui_kit::Styled::flex_1)
        .child(div().child(label.clone()))
        .child(
            Editor::new(source_input)
                .h(px(layout.source_height))
                .aria_label(label.clone()),
        )
        .child(buttons);
    if layout.beside {
        h_flex()
            .gap(px(SourceLayout::GAP))
            .items_start()
            .child(source_column)
            .child(drawing)
            .into_any_element()
    } else {
        v_flex()
            .gap_2()
            .child(source_column)
            .child(drawing)
            .into_any_element()
    }
}

/// The controls under the source: **Preview**, and the one that shows a
/// large drawing at its own size or fits it again.
fn preview_controls(
    kind: FigureKind,
    tone: PageTone,
    layout: &SourceLayout,
    preview: &Entity<SourcePreview>,
    source_input: &Entity<EditorState>,
    cx: &gpui_kit::App,
) -> gpui_kit::Div {
    let (preview_button, source_button) = (preview.clone(), source_input.clone());
    let zoom_button = preview.clone();
    let source = source_input.read(cx).value();
    let actual = preview.read(cx).actual;
    let zoom = preview
        .read(cx)
        .shown(&source)
        .and_then(|figure| layout.zoom(figure.width, figure.height))
        .map(|_| {
            control(
                "figure-zoom",
                if actual {
                    t!("prompt.figure.fit")
                } else {
                    t!("prompt.figure.actual_size")
                },
            )
            .ghost()
            .selected(actual)
            .on_click(move |_, window, cx| {
                zoom_button.update(cx, |preview, cx| {
                    preview.actual = !preview.actual;
                    cx.notify();
                });
                window.refresh();
            })
        });
    h_flex()
        .gap_2()
        .flex_wrap()
        .child(
            control("draw-figure", t!("prompt.figure.preview"))
                .ghost()
                .on_click(move |_, window, cx| {
                    let source = source_button.read(cx).value().to_string();
                    preview_button.update(cx, |preview, cx| {
                        preview.draw(kind, source, tone.into(), window, cx);
                    });
                    window.refresh();
                }),
        )
        .children(zoom)
}

fn figure_command(
    kind: FigureKind,
    editing: bool,
    path: String,
    source: String,
) -> Result<Command, Cow<'static, str>> {
    if path.is_empty() {
        return Err(t!("prompt.figure.no_path"));
    }
    if source.trim().is_empty() {
        return Err(t!("prompt.figure.no_source"));
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
        self.actual = false;
        let drawing = cx.background_spawn(compute::spawn(move || {
            render(kind, &source, Setting::Block, palette)
        }));
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

    /// The drawing of `source`, if that is what was drawn last.
    fn shown(&self, source: &str) -> Option<&Arc<Figure>> {
        match &self.drawing {
            Some(Ok(figure)) if self.source == source => Some(figure),
            _ => None,
        }
    }

    /// The box of the preview: the drawing of `source` once it is drawn, the
    /// renderer's error, or what to do to see it.
    fn display(
        &self,
        source: &str,
        layout: &SourceLayout,
        window: &Window,
        cx: &gpui_kit::App,
    ) -> AnyElement {
        let (paper, _) =
            crate::shared::ui::page_colors(Appearance::global(cx).read(cx).page_tone(window));
        let frame = div()
            .id("figure-preview-box")
            .test_support()
            .flex_none()
            .w(px(layout.preview_width))
            .h(px(layout.preview_height))
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border);
        let current = self.source == source && self.task.is_some();
        let message = |id: &'static str, words: SharedString| {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .p(px(SourceLayout::PADDING))
                .child(status_line(id, words))
        };
        let content = match &self.drawing {
            _ if !current => message(
                "figure-preview-hint",
                t!("prompt.figure.preview_hint").into(),
            )
            .into_any_element(),
            None => message(
                "figure-preview-loading",
                t!("prompt.figure.rendering").into(),
            )
            .into_any_element(),
            Some(Err(error)) => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .p(px(SourceLayout::PADDING))
                .child(
                    status_line("figure-preview-error", error.to_string())
                        .text_color(cx.theme().danger),
                )
                .into_any_element(),
            Some(Ok(figure)) => {
                let actual = self.actual && layout.zoom(figure.width, figure.height).is_some();
                let scale = if actual {
                    1.0
                } else {
                    layout.fit(figure.width, figure.height)
                };
                let drawing = div()
                    .id("figure-source-preview")
                    .role(Role::Image)
                    .aria_label(t!("prompt.figure.preview_label"))
                    .test_support()
                    .flex_none()
                    .child(
                        img(figure.image(scale * window.scale_factor()))
                            .w(px(figure.width * scale))
                            .h(px(figure.height * scale)),
                    );
                let room = div()
                    .id("figure-preview-room")
                    .size_full()
                    .p(px(SourceLayout::PADDING));
                if actual {
                    room.overflow_scrollbar().child(drawing).into_any_element()
                } else {
                    room.flex()
                        .items_center()
                        .justify_center()
                        .child(drawing)
                        .into_any_element()
                }
            }
        };
        frame
            .bg(paper)
            .overflow_hidden()
            .child(content)
            .into_any_element()
    }
}

#[cfg(test)]
mod words_tests {
    use super::{FigureKind, FigureWords, LATEX, MERMAID, source_words};

    #[test]
    fn a_new_figure_is_titled_by_its_kind_and_an_edited_one_by_its_path() {
        let (language, label, title) = source_words(FigureKind::Diagram, "dia/a.mmd", false);
        assert_eq!(
            (language, label.as_ref(), title.as_ref()),
            (MERMAID, "Mermaid source", "New diagram")
        );
        let (language, label, title) = source_words(FigureKind::Formula, "math/a.tex", true);
        assert_eq!(
            (language, label.as_ref(), title.as_ref()),
            (LATEX, "KaTeX source", "Edit math/a.tex")
        );
    }

    #[test]
    fn the_words_of_a_kind_name_that_kind() {
        let diagram = FigureWords::of(FigureKind::Diagram);
        assert_eq!(diagram.new_more, "New diagram…");
        assert_eq!(diagram.none, "This book has no diagrams yet.");
        assert_eq!(
            diagram.add_first,
            "Add a diagram in the Diagrams section first."
        );
        let formula = FigureWords::of(FigureKind::Formula);
        assert_eq!(formula.insert_title, "Insert a formula");
        assert_eq!(formula.insert, "Insert formula");
        assert_eq!(formula.name, "Formula");
        assert_eq!(
            FigureWords::remove_question(FigureKind::Formula, "math/a.tex"),
            "Remove formula math/a.tex?"
        );
        assert_eq!(
            FigureWords::edit_named(FigureKind::Diagram, "dia/a.mmd"),
            "Edit diagram dia/a.mmd"
        );
        assert_eq!(
            FigureWords::remove_named(FigureKind::Formula, "math/a.tex"),
            "Remove formula math/a.tex"
        );
    }
}

#[cfg(test)]
mod layout_tests {
    use super::SourceLayout;

    #[test]
    fn a_wide_window_puts_the_drawing_beside_the_source_and_takes_most_of_the_window() {
        let layout = SourceLayout::of(1280.0, 780.0, true, 32.0);
        assert!(layout.beside);
        assert_eq!(layout.width, 1232.0);
        assert!(layout.preview_width > 700.0, "{layout:?}");
        assert!(layout.source_height > 300.0, "{layout:?}");
        assert!(layout.preview_height > layout.source_height);
    }

    #[test]
    fn the_dialog_stops_growing_in_a_very_wide_window() {
        let layout = SourceLayout::of(3000.0, 1600.0, false, 32.0);
        assert_eq!(layout.width, SourceLayout::MAX_WIDTH);
    }

    #[test]
    fn a_narrow_window_puts_the_drawing_under_the_source_at_the_full_width() {
        let layout = SourceLayout::of(390.0, 844.0, true, 44.0);
        assert!(!layout.beside);
        assert_eq!(layout.width, 342.0);
        assert_eq!(layout.preview_width, 310.0);
        assert!(layout.preview_height > layout.source_height, "{layout:?}");
        assert!(layout.source_height >= SourceLayout::MIN_SOURCE);
    }

    #[test]
    fn a_short_window_keeps_the_smallest_source_and_drawing() {
        let layout = SourceLayout::of(400.0, 200.0, true, 32.0);
        assert_eq!(layout.width, 352.0);
        assert_eq!(layout.source_height, SourceLayout::MIN_SOURCE);
        assert_eq!(layout.preview_height, SourceLayout::MIN_PREVIEW);
        let layout = SourceLayout::of(100.0, 200.0, false, 32.0);
        assert_eq!(layout.width, SourceLayout::MIN_WIDTH);
    }

    #[test]
    fn the_book_path_takes_height_from_the_source() {
        let with_path = SourceLayout::of(1280.0, 780.0, true, 32.0);
        let without = SourceLayout::of(1280.0, 780.0, false, 32.0);
        assert_eq!(
            without.source_height - with_path.source_height,
            SourceLayout::PATH_LABEL + 32.0
        );
    }

    #[test]
    fn a_large_drawing_is_fitted_into_the_preview_and_can_be_seen_at_its_size() {
        let layout = SourceLayout::of(1280.0, 780.0, true, 32.0);
        let room = layout.preview_height - SourceLayout::PADDING * 2.0;
        let scale = layout.fit(400.0, room * 2.0);
        assert!((scale - 0.5).abs() < 1e-6, "{scale}");
        assert_eq!(layout.zoom(400.0, room * 2.0), Some(scale));
    }

    #[test]
    fn a_small_drawing_is_enlarged_at_most_twice_and_has_nothing_to_zoom() {
        let layout = SourceLayout::of(1280.0, 780.0, true, 32.0);
        assert_eq!(layout.fit(40.0, 10.0), SourceLayout::MAX_ENLARGE);
        assert_eq!(layout.zoom(40.0, 10.0), None);
    }
}

#[cfg(test)]
mod syntax_tests {
    use super::{LATEX, MERMAID, register_source_grammars};
    use gpui_kit::component::Rope;
    use gpui_kit::component::highlighter::{HighlightTheme, SyntaxHighlighter};

    #[test]
    fn the_source_grammars_highlight_mermaid_keywords_and_formula_commands() {
        register_source_grammars();
        for (language, source, token) in [
            (MERMAID, "flowchart LR\n  A --> B", "flowchart"),
            (LATEX, r"\frac{a}{b}", r"\frac"),
        ] {
            let mut highlighter = SyntaxHighlighter::new(language);
            assert!(highlighter.update(None, &Rope::from_str(source), None));
            let highlights =
                highlighter.styles(&(0..source.len()), &*HighlightTheme::default_dark());
            assert!(
                highlights.iter().any(|(range, style)| {
                    source.get(range.clone()) == Some(token) && style.color.is_some()
                }),
                "{language}: {highlights:?}",
            );
        }
    }
}
