use std::sync::{Arc, RwLock};

use crate::scene::{
    GratingParams, GratingStimulus, SceneState, Stimulus, StimulusSceneEntry, Waveform,
};
use crate::timing::FrameStats;
use uuid::Uuid;

pub struct BenchmarkResult {
    pub grating_count: usize,
    pub duration_frames: u64,
    pub drop_count: u64,
}

enum Phase {
    Idle,
    Running {
        start_frame: u64,
        start_drops: u64,
        duration_frames: u64,
        handles: Vec<u32>,
    },
    Done(BenchmarkResult),
}

pub struct BenchmarkState {
    phase: Phase,
}

impl Default for BenchmarkState {
    fn default() -> Self {
        Self::new()
    }
}

impl BenchmarkState {
    pub fn new() -> Self {
        Self { phase: Phase::Idle }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.phase, Phase::Running { .. })
    }

    /// Start the grating stress test: reset drops, spawn gratings, begin timing.
    pub fn start_grating_stress(
        &mut self,
        scene: &Arc<RwLock<SceneState>>,
        frame_stats: &mut FrameStats,
        screen_size: (u32, u32),
        cols: usize,
        rows: usize,
        duration_frames: u64,
    ) {
        frame_stats.reset_drops();
        let start_frame = frame_stats.summary().frame_index;

        let mut handles = Vec::with_capacity(cols * rows);
        if let Ok(mut sc) = scene.try_write() {
            let (sw, sh) = screen_size;
            let cell_w = sw as f32 / cols as f32;
            let cell_h = sh as f32 / rows as f32;
            let stim_w = cell_w * 0.9;
            let stim_h = cell_h * 0.9;

            for row in 0..rows {
                for col in 0..cols {
                    // pixel-space, origin at screen centre, Y-up
                    let cx = col as f32 * cell_w + cell_w / 2.0 - sw as f32 / 2.0;
                    let cy = sh as f32 / 2.0 - (row as f32 * cell_h + cell_h / 2.0);
                    let angle = (col * rows + row) as f32 * (180.0 / (cols * rows) as f32);

                    let h = sc.alloc_stim_handle();
                    sc.stimuli.insert(
                        h,
                        StimulusSceneEntry::new(
                            Uuid::new_v4(),
                            None,
                            Stimulus::Grating(GratingStimulus::new(
                                [cx, cy],
                                angle,
                                [stim_w / 2.0, stim_h / 2.0],
                                GratingParams {
                                    sf: 0.05,
                                    contrast: 1.0,
                                    drift_speed: 1.0,
                                    waveform: Waveform::Sin,
                                    drift_coupled: true,
                                    ..Default::default()
                                },
                            )),
                        ),
                    );
                    handles.push(h);
                }
            }
            log::info!("Benchmark: spawned {} gratings", handles.len());
        } else {
            log::warn!("Benchmark: could not acquire scene write lock");
        }

        self.phase = Phase::Running {
            start_frame,
            start_drops: 0,
            duration_frames,
            handles,
        };
    }

    /// Call once per frame while the overlay is active. Transitions to Done when
    /// the configured number of frames have elapsed.
    pub fn tick(&mut self, scene: &Arc<RwLock<SceneState>>, frame_stats: &FrameStats) {
        let current_frame = frame_stats.summary().frame_index;
        let current_drops = frame_stats.summary().drop_count;

        let should_finish = matches!(
            &self.phase,
            Phase::Running { start_frame, duration_frames, .. }
                if current_frame.saturating_sub(*start_frame) >= *duration_frames
        );

        if should_finish
            && let Phase::Running {
                start_drops,
                duration_frames,
                handles,
                ..
            } = std::mem::replace(&mut self.phase, Phase::Idle)
        {
            let grating_count = handles.len();
            if let Ok(mut sc) = scene.try_write() {
                for h in handles {
                    sc.stimuli.shift_remove(&h);
                }
            }
            let drop_count = current_drops.saturating_sub(start_drops);
            self.phase = Phase::Done(BenchmarkResult {
                grating_count,
                duration_frames,
                drop_count,
            });
        }
    }

    /// Remaining frames, or None if not running.
    pub fn remaining_frames(&self, frame_stats: &FrameStats) -> Option<u64> {
        if let Phase::Running {
            start_frame,
            duration_frames,
            ..
        } = self.phase
        {
            let elapsed = frame_stats
                .summary()
                .frame_index
                .saturating_sub(start_frame);
            Some(duration_frames.saturating_sub(elapsed))
        } else {
            None
        }
    }

    pub fn last_result(&self) -> Option<&BenchmarkResult> {
        if let Phase::Done(ref r) = self.phase {
            Some(r)
        } else {
            None
        }
    }
}
