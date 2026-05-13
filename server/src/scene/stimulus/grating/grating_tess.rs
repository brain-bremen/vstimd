use crate::geom::Vertex;

use super::grating_stimulus::GratingStimulus;

pub fn tessellate_grating(s: &GratingStimulus, half_w: f32, half_h: f32) -> (Vec<Vertex>, Vec<u32>) {
    let [hw, hh] = s.size.live;
    let [cx, cy] = s.transform.live.pos;
    // Axis-aligned quad in NDC — the fragment shader handles orientation internally.
    let corners = [
        (cx - hw, cy - hh),
        (cx + hw, cy - hh),
        (cx + hw, cy + hh),
        (cx - hw, cy + hh),
    ];
    let v = |(x, y): (f32, f32)| Vertex {
        position: [x / half_w, -y / half_h, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [0.0, 0.0],
        color: [0.0; 4],
    };
    let vertices = corners.map(v).to_vec();
    let indices = vec![0, 1, 2, 0, 2, 3];
    (vertices, indices)
}
