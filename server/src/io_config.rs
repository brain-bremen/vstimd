use crate::scene::SceneConfig;
use crate::vtl_state::VtlConfig;

pub const CONFIG_VERSION: u32 = 1;

/// Borrowed view of I/O config assembled at save time — never stored.
#[derive(serde::Serialize)]
pub struct IoConfigRef<'a> {
    pub vtl: &'a VtlConfig,
}

/// Owned I/O config populated at load time — each field moved to its subsystem owner.
#[derive(serde::Deserialize, Default)]
pub struct IoConfigFile {
    #[serde(default)]
    pub vtl: VtlConfig,
}

/// Borrowed top-level view — used only during save. No allocation or copies.
#[derive(serde::Serialize)]
struct ConfigFileRef<'a> {
    version: u32,
    scene:   &'a SceneConfig,
    io:      IoConfigRef<'a>,
}

/// Owned top-level struct — used only during load. Fields are moved to their owners.
#[derive(serde::Deserialize)]
struct ConfigFile {
    version: u32,
    scene:   SceneConfig,
    io:      IoConfigFile,
}

/// Serialize current state to pretty JSON without touching the filesystem.
pub fn retrieve_config_json(scene: &SceneConfig, vtl: &VtlConfig) -> anyhow::Result<String> {
    let view = ConfigFileRef {
        version: CONFIG_VERSION,
        scene,
        io: IoConfigRef { vtl },
    };
    Ok(serde_json::to_string_pretty(&view)?)
}

/// Write scene + I/O config to a file.
pub fn save_config(scene: &SceneConfig, vtl: &VtlConfig, path: &std::path::Path) -> anyhow::Result<()> {
    let json = retrieve_config_json(scene, vtl)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Parse and validate a config JSON string without touching the filesystem.
/// Used by both `load_config` and `UploadConfig` validation.
pub fn parse_config_json(s: &str) -> anyhow::Result<(SceneConfig, IoConfigFile)> {
    let f: ConfigFile = serde_json::from_str(s)?;
    anyhow::ensure!(
        f.version == CONFIG_VERSION,
        "Unsupported config version {} (expected {})",
        f.version,
        CONFIG_VERSION
    );
    Ok((f.scene, f.io))
}

/// Read a config file from disk and parse it.
pub fn load_config(path: &std::path::Path) -> anyhow::Result<(SceneConfig, IoConfigFile)> {
    let s = std::fs::read_to_string(path)?;
    parse_config_json(&s)
}

/// List bare config names (no path, no extension) from a config directory.
pub fn list_config_names(dir: &std::path::Path) -> anyhow::Result<Vec<String>> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut names = vec![];
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(bare) = name.strip_suffix(".config.json") {
            names.push(bare.to_string());
        }
    }
    names.sort();
    Ok(names)
}

pub fn is_io_error(e: &anyhow::Error) -> bool {
    e.downcast_ref::<std::io::Error>().is_some()
}

pub fn is_not_found(e: &anyhow::Error) -> bool {
    e.downcast_ref::<std::io::Error>()
        .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
}

pub fn is_format_error(e: &anyhow::Error) -> bool {
    e.downcast_ref::<serde_json::Error>().is_some()
}
