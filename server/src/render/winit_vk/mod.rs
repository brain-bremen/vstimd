mod init;

use std::sync::{Arc, RwLock};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::render::overlay::build_overlay_ui;
use crate::render::vk::{
    EguiFrameData, GpuBuffers, VkContext, VkEguiRenderer, VkPipeline, render_frame,
};
use crate::scene::SceneState;
use crate::timing::{FrameStats, FrameTick};

// FIFO is the only present mode used throughout the application.
//
// FIFO makes vkAcquireNextImageKHR block until the presentation engine returns
// an image at vblank. The render loop therefore runs at exactly the display
// refresh rate by construction — the swapchain IS the clock. One acquire =
// one vblank = one frame on screen. This is the correct foundation for a
// stimulus server that must never miss or duplicate a frame.
//
// MAILBOX decouples the render loop from the display clock (GPU runs uncapped,
// frames overwrite each other) — the exact opposite of what we want. FIFO is
// guaranteed to be available on every Vulkan implementation.

// ── Window creation options ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMode {
    Fullscreen,
    Windowed { width: u32, height: u32 },
}

impl Default for WindowMode {
    fn default() -> Self {
        Self::Fullscreen
    }
}

// ── Per-window Vulkan state ───────────────────────────────────────────────────

struct State {
    // Rust drops fields in declaration order.
    // All display-system and Vulkan resources must be declared — and therefore
    // dropped — BEFORE `window`, because they hold references (surface handles,
    // wl_surface proxies, …) into the window that become dangling once the
    // window is destroyed.
    ctx: VkContext,
    pipeline: VkPipeline,
    gpu_buffers: GpuBuffers,
    egui_renderer: VkEguiRenderer,
    egui_winit: egui_winit::State, // holds display-handle references
    // ── Window comes after all borrowers ─────────────────────────────────────
    window: Arc<Window>,
    scene: Arc<RwLock<SceneState>>,
    frame_stats: FrameStats,
    frame_index: usize,
    egui_ctx: egui::Context,
    show_overlay: bool,
}

impl State {
    fn new(
        window: Arc<Window>,
        scene: Arc<RwLock<SceneState>>,
        event_loop: &ActiveEventLoop,
    ) -> Self {
        let ctx = init::init(&window);
        // FIFO is set by build_context and never changed — the swapchain is
        // the screen clock.
        eprintln!("wonderlamp: present mode: FIFO");

        let pipeline = VkPipeline::new(&ctx.device, ctx.render_pass);
        let gpu_buffers = GpuBuffers::new(&ctx.instance, ctx.physical_device);
        let egui_renderer = VkEguiRenderer::new(
            &ctx.device,
            &ctx.instance,
            ctx.physical_device,
            ctx.egui_render_pass,
        );

        let egui_ctx = egui::Context::default();
        let viewport_id = egui_ctx.viewport_id();
        let egui_winit = egui_winit::State::new(
            egui_ctx.clone(),
            viewport_id,
            event_loop,
            Some(window.scale_factor() as f32),
            None,       // theme: use default
            Some(4096), // max texture side
        );

        let hz = window
            .current_monitor()
            .and_then(|m| m.refresh_rate_millihertz())
            .map(|mhz| mhz as f64 / 1000.0)
            .unwrap_or(60.0);

        Self {
            window,
            ctx,
            pipeline,
            gpu_buffers,
            egui_renderer,
            scene,
            frame_stats: FrameStats::new(hz),
            frame_index: 0,
            egui_ctx,
            egui_winit,
            show_overlay: false,
        }
    }

    fn render(&mut self) {
        // Build the egui overlay if enabled.
        if self.show_overlay {
            let raw_input = self.egui_winit.take_egui_input(&self.window);
            let output = self.egui_ctx.run_ui(raw_input, |ctx| {
                build_overlay_ui(ctx, &self.scene, &self.frame_stats);
            });
            self.egui_winit
                .handle_platform_output(&self.window, output.platform_output);

            // Tessellate egui output
            let primitives = self
                .egui_ctx
                .tessellate(output.shapes, output.pixels_per_point);

            let data = EguiFrameData {
                textures_delta: &output.textures_delta,
                primitives: &primitives,
                pixels_per_point: output.pixels_per_point,
            };

            let tick = render_frame(
                &self.ctx,
                &self.pipeline,
                &mut self.gpu_buffers,
                &self.scene,
                &mut self.frame_index,
                &mut self.frame_stats,
                Some(&mut self.egui_renderer),
                Some(data),
            );
            self.handle_tick(tick);
        } else {
            let tick = render_frame(
                &self.ctx,
                &self.pipeline,
                &mut self.gpu_buffers,
                &self.scene,
                &mut self.frame_index,
                &mut self.frame_stats,
                None,
                None,
            );
            self.handle_tick(tick);
        }
    }

    fn handle_tick(&mut self, tick: Option<FrameTick>) {
        match tick {
            None => {
                // Swapchain out of date (resize, monitor change, etc.).
                let size = self.window.inner_size();
                let extent = ash::vk::Extent2D {
                    width: size.width.max(1),
                    height: size.height.max(1),
                };
                self.ctx.recreate_swapchain(extent);
            }
            Some(_tick) => {
                // Tick is available here for stimulus scheduling.
                // TODO: forward to scene / scheduler once that layer exists.
            }
        }
    }
}

// ── WinitApp — ApplicationHandler ────────────────────────────────────────────

pub struct WinitApp {
    scene: Option<Arc<RwLock<SceneState>>>,
    window_mode: WindowMode,
    state: Option<State>,
    modifiers: winit::event::Modifiers,
    is_fullscreen: bool,
}

impl WinitApp {
    pub fn new(scene: Arc<RwLock<SceneState>>, window_mode: WindowMode) -> Self {
        Self {
            scene: Some(scene),
            is_fullscreen: window_mode == WindowMode::Fullscreen,
            window_mode,
            state: None,
            modifiers: winit::event::Modifiers::default(),
        }
    }

    fn toggle_fullscreen(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            return;
        }

        if self.is_fullscreen {
            // ── Leaving fullscreen → windowed ─────────────────────────
            // Present mode is already FIFO and does not change.
            let state = self.state.as_ref().unwrap();
            state.window.set_fullscreen(None);
            if let WindowMode::Windowed { width, height } = self.window_mode {
                let _ = state
                    .window
                    .request_inner_size(winit::dpi::LogicalSize::new(width, height));
            }
            // `state` borrow ends here; safe to write the flag.
            self.is_fullscreen = false;
            eprintln!("wonderlamp: windowed — present mode: FIFO");
        } else {
            // ── Entering fullscreen ───────────────────────────────────────
            // Present mode stays FIFO throughout — the swapchain is the
            // screen clock and must not be decoupled from vblank.

            let monitor = {
                let s = self.state.as_ref().unwrap();
                s.window
                    .current_monitor()
                    .or_else(|| event_loop.primary_monitor())
                    .or_else(|| event_loop.available_monitors().next())
            };

            let state = self.state.as_ref().unwrap();
            state
                .window
                .set_fullscreen(Some(Fullscreen::Borderless(monitor)));
            self.is_fullscreen = true;
            eprintln!("wonderlamp: fullscreen — present mode: FIFO");
        }
    }
}

impl ApplicationHandler for WinitApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            let attrs = match self.window_mode {
                WindowMode::Fullscreen => {
                    let monitor = event_loop
                        .primary_monitor()
                        .or_else(|| event_loop.available_monitors().next());
                    Window::default_attributes()
                        .with_title("Wonderlamp")
                        .with_fullscreen(Some(Fullscreen::Borderless(monitor)))
                }
                WindowMode::Windowed { width, height } => Window::default_attributes()
                    .with_title("Wonderlamp")
                    .with_inner_size(winit::dpi::LogicalSize::new(width, height))
                    .with_resizable(true),
            };
            let window = Arc::new(event_loop.create_window(attrs).unwrap());
            let scene = self.scene.take().expect("scene already consumed");
            self.state = Some(State::new(window, scene, event_loop));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Forward to egui first.
        if let Some(state) = &mut self.state {
            let response = state.egui_winit.on_window_event(&state.window, &event);
            if response.consumed {
                return;
            }
        }

        match &event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(state) = &mut self.state {
                    let extent = ash::vk::Extent2D {
                        width: size.width.max(1),
                        height: size.height.max(1),
                    };
                    state.ctx.recreate_swapchain(extent);
                }
            }
            WindowEvent::ModifiersChanged(mods) => self.modifiers = *mods,
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key: PhysicalKey::Code(key),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => match key {
                KeyCode::Escape => event_loop.exit(),
                KeyCode::F1 => {
                    if let Some(state) = &mut self.state {
                        state.show_overlay = !state.show_overlay;
                    }
                }
                KeyCode::F2 => {
                    if let Some(state) = &self.state {
                        let mut sc = state.scene.write().expect("scene lock");
                        sc.photodiode.enabled = !sc.photodiode.enabled;
                        sc.photodiode.flicker = true;
                        sc.photodiode.lit = false;
                    }
                }
                KeyCode::KeyD => {
                    if let Some(state) = &self.state {
                        crate::render::spawn_demo_stimuli(&state.scene);
                    }
                }
                KeyCode::F11 => self.toggle_fullscreen(event_loop),
                KeyCode::Enter if self.modifiers.state().alt_key() => {
                    self.toggle_fullscreen(event_loop);
                }
                _ => {}
            },
            WindowEvent::RedrawRequested => {
                if let Some(state) = &mut self.state {
                    state.render();
                    state.window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

