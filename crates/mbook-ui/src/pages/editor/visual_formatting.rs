//! Formatting controls for the block editor.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Disableable as _, Selectable as _, h_flex};
use gpui_kit::{
    App, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, Window,
};
use markdown_editor::{BlockEditorState, BlockKind, EditorCommand, MarkKind, MarkState};
use rust_i18n::t;

use crate::shared::ui::icon_button;

pub(super) fn formatting(input: &Entity<BlockEditorState>, cx: &App) -> impl IntoElement {
    let editor = input.read(cx);
    let model = editor.model();
    let enabled = !editor.is_readonly();
    let kind = editor.block_kind_at_caret();
    let marks_enabled = enabled && kind.is_some() && kind != Some(BlockKind::CodeBlock);
    let marks = [
        (
            "format-bold",
            IconName::Bold,
            t!("visual.bold"),
            MarkKind::Bold,
        ),
        (
            "format-italic",
            IconName::Italic,
            t!("visual.italic"),
            MarkKind::Italic,
        ),
        (
            "format-strike",
            IconName::Strikethrough,
            t!("visual.strikethrough"),
            MarkKind::Strikethrough,
        ),
        (
            "format-code",
            IconName::Code,
            t!("visual.inline_code"),
            MarkKind::Code,
        ),
    ];
    let structure = [
        (
            "format-quote",
            IconName::TextQuote,
            t!("visual.quote"),
            EditorCommand::ToggleBlockquote,
            kind.is_some() && kind != Some(BlockKind::TableCell),
            model.in_blockquote(),
        ),
        (
            "format-outdent",
            IconName::ListIndentDecrease,
            t!("visual.outdent"),
            EditorCommand::Outdent,
            model.can_outdent(),
            false,
        ),
        (
            "format-indent",
            IconName::ListIndentIncrease,
            t!("visual.indent"),
            EditorCommand::Indent,
            model.can_indent(),
            false,
        ),
    ];
    h_flex()
        .id("visual-formatting")
        .role(Role::Toolbar)
        .aria_label(t!("visual.formatting"))
        .gap_1()
        .flex_wrap()
        .child(style_menu(input.clone(), kind, enabled))
        .children(marks.into_iter().map(|(id, icon, label, mark)| {
            let input = input.clone();
            icon_button(id, icon, label)
                .disabled(!marks_enabled)
                .selected(model.mark_state(mark) == MarkState::On)
                .on_click(move |_, window, cx| {
                    run(&input, EditorCommand::ToggleMark(mark), window, cx);
                })
        }))
        .children(
            structure
                .into_iter()
                .map(|(id, icon, label, command, can_run, selected)| {
                    let input = input.clone();
                    icon_button(id, icon, label)
                        .disabled(!enabled || !can_run)
                        .selected(selected)
                        .on_click(move |_, window, cx| run(&input, command.clone(), window, cx))
                }),
        )
}

fn style_menu(
    input: Entity<BlockEditorState>,
    kind: Option<BlockKind>,
    enabled: bool,
) -> impl IntoElement {
    let label = match kind {
        Some(BlockKind::CodeBlock) => t!("visual.code_block"),
        Some(BlockKind::Heading { level }) => t!("visual.heading", level = level),
        _ => t!("visual.normal_text"),
    };
    Button::new("visual-paragraph-style")
        .ghost()
        .label(label)
        .dropdown_caret(true)
        .disabled(!enabled || kind.is_none())
        .dropdown_menu(move |menu, _, _| {
            (1..=6).fold(
                menu.item(
                    command_item(
                        &input,
                        t!("visual.normal_text"),
                        EditorCommand::SetBlockKind(BlockKind::Paragraph),
                    )
                    .checked(kind == Some(BlockKind::Paragraph)),
                ),
                |menu, level| {
                    let heading = BlockKind::Heading { level };
                    menu.item(
                        command_item(
                            &input,
                            t!("visual.heading", level = level),
                            EditorCommand::SetBlockKind(heading),
                        )
                        .checked(kind == Some(heading)),
                    )
                },
            )
        })
}

/// The menu of the blocks a chapter can get: a table, a code block, a rule
/// and the lists.
pub(super) fn insert_menu(input: Entity<BlockEditorState>, enabled: bool) -> impl IntoElement {
    Button::new("visual-insert")
        .ghost()
        .label(t!("visual.insert"))
        .dropdown_caret(true)
        .disabled(!enabled)
        .dropdown_menu(move |menu, _, _| {
            [
                (
                    t!("visual.table"),
                    EditorCommand::InsertTable { rows: 3, cols: 3 },
                ),
                (
                    t!("visual.code_block"),
                    EditorCommand::SetCodeBlock { lang: None },
                ),
                (
                    t!("visual.horizontal_rule"),
                    EditorCommand::InsertBlock(BlockKind::HorizontalRule),
                ),
                (
                    t!("visual.bulleted_list"),
                    EditorCommand::ToggleList { ordered: false },
                ),
                (
                    t!("visual.numbered_list"),
                    EditorCommand::ToggleList { ordered: true },
                ),
                (t!("visual.task_list"), EditorCommand::ToggleTask),
            ]
            .into_iter()
            .fold(menu, |menu, (label, command)| {
                menu.item(command_item(&input, label, command))
            })
        })
}

fn command_item(
    input: &Entity<BlockEditorState>,
    label: impl Into<gpui_kit::SharedString>,
    command: EditorCommand,
) -> PopupMenuItem {
    let input = input.clone();
    PopupMenuItem::new(label)
        .on_click(move |_, window, cx| run(&input, command.clone(), window, cx))
}

fn run(
    input: &Entity<BlockEditorState>,
    command: EditorCommand,
    window: &mut Window,
    cx: &mut App,
) {
    input.update(cx, |state, cx| {
        let _ = state.run(command, cx);
        state.focus(window, cx);
    });
}
