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

    let config_dir = args.config_dir.clone().unwrap_or_else(|| std::path::PathBuf::from("."));
    let scene = Arc::new(RwLock::new(SceneState::new_with_config_dir(config_dir.clone())));

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

    if let Some(ref path) = args.config_file {
        match vstimd::io_config::load_config(path) {
            Ok((scene_cfg, io)) => {
                if let Some(ref v) = vtl {
                    let mut v = v.lock().unwrap();
                    v.config.names = io.vtl.names;
                    v.sync_names_to_shm();
                }
                scene.write().unwrap().load_snapshot(scene_cfg, vstimd::scene::LoadMode::Replace);
                log::info!("vstimd: loaded config from {:?}", path);
            }
            Err(e) => log::error!("vstimd: failed to load config {:?}: {e}", path),
        }
    }

    let (zmq_thread, zmq_shutdown, zmq_bound) =
        vstimd::ipc::spawn_zmq_thread(
            scene.clone(),
            vtl.clone(),
            &format!("tcp://0.0.0.0:{}", args.zmq_port),
        );

    // Install signal handlers once, before any render path (including Vulkan
    // init which can take several seconds on DRM).
    install_signal_handlers();

    match args.render_target {
        #[cfg(target_os = "linux")]
        RenderTarget::Drm => {
            let rs = DrmRenderState::new(scene, vtl, log_buffer);
            if wait_zmq_bound(&zmq_bound, args.zmq_port) { notify_ready(); }
            rs.run_loop();
        }
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
            if wait_zmq_bound(&zmq_bound, args.zmq_port) { notify_ready(); }
            event_loop.run_app(&mut app).unwrap();
        }
        RenderTarget::Null => {
            log::info!("vstimd: null renderer — ZMQ server + animation loop running, no display");

            if wait_zmq_bound(&zmq_bound, args.zmq_port) { notify_ready(); }

            let frame_period = {
                let s = scene.read().unwrap();
                std::time::Duration::from_secs_f32(1.0 / s.runtime.frame_rate)
            };
            let mut output_pending = [0u64; vtl::MAX_BANKS];
            loop {
                if vstimd::shutdown::is_requested() {
                    break;
                }
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

    // Signal the ZMQ thread to exit and wait for it to finish.  This ensures
    // the thread's Arc references are released — VtlOwner::drop runs shm_unlink
    // and the shm segment is cleaned up before the process exits.
    drop(zmq_shutdown);
    zmq_thread.join().ok();
}

// ── Argument parsing ──────────────────────────────────────────────────────────

struct Args {
    render_target: RenderTarget,
    verbose: bool,
    zmq_port: u16,
    config_file: Option<std::path::PathBuf>,
    config_dir:  Option<std::path::PathBuf>,
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
    let zmq_port = vstimd::ipc::DEFAULT_ZMQ_PORT;
    let mut config_file: Option<std::path::PathBuf> = None;
    let mut config_dir:  Option<std::path::PathBuf> = None;

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
            "--config" => {
                config_file = args.next().map(std::path::PathBuf::from);
            }
            "--config-dir" => {
                config_dir = args.next().map(std::path::PathBuf::from);
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
        zmq_port,
        config_file,
        config_dir,
    }
}

/// Install SIGTERM/SIGINT handlers that set the shared shutdown flag.
/// Called once before any render path so the handler is active during
/// Vulkan init (which can take several seconds on DRM hardware).
fn install_signal_handlers() {
    extern "C" fn on_signal(_: libc::c_int) {
        vstimd::shutdown::request();
    }
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
    }
}

/// Block until the ZMQ thread signals that `socket.bind()` has succeeded.
/// Returns `true` if the signal arrived, `false` on timeout (ZMQ unavailable).
fn wait_zmq_bound(rx: &std::sync::mpsc::Receiver<()>, port: u16) -> bool {
    if rx.recv_timeout(std::time::Duration::from_secs(10)).is_err() {
        log::warn!("vstimd: ZMQ bind did not complete within 10 s — port {port} may not be listening");
        return false;
    }
    true
}

/// Send `READY=1` to systemd via `$NOTIFY_SOCKET` if present.
/// No-op when not launched by systemd or on non-Linux platforms.
fn notify_ready() {
    #[cfg(target_os = "linux")]
    {
        let has_socket = std::env::var_os("NOTIFY_SOCKET").is_some();
        match sd_notify::notify(false, &[sd_notify::NotifyState::Ready]) {
            Ok(()) if has_socket => log::info!("vstimd: systemd READY=1 sent"),
            Ok(()) => log::info!("vstimd: sd_notify: NOTIFY_SOCKET not set (not running under systemd)"),
            Err(e) => log::warn!("vstimd: sd_notify failed: {e}"),
        }
    }
}

fn print_usage() {
    eprintln!("Usage: vstimd [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -w, --windowed <WxH>      Start in windowed mode with size WxH (desktop only)");
    eprintln!("      --null                No rendering; ZMQ server only (also: VSTIMD_NULL=1)");
    eprintln!("  -v, --verbose             Enable debug logging (overridden by RUST_LOG)");
  eprintln!("      --config <path>        Load config file at startup");
  eprintln!("      --config-dir <path>    Directory for named config files (default: .)");
    eprintln!("  -h, --help                Show this help message");
    eprintln!();
    eprintln!("Render target is automatically detected:");
    eprintln!("  - Windows/macOS: desktop (winit)");
    eprintln!("  - Linux with DISPLAY or WAYLAND_DISPLAY: desktop (winit)");
    eprintln!("  - Linux without display server: console (DRM/KMS)");
}
