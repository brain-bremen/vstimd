use std::path::PathBuf;

use crate::log_buffer::LogBuffer;
use crate::render::overlay_ui::OverlayState;
use crate::render::system_metrics::MetricsSampler;
use crate::render::vk::{VkContext, VkEguiRenderer};

/// egui overlay state — renderer, context, and overlay-specific data.
/// Wrapped in `Option` in `RenderState` so the overlay can be entirely absent.
pub struct UiRenderer {
    pub egui_renderer: VkEguiRenderer,
    pub egui_ctx: egui::Context,
    /// Grouped-window visibility, focus, and owned dialogs.
    pub overlay: OverlayState,
    pub metrics: MetricsSampler,
    pub log_buffer: LogBuffer,
}

impl UiRenderer {
    pub fn new(ctx: &VkContext, config_dir: PathBuf, log_buffer: LogBuffer) -> Self {
        let egui_renderer = VkEguiRenderer::new(
            &ctx.device,
            &ctx.instance,
            ctx.physical_device,
            ctx.egui_render_pass,
        );
        Self {
            egui_renderer,
            egui_ctx: egui::Context::default(),
            overlay: OverlayState::new(config_dir),
            metrics: MetricsSampler::new(),
            log_buffer,
        }
    }

    pub(super) fn destroy(&mut self, device: &ash::Device) {
        self.egui_renderer.destroy(device);
    }
}
