use crate::scene::deferred::Deferred;
use crate::scene::stimulus::{StimulusFlags, Transform2D};

use super::text_params::{Anchor, LanguageStyle, TextRenderParams};

/// Serializable text configuration.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TextConfig {
    pub flags:            StimulusFlags,
    pub transform:        Deferred<Transform2D>,
    pub params:           Deferred<TextRenderParams>,
    pub box_size:         Deferred<[f32; 2]>,
    pub text_live:        String,
    // These never change post-creation (would require full re-layout).
    pub font_family:      String,
    pub letter_height_px: f32,
    pub anchor:           Anchor,
    pub language_style:   LanguageStyle,
}

/// Full text stimulus: serializable config + deferred text copy.
/// Deref/DerefMut give transparent access to the config fields.
#[derive(Clone)]
pub struct TextStimulus {
    pub config:    TextConfig,
    // String is not Copy, so live/copy are managed manually. Not serialized;
    // restored equal to text_live on deserialization.
    pub text_copy: String,
}

impl std::ops::Deref for TextStimulus {
    type Target = TextConfig;
    fn deref(&self) -> &TextConfig { &self.config }
}

impl std::ops::DerefMut for TextStimulus {
    fn deref_mut(&mut self) -> &mut TextConfig { &mut self.config }
}

impl serde::Serialize for TextStimulus {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.config.serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for TextStimulus {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let config = TextConfig::deserialize(d)?;
        let text_copy = config.text_live.clone();
        Ok(Self { config, text_copy })
    }
}

impl TextStimulus {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pos: [f32; 2],
        box_size: [f32; 2],
        text: String,
        font_family: String,
        letter_height_px: f32,
        anchor: Anchor,
        language_style: LanguageStyle,
        params: TextRenderParams,
    ) -> Self {
        let text_copy = text.clone();
        Self {
            config: TextConfig {
                flags:            StimulusFlags::enabled(true),
                transform:        Deferred::new(Transform2D { pos, angle: 0.0 }),
                params:           Deferred::new(params),
                box_size:         Deferred::new(box_size),
                text_live:        text,
                font_family,
                letter_height_px,
                anchor,
                language_style,
            },
            text_copy,
        }
    }

    pub fn set_text(&mut self, deferred: bool, text: String) {
        if deferred {
            self.text_copy = text;
        } else {
            self.text_live = text.clone();
            self.text_copy = text;
            self.flags.mark_dirty();
        }
    }

    pub fn set_color(&mut self, deferred: bool, color: [f32; 4]) {
        let prev = if deferred { self.params.copy } else { self.params.live };
        self.params.set(deferred, TextRenderParams { color, ..prev });
        if !deferred {
            self.flags.mark_dirty();
        }
    }

    pub fn make_copy(&mut self) {
        self.flags.make_copy();
        self.transform.make_copy();
        self.params.make_copy();
        self.box_size.make_copy();
        self.text_copy = self.text_live.clone();
    }

    pub fn flip(&mut self) {
        self.flags.get_copy();
        self.flags.mark_dirty();
        self.transform.flip();
        self.params.flip();
        self.box_size.flip();
        self.text_live = self.text_copy.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_stim() -> TextStimulus {
        TextStimulus::new(
            [0.0, 0.0],
            [200.0, 100.0],
            "hello".into(),
            "Open Sans".into(),
            32.0,
            Anchor::default(),
            LanguageStyle::default(),
            TextRenderParams::default(),
        )
    }

    #[test]
    fn set_text_immediate_updates_live_and_copy() {
        let mut s = default_stim();
        s.set_text(false, "world".into());
        assert_eq!(s.text_live, "world");
        assert_eq!(s.text_copy, "world");
        assert!(s.flags.dirty);
    }

    #[test]
    fn set_text_deferred_only_updates_copy() {
        let mut s = default_stim();
        s.flags.dirty = false;
        s.set_text(true, "world".into());
        assert_eq!(s.text_live, "hello");
        assert_eq!(s.text_copy, "world");
        assert!(!s.flags.dirty);
    }

    #[test]
    fn flip_promotes_text_to_live() {
        let mut s = default_stim();
        s.text_copy = "flipped".into();
        s.flip();
        assert_eq!(s.text_live, "flipped");
    }

    #[test]
    fn make_copy_snapshots_live_text() {
        let mut s = default_stim();
        s.text_live = "current".into();
        s.text_copy = "stale".into();
        s.make_copy();
        assert_eq!(s.text_copy, "current");
    }

    #[test]
    fn set_color_immediate() {
        let mut s = default_stim();
        s.set_color(false, [1.0, 0.0, 0.0, 0.5]);
        assert_eq!(s.params.live.color, [1.0, 0.0, 0.0, 0.5]);
        assert!(s.flags.dirty);
    }

    #[test]
    fn set_color_deferred_leaves_live_unchanged() {
        let mut s = default_stim();
        s.flags.dirty = false;
        s.set_color(true, [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(s.params.live.color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(s.params.copy.color, [0.0, 1.0, 0.0, 1.0]);
        assert!(!s.flags.dirty);
    }

    #[test]
    fn set_color_deferred_then_flip() {
        let mut s = default_stim();
        s.set_color(true, [0.5, 0.5, 0.5, 0.8]);
        s.flip();
        assert_eq!(s.params.live.color, [0.5, 0.5, 0.5, 0.8]);
    }

    #[test]
    fn default_params_white_text_transparent_fill() {
        let s = default_stim();
        assert_eq!(s.params.live.color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(s.params.live.fill_color[3], 0.0);
        assert_eq!(s.params.live.border_color[3], 0.0);
    }
}
