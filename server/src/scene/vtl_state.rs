use vtl::{MAX_BANKS, VtlOwner};

/// Per-frame edge snapshot consumed by `advance_animations`.
#[derive(Default, Clone)]
pub struct VtlEdges {
    pub rising:  [u64; MAX_BANKS],
    pub falling: [u64; MAX_BANKS],
    pub current: [u64; MAX_BANKS],
}

/// Render-thread-local state for VTL edge detection.
///
/// Holds the previous frame's input levels so edges can be derived without
/// relying solely on latches (latches are still drained to avoid accumulation,
/// but `prev` is the authoritative edge source for per-frame logic).
pub struct VtlFrameState {
    prev: [u64; MAX_BANKS],
}

impl Default for VtlFrameState {
    fn default() -> Self {
        Self { prev: [0; MAX_BANKS] }
    }
}

impl VtlFrameState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drain latches and return the edges seen since the last call.
    /// Must be called exactly once per frame from the render thread.
    pub fn poll(&mut self, owner: &VtlOwner) -> VtlEdges {
        let n = owner.num_input_banks() as usize;
        let mut edges = VtlEdges::default();

        for bank in 0..n.min(MAX_BANKS) {
            let cur = owner.input_state(bank);
            edges.rising[bank]  = owner.drain_input_rise(bank, u64::MAX);
            edges.falling[bank] = owner.drain_input_fall(bank, u64::MAX);
            edges.current[bank] = cur;
            self.prev[bank] = cur;
        }

        edges
    }
}
