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

pub struct GpuBuffers {
    pub meshes: HashMap<u32, VkMesh>,
    mem_props: vk::PhysicalDeviceMemoryProperties,
    // Cached photodiode render state — used to skip redundant uploads
    pub pd_enabled: bool,
    pub pd_lit: Option<bool>,
    pub pd_position: u32,
    pub pd_screen_size: (u32, u32),
}

impl GpuBuffers {
    pub fn new(instance: &ash::Instance, physical_device: vk::PhysicalDevice) -> Self {
        let mem_props =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };
        Self {
            meshes: HashMap::new(),
            mem_props,
            pd_enabled: false,
            pd_lit: None,
            pd_position: 0,
            pd_screen_size: (0, 0),
        }
    }

    pub fn upload(
        &mut self,
        handle: u32,
        device: &ash::Device,
        verts: &[Vertex],
        idxs: &[u32],
    ) {
        if let Some(old) = self.meshes.remove(&handle) {
            unsafe { old.destroy(device) };
        }
        if verts.is_empty() {
            return;
        }
        let (vb, vm) = self.alloc_upload(device, vk::BufferUsageFlags::VERTEX_BUFFER, bytemuck::cast_slice(verts));
        let (ib, im) = self.alloc_upload(device, vk::BufferUsageFlags::INDEX_BUFFER, bytemuck::cast_slice(idxs));
        self.meshes.insert(handle, VkMesh {
            vertex_buffer: vb,
            vertex_memory: vm,
            index_buffer: ib,
            index_memory: im,
            index_count: idxs.len() as u32,
        });
    }

    /// Overwrite vertex data in an existing buffer without reallocation.
    /// The new slice must be the same byte size as what was originally uploaded.
    pub fn overwrite_vertices(&self, handle: u32, device: &ash::Device, verts: &[Vertex]) {
        let Some(mesh) = self.meshes.get(&handle) else { return };
        let data: &[u8] = bytemuck::cast_slice(verts);
        unsafe {
            let ptr = device
                .map_memory(mesh.vertex_memory, 0, data.len() as vk::DeviceSize, vk::MemoryMapFlags::empty())
                .expect("map") as *mut u8;
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            device.unmap_memory(mesh.vertex_memory);
        }
    }

    pub fn destroy_all(&mut self, device: &ash::Device) {
        for mesh in self.meshes.values() {
            unsafe { mesh.destroy(device) };
        }
        self.meshes.clear();
    }

    fn alloc_upload(
        &self,
        device: &ash::Device,
        usage: vk::BufferUsageFlags,
        data: &[u8],
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
        let mem_type = self
            .find_memory_type(
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

    fn find_memory_type(&self, filter: u32, flags: vk::MemoryPropertyFlags) -> Option<u32> {
        (0..self.mem_props.memory_type_count).find(|&i| {
            (filter & (1 << i)) != 0
                && self.mem_props.memory_types[i as usize].property_flags.contains(flags)
        })
    }
}
