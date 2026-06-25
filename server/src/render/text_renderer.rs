use crate::render::vk::{GlyphAtlas, VkContext, VkTextPipeline};
use crate::scene::stimulus::text::{TextFontSystem, TextSwashCache};

pub struct TextRenderer {
    pub glyph_atlas: GlyphAtlas,
    pub text_pipeline: VkTextPipeline,
    pub font_system: TextFontSystem,
    pub swash_cache: TextSwashCache,
}

impl TextRenderer {
    pub fn new(ctx: &VkContext) -> Self {
        let glyph_atlas = GlyphAtlas::new(&ctx.device, &ctx.instance, ctx.physical_device);
        let text_pipeline = VkTextPipeline::new(
            &ctx.device,
            ctx.render_pass,
            glyph_atlas.descriptor_set_layout,
        );
        Self {
            glyph_atlas,
            text_pipeline,
            font_system: TextFontSystem::new(),
            swash_cache: TextSwashCache::new(),
        }
    }

    pub(super) fn destroy(&mut self, device: &ash::Device) {
        self.text_pipeline.destroy(device);
        unsafe { self.glyph_atlas.destroy(device) };
    }
}
