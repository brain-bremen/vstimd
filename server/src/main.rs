use std::sync::{Arc, RwLock};

#[cfg(target_os = "linux")]
use wonderlamp_server::render::DrmRenderState;
use wonderlamp_server::render::{WindowMode, WinitApp};
use wonderlamp_server::scene::SceneState;

fn main() {
    let (render_target, window_mode) = parse_args();
    let scene = Arc::new(RwLock::new(SceneState::new()));
    let _zmq = wonderlamp_server::ipc::spawn_zmq_thread(scene.clone(), "tcp://0.0.0.0:5555");

    match render_target {
        #[cfg(target_os = "linux")]
        RenderTarget::Drm => DrmRenderState::new(scene).run_loop(),
        #[cfg(not(target_os = "linux"))]
        RenderTarget::Drm => {
            eprintln!("DRM/console mode is only available on Linux");
            std::process::exit(1);
        }
        RenderTarget::Desktop => {
            let event_loop = winit::event_loop::EventLoop::new().unwrap_or_else(|e| {
                eprintln!("wonderlamp: failed to create event loop: {e}");
                std::process::exit(1);
            });
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
            let mut app = WinitApp::new(scene, window_mode);
            event_loop.run_app(&mut app).unwrap();
        }
    }
}

// ── Argument parsing ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderTarget {
    Drm,
    Desktop,
}

/// Automatically detect the best render target for the current platform.
///
/// Detection logic:
/// - **Windows/macOS:** Always desktop (winit)
/// - **Linux with DISPLAY or WAYLAND_DISPLAY:** Desktop session → winit
/// - **Linux without display env vars:** Bare console → DRM
fn detect_render_target() -> RenderTarget {
    #[cfg(not(target_os = "linux"))]
    {
        RenderTarget::Desktop
    }

    #[cfg(target_os = "linux")]
    {
        // Check for display server environment variables
        let has_display =
            std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok();

        if has_display {
            eprintln!("wonderlamp: detected desktop session (DISPLAY or WAYLAND_DISPLAY set)");
            RenderTarget::Desktop
        } else {
            eprintln!("wonderlamp: detected console environment (no display server)");
            RenderTarget::Drm
        }
    }
}

fn parse_args() -> (RenderTarget, WindowMode) {
    let mut window_mode = WindowMode::default();

    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fullscreen" | "-f" => window_mode = WindowMode::Fullscreen,
            "--windowed" | "-w" => {
                let size = args.next().and_then(|s| {
                    let (w, h) = s.split_once('x')?;
                    Some((w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?))
                });
                let (w, h) = size.unwrap_or((800, 600));
                window_mode = WindowMode::Windowed {
                    width: w,
                    height: h,
                };
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => {
                eprintln!("Unknown argument: {other}");
                print_usage();
                std::process::exit(1);
            }
        }
    }

    let render_target = detect_render_target();

    eprintln!("wonderlamp: render target: {:?}", render_target);
    (render_target, window_mode)
}

fn print_usage() {
    eprintln!("Usage: wonderlamp_server [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -f, --fullscreen        Start in fullscreen mode (desktop only)");
    eprintln!("  -w, --windowed <WxH>    Start in windowed mode with size WxH (desktop only)");
    eprintln!("  -h, --help              Show this help message");
    eprintln!();
    eprintln!("Render target is automatically detected:");
    eprintln!("  - Windows/macOS: desktop (winit)");
    eprintln!("  - Linux with DISPLAY or WAYLAND_DISPLAY: desktop (winit)");
    eprintln!("  - Linux without display server: console (DRM/KMS)");
}
