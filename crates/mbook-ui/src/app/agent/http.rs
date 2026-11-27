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

use futures::channel::oneshot;
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

/// Starts the server on a thread of its own. The future gives the address
/// once the port is bound.
///
/// The port is bound on that thread, so the caller does no I/O.
pub(crate) fn start(
    bridge: Bridge,
) -> impl Future<Output = Result<Running, StartError>> + Send + 'static {
    let (bound, answer) = oneshot::channel();
    let stop = CancellationToken::new();
    let serving = stop.clone();
    let spawned = thread::Builder::new()
        .name("mbook-mcp".into())
        .spawn(move || serve(bridge, &serving, bound));
    async move {
        spawned.map_err(StartError::Thread)?;
        let address = answer
            .await
            .unwrap_or_else(|_| Err(StartError::Thread(std::io::ErrorKind::Interrupted.into())))?;
        Ok(Running { address, stop })
    }
}

/// The body of the thread: binds, reports, and serves until `stop`.
fn serve(
    bridge: Bridge,
    stop: &CancellationToken,
    bound: oneshot::Sender<Result<SocketAddr, StartError>>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            drop(bound.send(Err(StartError::Thread(error))));
            return;
        }
    };
    let listener = bind()
        .and_then(|listener| {
            listener.set_nonblocking(true)?;
            let address = listener.local_addr()?;
            Ok((listener, address))
        })
        .map_err(StartError::Bind);
    let (listener, address) = match listener {
        Ok(bound) => bound,
        Err(error) => {
            drop(bound.send(Err(error)));
            return;
        }
    };
    runtime.block_on(async move {
        let listener = match tokio::net::TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(error) => {
                drop(bound.send(Err(StartError::Bind(error))));
                return;
            }
        };
        let router = router(bridge, address, stop.child_token());
        drop(bound.send(Ok(address)));
        let stopped = stop.clone();
        // An error of the listener ends the server; the settings then show it
        // as stopped when the application asks again.
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
