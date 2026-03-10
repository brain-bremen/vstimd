# Stimulus Data Model: Composition over Inheritance

> Companion document to `PLAN.md` and `3D_ROADMAP.md`.  
> Addresses the question: should the Rust port mirror the C++ inheritance hierarchy, or use
> a flat composition-based design instead? Also establishes the component structs that both
> 2-D and 3-D stimulus variants are built from.

---

## 1. What the C++ Hierarchy Actually Is

Before proposing an alternative, it is worth being precise about what the inheritance in the
C++ code really encodes. There are three distinct purposes mixed together:

### 1.1 Shared State (data inheritance)

Every level of the hierarchy adds fields that are implicitly "inherited" by all subclasses:

| Class | Fields added |
|---|---|
| `CStimulus` | `enabled`, `enabled_copy`, `suppressed`, `protected`, `animation*`, `error_code`, `type_name` |
| `CD2DStimulus` | `transform` (3×2 matrix, live + copy), `fill_color` (live + copy), `outline_color` (live + copy), `stroke_width`, `draw_mode`, `update_flags` |
| `C3DStimulus` | `viewport` (D3D11_VIEWPORT, live + copy) |
| `CLoadedStimulus` | nothing — marker only |
| `CPixelShader` | `phase`, `phase_inc`, `phase_copy`, `constant_buffer` (center + size + params[8]), `update_flags` |
| `CStimulusPic` | `transform` (4×4), `alpha`, `phi_inc` (live + copy) |

### 1.2 Shared Behaviour (method inheritance)

Some virtual methods have real default implementations that subclasses call via `Super::method()`:

| Method | Shared logic |
|---|---|
| `makeCopy()` / `getCopy()` | Each level copies its own fields; subclass chains upward |
| `Moveto()` | `CD2DStimulus` moves the 3×2 matrix; `C3DStimulus` moves the viewport |
| `GetPos()` | Inverse of above |
| `ShapeCommand()` | Handles opcodes 4–10 (orientation, colour, draw mode, outline, linewidth) for all D2D shapes |
| `ShapeSetSize()` | Parses a size sub-command for all D2D shapes |
| `SetAnimParam()` | Default returns false ("not supported") |

### 1.3 Dispatch Surface (polymorphism)

The only reason `CStimulus*` must be a pointer-to-base is to allow the `std::map<WORD,
CStimulus*>` to hold heterogeneous stimulus types and call `Draw()`, `Command()`, and
`GetPos()` through it.

---

## 2. Why Mirroring the Hierarchy in Rust Is Unwise

### 2.1 Rust has no data inheritance

There is no mechanism in Rust for a struct to automatically gain the fields of another struct.
Simulating it with a `base: BaseStruct` field is just composition with extra steps — and it
forces every access to be `self.base.field` instead of `self.field`. This is noisy and does
not buy anything the C++ hierarchy bought.

### 2.2 Trait objects are not free

`Box<dyn Stimulus>` is a fat pointer. Every virtual dispatch (Draw, Command, Moveto, GetPos,
makeCopy, getCopy) goes through a vtable. This is fine for the low call count we have per frame
(tens of stimuli, not thousands). But if the "base methods" (ShapeCommand, Moveto, makeCopy)
are also traits, you end up with deeply nested dynamic dispatch where the C++ had a simple
inlined chain of `Super::method()` calls. That is worse, not better.

### 2.3 The "shared state" is not truly shared across all types

`CD2DStimulus` fields (2D transform, fill colour, stroke) are only relevant to 2D shapes.
`C3DStimulus` fields (viewport) are only relevant to D3D11 stimuli. In the Rust port there is
no D3D11 at all — everything goes through wgpu. So the real question is: which fields does
every stimulus actually need?

Looking at the full type list, the answer is:

- **All stimuli**: `enabled`, `suppressed`, `protected`, `anim_handle`
- **All positional stimuli** (everything except `CPhotoDiode`): position `(x, y)`, rotation
  angle, live + copy variants of those
- **All coloured stimuli** (everything except video/bitmap which have their own alpha):
  fill colour, outline colour, stroke width, draw mode (fill / stroke / both)
- **Shape-specific**: everything else

There is no "D2D vs D3D11" split any more. The distinction existed because the C++ app mixed
two completely different rendering APIs. In wgpu, all stimuli go through the same device and
queue. The renderer difference that drove the class split no longer exists.

**However, a genuine 2-D vs 3-D split does exist in the planned roadmap** — but it is
expressed as separate component structs (`Transform2D` vs `Transform3D`, `Appearance` vs
`Material3D`) composed explicitly into each variant, not as an inheritance hierarchy. See
`3D_ROADMAP.md` for the full design of 3-D stimulus types. The composition model described
in this document extends cleanly to cover them.

### 2.4 The copy/flip pattern is cleaner as an explicit wrapper

The `makeCopy()` / `getCopy()` pattern is a poor fit for virtual dispatch because it requires
every subclass to remember to call `Super::makeCopy()`. A missed call silently breaks deferred
mode. In Rust, this is better expressed as a generic wrapper that handles the flip mechanically.

---

## 3. Proposed Composition Model

The core insight is that every stimulus is a **combination of a small number of independent
concerns**, each of which can be a plain struct with no trait objects involved:

```
2-D Stimulus = StimulusFlags        (enabled, suppressed, protected, anim_handle)
             + Transform2D          (position, rotation — live + copy)
             + Appearance           (fill, outline, stroke_width, draw_mode — live + copy)
             + <ShapeGeometry>      (rect, ellipse, petal, wedge, … — type-specific)
             + <ShapeParams>        (type-specific animatable params — live + copy)

3-D Stimulus = StimulusFlags        (identical — same struct, reused)
             + Transform3D          (position, orientation, scale in world space — live + copy)
             + Material3D           (albedo, emissive, roughness, shading mode — live + copy)
             + <ShapeGeometry3D>    (box, sphere, cylinder, corridor, mesh, … — type-specific)
             + <ShapeParams3D>      (type-specific animatable params — live + copy)
```

The only polymorphism needed is over the shape geometry and params. `StimulusFlags` and
`Deferred<T>` are reused unchanged across both 2-D and 3-D variants. The distinction between
`Transform2D` and `Transform3D` is explicit in each struct — there is no ambiguity about which
one a given stimulus uses.

---

## 4. The Component Structs

### 4.1 `StimulusFlags` — identical for every stimulus

```rust
/// Lifecycle and visibility flags. Identical across all stimulus types.
#[derive(Clone, Copy, Default)]
pub struct StimulusFlags {
    pub enabled:       bool,
    pub enabled_copy:  bool,
    pub suppressed:    bool,  // set by Flicker animation
    pub protected:     bool,  // survives RemoveAll
    pub anim_handle:   Option<u32>,
}

impl StimulusFlags {
    pub fn make_copy(&mut self) { self.enabled_copy = self.enabled; }
    pub fn get_copy(&mut self)  { self.enabled = self.enabled_copy; }

    pub fn is_visible(&self) -> bool {
        self.enabled && !self.suppressed
    }
}
```

### 4.2 `Transform2D` — position and rotation, with deferred copy

The C++ `CD2DStimulus` stored a full 3×2 affine matrix (which encoded position, rotation, and
the screen-origin offset all at once). That conflation made the code hard to follow. In Rust we
store position and angle separately and compute the affine matrix at draw time.

```rust
/// 2-D placement. Used by every positional stimulus.
#[derive(Clone, Copy)]
pub struct Transform2D {
    pub pos:   [f32; 2],   // (x, y) in stimulus space (centre = 0, Y-up)
    pub angle: f32,        // degrees, counter-clockwise
}

impl Default for Transform2D {
    fn default() -> Self { Self { pos: [0.0, 0.0], angle: 0.0 } }
}

impl Transform2D {
    /// Convert to a kurbo::Affine for tessellation.
    pub fn to_affine(&self) -> kurbo::Affine {
        kurbo::Affine::translate((self.pos[0] as f64, self.pos[1] as f64))
            * kurbo::Affine::rotate(self.angle.to_radians() as f64)
    }
}

/// Deferred-mode wrapper: holds a live value and a staging copy.
/// The copy is written during deferred mode; `flip()` promotes it to live.
#[derive(Clone, Copy, Default)]
pub struct Deferred<T: Copy + Default> {
    pub live: T,
    pub copy: T,
}

impl<T: Copy + Default> Deferred<T> {
    pub fn new(value: T) -> Self { Self { live: value, copy: value } }

    /// Write `value` to whichever slot is appropriate.
    pub fn set(&mut self, deferred: bool, value: T) {
        if deferred { self.copy = value; } else { self.live = value; }
    }

    /// Read the value that is currently active.
    pub fn get(&self) -> &T { &self.live }

    /// Snapshot live → copy (call at start of deferred mode).
    pub fn make_copy(&mut self) { self.copy = self.live; }

    /// Promote copy → live (call at frame flip).
    pub fn flip(&mut self) { self.live = self.copy; }
}
```

`Deferred<T>` is the key abstraction. Every parameter that can change during deferred mode is
wrapped in it. The flip is mechanical: call `.flip()` on every `Deferred<T>` field. No virtual
dispatch, no "remember to call super", no missed field.

### 4.3 `Appearance` — fill/outline/stroke, with deferred copy

```rust
#[derive(Clone, Copy)]
pub struct Appearance {
    pub fill:         [f32; 4],   // RGBA
    pub outline:      [f32; 4],   // RGBA
    pub stroke_width: f32,
    pub draw_mode:    DrawMode,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum DrawMode {
    #[default]
    Fill,
    Stroke,
    FillAndStroke,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            fill:         [1.0, 1.0, 1.0, 1.0],
            outline:      [0.0, 0.0, 0.0, 1.0],
            stroke_width: 2.0,
            draw_mode:    DrawMode::Fill,
        }
    }
}
```

Used as `Deferred<Appearance>` on each stimulus.

---

## 5. Concrete Stimulus Structs

Each stimulus type is a **plain struct** with no trait objects and no `Box<dyn Anything>`
inside it. The fields are explicit and flat.

### 5.1 `RectStimulus`

```rust
pub struct RectStimulus {
    pub flags:      StimulusFlags,
    pub transform:  Deferred<Transform2D>,
    pub appearance: Deferred<Appearance>,
    pub size:       Deferred<[f32; 2]>,    // [half_width, half_height]
}
```

All state. No behaviour. No virtuals. Every field is exactly what it says it is.

### 5.2 `EllipseStimulus`

```rust
pub struct EllipseStimulus {
    pub flags:      StimulusFlags,
    pub transform:  Deferred<Transform2D>,
    pub appearance: Deferred<Appearance>,
    pub radii:      Deferred<[f32; 2]>,    // [rx, ry]
}
```

### 5.3 `PetalStimulus`

```rust
#[derive(Clone, Copy)]
pub struct PetalParams {
    pub r: f32,   // inner arc radius
    pub R: f32,   // outer arc radius
    pub d: f32,   // tip distance
    pub q: f32,   // split ratio (golden ratio default)
}

pub struct PetalStimulus {
    pub flags:      StimulusFlags,
    pub transform:  Deferred<Transform2D>,
    pub appearance: Deferred<Appearance>,
    pub params:     Deferred<PetalParams>,
    pub rebuild:    bool,   // set when params change; cleared after tessellation
}
```

### 5.4 `WedgeStimulus`

```rust
pub struct WedgeStimulus {
    pub flags:      StimulusFlags,
    pub transform:  Deferred<Transform2D>,
    pub appearance: Deferred<Appearance>,
    pub half_angle: Deferred<f32>,          // degrees
    pub rebuild:    bool,
}
```

### 5.5 `DiscStimulus`

```rust
pub struct DiscStimulus {
    pub flags:      StimulusFlags,
    pub transform:  Deferred<Transform2D>,
    pub appearance: Deferred<Appearance>,
    pub radius:     Deferred<f32>,
}
```

### 5.6 `BitmapStimulus`

```rust
pub struct BitmapStimulus {
    pub flags:      StimulusFlags,
    pub transform:  Deferred<Transform2D>,
    // No Appearance: bitmaps have alpha but no fill/stroke
    pub alpha:      Deferred<f32>,
    pub phi_inc:    Deferred<f32>,          // continuous rotation rate (deg/frame)
    pub phi_accum:  f32,                    // accumulated rotation angle (not deferred)
    // GPU resource handle — index into a separate texture atlas or Vec<wgpu::Texture>
    pub texture_id: u32,
    pub size:       [f32; 2],               // half-extents, set at load time, never deferred
}
```

### 5.7 `BitmapSeqStimulus`

```rust
pub struct BitmapSeqStimulus {
    pub flags:         StimulusFlags,
    pub transform:     Deferred<Transform2D>,
    pub alpha:         Deferred<f32>,
    pub texture_ids:   Vec<u32>,           // one per frame
    pub frame_index:   u32,
    pub rate_num:      u32,                // fps numerator
    pub rate_den:      u32,                // fps denominator (display rate)
    pub frac_counter:  u32,
    pub size:          [f32; 2],
}
```

### 5.8 `WgslShaderStimulus`

```rust
#[derive(Clone, Copy)]
pub struct ShaderParams {
    pub center:     [f32; 2],
    pub size:       [f32; 2],
    pub params:     [f32; 8],
    pub phase:      f32,
    pub phase_inc:  f32,
}

pub struct WgslShaderStimulus {
    pub flags:      StimulusFlags,
    pub transform:  Deferred<Transform2D>,
    pub params:     Deferred<ShaderParams>,
    // Pipeline handle — index into a Vec<wgpu::RenderPipeline> in RenderState
    pub pipeline_id: u32,
}
```

### 5.9 `ParticleStimulus`

```rust
#[derive(Clone, Copy)]
pub struct ParticleParams {
    pub color:       [f32; 4],
    pub size:        f32,
    pub angle:       f32,
    pub velocity:    f32,
    pub patch_radius:f32,
    pub gauss_radius:f32,
}

pub struct ParticleStimulus {
    pub flags:       StimulusFlags,
    pub transform:   Deferred<Transform2D>,
    pub params:      Deferred<ParticleParams>,
    pub shift:       Deferred<f32>,
    // GPU vertex buffer handle
    pub vbuffer_id:  u32,
    pub n_particles: u32,
}
```

### 5.10 `PixelStimulus`

```rust
pub struct PixelStimulus {
    pub flags:       StimulusFlags,
    pub transform:   Deferred<Transform2D>,
    pub color:       Deferred<[f32; 4]>,
}
```

---

## 6. The Scene Enum

With all stimulus types as plain structs, the heterogeneous collection is modelled as an enum
rather than a trait object. This is the idiomatic Rust alternative to polymorphism when the
set of variants is closed and known at compile time.

```rust
pub enum Stimulus {
    Rect(RectStimulus),
    Ellipse(EllipseStimulus),
    Petal(PetalStimulus),
    Wedge(WedgeStimulus),
    Disc(DiscStimulus),
    Bitmap(BitmapStimulus),
    BitmapSeq(BitmapSeqStimulus),
    WgslShader(WgslShaderStimulus),
    Particle(ParticleStimulus),
    Pixel(PixelStimulus),
}
```

The "virtual dispatch" table from C++ becomes a `match` block. The compiler sees all branches
and can inline and optimise each one. There is no vtable, no heap allocation per stimulus, no
fat pointer.

---

## 7. Shared Operations via `match` + Helper Functions

The operations that were "inherited" in C++ become free functions or `match` arms that reach
into well-known component fields. Because the component struct names are the same across all
variants, the access pattern is uniform.

### 7.1 `flags()` / `flags_mut()` — access StimulusFlags from any variant

```rust
impl Stimulus {
    pub fn flags(&self) -> &StimulusFlags {
        match self {
            Stimulus::Rect(s)       => &s.flags,
            Stimulus::Ellipse(s)    => &s.flags,
            Stimulus::Petal(s)      => &s.flags,
            Stimulus::Wedge(s)      => &s.flags,
            Stimulus::Disc(s)       => &s.flags,
            Stimulus::Bitmap(s)     => &s.flags,
            Stimulus::BitmapSeq(s)  => &s.flags,
            Stimulus::WgslShader(s) => &s.flags,
            Stimulus::Particle(s)   => &s.flags,
            Stimulus::Pixel(s)      => &s.flags,
        }
    }

    pub fn flags_mut(&mut self) -> &mut StimulusFlags { /* same */ }
}
```

This is verbose but:
- It is entirely mechanical and can be generated by a macro (see §9).
- The compiler sees through it completely — there is no runtime overhead.
- It is impossible to accidentally omit a variant (the compiler enforces exhaustiveness).

### 7.2 `transform()` / `transform_mut()` — not all stimuli have one

Rather than returning `Option<&Deferred<Transform2D>>`, define a separate helper that is only
called when the caller already knows the stimulus has a transform (e.g. from a `MoveTo`
command):

```rust
/// Returns None only for stimuli that have no spatial position.
/// In practice all current stimuli have a transform.
pub fn transform_mut(&mut self) -> Option<&mut Deferred<Transform2D>> {
    match self {
        Stimulus::Rect(s)       => Some(&mut s.transform),
        Stimulus::Ellipse(s)    => Some(&mut s.transform),
        // ... etc
    }
}
```

### 7.3 `make_copy()` / `flip()` — the deferred-mode flip

```rust
impl Stimulus {
    /// Snapshot all live state into copy fields (call at start of deferred mode).
    pub fn make_copy(&mut self) {
        self.flags_mut().make_copy();
        if let Some(t) = self.transform_mut()   { t.make_copy(); }
        if let Some(a) = self.appearance_mut()  { a.make_copy(); }
        match self {
            Stimulus::Petal(s)       => { s.params.make_copy(); }
            Stimulus::Wedge(s)       => { s.half_angle.make_copy(); }
            Stimulus::Rect(s)        => { s.size.make_copy(); }
            Stimulus::Ellipse(s)     => { s.radii.make_copy(); }
            Stimulus::Disc(s)        => { s.radius.make_copy(); }
            Stimulus::Bitmap(s)      => { s.alpha.make_copy(); s.phi_inc.make_copy(); }
            Stimulus::WgslShader(s)  => { s.params.make_copy(); }
            Stimulus::Particle(s)    => { s.params.make_copy(); s.shift.make_copy(); }
            Stimulus::Pixel(s)       => { s.color.make_copy(); }
            Stimulus::BitmapSeq(s)   => { s.alpha.make_copy(); }
        }
    }

    /// Promote all copy fields to live (call at frame boundary after deferred mode ends).
    pub fn flip(&mut self) {
        self.flags_mut().get_copy();
        if let Some(t) = self.transform_mut()   { t.flip(); }
        if let Some(a) = self.appearance_mut()  { a.flip(); }
        match self {
            Stimulus::Petal(s)       => { s.params.flip();  s.rebuild = true; }
            Stimulus::Wedge(s)       => { s.half_angle.flip(); s.rebuild = true; }
            Stimulus::Rect(s)        => { s.size.flip(); }
            Stimulus::Ellipse(s)     => { s.radii.flip(); }
            Stimulus::Disc(s)        => { s.radius.flip(); }
            Stimulus::Bitmap(s)      => { s.alpha.flip(); s.phi_inc.flip(); }
            Stimulus::WgslShader(s)  => { s.params.flip(); }
            Stimulus::Particle(s)    => { s.params.flip(); s.shift.flip(); }
            Stimulus::Pixel(s)       => { s.color.flip(); }
            Stimulus::BitmapSeq(s)   => { s.alpha.flip(); }
        }
    }

    pub fn move_to(&mut self, deferred: bool, x: f32, y: f32) {
        if let Some(t) = self.transform_mut() {
            t.set(deferred, Transform2D { pos: [x, y], angle: t.live.angle });
        }
    }

    pub fn set_angle(&mut self, deferred: bool, degrees: f32) {
        if let Some(t) = self.transform_mut() {
            t.set(deferred, Transform2D { pos: t.live.pos, angle: degrees });
        }
    }

    pub fn get_pos(&self) -> [f32; 2] {
        self.transform().map(|t| t.live.pos).unwrap_or([0.0, 0.0])
    }

    pub fn is_visible(&self) -> bool {
        self.flags().is_visible()
    }
}
```

### 7.4 `set_anim_param()` — type-specific, only defined for types that support it

```rust
impl Stimulus {
    /// Returns false if the param index is not supported by this stimulus type.
    pub fn set_anim_param(&mut self, mode: u8, value: f32) -> bool {
        match self {
            Stimulus::WgslShader(s) => {
                let idx = mode as usize;
                if idx < 8 { s.params.live.params[idx] = value; true }
                else { false }
            }
            Stimulus::Wedge(s) if mode == 1 => {
                s.set_angle(false, value);
                true
            }
            Stimulus::Bitmap(s) if mode == 1 => {
                s.alpha.live = value;
                true
            }
            _ => false,
        }
    }
}
```

---

## 8. The `SceneState` Stimulus Collection

Instead of `HashMap<u32, Box<dyn Stimulus>>`, use `IndexMap<u32, Stimulus>` (from the
`indexmap` crate). The `IndexMap` preserves insertion order — which determines draw order,
exactly as in the C++ `std::map` (sorted by key, which grows monotonically on insertion) — and
also allows O(1) lookup by handle.

```rust
use indexmap::IndexMap;

pub struct SceneState {
    pub stimuli:    IndexMap<u32, Stimulus>,    // enum, inline storage
    pub animations: IndexMap<u32, Animation>,   // enum, inline storage
    // ...
}
```

No `Box`, no heap allocation per stimulus or animation beyond what `Vec`-backed fields inside
each struct need. Both collections are dense, cache-friendly slabs.

---

## 9. Reducing Boilerplate with a Macro

The repeated `match self { Stimulus::Rect(s) => &s.flags, ... }` pattern is mechanical.
A declarative macro removes the repetition:

```rust
/// Apply an expression to the inner struct of any Stimulus variant.
/// Usage: stim_field!(stimulus, |s| &s.flags)
macro_rules! stim_field {
    ($stim:expr, |$s:ident| $expr:expr) => {
        match $stim {
            Stimulus::Rect($s)       => $expr,
            Stimulus::Ellipse($s)    => $expr,
            Stimulus::Petal($s)      => $expr,
            Stimulus::Wedge($s)      => $expr,
            Stimulus::Disc($s)       => $expr,
            Stimulus::Bitmap($s)     => $expr,
            Stimulus::BitmapSeq($s)  => $expr,
            Stimulus::WgslShader($s) => $expr,
            Stimulus::Particle($s)   => $expr,
            Stimulus::Pixel($s)      => $expr,
        }
    };
}

// Then the accessor becomes a one-liner:
impl Stimulus {
    pub fn flags(&self) -> &StimulusFlags {
        stim_field!(self, |s| &s.flags)
    }
}
```

This is the only macro needed. It is defined once, used everywhere the common fields are
accessed. Adding a new stimulus type requires updating only the macro's match arms — the
compiler will catch every missed site via exhaustiveness checking.

---

## 10. The Render Side: No Trait Objects Either

In the C++ code, `CStimulus::Draw()` was virtual because the rendering API (D2D vs D3D11)
differed by type. In the Rust port, all stimuli are rendered through the same wgpu API.
The renderer does not need polymorphism — it can match on the enum directly:

```rust
pub fn draw_stimulus(
    pass:        &mut wgpu::RenderPass,
    stimulus:    &Stimulus,
    gpu_buffers: &GpuBuffers,
    screen_size: [f32; 2],
) {
    if !stimulus.is_visible() { return; }

    match stimulus {
        Stimulus::Rect(s)       => draw_solid_shape(pass, s, gpu_buffers, screen_size),
        Stimulus::Ellipse(s)    => draw_solid_shape(pass, s, gpu_buffers, screen_size),
        Stimulus::Petal(s)      => draw_solid_shape(pass, s, gpu_buffers, screen_size),
        Stimulus::Wedge(s)      => draw_solid_shape(pass, s, gpu_buffers, screen_size),
        Stimulus::Disc(s)       => draw_solid_shape(pass, s, gpu_buffers, screen_size),
        Stimulus::Bitmap(s)     => draw_bitmap(pass, s, gpu_buffers, screen_size),
        Stimulus::BitmapSeq(s)  => draw_bitmap_seq(pass, s, gpu_buffers, screen_size),
        Stimulus::WgslShader(s) => draw_shader(pass, s, gpu_buffers, screen_size),
        Stimulus::Particle(s)   => draw_particles(pass, s, gpu_buffers, screen_size),
        Stimulus::Pixel(s)      => draw_pixel(pass, s, gpu_buffers, screen_size),
    }
}
```

The five `draw_*` functions are plain functions. `draw_solid_shape` is generic over the struct
type because all solid shapes share the same pipeline and differ only in their tessellated
geometry, which has already been computed and stored in `GpuBuffers`.

### 10.1 `GpuBuffers` — parallel to `SceneState`

GPU resources are stored in a separate map that mirrors `SceneState::stimuli` by handle. This
keeps GPU types (`wgpu::Buffer`, `wgpu::Texture`) out of the scene state, which lives behind a
lock shared with the ZMQ thread.

```rust
pub struct GpuBuffers {
    /// Vertex + index buffer pair per stimulus handle.
    pub meshes:   HashMap<u32, StimulusMesh>,
    /// Textures per bitmap/shader stimulus handle.
    pub textures: HashMap<u32, wgpu::Texture>,
    /// Compiled wgpu::RenderPipeline per loaded WGSL shader.
    pub pipelines: Vec<wgpu::RenderPipeline>,
}

pub struct StimulusMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer:  wgpu::Buffer,
    pub index_count:   u32,
}
```

`GpuBuffers` is owned exclusively by the render thread. No locking needed on it.

---

## 11. Tessellation: Where Shape Geometry Lives

Because `GpuBuffers` is render-thread-private, tessellation can happen right at draw time
whenever a stimulus is dirty. The flow is:

```
Render thread, start of frame:
  1. Lock SceneState (read lock).
  2. For each (handle, stimulus) in scene.stimuli:
       a. if stimulus.needs_rebuild():
            geometry = tessellate(stimulus)       // pure function, no allocation beyond Vec
            gpu_buffers.upload(handle, geometry)  // write_buffer on wgpu Queue
       b. draw_stimulus(pass, stimulus, &gpu_buffers, screen_size)
  3. Unlock SceneState.
```

`tessellate` is a pure function:

```rust
pub fn tessellate(stimulus: &Stimulus, screen_size: [f32; 2]) -> (Vec<Vertex>, Vec<u32>) {
    match stimulus {
        Stimulus::Rect(s)    => tessellate_rect(&s.size.live, &s.transform.live, &s.appearance.live),
        Stimulus::Ellipse(s) => tessellate_ellipse(&s.radii.live, &s.transform.live, &s.appearance.live),
        Stimulus::Petal(s)   => tessellate_petal(&s.params.live, &s.transform.live, &s.appearance.live),
        Stimulus::Wedge(s)   => tessellate_wedge(s.half_angle.live, screen_size, &s.transform.live, &s.appearance.live),
        Stimulus::Disc(s)    => tessellate_disc(s.radius.live, &s.transform.live, &s.appearance.live),
        Stimulus::Pixel(s)   => tessellate_pixel(&s.transform.live, &s.color.live),
        // Bitmap, BitmapSeq, WgslShader, Particle: no CPU tessellation needed
        _ => (vec![], vec![]),
    }
}
```

No dynamic dispatch. No allocation beyond the returned `Vec`s. Fully inlinable per variant.

---

## 12. Comparison to the C++ Design

| Concern | C++ | Rust (this design) |
|---|---|---|
| Shared state | Implicit via inheritance | Explicit component structs (`StimulusFlags`, `Transform2D`, `Appearance`) |
| Deferred copy | Virtual `makeCopy()` / `getCopy()`, must chain `Super::` | `Deferred<T>` wrapper: `.make_copy()` / `.flip()` — mechanical, no chaining |
| Polymorphic dispatch | Vtable through `CStimulus*` | `match` on `Stimulus` enum — exhaustive, inlinable |
| Renderer coupling | `Draw()` is on the stimulus — renderer state accessed via globals | Tessellation is a pure function; GPU resources are renderer-private |
| Adding a new type | Subclass `CStimulus` or one of its subclasses; override virtual methods | Add a variant to `Stimulus` enum; add an arm to `stim_field!` macro; implement `tessellate` arm |
| Missing a copy field | Silent bug (forgot `Super::makeCopy()`) | Compile error (non-exhaustive `match` in `make_copy` / `flip`) |
| Allocation | `new CStimulusRect()` — heap per stimulus | `Stimulus::Rect(RectStimulus { .. })` — inline in `IndexMap` slab |

---

## 13. Animations: Also an Enum

Animations follow the same design as stimuli: a flat `enum Animation` with one variant per
animation type, no trait objects, no virtual dispatch.

The earlier argument for `Box<dyn Animation>` — that internal state is "too heterogeneous" —
does not hold up. `BitmapSeqStimulus` has more heterogeneous state than `FlashAnim`, yet it is
a stimulus enum variant without any issue. The asymmetry between arms (`PathAnim` has a
`Vec<[f32;2]>` while `FlashAnim` has just a counter) is exactly the same situation as
`BitmapStimulus` vs `PixelStimulus`. An enum handles it identically.

### 13.1 `AnimCommon` — shared across all animation types

```rust
#[derive(Clone)]
pub struct AnimCommon {
    pub stimulus_handle: Option<u32>,
    pub final_action:    FinalActionMask,
}
```

Every animation variant embeds `AnimCommon`. Accessors on the `Animation` enum delegate to it,
exactly as `Stimulus` delegates common fields to `StimulusFlags`.

### 13.2 Concrete animation structs

Each struct carries both **parameters** (config, serialized on save) and **runtime state**
(current progress, skipped on save with `#[serde(skip)]`).

```rust
pub struct FlashAnim {
    pub common:    AnimCommon,
    // parameters (saved):
    pub n_frames:  u32,
    // runtime state (not saved — reset on load):
    #[serde(skip)] pub frame_count: u32,
}

pub struct FlickerAnim {
    pub common:     AnimCommon,
    pub on_frames:  u32,
    pub off_frames: u32,
    #[serde(skip)] pub frame_count: u32,
    #[serde(skip)] pub phase_on:    bool,
}

pub struct HarmonicAnim {
    pub common:         AnimCommon,
    pub amplitude:      f32,
    pub freq_hz:        f32,
    pub direction_deg:  f32,
    pub phase_deg:      f32,   // initial phase (saved)
    #[serde(skip)] pub phase_accum: f32,  // running phase (not saved)
    #[serde(skip)] pub origin:      [f32; 2],
}

pub struct LineSegAnim {
    pub common:          AnimCommon,
    pub speed_px_per_s:  f32,
    pub vertices:        Vec<[f32; 2]>,
    #[serde(skip)] pub seg_index:   usize,
    #[serde(skip)] pub seg_frac:    f32,
}

pub struct PathAnim {
    pub common:   AnimCommon,
    pub path:     Vec<[f32; 2]>,  // loaded from file; stored inline
    #[serde(skip)] pub frame_index: usize,
}

// LinRangeAnim, IntRangeAnim, ExternalPosAnim, ZmqPosAnim, MouseAnim, GamepadAnim — same pattern
```

### 13.3 The `Animation` enum

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Animation {
    Flash(FlashAnim),
    Flicker(FlickerAnim),
    Harmonic(HarmonicAnim),
    LineSeg(LineSegAnim),
    Path(PathAnim),
    LinRange(LinRangeAnim),
    IntRange(IntRangeAnim),
    ExternalPos(ExternalPosAnim),
    ZmqPos(ZmqPosAnim),
    Mouse(MouseAnim),
    Gamepad(GamepadAnim),
}
```

`IndexMap<u32, Animation>` — no `Box`, no heap allocation per animation, no vtable.

### 13.4 `advance()` via match

```rust
impl Animation {
    pub fn advance(
        &mut self,
        stimuli:    &mut IndexMap<u32, Stimulus>,
        frame_rate: f32,
        deferred:   bool,
    ) {
        match self {
            Animation::Flash(a)    => a.advance(stimuli, frame_rate, deferred),
            Animation::Flicker(a)  => a.advance(stimuli, frame_rate, deferred),
            Animation::Harmonic(a) => a.advance(stimuli, frame_rate, deferred),
            // ...
        }
    }

    pub fn common(&self) -> &AnimCommon {
        match self {
            Animation::Flash(a)    => &a.common,
            Animation::Flicker(a)  => &a.common,
            // ...
        }
    }

    pub fn common_mut(&mut self) -> &mut AnimCommon { /* same */ }
}
```

The same `anim_field!` macro used for stimuli applies here.

### 13.5 Serialization

Because `Animation` is a plain enum with concrete struct variants, `#[derive(Serialize,
Deserialize)]` works directly. No separate `AnimationDef` type is needed. Runtime-only fields
are tagged `#[serde(skip)]`; on load they are initialized to their `Default` values, giving a
freshly started animation with the saved parameters.

### 13.6 Why not a trait object?

| Concern | `Box<dyn Animation>` | `enum Animation` |
|---|---|---|
| Serialization | Requires a separate `AnimationDef` registry | `#[derive(Serialize, Deserialize)]` directly |
| Exhaustiveness | Silent if a new type is missing from a match | Compiler error |
| Allocation | One heap alloc per animation | Inline in `IndexMap` |
| Dispatch | Vtable indirection | Inlined `match` |
| Adding a new type | Implement trait, register for serde | Add variant, add match arms — compiler guides you |

The only real advantage of trait objects — allowing user-defined animation types at runtime —
is not a requirement for this project. The set of animation types is defined in the codebase.

---

*End of document. See `PLAN.md` for phase ordering and `INPUT_LATENCY.md` for position control.*