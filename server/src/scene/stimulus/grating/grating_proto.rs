use crate::proto;

use super::grating_params::{GratingMask, GratingParams, Waveform};
use super::grating_stimulus::GratingStimulus;

// ── Waveform conversions ──────────────────────────────────────────────────────

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

// ── Mask conversions ──────────────────────────────────────────────────────────

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

// ── GratingParams ↔ proto ─────────────────────────────────────────────────────

pub fn grating_params_from_proto(cmd: &proto::CreateGratingRequest) -> GratingParams {
    let sf       = if cmd.sf       == 0.0 { 0.05 } else { cmd.sf };
    let contrast = if cmd.contrast == 0.0 { 1.0  } else { cmd.contrast };
    let opacity  = if cmd.opacity  == 0.0 { 1.0  } else { cmd.opacity };
    let fore = cmd.fore_color.map_or([1.0, 1.0, 1.0, 1.0], |c| [c.r, c.g, c.b, c.a]);
    let back = cmd.back_color.map_or([0.0, 0.0, 0.0, 1.0], |c| [c.r, c.g, c.b, c.a]);
    GratingParams {
        sf,
        phase:        cmd.phase,
        contrast,
        waveform:     proto_to_waveform(cmd.waveform),
        mask:         proto_to_mask(cmd.mask),
        mask_param:   cmd.mask_param,
        drift_speed:  cmd.drift_speed,
        drift_coupled: !cmd.drift_decoupled,
        drift_angle:  cmd.drift_angle,
        fore_color:   fore,
        back_color:   back,
        opacity,
    }
}

// ── Query ─────────────────────────────────────────────────────────────────────

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
            fore_color: Some(proto::Color { r: p.fore_color[0], g: p.fore_color[1], b: p.fore_color[2], a: p.fore_color[3] }),
            back_color: Some(proto::Color { r: p.back_color[0], g: p.back_color[1], b: p.back_color[2], a: p.back_color[3] }),
            opacity: p.opacity,
        })),
    }
}
