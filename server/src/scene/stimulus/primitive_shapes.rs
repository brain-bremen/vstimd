use super::super::deferred::Deferred;
use super::shape_appearance::ShapeAppearance;
use super::stimulus_flags::StimulusFlags;
use super::transform2d::Transform2D;

pub struct RectStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub appearance: Deferred<ShapeAppearance>,
    pub size: Deferred<[f32; 2]>, // [half_width, half_height]
}

impl RectStimulus {
    pub const TYPE_NAME: &'static str = "Rect";

    pub fn make_copy(&mut self) {
        self.flags.make_copy();
        self.transform.make_copy();
        self.appearance.make_copy();
        self.size.make_copy();
    }

    pub fn flip(&mut self) {
        self.flags.get_copy();
        self.flags.mark_dirty();
        self.transform.flip();
        self.appearance.flip();
        self.size.flip();
    }
}

pub struct EllipseStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub appearance: Deferred<ShapeAppearance>,
    pub radii: Deferred<[f32; 2]>, // [rx, ry]
}

impl EllipseStimulus {
    pub const TYPE_NAME: &'static str = "Ellipse";

    pub fn make_copy(&mut self) {
        self.flags.make_copy();
        self.transform.make_copy();
        self.appearance.make_copy();
        self.radii.make_copy();
    }

    pub fn flip(&mut self) {
        self.flags.get_copy();
        self.flags.mark_dirty();
        self.transform.flip();
        self.appearance.flip();
        self.radii.flip();
    }
}

pub struct DiscStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub appearance: Deferred<ShapeAppearance>,
    pub radius: Deferred<f32>,
}

impl DiscStimulus {
    pub const TYPE_NAME: &'static str = "Disc";

    pub fn make_copy(&mut self) {
        self.flags.make_copy();
        self.transform.make_copy();
        self.appearance.make_copy();
        self.radius.make_copy();
    }

    pub fn flip(&mut self) {
        self.flags.get_copy();
        self.flags.mark_dirty();
        self.transform.flip();
        self.appearance.flip();
        self.radius.flip();
    }
}
