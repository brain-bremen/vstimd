use std::sync::{Arc, Mutex, RwLock};

#[cfg(target_os = "linux")]
use vstimd::render::drm::DrmBackend;
use vstimd::render::{BackendData, HostInfo, NullBackend, RenderTarget, WindowMode};
use vstimd::render::{query_hardware_model, query_hostname, query_local_ip};
use vstimd::render::winit_vk::WinitBackend;
use vstimd::rig_config;
use vstimd::scene::SceneState;
use vstimd::vtl_state::VtlState;

fn main() {
    let args = parse_args();

    let default_level = if args.verbose { "debug" } else { "info" };
    let server_start = std::time::Instant::now();
    let env_logger =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level))
            .build();
    let log_buffer = vstimd::log_buffer::install(env_logger, server_start);

    log::info!(
        "vstimd v{} (built {})",
        env!("CARGO_PKG_VERSION"),
        env!("VSTIMD_BUILD_DATE"),
    );
    let host_info = HostInfo {
        hardware_model: query_hardware_model(),
        hostname: query_hostname(),
        local_ip: query_local_ip(),
        zmq_port: vstimd::ipc::DEFAULT_ZMQ_PORT,
    };
    log::info!("vstimd: hardware: {}", host_info.hardware_model);

    // Load rig-config (hardware-specific settings). Non-fatal if absent.
    let rig = match rig_config::load(&args.rig_config) {
        Ok(r) => {
            log::info!("vstimd: rig-config loaded from {}", args.rig_config);
            if let Some(ref d) = r.display.width.map(|w| format!("{w}×{}@{}Hz",
                r.display.height.unwrap_or(0),
                r.display.refresh_hz.unwrap_or(0.0)))
            {
                log::info!("vstimd: rig display preference: {d} (not yet applied to DRM mode)");
            }
            r
        }
        Err(e) => {
            log::error!("vstimd: {e}");
            std::process::exit(1);
        }
    };

    let config_dir = args
        .config_dir
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let scene = Arc::new(RwLock::new(SceneState::new_with_config_dir(
        config_dir.clone(),
    )));

    // Create VTL shared memory on Linux using rig-config parameters.
    // The Arc<Mutex<>> lets both the ZMQ thread (software triggers, naming)
    // and the render backend (frame polling) access it safely.
    #[cfg(target_os = "linux")]
    let vtl: Option<Arc<Mutex<VtlState>>> = vtl::VtlOwner::create(
        &rig.vtl.shm_name,
        rig.vtl.num_input_banks,
        rig.vtl.num_output_banks,
    )
    .map(|owner| {
        let mut state = VtlState::new(owner);
        state.vblank_vtl = rig.vtl.vblank;
        Arc::new(Mutex::new(state))
    })
    .map_err(|e| log::warn!("vtl: failed to create shm segment: {e}"))
    .ok();
    #[cfg(not(target_os = "linux"))]
    let vtl: Option<Arc<Mutex<VtlState>>> = None;

    if vtl.is_some() {
        log::info!(
            "vtl: segment '{}' created ({} in / {} out bank(s)){}",
            rig.vtl.shm_name,
            rig.vtl.num_input_banks,
            rig.vtl.num_output_banks,
            rig.vtl.vblank.map_or(String::new(), |vb| format!("  vblank=bank{}·bit{}", vb.bank, vb.bit)),
        );
    }

    if let Some(ref path) = args.config_file {
        match vstimd::io_config::load_config(path) {
            Ok((scene_cfg, io)) => {
                if let Some(ref v) = vtl {
                    let mut v = v.lock().unwrap();
                    v.config.names = io.vtl.names;
                    v.sync_names_to_shm();
                }
                scene
                    .write()
                    .unwrap()
                    .load_snapshot(scene_cfg, vstimd::scene::LoadMode::Replace);
                log::info!("vstimd: loaded config from {:?}", path);
            }
            Err(e) => log::error!("vstimd: failed to load config {:?}: {e}", path),
        }
    }

    let (zmq_thread, zmq_shutdown, zmq_bound) = vstimd::ipc::spawn_zmq_thread(
        scene.clone(),
        vtl.clone(),
        &format!("tcp://0.0.0.0:{}", args.zmq_port),
    );

    // Install signal handlers once, before any render path (including Vulkan
    // init which can take several seconds on DRM).
    install_signal_handlers();

    let data = BackendData { scene, vtl, host_info };
    let zmq_port = args.zmq_port;
    let on_ready = move || {
        if wait_zmq_bound(&zmq_bound, zmq_port) {
            notify_ready();
        }
    };

    match args.render_target {
        #[cfg(target_os = "linux")]
        RenderTarget::Drm => DrmBackend::new(data, log_buffer).run(on_ready),
        #[cfg(not(target_os = "linux"))]
        RenderTarget::Drm => {
            log::error!("DRM/console mode is only available on Linux");
            std::process::exit(1);
        }
        RenderTarget::Desktop(window_mode) => {
            WinitBackend::new(data, window_mode, log_buffer).run(on_ready);
        }
        RenderTarget::Null => NullBackend::new(data).run(on_ready),
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
    rig_config: String,
    config_file: Option<std::path::PathBuf>,
    config_dir: Option<std::path::PathBuf>,
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
    let mut explicit_windowed = false;
    let mut verbose = false;
    let mut null = false;
    let zmq_port = vstimd::ipc::DEFAULT_ZMQ_PORT;
    let mut rig_config  = rig_config::DEFAULT_PATH.to_string();
    let mut config_file: Option<std::path::PathBuf> = None;
    let mut config_dir: Option<std::path::PathBuf> = None;

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
                explicit_windowed = true;
            }
            "--rig-config" => {
                rig_config = args.next().unwrap_or_else(|| {
                    eprintln!("vstimd: --rig-config requires a path argument");
                    std::process::exit(1);
                });
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

    if explicit_windowed && render_target == RenderTarget::Drm {
        eprintln!(
            "vstimd: --windowed requires a desktop session \
             (DISPLAY or WAYLAND_DISPLAY must be set). \
             DRM/console mode does not support windowed output."
        );
        std::process::exit(1);
    }

    log::info!("vstimd: render target: {:?}", render_target);

    Args {
        render_target,
        verbose,
        zmq_port,
        rig_config,
        config_file,
        config_dir,
    }
}

/// Install SIGTERM/SIGINT handlers that set the shared shutdown flag.
/// Called once before any render path so the handler is active during
/// Vulkan init (which can take several seconds on DRM hardware).
fn install_signal_handlers() {
    #[cfg(target_os = "linux")]
    {
        extern "C" fn on_signal(_: libc::c_int) {
            vstimd::shutdown::request();
        }
        unsafe {
            libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
            libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
        }
    }
}

/// Block until the ZMQ thread signals that `socket.bind()` has succeeded.
/// Returns `true` if the signal arrived, `false` on timeout (ZMQ unavailable).
fn wait_zmq_bound(rx: &std::sync::mpsc::Receiver<()>, port: u16) -> bool {
    if rx.recv_timeout(std::time::Duration::from_secs(10)).is_err() {
        log::warn!(
            "vstimd: ZMQ bind did not complete within 10 s — port {port} may not be listening"
        );
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
            Ok(()) => {
                log::info!("vstimd: sd_notify: NOTIFY_SOCKET not set (not running under systemd)")
            }
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
    eprintln!("      --rig-config <path>   Rig config (default: {})", vstimd::rig_config::DEFAULT_PATH);
    eprintln!("      --config <path>       Load stim-config file at startup");
    eprintln!("      --config-dir <path>   Directory for named stim-config files (default: .)");

    eprintln!("  -h, --help                Show this help message");
    eprintln!();
    eprintln!("Render target is automatically detected:");
    eprintln!("  - Windows/macOS: desktop (winit)");
    eprintln!("  - Linux with DISPLAY or WAYLAND_DISPLAY: desktop (winit)");
    eprintln!("  - Linux without display server: console (DRM/KMS)");
}
