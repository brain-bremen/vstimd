/// Integration tests for protobuf command dispatch.
///
/// These tests call `handle_request` directly on a `SceneState` — no ZMQ, no
/// GPU required — so they can run in any environment.
use prost::Message;
use wonderlamp_server::proto;
use wonderlamp_server::proto::request;
use wonderlamp_server::scene::{SceneState, Stimulus};

// ── helpers ───────────────────────────────────────────────────────────────────

fn sys() -> request::Target {
    request::Target::System(proto::SystemTarget {})
}

fn stim(handle: u32) -> request::Target {
    request::Target::Stimulus(handle)
}

fn create_rect_req(target: request::Target, cmd: proto::CreateRect) -> proto::Request {
    proto::Request {
        target: Some(target),
        body: Some(request::Body::CreateRect(cmd)),
    }
}

fn set_enabled_req(handle: u32, enabled: bool) -> proto::Request {
    proto::Request {
        target: Some(stim(handle)),
        body: Some(request::Body::SetEnabled(proto::SetEnabled { enabled })),
    }
}

fn delete_req(handle: u32) -> proto::Request {
    proto::Request {
        target: Some(stim(handle)),
        body: Some(request::Body::Delete(proto::Delete {})),
    }
}

fn set_deferred_mode_req(active: bool, cancel: bool) -> proto::Request {
    proto::Request {
        target: Some(sys()),
        body: Some(request::Body::SetDeferredMode(proto::SetDeferredMode { active, cancel })),
    }
}

fn is_ok(resp: &proto::Response) -> bool {
    resp.code == proto::ErrorCode::Ok as i32
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[test]
fn test_create_rect() {
    let mut scene = SceneState::new();
    let resp = scene.handle_request(create_rect_req(
        sys(),
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
        sys(),
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
    let resp = scene.handle_request(create_rect_req(
        sys(),
        proto::CreateRect { center: None, width: 0.0, height: 0.0, fill: None },
    ));
    assert!(is_ok(&resp));
    let h = resp.handle as u32;
    let stim = scene.stimuli.get_mut(&h).unwrap();

    if let Stimulus::Rect(r) = stim {
        assert_eq!(r.size.live, [50.0, 50.0]);
    } else {
        panic!("expected Rect stimulus");
    }

    let stim = scene.stimuli.get_mut(&h).unwrap();
    let appearance = stim.appearance_mut().expect("rect should have appearance");
    assert_eq!(appearance.live.fill_color, default_fill);
}

#[test]
fn test_enable_disable() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(sys(), proto::CreateRect::default()))
        .handle as u32;

    assert!(scene.stimuli[&h].is_visible());

    let resp = scene.handle_request(set_enabled_req(h, false));
    assert!(is_ok(&resp));
    assert_eq!(resp.handle, -1);
    assert!(!scene.stimuli[&h].flags().enabled);

    let resp = scene.handle_request(set_enabled_req(h, true));
    assert!(is_ok(&resp));
    assert!(scene.stimuli[&h].flags().enabled);
}

#[test]
fn test_delete() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(sys(), proto::CreateRect::default()))
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
    let resp = scene.handle_request(proto::Request { target: Some(sys()), body: None });
    assert!(!is_ok(&resp));
    assert_eq!(resp.handle, 0);
}

#[test]
fn test_create_rect_wrong_handle() {
    let mut scene = SceneState::new();
    // CreateRect with a stimulus target should return an error.
    let resp = scene.handle_request(create_rect_req(stim(5), proto::CreateRect::default()));
    assert!(!is_ok(&resp));
    assert_eq!(resp.code, proto::ErrorCode::WrongTarget as i32);
}

#[test]
fn test_proto_roundtrip() {
    let req = proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateRect(proto::CreateRect {
            center: Some(proto::Vec2 { x: 1.0, y: 2.0 }),
            width: 50.0,
            height: 30.0,
            fill: Some(proto::Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
        })),
    };
    let bytes = req.encode_to_vec();
    let decoded = proto::Request::decode(bytes.as_slice()).unwrap();
    assert_eq!(decoded.target, req.target);

    if let Some(request::Body::CreateRect(c)) = decoded.body {
        assert_eq!(c.width, 50.0);
        assert_eq!(c.fill.unwrap().r, 1.0);
    } else {
        panic!("unexpected body variant");
    }
}

#[test]
fn test_create_ellipse() {
    let mut scene = SceneState::new();
    let resp = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateEllipse(proto::CreateEllipse {
            center: Some(proto::Vec2 { x: 0.0, y: 0.0 }),
            width: 120.0,
            height: 60.0,
            fill: Some(proto::Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 }),
            angle: 45.0,
        })),
    });
    assert!(is_ok(&resp), "unexpected error: {}", resp.error);
    let h = resp.handle as u32;
    assert!(h > 0);
    if let Stimulus::Ellipse(e) = &scene.stimuli[&h] {
        assert_eq!(e.radii.live, [60.0, 30.0]);
        assert_eq!(e.transform.live.angle, 45.0);
    } else {
        panic!("expected Ellipse stimulus");
    }
}

#[test]
fn test_set_position() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(sys(), proto::CreateRect::default()))
        .handle as u32;
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetPosition(proto::SetPosition { x: 42.0, y: -7.0 })),
    });
    assert!(is_ok(&resp));
    assert_eq!(scene.stimuli[&h].get_pos(), [42.0, -7.0]);
}

#[test]
fn test_set_fill_color() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(sys(), proto::CreateRect::default()))
        .handle as u32;
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetFillColor(proto::SetFillColor {
            color: Some(proto::Color { r: 1.0, g: 0.0, b: 0.5, a: 0.8 }),
        })),
    });
    assert!(is_ok(&resp));
    let app = scene.stimuli.get(&h).unwrap().appearance().unwrap();
    assert_eq!(app.live.fill_color, [1.0, 0.0, 0.5, 0.8]);
}

#[test]
fn test_immediate_mode_composes_mutations_and_marks_dirty() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(sys(), proto::CreateRect::default()))
        .handle as u32;
    scene.stimuli.get_mut(&h).unwrap().flags_mut().dirty = false;

    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetPosition(proto::SetPosition { x: 15.0, y: 25.0 })),
    });
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetOrientation(proto::SetOrientation { angle_deg: 30.0 })),
    });
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetFillColor(proto::SetFillColor {
            color: Some(proto::Color { r: 0.1, g: 0.2, b: 0.3, a: 0.4 }),
        })),
    });
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetAlpha(proto::SetAlpha { opacity: 0.9 })),
    });
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetDrawMode(proto::SetDrawMode {
            mode: proto::DrawMode::Stroke as i32,
        })),
    });
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetOutlineColor(proto::SetOutlineColor {
            color: Some(proto::Color { r: 0.8, g: 0.7, b: 0.6, a: 0.5 }),
        })),
    });
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetOutlineWidth(proto::SetOutlineWidth { line_width: 7.0 })),
    });
    assert!(is_ok(&resp));

    let stim = scene.stimuli.get(&h).unwrap();
    let t = stim.transform().unwrap();
    assert_eq!(t.live.pos, [15.0, 25.0]);
    assert_eq!(t.live.angle, 30.0);

    let app = stim.appearance().unwrap();
    assert_eq!(app.live.fill_color, [0.1, 0.2, 0.3, 0.9]);
    assert_eq!(app.live.draw_mode, wonderlamp_server::scene::DrawMode::Stroke);
    assert_eq!(app.live.outline_color, [0.8, 0.7, 0.6, 0.5]);
    assert_eq!(app.live.stroke_width, 7.0);
    assert!(stim.flags().dirty);
}

#[test]
fn test_deferred_mode_stages_composed_mutations_until_flip() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(sys(), proto::CreateRect::default()))
        .handle as u32;

    let stim = scene.stimuli.get_mut(&h).unwrap();
    if let Some(t) = stim.transform_mut() {
        t.live = wonderlamp_server::scene::Transform2D { pos: [1.0, 2.0], angle: 3.0 };
    }
    if let Some(app) = stim.appearance_mut() {
        app.live.fill_color = [0.11, 0.12, 0.13, 0.14];
        app.live.outline_color = [0.21, 0.22, 0.23, 0.24];
        app.live.stroke_width = 2.5;
        app.live.draw_mode = wonderlamp_server::scene::DrawMode::FillAndStroke;
    }
    stim.flags_mut().dirty = false;

    let resp = scene.handle_request(set_deferred_mode_req(true, false));
    assert!(is_ok(&resp));
    assert!(scene.deferred_mode);

    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetPosition(proto::SetPosition { x: 15.0, y: 25.0 })),
    });
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetOrientation(proto::SetOrientation { angle_deg: 30.0 })),
    });
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetFillColor(proto::SetFillColor {
            color: Some(proto::Color { r: 0.1, g: 0.2, b: 0.3, a: 0.4 }),
        })),
    });
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetAlpha(proto::SetAlpha { opacity: 0.9 })),
    });
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetDrawMode(proto::SetDrawMode {
            mode: proto::DrawMode::Stroke as i32,
        })),
    });
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetOutlineColor(proto::SetOutlineColor {
            color: Some(proto::Color { r: 0.8, g: 0.7, b: 0.6, a: 0.5 }),
        })),
    });
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetOutlineWidth(proto::SetOutlineWidth { line_width: 7.0 })),
    });
    assert!(is_ok(&resp));

    let stim = scene.stimuli.get(&h).unwrap();
    let t = stim.transform().unwrap();
    assert_eq!(t.live.pos, [1.0, 2.0]);
    assert_eq!(t.live.angle, 3.0);
    assert_eq!(t.copy.pos, [15.0, 25.0]);
    assert_eq!(t.copy.angle, 30.0);

    let app = stim.appearance().unwrap();
    assert_eq!(app.live.fill_color, [0.11, 0.12, 0.13, 0.14]);
    assert_eq!(app.live.outline_color, [0.21, 0.22, 0.23, 0.24]);
    assert_eq!(app.live.stroke_width, 2.5);
    assert_eq!(app.live.draw_mode, wonderlamp_server::scene::DrawMode::FillAndStroke);
    assert_eq!(app.copy.fill_color, [0.1, 0.2, 0.3, 0.9]);
    assert_eq!(app.copy.outline_color, [0.8, 0.7, 0.6, 0.5]);
    assert_eq!(app.copy.stroke_width, 7.0);
    assert_eq!(app.copy.draw_mode, wonderlamp_server::scene::DrawMode::Stroke);
    assert!(!stim.flags().dirty);

    let resp = scene.handle_request(set_deferred_mode_req(false, false));
    assert!(is_ok(&resp));
    assert!(!scene.deferred_mode);
    assert!(scene.pending_flip);

    scene.apply_flip();
    assert!(!scene.pending_flip);

    let stim = scene.stimuli.get(&h).unwrap();
    let t = stim.transform().unwrap();
    assert_eq!(t.live.pos, [15.0, 25.0]);
    assert_eq!(t.live.angle, 30.0);
    let app = stim.appearance().unwrap();
    assert_eq!(app.live.fill_color, [0.1, 0.2, 0.3, 0.9]);
    assert_eq!(app.live.outline_color, [0.8, 0.7, 0.6, 0.5]);
    assert_eq!(app.live.stroke_width, 7.0);
    assert_eq!(app.live.draw_mode, wonderlamp_server::scene::DrawMode::Stroke);
    assert!(stim.flags().dirty);
}

#[test]
fn test_set_rect_size() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(sys(), proto::CreateRect::default()))
        .handle as u32;
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetRectSize(proto::SetRectSize {
            width: 80.0,
            height: 40.0,
        })),
    });
    assert!(is_ok(&resp));
    if let Stimulus::Rect(r) = &scene.stimuli[&h] {
        assert_eq!(r.size.live, [40.0, 20.0]);
    }
}

#[test]
fn test_set_rect_size_wrong_type() {
    let mut scene = SceneState::new();
    let h = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateCircle(proto::CreateCircle::default())),
    }).handle as u32;
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetRectSize(proto::SetRectSize { width: 50.0, height: 50.0 })),
    });
    assert!(!is_ok(&resp));
    assert_eq!(resp.code, proto::ErrorCode::WrongStimulusType as i32);
}

#[test]
fn test_query_stimulus() {
    let mut scene = SceneState::new();
    let h = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateRect(proto::CreateRect {
            center: Some(proto::Vec2 { x: 5.0, y: 10.0 }),
            width: 200.0,
            height: 100.0,
            fill: Some(proto::Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
        })),
    }).handle as u32;

    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::QueryStimulus(proto::QueryStimulus {})),
    });
    assert!(is_ok(&resp), "query error: {}", resp.error);

    if let Some(proto::response::Body::StimulusInfo(info)) = resp.body {
        assert_eq!(info.stimulus_type, proto::StimulusType::Rect as i32);
        assert!(info.enabled);
        let pos = info.pos.unwrap();
        assert_eq!(pos.x, 5.0);
        assert_eq!(pos.y, 10.0);
        let fc = info.fill_color.unwrap();
        assert_eq!(fc.r, 1.0);
        if let Some(proto::stimulus_params::Shape::Rect(rp)) = info.params.unwrap().shape {
            assert_eq!(rp.width, 200.0);
            assert_eq!(rp.height, 100.0);
        } else {
            panic!("expected Rect params");
        }
    } else {
        panic!("expected StimulusInfo in response body");
    }
}

#[test]
fn test_list_stimuli() {
    let mut scene = SceneState::new();
    let h1 = scene
        .handle_request(create_rect_req(sys(), proto::CreateRect::default()))
        .handle as u32;
    let h2 = scene
        .handle_request(create_rect_req(sys(), proto::CreateRect::default()))
        .handle as u32;

    let resp = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::ListStimuli(proto::ListStimuli {})),
    });
    assert!(is_ok(&resp));

    if let Some(proto::response::Body::StimulusList(list)) = resp.body {
        assert_eq!(list.entries.len(), 2);
        let handles: Vec<u32> = list.entries.iter().map(|e| e.handle).collect();
        assert!(handles.contains(&h1));
        assert!(handles.contains(&h2));
    } else {
        panic!("expected StimulusList in response body");
    }
}

#[test]
fn test_query_server_info() {
    let scene = SceneState::new();
    // SceneState::cmd_query_server_info takes &self so we need a mutable reference for handle_request.
    let mut scene = scene;
    let resp = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::QueryServerInfo(proto::QueryServerInfo {})),
    });
    assert!(is_ok(&resp), "error: {}", resp.error);
    assert!(matches!(resp.body, Some(proto::response::Body::ServerInfo(_))));
}

#[test]
fn test_delete_all() {
    let mut scene = SceneState::new();
    scene.handle_request(create_rect_req(sys(), proto::CreateRect::default()));
    scene.handle_request(create_rect_req(sys(), proto::CreateRect::default()));
    assert_eq!(scene.stimuli.len(), 2);

    let resp = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::DeleteAll(proto::DeleteAll {})),
    });
    assert!(is_ok(&resp));
    assert_eq!(scene.stimuli.len(), 0);
}
