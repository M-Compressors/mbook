//! Agents: what the settings show of the MCP server and how a client reaches
//! it (mbook-ui docs, "Agents (MCP)", "Settings").
//!
//! The server itself is part of the application layer, which starts it when
//! **Serve MCP** is on and tells [`AgentStatus`] where it is.

use gpui_kit::{App, AppContext as _, Context, Entity, Global};

/// Where the server is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Status {
    /// The setting is off.
    #[default]
    Off,
    /// The port is being opened.
    Starting,
    /// It serves at this URL.
    Running(String),
    /// It could not start, for this reason.
    Failed(String),
}

/// What the settings show of the server.
#[derive(Debug, Default)]
pub(crate) struct AgentStatus {
    status: Status,
    /// A call of an agent is being answered.
    busy: usize,
}

struct Handle(Entity<AgentStatus>);

impl Global for Handle {}

impl AgentStatus {
    /// The status, made when first asked for.
    pub(crate) fn global(cx: &mut App) -> Entity<Self> {
        if let Some(handle) = cx.try_global::<Handle>() {
            return handle.0.clone();
        }
        let entity = cx.new(|_| Self::default());
        cx.set_global(Handle(entity.clone()));
        entity
    }

    /// The status, when it was made.
    pub(crate) fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<Handle>().map(|handle| handle.0.clone())
    }

    /// Where the server is.
    pub(crate) const fn status(&self) -> &Status {
        &self.status
    }

    /// Records where the server is.
    pub(crate) fn set(&mut self, status: Status, cx: &mut Context<'_, Self>) {
        if self.status != status {
            self.status = status;
            cx.notify();
        }
    }

    /// Whether an agent is at work: a call is being answered.
    pub(crate) const fn busy(&self) -> bool {
        self.busy > 0
    }

    /// A call began (`true`) or ended (`false`).
    pub(crate) fn working(&mut self, started: bool, cx: &mut Context<'_, Self>) {
        let before = self.busy();
        self.busy = if started {
            self.busy + 1
        } else {
            self.busy.saturating_sub(1)
        };
        if before != self.busy() {
            cx.notify();
        }
    }
}

/// A client whose configuration the settings offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Client {
    ClaudeCode,
    Cursor,
    Codex,
    /// Any other client of MCP over Streamable HTTP.
    Other,
}

impl Client {
    /// Every client, in the order of the settings.
    pub(crate) const ALL: [Self; 4] = [Self::ClaudeCode, Self::Cursor, Self::Codex, Self::Other];

    /// The name of the client.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Cursor => "Cursor",
            Self::Codex => "Codex",
            Self::Other => "Other",
        }
    }

    /// The language the configuration is written in, for its highlighting.
    pub(crate) const fn language(self) -> &'static str {
        match self {
            Self::ClaudeCode => "bash",
            Self::Cursor | Self::Other => "json",
            Self::Codex => "toml",
        }
    }

    /// An identifier for its control.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::ClaudeCode => "copy-claude-code",
            Self::Cursor => "copy-cursor",
            Self::Codex => "copy-codex",
            Self::Other => "copy-other",
        }
    }

    /// What the client is given to reach the server at `url`.
    pub(crate) fn configuration(self, url: &str) -> String {
        match self {
            Self::ClaudeCode => format!("claude mcp add --transport http mbook {url}"),
            Self::Cursor => format!(
                "{{\n  \"mcpServers\": {{\n    \"mbook\": {{\n      \"url\": \"{url}\"\n    }}\n  }}\n}}"
            ),
            Self::Codex => format!("[mcp_servers.mbook]\nurl = \"{url}\""),
            Self::Other => format!(
                "{{\n  \"mcpServers\": {{\n    \"mbook\": {{\n      \"type\": \"http\",\n      \"url\": \"{url}\"\n    }}\n  }}\n}}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_configuration_holds_the_address() {
        for client in Client::ALL {
            let text = client.configuration("http://127.0.0.1:7341/mcp");
            assert!(
                text.contains("http://127.0.0.1:7341/mcp"),
                "{}",
                client.name()
            );
            assert!(!text.contains("Authorization"), "{}", client.name());
            assert!(client.id().starts_with("copy-"));
        }
    }

    #[test]
    fn the_configuration_of_claude_code_is_one_command() {
        assert_eq!(
            Client::ClaudeCode.configuration("http://x/mcp"),
            "claude mcp add --transport http mbook http://x/mcp"
        );
    }

    #[test]
    fn the_configuration_of_any_other_client_is_the_common_json() {
        let text = Client::Other.configuration("http://x/mcp");
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["mcpServers"]["mbook"]["type"], "http");
        assert_eq!(Client::Other.language(), "json");
        assert_eq!(Client::ClaudeCode.language(), "bash");
        assert_eq!(Client::Codex.language(), "toml");
    }

    #[test]
    fn the_configuration_of_cursor_is_json() {
        let text = Client::Cursor.configuration("http://x/mcp");
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["mcpServers"]["mbook"]["url"], "http://x/mcp");
    }
}
