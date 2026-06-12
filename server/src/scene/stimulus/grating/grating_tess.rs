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
        color: crate::Color::TRANSPARENT,
    };
    let vertices = corners.map(v).to_vec();
    let indices = vec![0, 1, 2, 0, 2, 3];
    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::grating_params::GratingParams;

    fn stim_at(cx: f32, cy: f32, hw: f32, hh: f32) -> GratingStimulus {
        GratingStimulus::new([cx, cy], 0.0, [hw, hh], GratingParams::default())
    }

    #[test]
    fn produces_four_vertices_six_indices() {
        let s = stim_at(0.0, 0.0, 50.0, 50.0);
        let (verts, idx) = tessellate_grating(&s, 400.0, 300.0);
        assert_eq!(verts.len(), 4);
        assert_eq!(idx, vec![0, 1, 2, 0, 2, 3]);
    }

    #[test]
    fn ndc_coordinates_centered() {
        // Patch centred at origin, half-size 100×50, screen half 400×300.
        let s = stim_at(0.0, 0.0, 100.0, 50.0);
        let (verts, _) = tessellate_grating(&s, 400.0, 300.0);
        // Corner order: bottom-left, bottom-right, top-right, top-left
        // pixel (cx±hw, cy±hh) → NDC (x/half_w, -y/half_h)
        let xs: Vec<f32> = verts.iter().map(|v| v.position[0]).collect();
        let ys: Vec<f32> = verts.iter().map(|v| v.position[1]).collect();
        assert!(xs.iter().all(|x| (*x - (-100.0 / 400.0)).abs() < 1e-6 || (*x - (100.0 / 400.0)).abs() < 1e-6));
        // Y is negated: pixel y=-50 → NDC y=+50/300
        let expected_y_top    =  50.0_f32 / 300.0; // pixel y=-50 (top)
        let expected_y_bottom = -50.0_f32 / 300.0; // pixel y=+50 (bottom)
        assert!(ys.iter().any(|y| (y - expected_y_top).abs() < 1e-6));
        assert!(ys.iter().any(|y| (y - expected_y_bottom).abs() < 1e-6));
    }

    #[test]
    fn ndc_offset_patch() {
        // Patch at pixel (200, 0), half-size 100×100, screen half 400×300.
        let s = stim_at(200.0, 0.0, 100.0, 100.0);
        let (verts, _) = tessellate_grating(&s, 400.0, 300.0);
        let xs: Vec<f32> = verts.iter().map(|v| v.position[0]).collect();
        // left edge: (200-100)/400 = 0.25, right edge: (200+100)/400 = 0.75
        assert!(xs.iter().any(|x| (x - 0.25).abs() < 1e-6));
        assert!(xs.iter().any(|x| (x - 0.75).abs() < 1e-6));
    }
}
