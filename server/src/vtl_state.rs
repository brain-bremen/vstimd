/// Virtual Trigger Line state — top-level module, peer to `scene/` and `render/`.
///
/// Created in `main()` and shared as `Arc<Mutex<VtlState>>` with:
/// - The ZMQ thread (software-trigger and naming commands)
/// - The render backends (per-frame edge polling, output driving)
/// - The overlay (read-only display via try_lock)
///
/// # Input vs Output
///
/// daqd acts as a facade over physical DAQ hardware: lines that are inputs to
/// daqd are also inputs to vstimd; lines that are outputs from vstimd are read
/// by daqd to drive hardware DAQ outputs.
///
/// **Input lines** (`input_state`, `input_rise_latch`, `input_fall_latch`)
/// represent signals arriving *into* vstimd from the outside world.
/// Canonical writer: `daqd` — sets bits and latches when a DAQ edge fires.
/// Software writer: ZMQ `SetVirtualTriggerLine` (kind=INPUT) commands
/// (simulate a hardware trigger for testing).
/// Reader: the render loop, via [`VtlState::poll`].
/// **vstimd never writes input lines** — not in animations, not in the render loop.
///
/// **Output lines** (`output_state`) represent signals driven *by* vstimd.
/// Canonical writers: the render loop (animations + vblank trigger).
/// Software writer: ZMQ `SetVirtualTriggerLine` (kind=OUTPUT) commands —
/// **debug/manual override only**.
/// In normal operation all output writes come from the render loop.
/// Reader: `daqd` — woken by the output semaphore strobe that `commit_staged`
/// posts each frame (it does not poll), then reads `output_state` and pulses
/// hardware DAQ lines where a bit is high.
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
///   [A] OUTPUT COMMIT + VBLANK TRIGGER HIGH
///       write_outputs(output_pending_prev | vblank_mask)
///       First action after the vblank wait returns.  Two things happen at once:
///         1. Animation outputs from the PREVIOUS frame are committed.  In DRM
///            mode the write lands within microseconds of the actual scan-out
///            flip, aligning outputs with display visibility rather than with
///            GPU submission (~0–8 ms earlier).  Outputs stay stable for a full
///            frame period, giving daqd a reliable sampling window.
///         2. The vblank trigger bit is ORed in.  It goes HIGH here to signal
///            that vstimd has just woken from vblank and is starting to compute
///            the next frame.
///
///   [A] INPUT POLL — VtlState::poll() immediately after the output commit.
///       Drains rise/fall latches; returns VtlEdges (rising/falling/current).
///
///   [S] OUTPUT SNAPSHOT — read current output_state from shm.
///       Frozen copy used by animations to detect edges on output lines.
///       The snapshot includes the bits committed at [A] (previous animation
///       outputs + vblank HIGH), so animation-to-animation chaining works:
///       animation B sees animation A's output from the previous frame here.
///       Using a snapshot — not real-time state — prevents same-frame ordering
///       effects: bits in output_pending this frame are not visible until [S]
///       of the following frame.
///
///   animations run (ALL advance before output_pending is saved):
///     read VtlEdges (input edges) + output snapshot (output-line levels/edges)
///     update stimuli
///     accumulate output changes in output_pending[]
///     (completing animations execute final actions: DISABLE, FINAL_ACTION_TRIGGER_LINE, etc.)
///
///   tessellate scene / record Vulkan command buffer
///   vkQueueSubmit
///   vkQueuePresentKHR  ← frame N queued; appears at vblank N+1
///
///   [C] VBLANK TRIGGER LOW
///       write_outputs(output_pending_prev)   ← same as [A] without the vblank bit
///       Clears the vblank trigger.  Animation outputs are unchanged.
///       The HIGH→LOW transition marks when frame N's GPU work was submitted.
///       The pulse width (time between [A] and [C]) represents vstimd's active
///       compute time for this frame: tessellation + command recording + submit.
///
///   save output_pending for the next iteration's [A]
///
/// ── vblank N+1 fires ────────────────────────────────────────────────────────
///   frame N becomes visible on the display
///   [A] commits output_pending from frame N + raises vblank trigger again
/// ```
///
/// # Output patterns
///
/// **Sustained output (default):** Animation sets the bit each frame it is
/// active; bit is absent from output_pending when the animation ends.  Mirrors
/// "stimulus is currently visible" flags.
///
/// **Stimulus-onset pulse:** Animation sets the bit in the first (or final)
/// frame of a transition.  Written at [C] → pulse is high for the interval
/// between [C] and vblank N+1, then absent from output_pending next frame.
/// Duration: approximately one frame period.
///
/// **Python-mediated handoff — one-frame gap warning:**
/// A common pattern is: animation A runs for N frames; on its final frame
/// `FINAL_ACTION_TRIGGER_LINE` fires an output bit; a Python script polls the VTL and, on
/// seeing the bit, sends a ZMQ command to enable stimulus B.
///
/// This always produces a **one-frame gap**: the output bit is committed at [C]
/// (after present), the Python round-trip takes some milliseconds, and even in
/// the best case the ZMQ command for B arrives during frame N+1 tessellation.
/// Stimulus B therefore appears no earlier than vblank N+2, while A disappeared
/// at vblank N+1.
///
/// Workaround — "pre-final trigger": fire a separate output bit at frame N-1
/// (one frame before the animation's true final frame).  Python sees it and
/// enables B.  B appears from frame N onward (vblank N+1), while A is still
/// showing its final frame N.  The result is a **one-frame overlap** (both A and
/// B are visible together for frame N) rather than a gap.  Whether the overlap
/// is acceptable depends on the experiment design.
///
/// A dedicated animation concept ("pre-final output trigger", N frames before
/// completion) would make this pattern explicit.  Not yet implemented.
///
/// **Intra-server chaining — deterministic one-frame reaction (no gap):**
/// An animation's `start_trigger` / `cancel_trigger` may address an *output*
/// line (see [`VtlBit::kind`]). This lets one animation start or cancel
/// another entirely inside the server, with no hardware loopback or Python
/// round-trip. The timing is a fixed one-frame *reaction*, not a gap:
///
///   - Animation A pulses an output bit during frame N's animation pass (into
///     `output_pending`, i.e. `staged`).
///   - At frame N+1's [A], `commit_staged` writes that bit to shm; [S] then
///     reads it back via [`VtlState::output_edges`] as a rising edge.
///   - Animation B, whose trigger reads output edges, reacts in frame N+1's pass.
///
/// Because B reads the pre-pass output-edge snapshot — not A's in-progress
/// `output_pending` — the reaction is independent of animation iteration order
/// (mirroring `output_ordering_chained_animations_one_frame_latency`). Whether
/// the visual result is seamless, a one-frame blank, or a one-frame overlap
/// depends only on when A stops drawing (its `final_action`), not on the trigger
/// plumbing. A true same-tick (zero-frame) path would require reading
/// `output_pending` mid-pass with a defined evaluation order and cycle
/// detection — deliberately out of scope.
///
/// **Vblank trigger (special):** Not part of output_pending.  Goes HIGH at [A]
/// (ORed into the output_pending_prev write) and LOW at [C] (the same
/// output_pending_prev written again, without the vblank bit).  The pulse width
/// represents vstimd's active compute time for that frame (vblank to present
/// submit), typically a fraction of a frame period.
///
/// > **Status:** [`VtlState::poll`] and [`VtlState::write_outputs`] are
/// > wired into both the DRM and winit render loops.  ZMQ SetInput*/SetOutput*
/// > commands provide an additional software-only path for testing and override.
use vtl::{VtlKind, VtlOwner, MAX_BANKS};

/// A resolved (bank, bit, kind) address into the VTL shared memory.
///
/// The kind distinguishes the two independent banks that share the same
/// (bank, bit) index space. It selects which edge set a trigger reads (input
/// vs. output); for write-only uses (action trigger lines, the vblank bit) it
/// is informational.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VtlBit {
    pub bank: usize,
    pub bit:  u8,
    pub kind: VtlKind,
}

/// The polarity (rising or falling) of a VTL signal edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum VtlEdge {
    Rising,
    Falling,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct VtlNameEntry {
    pub name:      String,
    pub bank:      u8,
    pub bit:       u8,
    pub kind: VtlKind,
}

/// Serializable VTL configuration — owned by `VtlState.config`.
///
/// This is the experiment-level (stim-config) portion of VTL config: named
/// lines that describe what each bit means for the current experiment.
/// Hardware-level settings (shm name, bank counts, vblank trigger bit) live
/// in the rig-config (`RigConfig.vtl`).
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct VtlConfig {
    pub names: Vec<VtlNameEntry>,
}

#[derive(Default, Clone)]
pub struct VtlEdges {
    pub rising:  [u64; MAX_BANKS],
    pub falling: [u64; MAX_BANKS],
    pub current: [u64; MAX_BANKS],
}

pub struct VtlState {
    pub config:  VtlConfig,
    /// Vblank trigger bit from rig-config; None disables the trigger.
    pub vblank_vtl: Option<VtlBit>,
    /// Persistent output state committed to shm at [A] each frame.
    /// Animations and ZMQ commands both write here; never reset between frames.
    pub staged:  [u64; MAX_BANKS],
    owner:       VtlOwner,
    prev_input:  [u64; MAX_BANKS],
    prev_output: [u64; MAX_BANKS],
}

impl std::ops::Deref for VtlState {
    type Target = VtlConfig;
    fn deref(&self) -> &VtlConfig { &self.config }
}

impl std::ops::DerefMut for VtlState {
    fn deref_mut(&mut self) -> &mut VtlConfig { &mut self.config }
}

impl VtlState {
    pub fn new(owner: VtlOwner) -> Self {
        Self {
            config: VtlConfig::default(),
            vblank_vtl: None,
            staged:      [0; MAX_BANKS],
            owner,
            prev_input:  [0; MAX_BANKS],
            prev_output: [0; MAX_BANKS],
        }
    }

    pub fn owner(&self) -> &VtlOwner {
        &self.owner
    }

    /// Commit `staged` to shm and signal daqd.
    /// Called at [A] once per frame from the render thread.
    pub fn commit_staged(&self) {
        let n = self.owner.num_output_banks() as usize;
        for (bank, &val) in self.staged.iter().enumerate().take(n.min(MAX_BANKS)) {
            self.owner.set_output_state(bank, val);
        }
        self.owner.signal_output();
    }

    /// Set or clear a single bit in `staged` and write through to shm immediately
    /// so that `list_lines` sees the updated value without waiting for the next [A].
    pub fn set_staged_bit(&mut self, bank: usize, bit: u8, high: bool) {
        let mask = 1u64 << bit;
        if high {
            self.staged[bank] |= mask;
        } else {
            self.staged[bank] &= !mask;
        }
        self.owner.set_output_state(bank, self.staged[bank]);
    }

    /// Write a full bank into `staged` and through to shm immediately.
    pub fn set_staged_bank(&mut self, bank: usize, value: u64) {
        self.staged[bank] = value;
        self.owner.set_output_state(bank, value);
    }

    /// Drain input latches and return edges seen since the last call.
    /// Called at [A] — exactly once per frame from the render thread.
    pub fn poll(&mut self) -> VtlEdges {
        let n = self.owner.num_input_banks() as usize;
        let mut edges = VtlEdges::default();
        for bank in 0..n.min(MAX_BANKS) {
            let cur             = self.owner.input_state(bank);
            let latched_rising  = self.owner.drain_input_rise(bank, u64::MAX);
            let latched_falling = self.owner.drain_input_fall(bank, u64::MAX);
            let derived_rising  = (!self.prev_input[bank]) & cur;
            let derived_falling = self.prev_input[bank] & (!cur);
            edges.rising[bank]  = latched_rising  | derived_rising;
            edges.falling[bank] = latched_falling | derived_falling;
            edges.current[bank] = cur;
            self.prev_input[bank] = cur;
        }
        edges
    }

    /// Read output_state from shm and return the output-line edges since the
    /// last call, analogous to [`VtlState::poll`] for inputs.
    ///
    /// Called at [S] — after `commit_staged` at [A] has written the previous
    /// frame's animation outputs to shm, and before the animation pass.  Because
    /// the read happens *after* commit but *before* animations run, an output bit
    /// raised by animation A during frame N's pass is committed at frame N+1's
    /// [A] and seen here as a rising edge in frame N+1 — giving a deterministic
    /// one-frame reaction for animation-to-animation chaining that is independent
    /// of animation iteration order.
    ///
    /// Updates `prev_output` for the next frame's edge detection.
    pub fn output_edges(&mut self) -> VtlEdges {
        let n = self.owner.num_output_banks() as usize;
        let mut edges = VtlEdges::default();
        for bank in 0..n.min(MAX_BANKS) {
            let cur = self.owner.output_state(bank);
            edges.rising[bank]  = (!self.prev_output[bank]) & cur;
            edges.falling[bank] = self.prev_output[bank] & (!cur);
            edges.current[bank] = cur;
            self.prev_output[bank] = cur;
        }
        edges
    }

    /// Returns a bitmask array with the vblank trigger bit set (if configured).
    pub fn vblank_mask(&self) -> [u64; MAX_BANKS] {
        let mut mask = [0u64; MAX_BANKS];
        if let Some(vb) = self.vblank_vtl.filter(|vb| vb.bank < MAX_BANKS) {
            mask[vb.bank] |= 1u64 << vb.bit;
        }
        mask
    }

    pub fn sync_names_to_shm(&self) {
        for (idx, e) in self.names.iter().enumerate() {
            self.owner.write_named_line(idx, &e.name, e.bank, e.bit, e.kind);
        }
        self.owner.set_n_named_lines(self.names.len());
    }
}
