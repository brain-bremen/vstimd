use std::collections::HashMap;

use ash::vk;

use crate::render::Vertex;

pub struct VkMesh {
    pub vertex_buffer: vk::Buffer,
    vertex_memory: vk::DeviceMemory,
    pub index_buffer: vk::Buffer,
    index_memory: vk::DeviceMemory,
    pub index_count: u32,
}

impl VkMesh {
    pub fn from_raw(
        vertex_buffer: vk::Buffer,
        vertex_memory: vk::DeviceMemory,
        index_buffer: vk::Buffer,
        index_memory: vk::DeviceMemory,
        index_count: u32,
    ) -> Self {
        Self { vertex_buffer, vertex_memory, index_buffer, index_memory, index_count }
    }

    pub unsafe fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_buffer(self.vertex_buffer, None);
            device.free_memory(self.vertex_memory, None);
            device.destroy_buffer(self.index_buffer, None);
            device.free_memory(self.index_memory, None);
        }
    }
}

/// Last-uploaded state for the photodiode indicator — used to skip redundant
/// GPU uploads when nothing has changed.
#[derive(Default)]
pub struct PhotodiodeCache {
    pub enabled: bool,
    pub lit: Option<bool>,
    pub position: u32,
    pub screen_size: (u32, u32),
}

/// GPU mesh cache for the solid triangle-list pipeline (shape stimuli and the
/// photodiode indicator).  Gratings use their own shared quad; future 3-D
/// stimuli will use a separate buffer type.
pub struct SolidMeshCache {
    pub fill_meshes:   HashMap<u32, VkMesh>,
    pub stroke_meshes: HashMap<u32, VkMesh>,
    mem_props: vk::PhysicalDeviceMemoryProperties,
}

impl SolidMeshCache {
    pub fn new(instance: &ash::Instance, physical_device: vk::PhysicalDevice) -> Self {
        let mem_props =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };
        Self {
            fill_meshes:   HashMap::new(),
            stroke_meshes: HashMap::new(),
            mem_props,
        }
    }

    /// Upload fill and stroke geometry for a handle, replacing any existing
    /// buffers.  Passing empty vertex slices for either skips that upload and
    /// removes the old mesh.
    pub fn upload(
        &mut self,
        handle: u32,
        device: &ash::Device,
        fill:   (&[Vertex], &[u32]),
        stroke: (&[Vertex], &[u32]),
    ) {
        Self::upload_mesh(&mut self.fill_meshes,   &self.mem_props, handle, device, fill.0,   fill.1);
        Self::upload_mesh(&mut self.stroke_meshes, &self.mem_props, handle, device, stroke.0, stroke.1);
    }

    pub fn destroy_all(&mut self, device: &ash::Device) {
        for mesh in self.fill_meshes.values() {
            unsafe { mesh.destroy(device) };
        }
        self.fill_meshes.clear();
        for mesh in self.stroke_meshes.values() {
            unsafe { mesh.destroy(device) };
        }
        self.stroke_meshes.clear();
    }

    /// Overwrite vertex data in the fill buffer for `handle` without
    /// reallocation.  The new slice must be the same byte size as the original.
    /// Used for the photodiode's colour-only updates.
    pub fn overwrite_fill_vertices(&self, handle: u32, device: &ash::Device, verts: &[Vertex]) {
        let Some(mesh) = self.fill_meshes.get(&handle) else { return };
        let data: &[u8] = bytemuck::cast_slice(verts);
        unsafe {
            let ptr = device
                .map_memory(mesh.vertex_memory, 0, data.len() as vk::DeviceSize, vk::MemoryMapFlags::empty())
                .expect("map") as *mut u8;
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            device.unmap_memory(mesh.vertex_memory);
        }
    }

    fn upload_mesh(
        map:       &mut HashMap<u32, VkMesh>,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        handle:    u32,
        device:    &ash::Device,
        verts:     &[Vertex],
        idxs:      &[u32],
    ) {
        if let Some(old) = map.remove(&handle) {
            unsafe { old.destroy(device) };
        }
        if verts.is_empty() || idxs.is_empty() {
            return;
        }
        let (vb, vm) = Self::alloc_upload(mem_props, device, vk::BufferUsageFlags::VERTEX_BUFFER, bytemuck::cast_slice(verts));
        let (ib, im) = Self::alloc_upload(mem_props, device, vk::BufferUsageFlags::INDEX_BUFFER,  bytemuck::cast_slice(idxs));
        map.insert(handle, VkMesh {
            vertex_buffer: vb,
            vertex_memory: vm,
            index_buffer:  ib,
            index_memory:  im,
            index_count:   idxs.len() as u32,
        });
    }

    fn alloc_upload(
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        device:    &ash::Device,
        usage:     vk::BufferUsageFlags,
        data:      &[u8],
    ) -> (vk::Buffer, vk::DeviceMemory) {
        let size = data.len() as vk::DeviceSize;
        let buf = unsafe {
            device
                .create_buffer(
                    &vk::BufferCreateInfo::default()
                        .size(size)
                        .usage(usage)
                        .sharing_mode(vk::SharingMode::EXCLUSIVE),
                    None,
                )
                .expect("failed to create buffer")
        };
        let reqs = unsafe { device.get_buffer_memory_requirements(buf) };
        let mem_type = Self::find_memory_type(
            mem_props,
            reqs.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .expect("no HOST_VISIBLE|HOST_COHERENT memory");
        let mem = unsafe {
            device
                .allocate_memory(
                    &vk::MemoryAllocateInfo::default()
                        .allocation_size(reqs.size)
                        .memory_type_index(mem_type),
                    None,
                )
                .expect("failed to allocate buffer memory")
        };
        unsafe {
            device.bind_buffer_memory(buf, mem, 0).unwrap();
            let ptr = device
                .map_memory(mem, 0, size, vk::MemoryMapFlags::empty())
                .expect("failed to map buffer") as *mut u8;
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            device.unmap_memory(mem);
        }
        (buf, mem)
    }

    fn find_memory_type(
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        filter: u32,
        flags: vk::MemoryPropertyFlags,
    ) -> Option<u32> {
        (0..mem_props.memory_type_count).find(|&i| {
            (filter & (1 << i)) != 0
                && mem_props.memory_types[i as usize].property_flags.contains(flags)
        })
    }
}
