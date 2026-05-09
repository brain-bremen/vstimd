mod init;

use std::sync::{Arc, RwLock};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::log_buffer::LogBuffer;
use crate::render::overlay::build_overlay_ui;
use crate::render::{RenderTarget, StimulusDisplayInfo, SystemInfo, WindowMode, query_local_ip};
use crate::render::vk::{
    EguiFrameData, GpuBuffers, VkContext, VkEguiRenderer, VkPipeline, render_frame,
};
use crate::timing::FramePhases;
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
    last_phases: FramePhases,
    frame_index: usize,
    egui_ctx: egui::Context,
    show_overlay: bool,
    refresh_hz: f64,
    window_mode: WindowMode,
    local_ip: String,
    log_buffer: LogBuffer,
}

impl Drop for State {
    fn drop(&mut self) {
        self.egui_renderer.destroy(&self.ctx.device);
        self.gpu_buffers.destroy_all(&self.ctx.device);
        self.pipeline.destroy(&self.ctx.device);
    }
}

impl State {
    fn new(
        window: Arc<Window>,
        scene: Arc<RwLock<SceneState>>,
        event_loop: &ActiveEventLoop,
        window_mode: WindowMode,
        log_buffer: LogBuffer,
    ) -> Self {
        let ctx = init::init(&window);
        // FIFO is set by build_context and never changed — the swapchain is
        // the screen clock.
        log::info!("wonderlamp: present mode: FIFO");

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

        let hz = detect_refresh_hz(&window);

        Self {
            window,
            ctx,
            pipeline,
            gpu_buffers,
            egui_renderer,
            scene,
            frame_stats: FrameStats::new(hz),
            last_phases: FramePhases::default(),
            frame_index: 0,
            egui_ctx,
            egui_winit,
            show_overlay: false,
            refresh_hz: hz,
            window_mode,
            local_ip: query_local_ip(),
            log_buffer,
        }
    }

    fn render(&mut self) {
        // Build the egui overlay if enabled.
        if self.show_overlay {
            let raw_input = self.egui_winit.take_egui_input(&self.window);
            let phases = self.last_phases;
            let size = self.window.inner_size();
            let sys = SystemInfo {
                display: StimulusDisplayInfo {
                    width_px: size.width,
                    height_px: size.height,
                    refresh_hz: self.refresh_hz,
                },
                backend: RenderTarget::Desktop(self.window_mode),
                local_ip: self.local_ip.clone(),
                hostname: String::new(),
                gpu_name: String::new(),
            };
            let output = self.egui_ctx.run_ui(raw_input, |ctx| {
                build_overlay_ui(ctx, &self.scene, &self.frame_stats, phases, &sys, &self.log_buffer);
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
            Some(ref t) => {
                self.last_phases = t.phases;
                // TODO: forward tick to scene / scheduler once that layer exists.
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
    log_buffer: Option<LogBuffer>,
}

impl WinitApp {
    pub fn new(scene: Arc<RwLock<SceneState>>, window_mode: WindowMode, log_buffer: LogBuffer) -> Self {
        Self {
            scene: Some(scene),
            is_fullscreen: window_mode == WindowMode::Fullscreen,
            window_mode,
            state: None,
            modifiers: winit::event::Modifiers::default(),
            log_buffer: Some(log_buffer),
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
            log::info!("wonderlamp: windowed — present mode: FIFO");
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
            log::info!("wonderlamp: fullscreen — present mode: FIFO");
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
            let log_buffer = self.log_buffer.take().expect("log_buffer already consumed");
            self.state = Some(State::new(window, scene, event_loop, self.window_mode, log_buffer));
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

// ── Refresh-rate detection ────────────────────────────────────────────────────

/// Determine the display refresh rate by trying several methods in order:
///
/// 1. DRM kernel interface (Linux only) — reads the active connector mode
///    directly from the kernel, compositor-independent.
/// 2. winit `video_modes()` filtered by current window resolution.
/// 3. winit `refresh_rate_millihertz()` on the current monitor.
///
/// Panics if the refresh rate cannot be determined. No silent fallback is
/// allowed — an unknown rate would cause drop detection to produce garbage.
fn detect_refresh_hz(window: &Window) -> f64 {
    // 1. DRM kernel interface (Linux only).
    #[cfg(target_os = "linux")]
    if let Some(hz) = query_refresh_hz_from_drm() {
        log::info!("wonderlamp: display clock (DRM): {hz:.3} Hz");
        return hz;
    }

    // 2. winit video_modes() — works on X11 and some Wayland compositors.
    if let Some(hz) = window.current_monitor().and_then(|m| {
        let size = window.inner_size();
        m.video_modes()
            .filter(|vm| vm.size() == size)
            .map(|vm| vm.refresh_rate_millihertz())
            .max()
            .map(|mhz| mhz as f64 / 1000.0)
    }) {
        log::info!("wonderlamp: display clock (video_modes): {hz:.3} Hz");
        return hz;
    }

    // 3. refresh_rate_millihertz() directly.
    if let Some(mhz) = window
        .current_monitor()
        .and_then(|m| m.refresh_rate_millihertz())
    {
        let hz = mhz as f64 / 1000.0;
        log::info!("wonderlamp: display clock (monitor API): {hz:.3} Hz");
        return hz;
    }

    panic!(
        "wonderlamp: cannot determine display refresh rate — \
         DRM query failed and compositor did not report refresh rate. \
         Check driver, permissions (/dev/dri/card*), and compositor."
    );
}

/// Query the refresh rate of the first connected DRM connector by reading the
/// active mode from the kernel. Does not require DRM master.
///
/// Refresh rate is computed precisely as `clock_kHz × 1000 / (htotal × vtotal)`.
#[cfg(target_os = "linux")]
fn query_refresh_hz_from_drm() -> Option<f64> {
    use drm::control::Device as ControlDevice;
    use std::os::fd::{AsFd, BorrowedFd};

    struct Card(std::fs::File);
    impl AsFd for Card {
        fn as_fd(&self) -> BorrowedFd<'_> {
            self.0.as_fd()
        }
    }
    impl drm::Device for Card {}
    impl ControlDevice for Card {}

    for n in 0..8u8 {
        let path = format!("/dev/dri/card{n}");
        let Ok(file) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
        else {
            continue;
        };
        let card = Card(file);
        let Ok(res) = card.resource_handles() else {
            continue;
        };
        for &conn_handle in res.connectors() {
            let Ok(conn) = card.get_connector(conn_handle, false) else {
                continue;
            };
            if conn.state() != drm::control::connector::State::Connected {
                continue;
            }
            // Follow connector → encoder → CRTC to get the *active* mode,
            // not just the first supported mode in the connector's list.
            let active_mode = conn
                .current_encoder()
                .and_then(|enc_h| card.get_encoder(enc_h).ok())
                .and_then(|enc| enc.crtc())
                .and_then(|crtc_h| card.get_crtc(crtc_h).ok())
                .and_then(|crtc| crtc.mode());
            if let Some(mode) = active_mode {
                let clock_hz = mode.clock() as f64 * 1000.0;
                let (_, _, htotal) = mode.hsync();
                let (_, _, vtotal) = mode.vsync();
                if htotal > 0 && vtotal > 0 {
                    return Some(clock_hz / (htotal as f64 * vtotal as f64));
                }
            }
        }
    }
    None
}
