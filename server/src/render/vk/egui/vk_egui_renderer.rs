use std::collections::HashMap;

use ash::vk;

use super::vk_egui_pipeline::VkEguiPipeline;

/// Vulkan texture for egui (font atlas or user image)
struct VkEguiTexture {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
}

impl VkEguiTexture {
    unsafe fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
    }
}

/// Dynamic vertex/index buffers for egui mesh data
struct VkEguiMeshBuffers {
    vertex_buffer: vk::Buffer,
    vertex_memory: vk::DeviceMemory,
    vertex_capacity: usize,
    index_buffer: vk::Buffer,
    index_memory: vk::DeviceMemory,
    index_capacity: usize,
}

impl VkEguiMeshBuffers {
    fn new() -> Self {
        Self {
            vertex_buffer: vk::Buffer::null(),
            vertex_memory: vk::DeviceMemory::null(),
            vertex_capacity: 0,
            index_buffer: vk::Buffer::null(),
            index_memory: vk::DeviceMemory::null(),
            index_capacity: 0,
        }
    }

    unsafe fn destroy(&self, device: &ash::Device) {
        unsafe {
            if self.vertex_buffer != vk::Buffer::null() {
                device.destroy_buffer(self.vertex_buffer, None);
                device.free_memory(self.vertex_memory, None);
            }
            if self.index_buffer != vk::Buffer::null() {
                device.destroy_buffer(self.index_buffer, None);
                device.free_memory(self.index_memory, None);
            }
        }
    }
}

/// egui renderer for Vulkan
pub struct VkEguiRenderer {
    pipeline: VkEguiPipeline,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: HashMap<egui::TextureId, vk::DescriptorSet>,
    textures: HashMap<egui::TextureId, VkEguiTexture>,
    mesh_buffers: VkEguiMeshBuffers,
    mem_props: vk::PhysicalDeviceMemoryProperties,
}

impl VkEguiRenderer {
    pub fn new(
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        render_pass: vk::RenderPass,
    ) -> Self {
        let pipeline = VkEguiPipeline::new(device, render_pass);
        let mem_props = unsafe { instance.get_physical_device_memory_properties(physical_device) };

        // Create descriptor pool for texture bindings
        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLER,
                descriptor_count: 64,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLED_IMAGE,
                descriptor_count: 64,
            },
        ];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(64)
            .pool_sizes(&pool_sizes);
        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .expect("failed to create egui descriptor pool")
        };

        Self {
            pipeline,
            descriptor_pool,
            descriptor_sets: HashMap::new(),
            textures: HashMap::new(),
            mesh_buffers: VkEguiMeshBuffers::new(),
            mem_props,
        }
    }

    /// Process texture deltas from egui (allocate/free/update textures)
    pub fn update_textures(
        &mut self,
        device: &ash::Device,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        textures_delta: &egui::TexturesDelta,
    ) {
        for (id, image_delta) in &textures_delta.set {
            let (width, height, pixels) = Self::image_to_rgba(&image_delta.image);

            if let Some([x, y]) = image_delta.pos {
                // Partial update: patch a sub-region of the existing texture
                if let Some(tex) = self.textures.get(id) {
                    let image = tex.image;
                    self.update_texture_region(
                        device,
                        queue,
                        command_pool,
                        image,
                        x as i32,
                        y as i32,
                        width,
                        height,
                        &pixels,
                    );
                }
            } else {
                // Full replace: destroy old texture and allocate a new one
                if let Some(old) = self.textures.remove(id) {
                    unsafe { old.destroy(device) };
                    self.descriptor_sets.remove(id);
                }
                let texture =
                    self.create_texture(device, queue, command_pool, width, height, &pixels);
                self.textures.insert(*id, texture);
            }
        }

        for id in &textures_delta.free {
            if let Some(tex) = self.textures.remove(id) {
                unsafe { tex.destroy(device) };
                self.descriptor_sets.remove(id);
            }
        }
    }

    fn image_to_rgba(image: &egui::ImageData) -> (u32, u32, Vec<u8>) {
        match image {
            egui::ImageData::Color(color_img) => {
                let width = color_img.width() as u32;
                let height = color_img.height() as u32;
                let pixels: Vec<u8> = color_img.pixels.iter().flat_map(|c| c.to_array()).collect();
                (width, height, pixels)
            }
        }
    }

    /// Upload mesh data for the current frame
    pub fn upload_meshes(
        &mut self,
        device: &ash::Device,
        primitives: &[egui::ClippedPrimitive],
        _pixels_per_point: f32,
    ) {
        // Count total vertices and indices
        let mut total_vertices = 0;
        let mut total_indices = 0;
        for prim in primitives {
            if let egui::epaint::Primitive::Mesh(mesh) = &prim.primitive {
                total_vertices += mesh.vertices.len();
                total_indices += mesh.indices.len();
            }
        }

        if total_vertices == 0 {
            return;
        }

        // Grow buffers if needed
        self.ensure_vertex_buffer_capacity(device, total_vertices);
        self.ensure_index_buffer_capacity(device, total_indices);

        // Upload vertex data
        unsafe {
            let ptr = device
                .map_memory(
                    self.mesh_buffers.vertex_memory,
                    0,
                    (total_vertices * std::mem::size_of::<egui::epaint::Vertex>())
                        as vk::DeviceSize,
                    vk::MemoryMapFlags::empty(),
                )
                .expect("failed to map egui vertex buffer");

            let mut offset = 0;
            for prim in primitives {
                if let egui::epaint::Primitive::Mesh(mesh) = &prim.primitive {
                    let size = mesh.vertices.len() * std::mem::size_of::<egui::epaint::Vertex>();
                    std::ptr::copy_nonoverlapping(
                        mesh.vertices.as_ptr() as *const u8,
                        (ptr as *mut u8).add(offset),
                        size,
                    );
                    offset += size;
                }
            }
            device.unmap_memory(self.mesh_buffers.vertex_memory);
        }

        // Upload index data
        unsafe {
            let ptr = device
                .map_memory(
                    self.mesh_buffers.index_memory,
                    0,
                    (total_indices * std::mem::size_of::<u32>()) as vk::DeviceSize,
                    vk::MemoryMapFlags::empty(),
                )
                .expect("failed to map egui index buffer");

            let mut offset = 0;
            for prim in primitives {
                if let egui::epaint::Primitive::Mesh(mesh) = &prim.primitive {
                    let size = mesh.indices.len() * std::mem::size_of::<u32>();
                    std::ptr::copy_nonoverlapping(
                        mesh.indices.as_ptr() as *const u8,
                        (ptr as *mut u8).add(offset),
                        size,
                    );
                    offset += size;
                }
            }
            device.unmap_memory(self.mesh_buffers.index_memory);
        }
    }

    /// Record draw calls into an existing command buffer
    pub fn paint(
        &mut self,
        device: &ash::Device,
        command_buffer: vk::CommandBuffer,
        primitives: &[egui::ClippedPrimitive],
        screen_size_pixels: (u32, u32),
        pixels_per_point: f32,
    ) {
        if primitives.is_empty() {
            return;
        }

        unsafe {
            device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline.pipeline,
            );

            // Push constants: screen size
            let screen_size = [screen_size_pixels.0 as f32, screen_size_pixels.1 as f32];
            device.cmd_push_constants(
                command_buffer,
                self.pipeline.layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                bytemuck::cast_slice(&screen_size),
            );

            // Bind vertex and index buffers
            device.cmd_bind_vertex_buffers(
                command_buffer,
                0,
                &[self.mesh_buffers.vertex_buffer],
                &[0],
            );
            device.cmd_bind_index_buffer(
                command_buffer,
                self.mesh_buffers.index_buffer,
                0,
                vk::IndexType::UINT32,
            );

            let mut vertex_offset = 0;
            let mut index_offset = 0;

            for prim in primitives {
                let egui::epaint::Primitive::Mesh(mesh) = &prim.primitive else {
                    continue; // Skip callbacks
                };

                if mesh.vertices.is_empty() {
                    continue;
                }

                // Set scissor rect
                let clip_min_x = (prim.clip_rect.min.x * pixels_per_point).max(0.0);
                let clip_min_y = (prim.clip_rect.min.y * pixels_per_point).max(0.0);
                let clip_max_x =
                    (prim.clip_rect.max.x * pixels_per_point).min(screen_size_pixels.0 as f32);
                let clip_max_y =
                    (prim.clip_rect.max.y * pixels_per_point).min(screen_size_pixels.1 as f32);

                let scissor = vk::Rect2D {
                    offset: vk::Offset2D {
                        x: clip_min_x as i32,
                        y: clip_min_y as i32,
                    },
                    extent: vk::Extent2D {
                        width: ((clip_max_x - clip_min_x).max(0.0)) as u32,
                        height: ((clip_max_y - clip_min_y).max(0.0)) as u32,
                    },
                };
                device.cmd_set_scissor(command_buffer, 0, &[scissor]);

                // Bind texture descriptor set
                let descriptor_set = self.get_or_create_descriptor_set(device, mesh.texture_id);
                device.cmd_bind_descriptor_sets(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.pipeline.layout,
                    0,
                    &[descriptor_set],
                    &[],
                );

                // Draw indexed
                device.cmd_draw_indexed(
                    command_buffer,
                    mesh.indices.len() as u32,
                    1,
                    index_offset,
                    vertex_offset as i32,
                    0,
                );

                vertex_offset += mesh.vertices.len();
                index_offset += mesh.indices.len() as u32;
            }
        }
    }

    pub fn destroy(&mut self, device: &ash::Device) {
        unsafe {
            self.mesh_buffers.destroy(device);
            for tex in self.textures.values() {
                tex.destroy(device);
            }
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.pipeline.destroy(device);
        }
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    fn create_texture(
        &self,
        device: &ash::Device,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> VkEguiTexture {
        unsafe {
            // Create image
            let image_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);

            let image = device
                .create_image(&image_info, None)
                .expect("failed to create egui texture image");

            let mem_reqs = device.get_image_memory_requirements(image);
            let mem_type = self
                .find_memory_type(
                    mem_reqs.memory_type_bits,
                    vk::MemoryPropertyFlags::DEVICE_LOCAL,
                )
                .expect("no device-local memory for egui texture");

            let alloc_info = vk::MemoryAllocateInfo::default()
                .allocation_size(mem_reqs.size)
                .memory_type_index(mem_type);
            let memory = device
                .allocate_memory(&alloc_info, None)
                .expect("failed to allocate egui texture memory");

            device
                .bind_image_memory(image, memory, 0)
                .expect("failed to bind egui texture memory");

            // Create image view
            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let view = device
                .create_image_view(&view_info, None)
                .expect("failed to create egui texture view");

            // Upload via staging buffer
            self.upload_texture_data(device, queue, command_pool, image, width, height, pixels);

            VkEguiTexture {
                image,
                memory,
                view,
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn upload_texture_data(
        &self,
        device: &ash::Device,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        image: vk::Image,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) {
        let size = pixels.len() as vk::DeviceSize;

        unsafe {
            // Create staging buffer
            let buffer_info = vk::BufferCreateInfo::default()
                .size(size)
                .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);
            let staging_buffer = device
                .create_buffer(&buffer_info, None)
                .expect("failed to create staging buffer");

            let mem_reqs = device.get_buffer_memory_requirements(staging_buffer);
            let mem_type = self
                .find_memory_type(
                    mem_reqs.memory_type_bits,
                    vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
                )
                .expect("no host-visible memory");

            let alloc_info = vk::MemoryAllocateInfo::default()
                .allocation_size(mem_reqs.size)
                .memory_type_index(mem_type);
            let staging_memory = device
                .allocate_memory(&alloc_info, None)
                .expect("failed to allocate staging memory");

            device
                .bind_buffer_memory(staging_buffer, staging_memory, 0)
                .unwrap();

            let ptr = device
                .map_memory(staging_memory, 0, size, vk::MemoryMapFlags::empty())
                .expect("failed to map staging buffer");
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), ptr as *mut u8, pixels.len());
            device.unmap_memory(staging_memory);

            // Copy staging buffer to image
            let cmd_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            let cmd_buffer = device.allocate_command_buffers(&cmd_info).unwrap()[0];

            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            device
                .begin_command_buffer(cmd_buffer, &begin_info)
                .unwrap();

            // Transition image to TRANSFER_DST_OPTIMAL
            let barrier = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);

            device.cmd_pipeline_barrier(
                cmd_buffer,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );

            // Copy buffer to image
            let region = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                });

            device.cmd_copy_buffer_to_image(
                cmd_buffer,
                staging_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );

            // Transition image to SHADER_READ_ONLY_OPTIMAL
            let barrier = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ);

            device.cmd_pipeline_barrier(
                cmd_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );

            device.end_command_buffer(cmd_buffer).unwrap();

            // Submit and wait
            let cmd_buffers = [cmd_buffer];
            let submit_info = vk::SubmitInfo::default().command_buffers(&cmd_buffers);
            device
                .queue_submit(queue, &[submit_info], vk::Fence::null())
                .unwrap();
            device.queue_wait_idle(queue).unwrap();

            // Cleanup
            device.free_command_buffers(command_pool, &[cmd_buffer]);
            device.destroy_buffer(staging_buffer, None);
            device.free_memory(staging_memory, None);
        }
    }

    /// Upload pixel data into a sub-region of an existing texture that is
    /// currently in `SHADER_READ_ONLY_OPTIMAL` layout.
    #[allow(clippy::too_many_arguments)]
    fn update_texture_region(
        &self,
        device: &ash::Device,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        image: vk::Image,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) {
        let size = pixels.len() as vk::DeviceSize;

        unsafe {
            let buffer_info = vk::BufferCreateInfo::default()
                .size(size)
                .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);
            let staging_buffer = device
                .create_buffer(&buffer_info, None)
                .expect("failed to create staging buffer for partial update");

            let mem_reqs = device.get_buffer_memory_requirements(staging_buffer);
            let mem_type = self
                .find_memory_type(
                    mem_reqs.memory_type_bits,
                    vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
                )
                .expect("no host-visible memory");
            let alloc_info = vk::MemoryAllocateInfo::default()
                .allocation_size(mem_reqs.size)
                .memory_type_index(mem_type);
            let staging_memory = device
                .allocate_memory(&alloc_info, None)
                .expect("failed to allocate staging memory for partial update");
            device
                .bind_buffer_memory(staging_buffer, staging_memory, 0)
                .unwrap();

            let ptr = device
                .map_memory(staging_memory, 0, size, vk::MemoryMapFlags::empty())
                .expect("failed to map staging buffer");
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), ptr as *mut u8, pixels.len());
            device.unmap_memory(staging_memory);

            let cmd_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            let cmd_buffer = device.allocate_command_buffers(&cmd_info).unwrap()[0];
            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            device
                .begin_command_buffer(cmd_buffer, &begin_info)
                .unwrap();

            let subresource = vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            };

            // SHADER_READ_ONLY_OPTIMAL → TRANSFER_DST_OPTIMAL
            let to_transfer = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(subresource)
                .src_access_mask(vk::AccessFlags::SHADER_READ)
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
            device.cmd_pipeline_barrier(
                cmd_buffer,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_transfer],
            );

            let region = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D { x, y, z: 0 })
                .image_extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                });
            device.cmd_copy_buffer_to_image(
                cmd_buffer,
                staging_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );

            // TRANSFER_DST_OPTIMAL → SHADER_READ_ONLY_OPTIMAL
            let to_shader = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(subresource)
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ);
            device.cmd_pipeline_barrier(
                cmd_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_shader],
            );

            device.end_command_buffer(cmd_buffer).unwrap();
            let cmd_buffers = [cmd_buffer];
            let submit_info = vk::SubmitInfo::default().command_buffers(&cmd_buffers);
            device
                .queue_submit(queue, &[submit_info], vk::Fence::null())
                .unwrap();
            device.queue_wait_idle(queue).unwrap();

            device.free_command_buffers(command_pool, &[cmd_buffer]);
            device.destroy_buffer(staging_buffer, None);
            device.free_memory(staging_memory, None);
        }
    }

    fn get_or_create_descriptor_set(
        &mut self,
        device: &ash::Device,
        texture_id: egui::TextureId,
    ) -> vk::DescriptorSet {
        if let Some(&set) = self.descriptor_sets.get(&texture_id) {
            return set;
        }

        let texture = self
            .textures
            .get(&texture_id)
            .expect("egui texture not found");

        unsafe {
            let set_layouts = [self.pipeline.descriptor_set_layout];
            let alloc_info = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(self.descriptor_pool)
                .set_layouts(&set_layouts);

            let descriptor_set = device
                .allocate_descriptor_sets(&alloc_info)
                .expect("failed to allocate egui descriptor set")[0];

            // Write sampler binding
            let sampler_info = vk::DescriptorImageInfo::default().sampler(self.pipeline.sampler);
            let sampler_write = vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(std::slice::from_ref(&sampler_info));

            // Write image binding
            let image_info = vk::DescriptorImageInfo::default()
                .image_view(texture.view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
            let image_write = vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(std::slice::from_ref(&image_info));

            device.update_descriptor_sets(&[sampler_write, image_write], &[]);

            self.descriptor_sets.insert(texture_id, descriptor_set);
            descriptor_set
        }
    }

    fn ensure_vertex_buffer_capacity(&mut self, device: &ash::Device, required: usize) {
        if required <= self.mesh_buffers.vertex_capacity {
            return;
        }

        let new_capacity = required
            .max(self.mesh_buffers.vertex_capacity * 2)
            .max(1024);

        unsafe {
            if self.mesh_buffers.vertex_buffer != vk::Buffer::null() {
                device.destroy_buffer(self.mesh_buffers.vertex_buffer, None);
                device.free_memory(self.mesh_buffers.vertex_memory, None);
            }

            let size =
                (new_capacity * std::mem::size_of::<egui::epaint::Vertex>()) as vk::DeviceSize;
            let (buffer, memory) = self.create_buffer(
                device,
                size,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            );

            self.mesh_buffers.vertex_buffer = buffer;
            self.mesh_buffers.vertex_memory = memory;
            self.mesh_buffers.vertex_capacity = new_capacity;
        }
    }

    fn ensure_index_buffer_capacity(&mut self, device: &ash::Device, required: usize) {
        if required <= self.mesh_buffers.index_capacity {
            return;
        }

        let new_capacity = required.max(self.mesh_buffers.index_capacity * 2).max(1024);

        unsafe {
            if self.mesh_buffers.index_buffer != vk::Buffer::null() {
                device.destroy_buffer(self.mesh_buffers.index_buffer, None);
                device.free_memory(self.mesh_buffers.index_memory, None);
            }

            let size = (new_capacity * std::mem::size_of::<u32>()) as vk::DeviceSize;
            let (buffer, memory) = self.create_buffer(
                device,
                size,
                vk::BufferUsageFlags::INDEX_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            );

            self.mesh_buffers.index_buffer = buffer;
            self.mesh_buffers.index_memory = memory;
            self.mesh_buffers.index_capacity = new_capacity;
        }
    }

    fn create_buffer(
        &self,
        device: &ash::Device,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        properties: vk::MemoryPropertyFlags,
    ) -> (vk::Buffer, vk::DeviceMemory) {
        unsafe {
            let buffer_info = vk::BufferCreateInfo::default()
                .size(size)
                .usage(usage)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let buffer = device
                .create_buffer(&buffer_info, None)
                .expect("failed to create buffer");

            let mem_reqs = device.get_buffer_memory_requirements(buffer);
            let mem_type = self
                .find_memory_type(mem_reqs.memory_type_bits, properties)
                .expect("no suitable memory type");

            let alloc_info = vk::MemoryAllocateInfo::default()
                .allocation_size(mem_reqs.size)
                .memory_type_index(mem_type);

            let memory = device
                .allocate_memory(&alloc_info, None)
                .expect("failed to allocate buffer memory");

            device.bind_buffer_memory(buffer, memory, 0).unwrap();

            (buffer, memory)
        }
    }

    fn find_memory_type(&self, filter: u32, properties: vk::MemoryPropertyFlags) -> Option<u32> {
        (0..self.mem_props.memory_type_count).find(|&i| {
            (filter & (1 << i)) != 0
                && self.mem_props.memory_types[i as usize]
                    .property_flags
                    .contains(properties)
        })
    }
}
