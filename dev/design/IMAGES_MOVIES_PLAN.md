# Images & Movies Plan

Extends Phase 9 (Bitmap) and Phase 12 (Video) from `PLAN.md` with a **file-upload
protocol** so clients on remote machines can send image and movie data to the server
without requiring a shared filesystem.

---

## Overview

The existing plan for bitmaps and video uses `string path` — the server reads from
its local filesystem. That works when client and server share a filesystem (e.g.
localhost dev), but not for the primary deployment target (PsychoPy workstation →
Jetson Nano). This plan adds an **asset store** with a chunked upload API over the
existing ZMQ REQ/REP socket, then builds bitmap, sequence, and video stimuli on top.

### Four phases

| Phase | Feature | New proto messages |
|---|---|---|
| A | Asset upload / store | `UploadAsset`, `DeleteAsset`, `ListAssets` |
| B | Bitmap stimulus | `CreateBitmap`, `SetBitmapSize` |
| C | Bitmap-sequence stimulus | `CreateBitmapSeq`, `SetBitmapSeqFrame`, `SetBitmapSeqFps` |
| D | Video stimulus (deferred) | `CreateVideo`, `SetVideoFrame`, `SetVideoFps` |

---

## Phase A — Asset Upload Protocol

### Motivation

Images and video files are binary blobs of up to tens of MB. The ZMQ REQ/REP
socket already carries all command traffic; we extend it with three system commands
so no new socket is needed. The client splits large files into ≤ 512 KB chunks and
sends each as a separate request-response exchange.

### New `proto/v1/assets.proto`

```proto
syntax = "proto3";
package vstimd.v1;

// System command: upload one chunk of a named asset.
// First chunk (offset == 0): server pre-allocates storage for total_size bytes.
// Subsequent chunks: data is written at the given byte offset.
// On completion (offset + len(data) == total_size): asset becomes available.
// Re-uploading a name that already exists replaces it.
message UploadAsset {
  string name       = 1;  // client-chosen identifier, e.g. "gabor.png"
  uint64 total_size = 2;  // declared full size in bytes (must be consistent)
  uint64 offset     = 3;  // byte offset where this chunk starts
  bytes  data       = 4;  // chunk payload (≤ 512 KB recommended)
}

// System command: remove a named asset from the in-memory store.
message DeleteAsset {
  string name = 1;
}

// System command: list all assets currently in the store.
// Returns AssetList in Response.body.
message ListAssets {}

// Response payload for ListAssets.
message AssetList {
  repeated AssetInfo assets = 1;
}

message AssetInfo {
  string name       = 1;
  uint64 total_size = 2;
  bool   complete   = 3;  // false while upload is in progress
}
```

### New error codes (add to `service.proto`)

```proto
ERROR_CODE_ASSET_NOT_FOUND  = 8;  // name not in store
ERROR_CODE_ASSET_INCOMPLETE = 9;  // upload not yet finished
ERROR_CODE_DECODE_FAILED    = 10; // image / video decode error
```

### Server-side `AssetStore` (`server/src/assets.rs`)

```rust
pub struct AssetStore {
    assets: HashMap<String, AssetEntry>,
}

struct AssetEntry {
    total_size: u64,
    data: Vec<u8>,   // pre-allocated on first chunk
}

impl AssetStore {
    pub fn upload_chunk(&mut self, name: &str, total_size: u64,
                        offset: u64, data: &[u8]) -> Result<bool, ErrorCode>;
    //                                            ^ true = complete

    pub fn get(&self, name: &str) -> Result<&[u8], ErrorCode>;
    // returns ASSET_NOT_FOUND or ASSET_INCOMPLETE

    pub fn delete(&mut self, name: &str);
    pub fn list(&self) -> Vec<AssetInfo>;
}
```

`AssetStore` lives inside `SceneState` (or alongside it) so the ZMQ handler can
reach it under the existing write lock. Assets are in-memory and lost on server
restart; this is intentional — clients reload them at session start.

### Python client: `vstimd/assets/_client.py`

```python
class AssetsClient:
    CHUNK = 512 * 1024  # 512 KB

    def upload(self, name: str, path: str | Path) -> None:
        data = Path(path).read_bytes()
        self.upload_bytes(name, data)

    def upload_bytes(self, name: str, data: bytes) -> None:
        total = len(data)
        for offset in range(0, total, self.CHUNK):
            chunk = data[offset : offset + self.CHUNK]
            self._conn._send(UploadAsset(
                name=name, total_size=total, offset=offset, data=chunk))

    def delete(self, name: str) -> None: ...
    def list(self) -> list[AssetInfo]: ...
```

`Connection` gains a `.assets` attribute alongside `.stimuli` and `.system`.

---

## Phase B — Bitmap Stimulus

### Proto (`stimuli.proto`)

```proto
// System command: create a single-image bitmap stimulus.
// The named asset must be fully uploaded before this command is sent.
// Returns the new handle in Response.handle.
message CreateBitmap {
  string asset_name = 1;  // must be complete in the asset store
  Vec2   center     = 2;
  float  width      = 3;  // display width in pixels  (0 = natural pixel width)
  float  height     = 4;  // display height in pixels (0 = natural pixel height)
  float  angle      = 5;  // initial rotation in degrees CCW
  float  opacity    = 6;  // 0.0–1.0 (0.0 treated as 1.0 = fully opaque)
}

// Stimulus command: resize a Bitmap stimulus.
message SetBitmapSize {
  float width  = 1;
  float height = 2;
}
```

Add `STIMULUS_TYPE_BITMAP_SEQ = 10` to `StimulusType` in `common.proto`.

### `service.proto` additions

```proto
// body oneof additions:
CreateBitmap   create_bitmap    = 13;  // system target
SetBitmapSize  set_bitmap_size  = 42;  // stimulus target
```

### Server-side implementation

**Command handler** (`scene/command.rs`):

1. Look up `asset_name` in `AssetStore`; error on missing/incomplete.
2. `image::load_from_memory(bytes)?.into_rgba8()` → `(width, height, rgba_data)`.
3. Upload to GPU: create `VkImage` (RGBA8 sRGB, sampled + transfer-dst) and fill via
   staging buffer.
4. Store `texture_id` in a `Vec<GpuTexture>` inside `RenderState` (not `SceneState` —
   textures are render-private).
5. Construct `BitmapStimulus { texture_id, size, … }` and insert into `SceneState`.

**GPU texture path** (additions to `render/vk/`):

- `GpuTexture`: `VkImage` + `VkImageView` + `VkDeviceMemory` + `VkSampler`.
- `GpuBuffers` gains `textures: Vec<GpuTexture>` and `alloc_texture(width, height,
  data) -> u32`.
- New `textured_pipeline`: vertex layout adds `uv: [f32; 2]`; fragment shader samples
  a `sampler2D` uniform (push-constant index selects texture from a descriptor array,
  or use one descriptor set per draw call if the device lacks descriptor indexing).

**Tessellation** (`render/vk/tess.rs`):

```
tessellate_bitmap(stim: &BitmapStimulus) -> [Vertex; 4]  // two-triangle quad
```

UV coordinates are `[0,0]`–`[1,1]`. Rotation is baked into vertex positions via the
`Transform2D` matrix (same as shape stimuli).

**`phi_inc` / `phi_accum`** in `BitmapStimulus` enable continuous rotation animations
(port of the C++ `CStimulusPic` spin parameter). Advance `phi_accum += phi_inc` each
frame before tessellation.

### Python client additions

```python
h = conn.stimuli.create_bitmap("gabor.png", x=0, y=0, width=256, height=256)
conn.stimuli.set_bitmap_size(h, 512, 512)
conn.stimuli.set_alpha(h, 0.5)
conn.stimuli.set_position(h, -100, 200)
conn.stimuli.set_orientation(h, 45.0)
```

---

## Phase C — Bitmap-Sequence Stimulus

Sequences are ordered lists of frames displayed at a configurable rate. Two creation
modes are supported:

- **Explicit frame list**: client uploads N assets and names them in order.
- **Multi-frame single asset**: client uploads one GIF/APNG/TIFF; server decodes all
  frames. Detected by `asset_names` containing a single entry whose decoded image has
  multiple frames (via `image::AnimationDecoder`).

### Proto (`stimuli.proto`)

```proto
// System command: create a bitmap-sequence (flipbook) stimulus.
message CreateBitmapSeq {
  repeated string asset_names = 1;  // ≥1 asset; multi-frame single asset also OK
  Vec2   center    = 2;
  float  width     = 3;   // 0 = natural
  float  height    = 4;   // 0 = natural
  float  fps       = 5;   // playback rate (0.0 = use asset frame rate or 30 fps)
  bool   loop      = 6;   // true = loop; false = hold last frame
  float  opacity   = 7;
}

// Stimulus command: jump to an absolute frame index (clamped to [0, n_frames-1]).
message SetBitmapSeqFrame {
  uint32 frame_index = 1;
}

// Stimulus command: change playback rate.
message SetBitmapSeqFps {
  float fps = 1;
}
```

Add `STIMULUS_TYPE_BITMAP_SEQ = 10` to `StimulusType`.

### `service.proto` additions

```proto
CreateBitmapSeq    create_bitmap_seq    = 14;  // system target
SetBitmapSeqFrame  set_bitmap_seq_frame = 43;  // stimulus target
SetBitmapSeqFps    set_bitmap_seq_fps   = 44;  // stimulus target
```

### Frame advance logic

`BitmapSeqStimulus` tracks `frac_counter` using integer arithmetic to avoid float
accumulation drift. Each frame:

```
frac_counter += rate_num;          // rate_num = fps (e.g. 30)
if frac_counter >= rate_den {      // rate_den = display rate (e.g. 60)
    frac_counter -= rate_den;
    frame_index = (frame_index + 1) % n_frames;
    if !loop && frame_index == 0 { frame_index = n_frames - 1; hold; }
}
```

`rate_num` / `rate_den` are set at creation from `fps / display_frame_rate` reduced
to smallest integers.

### Python client additions

```python
conn.assets.upload("f0.png", "frames/f0.png")
conn.assets.upload("f1.png", "frames/f1.png")
h = conn.stimuli.create_bitmap_seq(["f0.png", "f1.png"], x=0, y=0, fps=30.0)
conn.stimuli.set_bitmap_seq_frame(h, 0)    # jump to frame 0
conn.stimuli.set_bitmap_seq_fps(h, 10.0)   # slow down
```

---

## Phase D — Video Stimulus (deferred, expands Phase 12)

Video is the most complex case and should be implemented last.

### Approach

- Reuse the asset upload mechanism: client uploads the full video file as a named asset
  (same `UploadAsset` chunking).
- `CreateVideo` decodes using `ffmpeg-next` (Rust bindings to libffmpeg); this is the
  only reasonable option for broad codec coverage.
- Decoding runs on a **background thread per stimulus**, feeding an
  `Arc<Mutex<RingBuffer<Vec<u8>>>>` of RGBA frames.
- The render thread reads the current frame by presentation timestamp, uploads to a
  `VkImage` via a staging buffer each frame.

### Proto

```proto
message CreateVideo {
  string asset_name = 1;
  Vec2   center     = 2;
  float  width      = 3;   // 0 = natural
  float  height     = 4;   // 0 = natural
  float  fps        = 5;   // 0 = use asset frame rate
  bool   loop       = 6;
  float  opacity    = 7;
}

// Stimulus command: seek to a specific frame.
message SetVideoFrame {
  uint32 frame_index = 1;
}

// Stimulus command: override playback rate.
message SetVideoFps {
  float fps = 1;
}
```

Add `STIMULUS_TYPE_VIDEO = 11` to `StimulusType`.

### Implementation notes

- `VideoStimulus` is a new `Stimulus` variant (not `BitmapSeqStimulus`) because its
  render loop is fundamentally different: texture content changes every frame via DMA
  rather than swapping a descriptor set index.
- The ring buffer holds ~4 decoded frames to give the decode thread headroom.
- Seek (`SetVideoFrame`) signals the decode thread to flush and restart from the target
  keyframe; expect a brief stall.
- The `ffmpeg-next` crate requires `libavcodec` / `libavformat` to be installed on the
  server. Add this to deployment docs.
- On Jetson Nano, prefer hardware-accelerated decode via NVDEC (`hwaccel = nvdec`).

### Deferred asset consideration

For video, keeping the entire file in the in-memory `AssetStore` while also decoding
it is wasteful. Phase D should extend `AssetStore` with an optional **disk-backed
mode**: assets above a configurable threshold (e.g. 64 MB) are written to a temp file
in `/tmp/vstimd_assets/` and the store keeps only the path. The `get()` API
returns a `AssetData` enum (`InMemory(&[u8])` or `File(PathBuf)`) so callers handle
both cases.

---

## File Layout

```
proto/v1/
  assets.proto           ← NEW: UploadAsset, DeleteAsset, ListAssets, AssetList, AssetInfo

server/src/
  assets.rs              ← NEW: AssetStore (in-memory / disk-backed)
  ipc.rs                 ← route UploadAsset / DeleteAsset / ListAssets
  scene/command.rs       ← cmd_create_bitmap, cmd_create_bitmap_seq, cmd_create_video
  scene/stimulus/types.rs  ← BitmapStimulus, BitmapSeqStimulus already exist; VideoStimulus new
  render/vk/
    pipeline.rs          ← textured_pipeline (new)
    tess.rs              ← tessellate_bitmap, tessellate_bitmap_seq, tessellate_video
    gpu_buffers.rs       ← GpuTexture, alloc_texture, free_texture

client/python/
  vstimd/
    __init__.py          ← export AssetsClient, Connection.assets
    assets/
      __init__.py
      _client.py         ← AssetsClient (upload, upload_bytes, delete, list)
    stimuli/
      _client.py         ← add create_bitmap, create_bitmap_seq, set_bitmap_size,
                              set_bitmap_seq_frame, set_bitmap_seq_fps,
                              create_video, set_video_frame, set_video_fps
    _proto/              ← regenerate after adding assets.proto
```

---

## Dependency additions

```toml
# Cargo.toml
image = "0.25"           # Phase B/C — already planned in PLAN.md

# Phase D only:
[target.'cfg(target_os = "linux")'.dependencies]
ffmpeg-next = "7"        # links to system libavcodec/libavformat
```

---

## Open questions

1. **Descriptor indexing vs. one-descriptor-set-per-draw**: The textured pipeline needs
   to bind a different texture per bitmap stimulus in the same render pass. The clean
   solution is `VK_EXT_descriptor_indexing` (bindless), which is core in Vulkan 1.2 and
   supported on both Jetson and desktop. The fallback is to split the render pass and
   rebind per stimulus. Recommend bindless.

2. **Max asset store size**: Should there be a hard cap (e.g. 512 MB) to prevent OOM on
   Jetson (4 GB RAM)? A soft cap that logs a warning is probably sufficient.

3. **Sequence from multi-frame GIF**: the `image` crate's `AnimationDecoder` is limited
   (only GIF/APNG). If richer format support (WebP animation, multi-frame TIFF) is
   needed, fall back to `ffmpeg-next` for sequence decode too.

4. **PsychoPy compatibility**: PsychoPy's `ImageStim` / `MovieStim3` expect file paths.
   The `vstimd/psychopy/` compatibility shim should intercept the `image` constructor
   argument and auto-upload the file to the asset store, returning a handle.
