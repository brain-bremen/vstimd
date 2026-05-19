use std::sync::{Arc, RwLock};

use prost::Message;
use zeromq::{Socket, SocketRecv, SocketSend};

use crate::proto;
use crate::scene::SceneState;
use crate::scene::stimulus::Stimulus;

/// Spawn the ZMQ REP server on a dedicated thread with its own tokio runtime.
///
/// The thread receives protobuf-encoded `Request` messages, dispatches them to
/// [`SceneState::handle_request`] under a **write lock**, and sends back the
/// encoded `Response`.  The write lock is held only for the duration of a
/// single `handle_request` call; it is released before the next `recv` so the
/// render thread is never blocked for more than one command dispatch at a time.
///
/// # Why a dedicated thread + its own runtime?
///
/// The main thread is owned by `winit`'s event loop, which does not expose an
/// async executor.  A dedicated `std::thread` with a single-threaded
/// `tokio::runtime` lets us use `zeromq`'s async API without interfering with
/// the render loop.
///
/// # Bind address
///
/// Use a concrete IP, not a wildcard hostname.  The `zeromq` crate resolves
/// the host part as a DNS name, so `tcp://*:5555` (libzmq C convention) will
/// fail with a lookup error.  Use `tcp://0.0.0.0:5555` to bind on all
/// interfaces, or `tcp://127.0.0.1:5555` for loopback only.
///
/// Returns the `JoinHandle` for the thread (detach or join on shutdown).
pub fn spawn_zmq_thread(
    scene: Arc<RwLock<SceneState>>,
    bind_addr: &str,
) -> std::thread::JoinHandle<()> {
    let addr = bind_addr.to_owned();
    std::thread::Builder::new()
        .name("zmq-server".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create tokio runtime for ZMQ thread");
            rt.block_on(zmq_loop(scene, &addr));
        })
        .expect("failed to spawn ZMQ server thread")
}

// ── Response helpers (used by all stimulus command impls) ─────────────────────

pub(crate) fn ok_ack() -> proto::Response {
    proto::Response {
        handle: -1,
        code: proto::ErrorCode::Ok as i32,
        error: String::new(),
        body: None,
    }
}

pub(crate) fn ok_handle(h: u32) -> proto::Response {
    proto::Response {
        handle: h as i32,
        code: proto::ErrorCode::Ok as i32,
        error: String::new(),
        body: None,
    }
}

pub(crate) fn ok_body(body: proto::response::Body) -> proto::Response {
    proto::Response {
        handle: -1,
        code: proto::ErrorCode::Ok as i32,
        error: String::new(),
        body: Some(body),
    }
}

pub(crate) fn err(code: proto::ErrorCode, msg: impl Into<String>) -> proto::Response {
    proto::Response {
        handle: 0,
        code: code as i32,
        error: msg.into(),
        body: None,
    }
}

pub(crate) fn err_not_found(handle: u32) -> proto::Response {
    proto::Response {
        handle: 0,
        code: proto::ErrorCode::HandleNotFound as i32,
        error: format!("stimulus handle {} not found", handle),
        body: None,
    }
}

pub(crate) fn err_wrong_type(stim: &Stimulus, cmd: &str, expected: &str) -> proto::Response {
    proto::Response {
        handle: 0,
        code: proto::ErrorCode::WrongStimulusType as i32,
        error: format!(
            "{} requires a {} stimulus, got {}",
            cmd,
            expected,
            stim.type_name()
        ),
        body: None,
    }
}

async fn zmq_loop(scene: Arc<RwLock<SceneState>>, addr: &str) {
    let mut socket = zeromq::RepSocket::new();
    socket
        .bind(addr)
        .await
        .unwrap_or_else(|e| panic!("ZMQ bind to {addr} failed: {e}"));
    log::info!("ZMQ REP server listening on {addr}");

    loop {
        let msg = match socket.recv().await {
            Ok(m) => m,
            Err(e) => {
                log::error!("ZMQ recv error: {e}");
                continue;
            }
        };

        let bytes: Vec<u8> = msg
            .into_vec()
            .into_iter()
            .flat_map(|frame| frame.to_vec())
            .collect();

        let response = match proto::Request::decode(bytes.as_slice()) {
            Ok(req) => {
                let mut scene = scene.write().expect("scene lock poisoned");
                scene.handle_request(req)
            }
            Err(e) => proto::Response {
                handle: 0,
                code: proto::ErrorCode::Unknown as i32,
                error: format!("protobuf decode error: {e}"),
                body: None,
            },
        };

        let out = response.encode_to_vec();
        if let Err(e) = socket.send(out.into()).await {
            log::error!("ZMQ send error: {e}");
        }
    }
}
