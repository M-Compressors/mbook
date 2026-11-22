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

use crate::shared::ui::icon_button;

pub(super) fn formatting(input: &Entity<BlockEditorState>, cx: &App) -> impl IntoElement {
    let editor = input.read(cx);
    let model = editor.model();
    let enabled = !editor.is_readonly();
    let kind = editor.block_kind_at_caret();
    let marks_enabled = enabled && kind.is_some() && kind != Some(BlockKind::CodeBlock);
    let marks = [
        ("format-bold", IconName::Bold, "Bold", MarkKind::Bold),
        (
            "format-italic",
            IconName::Italic,
            "Italic",
            MarkKind::Italic,
        ),
        (
            "format-strike",
            IconName::Strikethrough,
            "Strikethrough",
            MarkKind::Strikethrough,
        ),
        ("format-code", IconName::Code, "Inline code", MarkKind::Code),
    ];
    let structure = [
        (
            "format-quote",
            IconName::TextQuote,
            "Quote",
            EditorCommand::ToggleBlockquote,
            kind.is_some() && kind != Some(BlockKind::TableCell),
            model.in_blockquote(),
        ),
        (
            "format-outdent",
            IconName::ListIndentDecrease,
            "Decrease list indent",
            EditorCommand::Outdent,
            model.can_outdent(),
            false,
        ),
        (
            "format-indent",
            IconName::ListIndentIncrease,
            "Increase list indent",
            EditorCommand::Indent,
            model.can_indent(),
            false,
        ),
    ];
    h_flex()
        .id("visual-formatting")
        .role(Role::Toolbar)
        .aria_label("Text formatting")
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
        Some(BlockKind::CodeBlock) => "Code block".to_owned(),
        Some(BlockKind::Heading { level }) => format!("Heading {level}"),
        _ => "Normal text".to_owned(),
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
                        "Normal text",
                        EditorCommand::SetBlockKind(BlockKind::Paragraph),
                    )
                    .checked(kind == Some(BlockKind::Paragraph)),
                ),
                |menu, level| {
                    let heading = BlockKind::Heading { level };
                    menu.item(
                        command_item(
                            &input,
                            format!("Heading {level}"),
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
        .label("Insert")
        .dropdown_caret(true)
        .disabled(!enabled)
        .dropdown_menu(move |menu, _, _| {
            [
                ("Table", EditorCommand::InsertTable { rows: 3, cols: 3 }),
                ("Code block", EditorCommand::SetCodeBlock { lang: None }),
                (
                    "Horizontal rule",
                    EditorCommand::InsertBlock(BlockKind::HorizontalRule),
                ),
                (
                    "Bulleted list",
                    EditorCommand::ToggleList { ordered: false },
                ),
                ("Numbered list", EditorCommand::ToggleList { ordered: true }),
                ("Task list", EditorCommand::ToggleTask),
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
