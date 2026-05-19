/// 2-D placement. Used by every positional stimulus.
/// Position is in stimulus-space pixels with origin at screen centre, Y-up.
/// Rotation angle is counter-clockwise degrees.
#[derive(Clone, Copy)]
pub struct Transform2D {
    pub pos: [f32; 2],
    pub angle: f32, // ccw degrees, 0 = right, 90 = up
}

impl Default for Transform2D {
    fn default() -> Self {
        Self { pos: [0.0, 0.0], angle: 0.0 }
    }
}

impl Transform2D {
    /// Convert to a `kurbo::Affine` for tessellation (applies rotation then translation).
    pub fn to_affine(&self) -> kurbo::Affine {
        kurbo::Affine::translate((self.pos[0] as f64, self.pos[1] as f64))
            * kurbo::Affine::rotate(self.angle.to_radians() as f64)
    }
}
