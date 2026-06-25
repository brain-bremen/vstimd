use std::path::PathBuf;

use crate::render::benchmark::BenchmarkState;
use crate::render::file_browser::FileBrowser;
use crate::render::system_info::query_hostname;
use crate::render::vk::{VkContext, VkEguiRenderer};

/// egui overlay state — renderer, context, and overlay-specific data.
/// Wrapped in `Option` in `RenderState` so the overlay can be entirely absent.
pub struct UiRenderer {
    pub egui_renderer: VkEguiRenderer,
    pub egui_ctx: egui::Context,
    pub show_overlay: bool,
    pub benchmark: BenchmarkState,
    pub file_browser: FileBrowser,
    pub hostname: String,
}

impl UiRenderer {
    pub fn new(ctx: &VkContext, config_dir: PathBuf) -> Self {
        let egui_renderer = VkEguiRenderer::new(
            &ctx.device,
            &ctx.instance,
            ctx.physical_device,
            ctx.egui_render_pass,
        );
        Self {
            egui_renderer,
            egui_ctx: egui::Context::default(),
            show_overlay: false,
            benchmark: BenchmarkState::new(),
            file_browser: FileBrowser::new(config_dir),
            hostname: query_hostname(),
        }
    }

    pub(super) fn destroy(&mut self, device: &ash::Device) {
        self.egui_renderer.destroy(device);
    }
}
