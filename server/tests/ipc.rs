/// End-to-end ZMQ + protobuf integration tests.
///
/// These tests start the real [`ipc::spawn_zmq_thread`] server, connect a
/// genuine ZMQ REQ socket client, and exchange protobuf-encoded messages over
/// TCP loopback — exercising the full IPC pipeline without any GPU.
use std::convert::TryFrom;
use std::sync::{Arc, RwLock};

use prost::Message;
use zeromq::{Socket, SocketRecv, SocketSend};

use vstimd::ipc;
use vstimd::proto;
use vstimd::proto::request;
use vstimd::scene::SceneState;

// ── helpers ───────────────────────────────────────────────────────────────────

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn round_trip(client: &mut zeromq::ReqSocket, req: proto::Request) -> proto::Response {
    let bytes = req.encode_to_vec();
    client.send(bytes.into()).await.unwrap();
    let msg = client.recv().await.unwrap();
    let resp_bytes = Vec::<u8>::try_from(msg).expect("response should be a single frame");
    proto::Response::decode(resp_bytes.as_slice()).unwrap()
}

fn with_zmq_server<F>(f: F)
where
    F: FnOnce(
        zeromq::ReqSocket,
        Arc<RwLock<SceneState>>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>,
{
    let scene = Arc::new(RwLock::new(SceneState::new()));
    let port = free_port();
    let bind_addr = format!("tcp://127.0.0.1:{port}");
    let connect_addr = format!("tcp://127.0.0.1:{port}");

    let _server = ipc::spawn_zmq_thread(Arc::clone(&scene), None, &bind_addr);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let mut client = zeromq::ReqSocket::new();
        client.connect(&connect_addr).await.unwrap();
        f(client, scene).await;
    });
}

fn is_ok(resp: &proto::Response) -> bool {
    resp.code == proto::ErrorCode::Ok as i32
}

fn sys() -> request::Target {
    request::Target::System(proto::SystemTarget {})
}

fn stim(handle: u32) -> request::Target {
    request::Target::Stimulus(handle)
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[test]
fn test_zmq_create_rect() {
    with_zmq_server(|mut client, scene| {
        Box::pin(async move {
            let resp = round_trip(
                &mut client,
                proto::Request {
                    target: Some(sys()),
                    body: Some(request::Body::CreateRect(proto::CreateRectRequest {
                        center: Some(proto::Vec2 { x: 10.0, y: -20.0 }),
                        width: 200.0,
                        height: 100.0,
                        fill_color: Some(proto::Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
                        ..Default::default()
                    })),
                },
            )
            .await;

            assert!(is_ok(&resp), "unexpected error: {}", resp.error);
            let handle = resp.handle as u32;
            assert!(handle > 0);
            assert!(scene.read().unwrap().stimuli.contains_key(&handle));
        })
    });
}

#[test]
fn test_zmq_lifecycle() {
    with_zmq_server(|mut client, scene| {
        Box::pin(async move {
            // ── Create ──────────────────────────────────────────────────────
            let resp = round_trip(
                &mut client,
                proto::Request {
                    target: Some(sys()),
                    body: Some(request::Body::CreateRect(proto::CreateRectRequest {
                        width: 100.0,
                        height: 50.0,
                        ..Default::default()
                    })),
                },
            )
            .await;

            assert!(is_ok(&resp), "create error: {}", resp.error);
            let handle = resp.handle as u32;
            assert!(scene.read().unwrap().stimuli[&handle].stimulus.flags().enabled);

            // ── Disable ─────────────────────────────────────────────────────
            let resp = round_trip(
                &mut client,
                proto::Request {
                    target: Some(stim(handle)),
                    body: Some(request::Body::SetEnabled(proto::SetEnabledRequest { enabled: false })),
                },
            )
            .await;

            assert!(is_ok(&resp), "disable error: {}", resp.error);
            assert_eq!(resp.handle, -1);
            assert!(!scene.read().unwrap().stimuli[&handle].stimulus.flags().enabled);

            // ── Re-enable ────────────────────────────────────────────────────
            let resp = round_trip(
                &mut client,
                proto::Request {
                    target: Some(stim(handle)),
                    body: Some(request::Body::SetEnabled(proto::SetEnabledRequest { enabled: true })),
                },
            )
            .await;

            assert!(is_ok(&resp), "re-enable error: {}", resp.error);
            assert!(scene.read().unwrap().stimuli[&handle].stimulus.flags().enabled);

            // ── Delete ───────────────────────────────────────────────────────
            let resp = round_trip(
                &mut client,
                proto::Request {
                    target: Some(stim(handle)),
                    body: Some(request::Body::Delete(proto::DeleteRequest {})),
                },
            )
            .await;

            assert!(is_ok(&resp), "delete error: {}", resp.error);
            assert_eq!(resp.handle, -1);
            assert!(!scene.read().unwrap().stimuli.contains_key(&handle));
        })
    });
}

#[test]
fn test_zmq_error_on_bad_handle() {
    with_zmq_server(|mut client, _scene| {
        Box::pin(async move {
            let resp = round_trip(
                &mut client,
                proto::Request {
                    target: Some(stim(9999)),
                    body: Some(request::Body::Delete(proto::DeleteRequest {})),
                },
            )
            .await;

            assert!(!is_ok(&resp), "expected an error response");
            assert_eq!(resp.handle, 0);
            assert_eq!(resp.code, proto::ErrorCode::HandleNotFound as i32);
            assert!(resp.error.contains("9999"));
        })
    });
}

#[test]
fn test_zmq_multiple_stimuli() {
    with_zmq_server(|mut client, scene| {
        Box::pin(async move {
            let mut handles = Vec::new();
            for i in 0u32..3 {
                let resp = round_trip(
                    &mut client,
                    proto::Request {
                        target: Some(sys()),
                        body: Some(request::Body::CreateRect(proto::CreateRectRequest {
                            width: 50.0 * (i + 1) as f32,
                            height: 50.0,
                            ..Default::default()
                        })),
                    },
                )
                .await;

                assert!(is_ok(&resp), "create {i} error: {}", resp.error);
                handles.push(resp.handle as u32);
            }

            {
                let scene = scene.read().unwrap();
                for &h in &handles {
                    assert!(scene.stimuli.contains_key(&h), "handle {h} missing");
                }
                assert_eq!(scene.stimuli.len(), 3);
            }

            let middle = handles[1];
            let resp = round_trip(
                &mut client,
                proto::Request {
                    target: Some(stim(middle)),
                    body: Some(request::Body::Delete(proto::DeleteRequest {})),
                },
            )
            .await;

            assert!(is_ok(&resp));
            let scene = scene.read().unwrap();
            assert!(!scene.stimuli.contains_key(&middle));
            assert!(scene.stimuli.contains_key(&handles[0]));
            assert!(scene.stimuli.contains_key(&handles[2]));
            assert_eq!(scene.stimuli.len(), 2);
        })
    });
}
