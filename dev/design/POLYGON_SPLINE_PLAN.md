# Polygon and Spline Stimulus Plan

## Background

vstimd currently supports Rect, Circle, Ellipse, and Grating. Two new stimulus types are
planned:

1. **PolygonStimulus** — arbitrary vertex-defined polygon/polyline, emulating PsychoPy's
   `ShapeStim`. Supports named shapes (cross, star, arrow), equilateral N-gons, and custom
   vertex arrays.

2. **SplineStimulus** — vstimd-native smooth parametric curve. Supports Catmull-Rom (control
   points lie on the curve) and cubic Bézier (with explicit handle points). No PsychoPy
   equivalent.

**Spline note:** PsychoPy has no built-in spline support. Users approximate curves by passing
dense vertex arrays. vstimd's `SplineStimulus` stores control points and evaluates the curve
server-side using lyon.

---

## Design Decisions

| Decision | Choice | Reason |
|---|---|---|
| Polygon tessellation | `earcutr` crate (ear-clipping) | Named shapes (cross, star7, arrow) are concave — fan-from-centroid produces incorrect triangles |
| Heap vertex storage | Manual live/copy `Vec<[f32;2]>` pair | `Deferred<T>` requires `T: Copy`; Vec is not |
| Named shapes / N-gons | Resolved client-side in Python | Keeps server simple; mirrors PsychoPy's own architecture |
| `closeShape` | Immutable after creation | PsychoPy disallows dynamic changes; no deferred bookkeeping needed |
| `SplineStimulus` placement | `ShapeStimulus` variant | Shares tessellated-geometry pipeline (Grating is a top-level variant because it uses a fragment shader) |
| Spline evaluation | lyon `Path` | Cubic-path interpolation utilities fed directly into lyon tessellation |
| Catmull-Rom → Bézier | Standard tangent formula: `C1 = P[i] + (P[i+1]−P[i−1])/6`, `C2 = P[i+1] − (P[i+2]−P[i])/6` | Produces smooth C1-continuous cubic spline |
| Cubic Bézier encoding | `[P0, C1, C2, P1, C1, C2, P2, …]` — 1+3N points for N segments | Compact, unambiguous |
| Stroke/outline | Supported | Existing shape rendering supports fill, stroke, and fill+stroke draw modes |

---

## Part A — PolygonStimulus

### A1 — Dependency

`server/Cargo.toml`: add `earcutr = "0.4"`.

### A2 — Rust struct

`server/src/scene/stimulus/primitive_shapes.rs`:

```rust
pub struct PolygonStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub appearance: Deferred<ShapeAppearance>,
    pub close_shape: bool,               // immutable after creation
    pub vertices_live: Vec<[f32; 2]>,    // pixel-space, relative to centre, Y-up
    pub vertices_copy: Vec<[f32; 2]>,    // double-buffer copy
}
// make_copy: clone vertices_copy ← vertices_live + defer fields
// flip:      std::mem::swap(vertices_live, vertices_copy) + flip defer fields
```

### A3 — `ShapeStimulus` enum

`server/src/scene/stimulus/shape_stimulus.rs`:
- Add `Polygon(PolygonStimulus)` variant
- Extend `shape_field!` macro and `type_name()` match

### A4 — Tessellation

`server/src/render/tess.rs`:

```rust
fn tessellate_polygon(s: &PolygonStimulus, half_w: f32, half_h: f32) -> (Vec<Vertex>, Vec<u32>) {
    // open polylines or < 3 verts → empty (stroke-only, future work)
    if !s.close_shape || s.vertices_live.len() < 3 { return (vec![], vec![]); }
    let flat: Vec<f64> = s.vertices_live.iter().flat_map(|&[x,y]| [x as f64, y as f64]).collect();
    let Ok(indices) = earcutr::earcut(&flat, &[], 2) else { return (vec![], vec![]); };
    let color = s.appearance.live.fill_color;
    let xf = s.transform.live.to_transform();
    let vertices: Vec<Vertex> = s.vertices_live.iter().map(|&[x,y]| {
        let p = xf.transform_point(point(x, y));
        Vertex { position: px_to_ndc(p.x, p.y, half_w, half_h),
                 normal: FRONT_NORMAL, uv: NO_UV, color }
    }).collect();
    (vertices, indices.iter().map(|&i| i as u32).collect())
}
```

Add `ShapeStimulus::Polygon(s) => tessellate_polygon(s, half_w, half_h)` to
`tessellate_stimulus`.

### A5 — Proto

**`proto/vstimd/v1/common.proto`**
```proto
STIMULUS_TYPE_POLYGON = 11;
```

**`proto/vstimd/v1/stimuli_2d.proto`**
```proto
message CreatePolygonRequest {
  repeated Vec2 vertices    = 1;   // pixel-space, Y-up, relative to centre
  bool          close_shape = 2;
  Color         fill        = 3;
  string        id          = 4;
  string        name        = 5;
}

message SetPolygonVerticesRequest { repeated Vec2 vertices = 1; }

message PolygonParams { repeated Vec2 vertices = 1; bool close_shape = 2; }
// Add PolygonParams polygon = 5 to StimulusParams.oneof shape
```

**`proto/vstimd/v1/service.proto`**
```proto
CreatePolygonRequest      create_polygon         = 14;   // creation block
SetPolygonVerticesRequest set_polygon_vertices   = 43;   // mutation block
```

### A6 — Command dispatch

`server/src/scene/command.rs`:
- `command_summary`: `CreatePolygon`, `SetPolygonVertices` arms
- `handle_system_command`: `CreatePolygon` → build `PolygonStimulus`, insert (follow `cmd_create_circle` pattern)
- `handle_stimulus_command`: `SetPolygonVertices` → type-guard with `err_wrong_type`, update `vertices_live`, mark dirty
- `cmd_query_stimulus`: add `Polygon` params

---

## Part B — SplineStimulus

### B1 — Rust struct

`server/src/scene/stimulus/primitive_shapes.rs` (or a dedicated `spline_stimulus.rs`):

```rust
#[derive(Clone, Copy)]
pub enum SplineType { CatmullRom, CubicBezier }

pub struct SplineStimulus {
    pub flags: StimulusFlags,
    pub transform: Deferred<Transform2D>,
    pub appearance: Deferred<ShapeAppearance>,
    pub spline_type: SplineType,               // immutable after creation
    pub close_shape: bool,                     // immutable after creation
    pub control_points_live: Vec<[f32; 2]>,
    pub control_points_copy: Vec<[f32; 2]>,
}
// make_copy / flip: same pattern as PolygonStimulus
```

### B2 — `ShapeStimulus` enum

Add `Spline(SplineStimulus)` variant; update `shape_field!` macro and `type_name()`.

### B3 — Tessellation

`server/src/render/tess.rs`:

```rust
fn catmull_rom_to_path(pts: &[[f32;2]], closed: bool) -> lyon_tessellation::path::Path {
    // For each segment (P[i-1], P[i], P[i+1], P[i+2]):
    //   C1 = P[i]   + (P[i+1] - P[i-1]) / 6
    //   C2 = P[i+1] - (P[i+2] - P[i])   / 6
    // Closed: wrap indices mod N
}

fn cubic_bezier_to_path(pts: &[[f32;2]], closed: bool) -> lyon_tessellation::path::Path {
    // Encoding: [P0, C1, C2, P1, C1, C2, P2, ...] — 1+3N points for N segments
    // Validate: (pts.len() - 1) % 3 == 0
}

fn tessellate_spline(s: &SplineStimulus, half_w: f32, half_h: f32) -> (Vec<Vertex>, Vec<u32>) {
    let path = match s.spline_type {
        SplineType::CatmullRom  => catmull_rom_to_bezpath(&s.control_points_live, s.close_shape),
        SplineType::CubicBezier => cubic_bezier_to_bezpath(&s.control_points_live, s.close_shape),
    };
    if !s.close_shape { return (vec![], vec![]); }  // open: no fill geometry
    tessellate_filled_path(&path, s.transform.live, s.appearance.live.fill_color, half_w, half_h)
}
```

`tessellate_filled_path` already handles cubic `PathSeg` (tess.rs:143–157).

### B4 — Proto

**`proto/vstimd/v1/common.proto`**
```proto
STIMULUS_TYPE_SPLINE = 12;
```

**`proto/vstimd/v1/stimuli_2d.proto`**
```proto
enum SplineType {
  SPLINE_TYPE_UNSPECIFIED  = 0;
  SPLINE_TYPE_CATMULL_ROM  = 1;   // all control points lie ON the curve
  SPLINE_TYPE_CUBIC_BEZIER = 2;   // encoding: P0, (C1,C2,P1), (C1,C2,P2), …
}

message CreateSplineRequest {
  repeated Vec2 control_points = 1;
  SplineType    spline_type    = 2;
  bool          close_shape    = 3;
  Color         fill           = 4;
  string        id             = 5;
  string        name           = 6;
}

message SetSplineControlPointsRequest { repeated Vec2 control_points = 1; }

message SplineParams {
  repeated Vec2 control_points = 1;
  SplineType    spline_type    = 2;
  bool          close_shape    = 3;
}
// Add SplineParams spline = 6 to StimulusParams.oneof shape
```

**`proto/vstimd/v1/service.proto`**
```proto
CreateSplineRequest           create_spline              = 15;   // creation block
SetSplineControlPointsRequest set_spline_control_points  = 44;   // mutation block
```

### B5 — Command dispatch

`server/src/scene/command.rs`: same pattern as PolygonStimulus (system create + stimulus mutation + query params).

---

## Part C — Python Client

### C1 — Raw API

`client/python/vstimd/stimuli/stimuli_client.py`:

```python
def create_polygon(self, vertices, *, close_shape=True, r, g, b, a, x, y, id="", name="") -> int
def set_polygon_vertices(self, handle, vertices) -> None

def create_spline(self, control_points, *, spline_type="catmull_rom",
                  close_shape=False, r, g, b, a, x, y, id="", name="") -> int
def set_spline_control_points(self, handle, control_points) -> None
```

### C2 — PsychoPy compat: `ShapeStim`

**New: `client/python/vstimd/psychopy/visual/shape.py`**

```python
KNOWN_SHAPES = {
    "triangle":  [(0.0, 0.5), (-0.5, -0.5), (0.5, -0.5)],
    "rectangle": [(-0.5, 0.5), (0.5, 0.5), (0.5, -0.5), (-0.5, -0.5)],
    "cross":     [...],   # 12 vertices from psychopy knownShapes
    "star7":     [...],   # 14 vertices
    "arrow":     [...],   # 7 vertices
}

class ShapeStim:
    def __init__(self, win, vertices, *, lineColor=None, fillColor=None,
                 lineWidth=1.5, closeShape=True, pos=(0,0), size=1, ori=0.0, ...):
        # 1. Resolve: str → KNOWN_SHAPES, int → equilateral N-gon, array → scale by size
        # 2. Unit conversion (norm/deg/cm → pixels via _units helpers)
        # 3. win._conn.stimuli.create_polygon(resolved_verts, ...)

    def setVertices(self, verts): ...
    def setPos(self, pos): ...
    def setOri(self, deg): ...
    def setFillColor(self, color): ...
```

### C3 — PsychoPy compat: `Polygon`

**New: `client/python/vstimd/psychopy/visual/polygon.py`**

```python
class Polygon(ShapeStim):
    def __init__(self, win, edges=3, radius=0.5, **kwargs):
        verts = _calc_equilateral_vertices(edges, radius)
        super().__init__(win, vertices=verts, closeShape=True, **kwargs)
```

### C4 — vstimd-native: `Spline`

**New: `client/python/vstimd/visual/spline.py`** (NOT under `psychopy/`)

```python
class Spline:
    """vstimd-native smooth curve — no PsychoPy equivalent."""
    def __init__(self, win, control_points, *, spline_type="catmull_rom",
                 close_shape=False, fillColor=None, pos=(0,0), ...):
        ...
    def setControlPoints(self, pts): ...
```

### C5 — Exports

- `client/python/vstimd/psychopy/visual/__init__.py`: export `ShapeStim`, `Polygon`
- `client/python/vstimd/visual/__init__.py` (new or existing): export `Spline`

---

## Implementation Order

1. Proto files — drives Rust codegen via `build.rs`
2. `server/Cargo.toml` — add `earcutr`
3. `primitive_shapes.rs` — `PolygonStimulus`, `SplineStimulus`, `SplineType`
4. `shape_stimulus.rs` + `mod.rs` — new variants
5. `tess.rs` — `tessellate_polygon`, `tessellate_spline`, Catmull-Rom/Bézier helpers
6. `command.rs` — dispatch for both types
7. Python — `stimuli_client.py`, `shape.py`, `polygon.py`, `spline.py`

---

## Verification

1. `cargo build` — clean compile with `earcutr`
2. `cargo test` — existing tests pass
3. **Concavity test**: cross and star7 are concave — verify `earcutr` fills them correctly
   (fan-from-centroid would produce broken triangles here)
4. Python smoke test:
   ```python
   fix    = ShapeStim(win, vertices="cross", fillColor="white")
   tri    = ShapeStim(win, vertices=[(-50,0),(0,50),(50,0)])
   hex_   = Polygon(win, edges=6, radius=50)
   curve  = Spline(win, [(0,0),(100,50),(200,0)], spline_type="catmull_rom")
   bez    = Spline(win, [(0,0),(30,100),(70,100),(100,0)], spline_type="cubic_bezier")
   ```
5. `QueryStimulusRequest` round-trips `PolygonParams` and `SplineParams` correctly

---

## Critical Files

| File | Change |
|---|---|
| `server/Cargo.toml` | Add `earcutr = "0.4"` |
| `server/src/scene/stimulus/primitive_shapes.rs` | `PolygonStimulus`, `SplineStimulus`, `SplineType` |
| `server/src/scene/stimulus/shape_stimulus.rs` | `Polygon`, `Spline` variants + macro arms |
| `server/src/scene/stimulus/mod.rs` | Re-export new types |
| `server/src/render/tess.rs` | `tessellate_polygon`, `tessellate_spline`, path helpers |
| `server/src/scene/command.rs` | Create + mutation dispatch for both types |
| `proto/vstimd/v1/common.proto` | `POLYGON = 11`, `SPLINE = 12` |
| `proto/vstimd/v1/stimuli_2d.proto` | New messages, `SplineType` enum, params |
| `proto/vstimd/v1/service.proto` | Fields `14`, `15`, `43`, `44` |
| `client/python/vstimd/stimuli/stimuli_client.py` | `create_polygon`, `create_spline`, mutations |
| `client/python/vstimd/psychopy/visual/shape.py` | New: `ShapeStim` |
| `client/python/vstimd/psychopy/visual/polygon.py` | New: `Polygon` |
| `client/python/vstimd/visual/spline.py` | New: vstimd-native `Spline` |
