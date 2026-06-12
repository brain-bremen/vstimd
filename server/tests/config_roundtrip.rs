use vstimd::io_config::{parse_config_json, retrieve_config_json};
use vstimd::scene::{
    Deferred, LoadMode, SceneState, SceneConfig,
    CircleStimulus, RectStimulus, ShapeAppearance, ShapeStimulus, Stimulus, StimulusEntry,
    StimulusFlags, Transform2D,
};
use vstimd::vtl_state::{VtlConfig, VtlNameEntry};
use vtl::Direction;
use uuid::Uuid;

fn make_rect_entry() -> StimulusEntry {
    StimulusEntry::new(
        Uuid::new_v4(),
        Some("test_rect".into()),
        Stimulus::Shape(ShapeStimulus::Rect(RectStimulus {
            flags: StimulusFlags::enabled(true),
            transform: Deferred::new(Transform2D { pos: [100.0, -50.0], angle: 45.0 }),
            appearance: Deferred::new(ShapeAppearance {
                fill_color: vstimd::Color::new(1.0, 0.5, 0.0, 1.0),
                ..Default::default()
            }),
            size: Deferred::new([200.0, 80.0]),
        })),
    )
}

fn make_circle_entry() -> StimulusEntry {
    StimulusEntry::new(
        Uuid::new_v4(),
        Some("test_circle".into()),
        Stimulus::Shape(ShapeStimulus::Circle(CircleStimulus {
            flags: StimulusFlags::enabled(false),
            transform: Deferred::new(Transform2D { pos: [-200.0, 300.0], angle: 0.0 }),
            appearance: Deferred::new(ShapeAppearance {
                fill_color: vstimd::Color::new(0.0, 0.0, 1.0, 1.0),
                ..Default::default()
            }),
            radius: Deferred::new(75.0),
        })),
    )
}

#[test]
fn roundtrip_empty_scene() {
    let scene = SceneConfig::default();
    let vtl = VtlConfig::default();
    let json = retrieve_config_json(&scene, &vtl).unwrap();
    let (loaded, _io) = parse_config_json(&json).unwrap();
    assert_eq!(loaded.stimuli.len(), 0);
    assert_eq!(loaded.animations.len(), 0);
}

#[test]
fn roundtrip_rect_stimulus() {
    let mut scene = SceneState::new();
    scene.add_stimulus(make_rect_entry());

    let vtl = VtlConfig::default();
    let json = retrieve_config_json(&scene.config, &vtl).unwrap();
    let (loaded, _io) = parse_config_json(&json).unwrap();

    assert_eq!(loaded.stimuli.len(), 1);
    let entry = loaded.stimuli.values().next().unwrap();
    assert_eq!(entry.name.as_deref(), Some("test_rect"));
    let Stimulus::Shape(ShapeStimulus::Rect(rect)) = &entry.stimulus else {
        panic!("expected rect");
    };
    assert_eq!(rect.transform.live.pos, [100.0, -50.0]);
    assert!((rect.appearance.live.fill_color.r - 1.0).abs() < 1e-6);
}

#[test]
fn roundtrip_multiple_stimuli() {
    let mut scene = SceneState::new();
    scene.add_stimulus(make_rect_entry());
    scene.add_stimulus(make_circle_entry());

    let vtl = VtlConfig::default();
    let json = retrieve_config_json(&scene.config, &vtl).unwrap();
    let (loaded, _io) = parse_config_json(&json).unwrap();

    assert_eq!(loaded.stimuli.len(), 2);
}

#[test]
fn roundtrip_vtl_names() {
    let scene = SceneConfig::default();
    let vtl = VtlConfig {
        names: vec![
            VtlNameEntry { name: "stim_onset".into(), bank: 0, bit: 0, direction: Direction::Output },
            VtlNameEntry { name: "trial_start".into(), bank: 0, bit: 1, direction: Direction::Input },
        ],
    };
    let json = retrieve_config_json(&scene, &vtl).unwrap();
    let (_loaded, io) = parse_config_json(&json).unwrap();

    assert_eq!(io.vtl.names.len(), 2);
    assert_eq!(io.vtl.names[0].name, "stim_onset");
    assert_eq!(io.vtl.names[1].name, "trial_start");
    assert_eq!(io.vtl.names[0].direction, Direction::Output);
    assert_eq!(io.vtl.names[1].direction, Direction::Input);
}

#[test]
fn roundtrip_background_color() {
    let mut scene = SceneConfig::default();
    scene.background = Deferred::new(vstimd::Color::new(0.2, 0.3, 0.4, 1.0));

    let vtl = VtlConfig::default();
    let json = retrieve_config_json(&scene, &vtl).unwrap();
    let (loaded, _io) = parse_config_json(&json).unwrap();

    assert_eq!(loaded.background.live, vstimd::Color::new(0.2, 0.3, 0.4, 1.0));
}

#[test]
fn roundtrip_additive_load_remaps_handles() {
    let mut scene = SceneState::new();
    let h1 = scene.add_stimulus(make_rect_entry());

    let vtl = VtlConfig::default();
    let json = retrieve_config_json(&scene.config, &vtl).unwrap();
    let (snap, _io) = parse_config_json(&json).unwrap();

    // Load the same snapshot additively
    scene.load_snapshot(snap, LoadMode::Additive);

    // Should now have 2 stimuli with no handle collision
    assert_eq!(scene.stimuli.len(), 2);
    let handles: Vec<u32> = scene.stimuli.keys().copied().collect();
    assert_eq!(handles.iter().cloned().collect::<std::collections::HashSet<_>>().len(), 2);
    assert!(handles.contains(&h1));
}

#[test]
fn roundtrip_replace_load() {
    let mut scene = SceneState::new();
    scene.add_stimulus(make_rect_entry());
    scene.add_stimulus(make_circle_entry());

    // Serialize only 1-stimulus scene
    let mut one_stim = SceneState::new();
    one_stim.add_stimulus(make_rect_entry());
    let vtl = VtlConfig::default();
    let json = retrieve_config_json(&one_stim.config, &vtl).unwrap();
    let (snap, _io) = parse_config_json(&json).unwrap();

    // Replace the 2-stimulus scene with the 1-stimulus snapshot
    scene.load_snapshot(snap, LoadMode::Replace);
    assert_eq!(scene.stimuli.len(), 1);
}

#[test]
fn config_version_mismatch_rejected() {
    let json = r#"{"version":99,"scene":{"background":[0,0,0,1],"default_fill":[1,1,1,1],"default_outline":[0,0,0,1],"photodiode":{"lit":false,"live":[1,1,1,1],"copy":[1,1,1,1],"position":"BottomLeft","size":0.05},"stimuli":{},"next_stim_handle":1,"animations":{},"next_anim_handle":1},"io":{"vtl":{"names":[]}}}"#;
    assert!(parse_config_json(json).is_err());
}
