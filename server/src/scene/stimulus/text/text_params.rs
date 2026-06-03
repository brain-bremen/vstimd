/// How the text box is anchored relative to `pos`.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Anchor {
    #[default]
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Bidi / shaping mode.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum LanguageStyle {
    #[default]
    Ltr,
    Rtl,
    Arabic,
}

/// Deferred-friendly parameters for text rendering (all `Copy`).
/// `fill_color.a == 0` → no background fill; `border_color.a == 0` → no border.
#[derive(Clone, Copy)]
pub struct TextRenderParams {
    pub color: [f32; 4],
    pub fill_color: [f32; 4],
    pub border_color: [f32; 4],
    pub flip_horiz: bool,
}

impl Default for TextRenderParams {
    fn default() -> Self {
        Self {
            color: [1.0, 1.0, 1.0, 1.0],
            fill_color: [0.0, 0.0, 0.0, 0.0],
            border_color: [0.0, 0.0, 0.0, 0.0],
            flip_horiz: false,
        }
    }
}
