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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::stimulus::grating::grating_params::GratingParams;

    fn default_stim() -> GratingStimulus {
        GratingStimulus::new([0.0, 0.0], 0.0, [100.0, 100.0], [1.0, 1.0, 1.0, 1.0], GratingParams::default())
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

    // ── deferred mode branching ────────────────────────────────────────────────

    #[test]
    fn set_sf_immediate_writes_live() {
        let mut s = default_stim();
        let original_sf = s.params.live.sf; // 0.05 default
        s.set_sf(false, 0.1);
        assert_eq!(s.params.live.sf, 0.1);
        // immediate write leaves copy untouched
        assert_eq!(s.params.copy.sf, original_sf);
    }

    #[test]
    fn set_sf_deferred_writes_copy_not_live() {
        let mut s = default_stim();
        let original_live = s.params.live.sf;
        s.set_sf(true, 0.99);
        assert_eq!(s.params.live.sf, original_live);
        assert_eq!(s.params.copy.sf, 0.99);
    }

    #[test]
    fn set_drift_decoupled_inverts_to_coupled() {
        let mut s = default_stim();
        s.set_drift_decoupled(false, true);
        assert!(!s.params.live.drift_coupled);
        s.set_drift_decoupled(false, false);
        assert!(s.params.live.drift_coupled);
    }

    // ── grating_phase_inc ──────────────────────────────────────────────────────

    #[test]
    fn phase_inc_zero_fps_returns_zero() {
        let s = default_stim();
        assert_eq!(grating_phase_inc(&s, 0.0), 0.0);
        assert_eq!(grating_phase_inc(&s, -1.0), 0.0);
    }

    #[test]
    fn phase_inc_coupled_is_speed_over_fps() {
        let mut s = default_stim();
        s.params.live.drift_speed = 2.0;
        s.params.live.drift_coupled = true;
        let inc = grating_phase_inc(&s, 60.0);
        assert!((inc - 2.0 / 60.0).abs() < 1e-6);
    }

    #[test]
    fn phase_inc_decoupled_projects_onto_grating_axis() {
        let mut s = GratingStimulus::new(
            [0.0, 0.0],
            45.0,                            // grating orientation 45°
            [100.0, 100.0],
            [1.0; 4],
            GratingParams { drift_speed: 1.0, drift_coupled: false, drift_angle: 45.0, ..GratingParams::default() },
        );
        // drift_angle == grating_angle → cos(0) = 1 → inc = speed/fps
        let inc = grating_phase_inc(&s, 60.0);
        assert!((inc - 1.0 / 60.0).abs() < 1e-6);

        // 90° offset → cos(90°) ≈ 0
        s.params.live.drift_angle = 135.0; // 135 - 45 = 90°
        let inc_perp = grating_phase_inc(&s, 60.0);
        assert!(inc_perp.abs() < 1e-6);
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
