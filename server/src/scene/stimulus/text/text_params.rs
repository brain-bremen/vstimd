/// How the text box is anchored relative to `pos`.
#[derive(Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Anchor {
    #[default]
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Bidi / shaping mode.
#[derive(Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LanguageStyle {
    #[default]
    Ltr,
    Rtl,
    Arabic,
}

/// Deferred-friendly parameters for text rendering (all `Copy`).
/// `fill_color.a == 0` → no background fill; `border_color.a == 0` → no border.
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct TextRenderParams {
    pub color: crate::Color,
    pub fill_color: crate::Color,
    pub border_color: crate::Color,
    pub flip_horiz: bool,
}

impl Default for TextRenderParams {
    fn default() -> Self {
        Self {
            color: crate::Color::WHITE,
            fill_color: crate::Color::TRANSPARENT,
            border_color: crate::Color::TRANSPARENT,
            flip_horiz: false,
        }
    }
}
