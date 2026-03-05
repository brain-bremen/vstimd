use kurbo::ParamCurve as _;
use kurbo::Shape as _;

use crate::scene::stimulus::{
    DiscStimulus, EllipseStimulus, RectStimulus, Stimulus, Transform2D,
};

// ── Vertex ────────────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

// ── Coordinate conversion ─────────────────────────────────────────────────────

/// Pixel-space (centre = 0, Y-up) → wgpu NDC (centre = 0, Y-down).
fn px_to_ndc(x: f32, y: f32, half_w: f32, half_h: f32) -> [f32; 2] {
    [x / half_w, -y / half_h]
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Tessellate a stimulus into `(vertices, indices)` ready for upload.
/// All positions are in wgpu NDC space.
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
        Stimulus::Rect(s)    => tessellate_rect(s, half_w, half_h),
        Stimulus::Ellipse(s) => tessellate_ellipse(s, half_w, half_h),
        Stimulus::Disc(s)    => tessellate_disc(s, half_w, half_h),
        // Remaining types: tessellation added in Phase 2
        _ => (vec![], vec![]),
    }
}

// ── Per-type tessellators ─────────────────────────────────────────────────────

/// Rectangle: 4 corners → centroid fan (2 triangles per half-diagonal = 4 triangles).
fn tessellate_rect(s: &RectStimulus, half_w: f32, half_h: f32) -> (Vec<Vertex>, Vec<u32>) {
    let [hw, hh] = s.size.live;
    let color = s.appearance.live.fill_color;
    let affine = s.transform.live.to_affine();

    // Corners in local space (CCW starting bottom-left)
    let local = [
        kurbo::Point::new(-(hw as f64), -(hh as f64)),
        kurbo::Point::new(hw as f64, -(hh as f64)),
        kurbo::Point::new(hw as f64, hh as f64),
        kurbo::Point::new(-(hw as f64), hh as f64),
    ];

    let ndc: Vec<[f32; 2]> = local
        .iter()
        .map(|&p| {
            let tp = affine * p;
            px_to_ndc(tp.x as f32, tp.y as f32, half_w, half_h)
        })
        .collect();

    // Centroid = transform applied to local origin
    let c = affine * kurbo::Point::ZERO;
    let cn = px_to_ndc(c.x as f32, c.y as f32, half_w, half_h);

    let vertices = vec![
        Vertex { position: cn,     color }, // 0: centroid
        Vertex { position: ndc[0], color }, // 1
        Vertex { position: ndc[1], color }, // 2
        Vertex { position: ndc[2], color }, // 3
        Vertex { position: ndc[3], color }, // 4
    ];
    let indices = vec![0, 1, 2, 0, 2, 3, 0, 3, 4, 0, 4, 1];

    (vertices, indices)
}

/// Disc: kurbo Circle → centroid fan over sampled outline.
fn tessellate_disc(s: &DiscStimulus, half_w: f32, half_h: f32) -> (Vec<Vertex>, Vec<u32>) {
    let path = kurbo::Circle::new(kurbo::Point::ZERO, s.radius.live as f64).to_path(1.0);
    tessellate_filled_path(&path, s.transform.live, s.appearance.live.fill_color, half_w, half_h)
}

/// Ellipse: kurbo Ellipse → centroid fan over sampled outline.
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

// ── Shared helper ─────────────────────────────────────────────────────────────

/// Sample a closed kurbo path, apply `transform`, convert to NDC, build a centroid fan.
fn tessellate_filled_path(
    path: &kurbo::BezPath,
    transform: Transform2D,
    color: [f32; 4],
    half_w: f32,
    half_h: f32,
) -> (Vec<Vertex>, Vec<u32>) {
    let affine = transform.to_affine();
    let mut outline: Vec<[f32; 2]> = Vec::new();

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

    // Centroid = transform applied to local origin
    let c = affine * kurbo::Point::ZERO;
    let cn = px_to_ndc(c.x as f32, c.y as f32, half_w, half_h);

    let mut vertices = vec![Vertex { position: cn, color }];
    for pt in &outline {
        vertices.push(Vertex { position: *pt, color });
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
