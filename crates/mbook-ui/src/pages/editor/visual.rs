//! The block editor and its connection to the chapter's Markdown buffer.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Copy, Cut, Redo, SelectAll, Undo};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, ClipboardItem, Context, Entity, Focusable as _, Global,
    InteractiveElement as _, IntoElement, ParentElement as _, Role, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px, relative,
};
use markdown_editor::{
    BlockEditor, BlockEditorEvent, BlockEditorState, BlockEditorStyle, BlockKind, BlockSelection,
    EditorCommand, EditorToolbar, ImagePolicy,
};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use super::EditorPage;
use super::keys::VISUAL_CONTEXT;
use super::prompts::{PathRequest, ask_path};
use super::session::ViewMode;
use super::visual_source::VisualSource;
use crate::features::appearance::Appearance;
use crate::shared::ui::{announce, empty_state, icon_button, page_colors, status_line};

struct KeysInstalled;

impl Global for KeysInstalled {}

pub(super) struct VisualEditor {
    pub(super) input: Entity<BlockEditorState>,
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
            let mut editor =
                BlockEditorState::new(window, cx).with_image_policy(ImagePolicy::Placeholder);
            editor.set_scroll_handle(scroll.clone());
            editor.set_readonly(true, cx);
            editor
        });
        Self {
            input,
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
            let baseline =
                VisualSource::new(source.clone(), canonical.text().clone()).ok_or_else(|| {
                    SharedString::from(
                        "This chapter uses Markdown blocks Visual could not preserve.",
                    )
                })?;
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

    fn edit_visual_link(
        &self,
        url: Option<&SharedString>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let editor = self.visual.input.read(cx);
        if url.is_none()
            && matches!(editor.selection(), BlockSelection::Text { anchor, head } if anchor == head)
        {
            announce("Select the text for the link first.", window, cx);
            return;
        }
        let Ok(selection) = editor.capture_selection() else {
            return;
        };
        ask_path(
            &PathRequest {
                title: if url.is_some() {
                    "Edit link"
                } else {
                    "Insert link"
                },
                label: "Link destination",
                initial: url.map_or_else(String::new, ToString::to_string),
                ok: "Apply",
            },
            window,
            cx,
            move |this, value, window, cx| {
                this.visual.input.update(cx, |state, cx| {
                    if state.is_capture_current(&selection) {
                        state.set_link(Some(&value), cx);
                    }
                    state.focus(window, cx);
                });
            },
            |_, _, _| {},
        );
    }

    pub(super) fn render_visual(
        &self,
        path: &str,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        if self.loaded.is_none() {
            return status_line("visual-loading", "Opening the chapter").into_any_element();
        }
        if let Some(error) = &self.visual.error {
            return empty_state("This chapter needs Source", error.clone()).into_any_element();
        }
        let appearance = Appearance::global(cx);
        let appearance = appearance.read(cx);
        let font = appearance.reading_font(cx);
        let settings = appearance.settings();
        let text_size = px(settings.text_size);
        let line_height = relative(settings.line_height);
        let (paper, ink) = page_colors(appearance.page_tone(window));
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
            .child(self.visual_toolbar(path, cx))
            .child(
                div()
                    .id("visual-viewport")
                    .role(Role::Group)
                    .aria_label(format!("Visual editor of {path}"))
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
                    .child(
                        BlockEditor::new(&self.visual.input)
                            .min_h_full()
                            .px_4()
                            .py_3()
                            .font_family(font)
                            .text_size(text_size)
                            .line_height(line_height)
                            .text_color(ink),
                    ),
            )
            .into_any_element()
    }

    fn visual_toolbar(&self, chapter: &str, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let editor = self.visual.input.read(cx);
        let enabled = !editor.is_readonly();
        let (undo, redo) = (editor.model().can_undo(), editor.model().can_redo());
        let has_link = editor.link_at_caret().is_some();
        let code = editor.block_kind_at_caret() == Some(BlockKind::CodeBlock);
        let pictures: Vec<_> = self
            .session
            .pictures()
            .iter()
            .map(|picture| {
                let name = picture.path.clone();
                let markdown = picture_markdown(chapter, &name);
                (name, markdown)
            })
            .collect();
        let input = self.visual.input.clone();
        v_flex()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .id("visual-formatting")
                    .max_w_full()
                    .overflow_x_scroll()
                    .child(EditorToolbar::new(&self.visual.input)),
            )
            .child(
                h_flex()
                    .px_2()
                    .gap_1()
                    .flex_wrap()
                    .child(
                        icon_button("visual-undo", IconName::Undo2, "Undo")
                            .disabled(!enabled || !undo)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run_visual_history(false, window, cx);
                            })),
                    )
                    .child(
                        icon_button("visual-redo", IconName::Redo2, "Redo")
                            .disabled(!enabled || !redo)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run_visual_history(true, window, cx);
                            })),
                    )
                    .child(
                        icon_button("visual-link", IconName::Link, "Insert or edit link")
                            .disabled(!enabled)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.visual
                                    .input
                                    .update(cx, BlockEditorState::request_link_edit);
                            })),
                    )
                    .child(
                        icon_button("visual-unlink", IconName::Link2Off, "Remove link")
                            .disabled(!enabled || !has_link)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.visual.input.update(cx, |state, cx| {
                                    state.set_link(None, cx);
                                    state.focus(window, cx);
                                });
                            })),
                    )
                    .child(
                        icon_button("visual-image", IconName::Image, "Insert a book picture")
                            .disabled(!enabled || pictures.is_empty())
                            .dropdown_menu(move |menu, _, _| {
                                pictures.iter().fold(menu, |menu, (name, source)| {
                                    let input = input.clone();
                                    let source = source.clone();
                                    menu.item(PopupMenuItem::new(name.clone()).on_click(
                                        move |_, window, cx| {
                                            input.update(cx, |state, cx| {
                                                state.paste(
                                                    ClipboardItem::new_string(source.clone()),
                                                    false,
                                                    cx,
                                                );
                                                state.focus(window, cx);
                                            });
                                        },
                                    ))
                                })
                            }),
                    )
                    .children(code.then(|| self.code_language())),
            )
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
        icon_button("visual-code-language", IconName::FileCode, "Code language").dropdown_menu(
            move |menu, _, _| {
                [
                    ("Plain text", None),
                    ("Bash", Some("bash")),
                    ("C", Some("c")),
                    ("C++", Some("cpp")),
                    ("Go", Some("go")),
                    ("Java", Some("java")),
                    ("JavaScript", Some("javascript")),
                    ("JSON", Some("json")),
                    ("Python", Some("python")),
                    ("Rust", Some("rust")),
                    ("SQL", Some("sql")),
                    ("TypeScript", Some("typescript")),
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
            },
        )
    }
}

// Native Edit menu actions use Kit's input types. The block editor's
// clipboard actions are available through GPUI's action registry.
fn visual_clipboard_action(name: &str, window: &mut Window, cx: &mut App) {
    if let Ok(action) = cx.build_action(name, None) {
        window.dispatch_action(action, cx);
    }
}

fn picture_markdown(chapter: &str, picture: &str) -> String {
    const ESCAPED: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'/')
        .remove(b'.')
        .remove(b'-')
        .remove(b'_');
    let directory = "../".repeat(chapter.bytes().filter(|&byte| byte == b'/').count());
    let encoded = utf8_percent_encode(picture, ESCAPED);
    let name = picture.rsplit('/').next().unwrap_or(picture);
    let alt: String = name
        .strip_suffix(".avif")
        .unwrap_or(name)
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, ' ' | '-' | '.') {
                ch
            } else {
                ' '
            }
        })
        .collect();
    format!("![{}]({directory}{encoded})", alt.trim())
}

#[cfg(test)]
mod tests {
    use super::picture_markdown;

    #[test]
    fn a_book_picture_uses_an_encoded_path_relative_to_the_chapter() {
        assert_eq!(
            picture_markdown("part/intro/ch.md", "img/cover #1.avif"),
            "![cover  1](../../img/cover%20%231.avif)"
        );
    }
}
