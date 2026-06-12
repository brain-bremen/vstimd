/// Linear RGBA color, channels in `[0.0, 1.0]`.
///
/// Serializes as a 4-element JSON array `[r, g, b, a]` to match the wire format.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(from = "[f32; 4]", into = "[f32; 4]")]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const WHITE:       Self = Self::new(1.0, 1.0, 1.0, 1.0);
    pub const BLACK:       Self = Self::new(0.0, 0.0, 0.0, 1.0);
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);
}

impl From<[f32; 4]> for Color {
    fn from([r, g, b, a]: [f32; 4]) -> Self {
        Self { r, g, b, a }
    }
}

impl From<Color> for [f32; 4] {
    fn from(c: Color) -> Self {
        [c.r, c.g, c.b, c.a]
    }
}

impl From<crate::proto::Color> for Color {
    fn from(c: crate::proto::Color) -> Self {
        Self { r: c.r, g: c.g, b: c.b, a: c.a }
    }
}

impl From<Color> for crate::proto::Color {
    fn from(c: Color) -> Self {
        Self { r: c.r, g: c.g, b: c.b, a: c.a }
    }
}
