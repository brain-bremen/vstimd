use vstimd::io_config::load_config;
use vstimd::scene::Stimulus;
use vtl::Direction;

#[test]
fn load_v2_reference() {
    let (scene, io) = load_config(std::path::Path::new(
        "tests/configs/vstimd_reference_v2.config.json",
    ))
    .expect("reference v2 config must load without error");

    // Scene structure
    assert_eq!(scene.stimuli.len(), 3, "expected 3 stimuli");
    assert_eq!(scene.background.live, vstimd::Color::new(0.05, 0.05, 0.05, 1.0));

    // Stimulus 1: rect
    let rect_entry = scene.stimuli.values().find(|e| e.name.as_deref() == Some("ref_rect")).expect("ref_rect must exist");
    assert!(matches!(rect_entry.stimulus, Stimulus::Rect(_)));
    if let Stimulus::Rect(ref r) = rect_entry.stimulus {
        assert_eq!(r.common.transform.live.pos, [100.0, -50.0]);
        assert!((r.common.transform.live.angle - 30.0).abs() < 1e-4);
        assert!((r.common.appearance.live.fill_color.r - 1.0).abs() < 1e-6);
        assert!(r.common.flags.enabled);
    }

    // Stimulus 2: circle
    let circle_entry = scene.stimuli.values().find(|e| e.name.as_deref() == Some("ref_circle")).expect("ref_circle must exist");
    assert!(matches!(circle_entry.stimulus, Stimulus::Circle(_)));
    if let Stimulus::Circle(ref c) = circle_entry.stimulus {
        assert_eq!(c.common.transform.live.pos, [-300.0, 200.0]);
        assert!((c.radius.live - 50.0).abs() < 1e-4);
        assert!(!c.common.flags.enabled);
    }

    // Stimulus 3: grating
    let grating_entry = scene.stimuli.values().find(|e| e.name.as_deref() == Some("ref_grating")).expect("ref_grating must exist");
    assert!(matches!(grating_entry.stimulus, Stimulus::Grating(_)));

    // I/O: VTL names
    assert_eq!(io.vtl.names.len(), 2);
    assert_eq!(io.vtl.names[0].name, "stim_onset");
    assert_eq!(io.vtl.names[0].bank, 0);
    assert_eq!(io.vtl.names[0].bit, 0);
    assert_eq!(io.vtl.names[0].direction, Direction::Output);
    assert_eq!(io.vtl.names[1].name, "trial_gate");
    assert_eq!(io.vtl.names[1].direction, Direction::Input);
}

#[test]
fn reject_v1_reference() {
    // The v1 on-disk format (pre-homogenization) is no longer supported: loading
    // it must fail cleanly with a version error rather than silently mis-parsing.
    match load_config(std::path::Path::new(
        "tests/configs/vstimd_reference_v1.config.json",
    )) {
        Ok(_) => panic!("v1 config must be rejected after the v2 format break"),
        Err(e) => assert!(
            e.to_string().contains("config version"),
            "expected a version error, got: {e}",
        ),
    }
}
