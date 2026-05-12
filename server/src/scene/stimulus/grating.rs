use super::super::deferred::Deferred;
use super::super::state::SceneState;
use super::{GratingMask, GratingParams, GratingStimulus, Stimulus, StimulusFlags, Transform2D, Waveform};
use crate::proto;

// ── Proto ↔ scene type conversions ────────────────────────────────────────────

pub fn proto_to_waveform(v: i32) -> Waveform {
    match proto::WaveformType::try_from(v).unwrap_or(proto::WaveformType::Unspecified) {
        proto::WaveformType::Unspecified | proto::WaveformType::Sin => Waveform::Sin,
        proto::WaveformType::Sqr => Waveform::Sqr,
        proto::WaveformType::Saw => Waveform::Saw,
        proto::WaveformType::Tri => Waveform::Tri,
    }
}

pub fn waveform_to_proto(w: Waveform) -> proto::WaveformType {
    match w {
        Waveform::Sin => proto::WaveformType::Sin,
        Waveform::Sqr => proto::WaveformType::Sqr,
        Waveform::Saw => proto::WaveformType::Saw,
        Waveform::Tri => proto::WaveformType::Tri,
    }
}

pub fn proto_to_mask(v: i32) -> GratingMask {
    match proto::MaskType::try_from(v).unwrap_or(proto::MaskType::Unspecified) {
        proto::MaskType::Unspecified | proto::MaskType::None => GratingMask::None,
        proto::MaskType::Circle => GratingMask::Circle,
        proto::MaskType::Gauss => GratingMask::Gauss,
        proto::MaskType::Hann => GratingMask::Hann,
        proto::MaskType::RaisedCos => GratingMask::RaisedCos,
    }
}

pub fn mask_to_proto(m: GratingMask) -> proto::MaskType {
    match m {
        GratingMask::None => proto::MaskType::None,
        GratingMask::Circle => proto::MaskType::Circle,
        GratingMask::Gauss => proto::MaskType::Gauss,
        GratingMask::Hann => proto::MaskType::Hann,
        GratingMask::RaisedCos => proto::MaskType::RaisedCos,
    }
}

// ── SceneState grating command implementations ────────────────────────────────

impl SceneState {
    // ── CreateGrating ─────────────────────────────────────────────────────────

    pub fn cmd_create_grating(&mut self, cmd: proto::CreateGratingRequest) -> proto::Response {
        let center = cmd.center.unwrap_or_default();
        let width = if cmd.width == 0.0 { 200.0 } else { cmd.width };
        let height = if cmd.height == 0.0 { 200.0 } else { cmd.height };
        let sf = if cmd.sf == 0.0 { 0.05 } else { cmd.sf };
        let contrast = cmd.contrast;
        let opacity = cmd.opacity;

        let color = match cmd.color {
            Some(c) => [c.r, c.g, c.b, opacity],
            None => [1.0, 1.0, 1.0, opacity],
        };

        // drift_decoupled = false (proto3 default / absent) → coupled (most common case).
        let drift_coupled = !cmd.drift_decoupled;

        let handle = self.alloc_stim_handle();
        self.stimuli.insert(
            handle,
            Stimulus::Grating(GratingStimulus {
                flags: StimulusFlags { enabled: true, ..Default::default() },
                transform: Deferred::new(Transform2D {
                    pos: [center.x, center.y],
                    angle: cmd.angle,
                }),
                color: Deferred::new(color),
                size: Deferred::new([width / 2.0, height / 2.0]),
                params: Deferred::new(GratingParams {
                    sf,
                    phase: cmd.phase,
                    contrast,
                    waveform: proto_to_waveform(cmd.waveform),
                    mask: proto_to_mask(cmd.mask),
                    mask_param: cmd.mask_param,
                    drift_speed: cmd.drift_speed,
                    drift_coupled,
                    drift_angle: cmd.drift_angle,
                }),
                phase_accum: 0.0,
            }),
        );
        ok_handle(handle)
    }

    // ── SetGratingPhase ───────────────────────────────────────────────────────

    pub fn cmd_set_grating_phase(
        &mut self,
        handle: u32,
        cmd: proto::SetGratingPhaseRequest,
    ) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => {
                let deferred = self.deferred_mode;
                let prev = if deferred { s.params.copy } else { s.params.live };
                s.params.set(deferred, GratingParams { phase: cmd.phase, ..prev });
                if !deferred {
                    s.phase_accum = 0.0;
                    s.flags.mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err_wrong_type(stim, "SetGratingPhase"),
        }
    }

    // ── SetGratingSf ──────────────────────────────────────────────────────────

    pub fn cmd_set_grating_sf(
        &mut self,
        handle: u32,
        cmd: proto::SetGratingSfRequest,
    ) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => {
                let deferred = self.deferred_mode;
                let prev = if deferred { s.params.copy } else { s.params.live };
                s.params.set(deferred, GratingParams { sf: cmd.sf, ..prev });
                if !deferred {
                    s.flags.mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err_wrong_type(stim, "SetGratingSf"),
        }
    }

    // ── SetGratingContrast ────────────────────────────────────────────────────

    pub fn cmd_set_grating_contrast(
        &mut self,
        handle: u32,
        cmd: proto::SetGratingContrastRequest,
    ) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => {
                let deferred = self.deferred_mode;
                let prev = if deferred { s.params.copy } else { s.params.live };
                s.params.set(deferred, GratingParams { contrast: cmd.contrast, ..prev });
                if !deferred {
                    s.flags.mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err_wrong_type(stim, "SetGratingContrast"),
        }
    }

    // ── SetGratingWaveform ────────────────────────────────────────────────────

    pub fn cmd_set_grating_waveform(
        &mut self,
        handle: u32,
        cmd: proto::SetGratingWaveformRequest,
    ) -> proto::Response {
        let waveform = proto_to_waveform(cmd.waveform);
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => {
                let deferred = self.deferred_mode;
                let prev = if deferred { s.params.copy } else { s.params.live };
                s.params.set(deferred, GratingParams { waveform, ..prev });
                if !deferred {
                    s.flags.mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err_wrong_type(stim, "SetGratingWaveform"),
        }
    }

    // ── SetGratingMask ────────────────────────────────────────────────────────

    pub fn cmd_set_grating_mask(
        &mut self,
        handle: u32,
        cmd: proto::SetGratingMaskRequest,
    ) -> proto::Response {
        let mask = proto_to_mask(cmd.mask);
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => {
                let deferred = self.deferred_mode;
                let prev = if deferred { s.params.copy } else { s.params.live };
                s.params.set(deferred, GratingParams { mask, ..prev });
                if !deferred {
                    s.flags.mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err_wrong_type(stim, "SetGratingMask"),
        }
    }

    // ── SetGratingDriftSpeed ──────────────────────────────────────────────────

    pub fn cmd_set_grating_drift_speed(
        &mut self,
        handle: u32,
        cmd: proto::SetGratingDriftSpeedRequest,
    ) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => {
                let deferred = self.deferred_mode;
                let prev = if deferred { s.params.copy } else { s.params.live };
                s.params.set(deferred, GratingParams { drift_speed: cmd.speed, ..prev });
                if !deferred {
                    s.flags.mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err_wrong_type(stim, "SetGratingDriftSpeed"),
        }
    }

    // ── SetGratingDriftDecoupled ──────────────────────────────────────────────

    pub fn cmd_set_grating_drift_decoupled(
        &mut self,
        handle: u32,
        cmd: proto::SetGratingDriftDecoupledRequest,
    ) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => {
                let deferred = self.deferred_mode;
                let prev = if deferred { s.params.copy } else { s.params.live };
                // decoupled = true → coupled = false
                s.params.set(
                    deferred,
                    GratingParams { drift_coupled: !cmd.decoupled, ..prev },
                );
                if !deferred {
                    s.flags.mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err_wrong_type(stim, "SetGratingDriftDecoupled"),
        }
    }

    // ── SetGratingDriftAngle ──────────────────────────────────────────────────

    pub fn cmd_set_grating_drift_angle(
        &mut self,
        handle: u32,
        cmd: proto::SetGratingDriftAngleRequest,
    ) -> proto::Response {
        match self.stimuli.get_mut(&handle) {
            None => err_not_found(handle),
            Some(Stimulus::Grating(s)) => {
                let deferred = self.deferred_mode;
                let prev = if deferred { s.params.copy } else { s.params.live };
                s.params.set(
                    deferred,
                    GratingParams { drift_angle: cmd.angle_deg, ..prev },
                );
                if !deferred {
                    s.flags.mark_dirty();
                }
                ok_ack()
            }
            Some(stim) => err_wrong_type(stim, "SetGratingDriftAngle"),
        }
    }

    // ── Query helper (called from command.rs cmd_query_stimulus) ──────────────

    pub fn grating_query_params(s: &GratingStimulus) -> proto::StimulusParams {
        let p = s.params.live;
        proto::StimulusParams {
            shape: Some(proto::stimulus_params::Shape::Grating(proto::GratingParams {
                width: s.size.live[0] * 2.0,
                height: s.size.live[1] * 2.0,
                sf: p.sf,
                phase: p.phase,
                contrast: p.contrast,
                waveform: waveform_to_proto(p.waveform) as i32,
                mask: mask_to_proto(p.mask) as i32,
                mask_param: p.mask_param,
                drift_speed: p.drift_speed,
                drift_decoupled: !p.drift_coupled,
                drift_angle: p.drift_angle,
            })),
        }
    }
}

// ── Module-private response helpers ───────────────────────────────────────────

fn ok_ack() -> proto::Response {
    proto::Response {
        handle: -1,
        code: proto::ErrorCode::Ok as i32,
        error: String::new(),
        body: None,
    }
}

fn ok_handle(h: u32) -> proto::Response {
    proto::Response {
        handle: h as i32,
        code: proto::ErrorCode::Ok as i32,
        error: String::new(),
        body: None,
    }
}

fn err_not_found(handle: u32) -> proto::Response {
    proto::Response {
        handle: 0,
        code: proto::ErrorCode::HandleNotFound as i32,
        error: format!("stimulus handle {} not found", handle),
        body: None,
    }
}

fn err_wrong_type(stim: &Stimulus, cmd: &str) -> proto::Response {
    proto::Response {
        handle: 0,
        code: proto::ErrorCode::WrongStimulusType as i32,
        error: format!("{} requires a Grating stimulus, got {}", cmd, stim.type_name()),
        body: None,
    }
}
