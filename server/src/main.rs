use std::sync::{Arc, RwLock};

#[cfg(target_os = "linux")]
use wonderlamp_server::render::DrmRenderState;
use wonderlamp_server::render::{WindowMode, WinitApp};
use wonderlamp_server::scene::SceneState;

fn main() {
    let args = parse_args();

    let default_level = if args.verbose { "debug" } else { "info" };
    let server_start = std::time::Instant::now();
    let env_logger = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(default_level),
    )
    .build();
    let log_buffer = wonderlamp_server::log_buffer::install(env_logger, server_start);

    let scene = Arc::new(RwLock::new(SceneState::new()));
    let _zmq = wonderlamp_server::ipc::spawn_zmq_thread(scene.clone(), "tcp://0.0.0.0:5555");

    match args.render_target {
        #[cfg(target_os = "linux")]
        RenderTarget::Drm => DrmRenderState::new(scene, log_buffer).run_loop(),
        #[cfg(not(target_os = "linux"))]
        RenderTarget::Drm => {
            log::error!("DRM/console mode is only available on Linux");
            std::process::exit(1);
        }
        RenderTarget::Desktop => {
            let event_loop = winit::event_loop::EventLoop::new().unwrap_or_else(|e| {
                log::error!("wonderlamp: failed to create event loop: {e}");
                std::process::exit(1);
            });
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
            let mut app = WinitApp::new(scene, args.window_mode, log_buffer);
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

struct Args {
    render_target: RenderTarget,
    window_mode: WindowMode,
    verbose: bool,
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
        let has_display =
            std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok();

        if has_display {
            log::info!("wonderlamp: detected desktop session (DISPLAY or WAYLAND_DISPLAY set)");
            RenderTarget::Desktop
        } else {
            log::info!("wonderlamp: detected console environment (no display server)");
            RenderTarget::Drm
        }
    }
}

fn parse_args() -> Args {
    let mut window_mode = WindowMode::default();
    let mut verbose = false;

    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--verbose" | "-v" => verbose = true,
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
                eprintln!("wonderlamp: unknown argument: {other}");
                print_usage();
                std::process::exit(1);
            }
        }
    }

    let render_target = detect_render_target();
    log::info!("wonderlamp: render target: {:?}", render_target);

    Args {
        render_target,
        window_mode,
        verbose,
    }
}

fn print_usage() {
    eprintln!("Usage: wonderlamp_server [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -w, --windowed <WxH>      Start in windowed mode with size WxH (desktop only)");
    eprintln!("  -v, --verbose             Enable debug logging (overridden by RUST_LOG)");
    eprintln!("  -h, --help                Show this help message");
    eprintln!();
    eprintln!("Render target is automatically detected:");
    eprintln!("  - Windows/macOS: desktop (winit)");
    eprintln!("  - Linux with DISPLAY or WAYLAND_DISPLAY: desktop (winit)");
    eprintln!("  - Linux without display server: console (DRM/KMS)");
}
