use super::super::deferred::Deferred;
use super::primitive_shapes::{CircleStimulus, EllipseStimulus, RectStimulus};
use super::shape_appearance::ShapeAppearance;
use super::stimulus_flags::StimulusFlags;
use super::transform2d::Transform2D;

macro_rules! shape_field {
    ($stim:expr, |$s:ident| $expr:expr) => {
        match $stim {
            ShapeStimulus::Rect($s)    => $expr,
            ShapeStimulus::Ellipse($s) => $expr,
            ShapeStimulus::Circle($s)    => $expr,
        }
    };
}

pub enum ShapeStimulus {
    Rect(RectStimulus),
    Ellipse(EllipseStimulus),
    Circle(CircleStimulus),
}

impl ShapeStimulus {
    pub fn flags(&self) -> &StimulusFlags {
        shape_field!(self, |s| &s.flags)
    }

    pub fn flags_mut(&mut self) -> &mut StimulusFlags {
        shape_field!(self, |s| &mut s.flags)
    }

    pub fn transform(&self) -> &Deferred<Transform2D> {
        shape_field!(self, |s| &s.transform)
    }

    pub fn transform_mut(&mut self) -> &mut Deferred<Transform2D> {
        shape_field!(self, |s| &mut s.transform)
    }

    pub fn appearance(&self) -> &Deferred<ShapeAppearance> {
        shape_field!(self, |s| &s.appearance)
    }

    pub fn appearance_mut(&mut self) -> &mut Deferred<ShapeAppearance> {
        shape_field!(self, |s| &mut s.appearance)
    }

    pub fn make_copy(&mut self) {
        shape_field!(self, |s| s.make_copy())
    }

    pub fn flip(&mut self) {
        shape_field!(self, |s| s.flip())
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            ShapeStimulus::Rect(_)    => RectStimulus::TYPE_NAME,
            ShapeStimulus::Ellipse(_) => EllipseStimulus::TYPE_NAME,
            ShapeStimulus::Circle(_)    => CircleStimulus::TYPE_NAME,
        }
    }
}
