use crate::scene::deferred::Deferred;
use crate::scene::stimulus::{StimulusFlags, Transform2D};

use super::grating_params::{GratingMask, GratingParams, Waveform};
use super::grating_pipeline::GratingPushConstants;

// ── Grating stimulus ──────────────────────────────────────────────────────────

pub struct GratingStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub color: Deferred<[f32; 4]>, // rgba; alpha = opacity
    pub size: Deferred<[f32; 2]>,  // [half_width, half_height] in pixels
    pub params: Deferred<GratingParams>,
    /// Phase accumulated by the render thread each frame from `drift_speed`.
    /// Not deferred — updated in place; reset to 0 when drift_speed is set to 0.
    pub phase_accum: f32,
}

impl GratingStimulus {
    pub fn new(
        pos: [f32; 2],
        angle: f32,
        size: [f32; 2], // [half_width, half_height] in pixels
        color: [f32; 4],
        params: GratingParams,
    ) -> Self {
        Self {
            flags: StimulusFlags { enabled: true, ..Default::default() },
            transform: Deferred::new(Transform2D { pos, angle }),
            color: Deferred::new(color),
            size: Deferred::new(size),
            params: Deferred::new(params),
            phase_accum: 0.0,
        }
    }

    // ── Setters ───────────────────────────────────────────────────────────────

    pub fn set_phase(&mut self, deferred: bool, phase: f32) {
        let prev = if deferred { self.params.copy } else { self.params.live };
        self.params.set(deferred, GratingParams { phase, ..prev });
        if !deferred {
            self.phase_accum = 0.0;
            self.flags.mark_dirty();
        }
    }

    pub fn set_sf(&mut self, deferred: bool, sf: f32) {
        let prev = if deferred { self.params.copy } else { self.params.live };
        self.params.set(deferred, GratingParams { sf, ..prev });
        if !deferred {
            self.flags.mark_dirty();
        }
    }

    pub fn set_contrast(&mut self, deferred: bool, contrast: f32) {
        let prev = if deferred { self.params.copy } else { self.params.live };
        self.params.set(deferred, GratingParams { contrast, ..prev });
        if !deferred {
            self.flags.mark_dirty();
        }
    }

    pub fn set_waveform(&mut self, deferred: bool, waveform: Waveform) {
        let prev = if deferred { self.params.copy } else { self.params.live };
        self.params.set(deferred, GratingParams { waveform, ..prev });
        if !deferred {
            self.flags.mark_dirty();
        }
    }

    pub fn set_mask(&mut self, deferred: bool, mask: GratingMask) {
        let prev = if deferred { self.params.copy } else { self.params.live };
        self.params.set(deferred, GratingParams { mask, ..prev });
        if !deferred {
            self.flags.mark_dirty();
        }
    }

    pub fn set_drift_speed(&mut self, deferred: bool, speed: f32) {
        let prev = if deferred { self.params.copy } else { self.params.live };
        self.params.set(deferred, GratingParams { drift_speed: speed, ..prev });
        if !deferred {
            self.flags.mark_dirty();
        }
    }

    pub fn set_drift_decoupled(&mut self, deferred: bool, decoupled: bool) {
        let prev = if deferred { self.params.copy } else { self.params.live };
        // decoupled = true → coupled = false
        self.params.set(deferred, GratingParams { drift_coupled: !decoupled, ..prev });
        if !deferred {
            self.flags.mark_dirty();
        }
    }

    pub fn set_drift_angle(&mut self, deferred: bool, angle_deg: f32) {
        let prev = if deferred { self.params.copy } else { self.params.live };
        self.params.set(deferred, GratingParams { drift_angle: angle_deg, ..prev });
        if !deferred {
            self.flags.mark_dirty();
        }
    }

}

// ── Grating render helpers ────────────────────────────────────────────────────

/// Per-frame phase increment (cycles) for the drift accumulator.
pub fn grating_phase_inc(s: &GratingStimulus, fps: f32) -> f32 {
    let p = &s.params.live;
    if fps <= 0.0 {
        return 0.0;
    }
    if p.drift_coupled {
        p.drift_speed / fps
    } else {
        // Project drift velocity onto the grating axis.
        let grating_rad = s.transform.live.angle.to_radians();
        let drift_rad = p.drift_angle.to_radians();
        p.drift_speed * (drift_rad - grating_rad).cos() / fps
    }
}

/// Build push constants for one grating draw call.
pub fn build_grating_push_constants(
    s: &GratingStimulus,
    screen_w: f32,
    screen_h: f32,
) -> GratingPushConstants {
    let p = &s.params.live;
    GratingPushConstants {
        screen_half: [screen_w * 0.5, screen_h * 0.5],
        center_px: s.transform.live.pos,
        half_size: s.size.live,
        sf: p.sf,
        phase: p.phase + s.phase_accum,
        ori_rad: s.transform.live.angle.to_radians(),
        contrast: p.contrast,
        _pad_color: [0; 2],
        color: s.color.live,
        waveform: p.waveform as u32,
        mask_type: p.mask as u32,
        mask_param: p.mask_param,
        _pad: 0,
    }
}
