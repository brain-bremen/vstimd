const FRAME_HISTORY_SIZE: usize = 120;

/// Per-render-loop timing bookkeeping: aggregated frame statistics, last
/// per-phase breakdown, and the swapchain frame index.
pub struct FrameTiming {
    pub stats: FrameStats,
    pub last_phases: FramePhases,
    /// Swapchain slot index (cycles 0..swapchain_len); distinct from the global
    /// frame counter inside `FrameStats`.
    pub frame_index: usize,
}

impl FrameTiming {
    pub fn new(refresh_hz: f64) -> Self {
        Self {
            stats: FrameStats::new(refresh_hz),
            last_phases: FramePhases::default(),
            frame_index: 0,
        }
    }
}

/// Timing information for one successfully presented frame.
///
/// Returned from `render_frame` on every successful present.
/// The sequence of `FrameTick` values **is** the time axis of the server:
/// each tick maps a vblank serial number to the wall-clock time at which
/// it fired.
///
/// # Scheduling
/// - Use `frame` to express stimulus schedules in vblanks:
///   "start at frame N, show for M frames". Integer arithmetic, exact.
/// - Use `vblank_time` for experiment logging: record it as the stimulus
///   onset time in your data file.
/// - Check `dropped_frames` each tick; a non-zero value means the GPU
///   missed a deadline and the previous stimulus was shown for an extra
///   vblank. Flag the trial if timing precision matters.
#[derive(Debug, Clone)]
pub struct FrameTick {
    /// Present-ID assigned to this frame (1-based, resets after swapchain
    /// recreation). Monotonically increasing within a session.
    /// Use as the frame-number axis for scheduling stimuli.
    pub frame: u64,
    /// `Instant` captured immediately after `vkWaitForPresentKHR` returned,
    /// i.e. the best available proxy for the vblank that confirmed the
    /// *previous* frame on screen. On the first frame (no prior present)
    /// this is the time `render_frame` was entered.
    pub vblank_time: std::time::Instant,
    /// Extra vblanks elapsed beyond the expected one since the previous tick.
    /// 0 = on time.  1 = one dropped frame (GPU overran its budget once).
    pub dropped_frames: u32,
    /// Per-phase breakdown for profiling (see `FramePhases`).
    pub phases: FramePhases,
}

pub struct FrameSummary {
    pub fps: f64,
    pub mean_ms: f64,
    pub std_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub drop_count: u64,
    pub frame_index: u64,
}

/// Wall-clock time (µs) spent in each phase of `render_frame`.
/// Accumulated per frame and available for logging or overlay display.
#[derive(Debug, Clone, Copy, Default)]
pub struct FramePhases {
    pub tessellate_us: u32, // scene write-lock: tess + GPU upload
    pub fence_us: u32,      // wait_for_fences
    pub acquire_us: u32,    // acquire_next_image
    pub record_us: u32,     // command buffer record
    pub submit_us: u32,     // queue_submit + queue_present
}

pub struct FrameStats {
    frame_index: u64,
    last_present: Option<std::time::Instant>,
    durations_ns: [u64; FRAME_HISTORY_SIZE],
    ring_head: usize,
    valid_count: usize,
    drop_count: u64,
    expected_frame_ns: u64,
}

impl FrameStats {
    pub fn new(target_hz: f64) -> Self {
        Self {
            frame_index: 0,
            last_present: None,
            durations_ns: [0; FRAME_HISTORY_SIZE],
            ring_head: 0,
            valid_count: 0,
            drop_count: 0,
            expected_frame_ns: (1_000_000_000.0 / target_hz) as u64,
        }
    }

    /// Record a presented frame using the vblank timestamp captured
    /// immediately after `vkWaitForPresentKHR` returned.
    ///
    /// Using the actual vblank time rather than `Instant::now()` gives
    /// accurate inter-frame intervals independent of render duration.
    ///
    /// Returns the number of frames dropped since the previous call
    /// (0 = on time). The same value is included in the `FrameTick`
    /// returned from `render_frame`.
    /// Returns true while still in the warmup window (first few frames).
    /// Callers should suppress drop warnings during this period.
    pub fn is_warming_up(&self) -> bool {
        self.frame_index < 5
    }

    pub fn on_present(&mut self, vblank_time: std::time::Instant) -> u32 {
        let dropped = if let Some(last) = self.last_present {
            let dur_ns = vblank_time.duration_since(last).as_nanos() as u64;
            // 5/4 threshold: trigger if the interval exceeds 1.25× the expected period.
            // Using round-to-nearest division avoids the truncation bug where
            // 2 × period computes as 1.999× and floors to 1 → sub(1) = 0.
            let threshold = self.expected_frame_ns * 5 / 4;
            let d = if dur_ns > threshold && self.expected_frame_ns > 0 {
                let n = ((dur_ns + self.expected_frame_ns / 2) / self.expected_frame_ns)
                    .saturating_sub(1) as u32;
                self.drop_count += n as u64;
                n
            } else {
                0
            };
            self.durations_ns[self.ring_head] = dur_ns;
            self.ring_head = (self.ring_head + 1) % FRAME_HISTORY_SIZE;
            if self.valid_count < FRAME_HISTORY_SIZE {
                self.valid_count += 1;
            }
            d
        } else {
            0
        };
        self.last_present = Some(vblank_time);
        self.frame_index += 1;
        dropped
    }

    /// Frame durations in chronological order (oldest first).
    pub fn durations_recent_ns(&self) -> impl Iterator<Item = u64> + '_ {
        let n = self.valid_count.min(FRAME_HISTORY_SIZE);
        let start = (self.ring_head + FRAME_HISTORY_SIZE - n) % FRAME_HISTORY_SIZE;
        (0..n).map(move |i| self.durations_ns[(start + i) % FRAME_HISTORY_SIZE])
    }

    pub fn expected_ns(&self) -> u64 {
        self.expected_frame_ns
    }

    /// Reset the cumulative drop counter to zero (e.g. before a benchmark).
    pub fn reset_drops(&mut self) {
        self.drop_count = 0;
    }

    pub fn summary(&self) -> FrameSummary {
        let durations = &self.durations_ns[..self.valid_count.min(FRAME_HISTORY_SIZE)];
        if durations.is_empty() {
            return FrameSummary {
                fps: 0.0,
                mean_ms: 0.0,
                std_ms: 0.0,
                min_ms: 0.0,
                max_ms: 0.0,
                drop_count: self.drop_count,
                frame_index: self.frame_index,
            };
        }
        let n = durations.len() as f64;
        let mean_ns = durations.iter().sum::<u64>() as f64 / n;
        let var_ns = durations
            .iter()
            .map(|&d| {
                let x = d as f64 - mean_ns;
                x * x
            })
            .sum::<f64>()
            / n;
        FrameSummary {
            fps: if mean_ns > 0.0 {
                1_000_000_000.0 / mean_ns
            } else {
                0.0
            },
            mean_ms: mean_ns / 1_000_000.0,
            std_ms: var_ns.sqrt() / 1_000_000.0,
            min_ms: *durations.iter().min().unwrap() as f64 / 1_000_000.0,
            max_ms: *durations.iter().max().unwrap() as f64 / 1_000_000.0,
            drop_count: self.drop_count,
            frame_index: self.frame_index,
        }
    }
}
