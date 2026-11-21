//! The visual chapter editor and its link to the Markdown buffer.

use std::collections::VecDeque;
use std::ops::Range;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{
    Editor, EditorState, InputEditsEvent, Redo, TextDecoration, TextDecorationCollection, Undo,
};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Focusable as _, FontStyle, FontWeight,
    HighlightStyle, InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, StrikethroughStyle, Styled as _, TestSupportExt as _,
    UnderlineStyle, Window, div, px, relative,
};

use super::EditorPage;
use super::keys::{ESCAPED_CONTEXT, SOURCE_CONTEXT};
use super::session::ViewMode;
use super::visual_document::{Document, Format, Style, changed_range};
use crate::features::appearance::Appearance;
use crate::shared::ui::{icon_button, page_colors};

const HISTORY_LIMIT: usize = 40;
const HISTORY_BYTES: usize = 8 * 1024 * 1024;

struct Revision {
    source: SharedString,
    selection: Range<usize>,
}

#[derive(Default)]
struct History {
    undo: VecDeque<Revision>,
    redo: VecDeque<Revision>,
}

fn remember(stack: &mut VecDeque<Revision>, revision: Revision) {
    stack.push_back(revision);
    let mut bytes: usize = stack.iter().map(|item| item.source.len()).sum();
    while stack.len() > 1 && (stack.len() > HISTORY_LIMIT || bytes > HISTORY_BYTES) {
        if let Some(oldest) = stack.pop_front() {
            bytes -= oldest.source.len();
        }
    }
}

pub(super) struct VisualEditor {
    pub(super) input: Entity<EditorState>,
    decorations: TextDecorationCollection,
    source: SharedString,
    document: Document,
    history: History,
}

impl VisualEditor {
    pub(super) fn new(window: &mut Window, cx: &mut Context<'_, EditorPage>) -> Self {
        let input = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .placeholder("Start writing…")
                .line_number(false)
                .indent_guides(false)
                .soft_wrap(true)
                .searchable(false);
            state.set_disabled(true, cx);
            state
        });
        let decorations = input.update(cx, |state, cx| {
            state.create_decorations_collection(Vec::new(), cx)
        });
        Self {
            input,
            decorations,
            source: SharedString::default(),
            document: Document::default(),
            history: History::default(),
        }
    }

    fn update(
        &mut self,
        source: SharedString,
        selection: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.source != source {
            self.document = Document::parse(&source);
            self.source = source;
        }
        let selected = self.document.display_offset(selection.start)
            ..self.document.display_offset(selection.end);
        self.input.update(cx, |state, cx| {
            if state.value().as_str() != self.document.text {
                state.set_value(self.document.text.as_str(), window, cx);
            }
            if state.selected_range() != selected {
                state.set_selected_range(selected, cx);
            }
        });
        let theme = cx.theme();
        self.decorations.set(
            self.document
                .styles
                .iter()
                .map(|(range, style)| {
                    let style = match style {
                        Style::Heading | Style::Bold => HighlightStyle {
                            font_weight: Some(FontWeight::BOLD),
                            ..Default::default()
                        },
                        Style::Italic | Style::Quote => HighlightStyle {
                            font_style: Some(FontStyle::Italic),
                            ..Default::default()
                        },
                        Style::Image => HighlightStyle {
                            font_style: Some(FontStyle::Italic),
                            color: Some(theme.muted_foreground),
                            ..Default::default()
                        },
                        Style::Strike => HighlightStyle {
                            strikethrough: Some(StrikethroughStyle::default()),
                            ..Default::default()
                        },
                        Style::Code => HighlightStyle {
                            background_color: Some(theme.muted.opacity(0.5)),
                            ..Default::default()
                        },
                        Style::Link => HighlightStyle {
                            color: Some(theme.link),
                            underline: Some(UnderlineStyle::default()),
                            ..Default::default()
                        },
                    };
                    TextDecoration::new(range.clone(), style)
                })
                .collect(),
            cx,
        );
    }
}

impl EditorPage {
    pub(super) fn set_mode(
        &mut self,
        mode: ViewMode,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if mode == self.session.mode {
            return;
        }
        if self.session.mode == ViewMode::Visual && self.loaded.is_some() {
            let selected = self.visual.input.read(cx).selected_range();
            let range = self.visual.document.source_range(selected);
            self.text
                .update(cx, |state, cx| state.set_selected_range(range, cx));
        }
        self.session.mode = mode;
        if mode == ViewMode::Visual {
            self.sync_visual(window, cx);
        }
        cx.notify();
    }

    pub(super) fn sync_visual(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.session.mode == ViewMode::Visual {
            let source = self.text.read(cx).value();
            let selection = self.text.read(cx).selected_range();
            if self.visual.source != source {
                self.visual.history = History::default();
            }
            self.visual.update(source, selection, window, cx);
        }
    }

    pub(super) fn visual_loaded(
        &mut self,
        editable: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.visual.history = History::default();
        self.visual
            .input
            .update(cx, |state, cx| state.set_disabled(!editable, cx));
        self.sync_visual(window, cx);
    }

    /// Applies the input's actual edits, so repeated letters on the two
    /// sides of a format boundary do not move an edit to the other side.
    pub(super) fn visual_typed(
        &mut self,
        event: &InputEditsEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.session.mode != ViewMode::Visual || self.loaded.is_none() {
            return;
        }
        let before = self.visual.source.clone();
        let mut source = before.to_string();
        let mut document = &self.visual.document;
        let mut updated;
        let mut original = None;
        let mut caret = 0;
        for (index, (edit, snapshot)) in event.edits().iter().enumerate() {
            let after = snapshot.to_string();
            let range = edit.start_byte..edit.old_end_byte;
            let inserted = edit.start_byte..edit.new_end_byte;
            let (range, inserted) = if range.end <= document.text.len()
                && document.text.is_char_boundary(range.start)
                && document.text.is_char_boundary(range.end)
                && after.is_char_boundary(inserted.start)
                && after.is_char_boundary(inserted.end)
                && (range.start != 0 || range.end != document.text.len())
            {
                (range, inserted)
            } else {
                changed_range(&document.text, &after)
            };
            original.get_or_insert_with(|| document.source_range(range.clone()));
            let (range, text, at) = document.splice(&source, range, &after[inserted]);
            source.replace_range(range, &text);
            caret = at;
            if index + 1 < event.edits().len() {
                updated = Document::parse(&source);
                document = &updated;
            }
        }
        if let Some(selection) = original {
            self.apply_visual(before, selection, source, caret..caret, window, cx);
        }
    }

    fn format_visual(&mut self, format: Format, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.loaded.is_none() {
            return;
        }
        let selected = self.visual.input.read(cx).selected_range();
        let before = self.visual.source.clone();
        if let Some((range, replacement, selection)) =
            self.visual
                .document
                .toggle(&before, selected.clone(), format)
        {
            let mut source = before.to_string();
            source.replace_range(range, &replacement);
            let original = self.visual.document.source_range(selected);
            self.apply_visual(before, original, source, selection, window, cx);
        }
        self.visual.input.focus_handle(cx).focus(window, cx);
    }

    fn apply_visual(
        &mut self,
        before: SharedString,
        original: Range<usize>,
        source: String,
        selection: Range<usize>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if before.as_str() == source {
            self.visual.update(before, selection, window, cx);
            return;
        }
        remember(
            &mut self.visual.history.undo,
            Revision {
                source: before,
                selection: original,
            },
        );
        self.visual.history.redo.clear();
        self.write_visual(source.into(), selection, window, cx);
    }

    fn write_visual(
        &mut self,
        source: SharedString,
        selection: Range<usize>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (range, inserted) = changed_range(&self.visual.source, &source);
        self.text.update(cx, |state, cx| {
            state.set_selected_range(range, cx);
            state.replace(source[inserted].to_owned(), window, cx);
            state.set_selected_range(selection.clone(), cx);
        });
        self.visual.update(source, selection, window, cx);
    }

    fn visual_history(&mut self, redo: bool, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.loaded.is_none() {
            return;
        }
        let history = &mut self.visual.history;
        let (from, to) = if redo {
            (&mut history.redo, &mut history.undo)
        } else {
            (&mut history.undo, &mut history.redo)
        };
        if let Some(revision) = from.pop_back() {
            remember(
                to,
                Revision {
                    source: self.visual.source.clone(),
                    selection: self
                        .visual
                        .document
                        .source_range(self.visual.input.read(cx).selected_range()),
                },
            );
            self.write_visual(revision.source, revision.selection, window, cx);
        }
        self.visual.input.focus_handle(cx).focus(window, cx);
    }

    pub(super) fn render_visual(
        &self,
        path: &str,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let appearance = Appearance::global(cx);
        let appearance = appearance.read(cx);
        let font = appearance.reading_font(cx);
        let settings = appearance.settings();
        let text_size = px(settings.text_size);
        let line_height = relative(settings.line_height);
        let (paper, ink) = page_colors(appearance.page_tone(window));
        v_flex()
            .id("visual-editor")
            .role(Role::Group)
            .aria_label("Visual chapter editor")
            .test_support()
            .size_full()
            .min_h_0()
            .child(self.visual_toolbar(cx))
            .child(
                div()
                    .id("visual-source")
                    .key_context(if self.escaped {
                        ESCAPED_CONTEXT
                    } else {
                        SOURCE_CONTEXT
                    })
                    .capture_action(cx.listener(|this, _: &Undo, window, cx| {
                        cx.stop_propagation();
                        this.visual_history(false, window, cx);
                    }))
                    .capture_action(cx.listener(|this, _: &Redo, window, cx| {
                        cx.stop_propagation();
                        this.visual_history(true, window, cx);
                    }))
                    .min_h_0()
                    .flex_1()
                    .bg(paper)
                    .child(
                        Editor::new(&self.visual.input)
                            .aria_label(format!("Visual editor of {path}"))
                            .appearance(false)
                            .bordered(false)
                            .h(relative(1.0))
                            .font_family(font)
                            .text_size(text_size)
                            .line_height(line_height)
                            .text_color(ink),
                    ),
            )
            .into_any_element()
    }

    fn visual_toolbar(&self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme();
        let controls = [
            (
                "visual-paragraph",
                IconName::Pilcrow,
                "Paragraph",
                Format::Paragraph,
            ),
            (
                "visual-heading",
                IconName::Heading1,
                "Heading",
                Format::Heading,
            ),
            (
                "visual-subheading",
                IconName::Heading2,
                "Subheading",
                Format::Subheading,
            ),
            ("visual-bold", IconName::Bold, "Bold", Format::Bold),
            ("visual-italic", IconName::Italic, "Italic", Format::Italic),
            (
                "visual-strike",
                IconName::Strikethrough,
                "Strikethrough",
                Format::Strike,
            ),
            ("visual-code", IconName::Code, "Inline code", Format::Code),
            (
                "visual-bullets",
                IconName::List,
                "Bulleted list",
                Format::Bullet,
            ),
            (
                "visual-numbered",
                IconName::ListOrdered,
                "Numbered list",
                Format::Ordered,
            ),
            (
                "visual-quote",
                IconName::Quote,
                "Block quote",
                Format::Quote,
            ),
        ];
        h_flex()
            .flex_wrap()
            .gap_1()
            .px_3()
            .py_1()
            .border_b_1()
            .border_color(theme.border)
            .child(
                icon_button("visual-undo", IconName::Undo2, "Undo")
                    .disabled(self.loaded.is_none() || self.visual.history.undo.is_empty())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.visual_history(false, window, cx);
                    })),
            )
            .child(
                icon_button("visual-redo", IconName::Redo2, "Redo")
                    .disabled(self.loaded.is_none() || self.visual.history.redo.is_empty())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.visual_history(true, window, cx);
                    })),
            )
            .children(controls.map(|(id, icon, label, format)| {
                icon_button(id, icon, label)
                    .disabled(self.loaded.is_none())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.format_visual(format, window, cx);
                    }))
            }))
    }
}
