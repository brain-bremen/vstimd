# 3-D Stimulus Roadmap

> Companion document to `PLAN.md` and `STIMULUS_DATA_MODEL.md`.  
> Covers the planned evolution from the current 2-D stimulus set toward full 3-D scene
> rendering, including corridors/mazes, 3-D primitives, mesh models, and — as a long-horizon
> research target — Gaussian splatting.

---

## Table of Contents

1. [Guiding Principles](#1-guiding-principles)
2. [Rendering Architecture Evolution](#2-rendering-architecture-evolution)
3. [Coordinate Systems](#3-coordinate-systems)
4. [Phase A — 3-D Infrastructure](#4-phase-a--3-d-infrastructure)
5. [Phase B — 3-D Primitives](#5-phase-b--3-d-primitives)
6. [Phase C — Corridor and Maze Stimuli](#6-phase-c--corridor-and-maze-stimuli)
7. [Phase D — Mesh Model Stimuli](#7-phase-d--mesh-model-stimuli)
8. [Phase E — Gaussian Splatting (Long Horizon)](#8-phase-e--gaussian-splatting-long-horizon)
9. [Impact on the Stimulus Enum and Data Model](#9-impact-on-the-stimulus-enum-and-data-model)
10. [Impact on the Scene State and Protocol](#10-impact-on-the-scene-state-and-protocol)
11. [Impact on Animations](#11-impact-on-animations)
12. [Crate Dependencies for 3-D](#12-crate-dependencies-for-3-d)
13. [Open Questions](#13-open-questions)

---

## 1. Guiding Principles

### 1.1 2-D must never regress

The 2-D stimulus pipeline (flat shapes, bitmaps, pixel shaders) is the production-critical
path. All 3-D work is additive. The 2-D render pass must remain:

- Pixel-perfect in placement (centre-origin, Y-up, pixel coordinates).
- Frame-synchronised with vsync (no dropped frames due to heavy 3-D scenes).
- Unaffected by the presence or absence of 3-D stimuli in the scene.

### 1.2 2-D and 3-D coexist in the same frame

Many experiments will mix 2-D overlays (fixation cross, photodiode flash, reward cue) with a
3-D background scene. The render loop must composite both layers every frame. The draw order is:

```
[3-D pass]  → clear depth, draw 3-D world
[2-D pass]  → draw flat stimuli on top (no depth test, screen-aligned)
[overlay]   → egui debug overlay (feature-gated)
```

### 1.3 The stimulus enum stays closed; the 3-D variants are just more arms

The `Stimulus` enum design from `STIMULUS_DATA_MODEL.md` extends naturally. 3-D stimulus
variants follow exactly the same composition rules as 2-D ones: explicit component structs,
`Deferred<T>` for all deferrable parameters, no inheritance.

### 1.4 The camera is a first-class scene object, not a global

In a 3-D scene the camera pose (position, orientation, field of view) directly determines what
the animal sees. It must be controllable frame-by-frame from the same animation system that
moves 2-D stimuli. It therefore lives in `SceneState` as a named object, not as a global
rendering parameter.

### 1.5 Complexity is introduced in phases; each phase is independently shippable

Each phase below delivers a working system. Later phases build on earlier ones but do not
require rewriting them.

---

## 2. Rendering Architecture Evolution

### 2.1 Current state (2-D only)

```
SceneState
└── stimuli: IndexMap<u32, Stimulus>   (all 2-D variants)

Render thread
└── wgpu render pass
    ├── solid_pipeline      (shapes tessellated with lyon)
    ├── textured_pipeline   (bitmaps)
    └── shader_pipeline     (custom WGSL fragment shaders)
```

### 2.2 Target state (2-D + 3-D)

```
SceneState
├── stimuli: IndexMap<u32, Stimulus>   (2-D and 3-D variants)
├── camera:  Deferred<Camera3D>        (view/projection for the 3-D pass)
└── world:   World3D                   (static geometry: corridor, skybox, etc.)

Render thread
├── depth_texture: wgpu::Texture       (shared depth buffer, 3-D pass only)
│
├── [3-D render pass]   (clears colour + depth)
│   ├── geometry_pipeline   (opaque meshes, PBR or Phong)
│   ├── wireframe_pipeline  (optional debug)
│   └── splat_pipeline      (Phase E: Gaussian splatting)
│
└── [2-D render pass]   (no depth write/test, draws on top)
    ├── solid_pipeline
    ├── textured_pipeline
    └── shader_pipeline
```

The two render passes share the same `wgpu::CommandEncoder` and the same surface texture.
The 3-D pass writes and tests depth; the 2-D pass ignores depth entirely, so 2-D stimuli
always appear in front regardless of their notional Z position.

---

## 3. Coordinate Systems

Three coordinate systems are in play. Their relationship must be documented and kept consistent
throughout the codebase.

### 3.1 Stimulus space (2-D, existing)

- Origin at screen centre.
- X right, Y up.
- Units: pixels.
- Used by all existing 2-D stimuli and animations.
- **Unchanged by 3-D work.**

### 3.2 World space (3-D)

- Right-handed, Y-up convention (matches glTF, Blender export defaults, and most neuroscience
  VR literature).
- Origin is arbitrary; by convention the animal's nominal starting position.
- Units: centimetres (chosen because typical corridor widths are 20–100 cm — keeps numbers
  human-readable and avoids floating-point precision issues at metre scale).
- The camera lives in world space.

### 3.3 Clip space / NDC (wgpu)

- wgpu uses a left-handed NDC with Z in [0, 1] (DirectX/Metal convention, not OpenGL's [-1,1]).
- The projection matrix must account for this. Use `glam::Mat4::perspective_rh` — glam
  produces correct wgpu-compatible matrices when the `depth-zero-to-one` convention is
  selected, which it is by default in the wgpu ecosystem.

### 3.4 Conversion utilities

```rust
/// Convert a 2-D stimulus-space position to a world-space point on the
/// near plane (for overlay sprites attached to 3-D scenes).
pub fn stimulus_to_world(pos: [f32; 2], screen_size: [f32; 2], camera: &Camera3D) -> glam::Vec3 { ... }

/// Project a world-space point to stimulus space (for HUD labels, gaze overlays).
pub fn world_to_stimulus(world: glam::Vec3, screen_size: [f32; 2], camera: &Camera3D) -> [f32; 2] { ... }
```

---

## 4. Phase A — 3-D Infrastructure

> **Prerequisite:** Phases 1–7 of `PLAN.md` (core 2-D system) complete.

This phase introduces the machinery that all subsequent 3-D stimuli depend on.
No visible 3-D stimuli are added yet — only the scaffolding.

### A.1 `Camera3D` as a scene object

```rust
#[derive(Clone, Copy)]
pub struct Camera3D {
    pub position:   glam::Vec3,   // world space, cm
    pub yaw:        f32,          // degrees, rotation around Y axis
    pub pitch:      f32,          // degrees, tilt up/down
    pub roll:       f32,          // degrees, bank (usually 0)
    pub fov_y:      f32,          // vertical field of view, degrees
    pub near:       f32,          // near clip plane, cm (e.g. 1.0)
    pub far:        f32,          // far clip plane, cm (e.g. 100_000.0)
}

impl Camera3D {
    pub fn view_matrix(&self) -> glam::Mat4 { ... }
    pub fn proj_matrix(&self, aspect: f32) -> glam::Mat4 { ... }
    pub fn view_proj(&self, aspect: f32) -> glam::Mat4 {
        self.proj_matrix(aspect) * self.view_matrix()
    }
}
```

`Camera3D` lives in `SceneState` as `Deferred<Camera3D>` so it participates in the deferred
flip exactly like any other parameter. It can be assigned an animation handle, allowing gaze-
locked or trajectory-driven camera movement through the existing animation system.

The camera is initialised to a sensible default: positioned at the origin, looking down the
negative Z axis, 60° FoV, near=1 cm, far=50 000 cm.

### A.2 Camera uniform buffer

The view-projection matrix is uploaded to a `wgpu::Buffer` once per frame before the 3-D pass.
All 3-D pipelines share a single bind group layout with this buffer at binding 0.

```wgsl
// shared_3d.wgsl  (included by all 3-D vertex shaders via @include or manual concatenation)
struct Camera {
    view_proj: mat4x4<f32>,
    position:  vec3<f32>,
    _pad:      f32,
}
@group(0) @binding(0) var<uniform> camera: Camera;
```

### A.3 Depth texture

A `wgpu::Texture` of format `Depth32Float`, same size as the surface. Recreated on window
resize. Used only by the 3-D render pass. The 2-D pass does not attach it.

### A.4 New render pass structure

```rust
// In render/mod.rs, the per-frame render function becomes:

fn render_frame(state: &mut RenderState, scene: &SceneState) {
    let mut encoder = state.device.create_command_encoder(...);

    // ── 3-D pass ────────────────────────────────────────────────────────────
    if scene.has_3d_stimuli() {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(/* surface texture view */)],
            depth_stencil_attachment: Some(/* depth texture view, clear to 1.0 */),
            ..
        });
        render_3d_stimuli(&mut pass, scene, &state.gpu_buffers_3d);
    }

    // ── 2-D pass ────────────────────────────────────────────────────────────
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(/* surface texture view, LoadOp::Load — don't clear */)],
            depth_stencil_attachment: None,   // 2-D ignores depth
            ..
        });
        render_2d_stimuli(&mut pass, scene, &state.gpu_buffers_2d);
        render_photodiode(&mut pass, scene, &state.gpu_buffers_2d);
    }

    // ── Overlay pass ─────────────────────────────────────────────────────────
    #[cfg(feature = "overlay")]
    render_overlay(&mut encoder, scene, state);

    state.queue.submit(std::iter::once(encoder.finish()));
}
```

The key detail: the 2-D pass uses `LoadOp::Load` on the colour attachment, not `Clear` —
it draws on top of whatever the 3-D pass produced.

### A.5 `GpuBuffers3D` — render-thread-private 3-D resources

```rust
pub struct GpuBuffers3D {
    pub camera_buf:      wgpu::Buffer,              // uniform, updated every frame
    pub meshes:          HashMap<u32, Mesh3D>,      // one per stimulus handle
    pub textures:        HashMap<u32, wgpu::Texture>,
    pub pipelines:       ThreeDPipelines,
}

pub struct Mesh3D {
    pub vertex_buffer:   wgpu::Buffer,
    pub index_buffer:    wgpu::Buffer,
    pub index_count:     u32,
    pub index_format:    wgpu::IndexFormat,         // Uint16 or Uint32
}

pub struct ThreeDPipelines {
    pub opaque:          wgpu::RenderPipeline,
    pub opaque_textured: wgpu::RenderPipeline,
    pub wireframe:       wgpu::RenderPipeline,      // debug
}
```

### A.6 Protobuf additions for Phase A

```protobuf
// Camera control
message CmdSetCamera {
    Vec3  position  = 1;
    float yaw_deg   = 2;
    float pitch_deg = 3;
    float roll_deg  = 4;
    float fov_y_deg = 5;
    float near_cm   = 6;
    float far_cm    = 7;
}
message CmdGetCamera {}

message Vec3 { float x = 1; float y = 2; float z = 3; }
```

Add `CmdSetCamera` and `CmdGetCamera` to `SystemCmd`.

### A.7 New crate dependencies for Phase A

```toml
glam  = { version = "0.29", features = ["bytemuck"] }  # 3-D math
```

`glam` is the de-facto standard for wgpu projects. Its `Mat4`, `Vec3`, `Quat` types are
`bytemuck`-compatible and map directly to WGSL types.

---

## 5. Phase B — 3-D Primitives

> **Prerequisite:** Phase A complete.

Procedurally generated 3-D shapes, defined in world space, rendered in the 3-D pass.

### B.1 New stimulus variants

```rust
// Added to the Stimulus enum:
Stimulus::Box3D(BoxStimulus3D),
Stimulus::Sphere3D(SphereStimulus3D),
Stimulus::Cylinder3D(CylinderStimulus3D),
Stimulus::Plane3D(PlaneStimulus3D),       // infinite ground plane or bounded quad
```

### B.2 Component structs

```rust
/// 3-D placement — equivalent of Transform2D but in world space.
#[derive(Clone, Copy)]
pub struct Transform3D {
    pub position:    glam::Vec3,    // world space, cm
    pub orientation: glam::Quat,   // rotation
    pub scale:       glam::Vec3,   // non-uniform scale (default [1,1,1])
}

impl Transform3D {
    pub fn model_matrix(&self) -> glam::Mat4 {
        glam::Mat4::from_scale_rotation_translation(
            self.scale, self.orientation, self.position)
    }
}

/// Surface appearance for 3-D stimuli (simple Phong / unlit model for now).
#[derive(Clone, Copy)]
pub struct Material3D {
    pub albedo:     [f32; 4],   // RGBA base colour
    pub emissive:   [f32; 3],   // self-illumination (useful for stimuli that must be a specific luminance)
    pub roughness:  f32,        // 0=mirror, 1=fully diffuse (unused in unlit mode)
    pub shading:    Shading3D,
}

#[derive(Clone, Copy, Default)]
pub enum Shading3D {
    #[default]
    Unlit,    // albedo only, no lighting — most psychophysics stimuli want this
    Phong,    // simple diffuse + specular, one directional light
}
```

Both `Transform3D` and `Material3D` are wrapped in `Deferred<T>` on each stimulus struct.

### B.3 Concrete structs

```rust
pub struct BoxStimulus3D {
    pub flags:     StimulusFlags,
    pub transform: Deferred<Transform3D>,
    pub material:  Deferred<Material3D>,
    pub half_size: Deferred<glam::Vec3>,   // half-extents in cm
}

pub struct SphereStimulus3D {
    pub flags:     StimulusFlags,
    pub transform: Deferred<Transform3D>,
    pub material:  Deferred<Material3D>,
    pub radius:    Deferred<f32>,          // cm
    pub rings:     u32,                    // tessellation quality (not deferrable)
    pub sectors:   u32,
}

pub struct CylinderStimulus3D {
    pub flags:     StimulusFlags,
    pub transform: Deferred<Transform3D>,
    pub material:  Deferred<Material3D>,
    pub radius:    Deferred<f32>,
    pub height:    Deferred<f32>,
    pub segments:  u32,
}

pub struct PlaneStimulus3D {
    pub flags:     StimulusFlags,
    pub transform: Deferred<Transform3D>,
    pub material:  Deferred<Material3D>,
    pub half_size: Deferred<[f32; 2]>,    // [half_width, half_depth] in cm; None = infinite
    pub tile_uv:   Deferred<[f32; 2]>,   // UV tiling factors
}
```

### B.4 Vertex format for 3-D

```rust
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex3D {
    pub position: [f32; 3],
    pub normal:   [f32; 3],
    pub uv:       [f32; 2],
}
```

### B.5 Per-object uniform

Each 3-D stimulus gets a small uniform buffer with its model matrix and material parameters,
uploaded every frame if dirty. This avoids the overhead of a large instancing buffer for the
small number of stimuli typical in an experiment.

```wgsl
struct Object {
    model:    mat4x4<f32>,
    albedo:   vec4<f32>,
    emissive: vec3<f32>,
    shading:  u32,          // 0=unlit, 1=phong
}
@group(1) @binding(0) var<uniform> object: Object;
```

### B.6 Tessellation

For Phase B, all 3-D primitives are tessellated on the CPU at creation time or when parameters
change (same `rebuild` flag pattern as `PetalStimulus`). Libraries are not needed — the
geometry is simple enough to generate directly:

- **Box**: 6 faces × 2 triangles × 4 vertices with correct normals and UVs.
- **Sphere**: UV sphere — `rings × sectors` quads. `rings=32, sectors=32` is the default.
- **Cylinder**: top cap + bottom cap (fan) + side (quad strip).
- **Plane**: single quad or subdivided grid.

A future phase can move to GPU-side tessellation shaders, but that is not necessary for
psychophysics stimuli.

---

## 6. Phase C — Corridor and Maze Stimuli

> **Prerequisite:** Phase B complete.

Corridors and mazes are the primary use case for 3-D visual stimuli in rodent / primate
navigation experiments. They are **procedural environments**, not mesh files, making them easy
to parameterise and update from the control script.

### C.1 Corridor stimulus

A corridor is an axis-aligned tube with configurable cross-section, length, wall texture, and
visual cues (stripes, landmarks) placed at specified positions along its length.

```rust
#[derive(Clone, Copy)]
pub struct CorridorParams {
    pub width:      f32,          // cm
    pub height:     f32,          // cm
    pub length:     f32,          // cm
    pub floor_tex:  Option<u32>,  // texture_id, None = solid colour
    pub wall_tex:   Option<u32>,
    pub ceil_tex:   Option<u32>,
    pub floor_col:  [f32; 4],     // used if no texture
    pub wall_col:   [f32; 4],
    pub ceil_col:   [f32; 4],
    pub cue_period: f32,          // cm between visual cues (0 = no cues)
    pub cue_col:    [f32; 4],
}

pub struct CorridorStimulus {
    pub flags:     StimulusFlags,
    pub transform: Deferred<Transform3D>,   // position/orientation of corridor entrance
    pub params:    Deferred<CorridorParams>,
    pub rebuild:   bool,
    // The animal's position along the corridor is usually driven by AnimExternalPos
    // mapped to camera position — the corridor itself does not move.
}
```

### C.2 Maze stimulus

A maze is a collection of corridor segments connected at junctions. It is described by a graph:

```rust
pub struct MazeNode {
    pub position: glam::Vec3,    // junction centre in world space, cm
    pub radius:   f32,           // junction room radius
}

pub struct MazeCorridor {
    pub from:   usize,           // index into nodes
    pub to:     usize,
    pub width:  f32,
    pub height: f32,
    pub wall_col: [f32; 4],
    pub floor_col: [f32; 4],
}

pub struct MazeStimulus {
    pub flags:      StimulusFlags,
    pub transform:  Deferred<Transform3D>,
    pub nodes:      Vec<MazeNode>,       // not deferrable individually — rebuild on change
    pub corridors:  Vec<MazeCorridor>,
    pub rebuild:    bool,
}
```

The maze is tessellated once at creation and re-tessellated only when the topology changes.
Wall colour and texture changes can be applied without a full rebuild by updating per-segment
uniform buffers.

### C.3 Camera navigation in a corridor

The camera is the animal's viewpoint. It is driven by `AnimExternalPos` (mapped to world X/Z
position) or by dedicated 3-D animation types (see §11). A typical setup:

1. Create a `CorridorStimulus` (static, no animation).
2. Create `AnimExternalPos` reading from `/vstimd_treadmill` shared memory (linear position
   along corridor).
3. Assign the animation to the **camera**, not to a stimulus.

To support this, the camera participates in the animation system. It is addressable as a
special handle (e.g. `CAMERA_HANDLE = 0xFFFF`) in the command protocol.

### C.4 Tessellation approach for corridors

A corridor is just a box with the front and back faces open. Each wall/floor/ceiling panel is
a `PlaneStimulus3D` internally, but they are grouped under a single handle. The tessellator
generates the full corridor mesh in one call, taking `CorridorParams` as input.

For corridors with texture, UV coordinates are set so that a repeating wall texture tiles
naturally: U maps to the length axis, V maps to the height axis, with scale factors derived
from `length / texture_width_cm` and `height / texture_height_cm` (both configurable).

---

## 7. Phase D — Mesh Model Stimuli

> **Prerequisite:** Phase B complete.

Load and render static or animated 3-D mesh models from files. The primary use cases are:

- Object recognition tasks (a 3-D object rotates; the animal must identify it).
- Reward indicators (a 3-D icon appears at the goal location in a maze).
- Avatars / conspecifics for social neuroscience paradigms.

### D.1 File format

**glTF 2.0** (`.gltf` / `.glb`) is the recommended format:

- Open standard, widely supported by Blender, Maya, and all major 3-D tools.
- Embeds materials, textures, and optionally skeletal animation in a single file.
- The `gltf` crate provides a pure-Rust parser with no native dependencies.
- glTF uses Y-up, right-handed coordinates — consistent with our world-space convention.

**OBJ** (`.obj` + `.mtl`) is supported as a fallback for legacy assets via the `tobj` crate.

### D.2 Stimulus struct

```rust
pub struct MeshStimulus {
    pub flags:      StimulusFlags,
    pub transform:  Deferred<Transform3D>,
    pub material_override: Deferred<Option<Material3D>>,  // None = use material from file
    pub anim_frame: Deferred<f32>,   // for glTF skeletal animation: time in seconds
    pub anim_speed: Deferred<f32>,   // playback rate multiplier (0 = paused)
    // GPU resource handles — set at load time, owned by GpuBuffers3D
    pub mesh_id:    u32,
    pub skin_id:    Option<u32>,     // for skinned meshes
}
```

### D.3 Loading pipeline

```
File on disk (glTF/OBJ)
  ↓  (blocking, at creation time, on a background thread)
CPU geometry: Vec<Vertex3D>, Vec<u32>
  ↓  (render thread, next frame after load completes)
wgpu::Buffer upload → GpuBuffers3D::meshes[handle]
```

Loading is done on a background thread to avoid blocking the render loop. The stimulus is
created immediately with `flags.enabled = false`; once loading completes a flag is set and the
render thread uploads the buffers. The stimulus is then automatically enabled (or the client
polls for completion and enables it manually).

### D.4 glTF skeletal animation

glTF skeletal animation is evaluated on the CPU using the `gltf` crate's animation data and a
small skin evaluation loop. The resulting joint matrices are uploaded to a uniform buffer (or
storage buffer for large skeletons) and applied in the vertex shader.

This is optional and can be deferred to a sub-phase. Static meshes are the immediate priority.

### D.5 LOD considerations

For now, single-LOD meshes are sufficient. If experiments require very large or complex models,
a simple manual LOD system (the client loads multiple mesh variants and switches by handle) is
preferable to an automatic system, since the client controls what appears on screen.

---

## 8. Phase E — Gaussian Splatting (Long Horizon)

> **Prerequisite:** Phase D complete. Requires significant GPU compute capability.  
> **Timeline:** Research/experimental, no fixed schedule.

Gaussian splatting (3-D Gaussian Splatting, 3DGS) is a novel-view synthesis technique that
represents a scene as a collection of anisotropic 3-D Gaussians, each with position, covariance,
opacity, and colour (via spherical harmonics). It can reconstruct photorealistic scenes from
photographs and render them in real time.

In the context of visual neuroscience stimuli, the primary motivation is:

- **Photorealistic virtual environments** reconstructed from real-world locations (the animal's
  home cage, a real maze, a natural scene).
- **Natural image statistics** without the manual labour of 3-D modelling.
- **Dynamic novel-view synthesis**: the camera moves freely through a pre-recorded scene.

### E.1 What Gaussian splatting is and why it is hard

A trained 3DGS scene contains millions of Gaussians. Rendering requires:

1. **View-dependent sorting**: Gaussians must be drawn back-to-front (or an alpha-compositing
   order) from the current camera viewpoint. This requires a GPU radix sort every frame.
2. **Tile-based rasterisation**: the reference renderer uses a CUDA kernel for tile-based
   splatting. A wgpu port requires a compute shader implementation.
3. **Spherical harmonics evaluation**: per-Gaussian colour varies with view direction (up to
   degree 3 = 16 coefficients × 3 channels per Gaussian).

This is a substantial rendering research project, not a straightforward stimulus type. It is
listed here to ensure the architecture does not accidentally close off the possibility.

### E.2 Architectural prerequisites (to not foreclose this option)

The following decisions in earlier phases keep the door open:

1. **wgpu compute shaders are supported.** The sorting and splatting passes require
   `wgpu::ComputePipeline`. This is already available in wgpu — no change needed.

2. **The 3-D render pass can be replaced or augmented.** The Phase A architecture separates
   the 3-D pass from the 2-D pass. The splatting renderer replaces the geometry pipeline in
   the 3-D pass, not the whole frame. 2-D overlays still work.

3. **Large GPU buffer support.** A scene with 1–5 million Gaussians needs ~200–1000 MB of GPU
   memory for positions, covariances, and SH coefficients. `wgpu::Buffer` supports this; no
   special handling is needed beyond allowing large allocations.

4. **The `Camera3D` struct is already sufficient.** Gaussian splatting needs exactly
   position + orientation + FoV + near/far — identical to Phase A's camera.

### E.3 Proposed stimulus struct (placeholder)

```rust
pub struct GaussianSplatStimulus {
    pub flags:      StimulusFlags,
    // The camera is the viewpoint; the splat itself has no transform
    // (the scene is world-aligned at training time).
    // A global offset can be applied via the transform if needed.
    pub transform:  Deferred<Transform3D>,
    pub opacity_scale: Deferred<f32>,   // global opacity multiplier for fade-in/out
    pub sh_degree:  u32,                // 0–3; lower = faster, less view-dependent colour
    // GPU resource handle — points to a GaussianSplatScene in GpuBuffers3D
    pub scene_id:   u32,
}
```

### E.4 Loading and training

Training a 3DGS scene is done offline (using the original 3DGS CUDA code, or `nerfstudio`,
or similar). The trained scene is saved as a `.ply` file (the standard interchange format).
The stimulus server loads the `.ply` at runtime and uploads Gaussian attributes to GPU buffers.

A pure-Rust `.ply` parser exists (`ply-rs` crate). The Gaussian attribute layout in `.ply` is
well-defined by the original paper's reference implementation.

### E.5 Rendering approach for wgpu

The most practical wgpu implementation of 3DGS rasterisation is a **compute-then-draw** pipeline:

```
[compute pass]
  1. Cull Gaussians behind camera or outside frustum (compute shader)
  2. Project Gaussians to 2-D screen-space ellipses (compute shader)
  3. Compute sort keys (depth) (compute shader)
  4. GPU radix sort by depth (compute shader, e.g. using the `wgpu-radix-sort` pattern)

[render pass]
  5. For each Gaussian (indirect draw): splat a screen-aligned quad, alpha-blend
     the Gaussian footprint using the precomputed 2-D covariance
```

Step 4 is the hardest. A wgpu-compatible GPU radix sort in compute shaders exists in research
implementations and is a tractable engineering project. Reference: the `CUDA-Free 3DGS` work
and the `web-splat` open-source wgpu 3DGS viewer (MIT licence) are both good starting points.

### E.6 Integration with the experiment control protocol

From the client's perspective, a Gaussian splat scene is just another stimulus handle:

```protobuf
message CmdLoadGaussianSplat {
    string path       = 1;   // path to .ply file
    uint32 sh_degree  = 2;   // 0–3
}
```

The camera is controlled through the existing `CmdSetCamera` / `AnimExternalPos` mechanism.
No special protocol is needed.

---

## 9. Impact on the Stimulus Enum and Data Model

### 9.1 Extended `Stimulus` enum

```rust
pub enum Stimulus {
    // ── 2-D (existing) ────────────────────────────────────────────────
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

    // ── 3-D primitives (Phase B) ───────────────────────────────────────
    Box3D(BoxStimulus3D),
    Sphere3D(SphereStimulus3D),
    Cylinder3D(CylinderStimulus3D),
    Plane3D(PlaneStimulus3D),

    // ── 3-D environments (Phase C) ─────────────────────────────────────
    Corridor(CorridorStimulus),
    Maze(MazeStimulus),

    // ── 3-D mesh models (Phase D) ──────────────────────────────────────
    Mesh(MeshStimulus),

    // ── Gaussian splatting (Phase E) ───────────────────────────────────
    GaussianSplat(GaussianSplatStimulus),
}
```

### 9.2 The `stim_field!` macro extends to cover new arms

Every new variant is added to the `stim_field!` macro. The compiler enforces this — any
unhandled variant in a `flags()`, `make_copy()`, or `flip()` call is a compile error.

### 9.3 The `transform()` accessor splits into 2-D and 3-D variants

```rust
impl Stimulus {
    /// Returns the 2-D transform for 2-D stimuli. None for 3-D stimuli.
    pub fn transform2d(&self) -> Option<&Deferred<Transform2D>> { ... }

    /// Returns the 3-D transform for 3-D stimuli. None for 2-D stimuli.
    pub fn transform3d(&self) -> Option<&Deferred<Transform3D>> { ... }
}
```

`move_to` on a 3-D stimulus moves its `Transform3D.position.xz` (the horizontal plane), with
Y held fixed unless `move_to_3d` is used. This preserves backward compatibility: an animation
that writes (x, y) to a stimulus still works whether the stimulus is 2-D or 3-D.

### 9.4 `is_3d()` helper for render pass routing

```rust
impl Stimulus {
    pub fn is_3d(&self) -> bool {
        matches!(self,
            Stimulus::Box3D(_) | Stimulus::Sphere3D(_) | Stimulus::Cylinder3D(_) |
            Stimulus::Plane3D(_) | Stimulus::Corridor(_) | Stimulus::Maze(_) |
            Stimulus::Mesh(_)  | Stimulus::GaussianSplat(_))
    }
}
```

The render loop calls `scene.stimuli.values().any(Stimulus::is_3d)` to decide whether to run
the 3-D pass at all. If no 3-D stimuli are present, the depth texture allocation and the 3-D
render pass are skipped entirely — zero overhead for pure 2-D experiments.

---

## 10. Impact on the Scene State and Protocol

### 10.1 `SceneState` additions

```rust
pub struct SceneState {
    // ... existing fields ...
    pub camera:        Deferred<Camera3D>,
    pub ambient_light: Deferred<[f32; 3]>,   // RGB, used by Phong shading
    pub sun_direction: Deferred<glam::Vec3>,  // normalised, world space
    pub sun_colour:    Deferred<[f32; 3]>,
}
```

Lighting parameters are global scene properties, not per-stimulus, matching the conventions
of most real-time rendering systems. An experiment that uses only `Shading3D::Unlit` stimuli
can ignore them entirely.

### 10.2 Protobuf additions

```protobuf
// Add to SystemCmd:
CmdSetCamera        set_camera    = 70;
CmdSetAmbientLight  set_ambient   = 71;
CmdSetSunLight      set_sun       = 72;

// Add to object creation:
CmdCreateBox3D      create_box    = 40;
CmdCreateSphere3D   create_sphere = 41;
CmdCreateCylinder3D create_cyl    = 42;
CmdCreatePlane3D    create_plane  = 43;
CmdCreateCorridor   create_corr   = 44;
CmdCreateMaze       create_maze   = 45;
CmdLoadMesh         load_mesh     = 46;
CmdLoadGaussSplat   load_splat    = 47;

// Sub-messages
message CmdSetCamera       { Vec3 pos=1; float yaw=2; float pitch=3; float roll=4; float fov_y=5; float near_cm=6; float far_cm=7; }
message CmdSetAmbientLight { Vec3 colour = 1; }
message CmdSetSunLight     { Vec3 direction = 1; Vec3 colour = 2; }

message CmdCreateBox3D     { Vec3 half_size=1; Material3DMsg mat=2; }
message CmdCreateSphere3D  { float radius=1; uint32 rings=2; uint32 sectors=3; Material3DMsg mat=4; }
message CmdCreateCylinder3D{ float radius=1; float height=2; uint32 segments=3; Material3DMsg mat=4; }
message CmdCreatePlane3D   { float half_w=1; float half_d=2; Material3DMsg mat=3; }

message Material3DMsg {
    Color  albedo    = 1;
    Vec3   emissive  = 2;
    float  roughness = 3;
    uint32 shading   = 4;  // 0=unlit, 1=phong
}

message CmdCreateCorridor {
    float width=1; float height=2; float length=3;
    Color wall_col=4; Color floor_col=5; Color ceil_col=6;
    uint32 wall_tex=7; uint32 floor_tex=8; uint32 ceil_tex=9;
    float cue_period=10; Color cue_col=11;
}

message CmdLoadMesh       { string path=1; bool skeletal_anim=2; }
message CmdLoadGaussSplat { string path=1; uint32 sh_degree=2; }
```

### 10.3 Per-stimulus commands for 3-D

The existing per-stimulus commands (`MoveTo`, `SetEnabled`, `SetOrientation`, `SetParam`,
`MoveToFront`) all apply to 3-D stimuli unchanged in semantics. The following 3-D-specific
commands are added to `StimulusCmd`:

```protobuf
// Additional StimulusCmd variants for 3-D:
CmdSetTransform3D   set_transform3d = 20;  // full pose in one message
CmdSetMaterial      set_material    = 21;
CmdSetAnimFrame     set_anim_frame  = 22;  // for skeletal mesh animation
```

---

## 11. Impact on Animations

### 11.1 The camera as an animation target

The camera is assigned a sentinel handle `CAMERA_HANDLE = 0x0000_FFFE` (below the animation
range `0x8000` but safely outside the stimulus range). Any animation type can target the
camera handle. When `advance()` calls `stimuli.get_mut(handle)` and the handle is the camera
sentinel, it instead calls into the camera object.

This is the cleanest design: no new animation types are needed just for camera control.
`AnimExternalPos` (shared memory), `AnimHarmonic`, `AnimLineSegPath`, and all other existing
animations work for the camera out of the box.

### 11.2 New animation types for 3-D navigation

Two new animation types are valuable for 3-D experiments:

#### `AnimFlythrough`

Follows a preloaded camera path (position + orientation keyframes, interpolated with
Catmull-Rom splines). Used for passive viewing paradigms where the animal watches a
pre-recorded trajectory through a virtual environment.

```rust
pub struct AnimFlythrough {
    pub keyframes:      Vec<CameraKeyframe>,
    pub time:           f32,            // current time, seconds
    pub speed:          f32,            // playback rate
    pub stimulus_handle: Option<u32>,   // must be CAMERA_HANDLE
    pub final_action:   FinalActionMask,
}

pub struct CameraKeyframe {
    pub time:    f32,
    pub pos:     glam::Vec3,
    pub yaw:     f32,
    pub pitch:   f32,
}
```

#### `AnimLinearNav`

Moves the camera along the local forward axis at a constant speed. Used for treadmill-driven
linear corridor navigation:

```rust
pub struct AnimLinearNav {
    pub speed:           Deferred<f32>,   // cm/s, driven by treadmill via SetAnimParam
    pub stimulus_handle: Option<u32>,     // CAMERA_HANDLE
    pub final_action:    FinalActionMask,
}
```

`SetAnimParam(mode=0, value=speed_cm_s)` updates the speed every frame from treadmill data.
This is cleaner than using `AnimExternalPos` when the treadmill outputs velocity rather than
absolute position.

### 11.3 Existing animations that work for 3-D without modification

| Animation | 3-D use case |
|---|---|
| `AnimExternalPos` | Absolute position from eye tracker / treadmill (x, z) |
| `AnimHarmonic` | Oscillating camera or object for psychophysics |
| `AnimLinearRange` | Fade in/out via `set_anim_param` on `opacity_scale` |
| `AnimFlash` | Briefly show a 3-D object (N frames) |
| `AnimLineSegPath` | Camera follows a piecewise-linear corridor path |
| `AnimZmqPos` | Remote-controlled camera position over network |

---

## 12. Crate Dependencies for 3-D

```toml
# Phase A (infrastructure)
glam = { version = "0.29", features = ["bytemuck"] }

# Phase D (mesh loading)
gltf = "1"       # glTF 2.0 parser, pure Rust
tobj = "4"       # OBJ/MTL parser, pure Rust, fallback format

# Phase E (Gaussian splatting)
ply-rs = "0.1"   # .ply file parser for loading trained 3DGS scenes
# No dedicated 3DGS crate exists yet at planning time; the renderer is implemented
# directly using wgpu compute shaders.
```

`glam` is the only hard new dependency before Phase D. It has no transitive native dependencies
and compiles in seconds.

---

## 13. Open Questions

### 13.1 Depth buffer precision and corridor length

Corridors and mazes can be long (tens of metres). The default `near=1 cm, far=50 000 cm`
gives a depth precision of approximately 0.02 cm at 10 m and 5 cm at 500 m with a 32-bit
float depth buffer. This is adequate for most experiments. If longer ranges are needed,
a **reversed-Z** depth buffer (`near=far, far=near` in the projection, `DepthCompare::Greater`)
dramatically improves precision at distance and is straightforward to implement in wgpu.

### 13.2 Anti-aliasing

The current 2-D pipeline uses no anti-aliasing (not needed for sharp stimulus edges). For 3-D
scenes, especially corridors with sharp wall edges, aliasing can be distracting. Options:

- **MSAA ×4**: supported natively in wgpu (`sample_count: 4` on the render pipeline). Requires
  a multisampled colour target and a resolve step. Low complexity, good quality for geometry.
- **FXAA / TAA**: post-process passes. More complex but work on any hardware.
- **None**: may be acceptable for experiments where the animal's perception of aliasing is not
  a confound (e.g. optic flow experiments with textures rather than edges).

Decision deferred to Phase B implementation, when a concrete stimulus to test against exists.

### 13.3 Mixing 2-D and 3-D stimuli: Z-fighting and ordering

If a 2-D billboard (e.g. a fixation cross) needs to appear embedded in the 3-D scene (not
always on top), the current "2-D pass has no depth test" approach is insufficient. Options:

- **Render the billboard in the 3-D pass** as a camera-facing quad, with a depth value. This
  requires the billboard to be a `PlaneStimulus3D` with `Shading3D::Unlit`.
- **Keep all 2-D stimuli on top** (current design). This works for fixation crosses, cues, and
  overlays. It does not work for stimuli that should be occluded by 3-D geometry.

The current design (2-D always on top) is correct for the majority of use cases and is kept
as the default. A `Stimulus::Billboard3D` variant that renders in the 3-D pass is a possible
Phase B or C addition.

### 13.4 Deferred mode and 3-D

The `Deferred<T>` mechanism works identically for 3-D stimuli. The only additional
consideration is that `Deferred<Camera3D>` must be flipped atomically with all stimulus flips,
so that a batch update of camera + stimulus positions all becomes visible on the same frame.
This is handled automatically: the `pending_flip` loop in the render thread iterates
`SceneState` and calls `scene.camera.flip()` alongside `stimulus.flip()` for each stimulus.

### 13.5 Gaussian splatting and the depth buffer

3DGS rendering uses alpha blending over sorted Gaussians, producing a correct composited
image. However, it does not write to the depth buffer, so 3-D geometry drawn after the splat
pass would appear in front of it. If a scene mixes 3DGS backgrounds with explicit 3-D objects,
a depth pre-pass or screen-space depth reconstruction may be needed. This is an active research
problem and is explicitly out of scope until Phase E is reached.

---

*End of document. See `PLAN.md` for phase ordering, `STIMULUS_DATA_MODEL.md` for the
composition model, and `INPUT_LATENCY.md` for position control design.*