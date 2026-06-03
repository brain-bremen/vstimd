// All cosmic_text imports are confined to this file.
use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent};

use super::text_params::{Anchor, LanguageStyle};
use super::text_stimulus::TextStimulus;

// ── Fonts ─────────────────────────────────────────────────────────────────────
// Reuse the fonts already bundled by egui — no extra binary weight.
// Family names as registered in the TTF metadata:
//   "Ubuntu Light"  — proportional sans-serif (default)
//   "Hack"          — monospace

pub const DEFAULT_FONT_FAMILY: &str = "Ubuntu Light";

// ── Public wrapper types (no cosmic_text in their public signatures) ───────────

/// Wraps `cosmic_text::FontSystem`. Owns the font database.
/// Held by `RenderState`; callers never need to import cosmic_text.
pub struct TextFontSystem {
    pub(super) inner: FontSystem,
}

/// Wraps `cosmic_text::SwashCache`. Rasterizes glyphs on demand.
/// Held alongside `TextFontSystem` in `RenderState`.
pub struct TextSwashCache {
    pub(super) inner: SwashCache,
}

impl TextFontSystem {
    /// Creates a font system loaded with the fonts already bundled by egui
    /// (`Ubuntu Light` and `Hack`). No additional binary weight.
    /// System fonts are excluded for reproducibility on embedded targets.
    pub fn new() -> Self {
        let mut db = cosmic_text::fontdb::Database::new();
        db.load_font_data(epaint_default_fonts::UBUNTU_LIGHT.to_vec());
        db.load_font_data(epaint_default_fonts::HACK_REGULAR.to_vec());
        let inner = FontSystem::new_with_locale_and_db("en-US".into(), db);
        Self { inner }
    }
}

impl TextSwashCache {
    pub fn new() -> Self {
        Self { inner: SwashCache::new() }
    }
}

impl Default for TextFontSystem { fn default() -> Self { Self::new() } }
impl Default for TextSwashCache { fn default() -> Self { Self::new() } }

// ── GlyphKey ─────────────────────────────────────────────────────────────────

/// Opaque key identifying a unique rasterized glyph.
/// Wraps `cosmic_text::CacheKey`; defined here so other modules can use it as
/// a HashMap key without importing cosmic_text.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GlyphKey(pub(super) cosmic_text::CacheKey);

// ── LaidOutGlyph ─────────────────────────────────────────────────────────────

/// One rasterized glyph with its screen position.
/// Produced by `layout_and_rasterize`; consumed by the glyph atlas (step 4).
pub struct LaidOutGlyph {
    /// Unique key for atlas caching.
    pub key: GlyphKey,
    /// Top-left of the glyph bitmap in screen pixels, Y-down
    /// (origin = screen top-left, X right, Y down).
    pub screen_x: f32,
    pub screen_y: f32,
    /// R8_UNORM alpha mask, row-major.
    pub bitmap: Vec<u8>,
    pub bitmap_width: u32,
    pub bitmap_height: u32,
}

// ── Layout + rasterize ────────────────────────────────────────────────────────

/// Lay out and rasterize all glyphs in a text stimulus.
///
/// Returns one `LaidOutGlyph` per visible, alpha-mask glyph (colour emoji are
/// skipped). Screen positions are in Y-down pixel space with the origin at the
/// top-left corner of the screen.
///
/// Call this whenever `stim.flags.dirty` is set. Pass the resulting slice to
/// the glyph atlas to upload new bitmaps and obtain atlas UV coordinates.
pub fn layout_and_rasterize(
    stim: &TextStimulus,
    screen_w: f32,
    screen_h: f32,
    font_system: &mut TextFontSystem,
    swash_cache: &mut TextSwashCache,
) -> Vec<LaidOutGlyph> {
    let pos = stim.transform.live.pos;
    let [box_w, box_h] = stim.box_size.live;
    let size_px = stim.letter_height_px.max(1.0);
    let line_height = size_px * 1.25;

    // Compute text-box top-left in Y-up screen coordinates, then convert to
    // Y-down pixel coordinates for the cosmic-text buffer.
    let (tl_x_up, tl_y_up) = anchor_top_left(pos, [box_w, box_h], stim.anchor);
    let box_origin_x = tl_x_up + screen_w * 0.5;  // Y-down: right of center
    let box_origin_y = screen_h * 0.5 - tl_y_up;  // Y-down: below center

    // ── Phase 1: lay out text ─────────────────────────────────────────────────
    // `Buffer` borrows font_system only during set_*/shape calls; layout_runs()
    // operates on the buffer's cached result.
    let metrics = Metrics::new(size_px, line_height);
    let mut buffer = Buffer::new(&mut font_system.inner, metrics);
    buffer.set_size(&mut font_system.inner, Some(box_w), Some(box_h));

    let font_name: &str = if stim.font_family.is_empty() { DEFAULT_FONT_FAMILY } else { &stim.font_family };
    let attrs = Attrs::new().family(Family::Name(font_name));
    let shaping = match stim.language_style {
        LanguageStyle::Arabic | LanguageStyle::Rtl => Shaping::Advanced,
        _ => Shaping::Basic,
    };
    buffer.set_text(&mut font_system.inner, &stim.text_live, attrs, shaping);
    buffer.shape_until_scroll(&mut font_system.inner, false);

    // Collect (cache_key, buffer-relative position) for each glyph.
    // We need to free the `buffer` borrow of `font_system` before calling
    // `swash_cache.get_image`, which also needs `&mut font_system`.
    // In cosmic-text 0.12, call `glyph.physical(offset, scale)` to get a
    // `PhysicalGlyph` that carries a `CacheKey` and integer pixel position.
    struct PreGlyph {
        cache_key: cosmic_text::CacheKey,
        // Integer pixel position within the buffer (Y-down).
        px: i32,
        py: i32,
    }

    let pre: Vec<PreGlyph> = buffer
        .layout_runs()
        .flat_map(|run| {
            run.glyphs.iter().map(|g| {
                let phys = g.physical((0.0, 0.0), 1.0);
                PreGlyph { cache_key: phys.cache_key, px: phys.x, py: phys.y }
            }).collect::<Vec<_>>()
        })
        .collect();

    drop(buffer); // release font_system borrow

    // ── Phase 2: rasterize each glyph ─────────────────────────────────────────
    let mut out = Vec::with_capacity(pre.len());

    for pg in &pre {
        let Some(image) = swash_cache.inner.get_image(&mut font_system.inner, pg.cache_key) else {
            continue;
        };

        if !matches!(image.content, SwashContent::Mask) {
            continue; // skip colour emoji
        }
        if image.placement.width == 0 || image.placement.height == 0 {
            continue; // whitespace / zero-size glyph
        }

        // PhysicalGlyph.x/y is the advance position (baseline anchor).
        // placement.left/top are signed offsets from that anchor to bitmap top-left.
        let screen_x = box_origin_x + pg.px as f32 + image.placement.left as f32;
        let screen_y = box_origin_y + pg.py as f32 - image.placement.top as f32;

        out.push(LaidOutGlyph {
            key: GlyphKey(pg.cache_key),
            screen_x,
            screen_y,
            bitmap: image.data.to_vec(),
            bitmap_width: image.placement.width,
            bitmap_height: image.placement.height,
        });
    }

    out
}

// ── Coordinate helpers ────────────────────────────────────────────────────────

/// Returns the Y-up screen coordinates of the top-left corner of the text box,
/// given the anchor point position and box dimensions.
fn anchor_top_left(pos: [f32; 2], size: [f32; 2], anchor: Anchor) -> (f32, f32) {
    let [cx, cy] = pos;
    let [w, h] = size;
    match anchor {
        Anchor::Center      => (cx - w * 0.5, cy + h * 0.5),
        Anchor::TopLeft     => (cx,       cy),
        Anchor::TopRight    => (cx - w,   cy),
        Anchor::BottomLeft  => (cx,       cy + h),
        Anchor::BottomRight => (cx - w,   cy + h),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::stimulus::text::text_params::TextRenderParams;

    fn make_stim(text: &str) -> TextStimulus {
        TextStimulus::new(
            [0.0, 0.0],
            [400.0, 100.0],
            text.into(),
            "".into(), // empty → DEFAULT_FONT_FAMILY
            32.0,
            Anchor::Center,
            LanguageStyle::Ltr,
            TextRenderParams::default(),
        )
    }

    #[test]
    fn layout_produces_glyphs_for_ascii_text() {
        let mut fs = TextFontSystem::new();
        let mut sc = TextSwashCache::new();
        let glyphs = layout_and_rasterize(&make_stim("Hi"), 800.0, 600.0, &mut fs, &mut sc);
        assert!(!glyphs.is_empty(), "expected at least one glyph for 'Hi'");
    }

    #[test]
    fn layout_glyph_bitmaps_are_nonempty() {
        let mut fs = TextFontSystem::new();
        let mut sc = TextSwashCache::new();
        let glyphs = layout_and_rasterize(&make_stim("A"), 800.0, 600.0, &mut fs, &mut sc);
        for g in &glyphs {
            assert_eq!(
                g.bitmap.len(),
                (g.bitmap_width * g.bitmap_height) as usize,
                "bitmap dimensions mismatch"
            );
            assert!(!g.bitmap.iter().all(|&b| b == 0), "glyph bitmap should not be all zeros");
        }
    }

    #[test]
    fn layout_glyph_keys_differ_for_different_glyphs() {
        let mut fs = TextFontSystem::new();
        let mut sc = TextSwashCache::new();
        let glyphs = layout_and_rasterize(&make_stim("AB"), 800.0, 600.0, &mut fs, &mut sc);
        assert!(glyphs.len() >= 2);
        assert_ne!(glyphs[0].key, glyphs[1].key, "A and B should have different keys");
    }

    #[test]
    fn layout_positions_advance_left_to_right() {
        let mut fs = TextFontSystem::new();
        let mut sc = TextSwashCache::new();
        let glyphs = layout_and_rasterize(&make_stim("Hi"), 800.0, 600.0, &mut fs, &mut sc);
        assert!(glyphs.len() >= 2);
        assert!(
            glyphs[0].screen_x < glyphs[1].screen_x,
            "H should be left of i: {} vs {}",
            glyphs[0].screen_x, glyphs[1].screen_x,
        );
    }

    #[test]
    fn empty_text_produces_no_glyphs() {
        let mut fs = TextFontSystem::new();
        let mut sc = TextSwashCache::new();
        let glyphs = layout_and_rasterize(&make_stim(""), 800.0, 600.0, &mut fs, &mut sc);
        assert!(glyphs.is_empty());
    }

    #[test]
    fn anchor_center_positions_box_around_origin() {
        let mut fs = TextFontSystem::new();
        let mut sc = TextSwashCache::new();
        let glyphs = layout_and_rasterize(&make_stim("X"), 800.0, 600.0, &mut fs, &mut sc);
        // Box 400×100 centered at (0,0). Top-left in Y-down pixels: (200, 250).
        if let Some(g) = glyphs.first() {
            assert!(g.screen_x > 150.0 && g.screen_x < 450.0,
                "screen_x out of box: {}", g.screen_x);
            assert!(g.screen_y > 200.0 && g.screen_y < 400.0,
                "screen_y out of box: {}", g.screen_y);
        }
    }

    #[test]
    fn hack_monospace_font_resolves() {
        let mut fs = TextFontSystem::new();
        let mut sc = TextSwashCache::new();
        let stim = TextStimulus::new(
            [0.0, 0.0], [400.0, 100.0],
            "Hello".into(), "Hack".into(), 32.0,
            Anchor::Center, LanguageStyle::Ltr,
            TextRenderParams::default(),
        );
        let glyphs = layout_and_rasterize(&stim, 800.0, 600.0, &mut fs, &mut sc);
        assert!(!glyphs.is_empty(), "Hack font should resolve and produce glyphs");
    }
}
