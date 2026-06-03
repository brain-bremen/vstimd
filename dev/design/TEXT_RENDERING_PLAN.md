# Text Rendering Plan

## Motivation

Text stimuli are needed for experiment instructions, fixation labels, and PsychoPy script compatibility. The target API is PsychoPy's `TextBox2` class, as used in `extern/psychopy/psychopy/demos/coder/stimuli/textStimuli.py`.

## API Scope

### Server-side: one `TextStimulus` type

A single text stimulus type covers all use cases. The Python compatibility layer exposes two classes:

- **`TextBox2`** — full implementation, maps directly to the server's `TextStimulus`
- **`TextStim`** — thin adapter in Python only; translates `height→letterHeight`, `wrapWidth→size[0]`, and `anchorHoriz`/`anchorVert→anchor`, then delegates to `TextBox2`. No additional proto or Rust work needed.

### Key TextBox2 parameters to support (from the demo)

| Parameter | Type | Notes |
|---|---|---|
| `text` | `str` | UTF-8, mutable post-construction |
| `font` | `str` | Family name, e.g. `"Open Sans"` |
| `letterHeight` | `float` | Cap-height in stimulus units |
| `size` | `(float, float)` | Bounding box (width, height); text wraps within it |
| `pos` | `(float, float)` | Centre in stimulus units, mutable |
| `anchor` | `str` | `"center"`, `"bottom-left"`, `"top-right"`, `"top-left"` |
| `color` | color | Text colour, mutable |
| `fillColor` | color | Background box fill (absent → transparent) |
| `borderColor` | color | Box border (absent → none) |
| `flipHoriz` | `bool` | Mirror text horizontally |
| `languageStyle` | `str` | `"LTR"`, `"RTL"`, `"Arabic"` (bidi + reshaping) |

Post-construction mutations used in the demo: `.text`, `.pos`, `.color`.

---

## Rust Text Rendering Library

**Choice: `cosmic-text 0.12`**

| Requirement | fontdue | ab_glyph | cosmic-text |
|---|---|---|---|
| Arabic reshaping + bidi | ❌ | ❌ | ✅ (rustybuzz / HarfBuzz) |
| Word wrap + multi-line layout | ❌ | ❌ | ✅ |
| Pure Rust (Jetson Nano aarch64) | ✅ | ✅ | ✅ |
| Font discovery by name | ❌ | ❌ | ✅ (fontdb) |

`fontdue` and `ab_glyph` are eliminated by the Arabic/Farsi text requirement in the demo. `cosmic-text` bundles `fontdb` (font loading), `rustybuzz` (shaping), and `swash` (rasterization), all pure Rust, all cross-compiling to aarch64 without a C toolchain.

Add to `server/Cargo.toml`:
```toml
cosmic-text = "0.12"
```

### Font loading

Bundle the six TTF files used by the demo as `include_bytes!` in `server/fonts/`:
- Share Tech Mono, Indie Flower, EB Garamond, Open Sans, Josefin Sans, Cairo

Load them at startup via `fontdb::Database::load_font_data`. This avoids runtime network access on the Jetson. Auto-downloading Google Fonts is a v2 concern.

---

## Vulkan Architecture

Text rendering requires one new Vulkan resource type not currently in vstimd: a **glyph texture atlas**.

### Glyph Atlas (`server/src/render/vk/text_atlas.rs`)

```
Format:      R8_UNORM
Size:        2048 × 2048 (4 MB)
Sharing:     One atlas shared across all text stimuli
Allocation:  Shelf-packing (simple row-based bin packer)
Eviction:    None in v1 — demo's font set is bounded
```

`cosmic_text::SwashCache` rasterizes glyphs to CPU bitmaps. New glyphs are patched into the atlas via a staging buffer + `cmd_copy_buffer_to_image` (dirty-rectangle upload). The atlas image stays in `SHADER_READ_ONLY_OPTIMAL` at rest.

Cache key: `(fontdb::ID, glyph_id, size_px_u16)`.

```rust
pub struct GlyphAtlas {
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub descriptor_set: vk::DescriptorSet,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_pool: vk::DescriptorPool,
    memory: vk::DeviceMemory,
    cpu_pixels: Vec<u8>,        // 2048 × 2048, CPU mirror for incremental patching
    shelf_x: u32, shelf_y: u32, shelf_h: u32,
    cache: HashMap<GlyphKey, GlyphEntry>,
}
```

### Text Vertex

A smaller vertex than the shape `Vertex` (no normals needed):

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TextVertex {
    pub position: [f32; 2],  // NDC
    pub uv: [f32; 2],        // atlas UV
}
```

### Text Shader (`server/shaders/text.wgsl`)

```wgsl
struct PC { text_color: vec4<f32> }
var<push_constant> pc: PC;

@group(0) @binding(0) var atlas_sampler: sampler;
@group(0) @binding(1) var atlas_tex: texture_2d<f32>;

@fragment fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let alpha = textureSample(atlas_tex, atlas_sampler, uv).r;
    return vec4<f32>(pc.text_color.rgb, pc.text_color.a * alpha);
}
```

Compiled to SPIR-V at build time via `naga` (same as `solid.wgsl` and `grating.wgsl`).

### Text Pipeline (`server/src/scene/stimulus/text/text_pipeline.rs`)

```rust
pub struct VkTextPipeline {
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
}
```

- Descriptor set layout: binding 0 = sampler, binding 1 = sampled image (the atlas)
- Push constants: 16 bytes (`text_color: [f32; 4]`), fragment stage
- Blend: premultiplied alpha — same `SRC_ALPHA` / `ONE_MINUS_SRC_ALPHA` as the solid pipeline
- Scissor per stimulus for bounding-box clipping

---

## Stimulus Data Structure

**Module:** `server/src/scene/stimulus/text/` (mirrors the `grating/` layout)

```rust
pub struct TextStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub params: Deferred<TextParams>,   // color, fill_color, border_color, flip_horiz
    pub box_size: Deferred<[f32; 2]>,  // [width, height] in pixels
    // String can't be Copy → manual live/copy fields (same pattern as StimulusFlags)
    pub text_live: String,
    pub text_copy: String,
    pub font_family: String,            // not deferred — requires full re-layout
    pub letter_height_px: f32,
    pub anchor: Anchor,
    pub language_style: LanguageStyle,
}
```

Add `Stimulus::Text(TextStimulus)` to the `Stimulus` enum in `mod.rs` and all match arms.

---

## Frame Loop Integration (`render/vk/frame.rs`)

**Tessellation phase** (for dirty `Stimulus::Text`):
1. Call `text_layout::layout_stimulus(stim, screen_size) → (Vec<TextVertex>, Vec<u32>)` using cosmic-text for shaping, word wrap, and anchor offset
2. Rasterize any new glyphs into `GlyphAtlas` (staging-buffer patch)
3. Upload glyph quads to a per-stimulus `VkTextMesh`

**Draw phase** — add `Bound::Text` to the lazy-bind enum:
- Bind text pipeline + atlas descriptor set once per frame
- Per stimulus: push `text_color`, set scissor to bounding box, draw glyph quads
- If `fill_color.a > 0`: draw background quad via solid pipeline first

`FontSystem` and `GlyphAtlas` live in `RenderState`.

---

## Wire Protocol Changes

### `proto/vstimd/v1/common.proto`
```protobuf
STIMULUS_TYPE_TEXT = 11;
```

### `proto/vstimd/v1/stimuli_2d.proto`
```protobuf
enum LanguageStyle {
  LANGUAGE_STYLE_LTR    = 0;
  LANGUAGE_STYLE_RTL    = 1;
  LANGUAGE_STYLE_ARABIC = 2;
}

message CreateTextRequest {
  string        text           = 1;
  string        font           = 2;
  float         letter_height  = 3;   // cap-height in pixels (0 → default 32)
  Vec2          size           = 4;   // bounding box [width, height] in pixels
  Vec2          pos            = 5;
  string        anchor         = 6;   // "center"|"bottom-left"|"top-right"|"top-left"
  Color         color          = 7;
  Color         fill_color     = 8;   // absent → transparent
  Color         border_color   = 9;   // absent → no border
  bool          flip_horiz     = 10;
  LanguageStyle language_style = 11;
  string        id             = 12;
  string        name           = 13;
}

message SetTextRequest       { string text  = 1; }
message SetTextColorRequest  { Color  color = 1; }

// Add to StimulusParams.shape oneof (field 5):
message TextParams { string text = 1; float letter_height = 2; string font = 3; }
```

### `proto/vstimd/v1/service.proto`
```protobuf
// In Request.body oneof:
CreateTextRequest    create_text       = 14;  // system target
SetTextRequest       set_text          = 61;  // stimulus target
SetTextColorRequest  set_text_color    = 62;  // stimulus target
```

---

## Python Client and PsychoPy Layer

### `client/python/vstimd/stimuli/stimuli_client.py`
- Add `create_text(x, y, text, font, letter_height, size_w, size_h, anchor, r, g, b, a, flip_horiz, language_style, name, id) → int`
- Add `set_text(handle, text)`, `set_text_color(handle, r, g, b, a)`

### `client/python/vstimd/psychopy/visual/textbox2.py` (new)
```python
class TextBox2:
    def __init__(self, win, text="", font="Open Sans", letterHeight=0.05,
                 size=(0.5, 0.2), pos=(0, 0), anchor="center",
                 color="white", fillColor=None, borderColor=None,
                 flipHoriz=False, languageStyle="LTR",
                 units="", opacity=1.0, autoDraw=False, name=None,
                 # accepted-but-ignored: bold, italic, alignment, editable, etc.
                 **kwargs): ...

    # Mutable properties:
    @text.setter   # dispatches set_text RPC
    @pos.setter    # dispatches set_position RPC
    @color.setter  # dispatches set_text_color RPC
```

Units conversion: `letterHeight` in `"height"` units → `letterHeight * win.size[1]` px (uses existing `to_pixels`).

### `client/python/vstimd/psychopy/visual/textstim.py` (new, Python-only adapter)
```python
class TextStim(TextBox2):
    """Thin adapter over TextBox2. Maps TextStim's older parameter names."""
    def __init__(self, win, text="Hello World", font="", height=None,
                 wrapWidth=None, anchorHoriz="center", anchorVert="center", ...):
        letterHeight = height or 0.05
        size_w = wrapWidth or 0.8
        anchor = f"{anchorVert}-{anchorHoriz}".replace("center-center", "center")
        super().__init__(win, text, font=font, letterHeight=letterHeight,
                         size=(size_w, 0.2), anchor=anchor, ...)
```

### `client/python/vstimd/psychopy/visual/__init__.py`
```python
from .textbox2 import TextBox2
from .textstim import TextStim
__all__ = [..., "TextBox2", "TextStim"]
```

---

## Implementation Order

1. **Proto + Python stubs** — add messages, regenerate stubs, `StimuliClient` methods
2. **Rust `TextStimulus` struct + scene** — struct, enum variant, match arms, command handlers (testable via `lib.rs` integration tests, no GPU required)
3. **Font loading + cosmic-text layout** — bundle TTFs, `text_layout.rs` producing `Vec<TextVertex>` (testable without Vulkan)
4. **GlyphAtlas Vulkan** — `text_atlas.rs`: image, shelf packer, staging-buffer upload
5. **Text shader + pipeline** — `text.wgsl`, `VkTextPipeline`, `build.rs` entry
6. **Frame loop integration** — `Bound::Text`, draw logic, background rect, scissor
7. **Python `TextBox2` + `TextStim`** — classes, `__init__.py`, e2e test

---

## V1 vs. Deferred

**V1 (covers the demo):** Latin + Unicode + Arabic/Farsi bidi, 6 bundled fonts, word wrap, 4 anchor types, `flipHoriz`, fill/border color, `.text`/`.pos`/`.color` mutations.

**V2:** Google Fonts auto-download, SDF for resolution-independent scaling, LRU atlas eviction, `bold`/`italic` font variants, `alignment` (left/center/right) within box, `SetTextSize` mutation.

---

## Verification

```bash
# 1. Confirm it compiles and tests pass
cargo build && cargo test && cargo clippy

# 2. Null-renderer smoke test (no display required)
cargo run --release -- --null &
cd client/python && uv run python -c "
from vstimd import Connection
from vstimd.psychopy import visual
with Connection() as conn:
    win = visual.Window(size=(800,800), units='height', conn=conn)
    t = visual.TextBox2(win, text='hello', letterHeight=0.05, size=(0.4,0.1))
    t.text = '60 fps'
    t.color = 'green'
    print('OK')
"

# 3. Visual verification — run desktop server and check all 7 demo stimuli
cargo run --release -- --windowed 800x800
cd client/python && uv run examples/text_stimuli.py
```
