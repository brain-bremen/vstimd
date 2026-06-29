/// Machine-specific configuration loaded at startup from `rig-config.toml`.
///
/// Unlike `stim-config` (scene + named VTL lines, changed per experiment),
/// `rig-config` describes the physical rig and changes only when the hardware
/// is reconfigured:
///
///   - VTL shared-memory parameters (shm name, bank counts, vblank trigger bit)
///   - Display preferences for DRM/console mode (resolution, refresh rate)
///   - Thread scheduling options (CPU affinity, real-time priorities)
///
/// Default path: `/etc/braemons/vstimd-rig-config.toml`
/// Override with the `--rig-config` flag.
///
/// If the file is absent vstimd falls back to built-in defaults and logs a
/// notice — useful for development machines without a full rig setup.
use crate::vtl_state::VtlBit;

pub const DEFAULT_PATH: &str = "/etc/braemons/vstimd-rig-config.toml";
const EXAMPLES_DIR: &str = "/usr/share/braemons/vstimd/";

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct RigConfig {
    #[serde(default)]
    pub vtl: VtlRigConfig,
    #[serde(default)]
    pub display: DisplayRigConfig,
    #[serde(default)]
    pub scheduling: SchedulingRigConfig,
    #[serde(default)]
    pub web: WebRigConfig,
}

/// Embedded web control surface (HTTP + WebSocket) settings.
///
/// The web server can also be compiled out entirely via the `web` Cargo feature
/// (on by default). When the feature is disabled these fields are ignored.
/// CLI flags (`--no-web`, `--web-port`) override these values.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct WebRigConfig {
    /// Whether to start the web control surface. Default: true.
    #[serde(default = "WebRigConfig::default_enabled")]
    pub enabled: bool,
    /// HTTP/WebSocket port. Default: 8080.
    #[serde(default = "WebRigConfig::default_port")]
    pub port: u16,
}

impl WebRigConfig {
    fn default_enabled() -> bool { true }
    fn default_port() -> u16 { 8080 }
}

impl Default for WebRigConfig {
    fn default() -> Self {
        Self { enabled: Self::default_enabled(), port: Self::default_port() }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct VtlRigConfig {
    /// POSIX shared-memory name for the VTL segment (must start with `/`).
    #[serde(default = "VtlRigConfig::default_shm_name")]
    pub shm_name: String,
    /// Number of 64-bit input banks (1–4).  Each bank holds 64 input lines.
    /// 1 is sufficient for up to 64 physical trigger inputs.
    #[serde(default = "VtlRigConfig::default_input_banks")]
    pub num_input_banks: u32,
    /// Number of 64-bit output banks (1–4).
    #[serde(default = "VtlRigConfig::default_output_banks")]
    pub num_output_banks: u32,
    /// Output bit pulsed HIGH at the start of each frame (immediately after the
    /// vblank wait) and LOW once the GPU work is submitted.  The pulse width is
    /// vstimd's per-frame compute time.  Omit to disable.
    ///
    /// Choose a bit not used by any gpiochip-daqd output line so there is no
    /// conflict.  Bit 63 on bank 0 is a safe default.
    pub vblank: Option<VtlBit>,
}

impl VtlRigConfig {
    fn default_shm_name() -> String  { "/vstimd_vtl".into() }
    fn default_input_banks() -> u32  { 1 }
    fn default_output_banks() -> u32 { 1 }
}

impl Default for VtlRigConfig {
    fn default() -> Self {
        Self {
            shm_name:         Self::default_shm_name(),
            num_input_banks:  Self::default_input_banks(),
            num_output_banks: Self::default_output_banks(),
            vblank:           None,
        }
    }
}

/// Preferred display mode for DRM/console output.
///
/// All fields are optional.  Omit a field to let vstimd auto-select from the
/// display's EDID-reported preferred mode.  Useful when the display's preferred
/// mode differs from the experiment's target refresh rate.
///
/// Note: wiring into DRM mode selection is not yet implemented — these fields
/// are parsed and logged but not yet applied.  See Todo.md.
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct DisplayRigConfig {
    /// Preferred horizontal resolution (pixels).
    pub width: Option<u32>,
    /// Preferred vertical resolution (pixels).
    pub height: Option<u32>,
    /// Preferred refresh rate (Hz), e.g. `60.0` or `144.0`.
    pub refresh_hz: Option<f64>,
}

/// Thread scheduling options for vstimd.
///
/// All fields are parsed but not yet applied — CPU affinity wiring is planned.
#[derive(Debug, Clone, serde::Deserialize, Default)]
#[allow(dead_code)]
pub struct SchedulingRigConfig {
    /// CPU core to pin the render/vblank thread to.  Not yet applied.
    pub render_cpu_core: Option<usize>,
}

/// Load a rig-config from `path`.  Returns `Ok(RigConfig::default())` if the
/// file does not exist (non-fatal), or an error if the file exists but is
/// malformed.
pub fn load(path: &str) -> anyhow::Result<RigConfig> {
    match std::fs::read_to_string(path) {
        Ok(raw) => {
            let cfg: RigConfig = toml::from_str(&raw)
                .map_err(|e| anyhow::anyhow!("rig-config {path}: {e}"))?;
            Ok(cfg)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            log::info!(
                "rig-config not found at {path} — using built-in defaults. \
                 Copy a board example from {EXAMPLES_DIR} to customise."
            );
            Ok(RigConfig::default())
        }
        Err(e) => Err(anyhow::anyhow!("rig-config {path}: {e}")),
    }
}
