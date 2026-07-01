/// Integration tests for protobuf command dispatch.
///
/// These tests call `handle_request` directly on a `SceneState` — no ZMQ, no
/// GPU required — so they can run in any environment.
use prost::Message;
use vstimd::proto;
use vstimd::proto::request;
use vstimd::scene::{SceneState, Stimulus};
use vstimd::Color;

// ── helpers ───────────────────────────────────────────────────────────────────

fn sys() -> request::Target {
    request::Target::System(proto::SystemTarget {})
}

fn stim(handle: u32) -> request::Target {
    request::Target::Stimulus(handle)
}

fn create_rect_req(target: request::Target, cmd: proto::CreateRectRequest) -> proto::Request {
    proto::Request {
        target: Some(target),
        body: Some(request::Body::CreateRect(cmd)),
    }
}

fn set_enabled_req(handle: u32, enabled: bool) -> proto::Request {
    proto::Request {
        target: Some(stim(handle)),
        body: Some(request::Body::SetEnabled(proto::SetEnabledRequest { enabled })),
    }
}

fn delete_req(handle: u32) -> proto::Request {
    proto::Request {
        target: Some(stim(handle)),
        body: Some(request::Body::Delete(proto::DeleteRequest {})),
    }
}

fn set_deferred_mode_req(active: bool, cancel: bool) -> proto::Request {
    proto::Request {
        target: Some(sys()),
        body: Some(request::Body::SetDeferredMode(proto::SetDeferredModeRequest { active, cancel })),
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
        proto::CreateRectRequest {
            center: Some(proto::Vec2 { x: 10.0, y: -20.0 }),
            width: 200.0,
            height: 100.0,
            fill_color: None,
            ..Default::default()
        },
    ), None);
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
        proto::CreateRectRequest {
            center: None,
            width: 0.0,
            height: 0.0,
            fill_color: Some(fill),
            ..Default::default()
        },
    ), None);
    assert!(is_ok(&resp));
    let h = resp.handle as u32;
    let entry = scene.stimuli.get_mut(&h).unwrap();
    let appearance = entry.stimulus.shape_appearance().expect("expected shape stimulus");
    assert_eq!(appearance.live.fill_color.r, fill.r);
    assert_eq!(appearance.live.fill_color.g, fill.g);
    assert_eq!(appearance.live.fill_color.b, fill.b);
    assert_eq!(appearance.live.fill_color.a, fill.a);
}

#[test]
fn test_create_rect_defaults() {
    let mut scene = SceneState::new();
    let default_fill = scene.default_fill;
    let resp = scene.handle_request(create_rect_req(
        sys(),
        proto::CreateRectRequest { center: None, width: 0.0, height: 0.0, fill_color: None, ..Default::default() },
    ), None);
    assert!(is_ok(&resp));
    let h = resp.handle as u32;
    let entry = scene.stimuli.get_mut(&h).unwrap();

    let Stimulus::Rect(r) = &mut entry.stimulus else { panic!("expected Rect stimulus") };
    assert_eq!(r.size.live, [50.0, 50.0]);
    assert_eq!(r.common.appearance.live.fill_color, default_fill);
}

#[test]
fn test_enable_disable() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(sys(), proto::CreateRectRequest::default()), None)
        .handle as u32;

    assert!(scene.stimuli[&h].stimulus.is_visible());

    let resp = scene.handle_request(set_enabled_req(h, false), None);
    assert!(is_ok(&resp));
    assert_eq!(resp.handle, -1);
    assert!(!scene.stimuli[&h].stimulus.flags().enabled);

    let resp = scene.handle_request(set_enabled_req(h, true), None);
    assert!(is_ok(&resp));
    assert!(scene.stimuli[&h].stimulus.flags().enabled);
}

#[test]
fn test_delete() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(sys(), proto::CreateRectRequest::default()), None)
        .handle as u32;

    assert!(scene.stimuli.contains_key(&h));

    let resp = scene.handle_request(delete_req(h), None);
    assert!(is_ok(&resp));
    assert_eq!(resp.handle, -1);
    assert!(!scene.stimuli.contains_key(&h));
}

#[test]
fn test_delete_nonexistent() {
    let mut scene = SceneState::new();
    let resp = scene.handle_request(delete_req(9999), None);
    assert!(!is_ok(&resp));
    assert_eq!(resp.handle, 0);
    assert!(resp.error.contains("9999"));
}

#[test]
fn test_enable_nonexistent() {
    let mut scene = SceneState::new();
    let resp = scene.handle_request(set_enabled_req(9999, true), None);
    assert!(!is_ok(&resp));
    assert_eq!(resp.handle, 0);
    assert!(resp.error.contains("9999"));
}

#[test]
fn test_empty_body_returns_error() {
    let mut scene = SceneState::new();
    let resp = scene.handle_request(proto::Request { target: Some(sys()), body: None }, None);
    assert!(!is_ok(&resp));
    assert_eq!(resp.handle, 0);
}

#[test]
fn test_create_rect_wrong_handle() {
    let mut scene = SceneState::new();
    // CreateRect with a stimulus target should return an error.
    let resp = scene.handle_request(create_rect_req(stim(5), proto::CreateRectRequest::default()), None);
    assert!(!is_ok(&resp));
    assert_eq!(resp.code, proto::ErrorCode::WrongTarget as i32);
}

#[test]
fn test_proto_roundtrip() {
    let req = proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateRect(proto::CreateRectRequest {
            center: Some(proto::Vec2 { x: 1.0, y: 2.0 }),
            width: 50.0,
            height: 30.0,
            fill_color: Some(proto::Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
            ..Default::default()
        })),
    };
    let bytes = req.encode_to_vec();
    let decoded = proto::Request::decode(bytes.as_slice()).unwrap();
    assert_eq!(decoded.target, req.target);

    if let Some(request::Body::CreateRect(c)) = decoded.body {
        assert_eq!(c.width, 50.0);
        assert_eq!(c.fill_color.unwrap().r, 1.0);
        // id/name are proto string fields; default is empty string
    } else {
        panic!("unexpected body variant");
    }
}

#[test]
fn test_create_ellipse() {
    let mut scene = SceneState::new();
    let resp = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateEllipse(proto::CreateEllipseRequest {
            center: Some(proto::Vec2 { x: 0.0, y: 0.0 }),
            width: 120.0,
            height: 60.0,
            fill_color: Some(proto::Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 }),
            angle: 45.0,
            ..Default::default()
        })),
    }, None);
    assert!(is_ok(&resp), "unexpected error: {}", resp.error);
    let h = resp.handle as u32;
    assert!(h > 0);
    let Stimulus::Ellipse(e) = &scene.stimuli[&h].stimulus else {
        panic!("expected Ellipse stimulus");
    };
    assert_eq!(e.radii.live, [60.0, 30.0]);
    assert_eq!(e.common.transform.live.angle, 45.0);
}

#[test]
fn test_set_position() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(sys(), proto::CreateRectRequest::default()), None)
        .handle as u32;
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetPosition(proto::SetPositionRequest { x: 42.0, y: -7.0 })),
    }, None);
    assert!(is_ok(&resp));
    assert_eq!(scene.stimuli[&h].stimulus.get_pos(), [42.0, -7.0]);
}

#[test]
fn test_set_fill_color() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(sys(), proto::CreateRectRequest::default()), None)
        .handle as u32;
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetFillColor(proto::SetFillColorRequest {
            color: Some(proto::Color { r: 1.0, g: 0.0, b: 0.5, a: 0.8 }),
        })),
    }, None);
    assert!(is_ok(&resp));
    let app = scene.stimuli.get(&h).unwrap().stimulus.shape_appearance().expect("expected shape");
    assert_eq!(app.live.fill_color, Color::new(1.0, 0.0, 0.5, 0.8));
}

#[test]
fn test_immediate_mode_composes_mutations_and_marks_dirty() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(sys(), proto::CreateRectRequest::default()), None)
        .handle as u32;
    scene.stimuli.get_mut(&h).unwrap().stimulus.flags_mut().dirty = false;

    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetPosition(proto::SetPositionRequest { x: 15.0, y: 25.0 })),
    }, None);
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetOrientation(proto::SetOrientationRequest { angle_deg: 30.0 })),
    }, None);
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetFillColor(proto::SetFillColorRequest {
            color: Some(proto::Color { r: 0.1, g: 0.2, b: 0.3, a: 0.4 }),
        })),
    }, None);
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetAlpha(proto::SetAlphaRequest { opacity: 0.9 })),
    }, None);
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetDrawMode(proto::SetDrawModeRequest {
            mode: proto::ShapeDrawMode::Outlined as i32,
        })),
    }, None);
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetOutlineColor(proto::SetOutlineColorRequest {
            color: Some(proto::Color { r: 0.8, g: 0.7, b: 0.6, a: 0.5 }),
        })),
    }, None);
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetOutlineWidth(proto::SetOutlineWidthRequest { line_width: 7.0 })),
    }, None);
    assert!(is_ok(&resp));

    let entry = scene.stimuli.get(&h).unwrap();
    let stim = &entry.stimulus;
    let t = stim.transform();
    assert_eq!(t.live.pos, [15.0, 25.0]);
    assert_eq!(t.live.angle, 30.0);

    let app = stim.shape_appearance().expect("expected shape");
    assert_eq!(app.live.fill_color, Color::new(0.1, 0.2, 0.3, 0.9));
    assert!(app.live.draw_mode == vstimd::scene::DrawMode::Stroke);
    assert_eq!(app.live.outline_color, Color::new(0.8, 0.7, 0.6, 0.5));
    assert_eq!(app.live.stroke_width, 7.0);
    assert!(stim.flags().dirty);
}

#[test]
fn test_deferred_mode_stages_composed_mutations_until_flip() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(sys(), proto::CreateRectRequest::default()), None)
        .handle as u32;

    let stim_obj = &mut scene.stimuli.get_mut(&h).unwrap().stimulus;
    stim_obj.transform_mut().live = vstimd::scene::Transform2D { pos: [1.0, 2.0], angle: 3.0 };
    {
        let app = stim_obj.shape_appearance_mut().expect("expected shape");
        app.live.fill_color = Color::new(0.11, 0.12, 0.13, 0.14);
        app.live.outline_color = Color::new(0.21, 0.22, 0.23, 0.24);
        app.live.stroke_width = 2.5;
        app.live.draw_mode = vstimd::scene::DrawMode::FillAndStroke;
    }
    stim_obj.flags_mut().dirty = false;

    let resp = scene.handle_request(set_deferred_mode_req(true, false), None);
    assert!(is_ok(&resp));
    assert!(scene.runtime.deferred_mode);

    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetPosition(proto::SetPositionRequest { x: 15.0, y: 25.0 })),
    }, None);
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetOrientation(proto::SetOrientationRequest { angle_deg: 30.0 })),
    }, None);
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetFillColor(proto::SetFillColorRequest {
            color: Some(proto::Color { r: 0.1, g: 0.2, b: 0.3, a: 0.4 }),
        })),
    }, None);
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetAlpha(proto::SetAlphaRequest { opacity: 0.9 })),
    }, None);
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetDrawMode(proto::SetDrawModeRequest {
            mode: proto::ShapeDrawMode::Outlined as i32,
        })),
    }, None);
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetOutlineColor(proto::SetOutlineColorRequest {
            color: Some(proto::Color { r: 0.8, g: 0.7, b: 0.6, a: 0.5 }),
        })),
    }, None);
    assert!(is_ok(&resp));
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetOutlineWidth(proto::SetOutlineWidthRequest { line_width: 7.0 })),
    }, None);
    assert!(is_ok(&resp));

    let entry = scene.stimuli.get(&h).unwrap();
    let stim = &entry.stimulus;
    let t = stim.transform();
    assert_eq!(t.live.pos, [1.0, 2.0]);
    assert_eq!(t.live.angle, 3.0);
    assert_eq!(t.copy.pos, [15.0, 25.0]);
    assert_eq!(t.copy.angle, 30.0);

    let app = stim.shape_appearance().expect("expected shape");
    assert_eq!(app.live.fill_color, Color::new(0.11, 0.12, 0.13, 0.14));
    assert_eq!(app.live.outline_color, Color::new(0.21, 0.22, 0.23, 0.24));
    assert_eq!(app.live.stroke_width, 2.5);
    assert!(app.live.draw_mode == vstimd::scene::DrawMode::FillAndStroke);
    assert_eq!(app.copy.fill_color, Color::new(0.1, 0.2, 0.3, 0.9));
    assert_eq!(app.copy.outline_color, Color::new(0.8, 0.7, 0.6, 0.5));
    assert_eq!(app.copy.stroke_width, 7.0);
    assert!(app.copy.draw_mode == vstimd::scene::DrawMode::Stroke);
    assert!(!stim.flags().dirty);

    let resp = scene.handle_request(set_deferred_mode_req(false, false), None);
    assert!(is_ok(&resp));
    assert!(!scene.runtime.deferred_mode);
    assert!(scene.runtime.pending_flip);

    scene.apply_flip();
    assert!(!scene.runtime.pending_flip);

    let entry = scene.stimuli.get(&h).unwrap();
    let stim = &entry.stimulus;
    let t = stim.transform();
    assert_eq!(t.live.pos, [15.0, 25.0]);
    assert_eq!(t.live.angle, 30.0);
    let app = stim.shape_appearance().expect("expected shape");
    assert_eq!(app.live.fill_color, Color::new(0.1, 0.2, 0.3, 0.9));
    assert_eq!(app.live.outline_color, Color::new(0.8, 0.7, 0.6, 0.5));
    assert_eq!(app.live.stroke_width, 7.0);
    assert!(app.live.draw_mode == vstimd::scene::DrawMode::Stroke);
    assert!(stim.flags().dirty);
}

#[test]
fn test_set_rect_size() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(sys(), proto::CreateRectRequest::default()), None)
        .handle as u32;
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetRectSize(proto::SetRectSizeRequest {
            width: 80.0,
            height: 40.0,
        })),
    }, None);
    assert!(is_ok(&resp));
    let Stimulus::Rect(r) = &scene.stimuli[&h].stimulus else { panic!("expected Rect") };
    assert_eq!(r.size.live, [40.0, 20.0]);
}

#[test]
fn test_set_rect_size_wrong_type() {
    let mut scene = SceneState::new();
    let h = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateCircle(proto::CreateCircleRequest::default())),
    }, None).handle as u32;
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetRectSize(proto::SetRectSizeRequest { width: 50.0, height: 50.0 })),
    }, None);
    assert!(!is_ok(&resp));
    assert_eq!(resp.code, proto::ErrorCode::WrongStimulusType as i32);
}

#[test]
fn test_query_stimulus() {
    let mut scene = SceneState::new();
    let h = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateRect(proto::CreateRectRequest {
            center: Some(proto::Vec2 { x: 5.0, y: 10.0 }),
            width: 200.0,
            height: 100.0,
            fill_color: Some(proto::Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
            ..Default::default()
        })),
    }, None).handle as u32;

    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::QueryStimulus(proto::QueryStimulusRequest {})),
    }, None);
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
        .handle_request(create_rect_req(sys(), proto::CreateRectRequest::default()), None)
        .handle as u32;
    let h2 = scene
        .handle_request(create_rect_req(sys(), proto::CreateRectRequest::default()), None)
        .handle as u32;

    let resp = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::ListStimuli(proto::ListStimuliRequest {})),
    }, None);
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
    let mut scene = SceneState::new();
    scene.runtime.screen_size = Some((1920, 1080));
    let resp = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::QueryServerInfo(proto::QueryServerInfoRequest {})),
    }, None);
    assert!(is_ok(&resp), "error: {}", resp.error);
    assert!(matches!(resp.body, Some(proto::response::Body::ServerInfo(_))));
}

#[test]
fn test_delete_all() {
    let mut scene = SceneState::new();
    scene.handle_request(create_rect_req(sys(), proto::CreateRectRequest::default()), None);
    scene.handle_request(create_rect_req(sys(), proto::CreateRectRequest::default()), None);
    assert_eq!(scene.stimuli.len(), 2);

    let resp = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::DeleteAll(proto::DeleteAllRequest {})),
    }, None);
    assert!(is_ok(&resp));
    assert_eq!(scene.stimuli.len(), 0);
}

#[test]
fn test_create_with_name_and_query_returns_name_and_uuid() {
    let mut scene = SceneState::new();
    let resp = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateRect(proto::CreateRectRequest {
            name: "fix_cross".into(),
            ..Default::default()
        })),
    }, None);
    assert!(is_ok(&resp));
    let h = resp.handle as u32;
    assert!(!resp.id.is_empty(), "create response should contain UUID");

    let qresp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::QueryStimulus(proto::QueryStimulusRequest {})),
    }, None);
    assert!(is_ok(&qresp));
    if let Some(proto::response::Body::StimulusInfo(info)) = qresp.body {
        assert_eq!(info.name, "fix_cross");
        assert_eq!(info.id, resp.id);
    } else {
        panic!("expected StimulusInfo");
    }
}

#[test]
fn test_create_with_client_uuid_echoed_back() {
    let mut scene = SceneState::new();
    let client_id = "550e8400-e29b-41d4-a716-446655440000";
    let resp = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateRect(proto::CreateRectRequest {
            id: client_id.into(),
            ..Default::default()
        })),
    }, None);
    assert!(is_ok(&resp));
    assert_eq!(resp.id, client_id);
}

#[test]
fn test_set_name() {
    let mut scene = SceneState::new();
    let h = scene
        .handle_request(create_rect_req(sys(), proto::CreateRectRequest::default()), None)
        .handle as u32;

    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetName(proto::SetNameRequest { name: "new_name".into() })),
    }, None);
    assert!(is_ok(&resp));

    let qresp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::QueryStimulus(proto::QueryStimulusRequest {})),
    }, None);
    assert!(is_ok(&qresp));
    if let Some(proto::response::Body::StimulusInfo(info)) = qresp.body {
        assert_eq!(info.name, "new_name");
    } else {
        panic!("expected StimulusInfo");
    }
}

#[test]
fn test_create_text() {
    let mut scene = SceneState::new();
    let resp = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateText(proto::CreateTextRequest {
            text: "hello".into(),
            font: "Open Sans".into(),
            letter_height: 32.0,
            size: Some(proto::Vec2 { x: 400.0, y: 80.0 }),
            pos: Some(proto::Vec2 { x: 10.0, y: -20.0 }),
            anchor: "center".into(),
            color: Some(proto::Color { r: 1.0, g: 1.0, b: 0.0, a: 1.0 }),
            ..Default::default()
        })),
    }, None);
    assert!(is_ok(&resp), "unexpected error: {}", resp.error);
    let h = resp.handle as u32;
    assert!(h > 0);
    assert!(!resp.id.is_empty());

    let Stimulus::Text(t) = &scene.stimuli[&h].stimulus else {
        panic!("expected Text stimulus");
    };
    assert_eq!(t.text_live, "hello");
    assert_eq!(t.font_family, "Open Sans");
    assert_eq!(t.letter_height_px, 32.0);
    assert_eq!(t.box_size.live, [400.0, 80.0]);
    assert_eq!(t.transform.live.pos, [10.0, -20.0]);
    assert_eq!(t.params.live.color, Color::new(1.0, 1.0, 0.0, 1.0));
    assert_eq!(t.params.live.fill_color.a, 0.0); // transparent by default
}

#[test]
fn test_create_text_defaults() {
    let mut scene = SceneState::new();
    let resp = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateText(proto::CreateTextRequest {
            text: "test".into(),
            ..Default::default()
        })),
    }, None);
    assert!(is_ok(&resp), "unexpected error: {}", resp.error);
    let h = resp.handle as u32;
    let Stimulus::Text(t) = &scene.stimuli[&h].stimulus else {
        panic!("expected Text stimulus");
    };
    assert_eq!(t.box_size.live, [200.0, 100.0]);
    assert_eq!(t.letter_height_px, 32.0);
    assert_eq!(t.params.live.color, Color::WHITE); // white default
}

#[test]
fn test_set_text() {
    let mut scene = SceneState::new();
    let h = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateText(proto::CreateTextRequest {
            text: "before".into(),
            ..Default::default()
        })),
    }, None).handle as u32;

    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetText(proto::SetTextRequest { text: "after".into() })),
    }, None);
    assert!(is_ok(&resp));

    let Stimulus::Text(t) = &scene.stimuli[&h].stimulus else { panic!() };
    assert_eq!(t.text_live, "after");
    assert_eq!(t.text_copy, "after");
    assert!(t.flags.dirty);
}

#[test]
fn test_set_text_color() {
    let mut scene = SceneState::new();
    let h = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateText(proto::CreateTextRequest {
            text: "hi".into(),
            ..Default::default()
        })),
    }, None).handle as u32;

    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetTextColor(proto::SetTextColorRequest {
            color: Some(proto::Color { r: 0.0, g: 1.0, b: 0.5, a: 0.8 }),
        })),
    }, None);
    assert!(is_ok(&resp));

    let Stimulus::Text(t) = &scene.stimuli[&h].stimulus else { panic!() };
    assert_eq!(t.params.live.color, Color::new(0.0, 1.0, 0.5, 0.8));
}

#[test]
fn test_set_text_wrong_type() {
    let mut scene = SceneState::new();
    let h = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateRect(proto::CreateRectRequest::default())),
    }, None).handle as u32;

    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetText(proto::SetTextRequest { text: "oops".into() })),
    }, None);
    assert!(!is_ok(&resp));
    assert_eq!(resp.code, proto::ErrorCode::WrongStimulusType as i32);
}

#[test]
fn test_set_text_color_missing_color() {
    let mut scene = SceneState::new();
    let h = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateText(proto::CreateTextRequest {
            text: "hi".into(), ..Default::default()
        })),
    }, None).handle as u32;

    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetTextColor(proto::SetTextColorRequest { color: None })),
    }, None);
    assert!(!is_ok(&resp));
    assert_eq!(resp.code, proto::ErrorCode::InvalidArgument as i32);
}

#[test]
fn test_query_text_stimulus() {
    let mut scene = SceneState::new();
    let h = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateText(proto::CreateTextRequest {
            text: "hello".into(),
            font: "Cairo".into(),
            letter_height: 24.0,
            size: Some(proto::Vec2 { x: 300.0, y: 60.0 }),
            pos: Some(proto::Vec2 { x: 5.0, y: -10.0 }),
            anchor: "top-left".into(),
            color: Some(proto::Color { r: 0.5, g: 0.5, b: 1.0, a: 1.0 }),
            ..Default::default()
        })),
    }, None).handle as u32;

    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::QueryStimulus(proto::QueryStimulusRequest {})),
    }, None);
    assert!(is_ok(&resp), "query error: {}", resp.error);

    if let Some(proto::response::Body::StimulusInfo(info)) = resp.body {
        assert_eq!(info.stimulus_type, proto::StimulusType::Text as i32);
        assert!(info.enabled);
        let pos = info.pos.unwrap();
        assert_eq!((pos.x, pos.y), (5.0, -10.0));
        if let Some(proto::stimulus_params::Shape::Text(tp)) = info.params.unwrap().shape {
            assert_eq!(tp.text, "hello");
            assert_eq!(tp.font, "Cairo");
            assert_eq!(tp.letter_height, 24.0);
            assert_eq!(tp.anchor, "top-left");
            let size = tp.size.unwrap();
            assert_eq!((size.x, size.y), (300.0, 60.0));
        } else {
            panic!("expected Text params");
        }
    } else {
        panic!("expected StimulusInfo");
    }
}

#[test]
fn test_text_deferred_set_text_and_color() {
    let mut scene = SceneState::new();
    let h = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateText(proto::CreateTextRequest {
            text: "initial".into(),
            ..Default::default()
        })),
    }, None).handle as u32;

    // Enter deferred mode
    scene.handle_request(set_deferred_mode_req(true, false), None);

    scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetText(proto::SetTextRequest { text: "deferred".into() })),
    }, None);
    scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::SetTextColor(proto::SetTextColorRequest {
            color: Some(proto::Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
        })),
    }, None);

    // Live values unchanged before flip
    let Stimulus::Text(t) = &scene.stimuli[&h].stimulus else { panic!() };
    assert_eq!(t.text_live, "initial");
    assert_eq!(t.text_copy, "deferred");
    assert_eq!(t.params.live.color, Color::WHITE);
    assert_eq!(t.params.copy.color, Color::new(1.0, 0.0, 0.0, 1.0));

    // End deferred and flip
    scene.handle_request(set_deferred_mode_req(false, false), None);
    scene.apply_flip();

    let Stimulus::Text(t) = &scene.stimuli[&h].stimulus else { panic!() };
    assert_eq!(t.text_live, "deferred");
    assert_eq!(t.params.live.color, Color::new(1.0, 0.0, 0.0, 1.0));
}

#[test]
fn test_create_text_wrong_target() {
    let mut scene = SceneState::new();
    let h = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateRect(proto::CreateRectRequest::default())),
    }, None).handle as u32;
    // CreateText must use system target
    let resp = scene.handle_request(proto::Request {
        target: Some(stim(h)),
        body: Some(request::Body::CreateText(proto::CreateTextRequest {
            text: "bad".into(), ..Default::default()
        })),
    }, None);
    assert!(!is_ok(&resp));
    assert_eq!(resp.code, proto::ErrorCode::WrongTarget as i32);
}

#[test]
fn test_list_stimuli_includes_id_and_name() {
    let mut scene = SceneState::new();
    let h1 = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateRect(proto::CreateRectRequest {
            name: "rect_a".into(),
            ..Default::default()
        })),
    }, None).handle as u32;
    let h2 = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::CreateCircle(proto::CreateCircleRequest {
            name: "disc_b".into(),
            ..Default::default()
        })),
    }, None).handle as u32;

    let resp = scene.handle_request(proto::Request {
        target: Some(sys()),
        body: Some(request::Body::ListStimuli(proto::ListStimuliRequest {})),
    }, None);
    assert!(is_ok(&resp));
    if let Some(proto::response::Body::StimulusList(list)) = resp.body {
        let by_handle: std::collections::HashMap<u32, &proto::StimulusEntry> =
            list.entries.iter().map(|e| (e.handle, e)).collect();
        assert_eq!(by_handle[&h1].name, "rect_a");
        assert_eq!(by_handle[&h2].name, "disc_b");
        assert!(!by_handle[&h1].id.is_empty());
        assert!(!by_handle[&h2].id.is_empty());
    } else {
        panic!("expected StimulusList");
    }
}
