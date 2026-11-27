//! The MCP server of agents: on while the setting **Serve MCP** is on
//! (mbook-ui docs, "Agents (MCP)").
//!
//! - `http` serves MCP on a thread of its own,
//! - `server` is the handler of MCP: the tools, the skill and the prompts,
//! - `bridge` carries a call from that thread to the main thread,
//! - `dispatch` answers it there,
//! - `skill` holds what the agent is told,
//! - `pictures` turns what an agent gives into AVIF and pictures into PNG.
//!
//! What the settings show of the server is
//! [`AgentStatus`](crate::features::agents::AgentStatus).

mod bridge;
mod dispatch;
mod http;
mod pictures;
pub(crate) mod quit;
mod server;
mod skill;

use gpui_kit::{App, AppContext as _, Context, Entity, Global, Subscription, Task};

use self::bridge::Bridge;
pub(crate) use self::dispatch::MainWindow;
#[cfg(feature = "visual")]
pub(crate) use self::dispatch::show_confirmation;
use self::http::Running;
use crate::features::agents::{AgentStatus, Status};
use crate::features::appearance::Appearance;

/// The server, as the application keeps it.
pub(crate) struct AgentServer {
    /// The server that runs; dropping it stops the server.
    running: Option<Running>,
    starting: Option<Task<()>>,
    /// Answers the calls of the server that runs.
    calls: Option<Task<()>>,
    _settings: Subscription,
}

/// The entity of the server, as a global.
struct Handle {
    _server: Entity<AgentServer>,
}

impl Global for Handle {}

impl AgentServer {
    /// Makes the server follow its setting from now on.
    pub(crate) fn install(cx: &mut App) {
        let appearance = Appearance::global(cx);
        let server = cx.new(|cx| Self {
            running: None,
            starting: None,
            calls: None,
            _settings: cx.observe(&appearance, |this: &mut Self, _, cx| this.follow(cx)),
        });
        server.update(cx, Self::follow);
        cx.set_global(Handle { _server: server });
    }

    /// Tells the settings where the server is.
    fn tell(status: Status, cx: &mut App) {
        let shown = AgentStatus::global(cx);
        shown.update(cx, |shown, cx| shown.set(status, cx));
    }

    /// Starts or stops the server as the setting says.
    fn follow(&mut self, cx: &mut Context<'_, Self>) {
        let Some(appearance) = Appearance::try_global(cx) else {
            return;
        };
        let (loaded, on) = {
            let appearance = appearance.read(cx);
            (appearance.is_loaded(), appearance.settings().agents.serve)
        };
        if !loaded {
            return;
        }
        if !on {
            self.stop(cx);
            return;
        }
        if self.running.is_none() && self.starting.is_none() {
            self.start(cx);
        }
    }

    /// Opens the port and serves.
    fn start(&mut self, cx: &mut Context<'_, Self>) {
        let (bridge, calls) = Bridge::new();
        let starting = http::start(bridge);
        Self::tell(Status::Starting, cx);
        self.calls = Some(dispatch::listen(calls, cx));
        self.starting = Some(cx.spawn(async move |this, cx| {
            let started = starting.await;
            drop(this.update(cx, |this, cx| {
                this.starting = None;
                match started {
                    Ok(running) => {
                        Self::tell(Status::Running(running.url()), cx);
                        this.running = Some(running);
                    }
                    Err(error) => {
                        this.calls = None;
                        let reason = crate::features::edit_book::agent::error::chain(&error);
                        Self::tell(Status::Failed(reason), cx);
                    }
                }
            }));
        }));
    }

    /// Stops the server; the calls in flight end.
    fn stop(&mut self, cx: &mut Context<'_, Self>) {
        self.running = None;
        self.starting = None;
        self.calls = None;
        Self::tell(Status::Off, cx);
    }
}

#[cfg(test)]
mod tests;
