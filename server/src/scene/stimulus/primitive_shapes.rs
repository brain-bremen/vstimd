use super::super::deferred::Deferred;
use super::shape_appearance::ShapeAppearance;
use super::stimulus_flags::StimulusFlags;
use super::transform2d::Transform2D;

/// Config parameters shared by every shape stimulus (rect / ellipse / circle).
/// Flattened into each shape's serialization so the config JSON stays flat while
/// the sharing is explicit in Rust and reusable in the JSON Schema.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ShapeCommon {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub appearance: Deferred<ShapeAppearance>,
}

impl ShapeCommon {
    pub fn make_copy(&mut self) {
        self.flags.make_copy();
        self.transform.make_copy();
        self.appearance.make_copy();
    }

    pub fn flip(&mut self) {
        self.flags.get_copy();
        self.flags.mark_dirty();
        self.transform.flip();
        self.appearance.flip();
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct RectStimulus {
    #[serde(flatten)]
    pub common: ShapeCommon,
    pub size: Deferred<[f32; 2]>, // [half_width, half_height]
}

impl RectStimulus {
    pub const TYPE_NAME: &'static str = "Rect";

    pub fn make_copy(&mut self) {
        self.common.make_copy();
        self.size.make_copy();
    }

    pub fn flip(&mut self) {
        self.common.flip();
        self.size.flip();
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct EllipseStimulus {
    #[serde(flatten)]
    pub common: ShapeCommon,
    pub radii: Deferred<[f32; 2]>, // [rx, ry]
}

impl EllipseStimulus {
    pub const TYPE_NAME: &'static str = "Ellipse";

    pub fn make_copy(&mut self) {
        self.common.make_copy();
        self.radii.make_copy();
    }

    pub fn flip(&mut self) {
        self.common.flip();
        self.radii.flip();
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct CircleStimulus {
    #[serde(flatten)]
    pub common: ShapeCommon,
    pub radius: Deferred<f32>,
}

impl CircleStimulus {
    pub const TYPE_NAME: &'static str = "Circle";

    pub fn make_copy(&mut self) {
        self.common.make_copy();
        self.radius.make_copy();
    }

    pub fn flip(&mut self) {
        self.common.flip();
        self.radius.flip();
    }
}
