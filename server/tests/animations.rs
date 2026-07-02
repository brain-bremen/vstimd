use uuid::Uuid;
use vstimd::scene::deferred::Deferred;
/// Integration tests for the animation system.
///
/// Tests use the internal domain model directly — no proto, no ZMQ, no GPU.
/// Stimuli are inserted directly; animations are constructed with `AnimationEntry`
/// and inserted via `SceneState::add_animation`.
///
/// Frame numbering convention used throughout:
///   advance(n)  = the n-th call to advance_animations (0-indexed).
///   frame_counter inside Running starts at 0 and increments at the END of
///   advance_one, so on frame 0 the counter is 0 for the entire body and
///   becomes 1 after the call returns.
use vstimd::scene::{
    SceneState,
    animation::{AnimState, Animation, AnimationEntry, CancelAction, FinalAction, StartAction},
    stimulus::{
        RectStimulus, ShapeAppearance, ShapeCommon, Stimulus, StimulusSceneEntry, StimulusFlags,
        Transform2D,
    },
};
use vstimd::vtl_state::{Edge, VtlBit, VtlEdges};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn no_edges() -> VtlEdges {
    VtlEdges::default()
}

fn no_outputs() -> [u64; vtl::MAX_BANKS] {
    [0; vtl::MAX_BANKS]
}

/// Advance by one frame with no input edges.  Returns output_pending.
fn advance(scene: &mut SceneState) -> [u64; vtl::MAX_BANKS] {
    advance_with(scene, &no_edges())
}

/// Advance by one frame with given input edges (no output edges).  Returns output_pending.
fn advance_with(scene: &mut SceneState, edges: &VtlEdges) -> [u64; vtl::MAX_BANKS] {
    advance_with_edges(scene, edges, &no_edges())
}

/// Advance by one frame with explicit input and output edges.  Returns output_pending.
fn advance_with_edges(
    scene: &mut SceneState,
    input_edges: &VtlEdges,
    output_edges: &VtlEdges,
) -> [u64; vtl::MAX_BANKS] {
    let mut out = no_outputs();
    scene.advance_animations(input_edges, output_edges, &mut out);
    out
}

/// Create a rect stimulus and return its handle.  Starts with `enabled=true`.
fn create_rect(scene: &mut SceneState) -> u32 {
    scene.add_stimulus(StimulusSceneEntry::new(
        Uuid::new_v4(),
        None,
        Stimulus::Rect(RectStimulus {
            common: ShapeCommon {
                flags: StimulusFlags::enabled(true),
                transform: Deferred::new(Transform2D {
                    pos: [0.0, 0.0],
                    angle: 0.0,
                }),
                appearance: Deferred::new(ShapeAppearance::default()),
            },
            size: Deferred::new([50.0, 50.0]),
        }),
    ))
}

/// Enable a stimulus's `enabled` flag directly (bypassing ZMQ).
fn set_enabled(scene: &mut SceneState, stim: u32, val: bool) {
    scene
        .stimuli
        .get_mut(&stim)
        .unwrap()
        .stimulus
        .flags_mut()
        .enabled = val;
}

/// Arm an existing animation (Idle/Done → Armed).
fn arm(scene: &mut SceneState, anim: u32) {
    scene.animations.get_mut(&anim).unwrap().state = AnimState::Armed;
}

/// Cancel an animation through the proto boundary (exercises cmd_cancel_animation).
fn cancel_via_request(scene: &mut SceneState, anim: u32) {
    use vstimd::proto;
    use vstimd::proto::request;
    scene.handle_request(
        proto::Request {
            target: Some(request::Target::System(proto::SystemTarget {})),
            body: Some(request::Body::CancelAnimation(
                proto::CancelAnimationRequest { handle: anim },
            )),
        },
        None,
    );
}

// ── Accessors ─────────────────────────────────────────────────────────────────

fn anim_state(scene: &SceneState, anim: u32) -> &AnimState {
    &scene.animations[&anim].state
}

fn is_enabled(scene: &SceneState, stim: u32) -> bool {
    scene.stimuli[&stim].stimulus.flags().enabled
}

fn is_anim_enabled(scene: &SceneState, stim: u32) -> bool {
    scene.stimuli[&stim].stimulus.flags().anim_enabled
}

fn is_visible(scene: &SceneState, stim: u32) -> bool {
    scene.stimuli[&stim].stimulus.is_visible()
}

// ── VtlEdges constructors ─────────────────────────────────────────────────────

fn rising_edge(bank: usize, bit: u8) -> VtlEdges {
    let mut e = VtlEdges::default();
    e.rising[bank] |= 1u64 << bit;
    e.current[bank] |= 1u64 << bit;
    e
}

fn falling_edge(bank: usize, bit: u8) -> VtlEdges {
    let mut e = VtlEdges::default();
    e.falling[bank] |= 1u64 << bit;
    e
}

fn current_high(bank: usize, bit: u8) -> VtlEdges {
    let mut e = VtlEdges::default();
    e.current[bank] |= 1u64 << bit;
    e
}

fn bit(bank: usize, bit: u8) -> VtlBit {
    VtlBit { bank, bit, direction: vtl::Direction::Input }
}

fn out_bit(bank: usize, bit: u8) -> VtlBit {
    VtlBit { bank, bit, direction: vtl::Direction::Output }
}

// ── FlashForNFrames ───────────────────────────────────────────────────────────

#[test]
fn flash_1_frame_disables_immediately() {
    // duration=1: stimulus is enabled at Armed→Running (frame 0 start),
    // then immediately done (0+1 >= 1) → DISABLE fires.
    // Net result: Done, enabled=false.
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 1 }, vec![s]);
        e.final_action = FinalAction::DISABLE;
        e
    });

    advance(&mut scene);

    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert!(
        !is_enabled(&scene, s),
        "DISABLE fires on frame 0 for duration=1"
    );
}

#[test]
fn flash_3_frames_visible_during_frames_0_1_2_then_disabled() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 3 }, vec![s]);
        e.final_action = FinalAction::DISABLE;
        e
    });

    // Frame 0: enables stim, frame_counter→1, not done (0+1 < 3).
    advance(&mut scene);
    assert!(is_enabled(&scene, s), "frame 0: visible");
    assert_eq!(
        anim_state(&scene, a),
        &AnimState::Running { frame_counter: 1 }
    );

    // Frame 1: frame_counter=1, 1+1=2 < 3.
    advance(&mut scene);
    assert!(is_enabled(&scene, s), "frame 1: visible");
    assert_eq!(
        anim_state(&scene, a),
        &AnimState::Running { frame_counter: 2 }
    );

    // Frame 2: frame_counter=2, 2+1=3 >= 3 → Done → DISABLE.
    advance(&mut scene);
    assert!(!is_enabled(&scene, s), "frame 2: DISABLE fires");
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
}

#[test]
fn flash_not_advanced_while_idle() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, false); // start disabled to observe that idle animation doesn't enable
    let a = scene.add_animation(AnimationEntry::new(
        Animation::FlashForNFrames { duration_frames: 3 },
        vec![s],
    ));
    advance(&mut scene);
    assert!(
        !is_enabled(&scene, s),
        "idle animation should not enable stimulus"
    );
    assert_eq!(anim_state(&scene, a), &AnimState::Idle);
}

#[test]
fn flash_no_final_action_leaves_stim_enabled() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    let a = scene.add_animation(AnimationEntry::armed(
        Animation::FlashForNFrames { duration_frames: 2 },
        vec![s],
    ));

    advance(&mut scene);
    advance(&mut scene);

    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert!(
        is_enabled(&scene, s),
        "no DISABLE: stim stays enabled after Done"
    );
}

#[test]
fn flash_multiple_stimuli() {
    let mut scene = SceneState::new();
    let s1 = create_rect(&mut scene);
    let s2 = create_rect(&mut scene);

    let a = scene.add_animation({
        let mut e = AnimationEntry::armed(
            Animation::FlashForNFrames { duration_frames: 2 },
            vec![s1, s2],
        );
        e.final_action = FinalAction::DISABLE;
        e
    });
    arm(&mut scene, a);

    advance(&mut scene);
    assert!(is_enabled(&scene, s1));
    assert!(is_enabled(&scene, s2));

    advance(&mut scene);
    assert!(!is_enabled(&scene, s1), "s1 disabled");
    assert!(!is_enabled(&scene, s2), "s2 disabled");
}

// ── FlickerForNFrames ─────────────────────────────────────────────────────────

fn flicker(on: u32, off: u32, total: Option<u32>, start_on: bool) -> Animation {
    Animation::FlickerForNFrames {
        on_frames: on,
        off_frames: off,
        total_frames: total,
        start_on_phase: start_on,
    }
}

#[test]
fn flicker_on_off_phase_cycling() {
    // on=2, off=3, infinite, start_on_phase=true.
    // period=5: frames 0,1→on; 2,3,4→off; 5,6→on; 7,8,9→off; ...
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);

    let a = scene.add_animation(AnimationEntry::armed(flicker(2, 3, None, true), vec![s]));

    // Frame 0: Armed→Running sets anim_enabled=true (start_on_phase=true).
    advance(&mut scene);
    assert!(is_anim_enabled(&scene, s), "frame 0: on");
    assert_eq!(
        anim_state(&scene, a),
        &AnimState::Running { frame_counter: 1 }
    );

    // Frame 1: phase_frame=1 < 2 → on
    advance(&mut scene);
    assert!(is_anim_enabled(&scene, s), "frame 1: on");

    // Frame 2: phase_frame=2 >= 2 → off
    advance(&mut scene);
    assert!(!is_anim_enabled(&scene, s), "frame 2: off");

    // Frame 3: phase_frame=3 → off
    advance(&mut scene);
    assert!(!is_anim_enabled(&scene, s), "frame 3: off");

    // Frame 4: phase_frame=4 → off
    advance(&mut scene);
    assert!(!is_anim_enabled(&scene, s), "frame 4: off");

    // Frame 5: phase_frame=0 → on (second period)
    advance(&mut scene);
    assert!(is_anim_enabled(&scene, s), "frame 5: on");

    // Frame 6: phase_frame=1 → on
    advance(&mut scene);
    assert!(is_anim_enabled(&scene, s), "frame 6: on");

    // Frame 7: phase_frame=2 → off
    advance(&mut scene);
    assert!(!is_anim_enabled(&scene, s), "frame 7: off");

    assert_eq!(
        anim_state(&scene, a),
        &AnimState::Running { frame_counter: 8 }
    );
}

#[test]
fn flicker_start_off_phase() {
    // start_on_phase=false: off-phase comes first.
    // on=2, off=3, period=5:
    //   frames 0,1,2 → off (phase_frame 0,1,2 < off_frames=3 → off)
    //   frames 3,4   → on  (phase_frame 3,4 >= 3 → on)
    //   frames 5,6,7 → off again
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);

    let a = scene.add_animation(AnimationEntry::armed(flicker(2, 3, None, false), vec![s]));

    advance(&mut scene); // frame 0: start_on_phase=false → anim_enabled=false
    assert!(
        !is_anim_enabled(&scene, s),
        "frame 0: off (start_on_phase=false)"
    );

    advance(&mut scene); // frame 1: phase_frame=1 < 3 → off
    assert!(!is_anim_enabled(&scene, s), "frame 1: off");

    advance(&mut scene); // frame 2: phase_frame=2 < 3 → off
    assert!(!is_anim_enabled(&scene, s), "frame 2: off");

    advance(&mut scene); // frame 3: phase_frame=3 >= 3 → on
    assert!(is_anim_enabled(&scene, s), "frame 3: on");

    advance(&mut scene); // frame 4: phase_frame=4 >= 3 → on
    assert!(is_anim_enabled(&scene, s), "frame 4: on");

    advance(&mut scene); // frame 5: phase_frame=0 → off again
    assert!(!is_anim_enabled(&scene, s), "frame 5: off (second period)");

    let _ = a; // used
}

#[test]
fn flicker_total_frames_cutoff() {
    // on=2, off=2, total=5 → done after frame 4 (4+1=5 >= 5).
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);

    let a = scene.add_animation(AnimationEntry::armed(flicker(2, 2, Some(5), true), vec![s]));

    for i in 0..4u32 {
        advance(&mut scene);
        assert_eq!(
            anim_state(&scene, a),
            &AnimState::Running {
                frame_counter: i + 1
            },
            "frame {i}: still running"
        );
    }

    // Frame 4: 4+1=5 >= 5 → Done.
    advance(&mut scene);
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
}

#[test]
fn flicker_anim_enabled_reset_on_done() {
    // on=2, off=2, total=3 → done on frame 2 (off-phase).
    // finalize() must reset anim_enabled=true.
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);

    let a = scene.add_animation(AnimationEntry::armed(flicker(2, 2, Some(3), true), vec![s]));

    advance(&mut scene); // frame 0: on
    advance(&mut scene); // frame 1: on
    // frame 2: phase_frame=2 >= 2 → off, done (2+1=3 >= 3)
    advance(&mut scene);

    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert!(
        is_anim_enabled(&scene, s),
        "anim_enabled reset to true after Done"
    );
    assert!(
        is_visible(&scene, s),
        "stimulus visible (no DISABLE, anim_enabled restored)"
    );
}

#[test]
fn flicker_total_frames_1() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);

    let a = scene.add_animation(AnimationEntry::armed(flicker(1, 1, Some(1), true), vec![s]));

    advance(&mut scene);
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert!(is_anim_enabled(&scene, s), "anim_enabled reset after Done");
}

// ── start_trigger: Armed stays Armed until edge fires ────────────────────────

#[test]
fn flash_with_start_trigger_stays_armed_until_edge() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, false); // flash should not enable until trigger fires

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 2 }, vec![s]);
        e.start_trigger = Some((bit(0, 3), Edge::Rising));
        e
    });

    // No edge — stays Armed.
    advance(&mut scene);
    assert_eq!(anim_state(&scene, a), &AnimState::Armed);
    assert!(!is_enabled(&scene, s), "stim not yet enabled");

    advance(&mut scene);
    assert_eq!(anim_state(&scene, a), &AnimState::Armed);

    // Rising edge on bank 0, bit 3 → Armed→Running in same call.
    advance_with(&mut scene, &rising_edge(0, 3));
    assert!(matches!(anim_state(&scene, a), &AnimState::Running { .. }));
    assert!(is_enabled(&scene, s), "stim enabled after trigger fires");
}

#[test]
fn flash_start_trigger_wrong_edge_type_ignored() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 2 }, vec![s]);
        e.start_trigger = Some((bit(0, 0), Edge::Rising));
        e
    });

    // Falling edge — should NOT start (wants Rising).
    advance_with(&mut scene, &falling_edge(0, 0));
    assert_eq!(
        anim_state(&scene, a),
        &AnimState::Armed,
        "falling edge ignored"
    );

    // Rising on wrong bit — should NOT start.
    advance_with(&mut scene, &rising_edge(0, 1));
    assert_eq!(
        anim_state(&scene, a),
        &AnimState::Armed,
        "wrong bit ignored"
    );

    // Correct rising edge — starts.
    advance_with(&mut scene, &rising_edge(0, 0));
    assert!(matches!(anim_state(&scene, a), &AnimState::Running { .. }));
}

#[test]
fn flash_start_trigger_falling_edge() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 2 }, vec![s]);
        e.start_trigger = Some((bit(0, 2), Edge::Falling));
        e
    });

    // Rising edge — should NOT start.
    advance_with(&mut scene, &rising_edge(0, 2));
    assert_eq!(anim_state(&scene, a), &AnimState::Armed);

    // Falling edge — starts.
    advance_with(&mut scene, &falling_edge(0, 2));
    assert!(matches!(anim_state(&scene, a), &AnimState::Running { .. }));
}

#[test]
fn flash_no_start_trigger_fires_immediately() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    let a = scene.add_animation(AnimationEntry::armed(
        Animation::FlashForNFrames { duration_frames: 3 },
        vec![s],
    ));

    advance(&mut scene);
    assert!(matches!(anim_state(&scene, a), &AnimState::Running { .. }));
    assert!(is_enabled(&scene, s));
}

// ── EnableOnTriggerEdge ───────────────────────────────────────────────────────

#[test]
fn enable_on_trigger_edge_rising() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, false); // start disabled; animation enables on edge

    let a = scene.add_animation(AnimationEntry::armed(
        Animation::EnableOnTriggerEdge {
            trigger: bit(0, 5),
            edge: Edge::Rising,
            enabled: true,
        },
        vec![s],
    ));

    // No edge — nothing happens.
    advance(&mut scene);
    assert!(!is_enabled(&scene, s), "no edge yet: still disabled");
    assert_eq!(
        anim_state(&scene, a),
        &AnimState::Running { frame_counter: 1 }
    );

    // Rising edge — enabled + Done.
    advance_with(&mut scene, &rising_edge(0, 5));
    assert!(is_enabled(&scene, s), "enabled after rising edge");
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
}

#[test]
fn enable_on_trigger_edge_disable_on_falling() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);

    let a = scene.add_animation(AnimationEntry::armed(
        Animation::EnableOnTriggerEdge {
            trigger: bit(0, 2),
            edge: Edge::Falling,
            enabled: false,
        },
        vec![s],
    ));

    // Rising edge — ignored (wants Falling).
    advance_with(&mut scene, &rising_edge(0, 2));
    assert!(is_enabled(&scene, s), "rising edge ignored");

    // Falling edge — sets enabled=false + Done.
    advance_with(&mut scene, &falling_edge(0, 2));
    assert!(!is_enabled(&scene, s), "disabled on falling edge");
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
}

#[test]
fn enable_on_trigger_edge_wrong_bank_ignored() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, false); // start disabled; animation enables only on correct bank

    let a = scene.add_animation(AnimationEntry::armed(
        Animation::EnableOnTriggerEdge {
            trigger: bit(0, 0),
            edge: Edge::Rising,
            enabled: true,
        },
        vec![s],
    ));

    // Rising on correct bit but bank 1 — should not trigger.
    let mut edges = VtlEdges::default();
    edges.rising[1] |= 1;
    edges.current[1] |= 1;
    advance_with(&mut scene, &edges);
    assert!(!is_enabled(&scene, s), "different bank ignored");
    assert!(matches!(anim_state(&scene, a), &AnimState::Running { .. }));
}

// ── CoupleVisibilityToTriggerLine ─────────────────────────────────────────────

#[test]
fn couple_visibility_tracks_input_level() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);

    let a = scene.add_animation(AnimationEntry::armed(
        Animation::CoupleVisibilityToTriggerLine {
            trigger: bit(0, 1),
            polarity: true,
        },
        vec![s],
    ));

    // Input LOW → anim_enabled=false.
    advance_with(&mut scene, &no_edges());
    assert!(!is_anim_enabled(&scene, s), "input LOW → invisible");
    assert!(!is_visible(&scene, s));

    // Input HIGH → anim_enabled=true.
    advance_with(&mut scene, &current_high(0, 1));
    assert!(is_anim_enabled(&scene, s), "input HIGH → visible");
    assert!(is_visible(&scene, s));

    // Back to LOW.
    advance_with(&mut scene, &no_edges());
    assert!(!is_anim_enabled(&scene, s));

    // CoupleVisibility never transitions to Done.
    assert!(matches!(anim_state(&scene, a), &AnimState::Running { .. }));
}

#[test]
fn couple_visibility_inverted_polarity() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);

    let _a = scene.add_animation(AnimationEntry::armed(
        Animation::CoupleVisibilityToTriggerLine {
            trigger: bit(0, 0),
            polarity: false,
        },
        vec![s],
    ));

    // Input LOW + polarity=false → visible.
    advance_with(&mut scene, &no_edges());
    assert!(
        is_anim_enabled(&scene, s),
        "LOW with polarity=false → visible"
    );

    // Input HIGH + polarity=false → invisible.
    advance_with(&mut scene, &current_high(0, 0));
    assert!(
        !is_anim_enabled(&scene, s),
        "HIGH with polarity=false → invisible"
    );
}

#[test]
fn couple_visibility_anim_enabled_restored_on_disarm() {
    // cmd_disarm_animation resets anim_enabled=true for CoupleVisibility when Running.
    // This test goes through handle_request so the proto boundary exercises the fix.
    use vstimd::proto;
    use vstimd::proto::request;
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);

    let a = scene.add_animation(AnimationEntry::armed(
        Animation::CoupleVisibilityToTriggerLine {
            trigger: bit(0, 0),
            polarity: true,
        },
        vec![s],
    ));

    advance_with(&mut scene, &no_edges()); // Running, input LOW → anim_enabled=false
    assert!(!is_anim_enabled(&scene, s));

    // Disarm via handle_request (exercises cmd_disarm_animation).
    scene.handle_request(
        proto::Request {
            target: Some(request::Target::System(proto::SystemTarget {})),
            body: Some(request::Body::DisarmAnimation(
                proto::DisarmAnimationRequest { handle: a },
            )),
        },
        None,
    );
    assert_eq!(anim_state(&scene, a), &AnimState::Idle);
    assert!(
        is_anim_enabled(&scene, s),
        "anim_enabled restored to true on disarm"
    );
}

#[test]
fn flicker_anim_enabled_restored_on_disarm() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);

    let a = scene.add_animation(AnimationEntry::armed(flicker(2, 2, None, true), vec![s]));

    advance(&mut scene); // frame 0: on
    advance(&mut scene); // frame 1: on
    advance(&mut scene); // frame 2: off (phase_frame=2 >= 2)
    assert!(!is_anim_enabled(&scene, s), "in off-phase before disarm");

    use vstimd::proto;
    use vstimd::proto::request;
    scene.handle_request(
        proto::Request {
            target: Some(request::Target::System(proto::SystemTarget {})),
            body: Some(request::Body::DisarmAnimation(
                proto::DisarmAnimationRequest { handle: a },
            )),
        },
        None,
    );
    assert_eq!(anim_state(&scene, a), &AnimState::Idle);
    assert!(
        is_anim_enabled(&scene, s),
        "anim_enabled restored on disarm from off-phase"
    );
}

#[test]
fn disarm_while_armed_does_not_touch_anim_enabled() {
    // If the animation was Armed (never ran), anim_enabled was never changed
    // so disarm should not touch it either.
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);

    let a = scene.add_animation(AnimationEntry::armed(flicker(1, 1, None, true), vec![s]));
    assert!(
        is_anim_enabled(&scene, s),
        "before disarm: anim_enabled still true"
    );

    use vstimd::proto;
    use vstimd::proto::request;
    scene.handle_request(
        proto::Request {
            target: Some(request::Target::System(proto::SystemTarget {})),
            body: Some(request::Body::DisarmAnimation(
                proto::DisarmAnimationRequest { handle: a },
            )),
        },
        None,
    );
    assert_eq!(anim_state(&scene, a), &AnimState::Idle);
    assert!(
        is_anim_enabled(&scene, s),
        "anim_enabled unchanged (was never written)"
    );
}

// ── FinalAction::RESTORE_STATE ────────────────────────────────────────────────

#[test]
fn restore_state_restores_user_enabled() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, false); // start disabled; flash enables, RESTORE_STATE restores

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 2 }, vec![s]);
        e.final_action = FinalAction::RESTORE_STATE;
        e
    });

    assert!(!is_enabled(&scene, s));

    advance(&mut scene); // frame 0: captures enabled=false, enables stim
    assert!(is_enabled(&scene, s), "frame 0: stim enabled by flash");

    advance(&mut scene); // frame 1: done, RESTORE_STATE restores enabled=false
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert!(!is_enabled(&scene, s), "RESTORE_STATE restores disabled");
}

#[test]
fn restore_state_captures_at_armed_to_running() {
    // The snapshot is taken when Armed→Running fires (first advance),
    // not at create or arm time.
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true); // enabled=true at create time

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 1 }, vec![s]);
        e.final_action = FinalAction::RESTORE_STATE;
        e
    });

    // Change state AFTER arm but BEFORE first advance — snapshot taken at transition.
    // (Changing to false here; snapshot will capture the value AT the transition call.)
    set_enabled(&mut scene, s, false);

    // captured_user_enabled is None until Armed→Running fires.
    assert!(scene.animations[&a].captured_user_enabled.is_none());

    // Advance: transition fires, captures enabled=false, flash sets enabled=true,
    // then done → RESTORE_STATE restores enabled=false.
    advance(&mut scene);
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert!(
        !is_enabled(&scene, s),
        "RESTORE_STATE restores the value captured at transition"
    );
}

#[test]
fn restore_state_takes_priority_over_disable() {
    // RESTORE_STATE + DISABLE both set — RESTORE_STATE wins.
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 1 }, vec![s]);
        e.final_action = FinalAction::RESTORE_STATE | FinalAction::DISABLE;
        e
    });

    advance(&mut scene);
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    // captured=true → restored to true; DISABLE branch skipped.
    assert!(
        is_enabled(&scene, s),
        "RESTORE_STATE takes priority over DISABLE"
    );
}

// ── FinalAction::RESTART ──────────────────────────────────────────────────────

#[test]
fn restart_loops_animation() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 2 }, vec![s]);
        e.final_action = FinalAction::RESTART;
        e
    });

    advance(&mut scene); // frame 0 → Running{1}
    advance(&mut scene); // frame 1: done → RESTART → Running{0}
    assert_eq!(
        anim_state(&scene, a),
        &AnimState::Running { frame_counter: 0 },
        "RESTART resets frame_counter to 0"
    );
    assert!(is_enabled(&scene, s));

    advance(&mut scene); // second cycle → Running{1}
    advance(&mut scene); // second done → RESTART again
    assert_eq!(
        anim_state(&scene, a),
        &AnimState::Running { frame_counter: 0 }
    );
}

// ── FinalAction::TOGGLE_PHOTODIODE ────────────────────────────────────────────

#[test]
fn toggle_photodiode_fires_on_done() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    let initial = scene.photodiode.lit;

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 1 }, vec![s]);
        e.final_action = FinalAction::TOGGLE_PHOTODIODE;
        e
    });

    advance(&mut scene);
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert_ne!(scene.photodiode.lit, initial, "photodiode toggled");
}

#[test]
fn toggle_photodiode_toggles_each_restart() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    let initial = scene.photodiode.lit;

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 1 }, vec![s]);
        e.final_action = FinalAction::TOGGLE_PHOTODIODE | FinalAction::RESTART;
        e
    });

    advance(&mut scene); // first done: toggle
    assert_ne!(scene.photodiode.lit, initial, "first toggle");

    advance(&mut scene); // second done via RESTART: toggle back
    assert_eq!(scene.photodiode.lit, initial, "second toggle restores");

    let _ = a;
}

// ── FinalAction::FINAL_ACTION_TRIGGER_LINE ────────────────────────────────────

#[test]
fn final_action_trigger_line_sets_output_bit_on_done() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 2 }, vec![s]);
        e.final_action = FinalAction::FINAL_ACTION_TRIGGER_LINE;
        e.final_action_trigger_line = Some(bit(0, 7));
        e
    });

    let out0 = advance(&mut scene); // frame 0: running, no output
    assert_eq!(out0[0] & (1u64 << 7), 0, "frame 0: output bit not set");
    assert_eq!(
        anim_state(&scene, a),
        &AnimState::Running { frame_counter: 1 }
    );

    let out1 = advance(&mut scene); // frame 1: done
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert_ne!(out1[0] & (1u64 << 7), 0, "frame 1: output bit set on done");
}

#[test]
fn final_action_trigger_line_not_set_before_done() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);

    let _a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 3 }, vec![s]);
        e.final_action = FinalAction::FINAL_ACTION_TRIGGER_LINE;
        e.final_action_trigger_line = Some(bit(0, 2));
        e
    });

    for i in 0..2 {
        let out = advance(&mut scene);
        assert_eq!(
            out[0] & (1u64 << 2),
            0,
            "frame {i}: output bit not set before Done"
        );
    }
}

// ── Output ordering: A's output not visible to B in same frame ───────────────

#[test]
fn output_ordering_chained_animations_one_frame_latency() {
    // Animation A: FlashForNFrames(1) with FINAL_ACTION_TRIGGER_LINE on bit 0.
    // Animation B: start_trigger = Rising on bit 0.
    // A completes in frame N, writes output_pending bit 0.
    // B's start_trigger checks input_edges (empty this frame) — B stays Armed.
    // B only starts in frame N+1 when the output is delivered as input.
    let mut scene = SceneState::new();
    let s1 = create_rect(&mut scene);
    let s2 = create_rect(&mut scene);

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 1 }, vec![s1]);
        e.final_action = FinalAction::FINAL_ACTION_TRIGGER_LINE;
        e.final_action_trigger_line = Some(bit(0, 0));
        e
    });
    let b = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 2 }, vec![s2]);
        e.start_trigger = Some((bit(0, 0), Edge::Rising));
        e
    });

    // Frame N: A completes, output bit set; B checks input_edges (no rising edge) → stays Armed.
    let out = advance(&mut scene);
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert_ne!(out[0] & 1, 0, "A sets output bit");
    assert_eq!(
        anim_state(&scene, b),
        &AnimState::Armed,
        "B stays Armed in same frame A completes (one-frame latency)"
    );

    // Frame N+1: simulate caller committing out and daqd delivering a rising edge.
    advance_with(&mut scene, &rising_edge(0, 0));
    assert!(
        matches!(anim_state(&scene, b), &AnimState::Running { .. }),
        "B starts in frame N+1 when edge delivered"
    );
}

// ── FinalAction::END_DEFERRED ─────────────────────────────────────────────────

#[test]
fn end_deferred_sets_pending_flip() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);

    scene.begin_deferred();
    assert!(scene.runtime.deferred_mode);

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 1 }, vec![s]);
        e.final_action = FinalAction::END_DEFERRED;
        e
    });

    advance(&mut scene);
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert!(scene.runtime.pending_flip, "END_DEFERRED sets pending_flip");
    assert!(!scene.runtime.deferred_mode, "deferred_mode cleared");
}

// ── Idle and Done animations are not re-advanced ──────────────────────────────

#[test]
fn idle_animation_not_advanced() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, false); // verify idle animation never fires
    let a = scene.add_animation(AnimationEntry::new(
        Animation::FlashForNFrames { duration_frames: 1 },
        vec![s],
    ));
    for _ in 0..5 {
        advance(&mut scene);
    }
    assert_eq!(anim_state(&scene, a), &AnimState::Idle);
    assert!(
        !is_enabled(&scene, s),
        "idle animation must not enable stimulus"
    );
}

#[test]
fn done_animation_not_re_advanced() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 1 }, vec![s]);
        e.final_action = FinalAction::DISABLE;
        e
    });

    advance(&mut scene); // completes + disables
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert!(!is_enabled(&scene, s));

    set_enabled(&mut scene, s, true); // manually re-enable
    advance(&mut scene); // done animation should not fire again
    assert!(is_enabled(&scene, s), "Done animation does not re-fire");
}

// ── Delete running animation restores anim_enabled ───────────────────────────

#[test]
fn couple_visibility_anim_enabled_restored_on_delete() {
    use vstimd::proto;
    use vstimd::proto::request;
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);

    let a = scene.add_animation(AnimationEntry::armed(
        Animation::CoupleVisibilityToTriggerLine {
            trigger: bit(0, 0),
            polarity: true,
        },
        vec![s],
    ));

    advance_with(&mut scene, &no_edges()); // Running, input LOW → anim_enabled=false
    assert!(!is_anim_enabled(&scene, s));

    scene.handle_request(
        proto::Request {
            target: Some(request::Target::System(proto::SystemTarget {})),
            body: Some(request::Body::DeleteAnimation(
                proto::DeleteAnimationRequest { handle: a },
            )),
        },
        None,
    );
    assert!(!scene.animations.contains_key(&a), "animation removed");
    assert!(
        is_anim_enabled(&scene, s),
        "anim_enabled restored to true on delete"
    );
}

#[test]
fn flicker_anim_enabled_restored_on_delete() {
    use vstimd::proto;
    use vstimd::proto::request;
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);

    let a = scene.add_animation(AnimationEntry::armed(flicker(2, 2, None, true), vec![s]));

    advance(&mut scene); // frame 0: on
    advance(&mut scene); // frame 1: on
    advance(&mut scene); // frame 2: off (phase_frame=2 >= 2)
    assert!(!is_anim_enabled(&scene, s), "in off-phase before delete");

    scene.handle_request(
        proto::Request {
            target: Some(request::Target::System(proto::SystemTarget {})),
            body: Some(request::Body::DeleteAnimation(
                proto::DeleteAnimationRequest { handle: a },
            )),
        },
        None,
    );
    assert!(!scene.animations.contains_key(&a), "animation removed");
    assert!(
        is_anim_enabled(&scene, s),
        "anim_enabled restored on delete from off-phase"
    );
}

// ── Cancel: trigger edge + software command ───────────────────────────────────

#[test]
fn cancel_trigger_runs_cancel_action_restore_state() {
    // Flash(10) with cancel_action=RESTORE_STATE; a cancel edge while Running
    // restores the captured enabled=false and ends Done — well before duration.
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, false);

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 10 }, vec![s]);
        e.cancel_action = CancelAction::RESTORE_STATE;
        e.cancel_trigger = Some((bit(0, 4), Edge::Rising));
        e
    });

    advance(&mut scene); // frame 0: captures false, enables stim, Running
    assert!(is_enabled(&scene, s));
    assert!(matches!(anim_state(&scene, a), &AnimState::Running { .. }));

    // Cancel edge fires → RESTORE_STATE restores enabled=false, Done.
    advance_with(&mut scene, &rising_edge(0, 4));
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert!(!is_enabled(&scene, s), "cancel runs RESTORE_STATE teardown");
}

#[test]
fn cancel_empty_action_is_hard_abort_leaves_state() {
    // No cancel_action: cancel ends Done but leaves visibility untouched.
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, false);

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 10 }, vec![s]);
        e.cancel_trigger = Some((bit(0, 4), Edge::Rising));
        e
    });

    advance(&mut scene); // Running, flash enabled the stim
    assert!(is_enabled(&scene, s));

    advance_with(&mut scene, &rising_edge(0, 4)); // cancel, no action
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert!(
        is_enabled(&scene, s),
        "empty cancel_action is a hard abort: state left as-is"
    );
}

#[test]
fn cancel_action_disable_and_toggle_photodiode() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);
    let pd0 = scene.photodiode.lit;

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 10 }, vec![s]);
        e.cancel_action = CancelAction::DISABLE | CancelAction::TOGGLE_PHOTODIODE;
        e.cancel_trigger = Some((bit(0, 4), Edge::Rising));
        e
    });

    advance(&mut scene); // Running
    advance_with(&mut scene, &rising_edge(0, 4)); // cancel
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert!(!is_enabled(&scene, s), "cancel_action DISABLE turns stimulus off");
    assert_ne!(scene.photodiode.lit, pd0, "cancel_action toggles photodiode");
}

#[test]
fn cancel_trigger_aborts_armed_before_start() {
    // Armed flash with a start_trigger that never fires; a cancel edge stops it
    // before it ever runs — stim never enabled, animation ends Done.
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, false);

    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 5 }, vec![s]);
        e.start_trigger = Some((bit(0, 1), Edge::Rising));
        e.cancel_trigger = Some((bit(0, 2), Edge::Rising));
        e
    });

    advance(&mut scene); // no edge → stays Armed
    assert_eq!(anim_state(&scene, a), &AnimState::Armed);

    advance_with(&mut scene, &rising_edge(0, 2)); // cancel edge while Armed
    assert_eq!(
        anim_state(&scene, a),
        &AnimState::Done,
        "cancelled before start"
    );
    assert!(!is_enabled(&scene, s), "never enabled: flash never started");
}

#[test]
fn cancel_trigger_wrong_edge_ignored() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 5 }, vec![s]);
        e.cancel_trigger = Some((bit(0, 3), Edge::Rising));
        e
    });

    advance(&mut scene); // Running

    // Falling edge on the cancel bit — ignored (wants Rising).
    advance_with(&mut scene, &falling_edge(0, 3));
    assert!(
        matches!(anim_state(&scene, a), &AnimState::Running { .. }),
        "falling edge ignored"
    );

    // Rising on the wrong bit — ignored.
    advance_with(&mut scene, &rising_edge(0, 5));
    assert!(
        matches!(anim_state(&scene, a), &AnimState::Running { .. }),
        "wrong bit ignored"
    );

    // Correct rising edge — cancels.
    advance_with(&mut scene, &rising_edge(0, 3));
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
}

#[test]
fn cancel_action_pulses_cancel_trigger_line() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 10 }, vec![s]);
        e.cancel_action = CancelAction::CANCEL_ACTION_TRIGGER_LINE;
        e.cancel_action_trigger_line = Some(bit(0, 6));
        e.cancel_trigger = Some((bit(0, 0), Edge::Rising));
        e
    });

    advance(&mut scene); // Running
    let out = advance_with(&mut scene, &rising_edge(0, 0)); // cancel
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert_ne!(
        out[0] & (1u64 << 6),
        0,
        "cancel pulses the cancel_action trigger line"
    );
}

#[test]
fn cancel_ignores_final_action_restart() {
    // final_action=RESTART is irrelevant to cancel (which runs cancel_action) —
    // cancel is a terminal stop and lands in Done.
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 10 }, vec![s]);
        e.final_action = FinalAction::RESTART;
        e.cancel_trigger = Some((bit(0, 0), Edge::Rising));
        e
    });

    advance(&mut scene); // Running
    advance_with(&mut scene, &rising_edge(0, 0)); // cancel
    assert_eq!(
        anim_state(&scene, a),
        &AnimState::Done,
        "cancel does not run final_action / RESTART"
    );
}

#[test]
fn cancel_command_releases_anim_enabled_on_running_flicker() {
    // Software CancelAnimation on a Running flicker releases the anim_enabled
    // hold (like disarm) but ends in Done (unlike disarm's Idle).
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);

    let a = scene.add_animation(AnimationEntry::armed(flicker(2, 2, None, true), vec![s]));
    advance(&mut scene); // frame 0: on
    advance(&mut scene); // frame 1: on
    advance(&mut scene); // frame 2: off (phase_frame=2 >= 2)
    assert!(!is_anim_enabled(&scene, s), "off-phase before cancel");

    cancel_via_request(&mut scene, a);
    assert_eq!(
        anim_state(&scene, a),
        &AnimState::Done,
        "cancel ends in Done"
    );
    assert!(is_anim_enabled(&scene, s), "anim_enabled released on cancel");
}

#[test]
fn cancel_command_runs_disable_teardown() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, true);
    let a = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 10 }, vec![s]);
        e.cancel_action = CancelAction::DISABLE;
        e
    });

    advance(&mut scene); // Running, enabled
    assert!(is_enabled(&scene, s));
    cancel_via_request(&mut scene, a);
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
    assert!(!is_enabled(&scene, s), "cancel runs DISABLE teardown");
}

#[test]
fn cancel_command_on_armed_stops_before_start() {
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, false);
    let a = scene.add_animation(AnimationEntry::armed(
        Animation::FlashForNFrames { duration_frames: 5 },
        vec![s],
    ));

    cancel_via_request(&mut scene, a); // cancel while Armed, before any advance
    assert_eq!(anim_state(&scene, a), &AnimState::Done);

    advance(&mut scene); // must not start
    assert!(!is_enabled(&scene, s), "cancelled Armed animation never runs");
    assert_eq!(anim_state(&scene, a), &AnimState::Done);
}

#[test]
fn cancel_command_unknown_handle_errors() {
    use vstimd::proto;
    use vstimd::proto::request;
    let mut scene = SceneState::new();
    let resp = scene.handle_request(
        proto::Request {
            target: Some(request::Target::System(proto::SystemTarget {})),
            body: Some(request::Body::CancelAnimation(
                proto::CancelAnimationRequest { handle: 999 },
            )),
        },
        None,
    );
    assert_eq!(resp.code, proto::ErrorCode::HandleNotFound as i32);
}

// ── Multiple animations on the same stimulus ──────────────────────────────────

#[test]
fn two_animations_same_stimulus_last_write_wins() {
    // a1: FlashForNFrames(3), no final action.
    // a2: FlashForNFrames(1), DISABLE.
    // Both armed. Frame 0: both transition Armed→Running.
    //   a1 enables s; a2 enables s; a2 done → DISABLE fires → s disabled.
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);

    let a1 = scene.add_animation(AnimationEntry::armed(
        Animation::FlashForNFrames { duration_frames: 3 },
        vec![s],
    ));
    let a2 = scene.add_animation({
        let mut e =
            AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 1 }, vec![s]);
        e.final_action = FinalAction::DISABLE;
        e
    });

    advance(&mut scene);
    assert_eq!(anim_state(&scene, a2), &AnimState::Done);
    assert!(!is_enabled(&scene, s), "a2 DISABLE fires in same frame");
    assert!(matches!(
        anim_state(&scene, a1),
        &AnimState::Running { frame_counter: 1 }
    ));
}

// ── Output-edge triggers (intra-server chaining) ──────────────────────────────

fn out_rising_edge(bank: usize, bit: u8) -> VtlEdges {
    let mut e = VtlEdges::default();
    e.rising[bank] |= 1u64 << bit;
    e.current[bank] |= 1u64 << bit;
    e
}

#[test]
fn output_edge_starts_armed_animation() {
    // B is armed with an OUTPUT-directed start_trigger. It ignores input edges
    // and fires only when the matching output edge is presented — the deterministic
    // one-frame handoff a chained animation A would produce.
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);
    set_enabled(&mut scene, s, false);

    let b = scene.add_animation({
        let mut e = AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 5 }, vec![s]);
        e.start_action = StartAction::ENABLE;
        e.start_trigger = Some((out_bit(0, 0), Edge::Rising));
        e
    });

    // An input edge on the same (bank, bit) must NOT start an output-directed trigger.
    advance_with(&mut scene, &rising_edge(0, 0));
    assert!(matches!(anim_state(&scene, b), &AnimState::Armed), "input edge ignored");
    assert!(!is_enabled(&scene, s));

    // The matching output edge starts B.
    advance_with_edges(&mut scene, &no_edges(), &out_rising_edge(0, 0));
    assert!(matches!(anim_state(&scene, b), &AnimState::Running { .. }), "output edge starts B");
    assert!(is_enabled(&scene, s), "start_action ENABLE applied");
}

#[test]
fn output_edge_cancels_running_animation() {
    // A running animation is cancelled by an OUTPUT-directed cancel_trigger.
    let mut scene = SceneState::new();
    let s = create_rect(&mut scene);

    let a = scene.add_animation({
        let mut e = AnimationEntry::armed(Animation::FlashForNFrames { duration_frames: 100 }, vec![s]);
        e.cancel_action = CancelAction::DISABLE;
        e.cancel_trigger = Some((out_bit(1, 3), Edge::Rising));
        e
    });

    advance(&mut scene); // Armed → Running
    assert!(matches!(anim_state(&scene, a), &AnimState::Running { .. }));

    // Input edge on the same (bank, bit) is not the output line — no cancel.
    advance_with(&mut scene, &rising_edge(1, 3));
    assert!(matches!(anim_state(&scene, a), &AnimState::Running { .. }), "input edge ignored");

    // Output edge cancels and runs the DISABLE teardown.
    advance_with_edges(&mut scene, &no_edges(), &out_rising_edge(1, 3));
    assert_eq!(anim_state(&scene, a), &AnimState::Done, "output edge cancels A");
    assert!(!is_enabled(&scene, s), "cancel_action DISABLE applied");
}
