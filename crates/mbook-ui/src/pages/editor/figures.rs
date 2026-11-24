//! The diagram and formula sections and their chapter insertion controls.

use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Once};

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::highlighter::{GrammarConfig, LanguageRegistry};
use gpui_kit::component::input::{Editor, EditorState, Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, SharedString, StatefulInteractiveElement as _, Styled as _, Task,
    TestSupportExt as _, Window, div, img, px,
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
    let (language, source_label, title) = source_words(kind, path, editing);
    let path_input = cx.new(|cx| {
        let mut state = InputState::new(window, cx);
        state.set_value(path, window, cx);
        state
    });
    let source_input = cx.new(|cx| {
        EditorState::new(window, cx)
            .language(language)
            .default_value(initial.unwrap_or_default().to_owned())
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
                    .child(div().child(SharedString::from(t!("prompt.figure.book_path"))))
                    .child(Input::new(&path_input).aria_label(t!("prompt.figure.book_path")))
            }))
            .child(div().child(source_label.clone()))
            .child(
                Editor::new(&source_input)
                    .h(px(120.0))
                    .aria_label(source_label.clone()),
            )
            .child(
                h_flex().child(
                    control("draw-figure", t!("prompt.figure.preview"))
                        .ghost()
                        .on_click(move |_, window, cx| {
                            let source = source_button.read(cx).value().to_string();
                            preview_button.update(cx, |preview, cx| {
                                preview.draw(kind, source, tone.into(), window, cx);
                            });
                            window.refresh();
                        }),
                ),
            )
            .children(preview.read(cx).display(&source, width - 56.0, window, cx))
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
            .width(px(width))
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
            None => status_line("figure-preview-loading", t!("prompt.figure.rendering"))
                .into_any_element(),
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
                    .aria_label(t!("prompt.figure.preview_label"))
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
