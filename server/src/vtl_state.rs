/// Virtual Trigger Line state — top-level module, peer to `scene/` and `render/`.
///
/// Created in `main()` and shared as `Arc<Mutex<VtlState>>` with:
/// - The ZMQ thread (software-trigger and naming commands)
/// - The render backends (per-frame edge polling, output driving)
/// - The overlay (read-only display via try_lock)
///
/// # Input vs Output
///
/// **Input lines** (`input_state`, `input_rise_latch`, `input_fall_latch`)
/// represent signals arriving *into* vstimd from the outside world.
/// Canonical writer: `nidaqd` — sets bits and latches when a DAQ edge fires.
/// Software writer: ZMQ `SetInput*` commands (simulate a hardware trigger).
/// Reader: the render loop, via [`VtlState::poll`].
///
/// **Output lines** (`output_state`) represent signals driven *by* vstimd.
/// Canonical writer: the render loop, via [`VtlState::write_outputs`].
/// Software writer: ZMQ `SetOutput*` commands (manual override / testing).
/// Reader: `nidaqd` — pulses hardware DAQ lines when a bit goes high.
///
/// # Timing: where VTL fits in the render loop
///
/// The render loop runs once per vblank.  Frame N refers to the content that
/// becomes visible at vblank N.  The loop for frame N looks like this:
///
/// ```text
/// ── vblank N fires ──────────────────────────────────────────────────────────
///   (DRM: wait_vblank() returns)
///   (winit: vkWaitForPresentKHR confirms that frame N-1 is now on screen)
///
///   [A] INPUT POLL — call VtlState::poll() here.
///       Reads input_state and drains rise/fall latches that nidaqd wrote
///       since the last poll.  Returns VtlEdges (per-bank rising/falling masks)
///       for the animation system.
///       This is the first moment we know frame N-1 is confirmed on screen,
///       so edges here represent events that occurred during frame N-1's display.
///
///   [B] OUTPUT WRITE (frame-start position) — optional, for "preparation-gated"
///       output patterns.  Write outputs HERE if they should be high while
///       vstimd is preparing frame N (see below).
///
///   tessellate scene / advance animations → produces frame N's pixel content
///   record Vulkan command buffer
///   vkQueueSubmit
///   vkQueuePresentKHR  ← frame N is now queued; it will appear at vblank N+1
///
///   [C] OUTPUT WRITE (frame-end position) — call VtlState::write_outputs() here.
///       Writes the output state that animations computed for frame N.
///       nidaqd will read this state and can pulse hardware lines during the
///       interval between now and vblank N+1, just before frame N becomes visible.
///       For a stimulus-onset marker ("frame N is now on screen"), this is the
///       right place: outputs go high just before vblank N+1.
///
/// ── vblank N+1 fires ────────────────────────────────────────────────────────
///   frame N becomes visible on the display
///   next iteration: poll() sees any edges that occurred since the last poll
/// ```
///
/// # Output patterns
///
/// **Sustained output (default):** Write at [C].  The output stays high until
/// the animation clears it.  Used for "stimulus is currently visible" flags.
///
/// **Stimulus-onset pulse:** Animation sets the bit at the final frame of a
/// transition.  Written at [C] → the pulse goes high just before vblank N+1
/// (frame N appears) and is cleared by the animation at [C] in the next frame.
/// Duration: one frame period.
///
/// **Preparation-gated pulse** (special case): Write HIGH at [B] and LOW at [C]
/// in the same iteration.  The trigger is high only during vstimd's compute and
/// render time for frame N (~half a frame period, GPU-dependent).  Use when
/// nidaqd needs to know exactly when vstimd is actively computing, rather than
/// when the frame is on screen.  This requires two separate `write_outputs`
/// calls per frame with different state arrays.
///
/// > **Status:** [`VtlState::poll`] and [`VtlState::write_outputs`] are
/// > implemented but not yet wired into the render loop.  See the `// TODO: VTL`
/// > comments in `render/drm/mod.rs` and `render/winit_vk/mod.rs`.
/// > Currently, both directions are only accessible via ZMQ commands.

use vtl::{Direction, VtlOwner, MAX_BANKS};

pub struct VtlNameEntry {
    pub name:      String,
    pub bank:      u8,
    pub bit:       u8,
    pub direction: Direction,
}

#[derive(Default, Clone)]
pub struct VtlEdges {
    pub rising:  [u64; MAX_BANKS],
    pub falling: [u64; MAX_BANKS],
    pub current: [u64; MAX_BANKS],
}

pub struct VtlState {
    owner: VtlOwner,
    prev:  [u64; MAX_BANKS],
    pub names: Vec<VtlNameEntry>,
}

impl VtlState {
    pub fn new(owner: VtlOwner) -> Self {
        Self { owner, prev: [0; MAX_BANKS], names: Vec::new() }
    }

    pub fn owner(&self) -> &VtlOwner {
        &self.owner
    }

    /// Drain latches and return edges seen since the last call.
    /// Must be called exactly once per frame from the render thread.
    pub fn poll(&mut self) -> VtlEdges {
        let n = self.owner.num_input_banks() as usize;
        let mut edges = VtlEdges::default();
        for bank in 0..n.min(MAX_BANKS) {
            let cur             = self.owner.input_state(bank);
            let latched_rising  = self.owner.drain_input_rise(bank, u64::MAX);
            let latched_falling = self.owner.drain_input_fall(bank, u64::MAX);
            let derived_rising  = (!self.prev[bank]) & cur;
            let derived_falling = self.prev[bank] & (!cur);
            edges.rising[bank]  = latched_rising  | derived_rising;
            edges.falling[bank] = latched_falling | derived_falling;
            edges.current[bank] = cur;
            self.prev[bank] = cur;
        }
        edges
    }

    pub fn write_outputs(&self, state: &[u64; MAX_BANKS]) {
        let n = self.owner.num_output_banks() as usize;
        for bank in 0..n.min(MAX_BANKS) {
            self.owner.set_output_state(bank, state[bank]);
        }
    }

    pub fn sync_names_to_shm(&self) {
        for (idx, e) in self.names.iter().enumerate() {
            self.owner.write_named_line(idx, &e.name, e.bank, e.bit, e.direction);
        }
        self.owner.set_n_named_lines(self.names.len());
    }
}
