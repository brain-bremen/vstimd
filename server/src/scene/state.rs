use std::sync::Arc;

use indexmap::IndexMap;

use super::deferred::Deferred;
use super::photodiode::PhotoDiodeState;
use super::stimulus::StimulusEntry;

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

    /// VTL shared memory owner. `None` when running without VTL (e.g. null renderer on non-Linux).
    /// `Arc` so the render thread can hold a ref without going through the scene RwLock each frame.
    pub vtl: Option<Arc<vtl::VtlOwner>>,
    /// Named VTL lines registered via `SetVtlLineName`. Source of truth; shm table mirrors this.
    pub vtl_names: Vec<VtlNameEntry>,
}

/// A named VTL line stored in `SceneState`.
pub struct VtlNameEntry {
    pub name: String,
    pub bank: u8,
    pub bit:  u8,
    pub direction: vtl::Direction,
}

impl SceneState {
    pub fn new() -> Self {
        Self {
            stimuli: IndexMap::new(),
            next_stim_handle: 1,
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
            vtl: None,
            vtl_names: Vec::new(),
        }
    }

    // ── Handle allocation ─────────────────────────────────────────────────────

    pub fn alloc_stim_handle(&mut self) -> u32 {
        let h = self.next_stim_handle;
        self.next_stim_handle += 1;
        h
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
