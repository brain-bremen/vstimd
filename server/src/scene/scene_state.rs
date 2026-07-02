use super::animation::{AnimState, AnimationEntry};
use super::scene_config::SceneConfig;
use super::stimulus::StimulusSceneEntry;
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

// ── Non-serializable runtime state ────────────────────────────────────────────

pub struct SceneRuntimeState {
    /// Directory where named config files are stored.  Set from `--config-dir` arg.
    pub config_dir: std::path::PathBuf,
    /// True while commands should write into copy fields instead of live fields.
    pub deferred_mode: bool,
    /// Set by `DeferredMode{start:false}`; cleared by the render thread after flip.
    pub pending_flip: bool,
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
    pub command_log: std::collections::VecDeque<CommandEntry>,
    pub command_log_total: u64,
    pub command_log_errors: u64,
    pub server_start: std::time::Instant,
    /// Incremented by the render thread (or null loop) once per rendered frame.
    pub frame_count: u64,
    /// Notifies the ZMQ thread whenever `frame_count` advances.
    pub frame_notifier: std::sync::Arc<tokio::sync::watch::Sender<u64>>,
    /// Reusable buffer for the per-frame animation-handle snapshot in
    /// [`SceneState::advance_animations`]. Kept here so its allocation is reused
    /// across frames instead of being reallocated each tick.
    anim_scratch: Vec<u32>,
}

impl SceneRuntimeState {
    pub fn new_with_config_dir(config_dir: std::path::PathBuf) -> Self {
        let (tx, _rx) = tokio::sync::watch::channel(0u64);
        Self {
            config_dir,
            deferred_mode: false,
            pending_flip: false,
            frame_rate: 60.0,
            screen_size: None,
            last_uploaded_size: (0, 0),
            error_mask: 0,
            error_code: 0,
            command_log: std::collections::VecDeque::new(),
            command_log_total: 0,
            command_log_errors: 0,
            server_start: std::time::Instant::now(),
            frame_count: 0,
            frame_notifier: std::sync::Arc::new(tx),
            anim_scratch: Vec::new(),
        }
    }

    fn new() -> Self {
        Self::new_with_config_dir(std::path::PathBuf::from("."))
    }
}

// ── SceneState ────────────────────────────────────────────────────────────────

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
    pub config: SceneConfig,
    pub runtime: SceneRuntimeState,
}

impl std::ops::Deref for SceneState {
    type Target = SceneConfig;
    fn deref(&self) -> &SceneConfig {
        &self.config
    }
}

impl std::ops::DerefMut for SceneState {
    fn deref_mut(&mut self) -> &mut SceneConfig {
        &mut self.config
    }
}

impl SceneState {
    pub fn new() -> Self {
        Self {
            config: SceneConfig::default(),
            runtime: SceneRuntimeState::new(),
        }
    }

    pub fn new_with_config_dir(config_dir: std::path::PathBuf) -> Self {
        Self {
            config: SceneConfig::default(),
            runtime: SceneRuntimeState::new_with_config_dir(config_dir),
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
    pub fn add_stimulus(&mut self, entry: super::stimulus::StimulusSceneEntry) -> u32 {
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

    /// Arm an animation. Returns false if the handle is unknown. Shared by
    /// `cmd_arm_animation` and the overlay UI.
    pub fn arm_animation(&mut self, handle: u32) -> bool {
        match self.config.animations.get_mut(&handle) {
            Some(entry) => {
                entry.state = AnimState::Armed;
                true
            }
            None => false,
        }
    }

    /// Disarm an animation back to Idle, releasing any flicker `anim_enabled`
    /// hold it placed on its stimuli. Returns false if the handle is unknown.
    /// Shared by `cmd_disarm_animation` and the overlay UI.
    pub fn disarm_animation(&mut self, handle: u32) -> bool {
        let entry = match self.config.animations.get_mut(&handle) {
            Some(e) => e,
            None => return false,
        };
        let was_running = matches!(entry.state, AnimState::Running { .. });
        let stim_handles = entry.stimuli.clone();
        entry.state = AnimState::Idle;
        if was_running {
            self.release_anim_hold(&stim_handles);
        }
        true
    }

    /// Cancel an animation with a clean teardown, distinct from `disarm` — see
    /// [`super::animation::cancel_one`]. A `Running` animation applies its
    /// configured `cancel_action` (which may be empty for a hard abort) and
    /// ends in `Done`; an `Armed` one is stopped before it starts. For
    /// `CANCEL_ACTION_TRIGGER_LINE`, `output_pending` receives the pulse on
    /// `cancel_action_trigger_line`; callers outside the render loop pass a
    /// scratch buffer seeded from `VtlState::staged`. Returns false if the
    /// handle is unknown.
    /// Shared by `cmd_cancel_animation` and the overlay UI.
    pub fn cancel_animation(
        &mut self,
        handle: u32,
        output_pending: &mut [u64; vtl::MAX_BANKS],
    ) -> bool {
        super::animation::cancel_one(handle, self, output_pending)
    }

    /// Remove an animation, releasing any flicker hold if it was running.
    /// Returns false if the handle is unknown. Shared by `cmd_delete_animation`
    /// and the overlay UI.
    pub fn delete_animation(&mut self, handle: u32) -> bool {
        let entry = match self.config.animations.shift_remove(&handle) {
            Some(e) => e,
            None => return false,
        };
        if matches!(entry.state, AnimState::Running { .. }) {
            self.release_anim_hold(&entry.config.stimuli);
        }
        true
    }

    /// Release the `anim_enabled` hold a running animation placed on `stimuli`.
    /// Setting true when already true is a no-op, so this is safe unconditionally.
    fn release_anim_hold(&mut self, stimuli: &[u32]) {
        for &sh in stimuli {
            if let Some(se) = self.config.stimuli.get_mut(&sh) {
                se.stimulus.flags_mut().anim_enabled = true;
                se.stimulus.flags_mut().mark_dirty();
            }
        }
    }

    /// Advance all animations by one frame.  Called once per frame by the render
    /// thread at [S] (after output commit and input poll).
    ///
    /// `input_edges`    — rising/falling/current input lines from `VtlState::poll()`
    /// `output_edges`   — rising/falling/current output lines from `VtlState::output_edges()`,
    ///                    used to start/cancel/couple animations off output-line edges
    /// `output_pending` — `VtlState::staged` passed by value; animations set/clear bits directly;
    ///                    written back to staged after all animations have run
    pub fn advance_animations(
        &mut self,
        input_edges: &crate::vtl_state::VtlEdges,
        output_edges: &crate::vtl_state::VtlEdges,
        output_pending: &mut [u64; vtl::MAX_BANKS],
    ) {
        // Snapshot the handles into a reused buffer: `advance_one` borrows the
        // whole `SceneState` mutably, so we can't iterate `self.animations`
        // directly. Taking the scratch Vec out lets us hand `self` to the callee.
        let mut handles = std::mem::take(&mut self.runtime.anim_scratch);
        handles.clear();
        handles.extend(self.animations.keys().copied());
        for &handle in &handles {
            super::animation::advance_one(
                handle,
                self,
                input_edges,
                output_edges,
                output_pending,
            );
        }
        self.runtime.anim_scratch = handles;
    }

    // ── Deferred mode ─────────────────────────────────────────────────────────

    /// Start deferred mode: snapshot all live state into copy fields.
    pub fn begin_deferred(&mut self) {
        for entry in self.stimuli.values_mut() {
            entry.stimulus.make_copy();
        }
        self.background.make_copy();
        self.photodiode.make_copy();
        self.runtime.deferred_mode = true;
    }

    /// End deferred mode: schedule an atomic flip on the next frame boundary.
    pub fn end_deferred(&mut self) {
        self.runtime.pending_flip = true;
        self.runtime.deferred_mode = false;
    }

    /// Promote all copy fields to live. Called by the render thread when
    /// `pending_flip` is set, before animation advance and tessellation.
    pub fn apply_flip(&mut self) {
        for entry in self.stimuli.values_mut() {
            entry.stimulus.flip();
        }
        self.background.flip();
        self.photodiode.flip();
        self.runtime.pending_flip = false;
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
            self.runtime.command_log_errors += 1;
        }
        self.runtime.command_log_total += 1;
        self.runtime.command_log.push_back(CommandEntry {
            elapsed_ms: self.runtime.server_start.elapsed().as_secs_f64() * 1000.0,
            handle,
            summary,
            ok,
            response: response.handle,
        });
        if self.runtime.command_log.len() > MAX_LOG {
            self.runtime.command_log.pop_front();
        }
    }

    // ── Config persistence ────────────────────────────────────────────────────

    pub fn load_snapshot(&mut self, cfg: SceneConfig, mode: super::scene_config::LoadMode) {
        match mode {
            super::scene_config::LoadMode::Replace => {
                self.config = cfg;
                self.fixup_after_load();
            }
            super::scene_config::LoadMode::Additive => {
                let stim_offset = self.config.next_stim_handle;
                let anim_offset = self.config.next_anim_handle;
                let additive_next_stim = cfg.next_stim_handle;
                let additive_next_anim = cfg.next_anim_handle;
                for (handle, entry) in cfg.stimuli {
                    let new_handle = handle + stim_offset;
                    self.config
                        .stimuli
                        .insert(new_handle, make_entry_dirty(entry));
                }
                for (handle, mut anim) in cfg.animations {
                    for sh in &mut anim.config.stimuli {
                        *sh += stim_offset;
                    }
                    anim.state = AnimState::Idle;
                    anim.captured_user_enabled = None;
                    self.config.animations.insert(handle + anim_offset, anim);
                }
                self.config.next_stim_handle += additive_next_stim;
                self.config.next_anim_handle += additive_next_anim;
            }
        }
    }

    fn fixup_after_load(&mut self) {
        for entry in self.config.stimuli.values_mut() {
            entry.stimulus.flags_mut().dirty = true;
            entry.stimulus.reset_phase_accum();
            entry.stimulus.make_copy();
        }
        for anim in self.config.animations.values_mut() {
            anim.state = AnimState::Idle;
            anim.captured_user_enabled = None;
        }
        self.config.background.make_copy();
        self.config.photodiode.make_copy();
    }
}

impl Default for SceneState {
    fn default() -> Self {
        Self::new()
    }
}

fn make_entry_dirty(mut entry: StimulusSceneEntry) -> StimulusSceneEntry {
    entry.stimulus.flags_mut().dirty = true;
    entry.stimulus.reset_phase_accum();
    entry.stimulus.make_copy();
    entry
}
