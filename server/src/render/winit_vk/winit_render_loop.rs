use std::sync::{Arc, Mutex};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::log_buffer::LogBuffer;
use crate::render::RenderState;
use crate::render::backend::BackendData;
use crate::render::system_info::{ClockSource, SystemInfo};
use crate::render::{RenderTarget, StimulusDisplayInfo, WindowMode};
use crate::render::{SceneRenderer, TextRenderer, UiRenderer};
use crate::render::render_frame;
use crate::timing::FrameTiming;
use crate::vtl_state::VtlState;
extern crate vtl;

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

// ── Public entry point ────────────────────────────────────────────────────────

/// Create the winit event loop and window, call `on_ready` (for ZMQ bind +
/// systemd notify), then run until the window is closed or shutdown is requested.
pub fn run_render_loop(
    data: BackendData,
    window_mode: WindowMode,
    log_buffer: LogBuffer,
    on_ready: impl FnOnce(),
) {
    let event_loop = winit::event_loop::EventLoop::new().unwrap_or_else(|e| {
        log::error!("vstimd: failed to create event loop: {e}");
        std::process::exit(1);
    });
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut handler = WinitEventHandler::new(data, window_mode, log_buffer);
    on_ready();
    event_loop.run_app(&mut handler).unwrap();
}

// ── Per-window render data ────────────────────────────────────────────────────

struct WinitRenderLoopData {
    // Rust drops fields in declaration order.
    // `rs` (all Vulkan resources) and `egui_winit` must drop BEFORE `window`,
    // because they hold surface handles and wl_surface proxies into the window
    // that become dangling once the window is destroyed.
    rs: RenderState,
    vtl: Option<Arc<Mutex<VtlState>>>,
    /// Animation output bits accumulated this frame; committed at [A] next frame.
    pending_outputs: [u64; vtl::MAX_BANKS],
    egui_winit: egui_winit::State,
    // ── Window comes after all borrowers ─────────────────────────────────────
    window: Arc<Window>,
}

impl WinitRenderLoopData {
    fn new(
        window: Arc<Window>,
        data: BackendData,
        event_loop: &ActiveEventLoop,
        window_mode: WindowMode,
        log_buffer: LogBuffer,
    ) -> Self {
        let BackendData { scene, vtl, host_info } = data;
        let ctx = super::winit_init::init(&window);
        // FIFO is set by build_context and never changed — the swapchain is
        // the screen clock.
        log::info!("vstimd: present mode: FIFO");

        // Build sub-renderers before ctx moves into RenderState.
        let config_dir = scene.read().unwrap().runtime.config_dir.clone();
        let scene_renderer = SceneRenderer::new(&ctx, scene);
        let text = TextRenderer::new(&ctx);

        ctx.set_debug_name(ctx.render_pass, "render_pass");
        ctx.set_debug_name(ctx.egui_render_pass, "egui_render_pass");
        for (i, frame) in ctx.frames.iter().enumerate() {
            ctx.set_debug_name(frame.command_buffer, &format!("frame[{i}]_cmd"));
        }
        for (i, img) in ctx.swapchain_images.iter().enumerate() {
            ctx.set_debug_name(*img, &format!("swapchain[{i}]"));
        }

        let system_info = SystemInfo {
            host: host_info,
            gpu_name: String::new(),
            backend: RenderTarget::Desktop(window_mode),
            supports_wireframe: ctx.supports_wireframe,
            clock_source: if ctx.present_wait.is_some() {
                ClockSource::PresentWait
            } else {
                ClockSource::GpuCompletion
            },
        };

        let ui = UiRenderer::new(&ctx, config_dir, log_buffer);
        // egui::Context is Arc-based; clone gives egui_winit a handle to the
        // same context so it can read/write egui state (e.g. zoom factor).
        let egui_ctx = ui.egui_ctx.clone();
        let viewport_id = egui_ctx.viewport_id();
        let egui_winit = egui_winit::State::new(
            egui_ctx,
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

        let size = window.inner_size();
        let rs = RenderState {
            scene_renderer,
            text,
            ui: Some(ui),
            timing: FrameTiming::new(hz),
            system_info,
            display_info: StimulusDisplayInfo {
                width_px: size.width,
                height_px: size.height,
                refresh_hz: hz,
                mode_index: None,
            },
            ctx,
        };

        Self {
            rs,
            vtl,
            pending_outputs: [0; vtl::MAX_BANKS],
            egui_winit,
            window,
        }
    }

    fn render(&mut self) {
        // NOTE: vblank timing in the winit backend differs from DRM.
        //
        // In DRM mode, wait_vblank() is a dedicated blocking call before render,
        // giving a clean "frame start" boundary.  Here, the vblank boundary is
        // implicit: vkAcquireNextImageKHR blocks on FIFO until vblank, and
        // vkWaitForPresentKHR (at the top of render_frame, if present_wait is
        // available) confirms the previous frame is on screen.
        //
        // VTL input poll [A] and output write [B/C] should therefore be placed
        // around render_one_frame, but the precise vblank-relative position is
        // less clean than in DRM mode.  See vtl_state.rs for the canonical
        // description of the frame timeline.

        // 1. Collect egui input (via winit event integration, if overlay is on).
        let egui_raw_input = self.rs.ui
            .as_ref()
            .filter(|ui| ui.show_overlay)
            .map(|_| self.egui_winit.take_egui_input(&self.window));

        // [A] Commit previous frame's animation outputs; poll inputs.
        // Note: in winit mode the vkWaitForPresentKHR confirmation lives inside
        // render_frame(), so this poll fires at the top of the render loop rather
        // than at the true vblank boundary.  DRM mode gets exact vblank alignment.
        if let Some(vtl) = &self.vtl {
            let (input_edges, output_snapshot) = {
                let mut v = vtl.lock().unwrap();
                v.write_outputs(&self.pending_outputs);
                let edges = v.poll();
                let snap  = v.output_snapshot();
                (edges, snap)
            };
            self.pending_outputs = [0; vtl::MAX_BANKS];
            self.rs.scene_renderer.scene.write().unwrap().advance_animations(
                &input_edges, &output_snapshot, &mut self.pending_outputs,
            );
        }

        // 2. Render: build overlay UI, tessellate scene, record Vulkan commands,
        //    submit to GPU, present to display.
        //    The frame prepared here will become visible at the next vblank.
        let (tick, platform_output) = render_frame(
            &mut self.rs,
            None,
            egui_raw_input,
            self.vtl.as_deref(),
        );

        // pending_outputs is already saved for commit at next [A].

        // 3. Forward egui platform output (cursor changes, clipboard, etc.).
        if let Some(po) = platform_output {
            self.egui_winit.handle_platform_output(&self.window, po);
        }

        // 4. Handle swapchain out of date (resize, monitor change, etc.).
        if tick.is_none() {
            let size = self.window.inner_size();
            self.rs.display_info.width_px = size.width;
            self.rs.display_info.height_px = size.height;
            self.rs.ctx.recreate_swapchain(ash::vk::Extent2D {
                width: size.width.max(1),
                height: size.height.max(1),
            });
        }
    }
}

// ── WinitEventHandler — winit ApplicationHandler ──────────────────────────────

struct WinitEventHandler {
    backend_data: Option<BackendData>,
    log_buffer: Option<LogBuffer>,
    window_mode: WindowMode,
    render_data: Option<WinitRenderLoopData>,
    modifiers: winit::event::Modifiers,
    is_fullscreen: bool,
}

impl WinitEventHandler {
    fn new(data: BackendData, window_mode: WindowMode, log_buffer: LogBuffer) -> Self {
        Self {
            is_fullscreen: window_mode == WindowMode::Fullscreen,
            backend_data: Some(data),
            log_buffer: Some(log_buffer),
            window_mode,
            render_data: None,
            modifiers: winit::event::Modifiers::default(),
        }
    }

    fn toggle_fullscreen(&mut self, event_loop: &ActiveEventLoop) {
        if self.render_data.is_none() {
            return;
        }

        if self.is_fullscreen {
            // ── Leaving fullscreen → windowed ─────────────────────────────────
            // Present mode is already FIFO and does not change.
            let data = self.render_data.as_ref().unwrap();
            data.window.set_fullscreen(None);
            if let WindowMode::Windowed { width, height } = self.window_mode {
                let _ = data
                    .window
                    .request_inner_size(winit::dpi::LogicalSize::new(width, height));
            }
            self.is_fullscreen = false;
            log::info!("vstimd: windowed — present mode: FIFO");
        } else {
            // ── Entering fullscreen ───────────────────────────────────────────
            // Present mode stays FIFO throughout — the swapchain is the screen
            // clock and must not be decoupled from vblank.
            let monitor = {
                let d = self.render_data.as_ref().unwrap();
                d.window
                    .current_monitor()
                    .or_else(|| event_loop.primary_monitor())
                    .or_else(|| event_loop.available_monitors().next())
            };

            let data = self.render_data.as_ref().unwrap();
            data.window
                .set_fullscreen(Some(Fullscreen::Borderless(monitor)));
            self.is_fullscreen = true;
            log::info!("vstimd: fullscreen — present mode: FIFO");
        }
    }
}

impl ApplicationHandler for WinitEventHandler {
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if crate::shutdown::is_requested() {
            event_loop.exit();
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.render_data.is_none() {
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
            let data = self.backend_data.take().expect("backend_data already consumed");
            let log_buffer = self.log_buffer.take().expect("log_buffer already consumed");
            self.render_data = Some(WinitRenderLoopData::new(
                window,
                data,
                event_loop,
                self.window_mode,
                log_buffer,
            ));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Forward all window events to egui first; it may consume input events
        // (e.g. keyboard when a text field is focused).
        if let Some(data) = &mut self.render_data {
            let response = data.egui_winit.on_window_event(&data.window, &event);
            if response.consumed {
                return;
            }
        }

        match &event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(data) = &mut self.render_data {
                    data.rs.display_info.width_px = size.width;
                    data.rs.display_info.height_px = size.height;
                    data.rs.ctx.recreate_swapchain(ash::vk::Extent2D {
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
                    if let Some(data) = &mut self.render_data
                        && let Some(ui) = &mut data.rs.ui
                    {
                        ui.show_overlay = !ui.show_overlay;
                    }
                }
                KeyCode::F2 => {
                    if let Some(data) = &self.render_data {
                        let mut sc = data.rs.scene_renderer.scene.write().expect("scene lock");
                        sc.photodiode.enabled = !sc.photodiode.enabled;
                        sc.photodiode.flicker = true;
                        sc.photodiode.lit = false;
                    }
                }
                KeyCode::F3 => {
                    if let Some(data) = &mut self.render_data
                        && data.rs.ctx.supports_wireframe
                    {
                        data.rs.scene_renderer.wireframe = !data.rs.scene_renderer.wireframe;
                        log::info!(
                            "vstimd: wireframe {}",
                            if data.rs.scene_renderer.wireframe { "ON" } else { "OFF" }
                        );
                    }
                }
                KeyCode::KeyD => {
                    if let Some(data) = &self.render_data {
                        crate::render::spawn_demo_stimuli(&data.rs.scene_renderer.scene);
                    }
                }
                KeyCode::F11 => self.toggle_fullscreen(event_loop),
                KeyCode::Enter if self.modifiers.state().alt_key() => {
                    self.toggle_fullscreen(event_loop);
                }
                _ => {}
            },
            WindowEvent::RedrawRequested => {
                if let Some(data) = &mut self.render_data {
                    data.render();
                    data.window.request_redraw();
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
    if let Some(mhz) = window
        .current_monitor()
        .and_then(|m| m.refresh_rate_millihertz())
    {
        let hz = mhz as f64 / 1000.0;
        log::info!("vstimd: display clock (monitor API): {hz:.3} Hz");
        return hz;
    }

    // 3. winit video_modes() — fallback; filtered by the window's current size.
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
