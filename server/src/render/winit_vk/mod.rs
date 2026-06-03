mod init;

use std::sync::{Arc, RwLock};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::log_buffer::LogBuffer;
use crate::render::BenchmarkState;
use crate::render::MetricsSampler;
use crate::render::RenderState;
use crate::render::system_info::ClockSource;
use crate::render::vk::{GlyphAtlas, PhotodiodeCache, SolidMeshCache, VkEguiRenderer, VkGratingPipeline, VkPipeline};
use crate::scene::stimulus::text::{TextFontSystem, TextSwashCache};
use crate::render::{RenderTarget, StimulusDisplayInfo, SystemInfo, WindowMode, query_local_ip};
use crate::scene::SceneState;
use crate::timing::{FramePhases, FrameStats};

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

// ── Per-window state ──────────────────────────────────────────────────────────

struct State {
    // Rust drops fields in declaration order.
    // `rs` (all Vulkan resources) and `egui_winit` must drop BEFORE `window`,
    // because they hold surface handles and wl_surface proxies into the window
    // that become dangling once the window is destroyed.
    rs: RenderState,
    egui_winit: egui_winit::State,
    // ── Window comes after all borrowers ─────────────────────────────────────
    window: Arc<Window>,
    refresh_hz: f64,
    window_mode: WindowMode,
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
        log::info!("vstimd: present mode: FIFO");

        let wf_mode = if ctx.supports_wireframe {
            ash::vk::PolygonMode::LINE
        } else {
            ash::vk::PolygonMode::FILL
        };
        let pipeline = VkPipeline::new(&ctx.device, ctx.render_pass, ash::vk::PolygonMode::FILL);
        let grating_pipeline = VkGratingPipeline::new(
            &ctx.device,
            &ctx.instance,
            ctx.physical_device,
            ctx.render_pass,
            ash::vk::PolygonMode::FILL,
        );
        let wireframe_pipeline = VkPipeline::new(&ctx.device, ctx.render_pass, wf_mode);
        let wireframe_grating = VkGratingPipeline::new(
            &ctx.device,
            &ctx.instance,
            ctx.physical_device,
            ctx.render_pass,
            wf_mode,
        );

        ctx.set_debug_name(pipeline.pipeline, "solid_pipeline");
        ctx.set_debug_name(grating_pipeline.pipeline, "grating_pipeline");
        ctx.set_debug_name(wireframe_pipeline.pipeline, "solid_wireframe_pipeline");
        ctx.set_debug_name(wireframe_grating.pipeline, "grating_wireframe_pipeline");
        ctx.set_debug_name(ctx.render_pass, "render_pass");
        ctx.set_debug_name(ctx.egui_render_pass, "egui_render_pass");
        for (i, frame) in ctx.frames.iter().enumerate() {
            ctx.set_debug_name(frame.command_buffer, &format!("frame[{i}]_cmd"));
        }
        for (i, img) in ctx.swapchain_images.iter().enumerate() {
            ctx.set_debug_name(*img, &format!("swapchain[{i}]"));
        }

        let solid_meshes = SolidMeshCache::new(&ctx.instance, ctx.physical_device);
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

        if ctx.present_wait.is_none() {
            if ctx.display_timing.is_some() {
                log::warn!(
                    "vstimd: *** VK_GOOGLE_display_timing is available, but this path \
                     does not use it for vblank timestamping without \
                     VK_KHR_present_wait. ***"
                );
            } else {
                log::warn!(
                    "vstimd: *** No vblank clock available (VK_KHR_present_wait absent). ***"
                );
            }
            log::warn!(
                "vstimd: Stimulus timestamps will reflect GPU-completion time, not \
                 vblank. Use DRM mode or a GPU with present_wait for accurate timing."
            );
        }

        let glyph_atlas = GlyphAtlas::new(&ctx.device, &ctx.instance, ctx.physical_device);
        let rs = RenderState {
            frame_stats: FrameStats::new(hz),
            ctx,
            pipeline,
            grating_pipeline,
            wireframe_pipeline,
            wireframe_grating,
            wireframe: false,
            solid_meshes,
            pd_cache: PhotodiodeCache::default(),
            glyph_atlas,
            font_system: TextFontSystem::new(),
            swash_cache: TextSwashCache::new(),
            egui_renderer,
            egui_ctx,
            scene,
            last_phases: FramePhases::default(),
            frame_index: 0,
            show_overlay: false,
            benchmark: BenchmarkState::new(),
            local_ip: query_local_ip(),
            log_buffer,
            metrics: MetricsSampler::new(),
        };

        Self {
            rs,
            egui_winit,
            window,
            refresh_hz: hz,
            window_mode,
        }
    }

    fn sys_info(&self) -> SystemInfo {
        let size = self.window.inner_size();
        SystemInfo {
            display: StimulusDisplayInfo {
                width_px: size.width,
                height_px: size.height,
                refresh_hz: self.refresh_hz,
            },
            backend: RenderTarget::Desktop(self.window_mode),
            local_ip: self.rs.local_ip.clone(),
            hostname: String::new(),
            gpu_name: String::new(),
            wireframe: self.rs.ctx.supports_wireframe.then_some(self.rs.wireframe),
            // VK_GOOGLE_display_timing alone does not mean we're using display
            // timestamps yet — report GpuCompletion until present_wait is active.
            clock_source: if self.rs.ctx.present_wait.is_some() {
                ClockSource::PresentWait
            } else {
                ClockSource::GpuCompletion
            },
        }
    }

    fn render(&mut self) {
        // 1. Collect egui input (via winit event integration, if overlay is on).
        let egui_raw_input = self
            .rs
            .show_overlay
            .then(|| self.egui_winit.take_egui_input(&self.window));

        // 2. Render: build overlay UI, tessellate scene, record Vulkan commands,
        //    submit to GPU, present to display.
        let sys_info = self.sys_info();
        let (tick, platform_output) = self.rs.render_one_frame(None, egui_raw_input, &sys_info);

        // 3. Forward egui platform output (cursor changes, clipboard, etc.).
        if let Some(po) = platform_output {
            self.egui_winit.handle_platform_output(&self.window, po);
        }

        // 4. Handle swapchain out of date (resize, monitor change, etc.).
        if tick.is_none() {
            let size = self.window.inner_size();
            self.rs.ctx.recreate_swapchain(ash::vk::Extent2D {
                width: size.width.max(1),
                height: size.height.max(1),
            });
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
    pub fn new(
        scene: Arc<RwLock<SceneState>>,
        window_mode: WindowMode,
        log_buffer: LogBuffer,
    ) -> Self {
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
            // ── Leaving fullscreen → windowed ─────────────────────────────────
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
            log::info!("vstimd: windowed — present mode: FIFO");
        } else {
            // ── Entering fullscreen ───────────────────────────────────────────
            // Present mode stays FIFO throughout — the swapchain is the screen
            // clock and must not be decoupled from vblank.
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
            log::info!("vstimd: fullscreen — present mode: FIFO");
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
                        .with_title("vstimd")
                        .with_fullscreen(Some(Fullscreen::Borderless(monitor)))
                }
                WindowMode::Windowed { width, height } => Window::default_attributes()
                    .with_title("vstimd")
                    .with_inner_size(winit::dpi::LogicalSize::new(width, height))
                    .with_resizable(true),
            };
            let window = Arc::new(event_loop.create_window(attrs).unwrap());
            let scene = self.scene.take().expect("scene already consumed");
            let log_buffer = self.log_buffer.take().expect("log_buffer already consumed");
            self.state = Some(State::new(
                window,
                scene,
                event_loop,
                self.window_mode,
                log_buffer,
            ));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Forward all window events to egui first; it may consume input events
        // (e.g. keyboard when a text field is focused).
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
                    state.rs.ctx.recreate_swapchain(ash::vk::Extent2D {
                        width: size.width.max(1),
                        height: size.height.max(1),
                    });
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
                        state.rs.show_overlay = !state.rs.show_overlay;
                    }
                }
                KeyCode::F2 => {
                    if let Some(state) = &self.state {
                        let mut sc = state.rs.scene.write().expect("scene lock");
                        sc.photodiode.enabled = !sc.photodiode.enabled;
                        sc.photodiode.flicker = true;
                        sc.photodiode.lit = false;
                    }
                }
                KeyCode::F3 => {
                    if let Some(state) = &mut self.state
                        && state.rs.ctx.supports_wireframe
                    {
                        state.rs.wireframe = !state.rs.wireframe;
                        log::info!(
                            "vstimd: wireframe {}",
                            if state.rs.wireframe { "ON" } else { "OFF" }
                        );
                    }
                }
                KeyCode::KeyD => {
                    if let Some(state) = &self.state {
                        crate::render::spawn_demo_stimuli(&state.rs.scene);
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
/// 1. DRM kernel interface (Linux bare-metal only) — reads the active connector
///    mode directly from the kernel. Skipped when a compositor is running.
/// 2. winit `refresh_rate_millihertz()` — queries the active compositor mode.
/// 3. winit `video_modes()` filtered by current window resolution — fallback.
///
/// Panics if the refresh rate cannot be determined. No silent fallback is
/// allowed — an unknown rate would cause drop detection to produce garbage.
fn detect_refresh_hz(window: &Window) -> f64 {
    // 1. DRM kernel interface (Linux only, bare-metal only).
    // Skip when a compositor is running: DRM iterates connectors in kernel
    // order and has no way to match one to the window's monitor. On a
    // multi-monitor system it will pick the wrong connector.
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_none()
        && let Some(hz) = query_refresh_hz_from_drm()
    {
        log::info!("vstimd: display clock (DRM): {hz:.3} Hz");
        return hz;
    }

    // 2. refresh_rate_millihertz() — queries the active XRandR/compositor mode.
    // Preferred over video_modes() because it reflects what the compositor is
    // actually running, not just what modes the monitor supports.
    if let Some(mhz) = window
        .current_monitor()
        .and_then(|m| m.refresh_rate_millihertz())
    {
        let hz = mhz as f64 / 1000.0;
        log::info!("vstimd: display clock (monitor API): {hz:.3} Hz");
        return hz;
    }

    // 3. winit video_modes() — fallback; filtered by the window's current size.
    // Unreliable at construction time: inner_size() may be a platform default
    // (e.g. 800×600) before the compositor applies fullscreen, which can match
    // a wrong VESA mode (800×600@75 Hz) instead of the native rate.
    if let Some(hz) = window.current_monitor().and_then(|m| {
        let size = window.inner_size();
        m.video_modes()
            .filter(|vm| vm.size() == size)
            .map(|vm| vm.refresh_rate_millihertz())
            .max()
            .map(|mhz| mhz as f64 / 1000.0)
    }) {
        log::info!("vstimd: display clock (video_modes): {hz:.3} Hz");
        return hz;
    }

    panic!(
        "vstimd: cannot determine display refresh rate — \
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
