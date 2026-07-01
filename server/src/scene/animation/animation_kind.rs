//! The `Animation` enum — the kind of behaviour an animation drives, plus its
//! per-kind parameters. Advancing these each frame lives in [`super::advance`].

use crate::vtl_state::{Edge, VtlBit};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Animation {
    /// Mirror stimulus enabled state to the level of a trigger line (input or output).
    CoupleVisibilityToTriggerLine { trigger: VtlBit, polarity: bool },
    /// Set stimulus enabled once when a trigger edge fires.
    EnableOnTriggerEdge {
        trigger: VtlBit,
        edge: Edge,
        enabled: bool,
    },
    /// Enable stimuli for `duration_frames`.
    FlashForNFrames { duration_frames: u32 },
    /// Flicker stimuli on/off.
    FlickerForNFrames {
        on_frames: u32,
        off_frames: u32,
        /// None = run forever.
        total_frames: Option<u32>,
        /// If false, start in the off-phase instead of the on-phase.
        start_on_phase: bool,
    },
    /// Move stimulus through a preloaded sequence of positions, one per frame.
    MoveAlongPath2D { coords: Vec<[f32; 2]> },
    /// Move stimulus along piecewise-linear waypoints at a constant speed.
    MoveAlongSegments2D {
        waypoints: Vec<[f32; 2]>,
        speed_px_per_sec: f32,
    },
    /// Read 2-D position from a POSIX shm float array each frame.
    ExternalPosition2D {
        shm_name: String,
        x_offset: f32,
        y_offset: f32,
    },
}

impl Animation {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::CoupleVisibilityToTriggerLine { .. } => "CoupleVisibilityToTriggerLine",
            Self::EnableOnTriggerEdge { .. } => "EnableOnTriggerEdge",
            Self::FlashForNFrames { .. } => "FlashForNFrames",
            Self::FlickerForNFrames { .. } => "FlickerForNFrames",
            Self::MoveAlongPath2D { .. } => "MoveAlongPath2D",
            Self::MoveAlongSegments2D { .. } => "MoveAlongSegments2D",
            Self::ExternalPosition2D { .. } => "ExternalPosition2D",
        }
    }
}
