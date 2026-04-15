use std::collections::HashMap;

use ash::vk;

use crate::render::vertex::Vertex;

// ── Per-stimulus GPU mesh ─────────────────────────────────────────────────────

pub struct VkMesh {
    pub vertex_buffer: vk::Buffer,
    vertex_memory: vk::DeviceMemory,
    pub index_buffer: vk::Buffer,
    index_memory: vk::DeviceMemory,
    pub index_count: u32,
}

impl VkMesh {
    /// Free all Vulkan resources owned by this mesh.
    pub unsafe fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_buffer(self.vertex_buffer, None);
            device.free_memory(self.vertex_memory, None);
            device.destroy_buffer(self.index_buffer, None);
            device.free_memory(self.index_memory, None);
        }
    }
}

// ── Buffer collection ─────────────────────────────────────────────────────────

pub struct GpuBuffers {
    pub meshes: HashMap<u32, VkMesh>,
    /// Cached memory properties — queried once, used for every allocation.
    mem_props: vk::PhysicalDeviceMemoryProperties,
}

impl GpuBuffers {
    pub fn new(instance: &ash::Instance, physical_device: vk::PhysicalDevice) -> Self {
        let mem_props =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };
        Self { meshes: HashMap::new(), mem_props }
    }

    /// Upload tessellated geometry for `handle`, replacing any previous mesh.
    /// If `verts` is empty the old mesh (if any) is removed.
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

        let vertex_bytes = bytemuck::cast_slice(verts);
        let index_bytes = bytemuck::cast_slice(idxs);

        let (vertex_buffer, vertex_memory) = self.alloc_and_upload(
            device,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vertex_bytes,
        );
        let (index_buffer, index_memory) = self.alloc_and_upload(
            device,
            vk::BufferUsageFlags::INDEX_BUFFER,
            index_bytes,
        );

        self.meshes.insert(
            handle,
            VkMesh {
                vertex_buffer,
                vertex_memory,
                index_buffer,
                index_memory,
                index_count: idxs.len() as u32,
            },
        );
    }

    /// Free every mesh; call before destroying the Vulkan device.
    pub fn destroy_all(&mut self, device: &ash::Device) {
        for mesh in self.meshes.values() {
            unsafe { mesh.destroy(device) };
        }
        self.meshes.clear();
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn alloc_and_upload(
        &self,
        device: &ash::Device,
        usage: vk::BufferUsageFlags,
        data: &[u8],
    ) -> (vk::Buffer, vk::DeviceMemory) {
        let size = data.len() as vk::DeviceSize;

        let buf_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            device.create_buffer(&buf_info, None).expect("failed to create buffer")
        };

        let mem_reqs = unsafe { device.get_buffer_memory_requirements(buffer) };
        let mem_type = self
            .find_memory_type(
                mem_reqs.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )
            .expect("no HOST_VISIBLE | HOST_COHERENT memory type available");

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_reqs.size)
            .memory_type_index(mem_type);

        let memory = unsafe {
            device.allocate_memory(&alloc_info, None).expect("failed to allocate buffer memory")
        };

        unsafe {
            device.bind_buffer_memory(buffer, memory, 0).unwrap();
            let ptr = device
                .map_memory(memory, 0, size, vk::MemoryMapFlags::empty())
                .expect("failed to map buffer memory") as *mut u8;
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            device.unmap_memory(memory);
        }

        (buffer, memory)
    }

    fn find_memory_type(
        &self,
        type_filter: u32,
        required: vk::MemoryPropertyFlags,
    ) -> Option<u32> {
        (0..self.mem_props.memory_type_count).find(|&i| {
            (type_filter & (1 << i)) != 0
                && self.mem_props.memory_types[i as usize].property_flags.contains(required)
        })
    }
}
