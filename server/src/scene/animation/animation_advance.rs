//! The per-frame animation engine: advance one animation by a single frame,
//! applying its stimulus effects and start/final actions.
//!
//! These are free functions (not `SceneState` methods) so the borrow checker can
//! see that `animations` and `stimuli` are disjoint fields being borrowed
//! independently. `SceneState::advance_animations` drives them once per frame.

use super::{AnimState, Animation, CancelAction, FinalAction, StartAction};
use crate::scene::SceneState;
use crate::vtl_state::{VtlEdge, VtlBit, VtlEdges};
use vtl::VtlKind;

/// Pick the edge set (input vs. output) that a trigger's kind addresses.
fn edges_for<'a>(bit: VtlBit, input: &'a VtlEdges, output: &'a VtlEdges) -> &'a VtlEdges {
    match bit.kind {
        VtlKind::Input => input,
        VtlKind::Output => output,
    }
}

fn edge_fired(input: &VtlEdges, output: &VtlEdges, bit: VtlBit, edge: VtlEdge) -> bool {
    let edges = edges_for(bit, input, output);
    let bank = match edge {
        VtlEdge::Rising => edges.rising[bit.bank],
        VtlEdge::Falling => edges.falling[bit.bank],
    };
    (bank >> bit.bit) & 1 != 0
}

/// Advance a single animation by one frame and apply stimulus effects.
///
/// The animation may vanish between steps only if another thread mutated the
/// scene, which cannot happen while the caller holds the write lock — but every
/// re-fetch still handles a missing handle by returning, so a stale handle is a
/// no-op rather than a panic.
pub(crate) fn advance_one(
    handle: u32,
    scene: &mut SceneState,
    input_edges: &VtlEdges,
    output_edges: &VtlEdges,
    output_pending: &mut [u64; vtl::MAX_BANKS],
) {
    // ── 0. Cancel trigger (Armed or Running) ──────────────────────────────────
    // Evaluated before anything else so a pending (Armed) animation can be
    // cancelled before it ever starts, and a Running one aborts this frame.
    {
        let Some(entry) = scene.config.animations.get(&handle) else {
            return;
        };
        let cancellable = matches!(entry.state, AnimState::Armed | AnimState::Running { .. });
        if cancellable
            && let Some((bit, edge)) = entry.cancel_trigger
            && edge_fired(input_edges, output_edges, bit, edge)
        {
            cancel_one(handle, scene, output_pending);
            return;
        }
    }

    // ── 1. Armed → Running ────────────────────────────────────────────────────
    {
        let Some(entry) = scene.config.animations.get(&handle) else {
            return;
        };
        if entry.state == AnimState::Armed {
            let fires = match &entry.start_trigger {
                None => true,
                Some((bit, edge)) => edge_fired(input_edges, output_edges, *bit, *edge),
            };
            if fires {
                // Snapshot user_enabled for RESTORE_STATE before modifying anything.
                // Either a final-action or cancel-action RESTORE_STATE needs the capture.
                let captures_state = entry.final_action.contains(FinalAction::RESTORE_STATE)
                    || entry.cancel_action.contains(CancelAction::RESTORE_STATE);
                let stim_handles: Vec<u32> = entry.config.stimuli.clone();
                let start_action = entry.start_action;
                let start_action_trigger_line = entry.start_action_trigger_line;

                if captures_state {
                    let captured: Vec<bool> = stim_handles
                        .iter()
                        .map(|&sh| {
                            scene
                                .config
                                .stimuli
                                .get(&sh)
                                .is_some_and(|e| e.stimulus.flags().enabled)
                        })
                        .collect();
                    if let Some(entry) = scene.config.animations.get_mut(&handle) {
                        entry.captured_user_enabled = Some(captured);
                    }
                }

                // FlashForNFrames enables stimuli at start; FlickerForNFrames sets initial phase.
                let Some(entry) = scene.config.animations.get(&handle) else {
                    return;
                };
                match &entry.animation {
                    Animation::FlashForNFrames { .. } => {
                        for &sh in &stim_handles {
                            if let Some(e) = scene.config.stimuli.get_mut(&sh) {
                                e.stimulus.flags_mut().enabled = true;
                                e.stimulus.flags_mut().mark_dirty();
                            }
                        }
                    }
                    Animation::FlickerForNFrames { start_on_phase, .. } => {
                        let on = *start_on_phase;
                        for &sh in &stim_handles {
                            if let Some(e) = scene.config.stimuli.get_mut(&sh) {
                                e.stimulus.flags_mut().anim_enabled = on;
                                e.stimulus.flags_mut().mark_dirty();
                            }
                        }
                    }
                    _ => {}
                }

                // Apply start_action bits.
                if start_action.contains(StartAction::ENABLE) {
                    for &sh in &stim_handles {
                        if let Some(e) = scene.config.stimuli.get_mut(&sh) {
                            e.stimulus.flags_mut().enabled = true;
                            e.stimulus.flags_mut().mark_dirty();
                        }
                    }
                }
                if start_action.contains(StartAction::TOGGLE_PHOTODIODE) {
                    scene.photodiode.lit = !scene.photodiode.lit;
                }
                if start_action.contains(StartAction::START_ACTION_TRIGGER_LINE)
                    && let Some(bit) = start_action_trigger_line
                {
                    output_pending[bit.bank] |= 1u64 << bit.bit;
                }

                if let Some(entry) = scene.config.animations.get_mut(&handle) {
                    entry.state = AnimState::Running { frame_counter: 0 };
                }
            }
        }
    }

    // ── 2. Advance Running ────────────────────────────────────────────────────
    let (frame_counter, stim_handles) = {
        let Some(entry) = scene.config.animations.get(&handle) else {
            return;
        };
        match entry.state {
            AnimState::Running { frame_counter } => (frame_counter, entry.config.stimuli.clone()),
            _ => return,
        }
    };

    let done: bool = {
        let Some(entry) = scene.config.animations.get(&handle) else {
            return;
        };
        match &entry.animation {
            Animation::CoupleVisibilityToTriggerLine { trigger, polarity } => {
                let edges = edges_for(*trigger, input_edges, output_edges);
                let level = (edges.current[trigger.bank] >> trigger.bit) & 1 != 0;
                let anim_en = level == *polarity;
                for &sh in &stim_handles {
                    if let Some(e) = scene.config.stimuli.get_mut(&sh)
                        && e.stimulus.flags().anim_enabled != anim_en
                    {
                        e.stimulus.flags_mut().anim_enabled = anim_en;
                        e.stimulus.flags_mut().mark_dirty();
                    }
                }
                false
            }

            Animation::EnableOnTriggerEdge {
                trigger,
                edge,
                enabled,
            } => {
                let fired = edge_fired(input_edges, output_edges, *trigger, *edge);
                if fired {
                    let en = *enabled;
                    for &sh in &stim_handles {
                        if let Some(e) = scene.config.stimuli.get_mut(&sh) {
                            e.stimulus.flags_mut().enabled = en;
                            e.stimulus.flags_mut().mark_dirty();
                        }
                    }
                }
                fired
            }

            Animation::FlashForNFrames { duration_frames } => frame_counter + 1 >= *duration_frames,

            Animation::FlickerForNFrames {
                on_frames,
                off_frames,
                total_frames,
                start_on_phase,
            } => {
                let period = on_frames + off_frames;
                let phase_frame = frame_counter % period;
                let is_on = if *start_on_phase {
                    phase_frame < *on_frames
                } else {
                    phase_frame >= *off_frames
                };
                for &sh in &stim_handles {
                    if let Some(e) = scene.config.stimuli.get_mut(&sh)
                        && e.stimulus.flags().anim_enabled != is_on
                    {
                        e.stimulus.flags_mut().anim_enabled = is_on;
                        e.stimulus.flags_mut().mark_dirty();
                    }
                }
                total_frames.is_some_and(|tf| frame_counter + 1 >= tf)
            }

            Animation::MoveAlongPath2D { coords } => {
                let idx = frame_counter as usize;
                if idx < coords.len() {
                    let [x, y] = coords[idx];
                    for &sh in &stim_handles {
                        if let Some(e) = scene.config.stimuli.get_mut(&sh) {
                            e.stimulus.move_to(false, x, y);
                        }
                    }
                }
                frame_counter + 1 >= coords.len() as u32
            }
            Animation::MoveAlongSegments2D {
                waypoints,
                speed_px_per_sec,
            } => {
                if waypoints.len() < 2 || *speed_px_per_sec <= 0.0 {
                    true
                } else {
                    // Compute cumulative lengths along each segment.
                    let seg_lens: Vec<f32> = waypoints
                        .windows(2)
                        .map(|w| {
                            let dx = w[1][0] - w[0][0];
                            let dy = w[1][1] - w[0][1];
                            (dx * dx + dy * dy).sqrt()
                        })
                        .collect();
                    let total_len: f32 = seg_lens.iter().sum();
                    let total_frames =
                        (total_len / speed_px_per_sec * scene.runtime.frame_rate).ceil() as u32;
                    let total_frames = total_frames.max(1);

                    // How far along the path are we at this frame?
                    let t = frame_counter as f32 / (total_frames - 1).max(1) as f32;
                    let dist = t * total_len;

                    // Walk segments to find the current interpolated position.
                    let mut accum = 0.0f32;
                    let mut pos = waypoints[0];
                    for (i, &seg_len) in seg_lens.iter().enumerate() {
                        if accum + seg_len >= dist || i + 1 == seg_lens.len() {
                            let local_t = if seg_len > 0.0 {
                                (dist - accum) / seg_len
                            } else {
                                0.0
                            };
                            let local_t = local_t.clamp(0.0, 1.0);
                            let a = waypoints[i];
                            let b = waypoints[i + 1];
                            pos = [
                                a[0] + (b[0] - a[0]) * local_t,
                                a[1] + (b[1] - a[1]) * local_t,
                            ];
                            break;
                        }
                        accum += seg_len;
                    }
                    for &sh in &stim_handles {
                        if let Some(e) = scene.config.stimuli.get_mut(&sh) {
                            e.stimulus.move_to(false, pos[0], pos[1]);
                        }
                    }
                    frame_counter + 1 >= total_frames
                }
            }
            // External position is driven by an external process; never self-terminates.
            Animation::ExternalPosition2D { .. } => false,
        }
    };

    // Increment frame counter.
    if let Some(AnimState::Running { frame_counter }) = scene
        .config
        .animations
        .get_mut(&handle)
        .map(|e| &mut e.state)
    {
        *frame_counter += 1;
    }

    // ── 3. Final actions ──────────────────────────────────────────────────────
    if done {
        let (action, trigger_line) = {
            let Some(entry) = scene.config.animations.get(&handle) else {
                return;
            };
            (entry.final_action, entry.final_action_trigger_line)
        };
        finalize(handle, scene, &stim_handles, output_pending, action, trigger_line, true, true);
    }
}

/// Cancel an animation: distinct from disarm. Applies the animation's
/// `cancel_action` (independent of `final_action`) — leaving visibility in a
/// defined state via `RESTORE_STATE` / `DISABLE`, pulsing any cancel trigger
/// line, toggling the photodiode — and always ends in `Done` (`RESTART` is not a
/// cancel action). An empty `cancel_action` is a hard abort that leaves state
/// as-is. Works while `Running` (the `anim_enabled` hold is released) or `Armed`
/// (never started: no hold to release, and `RESTORE_STATE` is a no-op with no
/// capture). Returns false if the handle is unknown.
pub(crate) fn cancel_one(
    handle: u32,
    scene: &mut SceneState,
    output_pending: &mut [u64; vtl::MAX_BANKS],
) -> bool {
    let Some(entry) = scene.config.animations.get(&handle) else {
        return false;
    };
    match entry.state {
        AnimState::Running { .. } | AnimState::Armed => {
            let running = matches!(entry.state, AnimState::Running { .. });
            let stim_handles = entry.config.stimuli.clone();
            let action = entry.cancel_action.as_final_action();
            let trigger_line = entry.cancel_action_trigger_line;
            // Release the anim_enabled hold only if it was actually Running; an
            // Armed animation never grabbed it. RESTART is never honored.
            finalize(
                handle,
                scene,
                &stim_handles,
                output_pending,
                action,
                trigger_line,
                false,
                running,
            );
        }
        // Idle (never armed) or already Done: nothing to tear down.
        _ => {
            if let Some(entry) = scene.config.animations.get_mut(&handle) {
                entry.state = AnimState::Done;
            }
        }
    }
    true
}

/// Shared teardown for both normal completion and cancel. Applies `action`
/// (a [`FinalAction`] bitset — cancel converts its `CancelAction` via
/// `as_final_action`), pulsing `trigger_line` for the trigger-line bit. When
/// `allow_restart` is false, `RESTART` is ignored and the animation lands in
/// `Done`. When `release_anim_hold` is false, the `anim_enabled` reset is
/// skipped (used for Armed cancel, which never grabbed the hold).
#[allow(clippy::too_many_arguments)]
fn finalize(
    handle: u32,
    scene: &mut SceneState,
    stim_handles: &[u32],
    output_pending: &mut [u64; vtl::MAX_BANKS],
    final_action: FinalAction,
    trigger_line: Option<VtlBit>,
    allow_restart: bool,
    release_anim_hold: bool,
) {
    let (captured, restart) = {
        let Some(entry) = scene.config.animations.get(&handle) else {
            return;
        };
        let cap = entry.captured_user_enabled.clone();
        let restart = allow_restart && final_action.contains(FinalAction::RESTART);
        (cap, restart)
    };

    if final_action.contains(FinalAction::RESTORE_STATE) {
        if let Some(caps) = &captured {
            for (&sh, &was_enabled) in stim_handles.iter().zip(caps.iter()) {
                if let Some(e) = scene.config.stimuli.get_mut(&sh) {
                    e.stimulus.flags_mut().enabled = was_enabled;
                    e.stimulus.flags_mut().mark_dirty();
                }
            }
        }
    } else if final_action.contains(FinalAction::DISABLE) {
        for &sh in stim_handles {
            if let Some(e) = scene.config.stimuli.get_mut(&sh) {
                e.stimulus.flags_mut().enabled = false;
                e.stimulus.flags_mut().mark_dirty();
            }
        }
    }

    // Reset anim_enabled for animations that held it during execution.
    {
        let anim_held = release_anim_hold
            && matches!(
                scene.config.animations.get(&handle).map(|e| &e.animation),
                Some(Animation::FlickerForNFrames { .. })
                    | Some(Animation::CoupleVisibilityToTriggerLine { .. })
            );
        if anim_held {
            for &sh in stim_handles {
                if let Some(e) = scene.config.stimuli.get_mut(&sh)
                    && !e.stimulus.flags().anim_enabled
                {
                    e.stimulus.flags_mut().anim_enabled = true;
                    e.stimulus.flags_mut().mark_dirty();
                }
            }
        }
    }

    if final_action.contains(FinalAction::TOGGLE_PHOTODIODE) {
        scene.photodiode.lit = !scene.photodiode.lit;
    }

    if final_action.contains(FinalAction::FINAL_ACTION_TRIGGER_LINE)
        && let Some(bit) = trigger_line
    {
        output_pending[bit.bank] |= 1u64 << bit.bit;
    }

    if final_action.contains(FinalAction::END_DEFERRED) {
        scene.runtime.pending_flip = true;
        scene.runtime.deferred_mode = false;
    }

    if restart {
        if let Some(entry) = scene.config.animations.get_mut(&handle) {
            entry.state = AnimState::Running { frame_counter: 0 };
            entry.captured_user_enabled = None;
        }
    } else if let Some(entry) = scene.config.animations.get_mut(&handle) {
        entry.state = AnimState::Done;
    }
}
