/// CPU-side vertex type shared by the render and scene modules.
/// Plain data (`bytemuck::Pod`) with no GPU handles — safe to use outside `render/`.
///
/// A single type covers all stimulus geometry (2-D flat shapes, billboards, 3-D meshes).
/// Unused fields are zeroed by convention:
///   - Flat / unlit shapes: `normal = [0, 0, 1]`, `uv = [0, 0]` (samples white pixel)
///   - Solid-colour shapes: `uv = [0, 0]`, `color = fill_color`
///   - Textured shapes:     `uv` = real UV, `color = [1,1,1,1]` (no tint)
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}
