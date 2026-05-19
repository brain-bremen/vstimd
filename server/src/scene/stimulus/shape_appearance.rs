#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum DrawMode {
    #[default]
    Fill,
    Stroke,
    FillAndStroke,
}

/// Fill / outline / stroke appearance for coloured shape stimuli.
#[derive(Clone, Copy)]
pub struct ShapeAppearance {
    pub fill_color: [f32; 4],    // RGBA
    pub outline_color: [f32; 4], // RGBA
    pub stroke_width: f32,
    pub draw_mode: DrawMode,
}

impl Default for ShapeAppearance {
    fn default() -> Self {
        Self {
            fill_color: [1.0, 1.0, 1.0, 1.0],
            outline_color: [0.0, 0.0, 0.0, 1.0],
            stroke_width: 2.0,
            draw_mode: DrawMode::Fill,
        }
    }
}
