//! The section **Agents (MCP)**: the switch of the server, where it is, and
//! the configuration of the client chosen below it, highlighted as the client
//! reads it (mbook-ui docs, "Agents (MCP)", "Settings").

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::text::{TextView, TextViewStyle};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClipboardItem, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, SharedString, StatefulInteractiveElement as _, StyleRefinement,
    Styled as _, TestSupportExt as _, Window, div, px,
};
use rust_i18n::t;

use super::{SettingsPage, change, value_text};
use crate::entities::catalogue::Settings;
use crate::features::agents::{AgentStatus, Client, Status};
use crate::shared::ui::{announce, target_size};

/// What the section says of the server.
pub(super) fn status_words(status: &Status) -> String {
    match status {
        Status::Off => t!("settings.mcp_off").into_owned(),
        Status::Starting => t!("settings.mcp_starting").into_owned(),
        Status::Running(url) => t!("settings.mcp_running", url = url).into_owned(),
        Status::Failed(reason) => t!("settings.mcp_failed", reason = reason).into_owned(),
    }
}

/// Where the configuration of a client goes.
pub(super) fn client_hint(client: Client) -> String {
    match client {
        Client::ClaudeCode => t!("settings.mcp_hint_claude_code"),
        Client::Cursor => t!("settings.mcp_hint_cursor"),
        Client::Codex => t!("settings.mcp_hint_codex"),
        Client::Other => t!("settings.mcp_hint_other"),
    }
    .into_owned()
}

/// The name of a client on its tab.
fn client_label(client: Client) -> SharedString {
    match client {
        Client::Other => t!("settings.mcp_other").into_owned().into(),
        other => other.name().into(),
    }
}

/// The configuration as Markdown: one fenced block in the language of the
/// client, which the text view highlights.
pub(super) fn preview(client: Client, url: &str) -> String {
    format!(
        "```{}\n{}\n```\n",
        client.language(),
        client.configuration(url)
    )
}

/// Puts text on the clipboard and says so.
fn copy(text: String, said: String, window: &mut Window, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    announce(said, window, cx);
}

impl SettingsPage {
    /// The section.
    pub(super) fn agents_group(
        &self,
        settings: &Settings,
        compact: bool,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let status = AgentStatus::try_global(cx)
            .map_or_else(Status::default, |status| status.read(cx).status().clone());
        let url = match &status {
            Status::Running(url) => Some(url.clone()),
            _ => None,
        };
        let card = url.map(|url| self.configuration_card(&url, compact, cx));
        v_flex()
            .w_full()
            .gap_3()
            .child(
                Switch::new("serve-mcp")
                    .label(SharedString::from(t!("settings.serve_mcp")))
                    .checked(settings.agents.serve)
                    .on_click(|checked, _, cx| {
                        let checked = *checked;
                        change(cx, |settings| settings.agents.serve = checked);
                    }),
            )
            .child(value_text("mcp-status", status_words(&status)))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(value_text("mcp-hint", t!("settings.mcp_hint"))),
            )
            .children(card)
            .into_any_element()
    }

    /// The preview of the configuration of the chosen client, with the bar
    /// that chooses the client and copies the configuration under it.
    fn configuration_card(
        &self,
        url: &str,
        compact: bool,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let client = self.client;
        let selected = Client::ALL
            .iter()
            .position(|each| *each == client)
            .unwrap_or_default();
        let page = cx.entity().downgrade();
        let configuration = client.configuration(url);
        let theme = cx.theme();
        let preview_name =
            SharedString::from(t!("settings.mcp_preview", client = client_label(client)));
        let clients = TabBar::new("mcp-clients")
            .segmented()
            .small()
            .selected_index(selected)
            .children(Client::ALL.map(|each| Tab::new().label(client_label(each))))
            .on_click(move |index, _, cx| {
                let Some(chosen) = Client::ALL.get(*index).copied() else {
                    return;
                };
                drop(page.update(cx, |page, cx| {
                    page.client = chosen;
                    cx.notify();
                }));
            });
        // A narrow window keeps the button beside the clients, as an icon
        // named by the label it would have.
        let copy_button = Button::new(client.id())
            .primary()
            .small()
            .icon(IconName::Copy)
            .when(compact, |button| button.tooltip(t!("settings.copy")))
            .when(!compact, |button| button.label(t!("settings.copy")))
            .min_h(px(target_size()))
            .on_click(move |_, window, cx| {
                let said = t!(
                    "settings.copied_configuration",
                    client = client_label(client)
                )
                .into_owned();
                copy(configuration.clone(), said, window, cx);
            });
        v_flex()
            .w_full()
            .child(
                v_flex()
                    .w_full()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .overflow_hidden()
                    .child(
                        div()
                            .id("mcp-preview")
                            .role(Role::Group)
                            .aria_label(preview_name)
                            .test_support()
                            .w_full()
                            .text_sm()
                            .child(
                                TextView::markdown("mcp-configuration", preview(client, url))
                                    .style(code_style(theme.background))
                                    .selectable(true),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .p_2()
                            .border_t_1()
                            .border_color(theme.border)
                            .bg(theme.muted)
                            .justify_between()
                            .flex_wrap()
                            .child(clients)
                            .child(copy_button),
                    ),
            )
            .child(
                div()
                    .pt_2()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(value_text("mcp-client-hint", client_hint(client))),
            )
            .into_any_element()
    }
}

/// The code block of the preview: the surface of the card, from edge to
/// edge, so that the card is its only frame.
fn code_style(surface: gpui_kit::Hsla) -> TextViewStyle {
    // Refines the style of the theme: its colours and highlighting stay.
    TextViewStyle::default().code_block(
        StyleRefinement::default()
            .bg(surface)
            .m_0()
            .rounded_none()
            .px_4()
            .py_3(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_is_said_in_words() {
        assert_eq!(status_words(&Status::Off), "The server is off.");
        assert!(
            status_words(&Status::Running("http://127.0.0.1:7341/mcp".into()))
                .contains("http://127.0.0.1:7341/mcp")
        );
        assert!(status_words(&Status::Failed("no port".into())).contains("no port"));
        assert_ne!(status_words(&Status::Starting), "");
    }

    #[test]
    fn the_preview_is_the_configuration_in_a_block_of_its_language() {
        let url = "http://127.0.0.1:7341/mcp";
        let text = preview(Client::Codex, url);
        assert_eq!(
            text,
            format!("```toml\n{}\n```\n", Client::Codex.configuration(url))
        );
    }

    #[test]
    fn every_client_says_where_its_configuration_goes() {
        for client in Client::ALL {
            assert_ne!(client_hint(client), "", "{}", client.name());
        }
        assert_eq!(client_label(Client::Other), "Other");
    }
}
