use std::sync::{Arc, Mutex, RwLock};

#[cfg(target_os = "linux")]
use vstimd::render::DrmRenderState;
use vstimd::render::{RenderTarget, WindowMode, WinitApp};
use vstimd::scene::SceneState;
use vstimd::vtl_state::VtlState;

fn main() {
    let args = parse_args();

    let default_level = if args.verbose { "debug" } else { "info" };
    let server_start = std::time::Instant::now();
    let env_logger = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(default_level),
    )
    .build();
    let log_buffer = vstimd::log_buffer::install(env_logger, server_start);

    let scene = Arc::new(RwLock::new(SceneState::new()));

    // Create VTL shared memory on Linux. The Arc<Mutex<>> lets both the ZMQ
    // thread (software triggers, naming) and the render backend (frame polling)
    // access it safely.
    #[cfg(target_os = "linux")]
    let vtl: Option<Arc<Mutex<VtlState>>> = vtl::VtlOwner::create("/vstimd_vtl", 4, 1)
        .map(|owner| Arc::new(Mutex::new(VtlState::new(owner))))
        .map_err(|e| log::warn!("vtl: failed to create shm segment: {e}"))
        .ok();
    #[cfg(not(target_os = "linux"))]
    let vtl: Option<Arc<Mutex<VtlState>>> = None;

    if vtl.is_some() {
        log::info!("vtl: shared memory segment created at /vstimd_vtl");
    }

    let _zmq = vstimd::ipc::spawn_zmq_thread(scene.clone(), vtl.clone(), "tcp://0.0.0.0:5555");

    match args.render_target {
        #[cfg(target_os = "linux")]
        RenderTarget::Drm => DrmRenderState::new(scene, vtl, log_buffer).run_loop(),
        #[cfg(not(target_os = "linux"))]
        RenderTarget::Drm => {
            log::error!("DRM/console mode is only available on Linux");
            std::process::exit(1);
        }
        RenderTarget::Desktop(window_mode) => {
            let event_loop = winit::event_loop::EventLoop::new().unwrap_or_else(|e| {
                log::error!("vstimd: failed to create event loop: {e}");
                std::process::exit(1);
            });
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
            let mut app = WinitApp::new(scene, vtl, window_mode, log_buffer);
            event_loop.run_app(&mut app).unwrap();
        }
        RenderTarget::Null => {
            log::info!("vstimd: null renderer — ZMQ server + animation loop running, no display");
            let frame_period = {
                let s = scene.read().unwrap();
                std::time::Duration::from_secs_f32(1.0 / s.runtime.frame_rate)
            };
            let mut output_pending = [0u64; vtl::MAX_BANKS];
            loop {
                let t0 = std::time::Instant::now();
                let edges = vtl.as_ref()
                    .and_then(|v| v.lock().ok().map(|mut g| g.poll()))
                    .unwrap_or_default();
                {
                    let mut s = scene.write().unwrap();
                    if s.runtime.pending_flip {
                        s.apply_flip();
                    }
                    s.runtime.frame_count += 1;
                    let _ = s.runtime.frame_notifier.send(s.runtime.frame_count);
                    let output_snapshot = [0u64; vtl::MAX_BANKS];
                    s.advance_animations(&edges, &output_snapshot, &mut output_pending);
                }
                if let Some(remaining) = frame_period.checked_sub(t0.elapsed()) {
                    std::thread::sleep(remaining);
                }
            }
        }
    }
}

// ── Argument parsing ──────────────────────────────────────────────────────────

struct Args {
    render_target: RenderTarget,
    verbose: bool,
}

/// Automatically detect the best render target for the current platform.
///
/// Detection logic:
/// - **Windows/macOS:** Always desktop (winit)
/// - **Linux with DISPLAY or WAYLAND_DISPLAY:** Desktop session → winit
/// - **Linux without display env vars:** Bare console → DRM
fn detect_render_target(window_mode: WindowMode) -> RenderTarget {
    #[cfg(not(target_os = "linux"))]
    {
        RenderTarget::Desktop(window_mode)
    }

    #[cfg(target_os = "linux")]
    {
        let has_display =
            std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok();

        if has_display {
            log::info!("vstimd: detected desktop session (DISPLAY or WAYLAND_DISPLAY set)");
            RenderTarget::Desktop(window_mode)
        } else {
            log::info!("vstimd: detected console environment (no display server)");
            RenderTarget::Drm
        }
    }
}

fn parse_args() -> Args {
    let mut window_mode = WindowMode::default();
    let mut verbose = false;
    let mut null = false;

    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--verbose" | "-v" => verbose = true,
            "--null" => null = true,
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
                eprintln!("vstimd: unknown argument: {other}");
                print_usage();
                std::process::exit(1);
            }
        }
    }

    let render_target = if null || std::env::var("VSTIMD_NULL").is_ok() {
        RenderTarget::Null
    } else {
        detect_render_target(window_mode)
    };
    log::info!("vstimd: render target: {:?}", render_target);

    Args {
        render_target,
        verbose,
    }
}

fn print_usage() {
    eprintln!("Usage: vstimd [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -w, --windowed <WxH>      Start in windowed mode with size WxH (desktop only)");
    eprintln!("      --null                No rendering; ZMQ server only (also: VSTIMD_NULL=1)");
    eprintln!("  -v, --verbose             Enable debug logging (overridden by RUST_LOG)");
    eprintln!("  -h, --help                Show this help message");
    eprintln!();
    eprintln!("Render target is automatically detected:");
    eprintln!("  - Windows/macOS: desktop (winit)");
    eprintln!("  - Linux with DISPLAY or WAYLAND_DISPLAY: desktop (winit)");
    eprintln!("  - Linux without display server: console (DRM/KMS)");
}
