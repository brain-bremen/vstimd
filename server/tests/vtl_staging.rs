/// Tests for VtlState staged output buffer behaviour.
///
/// Key invariant: `staged` is never reset to zero between frames.
/// ZMQ writes via `set_staged_bit`/`set_staged_bank` persist until explicitly
/// cleared.  Animation trigger writes accumulate into staged and also persist.
use vstimd::scene::{
    SceneState,
    animation::{Animation, AnimationEntry, FinalAction, StartAction},
};
use vstimd::vtl_state::{VtlBit, VtlEdges};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn no_edges() -> VtlEdges {
    VtlEdges::default()
}

fn bit(bank: usize, b: u8) -> VtlBit {
    VtlBit { bank, bit: b, kind: vtl::VtlKind::Output }
}

/// Simulate the render loop's copy-advance-writeback pattern.
/// Returns the new staged value after animations have run.
fn advance_staged(
    scene: &mut SceneState,
    staged: &mut [u64; vtl::MAX_BANKS],
) {
    scene.advance_animations(&no_edges(), &VtlEdges::default(), staged);
}

// ── VtlState::set_staged_bit / set_staged_bank ────────────────────────────────

#[cfg(unix)]
mod vtl_state_tests {
    use super::*;
    use vstimd::vtl_state::VtlState;
    use vtl::VtlOwner;

    fn unique_shm_name() -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        std::thread::current().id().hash(&mut h);
        format!("/vtl_staging_test_{}_{:x}", std::process::id(), h.finish())
    }

    fn make_vtl() -> VtlState {
        let owner = VtlOwner::create(&unique_shm_name(), 1, 1).expect("VtlOwner::create");
        VtlState::new(owner)
    }

    #[test]
    fn set_staged_bit_persists_in_shm_immediately() {
        let mut vtl = make_vtl();
        vtl.set_staged_bit(0, 5, true);
        assert_ne!(vtl.owner().output_state(0) & (1u64 << 5), 0,
            "shm reflects the write immediately");
        assert_ne!(vtl.staged[0] & (1u64 << 5), 0,
            "staged reflects the write");
    }

    #[test]
    fn set_staged_bit_clear_removes_bit() {
        let mut vtl = make_vtl();
        vtl.set_staged_bit(0, 3, true);
        vtl.set_staged_bit(0, 3, false);
        assert_eq!(vtl.staged[0] & (1u64 << 3), 0, "bit cleared in staged");
        assert_eq!(vtl.owner().output_state(0) & (1u64 << 3), 0, "bit cleared in shm");
    }

    #[test]
    fn set_staged_bank_writes_full_word() {
        let mut vtl = make_vtl();
        vtl.set_staged_bank(0, 0b1100_0011);
        assert_eq!(vtl.staged[0], 0b1100_0011);
        assert_eq!(vtl.owner().output_state(0), 0b1100_0011);
    }

    #[test]
    fn commit_staged_writes_to_shm() {
        let mut vtl = make_vtl();
        vtl.staged[0] = 0xAB;
        vtl.commit_staged();
        assert_eq!(vtl.owner().output_state(0), 0xAB,
            "commit_staged writes staged to shm");
    }

    #[test]
    fn staged_survives_advance_animations_cycle() {
        let mut vtl = make_vtl();
        let mut scene = SceneState::new();

        vtl.set_staged_bit(0, 7, true);

        // Simulate three render loop iterations: copy out, advance, write back.
        for _ in 0..3 {
            let mut staged = vtl.staged;
            advance_staged(&mut scene, &mut staged);
            vtl.staged = staged;
        }

        assert_ne!(vtl.staged[0] & (1u64 << 7), 0,
            "ZMQ-set bit persists across advance_animations cycles");
    }

    // ── Intra-server animation chaining (full render-loop pipeline) ───────────
    //
    // These drive the exact [A]/[S] sequence the render backends run each frame
    // (commit_staged → poll → output_edges → advance → write staged back), so
    // they exercise the real output-edge path rather than synthetic edges. This
    // is where the deterministic one-frame reaction of animation-to-animation
    // chaining is proven.

    use vstimd::scene::animation::{AnimState, CancelAction};
    use vstimd::vtl_state::VtlEdge;

    /// One render-loop iteration, mirroring null_backend/drm/winit exactly.
    fn frame(vtl: &mut VtlState, scene: &mut SceneState) {
        vtl.commit_staged();
        let input_edges = vtl.poll();
        let output_edges = vtl.output_edges();
        let mut staged = vtl.staged;
        scene.advance_animations(&input_edges, &output_edges, &mut staged);
        vtl.staged = staged;
    }

    fn state(scene: &SceneState, h: u32) -> &AnimState {
        &scene.animations[&h].state
    }

    #[test]
    fn output_edge_chaining_deterministic_one_frame_reaction() {
        // A: 2-frame flash that pulses output bit (0,5) when it completes.
        // B: armed, starts on a rising edge of that OUTPUT line.
        // Expected timeline (the "clean handoff" from vtl_state.rs docs):
        //   frame 0: A Armed→Running; B stays Armed (no output edge yet).
        //   frame 1: A completes, writes bit 5 into staged; B still Armed
        //            (A's mid-pass write is NOT visible as an edge this frame).
        //   frame 2: commit raises bit 5 in shm; output_edges sees the rising
        //            edge; B fires → Running. Exactly one frame after A finished.
        let mut vtl = make_vtl();
        let mut scene = SceneState::new();

        let a = scene.add_animation({
            let mut e = AnimationEntry::armed(
                Animation::FlashForNFrames { duration_frames: 2 },
                vec![],
            );
            e.final_action = FinalAction::FINAL_ACTION_TRIGGER_LINE;
            e.final_action_trigger_line = Some(bit(0, 5));
            e
        });
        let b = scene.add_animation({
            let mut e = AnimationEntry::armed(
                Animation::FlashForNFrames { duration_frames: 3 },
                vec![],
            );
            e.start_trigger = Some((bit(0, 5), VtlEdge::Rising));
            e
        });

        frame(&mut vtl, &mut scene); // frame 0
        assert!(matches!(state(&scene, a), AnimState::Running { .. }), "A running");
        assert_eq!(state(&scene, b), &AnimState::Armed, "B waits");

        frame(&mut vtl, &mut scene); // frame 1: A done, writes bit 5
        assert_eq!(state(&scene, a), &AnimState::Done, "A done on frame 1");
        assert_ne!(vtl.staged[0] & (1 << 5), 0, "A staged output bit 5");
        assert_eq!(state(&scene, b), &AnimState::Armed,
            "B not started same frame A wrote the bit (no zero-frame cascade)");

        frame(&mut vtl, &mut scene); // frame 2: B sees the committed edge
        assert!(matches!(state(&scene, b), AnimState::Running { .. }),
            "B starts exactly one frame after A's output pulse");
    }

    #[test]
    fn output_edge_chaining_is_iteration_order_independent() {
        // Same as above but B is inserted BEFORE A, so it is likely advanced
        // first within a frame. Because B reads the pre-pass output-edge
        // snapshot (not A's in-progress staged write), the result is identical.
        let mut vtl = make_vtl();
        let mut scene = SceneState::new();

        let b = scene.add_animation({
            let mut e = AnimationEntry::armed(
                Animation::FlashForNFrames { duration_frames: 3 },
                vec![],
            );
            e.start_trigger = Some((bit(0, 6), VtlEdge::Rising));
            e
        });
        let a = scene.add_animation({
            let mut e = AnimationEntry::armed(
                Animation::FlashForNFrames { duration_frames: 2 },
                vec![],
            );
            e.final_action = FinalAction::FINAL_ACTION_TRIGGER_LINE;
            e.final_action_trigger_line = Some(bit(0, 6));
            e
        });

        frame(&mut vtl, &mut scene); // 0
        frame(&mut vtl, &mut scene); // 1: A done, bit set; B still armed
        assert_eq!(state(&scene, a), &AnimState::Done);
        assert_eq!(state(&scene, b), &AnimState::Armed,
            "insertion order does not leak A's mid-pass write to B");
        frame(&mut vtl, &mut scene); // 2
        assert!(matches!(state(&scene, b), AnimState::Running { .. }),
            "B still fires one frame later regardless of iteration order");
    }

    #[test]
    fn output_edge_cancels_running_animation() {
        // A completes and pulses output bit (0,7); B is long-running with a
        // cancel_trigger on that OUTPUT edge → B is cancelled one frame later.
        let mut vtl = make_vtl();
        let mut scene = SceneState::new();

        let a = scene.add_animation({
            let mut e = AnimationEntry::armed(
                Animation::FlashForNFrames { duration_frames: 1 },
                vec![],
            );
            e.final_action = FinalAction::FINAL_ACTION_TRIGGER_LINE;
            e.final_action_trigger_line = Some(bit(0, 7));
            e
        });
        let b = scene.add_animation({
            let mut e = AnimationEntry::armed(
                Animation::FlashForNFrames { duration_frames: 1000 },
                vec![],
            );
            e.cancel_trigger = Some((bit(0, 7), VtlEdge::Rising));
            e.cancel_action = CancelAction::DISABLE;
            e
        });

        frame(&mut vtl, &mut scene); // 0: A done (dur 1), writes bit 7; B running
        assert_eq!(state(&scene, a), &AnimState::Done);
        assert!(matches!(state(&scene, b), AnimState::Running { .. }), "B running");
        assert_ne!(vtl.staged[0] & (1 << 7), 0, "A staged output bit 7");

        frame(&mut vtl, &mut scene); // 1: B sees the output edge → cancelled
        assert_eq!(state(&scene, b), &AnimState::Done, "B cancelled by A's output edge");
    }

    #[test]
    fn output_edge_fan_out_starts_multiple_animations() {
        // One output edge (bit 0,8) starts two armed animations at once.
        let mut vtl = make_vtl();
        let mut scene = SceneState::new();

        let a = scene.add_animation({
            let mut e = AnimationEntry::armed(
                Animation::FlashForNFrames { duration_frames: 1 },
                vec![],
            );
            e.final_action = FinalAction::FINAL_ACTION_TRIGGER_LINE;
            e.final_action_trigger_line = Some(bit(0, 8));
            e
        });
        let mk_follower = |scene: &mut SceneState| {
            scene.add_animation({
                let mut e = AnimationEntry::armed(
                    Animation::FlashForNFrames { duration_frames: 3 },
                    vec![],
                );
                e.start_trigger = Some((bit(0, 8), VtlEdge::Rising));
                e
            })
        };
        let b = mk_follower(&mut scene);
        let c = mk_follower(&mut scene);

        frame(&mut vtl, &mut scene); // 0: A done, writes bit 8
        assert_eq!(state(&scene, a), &AnimState::Done);
        frame(&mut vtl, &mut scene); // 1: both B and C see the edge
        assert!(matches!(state(&scene, b), AnimState::Running { .. }), "B started");
        assert!(matches!(state(&scene, c), AnimState::Running { .. }), "C started");
    }
}

// ── Animation trigger lines preserve staged state ────────────────────────────

#[test]
fn start_trigger_line_bit_persists_in_staged() {
    let mut scene = SceneState::new();
    let mut staged = [0u64; vtl::MAX_BANKS];

    let _a = scene.add_animation({
        let mut e = AnimationEntry::armed(
            Animation::FlashForNFrames { duration_frames: 3 },
            vec![],
        );
        e.start_action = StartAction::START_ACTION_TRIGGER_LINE;
        e.start_action_trigger_line = Some(bit(0, 4));
        e
    });

    // Frame 0: Armed → Running, start trigger fires → bit set in staged.
    advance_staged(&mut scene, &mut staged);
    assert_ne!(staged[0] & (1u64 << 4), 0, "start trigger bit set on frame 0");

    // Frame 1: still running — bit must NOT be zeroed (no reset).
    advance_staged(&mut scene, &mut staged);
    assert_ne!(staged[0] & (1u64 << 4), 0, "bit persists on frame 1 (no reset)");

    // Frame 2: done — bit still present until explicitly cleared.
    advance_staged(&mut scene, &mut staged);
    assert_ne!(staged[0] & (1u64 << 4), 0, "bit persists after animation done");
}

#[test]
fn final_trigger_line_bit_persists_in_staged() {
    let mut scene = SceneState::new();
    let mut staged = [0u64; vtl::MAX_BANKS];

    let _a = scene.add_animation({
        let mut e = AnimationEntry::armed(
            Animation::FlashForNFrames { duration_frames: 1 },
            vec![],
        );
        e.final_action = FinalAction::FINAL_ACTION_TRIGGER_LINE;
        e.final_action_trigger_line = Some(bit(0, 2));
        e
    });

    // Frame 0: done on first advance, final trigger fires.
    advance_staged(&mut scene, &mut staged);
    assert_ne!(staged[0] & (1u64 << 2), 0, "final trigger bit set when done");

    // Frame 1: animation is gone but bit must persist in staged.
    advance_staged(&mut scene, &mut staged);
    assert_ne!(staged[0] & (1u64 << 2), 0, "final trigger bit persists after animation removed");
}

#[test]
fn staged_bit_from_earlier_frame_not_overwritten_by_later_frame_with_no_animation() {
    let mut scene = SceneState::new();
    let mut staged = [0u64; vtl::MAX_BANKS];

    // Manually set a bit (simulating a ZMQ override).
    staged[0] |= 1u64 << 10;

    // Run several frames with no animations active.
    for _ in 0..5 {
        advance_staged(&mut scene, &mut staged);
    }

    assert_ne!(staged[0] & (1u64 << 10), 0,
        "manually-set bit survives N frames with no animations");
}

#[test]
fn cascade_prevention_unaffected_by_persistent_staged() {
    // Verify same-frame cascade prevention: animation A's in-progress output
    // write is NOT visible to animation B's output-directed start_trigger within
    // the same pass. Output edges are computed *before* the animation pass (from
    // the committed staged of the previous frame), so B reacts one frame later —
    // not from A's mid-pass write. Here output_edges is empty, so B stays Armed.
    use vstimd::vtl_state::VtlEdge;
    use vstimd::scene::animation::AnimState;

    let mut scene = SceneState::new();
    let mut staged = [0u64; vtl::MAX_BANKS];

    let a = scene.add_animation({
        let mut e = AnimationEntry::armed(
            Animation::FlashForNFrames { duration_frames: 1 },
            vec![],
        );
        e.final_action = FinalAction::FINAL_ACTION_TRIGGER_LINE;
        e.final_action_trigger_line = Some(bit(0, 0));
        e
    });
    let b = scene.add_animation({
        let mut e = AnimationEntry::armed(
            Animation::FlashForNFrames { duration_frames: 1 },
            vec![],
        );
        // B starts on a rising edge of output bit 0. It reads the pre-pass output
        // edges, not A's mid-pass write into staged.
        e.start_trigger = Some((bit(0, 0), VtlEdge::Rising));
        e
    });

    // Frame 0: A completes and writes bit 0 into staged.
    // B's start_trigger sees no output edge this pass — B must stay Armed.
    scene.advance_animations(&no_edges(), &VtlEdges::default(), &mut staged);

    assert_eq!(scene.animations[&a].state, AnimState::Done, "A done");
    assert_ne!(staged[0] & 1, 0, "A wrote bit 0 into staged");
    assert_eq!(scene.animations[&b].state, AnimState::Armed,
        "B stays Armed — A's mid-pass write is not visible as an output edge this frame");
}
