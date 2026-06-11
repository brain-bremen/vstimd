# Scene Serialization / Deserialization

## Context

The server currently holds all scene state in RAM only. This change adds versioned JSON config files
that bundle the scene (stimuli, animations, background, photodiode) **and** I/O configuration (VTL
channel names; extensible to gamepad/key mappings later) in a single file. Files can be saved/loaded
via the egui overlay (pure egui — no native file dialog — so it works in DRM/headless mode) and via
`--config <path>` at startup. Two scene load modes: **replace** (clear then restore) and **additive**
(merge into existing scene with handle remapping). The I/O section is always fully replaced on load.

---

## Architecture

Both `SceneState` and `VtlState` follow the same pattern: a serializable config sub-struct is
embedded directly, with `Deref<Target = ConfigType>` so all existing field access continues to
compile unchanged. The config structs are the canonical owners of their data — nothing is ever
copied between them for save/load.

### `SceneState`

```rust
// scene/config.rs  (new file)
#[derive(Clone, Serialize, Deserialize)]
pub struct SceneConfig {
    pub background:       Deferred<[f32; 4]>,
    pub default_fill:     [f32; 4],
    pub default_outline:  [f32; 4],
    pub photodiode:       PhotoDiodeState,
    pub stimuli:          IndexMap<u32, StimulusEntry>,
    pub next_stim_handle: u32,
    pub animations:       IndexMap<u32, AnimationEntry>,
    pub next_anim_handle: u32,
    // No `io` field — I/O config lives in VtlState
}
```

```rust
pub struct SceneRuntimeState {
    pub deferred_mode:      bool,
    pub pending_flip:       bool,
    pub frame_rate:         f32,
    pub screen_size:        Option<(u32, u32)>,
    pub last_uploaded_size: (u32, u32),
    pub error_mask:         u16,
    pub error_code:         i16,
    pub command_log:        VecDeque<CommandEntry>,
    pub command_log_total:  u64,
    pub command_log_errors: u64,
    pub server_start:       Instant,
    pub frame_count:        u64,
    pub frame_notifier:     Arc<tokio::sync::watch::Sender<u64>>,
}

pub struct SceneState {
    pub config:  SceneConfig,
    pub runtime: SceneRuntimeState,
}

impl Deref    for SceneState { type Target = SceneConfig; fn deref    (&self)    -> &SceneConfig { &self.config } }
impl DerefMut for SceneState {                             fn deref_mut(&mut self) -> &mut SceneConfig { &mut self.config } }
```

### `VtlState`

`IoConfig` is a container with one sub-config per I/O subsystem. Each subsystem owns its config
directly; `IoConfig` only exists transiently during file I/O (as a borrowed view for save, as an
owned struct for load).

```rust
// src/io_config.rs  (new file)

/// VTL-specific config — owned by VtlState.
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct VtlConfig {
    pub vtl_names: Vec<VtlNameEntry>,
}

/// Borrowed view assembled at save time — never stored.
#[derive(Serialize)]
pub struct IoConfigRef<'a> {
    pub vtl: &'a VtlConfig,
    // future: pub keyboard: &'a KeyboardConfig,
    // future: pub gamepad:  &'a GamepadConfig,
}

/// Owned struct populated at load time — each field moved to its subsystem owner.
#[derive(Deserialize, Default)]
pub struct IoConfigFile {
    #[serde(default)]
    pub vtl: VtlConfig,
    // future: pub keyboard: KeyboardConfig,
    // future: pub gamepad:  GamepadConfig,
}
```

```rust
// vtl_state.rs  (additions)
pub struct VtlState {
    pub config:  VtlConfig,        // serializable — canonical VTL config
    owner:       VtlOwner,
    prev_input:  [u64; MAX_BANKS],
    prev_output: [u64; MAX_BANKS],
}

impl Deref    for VtlState { type Target = VtlConfig; fn deref    (&self)    -> &VtlConfig { &self.config } }
impl DerefMut for VtlState {                           fn deref_mut(&mut self) -> &mut VtlConfig { &mut self.config } }
```

Existing `vtl.vtl_names` access continues to work via `DerefMut`. The `names` field (previously a
bare `Vec<VtlNameEntry>` on `VtlState`) moves into `VtlConfig`.

---

## File Naming Convention

Files saved from vstimd follow the pattern `vstimd_<name>.<type>.ext`:

| File | Convention | Example |
|------|-----------|---------|
| Config (scene + I/O) | `vstimd_<name>.config.json` | `vstimd_motion_exp.config.json` |
| Archive (config + assets) | `vstimd_<name>.archive.zip` | `vstimd_motion_exp.archive.zip` |
| Event log | `vstimd_<name>.events.sqlite` | `vstimd_motion_exp.events.sqlite` |

The egui file browser enforces this when saving. The ZMQ protocol accepts any path.

---

## File Format & I/O

The JSON file has three top-level keys: `version`, `scene`, and `io`. `io` is itself an object
with one key per subsystem (`vtl`, and future `keyboard`, `gamepad`, …).

Two thin types handle serialization — neither is stored; they exist only on the call stack:

```rust
// src/io_config.rs  (additions)
pub const CONFIG_VERSION: u32 = 1;

/// Borrowed view — used only during save. No allocation or copies.
#[derive(Serialize)]
struct ConfigFileRef<'a> {
    version: u32,
    scene:   &'a SceneConfig,
    io:      IoConfigRef<'a>,
}

/// Owned — used only during load. Fields are moved to their subsystem owners.
#[derive(Deserialize)]
struct ConfigFile {
    version: u32,
    scene:   SceneConfig,
    io:      IoConfigFile,
}

pub fn save_config(scene: &SceneConfig, vtl: &VtlConfig, path: &Path) -> std::io::Result<()> {
    let view = ConfigFileRef {
        version: CONFIG_VERSION,
        scene,
        io: IoConfigRef { vtl },
    };
    std::fs::write(path, serde_json::to_string_pretty(&view)?)
}

pub fn load_config(path: &Path) -> anyhow::Result<(SceneConfig, IoConfigFile)> {
    let s = std::fs::read_to_string(path)?;
    parse_config_json(&s)
}

/// Parse and validate a config JSON string without touching the filesystem.
/// Used by both `load_config` and `UploadConfig` validation.
pub fn parse_config_json(s: &str) -> anyhow::Result<(SceneConfig, IoConfigFile)> {
    let f: ConfigFile = serde_json::from_str(s)?;
    anyhow::ensure!(f.version == CONFIG_VERSION,
        "Unsupported config version {} (expected {})", f.version, CONFIG_VERSION);
    Ok((f.scene, f.io))
}
```

**Save** — borrows in place, no copies:
```rust
save_config(&scene.read().config, &vtl.lock().config, &path)?;
```

**Load** — moves each piece to its owner:
```rust
let (scene_cfg, io) = load_config(&path)?;
vtl.lock().config = io.vtl;          // move, not copy
vtl.lock().sync_names_to_shm();
scene.write().load_snapshot(scene_cfg, load_mode);
// future: keyboard.lock().config = io.keyboard;
```

---

## Load Modes

```rust
pub enum LoadMode { Replace, Additive }

impl SceneState {
    pub fn load_snapshot(&mut self, cfg: SceneConfig, mode: LoadMode) {
        match mode {
            LoadMode::Replace => {
                self.config = cfg;
                self.fixup_after_load();
            }
            LoadMode::Additive => {
                let stim_offset = self.config.next_stim_handle;
                let anim_offset = self.config.next_anim_handle;
                for (handle, entry) in cfg.stimuli {
                    self.config.stimuli.insert(handle + stim_offset, entry_dirty(entry));
                }
                for (handle, mut anim) in cfg.animations {
                    for sh in &mut anim.stimuli { *sh += stim_offset; }
                    anim.state = AnimState::Idle;
                    self.config.animations.insert(handle + anim_offset, anim);
                }
                self.config.next_stim_handle += cfg.next_stim_handle;
                self.config.next_anim_handle += cfg.next_anim_handle;
                // background/photodiode not merged in additive mode
            }
        }
    }

    fn fixup_after_load(&mut self) {
        for entry in self.config.stimuli.values_mut() {
            entry.stimulus.flags_mut().dirty = true;
            entry.stimulus.reset_phase_accum();
            entry.stimulus.make_copy();
        }
        for anim in self.config.animations.values_mut() {
            anim.state = AnimState::Idle;
            anim.captured_user_enabled = None;
        }
        self.config.background.make_copy();
        self.config.photodiode.make_copy();
    }
}
```

---

## `Deferred<T>` — transparent in JSON

Custom serde that serializes only `live` and deserializes into both halves:

```rust
impl<T: Serialize + Copy + Default> Serialize for Deferred<T> {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error> { self.live.serialize(s) }
}
impl<'de, T: Deserialize<'de> + Copy + Default> Deserialize<'de> for Deferred<T> {
    fn deserialize<D>(d: D) -> Result<Self, D::Error> {
        let v = T::deserialize(d)?;
        Ok(Deferred { live: v, copy: v })
    }
}
```

---

## Serde Derives — Files to Touch

Add `#[derive(Serialize, Deserialize)]` to:

| File | Types |
|------|-------|
| `scene/deferred.rs` | `Deferred<T>` — custom serde (live only, see above) |
| `scene/stimulus/stimulus_flags.rs` | `StimulusFlags` — skip `dirty`; restore with `dirty: true` |
| `scene/stimulus/transform2d.rs` | `Transform2D` |
| `scene/stimulus/shape_appearance.rs` | `ShapeAppearance`, `DrawMode` |
| `scene/stimulus/primitive_shapes.rs` | `RectStimulus`, `CircleStimulus`, `EllipseStimulus` |
| `scene/stimulus/grating/grating_params.rs` | `GratingParams`, `Waveform`, `GratingMask` |
| `scene/stimulus/grating/grating_stimulus.rs` | `GratingStimulus` — `#[serde(skip, default)]` on `phase_accum` |
| `scene/stimulus/text/text_params.rs` | `TextRenderParams`, `Anchor`, `LanguageStyle` |
| `scene/stimulus/text/text_stimulus.rs` | `TextStimulus` — `#[serde(skip, default)]` on `text_copy` |
| `scene/stimulus/mod.rs` | `Stimulus`, `ShapeStimulus` |
| `scene/stimulus/entry.rs` (or mod.rs) | `StimulusEntry` |
| `scene/animation.rs` | `Animation`, `AnimState`, `StartAction`, `FinalAction`, `Edge`, `AnimationEntry` |
| `scene/photodiode.rs` | `PhotoDiodeState` |
| `vtl_state.rs` | `VtlConfig`, `VtlNameEntry`, `VtlBit`, `Edge` |

`bitflags!` types (`StartAction`, `FinalAction`) — `#[serde(transparent)]` wrapping u8.
`uuid` needs the `"serde"` feature. `indexmap` needs the `"serde"` feature.

If `vtl::Direction` lacks serde, use a remote derive:
```rust
#[derive(Serialize, Deserialize)]
#[serde(remote = "vtl::Direction")]
enum DirectionDef { In, Out }
```

---

## CLI `--config <path>` (main.rs)

```rust
struct Args {
    render_target:  RenderTarget,
    verbose:        bool,
    config_file:    Option<PathBuf>,  // --config <path>  arbitrary startup file
    config_dir:     Option<PathBuf>,  // --config-dir <path>  managed folder (default: platform-specific)
}
```

After `SceneState::new()`, before `spawn_zmq_thread()`:
```rust
if let Some(ref path) = args.config_file {
    match load_config(path) {
        Ok((scene_cfg, io)) => {
            if let Some(ref vtl) = vtl {
                let mut v = vtl.lock().unwrap();
                v.config = io.vtl;
                v.sync_names_to_shm();
            }
            scene.write().unwrap().load_snapshot(scene_cfg, LoadMode::Replace);
        }
        Err(e) => log::error!("Failed to load config {path:?}: {e}"),
    }
}
```

---

## egui File Browser (`render/file_browser.rs` — new file)

A self-contained egui modal, no native OS dialog, works in DRM mode.

```rust
pub enum BrowserMode { Save, OpenReplace, OpenAdditive }

pub struct FileBrowser {
    pub open: bool,
    mode: BrowserMode,
    current_dir: PathBuf,
    entries: Vec<DirEntry>,    // sorted: dirs first, then *.config.json
    filename: String,
    pub result: Option<(BrowserMode, PathBuf)>,
}
```

UI layout (inside `egui::Window`):
- Path breadcrumbs (clickable directory segments)
- Scrollable `egui::Grid` of entries: `📁 dirname` / `📄 vstimd_name.config.json`
- `TextEdit` for filename (auto-appends `vstimd_` prefix and `.config.json` suffix on save)
- `[Open]` / `[Save]` + `[Cancel]` buttons

File filter: `*.config.json` (parent dirs always shown).

---

## egui Overlay Integration (`render/overlay.rs`)

New `egui::Window::new("Config")`:

```rust
if let Some((mode, path)) = args.file_browser.take_result() {
    match mode {
        BrowserMode::Save => {
            let default_vtl = VtlConfig::default();
            let vtl_cfg = args.vtl.as_ref().map(|v| &v.lock().unwrap().config).unwrap_or(&default_vtl);
            if let Err(e) = save_config(&scene.read().unwrap().config, vtl_cfg, &path) {
                log::error!("{e}");
            }
        }
        BrowserMode::OpenReplace | BrowserMode::OpenAdditive => {
            let load_mode = if matches!(mode, BrowserMode::OpenReplace) {
                LoadMode::Replace } else { LoadMode::Additive };
            match load_config(&path) {
                Ok((scene_cfg, io)) => {
                    if let Some(vtl) = args.vtl.as_ref() {
                        let mut v = vtl.lock().unwrap();
                        v.config = io.vtl;
                        v.sync_names_to_shm();
                    }
                    scene.write().unwrap().load_snapshot(scene_cfg, load_mode);
                }
                Err(e) => log::error!("{e}"),
            }
        }
    }
}
```

`OverlayArgs` gains `file_browser: &mut FileBrowser` and `vtl: Option<&Arc<Mutex<VtlState>>>`.

---

## Cargo.toml Changes

```toml
serde      = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow     = "1"
uuid       = { version = "1", features = ["v4", "serde"] }
indexmap   = { version = "2", features = ["serde"] }
```

---

## Files Created / Modified

**New files:**
- `server/src/scene/config.rs` — `SceneConfig`, `SceneRuntimeState`
- `server/src/io_config.rs` — `VtlConfig`, `IoConfigRef`, `IoConfigFile`, `CONFIG_VERSION`, `save_config`, `load_config`, `parse_config_json`, `retrieve_config_json`
- `server/src/render/file_browser.rs` — egui file browser widget
- `server/tests/config_roundtrip.rs` — round-trip unit tests for all stimulus/animation/IO types
- `server/tests/config_compat.rs` — backwards-compatibility tests against reference files
- `server/tests/configs/vstimd_reference_v1.config.json` — committed once v1 format is finalised

**Modified files (main changes):**
- `server/Cargo.toml` — add serde, serde_json, anyhow; add features to uuid + indexmap
- `server/src/main.rs` — `--config` arg + startup load
- `server/src/scene/mod.rs` — expose `config` module; re-export key types + `LoadMode`
- `server/src/scene/state.rs` — restructure `SceneState` (embed `config: SceneConfig`, `runtime: SceneRuntimeState`), add `Deref`, `load_snapshot`, `fixup_after_load`
- `server/src/scene/deferred.rs` — custom `Serialize`/`Deserialize` for `Deferred<T>`
- `server/src/scene/animation.rs` — serde derives on all animation types
- `server/src/scene/photodiode.rs` — serde derives
- `server/src/scene/stimulus/{mod,entry,stimulus_flags,transform2d,shape_appearance,primitive_shapes}.rs` — serde derives
- `server/src/scene/stimulus/grating/{grating_params,grating_stimulus}.rs` — serde derives
- `server/src/scene/stimulus/text/{text_params,text_stimulus}.rs` — serde derives
- `server/src/vtl_state.rs` — add `VtlConfig`, split into `config: VtlConfig` + runtime fields; serde derives on `VtlConfig`, `VtlNameEntry`, `VtlBit`, `Edge`; add `Deref<Target=VtlConfig>` for `VtlState`
- `server/src/render/overlay.rs` — Config window + file browser integration
- `server/src/render/mod.rs` — expose `file_browser` module
- `proto/vstimd/v1/service.proto` — add 4 new request fields to `Request.body`, 2 new response body variants, 5 new `ErrorCode` values
- `proto/vstimd/v1/system.proto` — define `ListConfigsRequest/Response`, `LoadConfigRequest`, `UploadConfigRequest`, `RetrieveConfigRequest/Response`
- `server/src/scene/command.rs` — dispatch `SaveConfig` and `LoadConfig` arms in `handle_system_command`

---

## Config Directory

The server maintains a managed config folder. Location is set by `--config-dir <path>` (new CLI
flag). Sensible defaults:

- **Linux interactive/daemon**: `/etc/vstimd/configs/` (daemon) or `~/.config/vstimd/configs/`
- **Windows**: `%APPDATA%\vstimd\configs\`

`SceneState` (or a top-level server struct) holds the resolved `config_dir: PathBuf` at runtime.
All folder-based proto commands operate within this directory; filenames are bare names
(e.g. `motion_exp`), the `.config.json` suffix is appended/stripped by the server.

The `--config <path>` startup flag (arbitrary path, handled in `main.rs`) is separate and
independent of `--config-dir`.

---

## Proto: Config Persistence Commands

### New messages (`system.proto`)

```protobuf
// List all configs available in the server's config directory.
// Returns a ListConfigsResponse.
message ListConfigsRequest {}

message ListConfigsResponse {
  repeated string names = 1;  // bare names, no path or extension
}

// Load a named config from the server's config directory and apply it.
// additive=false: clear existing scene then restore (default).
// additive=true:  merge stimuli/animations; I/O config always fully replaced.
message LoadConfigRequest {
  string name     = 1;
  bool   additive = 2;
}

// Save a config (supplied as JSON) to the server's config directory.
// apply_now=true also applies the config immediately after saving.
// overwrite=false returns ERROR_CODE_FILE_ALREADY_EXISTS if the name exists.
message UploadConfigRequest {
  string name      = 1;
  string json      = 2;
  bool   overwrite = 3;
  bool   apply_now = 4;
  bool   additive  = 5;  // only used when apply_now=true
}

// Return the current scene + I/O config as a JSON string.
// The client can inspect it, save it locally, or re-upload it.
message RetrieveConfigRequest {}

message RetrieveConfigResponse {
  string json = 1;
}
```

### New fields in `Request.body` oneof (`service.proto`, system target)

```protobuf
// ── Config persistence (system target) ─────────────────────────────────────
ListConfigsRequest   list_configs    = 110;
LoadConfigRequest    load_config     = 111;
UploadConfigRequest  upload_config   = 112;
RetrieveConfigRequest retrieve_config = 113;
```

### New response body variant

```protobuf
// In Response.body oneof:
ListConfigsResponse   config_list        = 17;
RetrieveConfigResponse retrieved_config  = 18;
```

### New error codes (`service.proto`)

```protobuf
ERROR_CODE_FILE_NOT_FOUND      = 10;  // named config does not exist
ERROR_CODE_FILE_IO             = 11;  // permission denied, disk full, etc.
ERROR_CODE_FILE_FORMAT         = 12;  // invalid JSON or schema mismatch
ERROR_CODE_UNSUPPORTED_VERSION = 13;  // version field not supported
ERROR_CODE_FILE_ALREADY_EXISTS = 14;  // upload collision with overwrite=false
```

### Dispatch (`command.rs`)

```rust
request::Body::ListConfigs(_) => {
    let names = list_config_names(&self.config_dir)?;
    ok_body(proto::ListConfigsResponse { names })
}
request::Body::LoadConfig(r) => {
    let path = self.config_dir.join(format!("{}.config.json", r.name));
    match load_config(&path) {
        Ok((scene_cfg, io)) => {
            if let Some(vtl) = vtl {
                vtl.config = io.vtl;
                vtl.sync_names_to_shm();
            }
            let mode = if r.additive { LoadMode::Additive } else { LoadMode::Replace };
            self.load_snapshot(scene_cfg, mode);
            ok_ack()
        }
        Err(e) if is_not_found(&e) => err(ErrorCode::FileNotFound, &e.to_string()),
        Err(e) if is_format(&e)    => err(ErrorCode::FileFormat, &e.to_string()),
        Err(e)                     => err(ErrorCode::FileIo, &e.to_string()),
    }
}
request::Body::UploadConfig(r) => {
    // Validate before touching disk: parse JSON and check version + schema.
    let (scene_cfg, io) = match parse_config_json(&r.json) {
        Ok(v)  => v,
        Err(e) => return err(ErrorCode::FileFormat, &e.to_string()),
    };
    let path = self.config_dir.join(format!("{}.config.json", r.name));
    if path.exists() && !r.overwrite {
        return err(ErrorCode::FileAlreadyExists, "config already exists");
    }
    if let Err(e) = std::fs::write(&path, &r.json) {
        return err(ErrorCode::FileIo, &e.to_string());
    }
    if r.apply_now {
        if let Some(vtl) = vtl { vtl.config = io.vtl; vtl.sync_names_to_shm(); }
        let mode = if r.additive { LoadMode::Additive } else { LoadMode::Replace };
        self.load_snapshot(scene_cfg, mode);
    }
    ok_ack()
}
request::Body::RetrieveConfig(_) => {
    let default_vtl = VtlConfig::default();
    let vtl_cfg = vtl.as_ref().map(|v| &v.config).unwrap_or(&default_vtl);
    match retrieve_config(&self.config, vtl_cfg) {
        Ok(json) => ok_body(proto::RetrieveConfigResponse { json }),
        Err(e)   => err(ErrorCode::Unknown, &e.to_string()),
    }
}
```

`config_dir` is added to `SceneState` (or a separate server context struct passed alongside it).
`retrieve_config` serializes the current state using `ConfigFileRef` — no copies.

---

## Future: Config Schema

When the config format stabilises, expose a JSON Schema so clients can validate configs locally
before uploading and editors can provide IntelliSense.

Preferred approach: **`schemars`** — add `#[derive(JsonSchema)]` alongside `Serialize/Deserialize`
on `SceneConfig`, `IoConfigFile`, and all nested types. The schema is then generated at build time
or on demand. A new proto command exposes it:

```protobuf
message GetConfigSchemaRequest {}
message GetConfigSchemaResponse { string json_schema = 1; }
```

This keeps the schema in sync with the Rust types automatically. No manual schema maintenance.

`UploadConfigRequest` can optionally enforce schema validation using the generated schema before
the `parse_config_json` step (strict field validation beyond what serde gives by default).

---

## Future: Archive Format (`vstimd_<name>.archive.zip`)

When stimuli reference external assets, a future archive will bundle:
- `vstimd_<name>.config.json` (scene + I/O config, same format)
- All referenced asset files under `assets/` inside the ZIP

Asset paths stored as relative `Option<PathBuf>` on future stimulus types. JSON format unchanged.

---

## Tests

### Round-trip unit tests (`server/tests/config_roundtrip.rs`)

Integration tests (no GPU, no ZMQ) that call `SceneState` directly via the library crate:

```rust
#[test]
fn roundtrip_rect_stimulus() {
    let mut scene = SceneState::new();
    // build a known scene: rect with specific position, color, name
    scene.add_stimulus(StimulusEntry { ... });

    let vtl_cfg = VtlConfig::default();
    let json = retrieve_config_json(&scene.config, &vtl_cfg).unwrap();
    let (loaded_scene, _io) = parse_config_json(&json).unwrap();

    assert_eq!(loaded_scene.stimuli.len(), 1);
    let entry = loaded_scene.stimuli.values().next().unwrap();
    // check position, appearance, name survive the round-trip
    assert_eq!(entry.name, Some("my_rect".into()));
    // ...
}

#[test]
fn roundtrip_vtl_names() {
    let vtl_cfg = VtlConfig {
        vtl_names: vec![VtlNameEntry { name: "stim_onset".into(), bank: 0, bit: 0, direction: Direction::Out }],
    };
    let json = retrieve_config_json(&SceneConfig::default(), &vtl_cfg).unwrap();
    let (_scene, io) = parse_config_json(&json).unwrap();
    assert_eq!(io.vtl.vtl_names[0].name, "stim_onset");
}

#[test]
fn roundtrip_animation() { ... }   // animation + stimulus handle references survive

#[test]
fn additive_load_remaps_handles() {
    // load a scene, then additive-load a second one and verify no handle collision
}
```

### Backwards compatibility tests (`server/tests/config_compat.rs`)

One test per reference file that loads it and checks a fixed set of known values:

```rust
#[test]
fn load_v1_reference() {
    let (scene, io) = load_config(Path::new("tests/configs/vstimd_reference_v1.config.json")).unwrap();
    assert_eq!(scene.stimuli.len(), 3);
    // spot-check known values from the reference file
    assert_eq!(scene.background.live, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(io.vtl.vtl_names[0].name, "stim_onset");
    // ...
}
```

If a future refactor breaks deserialisation of a reference file, this test fails — making format
changes an explicit, visible decision rather than a silent regression.

### Reference files (`server/tests/configs/`)

Once the v1 format is finalised, commit one hand-verified reference file:

```
server/tests/configs/vstimd_reference_v1.config.json
```

It should exercise all stimulus types (rect, circle, ellipse, grating, text), all animation types,
at least one VTL name, and non-default background/photodiode state. Keep it human-readable and
commented where JSON allows (it doesn't — use a companion `vstimd_reference_v1.notes.md` if
needed). When a new version is introduced, add a new reference file alongside the old one; never
delete old reference files.

---

## Verification

1. `cargo build --release` — no compile errors
2. `cargo clippy` — no new warnings
3. Start server (null renderer); create stimuli + animations; set VTL names via Python client; open overlay → "Save…" → inspect `vstimd_test.config.json` (check `version=1`, `scene.*` and `io.vtl_names` present)
4. Restart with `--config <file>` → stimuli reappear; VTL names restored
5. Load via overlay (replace) → scene replaced, retessellation triggered
6. Load via overlay (additive) → stimuli appended, no handle collisions
7. Animation stimulus handle references remapped correctly in additive mode
8. `SaveConfigRequest` via Python client → file written, valid JSON
9. `LoadConfigRequest` with bad path → `ERROR_CODE_FILE_NOT_FOUND`; bad JSON → `ERROR_CODE_FILE_FORMAT`
10. `make test-e2e-null` — existing tests pass
