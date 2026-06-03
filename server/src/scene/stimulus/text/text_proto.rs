use crate::proto;

use super::text_params::{Anchor, LanguageStyle, TextRenderParams};
use super::text_stimulus::TextStimulus;

// ── Anchor ────────────────────────────────────────────────────────────────────

pub fn anchor_from_str(s: &str) -> Anchor {
    match s {
        "top-left"     => Anchor::TopLeft,
        "top-right"    => Anchor::TopRight,
        "bottom-left"  => Anchor::BottomLeft,
        "bottom-right" => Anchor::BottomRight,
        _              => Anchor::Center,
    }
}

pub fn anchor_to_str(a: Anchor) -> &'static str {
    match a {
        Anchor::Center      => "center",
        Anchor::TopLeft     => "top-left",
        Anchor::TopRight    => "top-right",
        Anchor::BottomLeft  => "bottom-left",
        Anchor::BottomRight => "bottom-right",
    }
}

// ── LanguageStyle ─────────────────────────────────────────────────────────────

pub fn proto_to_language_style(v: i32) -> LanguageStyle {
    match proto::LanguageStyle::try_from(v).unwrap_or(proto::LanguageStyle::Unspecified) {
        proto::LanguageStyle::Rtl    => LanguageStyle::Rtl,
        proto::LanguageStyle::Arabic => LanguageStyle::Arabic,
        _                            => LanguageStyle::Ltr,
    }
}

pub fn language_style_to_proto(ls: LanguageStyle) -> i32 {
    match ls {
        LanguageStyle::Ltr    => proto::LanguageStyle::Ltr as i32,
        LanguageStyle::Rtl    => proto::LanguageStyle::Rtl as i32,
        LanguageStyle::Arabic => proto::LanguageStyle::Arabic as i32,
    }
}

// ── CreateTextRequest → scene types ───────────────────────────────────────────

pub fn text_render_params_from_proto(cmd: &proto::CreateTextRequest) -> TextRenderParams {
    let color = cmd.color.as_ref()
        .map(|c| [c.r, c.g, c.b, c.a])
        .unwrap_or([1.0, 1.0, 1.0, 1.0]);
    let fill_color = cmd.fill_color.as_ref()
        .map(|c| [c.r, c.g, c.b, c.a])
        .unwrap_or([0.0, 0.0, 0.0, 0.0]);
    let border_color = cmd.border_color.as_ref()
        .map(|c| [c.r, c.g, c.b, c.a])
        .unwrap_or([0.0, 0.0, 0.0, 0.0]);
    TextRenderParams {
        color,
        fill_color,
        border_color,
        flip_horiz: cmd.flip_horiz,
    }
}

// ── Scene → QueryStimulusResponse payload ────────────────────────────────────

pub fn text_query_params(s: &TextStimulus) -> proto::StimulusParams {
    let p = &s.params.live;
    proto::StimulusParams {
        shape: Some(proto::stimulus_params::Shape::Text(proto::TextParams {
            text:          s.text_live.clone(),
            font:          s.font_family.clone(),
            letter_height: s.letter_height_px,
            size: Some(proto::Vec2 {
                x: s.box_size.live[0],
                y: s.box_size.live[1],
            }),
            anchor: anchor_to_str(s.anchor).to_string(),
            fill_color: Some(proto::Color {
                r: p.fill_color[0], g: p.fill_color[1],
                b: p.fill_color[2], a: p.fill_color[3],
            }),
            border_color: Some(proto::Color {
                r: p.border_color[0], g: p.border_color[1],
                b: p.border_color[2], a: p.border_color[3],
            }),
            flip_horiz:     p.flip_horiz,
            language_style: language_style_to_proto(s.language_style),
        })),
    }
}
