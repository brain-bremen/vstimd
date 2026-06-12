use crate::Color;

#[derive(Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DrawMode {
    #[default]
    Fill,
    Stroke,
    FillAndStroke,
}

/// Fill / outline / stroke appearance for coloured shape stimuli.
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct ShapeAppearance {
    pub fill_color: Color,
    pub outline_color: Color,
    pub stroke_width: f32,
    pub draw_mode: DrawMode,
}

impl Default for ShapeAppearance {
    fn default() -> Self {
        Self {
            fill_color: Color::WHITE,
            outline_color: Color::BLACK,
            stroke_width: 2.0,
            draw_mode: DrawMode::Fill,
        }
    }
}
