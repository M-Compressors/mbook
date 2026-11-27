//! The thread that serves MCP over HTTP.
//!
//! The thread runs a current-thread runtime of `tokio` with `axum` and the
//! Streamable HTTP service of `rmcp`, which refuses a request with an
//! `Origin` or for a `Host` other than the loopback address and the port. It touches nothing of the library: each
//! call goes to the main thread through the [`Bridge`] (mbook-ui docs,
//! "Agents (MCP)").

use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::Arc;
use std::thread;

use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio_util::sync::CancellationToken;

use super::server::{Bridge, Server};

/// The port the server asks for first.
pub(crate) const PORT: u16 = 7341;
/// The last port of the range tried before any free port.
const LAST_PORT: u16 = 7350;
/// The largest request body, the frame limit of `mbook`.
const MAX_BODY: usize = 64 * 1024 * 1024;
/// The path of the endpoint.
pub(crate) const PATH: &str = "/mcp";

/// Why the server did not start.
#[derive(Debug, thiserror::Error)]
pub(crate) enum StartError {
    /// No port could be bound on the loopback address.
    #[error("no port could be opened on 127.0.0.1")]
    Bind(#[source] std::io::Error),
    /// The thread or its runtime could not be made.
    #[error("the server thread could not start")]
    Thread(#[source] std::io::Error),
}

/// A server that runs. Dropping it stops the server: the thread finishes
/// the requests in flight and ends.
#[derive(Debug)]
pub(crate) struct Running {
    address: SocketAddr,
    stop: CancellationToken,
}

impl Running {
    /// The URL of the endpoint.
    pub(crate) fn url(&self) -> String {
        format!("http://{}{PATH}", self.address)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

/// Binds the first free port of 7341 to 7350 on 127.0.0.1, then any free
/// one.
fn bind() -> Result<TcpListener, std::io::Error> {
    let mut last = None;
    for port in PORT..=LAST_PORT {
        match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
            Ok(listener) => return Ok(listener),
            Err(error) => last = Some(error),
        }
    }
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|error| last.unwrap_or(error))
}

/// Starts the server: binds the first free port of 7341 to 7350 on
/// 127.0.0.1, or any free one, and serves on a thread of its own.
///
/// The port and the runtime are made here, so that a failure is told at
/// once and the thread hands nothing back: binding a port of the loopback
/// address returns at once.
///
/// # Errors
///
/// When no port can be bound, or the runtime or the thread cannot be made.
pub(crate) fn start(bridge: Bridge) -> Result<Running, StartError> {
    let listener = bind().map_err(StartError::Bind)?;
    listener.set_nonblocking(true).map_err(StartError::Bind)?;
    let address = listener.local_addr().map_err(StartError::Bind)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(StartError::Thread)?;
    let stop = CancellationToken::new();
    let serving = stop.clone();
    thread::Builder::new()
        .name("mbook-mcp".into())
        .spawn(move || serve(&runtime, listener, address, bridge, &serving))
        .map_err(StartError::Thread)?;
    Ok(Running { address, stop })
}

/// The body of the thread: serves until `stop`.
fn serve(
    runtime: &tokio::runtime::Runtime,
    listener: TcpListener,
    address: SocketAddr,
    bridge: Bridge,
    stop: &CancellationToken,
) {
    runtime.block_on(async move {
        // Made from a listener that is bound and non-blocking, inside the
        // runtime: it does not fail.
        let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
            return;
        };
        let router = router(bridge, address, stop.child_token());
        let stopped = stop.clone();
        // An error of the listener ends the server.
        drop(
            axum::serve(listener, router)
                .with_graceful_shutdown(async move { stopped.cancelled().await })
                .await,
        );
    });
}

/// The routes: the MCP endpoint.
pub(super) fn router(bridge: Bridge, address: SocketAddr, stop: CancellationToken) -> axum::Router {
    let port = address.port();
    let config = StreamableHttpServerConfig::default()
        .with_allowed_hosts([format!("127.0.0.1:{port}"), format!("localhost:{port}")])
        .enforce_origin_validation()
        .with_max_request_body_bytes(MAX_BODY)
        .with_cancellation_token(stop);
    let service = StreamableHttpService::new(
        move || Ok(Server::new(bridge.clone())),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    axum::Router::new().nest_service(PATH, service)
}
