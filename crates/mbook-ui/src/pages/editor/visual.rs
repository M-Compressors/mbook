//! The block editor and its connection to the chapter's Markdown buffer.

use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Copy, Cut, Redo, SelectAll, Undo};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, ClipboardItem, Context, Entity, ExternalPaths,
    Focusable as _, Global, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    Window, div, px, relative,
};
use markdown_editor::{
    BlockEditor, BlockEditorEvent, BlockEditorState, BlockEditorStyle, BlockKind, BlockSelection,
    EditorCommand, ImagePolicy,
};
use rust_i18n::t;

use super::EditorPage;
use super::encode::Target;
use super::keys::VISUAL_CONTEXT;
use super::link_prompt::{LinkAnswer, LinkRequest, ask_link};
use super::place_picker::chapter_choices;
use super::session::ViewMode;
use super::visual_figures::SharedFigures;
use super::visual_formatting::{formatting, insert_menu};
use super::visual_pictures::VisualPictures;
use super::visual_source::VisualSource;
use crate::entities::figure::{FigureKind, Palette};
use crate::entities::picture::picture_file_extensions;
use crate::features::appearance::Appearance;
use crate::shared::ui::{empty_state, icon_button, page_colors, status_line, target_size};

/// The space between the edges of Visual and its text, in logical pixels.
const PADDING: f32 = 16.0;

struct KeysInstalled;

impl Global for KeysInstalled {}

pub(super) struct VisualEditor {
    pub(super) input: Entity<BlockEditorState>,
    pub(super) pictures: Arc<Mutex<VisualPictures>>,
    /// The diagrams and formulas of the chapter.
    pub(super) figures: SharedFigures,
    scroll: ScrollHandle,
    baseline: Option<VisualSource>,
    /// The text last loaded from or copied into Source.
    mirrored: Option<SharedString>,
    error: Option<SharedString>,
}

impl VisualEditor {
    pub(super) fn new(window: &mut Window, cx: &mut Context<'_, EditorPage>) -> Self {
        if !cx.has_global::<KeysInstalled>() {
            markdown_editor::init(cx);
            cx.set_global(KeysInstalled);
        }
        let scroll = ScrollHandle::new();
        let input = cx.new(|cx| {
            let mut editor = BlockEditorState::new(window, cx).with_image_policy(ImagePolicy::Load);
            editor.set_scroll_handle(scroll.clone());
            editor.set_readonly(true, cx);
            editor
        });
        Self {
            input,
            pictures: Arc::default(),
            figures: Arc::default(),
            scroll,
            baseline: None,
            mirrored: None,
            error: None,
        }
    }
}

impl EditorPage {
    pub(super) fn set_mode(
        &mut self,
        mode: ViewMode,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if mode != self.session.mode {
            self.session.mode = mode;
            self.escaped = false;
            if mode == ViewMode::Visual && self.loaded.is_some() {
                self.sync_visual(window, cx);
            }
            self.refresh_search(true, cx);
            cx.notify();
        }
    }

    pub(super) fn visual_loading(&mut self, cx: &mut Context<'_, Self>) {
        self.visual.mirrored = None;
        self.visual.baseline = None;
        self.visual.error = None;
        self.visual
            .input
            .update(cx, |state, cx| state.set_readonly(true, cx));
    }

    pub(super) fn visual_loaded(
        &mut self,
        editable: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.visual_loading(cx);
        if editable && self.session.mode == ViewMode::Visual {
            self.sync_visual(window, cx);
        }
    }

    fn sync_visual(&mut self, window: &Window, cx: &mut Context<'_, Self>) {
        let source = self.text.read(cx).value();
        if self.visual.mirrored.as_ref() == Some(&source) {
            return;
        }
        let appearance = Appearance::global(cx);
        let (_, ink) = page_colors(appearance.read(cx).page_tone(window));
        let baseline = self.visual.input.update(cx, |state, cx| {
            state.set_source(&source, cx)?;
            let canonical = state.source_snapshot();
            let baseline = VisualSource::new(source.clone(), canonical.text().clone())
                .ok_or_else(|| SharedString::from(t!("visual.unpreservable")))?;
            state.set_editor_style(BlockEditorStyle::default().with_foreground(ink), cx);
            state.set_readonly(false, cx);
            Ok::<_, SharedString>(baseline)
        });
        self.visual.mirrored = Some(source);
        match baseline {
            Ok(source) => {
                self.visual.baseline = Some(source);
                self.visual.error = None;
                self.visual
                    .scroll
                    .set_offset(gpui_kit::point(px(0.), px(0.)));
            }
            Err(error) => {
                self.visual.baseline = None;
                self.visual.error = Some(error);
                self.visual
                    .input
                    .update(cx, |state, cx| state.set_readonly(true, cx));
            }
        }
    }

    pub(super) fn on_visual_event(
        &mut self,
        event: &BlockEditorEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.session.mode != ViewMode::Visual || self.loaded.is_none() {
            return;
        }
        match event {
            BlockEditorEvent::Changed { .. } => {
                let Some(baseline) = &self.visual.baseline else {
                    return;
                };
                let snapshot = self.visual.input.read(cx).source_snapshot();
                let source = baseline.restore(snapshot.text().clone());
                if self.visual.mirrored.as_ref() != Some(&source) {
                    self.visual.mirrored = Some(source.clone());
                    self.text
                        .update(cx, |state, cx| state.replace_all(source, window, cx));
                }
                cx.notify();
            }
            BlockEditorEvent::EditLinkRequested { url } => {
                self.edit_visual_link(url.as_ref(), window, cx);
            }
            BlockEditorEvent::Focus | BlockEditorEvent::Blur => {
                self.escaped = false;
                cx.notify();
            }
            BlockEditorEvent::SelectionChanged => cx.notify(),
            _ => {}
        }
    }

    /// Opens the dialog of a link: for the link at the caret, for the words
    /// that are selected, or, with nothing selected, for new words.
    fn edit_visual_link(
        &self,
        url: Option<&SharedString>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let editor = self.visual.input.read(cx);
        let ask_text = url.is_none() && editor.selection().is_collapsed();
        let Ok(selection) = editor.capture_selection() else {
            return;
        };
        let Some(current) = &self.loaded else {
            return;
        };
        ask_link(
            LinkRequest {
                current: url.map(ToString::to_string),
                ask_text,
                from: current.clone(),
                choices: chapter_choices(self.session.chapters()),
            },
            window,
            cx,
            move |this, answer, window, cx| {
                this.visual.input.update(cx, |state, cx| {
                    if state.is_capture_current(&selection) {
                        apply_link(state, answer, cx);
                    }
                    state.focus(window, cx);
                });
            },
        );
    }

    /// Puts Markdown the editor wrote itself where the caret of Visual is.
    pub(super) fn insert_markdown(
        &self,
        markdown: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.visual.input.update(cx, |state, cx| {
            state.paste(ClipboardItem::new_string(markdown), false, cx);
            state.focus(window, cx);
        });
    }

    /// Files were dropped on the chapter: the pictures among them are added
    /// to the book and put where the caret is.
    pub(super) fn drop_files(
        &mut self,
        files: &[std::path::PathBuf],
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let pictures: Vec<_> = files
            .iter()
            .filter(|file| is_picture(file))
            .cloned()
            .collect();
        if !pictures.is_empty() {
            self.encode_files(pictures, Target::Chapter, window, cx);
        }
    }

    /// Whether Visual shows the open chapter and can be typed into.
    pub(super) fn visual_is_editing(&self, chapter: &str, cx: &Context<'_, Self>) -> bool {
        self.session.mode == ViewMode::Visual
            && self.loaded.as_deref() == Some(chapter)
            && !self.visual.input.read(cx).is_readonly()
    }

    pub(super) fn render_visual(
        &self,
        path: &str,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        if self.loaded.is_none() {
            return status_line("visual-loading", t!("visual.opening")).into_any_element();
        }
        if let Some(error) = &self.visual.error {
            return empty_state(t!("visual.needs_source"), error.clone()).into_any_element();
        }
        let (font, size, line_height, (paper, ink)) = {
            let appearance = Appearance::global(cx);
            let appearance = appearance.read(cx);
            let settings = appearance.settings();
            (
                appearance.reading_font(cx),
                settings.text_size,
                settings.line_height,
                page_colors(appearance.page_tone(window)),
            )
        };
        let (text_size, line_height) = (px(size), relative(line_height));
        // The width of the text as it was last laid out; none before the
        // first layout, when pictures wait for it.
        let column = self.visual.scroll.bounds().size.width.as_f32() - 2.0 * PADDING;
        self.draw_visual_figures(Palette::of(paper, ink), cx);
        v_flex()
            .id("visual-editor")
            .key_context(if self.escaped {
                super::keys::VISUAL_ESCAPED_CONTEXT
            } else {
                VISUAL_CONTEXT
            })
            .on_action(cx.listener(|this, _: &Undo, window, cx| {
                this.run_visual_history(false, window, cx);
            }))
            .on_action(cx.listener(|this, _: &Redo, window, cx| {
                this.run_visual_history(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                this.visual.input.update(cx, |state, cx| {
                    let _ = state.run(EditorCommand::SelectAll, cx);
                });
            }))
            .on_action(|_: &Copy, window, cx| {
                visual_clipboard_action("block_editor::Copy", window, cx);
            })
            .on_action(|_: &Cut, window, cx| {
                visual_clipboard_action("block_editor::Cut", window, cx);
            })
            .size_full()
            .min_h_0()
            .child(self.visual_toolbar(cx))
            .child(
                div()
                    .id("visual-viewport")
                    .role(Role::Group)
                    .aria_label(t!("visual.editor_of", path = path))
                    .test_support()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .bg(paper)
                    .overflow_y_scroll()
                    .track_scroll(&self.visual.scroll)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.visual.input.focus_handle(cx).focus(window, cx);
                    }))
                    .can_drop(|dragged, _, _| holds_pictures(dragged))
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                        this.drop_files(paths.paths(), window, cx);
                    }))
                    .child(
                        BlockEditor::new(&self.visual.input)
                            .image_source(self.visual_image_source(path, column, size))
                            .min_h_full()
                            .px(px(PADDING))
                            .py_3()
                            .font_family(font)
                            .text_size(text_size)
                            .line_height(line_height)
                            .text_color(ink),
                    ),
            )
            .into_any_element()
    }

    /// The one toolbar of Visual: history, the style of the text, then what
    /// is put into the chapter: a link, a picture, a table and the rest.
    fn visual_toolbar(&self, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let editor = self.visual.input.read(cx);
        let enabled = !editor.is_readonly();
        let (undo, redo) = (editor.model().can_undo(), editor.model().can_redo());
        let block = editor.block_kind_at_caret();
        let code = block == Some(BlockKind::CodeBlock);
        let figure_enabled =
            enabled && !matches!(block, Some(BlockKind::CodeBlock | BlockKind::TableCell));
        let divider = || {
            div()
                .flex_none()
                .w(px(1.0))
                .h(px(20.0))
                .mx_1()
                .bg(cx.theme().border)
        };
        let labelled = |id: &'static str, icon: IconName, label: Cow<'static, str>| {
            Button::new(id)
                .ghost()
                .icon(icon)
                .label(label)
                .min_h(px(target_size()))
                .cursor_pointer()
        };
        h_flex()
            .id("visual-toolbar")
            .w_full()
            .px_2()
            .py_1()
            .gap_1()
            .flex_wrap()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                icon_button("visual-undo", IconName::Undo2, t!("visual.undo"))
                    .disabled(!enabled || !undo)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.run_visual_history(false, window, cx);
                    })),
            )
            .child(
                icon_button("visual-redo", IconName::Redo2, t!("visual.redo"))
                    .disabled(!enabled || !redo)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.run_visual_history(true, window, cx);
                    })),
            )
            .child(divider())
            .child(formatting(&self.visual.input, cx))
            .child(divider())
            .child(
                labelled("visual-link", IconName::Link, t!("visual.link"))
                    .tooltip(t!("visual.link_tooltip"))
                    .disabled(!enabled)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.visual
                            .input
                            .update(cx, BlockEditorState::request_link_edit);
                    })),
            )
            .child(
                labelled("visual-image", IconName::ImagePlus, t!("visual.picture"))
                    .tooltip(t!("visual.picture_tooltip"))
                    .disabled(!enabled)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.ask_chapter_picture(window, cx);
                    })),
            )
            .children(
                [
                    ("visual-diagram", t!("visual.diagram"), FigureKind::Diagram),
                    ("visual-formula", t!("visual.formula"), FigureKind::Formula),
                ]
                .into_iter()
                .map(|(id, label, kind)| {
                    super::sections::control(id, label)
                        .ghost()
                        .disabled(!figure_enabled)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.ask_chapter_figure(kind, window, cx);
                        }))
                }),
            )
            .child(insert_menu(self.visual.input.clone(), enabled))
            .child(divider())
            .child(
                icon_button(
                    "visual-image-preview",
                    IconName::Images,
                    t!("visual.chapter_pictures"),
                )
                .disabled(!enabled || self.session.pictures().is_empty())
                .on_click(cx.listener(|this, _, window, cx| {
                    this.preview_chapter_pictures(window, cx);
                })),
            )
            .children(code.then(|| self.code_language()))
    }

    fn run_visual_history(&self, redo: bool, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.visual.input.update(cx, |state, cx| {
            if redo {
                state.redo(cx);
            } else {
                state.undo(cx);
            }
            state.focus(window, cx);
        });
    }

    fn code_language(&self) -> impl IntoElement {
        let input = self.visual.input.clone();
        icon_button(
            "visual-code-language",
            IconName::FileCode,
            t!("visual.code_language"),
        )
        .dropdown_menu(move |menu, _, _| {
            [
                (t!("visual.plain_text"), None),
                (Cow::Borrowed("Bash"), Some("bash")),
                (Cow::Borrowed("C"), Some("c")),
                (Cow::Borrowed("C++"), Some("cpp")),
                (Cow::Borrowed("Go"), Some("go")),
                (Cow::Borrowed("Java"), Some("java")),
                (Cow::Borrowed("JavaScript"), Some("javascript")),
                (Cow::Borrowed("JSON"), Some("json")),
                (Cow::Borrowed("Python"), Some("python")),
                (Cow::Borrowed("Rust"), Some("rust")),
                (Cow::Borrowed("SQL"), Some("sql")),
                (Cow::Borrowed("TypeScript"), Some("typescript")),
            ]
            .into_iter()
            .fold(menu, |menu, (label, lang)| {
                let input = input.clone();
                menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                    input.update(cx, |state, cx| {
                        let _ = state.run(
                            EditorCommand::SetCodeBlock {
                                lang: lang.map(str::to_owned),
                            },
                            cx,
                        );
                        state.focus(window, cx);
                    });
                }))
            })
        })
    }
}

// Native Edit menu actions use Kit's input types. The block editor's
// clipboard actions are available through GPUI's action registry.
fn visual_clipboard_action(name: &str, window: &mut Window, cx: &mut App) {
    if let Ok(action) = cx.build_action(name, None) {
        window.dispatch_action(action, cx);
    }
}

/// Makes, changes or removes a link at the selection of the editor.
fn apply_link(
    state: &mut BlockEditorState,
    answer: LinkAnswer,
    cx: &mut Context<'_, BlockEditorState>,
) {
    match answer {
        LinkAnswer::Remove => state.set_link(None, cx),
        LinkAnswer::Set {
            text: None,
            destination,
        } => state.set_link(Some(&destination), cx),
        LinkAnswer::Set {
            text: Some(text),
            destination,
        } => {
            // The words are typed by the editor, which escapes them, and then
            // selected from where the caret was to where it is.
            let BlockSelection::Text { head: before, .. } = *state.selection() else {
                return;
            };
            state.paste(ClipboardItem::new_string(text), true, cx);
            let BlockSelection::Text { head: after, .. } = *state.selection() else {
                return;
            };
            let linked = state.set_selection(
                BlockSelection::Text {
                    anchor: before,
                    head: after,
                },
                cx,
            );
            if linked.is_ok() {
                state.set_link(Some(&destination), cx);
                let _ = state.set_selection(BlockSelection::caret(after), cx);
            }
        }
    }
}

/// Whether what is dragged over the chapter is files with a picture among
/// them. Other files are left to the window, which imports books.
fn holds_pictures(dragged: &dyn std::any::Any) -> bool {
    dragged
        .downcast_ref::<ExternalPaths>()
        .is_some_and(|paths| paths.paths().iter().any(|path| is_picture(path)))
}

/// Whether a file that was dropped has the extension of a picture the book
/// can take.
fn is_picture(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            picture_file_extensions()
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{holds_pictures, is_picture};

    #[test]
    fn what_is_dragged_must_be_files_to_be_pictures() {
        assert!(!holds_pictures(&"words"));
    }

    #[test]
    fn a_dropped_file_is_a_picture_by_its_extension() {
        assert!(is_picture(Path::new("/tmp/Pond.PNG")));
        assert!(is_picture(Path::new("a.avif")));
        assert!(!is_picture(Path::new("book.epub")));
        assert!(!is_picture(Path::new("no-extension")));
    }
}
