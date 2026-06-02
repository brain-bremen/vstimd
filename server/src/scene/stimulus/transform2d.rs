use lyon_tessellation::math::{Angle, Transform, Vector};

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
    /// Rotation-then-translation affine transform for tessellation.
    pub fn to_transform(&self) -> Transform {
        Transform::rotation(Angle::degrees(self.angle))
            .then_translate(Vector::new(self.pos[0], self.pos[1]))
    }
}
