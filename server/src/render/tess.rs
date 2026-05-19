use kurbo::ParamCurve as _;
use kurbo::Shape as _;

use crate::geom::Vertex;
use crate::scene::photodiode::PhotoDiodeState;
use crate::scene::stimulus::{DiscStimulus, EllipseStimulus, RectStimulus, ShapeStimulus, Stimulus, Transform2D};

// ── Coordinate conversion ─────────────────────────────────────────────────────

/// Pixel-space (centre = 0, Y-up) → NDC (centre = 0, Y-down), z = 0.
fn px_to_ndc(x: f32, y: f32, half_w: f32, half_h: f32) -> [f32; 3] {
    [x / half_w, -y / half_h, 0.0]
}

const FRONT_NORMAL: [f32; 3] = [0.0, 0.0, 1.0];
const NO_UV: [f32; 2] = [0.0, 0.0];

// ── Public entry point ────────────────────────────────────────────────────────

/// Tessellate a stimulus into `(vertices, indices)` ready for upload.
/// All positions are in NDC space, z = 0 (flat / billboard geometry).
/// Returns empty vecs for invisible stimuli or types not yet tessellated.
pub fn tessellate_stimulus(
    stimulus: &Stimulus,
    screen_size: (u32, u32),
) -> (Vec<Vertex>, Vec<u32>) {
    if !stimulus.is_visible() {
        return (vec![], vec![]);
    }
    let half_w = screen_size.0 as f32 * 0.5;
    let half_h = screen_size.1 as f32 * 0.5;

    match stimulus {
        Stimulus::Shape(s) => match s {
            ShapeStimulus::Rect(s)    => tessellate_rect(s, half_w, half_h),
            ShapeStimulus::Ellipse(s) => tessellate_ellipse(s, half_w, half_h),
            ShapeStimulus::Disc(s)    => tessellate_disc(s, half_w, half_h),
        },
        Stimulus::Grating(_) => (vec![], vec![]),
    }
}

// ── Per-type tessellators ─────────────────────────────────────────────────────

fn tessellate_rect(s: &RectStimulus, half_w: f32, half_h: f32) -> (Vec<Vertex>, Vec<u32>) {
    let [hw, hh] = s.size.live;
    let color = s.appearance.live.fill_color;
    let affine = s.transform.live.to_affine();

    let local = [
        kurbo::Point::new(-(hw as f64), -(hh as f64)),
        kurbo::Point::new(hw as f64, -(hh as f64)),
        kurbo::Point::new(hw as f64, hh as f64),
        kurbo::Point::new(-(hw as f64), hh as f64),
    ];

    let ndc: Vec<[f32; 3]> = local
        .iter()
        .map(|&p| {
            let tp = affine * p;
            px_to_ndc(tp.x as f32, tp.y as f32, half_w, half_h)
        })
        .collect();

    let c = affine * kurbo::Point::ZERO;
    let cn = px_to_ndc(c.x as f32, c.y as f32, half_w, half_h);

    let v = |position| Vertex { position, normal: FRONT_NORMAL, uv: NO_UV, color };
    let vertices = vec![v(cn), v(ndc[0]), v(ndc[1]), v(ndc[2]), v(ndc[3])];
    let indices = vec![0, 1, 2, 0, 2, 3, 0, 3, 4, 0, 4, 1];

    (vertices, indices)
}

fn tessellate_disc(s: &DiscStimulus, half_w: f32, half_h: f32) -> (Vec<Vertex>, Vec<u32>) {
    let path = kurbo::Circle::new(kurbo::Point::ZERO, s.radius.live as f64).to_path(1.0);
    tessellate_filled_path(&path, s.transform.live, s.appearance.live.fill_color, half_w, half_h)
}

fn tessellate_ellipse(s: &EllipseStimulus, half_w: f32, half_h: f32) -> (Vec<Vertex>, Vec<u32>) {
    let [rx, ry] = s.radii.live;
    let path = kurbo::Ellipse::new(
        kurbo::Point::ZERO,
        kurbo::Vec2::new(rx as f64, ry as f64),
        0.0,
    )
    .to_path(1.0);
    tessellate_filled_path(&path, s.transform.live, s.appearance.live.fill_color, half_w, half_h)
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
    let color: [f32; 4] = if state.lit {
        [1.0, 1.0, 1.0, 1.0]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };
    let size = 60.0_f32;
    let half_w = screen_size.0 as f32 * 0.5;
    let half_h = screen_size.1 as f32 * 0.5;
    // Pixel-space corners (Y-up, bottom = −half_h).
    let (x0, x1, y0, y1) = if state.position == 0 {
        (-half_w, -half_w + size, -half_h, -half_h + size)
    } else {
        (half_w - size, half_w, -half_h, -half_h + size)
    };
    let v = |x, y| Vertex {
        position: px_to_ndc(x, y, half_w, half_h),
        normal: FRONT_NORMAL,
        uv: NO_UV,
        color,
    };
    let vertices = vec![v(x0, y0), v(x1, y0), v(x1, y1), v(x0, y1)];
    let indices = vec![0, 1, 2, 0, 2, 3];
    (vertices, indices)
}

// ── Shared helper ─────────────────────────────────────────────────────────────

fn tessellate_filled_path(
    path: &kurbo::BezPath,
    transform: Transform2D,
    color: [f32; 4],
    half_w: f32,
    half_h: f32,
) -> (Vec<Vertex>, Vec<u32>) {
    let affine = transform.to_affine();
    let mut outline: Vec<[f32; 3]> = Vec::new();

    for seg in path.segments() {
        match seg {
            kurbo::PathSeg::Line(l) => {
                let p = affine * l.p0;
                outline.push(px_to_ndc(p.x as f32, p.y as f32, half_w, half_h));
            }
            kurbo::PathSeg::Cubic(c) => {
                for i in 0..16_usize {
                    let p = affine * c.eval(i as f64 / 16.0);
                    outline.push(px_to_ndc(p.x as f32, p.y as f32, half_w, half_h));
                }
            }
            kurbo::PathSeg::Quad(q) => {
                for i in 0..8_usize {
                    let p = affine * q.eval(i as f64 / 8.0);
                    outline.push(px_to_ndc(p.x as f32, p.y as f32, half_w, half_h));
                }
            }
        }
    }

    if outline.is_empty() {
        return (vec![], vec![]);
    }

    let c = affine * kurbo::Point::ZERO;
    let cn = px_to_ndc(c.x as f32, c.y as f32, half_w, half_h);

    let v = |position| Vertex { position, normal: FRONT_NORMAL, uv: NO_UV, color };
    let mut vertices = vec![v(cn)];
    for &pt in &outline {
        vertices.push(v(pt));
    }

    let n = outline.len() as u32;
    let mut indices = Vec::with_capacity((n * 3) as usize);
    for i in 0..n {
        indices.push(0);
        indices.push(1 + i);
        indices.push(1 + (i + 1) % n);
    }

    (vertices, indices)
}
