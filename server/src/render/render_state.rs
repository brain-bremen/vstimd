use crate::render::scene_renderer::SceneRenderer;
use crate::render::text_renderer::TextRenderer;
use crate::render::ui_renderer::UiRenderer;
use crate::timing::FrameTiming;

/// Vulkan rendering resources shared between the DRM and winit backends.
///
/// Backend-specific state (input devices, window handle, vblank clock source,
/// display geometry, local IP, log buffer, system metrics) lives in the
/// embedding backend struct alongside this.
///
/// Field declaration order matters for `Drop`: `ctx` is declared last so
/// Rust's automatic drop (which fires after our explicit `drop()`) frees
/// all GPU sub-struct handles before the device itself is destroyed.
pub struct RenderState {
    pub scene_renderer: SceneRenderer,
    pub text: TextRenderer,
    pub ui: Option<UiRenderer>,
    pub timing: FrameTiming,
    pub ctx: crate::render::vk::VkContext,
}

impl Drop for RenderState {
    fn drop(&mut self) {
        if let Some(ref mut ui) = self.ui {
            ui.destroy(&self.ctx.device);
        }
        self.text.destroy(&self.ctx.device);
        self.scene_renderer.destroy(&self.ctx.device);
    }
}
