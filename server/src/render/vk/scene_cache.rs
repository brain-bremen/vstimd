use ash::vk;

use super::buffers::{PhotodiodeCache, SolidMeshCache};
use super::text_pipeline::TextMeshCache;

/// Unified GPU-side cache for all stimulus types.
///
/// One `SceneCache` lives in `RenderState` and is passed as a single `&mut`
/// argument to `render_frame`.  Each field is the GPU buffer store for one
/// stimulus category; new categories (3-D meshes, video frames, …) add a
/// field here.
pub struct SceneCache {
    pub solid:      SolidMeshCache,
    pub text:       TextMeshCache,
    pub photodiode: PhotodiodeCache,
}

impl SceneCache {
    pub fn new(instance: &ash::Instance, physical_device: vk::PhysicalDevice) -> Self {
        Self {
            solid:      SolidMeshCache::new(instance, physical_device),
            text:       TextMeshCache::new(instance, physical_device),
            photodiode: PhotodiodeCache::default(),
        }
    }

    pub fn destroy_all(&mut self, device: &ash::Device) {
        self.solid.destroy_all(device);
        self.text.destroy_all(device);
    }
}
