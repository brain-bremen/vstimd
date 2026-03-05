pub mod animation;
pub mod photodiode;
pub mod stimulus;

use indexmap::IndexMap;

pub use animation::{Animation, FinalActionMask};
pub use photodiode::PhotoDiodeState;
pub use stimulus::{
    BitmapSeqStimulus, BitmapStimulus, Deferred, DiscStimulus, DrawMode, EllipseStimulus,
    ParticleParams, ParticleStimulus, PetalParams, PetalStimulus, PixelStimulus, RectStimulus,
    ShaderParams, ShapeAppearance, Stimulus, StimulusFlags, Transform2D, WedgeStimulus,
    WgslShaderStimulus,
};

/// All shared scene state. Wrapped in `Arc<RwLock<SceneState>>` and shared
/// between the render thread (read lock) and the ZMQ server thread (write lock).
pub struct SceneState {
    /// Stimulus objects in insertion order (insertion order = draw order).
    pub stimuli: IndexMap<u32, Stimulus>,
    /// Animation objects, each potentially assigned to a stimulus.
    pub animations: IndexMap<u32, Box<dyn Animation>>,
    /// Next handle to allocate for a new stimulus (starts at 1).
    pub next_stim_handle: u32,
    /// Next handle to allocate for a new animation (starts at 0x8000).
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
    pub screen_size: (u32, u32),
    pub error_mask: u16,
    pub error_code: i16,
}

impl SceneState {
    pub fn new() -> Self {
        Self {
            stimuli: IndexMap::new(),
            animations: IndexMap::new(),
            next_stim_handle: 1,
            next_anim_handle: 0x8000,
            background: Deferred::new([0.0, 0.0, 0.0, 1.0]),
            deferred_mode: false,
            pending_flip: false,
            photodiode: PhotoDiodeState::default(),
            default_fill: [1.0, 1.0, 1.0, 1.0],
            default_outline: [0.0, 0.0, 0.0, 1.0],
            frame_rate: 60.0,
            screen_size: (0, 0),
            error_mask: 0,
            error_code: 0,
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

    // ── Deferred mode ─────────────────────────────────────────────────────────

    /// Start deferred mode: snapshot all live state into copy fields.
    pub fn begin_deferred(&mut self) {
        for stim in self.stimuli.values_mut() {
            stim.make_copy();
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
        for stim in self.stimuli.values_mut() {
            stim.flip();
        }
        self.background.flip();
        self.photodiode.flip();
        self.pending_flip = false;
    }

    // ── Scene commands ────────────────────────────────────────────────────────

    pub fn clear_all(&mut self, protected_too: bool) {
        if protected_too {
            self.stimuli.clear();
            self.animations.clear();
        } else {
            self.stimuli.retain(|_, s| s.flags().protected);
            // Remove animations whose assigned stimulus was deleted
            let live_handles: std::collections::HashSet<u32> =
                self.stimuli.keys().copied().collect();
            self.animations.retain(|_, a| {
                a.stimulus_handle()
                    .map_or(true, |h| live_handles.contains(&h))
            });
        }
    }

    pub fn set_all_enabled(&mut self, enabled: bool, protected_too: bool) {
        for stim in self.stimuli.values_mut() {
            if protected_too || !stim.flags().protected {
                stim.flags_mut().enabled = enabled;
            }
        }
    }
}

impl Default for SceneState {
    fn default() -> Self {
        Self::new()
    }
}
