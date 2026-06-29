//! Embedded HTTP + WebSocket control surface for the browser UI.
//!
//! Mirrors the design of [`crate::ipc`] (the ZMQ server): a dedicated thread
//! with its own current-thread tokio runtime, sharing the same
//! `Arc<RwLock<SceneState>>` / `Arc<Mutex<VtlState>>` as the render and ZMQ
//! threads.
//!
//! Two WebSocket endpoints, each carrying exactly one message type (no
//! multiplexing, no envelope):
//!
//! * **`/ws` -- command channel:** client sends `proto::Request`, server replies
//!   `proto::Response`. Pure REQ/REP, dispatched through
//!   [`SceneState::handle_request`] exactly like the ZMQ path.
//! * **`/events` -- state channel:** server pushes `proto::SceneSnapshot` frames;
//!   the client only receives.
//!
//! Snapshots are built once per tick under a *read* lock and fanned out to all
//! connected clients via a `tokio::sync::broadcast` channel, so the cost is
//! independent of client count and skipped entirely when nobody is connected.
//! The render thread is never part of this path.

use std::sync::{Arc, Mutex, RwLock};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
#[cfg(not(feature = "embed-ui"))]
use axum::response::Html;
use futures_util::{SinkExt, StreamExt};
use prost::Message as _;

use crate::proto;
use crate::scene::SceneState;
use crate::vtl_state::VtlState;

/// Default HTTP port for the web control surface.
pub const DEFAULT_WEB_PORT: u16 = 8080;

/// Snapshot push interval (~30 Hz).
const SNAPSHOT_PERIOD: std::time::Duration = std::time::Duration::from_millis(33);

struct WebState {
    scene: Arc<RwLock<SceneState>>,
    vtl: Option<Arc<Mutex<VtlState>>>,
    /// Encoded `SceneSnapshot` bytes, fanned out to all `/events` clients.
    snapshots: tokio::sync::broadcast::Sender<Vec<u8>>,
}

/// Spawn the web server on a dedicated thread with its own tokio runtime.
///
/// Returns the `JoinHandle`, a shutdown sender (drop or send to stop), and a
/// bound receiver that fires once the listener is accepting connections.
pub fn spawn_web_thread(
    scene: Arc<RwLock<SceneState>>,
    vtl: Option<Arc<Mutex<VtlState>>>,
    bind_addr: &str,
) -> (
    std::thread::JoinHandle<()>,
    tokio::sync::oneshot::Sender<()>,
    std::sync::mpsc::Receiver<()>,
) {
    let addr = bind_addr.to_owned();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (bound_tx, bound_rx) = std::sync::mpsc::sync_channel::<()>(1);
    let handle = std::thread::Builder::new()
        .name("web-server".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create tokio runtime for web thread");
            rt.block_on(web_loop(scene, vtl, &addr, shutdown_rx, bound_tx));
        })
        .expect("failed to spawn web server thread");
    (handle, shutdown_tx, bound_rx)
}

async fn web_loop(
    scene: Arc<RwLock<SceneState>>,
    vtl: Option<Arc<Mutex<VtlState>>>,
    addr: &str,
    shutdown: tokio::sync::oneshot::Receiver<()>,
    bound_tx: std::sync::mpsc::SyncSender<()>,
) {
    let (snapshots, _) = tokio::sync::broadcast::channel::<Vec<u8>>(8);

    let state = Arc::new(WebState {
        scene: scene.clone(),
        vtl: vtl.clone(),
        snapshots: snapshots.clone(),
    });

    // Snapshot pump: build once per tick under a read lock, broadcast to clients.
    // Skips work entirely when no `/events` client is subscribed.
    {
        let scene = scene.clone();
        let vtl = vtl.clone();
        let snapshots = snapshots.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(SNAPSHOT_PERIOD);
            loop {
                ticker.tick().await;
                if snapshots.receiver_count() == 0 {
                    continue;
                }
                let snap = {
                    let s = scene.read().expect("scene lock poisoned");
                    let vg = vtl.as_ref().and_then(|v| v.lock().ok());
                    s.build_snapshot(vg.as_deref())
                };
                let _ = snapshots.send(snap.encode_to_vec());
            }
        });
    }

    let app = {
        let r = Router::new()
            .route("/ws", get(ws_command_upgrade))
            .route("/events", get(ws_events_upgrade));
        // With `embed-ui`, serve the baked React bundle (SPA fallback). Without
        // it, serve a small placeholder at `/`.
        #[cfg(feature = "embed-ui")]
        let r = r.fallback(static_handler);
        #[cfg(not(feature = "embed-ui"))]
        let r = r.route("/", get(index));
        r.with_state(state)
    };

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            log::error!("web: bind to {addr} failed: {e}");
            return;
        }
    };
    log::info!("web: HTTP/WebSocket server listening on http://{addr}");
    let _ = bound_tx.try_send(());

    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        let _ = shutdown.await;
        log::info!("vstimd: web server shutting down");
    });
    if let Err(e) = server.await {
        log::error!("web: server error: {e}");
    }
}

#[cfg(not(feature = "embed-ui"))]
async fn index() -> impl IntoResponse {
    // Placeholder when the UI is not embedded (build with `--features embed-ui`
    // after `npm run build` to serve the real React app here). The WebSocket API
    // is the real surface and is what the e2e tests exercise.
    Html(concat!(
        "<!doctype html><meta charset=utf-8><title>vstimd</title>",
        "<h1>vstimd web control surface</h1>",
        "<p>Command channel: <code>/ws</code> (Request/Response).<br>",
        "State stream: <code>/events</code> (SceneSnapshot push).</p>"
    ))
}

// ── Command channel: /ws ──────────────────────────────────────────────────────

async fn ws_command_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| command_socket(socket, state))
}

/// Pure REQ/REP loop: decode `Request`, dispatch, send `Response`.
async fn command_socket(mut socket: WebSocket, state: Arc<WebState>) {
    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Binary(data) => {
                let out = dispatch(&state, &data);
                if socket.send(Message::Binary(out.into())).await.is_err() {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}

/// Decode a `Request`, dispatch it through the shared scene exactly like the ZMQ
/// path (`crate::ipc`), and return the encoded `Response` bytes.
fn dispatch(state: &WebState, data: &[u8]) -> Vec<u8> {
    let resp = match proto::Request::decode(data) {
        Ok(req) => {
            let mut scene = state.scene.write().expect("scene lock poisoned");
            let mut vtl_guard = state.vtl.as_ref().and_then(|v| v.lock().ok());
            let mut resp = scene.handle_request(req, vtl_guard.as_deref_mut());
            resp.frame_count = scene.runtime.frame_count;
            resp.server_time_ns = scene.runtime.server_start.elapsed().as_nanos() as u64;
            resp
        }
        Err(e) => proto::Response {
            code: proto::ErrorCode::Unknown as i32,
            error: format!("protobuf decode error: {e}"),
            ..Default::default()
        },
    };
    resp.encode_to_vec()
}

// ── State channel: /events ────────────────────────────────────────────────────

async fn ws_events_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| events_socket(socket, state))
}

/// Push-only loop: forward broadcast `SceneSnapshot` frames to the client and
/// drain any incoming frames (so close/ping are handled promptly).
async fn events_socket(socket: WebSocket, state: Arc<WebState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut snap_rx = state.snapshots.subscribe();

    let push = tokio::spawn(async move {
        loop {
            match snap_rx.recv().await {
                Ok(bytes) => {
                    if sender.send(Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Drain inbound frames until the client closes; this also lets us notice the
    // socket closing so we can abort the push task.
    while let Some(Ok(msg)) = receiver.next().await {
        if matches!(msg, Message::Close(_)) {
            break;
        }
    }
    push.abort();
    let _ = push.await;
}

// ── Embedded React bundle (feature = "embed-ui") ──────────────────────────────

#[cfg(feature = "embed-ui")]
#[derive(rust_embed::RustEmbed)]
#[folder = "../client/web/dist"]
struct Assets;

/// Serve an embedded asset by path, falling back to index.html for SPA routes.
#[cfg(feature = "embed-ui")]
async fn static_handler(uri: axum::http::Uri) -> axum::response::Response {
    use axum::http::{header, StatusCode};
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    if let Some(file) = Assets::get(path) {
        return (
            [(header::CONTENT_TYPE, file.metadata.mimetype())],
            file.data,
        )
            .into_response();
    }
    // Unknown path → SPA fallback to index.html.
    match Assets::get("index.html") {
        Some(file) => ([(header::CONTENT_TYPE, "text/html")], file.data).into_response(),
        None => (StatusCode::NOT_FOUND, "UI not embedded").into_response(),
    }
}
