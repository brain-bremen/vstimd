/// Integration tests for protobuf command dispatch.
///
/// These tests call `handle_request` directly on a `SceneState` — no ZMQ, no
/// GPU required — so they can run in any environment.
use prost::Message;
use wonderlamp_server::proto;
use wonderlamp_server::scene::{SceneState, Stimulus};

// ── helpers ───────────────────────────────────────────────────────────────────

fn create_rect_req(handle: u32, cmd: proto::CreateRect) -> proto::Request {
    proto::Request {
        handle,
        body: Some(proto::request::Body::CreateRect(cmd)),
    }
}

fn set_enabled_req(handle: u32, enabled: bool) -> proto::Request {
    proto::Request {
        handle,
        body: Some(proto::request::Body::SetEnabled(proto::SetEnabled {
            enabled,
        })),
    }
}

fn delete_req(handle: u32) -> proto::Request {
    proto::Request {
        handle,
        body: Some(proto::request::Body::Delete(proto::Delete {})),
    }
}

fn is_ok(resp: &proto::Response) -> bool {
    resp.error.is_empty()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[test]
fn test_create_rect() {
    let mut scene = SceneState::new();
    let resp = scene.handle_request(create_rect_req(
        0,
        proto::CreateRect {
            center: Some(proto::Vec2 { x: 10.0, y: -20.0 }),
            width: 200.0,
            height: 100.0,
            fill: None,
        },
    ));
    assert!(is_ok(&resp), "unexpected error: {}", resp.error);
    let new_handle = resp.handle as u32;
    assert!(new_handle > 0, "handle should be positive");
    assert!(scene.stimuli.contains_key(&new_handle), "stimulus should exist in scene");
}

#[test]
fn test_create_rect_with_fill() {
    let mut scene = SceneState::new();
    let fill = proto::Color { r: 0.5, g: 0.25, b: 0.75, a: 1.0 };
    let resp = scene.handle_request(create_rect_req(
        0,
        proto::CreateRect {
            center: None,
            width: 0.0,
            height: 0.0,
            fill: Some(fill.clone()),
        },
    ));
    assert!(is_ok(&resp));
    let h = resp.handle as u32;
    let stim = scene.stimuli.get_mut(&h).unwrap();
    let appearance = stim.appearance_mut().expect("rect should have appearance");
    assert_eq!(appearance.live.fill_color[0], fill.r);
    assert_eq!(appearance.live.fill_color[1], fill.g);
    assert_eq!(appearance.live.fill_color[2], fill.b);
    assert_eq!(appearance.live.fill_color[3], fill.a);
}

#[test]
fn test_create_rect_defaults() {
    let mut scene = SceneState::new();
    let default_fill = scene.default_fill;
    // All proto fields at zero / absent → should use defaults (100×100, white).
    let resp = scene.handle_request(create_rect_req(
        0,
        proto::CreateRect {
            center: None,
            width: 0.0,
            height: 0.0,
            fill: None,
        },
    ));
    assert!(is_ok(&resp));
    let h = resp.handle as u32;
    let stim = scene.stimuli.get_mut(&h).unwrap();

    // Half-extents of [50, 50] correspond to 100×100 default.
    if let Stimulus::Rect(r) = stim {
        assert_eq!(r.size.live, [50.0, 50.0]);
    } else {
        panic!("expected Rect stimulus");
    }

    // Fill should be the scene default (white = [1,1,1,1]).
    let stim = scene.stimuli.get_mut(&h).unwrap();
    let appearance = stim.appearance_mut().expect("rect should have appearance");
    assert_eq!(appearance.live.fill_color, default_fill);
}

#[test]
fn test_enable_disable() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(0, proto::CreateRect::default()))
        .handle as u32;

    // Newly created stimulus is enabled and visible.
    assert!(scene.stimuli[&h].is_visible());

    // Disable it.
    let resp = scene.handle_request(set_enabled_req(h, false));
    assert!(is_ok(&resp));
    assert_eq!(resp.handle, -1);
    assert!(!scene.stimuli[&h].flags().enabled);

    // Re-enable it.
    let resp = scene.handle_request(set_enabled_req(h, true));
    assert!(is_ok(&resp));
    assert!(scene.stimuli[&h].flags().enabled);
}

#[test]
fn test_delete() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(0, proto::CreateRect::default()))
        .handle as u32;

    assert!(scene.stimuli.contains_key(&h));

    let resp = scene.handle_request(delete_req(h));
    assert!(is_ok(&resp));
    assert_eq!(resp.handle, -1);
    assert!(!scene.stimuli.contains_key(&h));
}

#[test]
fn test_delete_nonexistent() {
    let mut scene = SceneState::new();
    let resp = scene.handle_request(delete_req(9999));
    assert!(!is_ok(&resp));
    assert_eq!(resp.handle, 0);
    assert!(resp.error.contains("9999"));
}

#[test]
fn test_enable_nonexistent() {
    let mut scene = SceneState::new();
    let resp = scene.handle_request(set_enabled_req(9999, true));
    assert!(!is_ok(&resp));
    assert_eq!(resp.handle, 0);
    assert!(resp.error.contains("9999"));
}

#[test]
fn test_empty_body_returns_error() {
    let mut scene = SceneState::new();
    let resp = scene.handle_request(proto::Request { handle: 0, body: None });
    assert!(!is_ok(&resp));
    assert_eq!(resp.handle, 0);
}

#[test]
fn test_create_rect_wrong_handle() {
    let mut scene = SceneState::new();
    // CreateRect with handle > 0 should return an error.
    let resp = scene.handle_request(create_rect_req(5, proto::CreateRect::default()));
    assert!(!is_ok(&resp));
}

#[test]
fn test_proto_roundtrip() {
    // Verify that Request/Response encode → decode correctly.
    let req = proto::Request {
        handle: 0,
        body: Some(proto::request::Body::CreateRect(proto::CreateRect {
            center: Some(proto::Vec2 { x: 1.0, y: 2.0 }),
            width: 50.0,
            height: 30.0,
            fill: Some(proto::Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
        })),
    };
    let bytes = req.encode_to_vec();
    let decoded = proto::Request::decode(bytes.as_slice()).unwrap();
    assert_eq!(decoded.handle, req.handle);

    if let Some(proto::request::Body::CreateRect(c)) = decoded.body {
        assert_eq!(c.width, 50.0);
        assert_eq!(c.fill.unwrap().r, 1.0);
    } else {
        panic!("unexpected body variant");
    }
}
