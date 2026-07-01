use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex,
    StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};
use lyon_tessellation::math::{Angle, Box2D, Transform, Vector, point};
use lyon_tessellation::path::{Path, Winding};

use crate::geom::Vertex;
use crate::Color;
use crate::scene::photodiode::PhotoDiodeState;
use crate::scene::stimulus::{
    CircleStimulus, DrawMode, EllipseStimulus, RectStimulus, ShapeAppearance, Stimulus,
    Transform2D,
};

pub struct ShapeTessellationResult {
    pub fill:   (Vec<Vertex>, Vec<u32>),
    pub stroke: (Vec<Vertex>, Vec<u32>),
}

// ── Coordinate conversion ─────────────────────────────────────────────────────

/// Pixel-space (centre = 0, Y-up) → clip space (centre = 0, Y-up), z = 0.
/// The renderer's clip space is Y-up (top of screen = +1), matching the text
/// path; do NOT negate Y here or shapes render vertically flipped.
fn px_to_ndc(x: f32, y: f32, half_w: f32, half_h: f32) -> [f32; 3] {
    [x / half_w, y / half_h, 0.0]
}

const FRONT_NORMAL: [f32; 3] = [0.0, 0.0, 1.0];
const NO_UV: [f32; 2] = [0.0, 0.0];

// ── Public entry point ────────────────────────────────────────────────────────

/// Tessellate a shape stimulus into fill and stroke geometry ready for GPU upload.
/// All positions are in NDC space, z = 0 (flat / billboard geometry).
/// Returns empty vecs for invisible stimuli, and `None` for non-shape stimuli.
pub fn tessellate_shape_stimulus(
    stimulus: &Stimulus,
    screen_size: (u32, u32),
) -> Option<ShapeTessellationResult> {
    let empty = ShapeTessellationResult { fill: (vec![], vec![]), stroke: (vec![], vec![]) };
    if !stimulus.is_shape() {
        return None;
    }
    if !stimulus.flags().is_visible() {
        return Some(empty);
    }
    let half_w = screen_size.0 as f32 * 0.5;
    let half_h = screen_size.1 as f32 * 0.5;

    Some(match stimulus {
        Stimulus::Rect(s)    => tessellate_rect(s, half_w, half_h),
        Stimulus::Ellipse(s) => tessellate_ellipse(s, half_w, half_h),
        Stimulus::Circle(s)  => tessellate_circle(s, half_w, half_h),
        _ => unreachable!("is_shape() checked"),
    })
}

// ── Per-type tessellators ─────────────────────────────────────────────────────

fn tessellate_rect(s: &RectStimulus, half_w: f32, half_h: f32) -> ShapeTessellationResult {
    let [hw, hh] = s.size.live;
    let mut builder = Path::builder();
    builder.add_rectangle(
        &Box2D::new(point(-hw, -hh), point(hw, hh)),
        Winding::Positive,
    );
    let path = builder.build();
    tessellate_path(&path, s.common.transform.live, &s.common.appearance.live, half_w, half_h)
}

fn tessellate_circle(s: &CircleStimulus, half_w: f32, half_h: f32) -> ShapeTessellationResult {
    let mut builder = Path::builder();
    builder.add_circle(point(0.0, 0.0), s.radius.live, Winding::Positive);
    let path = builder.build();
    tessellate_path(&path, s.common.transform.live, &s.common.appearance.live, half_w, half_h)
}

fn tessellate_ellipse(s: &EllipseStimulus, half_w: f32, half_h: f32) -> ShapeTessellationResult {
    let [rx, ry] = s.radii.live;
    let mut builder = Path::builder();
    builder.add_ellipse(
        point(0.0, 0.0),
        Vector::new(rx, ry),
        Angle::zero(),
        Winding::Positive,
    );
    let path = builder.build();
    tessellate_path(&path, s.common.transform.live, &s.common.appearance.live, half_w, half_h)
}

// ── Shared tessellation ───────────────────────────────────────────────────────

/// Fill and stroke-tessellate a lyon `Path`, applying `transform` and converting
/// to NDC.  The path is expected to be in local pixel-space (origin = centre).
fn tessellate_path(
    path: &Path,
    transform: Transform2D,
    appearance: &ShapeAppearance,
    half_w: f32,
    half_h: f32,
) -> ShapeTessellationResult {
    let xf = transform.to_transform();
    let tess_fill = matches!(appearance.draw_mode, DrawMode::Fill | DrawMode::FillAndStroke);
    let tess_stroke = matches!(
        appearance.draw_mode,
        DrawMode::Stroke | DrawMode::FillAndStroke
    );
    ShapeTessellationResult {
        fill: if tess_fill {
            tessellate_fill(path, &xf, appearance.fill_color, half_w, half_h)
        } else {
            (vec![], vec![])
        },
        stroke: if tess_stroke {
            tessellate_stroke(path, &xf, appearance, half_w, half_h)
        } else {
            (vec![], vec![])
        },
    }
}

fn tessellate_fill(
    path: &Path,
    xf: &Transform,
    color: Color,
    half_w: f32,
    half_h: f32,
) -> (Vec<Vertex>, Vec<u32>) {
    let mut buffers: VertexBuffers<Vertex, u32> = VertexBuffers::new();
    let mut tess = FillTessellator::new();
    let ok = tess.tessellate_path(
        path,
        &FillOptions::default(),
        &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| {
            let p = xf.transform_point(v.position());
            Vertex { position: px_to_ndc(p.x, p.y, half_w, half_h), normal: FRONT_NORMAL, uv: NO_UV, color }
        }),
    );
    if ok.is_err() { return (vec![], vec![]); }
    (buffers.vertices, buffers.indices)
}

fn tessellate_stroke(
    path: &Path,
    xf: &Transform,
    appearance: &ShapeAppearance,
    half_w: f32,
    half_h: f32,
) -> (Vec<Vertex>, Vec<u32>) {
    let sw = appearance.stroke_width;
    if sw <= 0.0 { return (vec![], vec![]); }
    let color = appearance.outline_color;
    let mut buffers: VertexBuffers<Vertex, u32> = VertexBuffers::new();
    let mut tess = StrokeTessellator::new();
    let ok = tess.tessellate_path(
        path,
        &StrokeOptions::default().with_line_width(sw),
        &mut BuffersBuilder::new(&mut buffers, |v: StrokeVertex| {
            let p = xf.transform_point(v.position());
            Vertex { position: px_to_ndc(p.x, p.y, half_w, half_h), normal: FRONT_NORMAL, uv: NO_UV, color }
        }),
    );
    if ok.is_err() { return (vec![], vec![]); }
    (buffers.vertices, buffers.indices)
}

// ── Photodiode corner square ──────────────────────────────────────────────────

/// Tessellate the photodiode indicator as a 60×60 px square in a screen corner.
/// Returns empty vecs when the photodiode is disabled.
pub fn tessellate_photodiode(
    state: &PhotoDiodeState,
    screen_size: (u32, u32),
) -> (Vec<Vertex>, Vec<u32>) {
    if !state.enabled {
        return (vec![], vec![]);
    }
    let color = if state.lit { Color::WHITE } else { Color::BLACK };
    let size = 60.0_f32;
    let half_w = screen_size.0 as f32 * 0.5;
    let half_h = screen_size.1 as f32 * 0.5;
    let (x0, x1, y0, y1) = if state.position == 0 {
        (-half_w, -half_w + size, -half_h, -half_h + size)
    } else {
        (half_w - size, half_w, -half_h, -half_h + size)
    };
    let v = |x, y| Vertex { position: px_to_ndc(x, y, half_w, half_h), normal: FRONT_NORMAL, uv: NO_UV, color };
    let vertices = vec![v(x0, y0), v(x1, y0), v(x1, y1), v(x0, y1)];
    let indices  = vec![0, 1, 2, 0, 2, 3];
    (vertices, indices)
}
