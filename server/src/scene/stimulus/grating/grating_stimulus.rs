use crate::scene::deferred::Deferred;
use crate::scene::stimulus::{StimulusFlags, Transform2D};

use super::grating_params::{GratingMask, GratingParams, Waveform};
use super::grating_pipeline::GratingPushConstants;

// ── Grating stimulus ──────────────────────────────────────────────────────────

pub struct GratingStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub size: Deferred<[f32; 2]>, // [half_width, half_height] in pixels
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
        params: GratingParams,
    ) -> Self {
        Self {
            flags: StimulusFlags { enabled: true, ..Default::default() },
            transform: Deferred::new(Transform2D { pos, angle }),
            size: Deferred::new(size),
            params: Deferred::new(params),
            phase_accum: 0.0,
        }
    }

    // ── Setters ───────────────────────────────────────────────────────────────

    pub fn set_fore_color(&mut self, deferred: bool, color: [f32; 4]) {
        let prev = if deferred { self.params.copy } else { self.params.live };
        self.params.set(deferred, GratingParams { fore_color: color, ..prev });
        if !deferred {
            self.flags.mark_dirty();
        }
    }

    pub fn set_back_color(&mut self, deferred: bool, color: [f32; 4]) {
        let prev = if deferred { self.params.copy } else { self.params.live };
        self.params.set(deferred, GratingParams { back_color: color, ..prev });
        if !deferred {
            self.flags.mark_dirty();
        }
    }

    pub fn set_opacity(&mut self, deferred: bool, opacity: f32) {
        let prev = if deferred { self.params.copy } else { self.params.live };
        self.params.set(deferred, GratingParams { opacity, ..prev });
        if !deferred {
            self.flags.mark_dirty();
        }
    }

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
            if speed == 0.0 {
                self.phase_accum = 0.0;
            }
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

/// Phase increment per frame for drift animation (called by the render thread).
pub fn grating_phase_inc(s: &GratingStimulus, fps: f32) -> f32 {
    let p = &s.params.live;
    if p.drift_coupled {
        p.drift_speed / fps
    } else {
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
        global_opacity: p.opacity,
        _pad_color: 0,
        fore_color: p.fore_color,
        back_color: p.back_color,
        waveform: p.waveform as u32,
        mask_type: p.mask as u32,
        mask_param: p.mask_param,
        _pad: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::stimulus::grating::grating_params::GratingParams;

    fn default_stim() -> GratingStimulus {
        GratingStimulus::new([0.0, 0.0], 0.0, [100.0, 100.0], GratingParams::default())
    }

    // ── set_phase ──────────────────────────────────────────────────────────────

    #[test]
    fn set_phase_immediate_resets_accum() {
        let mut s = default_stim();
        s.phase_accum = 3.14;
        s.set_phase(false, 0.5);
        assert_eq!(s.phase_accum, 0.0);
        assert_eq!(s.params.live.phase, 0.5);
    }

    #[test]
    fn set_phase_deferred_preserves_accum() {
        let mut s = default_stim();
        s.phase_accum = 3.14;
        s.set_phase(true, 0.5);
        assert_eq!(s.phase_accum, 3.14);
        // live untouched, copy updated
        assert_ne!(s.params.live.phase, 0.5);
        assert_eq!(s.params.copy.phase, 0.5);
    }

    // ── set_fore_color / set_back_color / set_opacity ─────────────────────────

    #[test]
    fn set_fore_color_immediate() {
        let mut s = default_stim();
        s.set_fore_color(false, [1.0, 0.0, 0.0, 0.5]);
        assert_eq!(s.params.live.fore_color, [1.0, 0.0, 0.0, 0.5]);
    }

    #[test]
    fn set_fore_color_deferred() {
        let mut s = default_stim();
        s.set_fore_color(true, [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(s.params.live.fore_color, [1.0, 1.0, 1.0, 1.0]); // live unchanged
        assert_eq!(s.params.copy.fore_color, [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn set_back_color_immediate() {
        let mut s = default_stim();
        s.set_back_color(false, [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(s.params.live.back_color, [0.0, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn set_opacity_immediate() {
        let mut s = default_stim();
        s.set_opacity(false, 0.5);
        assert_eq!(s.params.live.opacity, 0.5);
    }

    #[test]
    fn push_constants_colors_and_opacity() {
        let mut s = default_stim();
        s.set_fore_color(false, [1.0, 0.5, 0.25, 0.8]);
        s.set_opacity(false, 0.75);
        s.set_back_color(false, [0.1, 0.2, 0.3, 0.0]);
        let pc = build_grating_push_constants(&s, 800.0, 600.0);
        assert_eq!(pc.fore_color, [1.0, 0.5, 0.25, 0.8]);
        assert_eq!(pc.back_color, [0.1, 0.2, 0.3, 0.0]);
        assert_eq!(pc.global_opacity, 0.75);
    }
}
