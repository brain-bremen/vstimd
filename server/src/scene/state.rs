use indexmap::IndexMap;

use super::animation::{AnimState, Animation, AnimationEntry, FinalAction};
use super::deferred::Deferred;
use super::photodiode::PhotoDiodeState;
use super::stimulus::StimulusEntry;
use crate::vtl_state::{Edge, VtlEdges};
extern crate vtl;

// ── Command log (overlay feature only) ────────────────────────────────────────

/// One recorded ZMQ command, held in a capped ring buffer inside `SceneState`.
/// Written by the ZMQ thread (under the existing write lock) and read by
/// reference from the render thread (under the read lock) — no extra locking.
pub struct CommandEntry {
    /// Milliseconds since server start.
    pub elapsed_ms: f64,
    pub handle: u32,
    /// Short human-readable command name + key params, e.g. "CreateRect 100×50".
    pub summary: String,
    pub ok: bool,
    pub response: i32,
}

/// All shared scene state. Wrapped in `Arc<RwLock<SceneState>>` and shared
/// between the render thread (read lock) and the ZMQ server thread (write lock).
///
/// # Thread-safety contract
///
/// `SceneState` itself does not contain any synchronisation primitives; all
/// locking is done by the caller via the outer `RwLock`.  Two threads access
/// the state concurrently:
///
/// | Thread | Lock | Duration |
/// |---|---|---|
/// | **ZMQ server** (`ipc.rs`) | **write** | One decoded request at a time |
/// | **Render** (`render/state.rs`) | **write** then **read** | One frame at a time |
///
/// The ZMQ thread holds the write lock only while dispatching a single
/// `handle_request()` call, so it releases it before the next ZMQ recv.
/// The render thread takes a write lock briefly in `update()` for
/// `apply_flip()` and scene bookkeeping, then drops it before drawing.
pub struct SceneState {
    /// Stimulus objects in insertion order (insertion order = draw order).
    pub stimuli: IndexMap<u32, StimulusEntry>,
    /// Next handle to allocate for a new stimulus (starts at 1).
    pub next_stim_handle: u32,
    /// Animation objects in insertion order.
    pub animations: IndexMap<u32, AnimationEntry>,
    /// Next handle to allocate for a new animation (starts at 1).
    pub next_anim_handle: u32,
    /// Background clear colour with deferred-copy support.
    pub background: Deferred<[f32; 4]>,
    /// True while commands should write into copy fields instead of live fields.
    pub deferred_mode: bool,
    /// Set by `DeferredMode{start:false}`; cleared by the render thread after flip.
    pub pending_flip: bool,
    pub photodiode: PhotoDiodeState,
    pub default_fill: [f32; 4],
    pub default_outline: [f32; 4],
    /// Measured frame rate, updated by the render thread each frame.
    pub frame_rate: f32,
    /// Set by the render thread on each frame. `None` until the first frame completes.
    pub screen_size: Option<(u32, u32)>,
    /// Screen size at which meshes were last tessellated. When this changes all
    /// stimuli are re-uploaded (NDC coordinates depend on screen dimensions).
    pub last_uploaded_size: (u32, u32),
    pub error_mask: u16,
    pub error_code: i16,

    /// Command ring buffer — written by ZMQ thread, read by overlay.
    /// Gated behind the overlay feature so production builds carry no overhead.
    pub command_log: std::collections::VecDeque<CommandEntry>,
    pub command_log_total: u64,
    pub command_log_errors: u64,
    pub server_start: std::time::Instant,
}

impl SceneState {
    pub fn new() -> Self {
        Self {
            stimuli: IndexMap::new(),
            next_stim_handle: 1,
            animations: IndexMap::new(),
            next_anim_handle: 1,
            background: Deferred::new([0.0, 0.0, 0.0, 1.0]),
            deferred_mode: false,
            pending_flip: false,
            photodiode: PhotoDiodeState::default(),
            default_fill: [1.0, 1.0, 1.0, 1.0],
            default_outline: [0.0, 0.0, 0.0, 1.0],
            frame_rate: 60.0,
            screen_size: None,
            last_uploaded_size: (0, 0),
            error_mask: 0,
            error_code: 0,
            command_log: std::collections::VecDeque::new(),
            command_log_total: 0,
            command_log_errors: 0,
            server_start: std::time::Instant::now(),
        }
    }

    // ── Handle allocation ─────────────────────────────────────────────────────

    pub fn alloc_stim_handle(&mut self) -> u32 {
        let h = self.next_stim_handle;
        self.next_stim_handle += 1;
        h
    }

    pub fn alloc_anim_handle(&mut self) -> u32 {
        let h = self.next_anim_handle;
        self.next_anim_handle += 1;
        h
    }

    /// Insert a `StimulusEntry` and return the allocated handle.
    /// The internal insertion path used by both `cmd_create_*` and tests.
    pub fn add_stimulus(&mut self, entry: super::stimulus::StimulusEntry) -> u32 {
        let h = self.alloc_stim_handle();
        self.stimuli.insert(h, entry);
        h
    }

    /// Insert an `AnimationEntry` and return the allocated handle.
    /// The internal insertion path used by both `cmd_create_animation` and tests.
    pub fn add_animation(&mut self, entry: AnimationEntry) -> u32 {
        let h = self.alloc_anim_handle();
        self.animations.insert(h, entry);
        h
    }

    /// Advance all animations by one frame.  Called once per frame by the render
    /// thread at [S] (after output commit and input poll).
    ///
    /// `input_edges`     — rising/falling/current input lines from `VtlState::poll()`
    /// `output_snapshot` — frozen output_state read at [S] for trigger detection
    /// `output_pending`  — accumulator for this frame's output changes (committed at [A] next frame)
    pub fn advance_animations(
        &mut self,
        input_edges: &crate::vtl_state::VtlEdges,
        output_snapshot: &[u64; vtl::MAX_BANKS],
        output_pending: &mut [u64; vtl::MAX_BANKS],
    ) {
        let handles: Vec<u32> = self.animations.keys().copied().collect();
        for handle in handles {
            advance_one(handle, self, input_edges, output_snapshot, output_pending);
        }
    }

    // ── Deferred mode ─────────────────────────────────────────────────────────

    /// Start deferred mode: snapshot all live state into copy fields.
    pub fn begin_deferred(&mut self) {
        for entry in self.stimuli.values_mut() {
            entry.stimulus.make_copy();
        }
        self.background.make_copy();
        self.photodiode.make_copy();
        self.deferred_mode = true;
    }

    /// End deferred mode: schedule an atomic flip on the next frame boundary.
    pub fn end_deferred(&mut self) {
        self.pending_flip = true;
        self.deferred_mode = false;
    }

    /// Promote all copy fields to live. Called by the render thread when
    /// `pending_flip` is set, before animation advance and tessellation.
    pub fn apply_flip(&mut self) {
        for entry in self.stimuli.values_mut() {
            entry.stimulus.flip();
        }
        self.background.flip();
        self.photodiode.flip();
        self.pending_flip = false;
    }

    // ── Scene commands ────────────────────────────────────────────────────────

    pub fn clear_all(&mut self, protected_too: bool) {
        if protected_too {
            self.stimuli.clear();
        } else {
            self.stimuli.retain(|_, e| e.stimulus.flags().protected);
        }
    }

    pub fn set_all_enabled(&mut self, enabled: bool, protected_too: bool) {
        for entry in self.stimuli.values_mut() {
            if protected_too || !entry.stimulus.flags().protected {
                entry.stimulus.flags_mut().enabled = enabled;
            }
        }
    }

    /// Record a completed command in the ring buffer.
    /// Called from `handle_request` while the write lock is already held —
    /// no extra synchronisation needed.
    pub fn push_command_log(
        &mut self,
        handle: u32,
        summary: String,
        response: &crate::proto::Response,
    ) {
        const MAX_LOG: usize = 200;
        let ok = response.code == 0;
        if !ok {
            self.command_log_errors += 1;
        }
        self.command_log_total += 1;
        self.command_log.push_back(CommandEntry {
            elapsed_ms: self.server_start.elapsed().as_secs_f64() * 1000.0,
            handle,
            summary,
            ok,
            response: response.handle,
        });
        if self.command_log.len() > MAX_LOG {
            self.command_log.pop_front();
        }
    }
}

impl Default for SceneState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Per-animation advance (free function to work around borrow-checker) ───────

fn edge_fired(edges: &VtlEdges, bit: crate::vtl_state::VtlBit, edge: Edge) -> bool {
    let bank = match edge {
        Edge::Rising  => edges.rising[bit.bank],
        Edge::Falling => edges.falling[bit.bank],
    };
    (bank >> bit.bit) & 1 != 0
}

/// Advance a single animation by one frame and apply stimulus effects.
/// Takes `scene: &mut SceneState` explicitly so the compiler can see that
/// `animations` and `stimuli` are disjoint fields being borrowed independently.
fn advance_one(
    handle: u32,
    scene: &mut SceneState,
    input_edges: &VtlEdges,
    _output_snapshot: &[u64; vtl::MAX_BANKS],
    output_pending: &mut [u64; vtl::MAX_BANKS],
) {
    // ── 1. Armed → Running ────────────────────────────────────────────────────
    {
        let entry = match scene.animations.get(&handle) {
            Some(e) => e,
            None => return,
        };
        if entry.state == AnimState::Armed {
            let fires = match &entry.start_trigger {
                None                       => true,
                Some((bit, edge))          => edge_fired(input_edges, *bit, *edge),
            };
            if fires {
                // Snapshot user_enabled for RESTORE_STATE before modifying anything.
                let captures_state = entry.final_action.contains(FinalAction::RESTORE_STATE);
                let stim_handles: Vec<u32> = entry.stimuli.clone();

                if captures_state {
                    let captured: Vec<bool> = stim_handles.iter()
                        .map(|&sh| scene.stimuli.get(&sh).is_some_and(|e| e.stimulus.flags().enabled))
                        .collect();
                    scene.animations.get_mut(&handle).unwrap().captured_user_enabled = Some(captured);
                }

                // FlashForNFrames enables stimuli at start; FlickerForNFrames sets initial phase.
                let entry = scene.animations.get(&handle).unwrap();
                match &entry.animation {
                    Animation::FlashForNFrames { .. } => {
                        for &sh in &stim_handles {
                            if let Some(e) = scene.stimuli.get_mut(&sh) {
                                e.stimulus.flags_mut().enabled = true;
                                e.stimulus.flags_mut().mark_dirty();
                            }
                        }
                    }
                    Animation::FlickerForNFrames { start_on_phase, .. } => {
                        let on = *start_on_phase;
                        for &sh in &stim_handles {
                            if let Some(e) = scene.stimuli.get_mut(&sh) {
                                e.stimulus.flags_mut().anim_enabled = on;
                                e.stimulus.flags_mut().mark_dirty();
                            }
                        }
                    }
                    _ => {}
                }
                scene.animations.get_mut(&handle).unwrap().state =
                    AnimState::Running { frame_counter: 0 };
            }
        }
    }

    // ── 2. Advance Running ────────────────────────────────────────────────────
    let (frame_counter, stim_handles) = {
        let entry = match scene.animations.get(&handle) {
            Some(e) => e,
            None => return,
        };
        match entry.state {
            AnimState::Running { frame_counter } => (frame_counter, entry.stimuli.clone()),
            _ => return,
        }
    };

    let done: bool = {
        let entry = scene.animations.get(&handle).unwrap();
        match &entry.animation {
            Animation::CoupleVisibilityToInputTriggerLine { trigger, polarity } => {
                let level = (input_edges.current[trigger.bank] >> trigger.bit) & 1 != 0;
                let anim_en = level == *polarity;
                for &sh in &stim_handles {
                    if let Some(e) = scene.stimuli.get_mut(&sh)
                        && e.stimulus.flags().anim_enabled != anim_en {
                        e.stimulus.flags_mut().anim_enabled = anim_en;
                        e.stimulus.flags_mut().mark_dirty();
                    }
                }
                false
            }

            Animation::EnableOnTriggerEdge { trigger, edge, enabled } => {
                let fired = edge_fired(input_edges, *trigger, *edge);
                if fired {
                    let en = *enabled;
                    for &sh in &stim_handles {
                        if let Some(e) = scene.stimuli.get_mut(&sh) {
                            e.stimulus.flags_mut().enabled = en;
                            e.stimulus.flags_mut().mark_dirty();
                        }
                    }
                }
                fired
            }


            Animation::FlashForNFrames { duration_frames } => {
                frame_counter + 1 >= *duration_frames
            }

            Animation::FlickerForNFrames { on_frames, off_frames, total_frames, start_on_phase } => {
                let period = on_frames + off_frames;
                let phase_frame = frame_counter % period;
                let is_on = if *start_on_phase {
                    phase_frame < *on_frames
                } else {
                    phase_frame >= *off_frames
                };
                for &sh in &stim_handles {
                    if let Some(e) = scene.stimuli.get_mut(&sh)
                        && e.stimulus.flags().anim_enabled != is_on {
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
                        if let Some(e) = scene.stimuli.get_mut(&sh) {
                            e.stimulus.move_to(false, x, y);
                        }
                    }
                }
                frame_counter + 1 >= coords.len() as u32
            }
            Animation::MoveAlongSegments2D { waypoints, speed_px_per_sec } => {
                if waypoints.len() < 2 {
                    true
                } else {
                    // Compute cumulative lengths along each segment.
                    let seg_lens: Vec<f32> = waypoints.windows(2).map(|w| {
                        let dx = w[1][0] - w[0][0];
                        let dy = w[1][1] - w[0][1];
                        (dx * dx + dy * dy).sqrt()
                    }).collect();
                    let total_len: f32 = seg_lens.iter().sum();
                    let total_frames = (total_len / speed_px_per_sec * scene.frame_rate).ceil() as u32;
                    let total_frames = total_frames.max(1);

                    // How far along the path are we at this frame?
                    let t = frame_counter as f32 / (total_frames - 1).max(1) as f32;
                    let dist = t * total_len;

                    // Walk segments to find the current interpolated position.
                    let mut accum = 0.0f32;
                    let mut pos = waypoints[0];
                    for (i, &seg_len) in seg_lens.iter().enumerate() {
                        if accum + seg_len >= dist || i + 1 == seg_lens.len() {
                            let local_t = if seg_len > 0.0 { (dist - accum) / seg_len } else { 0.0 };
                            let local_t = local_t.clamp(0.0, 1.0);
                            let a = waypoints[i];
                            let b = waypoints[i + 1];
                            pos = [a[0] + (b[0] - a[0]) * local_t, a[1] + (b[1] - a[1]) * local_t];
                            break;
                        }
                        accum += seg_len;
                    }
                    for &sh in &stim_handles {
                        if let Some(e) = scene.stimuli.get_mut(&sh) {
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
    if let Some(AnimState::Running { frame_counter }) =
        scene.animations.get_mut(&handle).map(|e| &mut e.state)
    {
        *frame_counter += 1;
    }

    // ── 3. Final actions ──────────────────────────────────────────────────────
    if done {
        finalize(handle, scene, &stim_handles, output_pending);
    }
}

fn finalize(
    handle: u32,
    scene: &mut SceneState,
    stim_handles: &[u32],
    output_pending: &mut [u64; vtl::MAX_BANKS],
) {
    let (final_action, trigger_line, captured, restart) = {
        let entry = match scene.animations.get(&handle) {
            Some(e) => e,
            None => return,
        };
        let fa = entry.final_action;
        let tl = entry.final_action_trigger_line;
        let cap = entry.captured_user_enabled.clone();
        let restart = fa.contains(FinalAction::RESTART);
        (fa, tl, cap, restart)
    };

    if final_action.contains(FinalAction::RESTORE_STATE) {
        if let Some(caps) = &captured {
            for (&sh, &was_enabled) in stim_handles.iter().zip(caps.iter()) {
                if let Some(e) = scene.stimuli.get_mut(&sh) {
                    e.stimulus.flags_mut().enabled = was_enabled;
                    e.stimulus.flags_mut().mark_dirty();
                }
            }
        }
    } else if final_action.contains(FinalAction::DISABLE) {
        for &sh in stim_handles {
            if let Some(e) = scene.stimuli.get_mut(&sh) {
                e.stimulus.flags_mut().enabled = false;
                e.stimulus.flags_mut().mark_dirty();
            }
        }
    }

    // Reset anim_enabled for animations that held it during execution.
    {
        let anim_held = matches!(
            scene.animations.get(&handle).map(|e| &e.animation),
            Some(Animation::FlickerForNFrames { .. }) | Some(Animation::CoupleVisibilityToInputTriggerLine { .. })
        );
        if anim_held {
            for &sh in stim_handles {
                if let Some(e) = scene.stimuli.get_mut(&sh)
                    && !e.stimulus.flags().anim_enabled {
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
        && let Some(bit) = trigger_line {
        output_pending[bit.bank] |= 1u64 << bit.bit;
    }

    if final_action.contains(FinalAction::END_DEFERRED) {
        scene.pending_flip = true;
        scene.deferred_mode = false;
    }

    if restart {
        if let Some(entry) = scene.animations.get_mut(&handle) {
            entry.state = AnimState::Running { frame_counter: 0 };
            entry.captured_user_enabled = None;
        }
    } else {
        if let Some(entry) = scene.animations.get_mut(&handle) {
            entry.state = AnimState::Done;
        }
    }
}
