/// Virtual Trigger Line state — top-level module, peer to `scene/` and `render/`.
///
/// Created in `main()` and shared as `Arc<Mutex<VtlState>>` with:
/// - The ZMQ thread (software-trigger and naming commands)
/// - The render backends (per-frame edge polling, future animation driving)
/// - The overlay (read-only display via try_lock)

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
