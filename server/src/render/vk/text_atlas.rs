use std::collections::HashMap;

use ash::vk;

use crate::scene::stimulus::text::GlyphKey;

// ── Constants ─────────────────────────────────────────────────────────────────

pub const ATLAS_SIZE: u32 = 2048;
const ATLAS_BYTES: usize = (ATLAS_SIZE * ATLAS_SIZE) as usize; // R8_UNORM, 1 byte/texel

// ── AtlasEntry ────────────────────────────────────────────────────────────────

/// UV coordinates and pixel dimensions of one glyph in the atlas.
#[derive(Clone, Copy, Debug)]
pub struct AtlasEntry {
    /// Top-left UV (0..1 range).
    pub u0: f32,
    pub v0: f32,
    /// Bottom-right UV.
    pub u1: f32,
    pub v1: f32,
    /// Glyph bitmap dimensions in pixels.
    pub pixel_w: u32,
    pub pixel_h: u32,
}

// ── GlyphAtlas ────────────────────────────────────────────────────────────────

/// R8_UNORM glyph atlas backed by a 2048×2048 Vulkan image.
///
/// Glyphs are packed via a simple shelf algorithm: rows grow downward; within
/// each row (shelf), glyphs are placed left-to-right with 1-pixel padding
/// between them.
///
/// CPU mirror (`cpu_pixels`) is the authoritative pixel source; `flush()`
/// copies it to the GPU via a staging buffer when `dirty` is set.
pub struct GlyphAtlas {
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub descriptor_set: vk::DescriptorSet,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_pool: vk::DescriptorPool,
    image_memory: vk::DeviceMemory,
    staging_buffer: vk::Buffer,
    staging_memory: vk::DeviceMemory,
    /// CPU-side R8_UNORM pixels (ATLAS_SIZE × ATLAS_SIZE).
    cpu_pixels: Vec<u8>,
    /// Shelf packer state.
    shelf_x: u32,
    shelf_y: u32,
    shelf_h: u32,
    cache: HashMap<GlyphKey, AtlasEntry>,
    dirty: bool,
    /// False until the first `flush()`; determines the image's starting layout.
    initialized: bool,
}

impl GlyphAtlas {
    /// Allocate the atlas image, persistent staging buffer, sampler, and
    /// descriptor set.  The GPU image is in UNDEFINED layout until the first
    /// `flush()` call.
    pub fn new(
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
    ) -> Self {
        let mem_props =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };

        let image = create_atlas_image(device);
        let image_memory = alloc_bind_device_local(device, &mem_props, image);
        let view = create_atlas_view(device, image);
        let sampler = create_atlas_sampler(device);

        let (staging_buffer, staging_memory) =
            create_staging_buffer(device, &mem_props, ATLAS_BYTES as vk::DeviceSize);

        let (descriptor_pool, descriptor_set_layout, descriptor_set) =
            create_descriptor_set(device, sampler, view);

        let _ = mem_props; // used only during construction
        Self {
            image,
            view,
            sampler,
            descriptor_set,
            descriptor_set_layout,
            descriptor_pool,
            image_memory,
            staging_buffer,
            staging_memory,
            cpu_pixels: vec![0u8; ATLAS_BYTES],
            shelf_x: 0,
            shelf_y: 0,
            shelf_h: 0,
            cache: HashMap::new(),
            dirty: false,
            initialized: false,
        }
    }

    // ── Glyph insertion ───────────────────────────────────────────────────────

    /// Insert a glyph into the atlas (no-op if already present).
    ///
    /// Returns `None` when the atlas is full.
    pub fn insert(
        &mut self,
        key: GlyphKey,
        bitmap: &[u8],
        glyph_w: u32,
        glyph_h: u32,
    ) -> Option<AtlasEntry> {
        if let Some(&entry) = self.cache.get(&key) {
            return Some(entry);
        }

        const PAD: u32 = 1;

        // Try current shelf first, then open a new one.
        let (ax, ay) = if self.shelf_x + glyph_w <= ATLAS_SIZE {
            let x = self.shelf_x;
            let y = self.shelf_y;
            self.shelf_x += glyph_w + PAD;
            self.shelf_h = self.shelf_h.max(glyph_h);
            (x, y)
        } else {
            let new_y = self.shelf_y + self.shelf_h + PAD;
            if new_y + glyph_h > ATLAS_SIZE {
                log::warn!("GlyphAtlas full — glyph dropped");
                return None;
            }
            self.shelf_y = new_y;
            self.shelf_h = glyph_h;
            self.shelf_x = glyph_w + PAD;
            (0, new_y)
        };

        // Write rows into the CPU mirror.
        for row in 0..glyph_h {
            let src = (row * glyph_w) as usize;
            let dst = ((ay + row) * ATLAS_SIZE + ax) as usize;
            self.cpu_pixels[dst..dst + glyph_w as usize]
                .copy_from_slice(&bitmap[src..src + glyph_w as usize]);
        }
        self.dirty = true;

        let inv = ATLAS_SIZE as f32;
        let entry = AtlasEntry {
            u0: ax as f32 / inv,
            v0: ay as f32 / inv,
            u1: (ax + glyph_w) as f32 / inv,
            v1: (ay + glyph_h) as f32 / inv,
            pixel_w: glyph_w,
            pixel_h: glyph_h,
        };
        self.cache.insert(key, entry);
        Some(entry)
    }

    /// Look up a previously inserted glyph.
    pub fn lookup(&self, key: GlyphKey) -> Option<AtlasEntry> {
        self.cache.get(&key).copied()
    }

    // ── GPU upload ────────────────────────────────────────────────────────────

    /// Upload `cpu_pixels` to the GPU image when dirty.
    ///
    /// Uses a one-time command buffer submission so it can be called outside the
    /// main frame loop (e.g., during the tessellation phase before recording draw
    /// commands).  No-op when the atlas has not changed since the last call.
    pub fn flush(
        &mut self,
        device: &ash::Device,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
    ) {
        if !self.dirty {
            return;
        }

        let size = ATLAS_BYTES as vk::DeviceSize;
        unsafe {
            // Copy cpu_pixels → staging buffer.
            let ptr = device
                .map_memory(self.staging_memory, 0, size, vk::MemoryMapFlags::empty())
                .expect("GlyphAtlas: failed to map staging buffer") as *mut u8;
            std::ptr::copy_nonoverlapping(self.cpu_pixels.as_ptr(), ptr, ATLAS_BYTES);
            device.unmap_memory(self.staging_memory);

            let cb = begin_one_time_cb(device, command_pool);

            let subresource_range = vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            };

            // Layout transition → TRANSFER_DST_OPTIMAL.
            let (old_layout, src_stage, src_access) = if self.initialized {
                (
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::AccessFlags::SHADER_READ,
                )
            } else {
                (
                    vk::ImageLayout::UNDEFINED,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::AccessFlags::empty(),
                )
            };

            let to_transfer = vk::ImageMemoryBarrier::default()
                .old_layout(old_layout)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(self.image)
                .subresource_range(subresource_range)
                .src_access_mask(src_access)
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
            device.cmd_pipeline_barrier(
                cb,
                src_stage,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_transfer],
            );

            // Buffer → image copy (full atlas).
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
                    width: ATLAS_SIZE,
                    height: ATLAS_SIZE,
                    depth: 1,
                });
            device.cmd_copy_buffer_to_image(
                cb,
                self.staging_buffer,
                self.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );

            // Layout transition → SHADER_READ_ONLY_OPTIMAL.
            let to_shader = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(self.image)
                .subresource_range(subresource_range)
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ);
            device.cmd_pipeline_barrier(
                cb,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_shader],
            );

            end_and_submit_one_time_cb(device, command_pool, queue, cb);
        }

        self.dirty = false;
        self.initialized = true;
    }

    // ── Destroy ───────────────────────────────────────────────────────────────

    /// Free all GPU resources.  Call before dropping `VkContext`.
    pub unsafe fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            device.destroy_sampler(self.sampler, None);
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.image_memory, None);
            device.destroy_buffer(self.staging_buffer, None);
            device.free_memory(self.staging_memory, None);
        }
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn find_memory_type(
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    filter: u32,
    flags: vk::MemoryPropertyFlags,
) -> u32 {
    (0..mem_props.memory_type_count)
        .find(|&i| {
            (filter & (1 << i)) != 0
                && mem_props.memory_types[i as usize].property_flags.contains(flags)
        })
        .expect("GlyphAtlas: no suitable memory type")
}

fn create_atlas_image(device: &ash::Device) -> vk::Image {
    let info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(vk::Format::R8_UNORM)
        .extent(vk::Extent3D { width: ATLAS_SIZE, height: ATLAS_SIZE, depth: 1 })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    unsafe {
        device
            .create_image(&info, None)
            .expect("GlyphAtlas: failed to create image")
    }
}

fn alloc_bind_device_local(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    image: vk::Image,
) -> vk::DeviceMemory {
    let reqs = unsafe { device.get_image_memory_requirements(image) };
    let mem_type = find_memory_type(mem_props, reqs.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL);
    let mem = unsafe {
        device
            .allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(reqs.size)
                    .memory_type_index(mem_type),
                None,
            )
            .expect("GlyphAtlas: failed to allocate image memory")
    };
    unsafe {
        device
            .bind_image_memory(image, mem, 0)
            .expect("GlyphAtlas: bind_image_memory failed");
    }
    mem
}

fn create_atlas_view(device: &ash::Device, image: vk::Image) -> vk::ImageView {
    let info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(vk::Format::R8_UNORM)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });
    unsafe {
        device
            .create_image_view(&info, None)
            .expect("GlyphAtlas: failed to create image view")
    }
}

fn create_atlas_sampler(device: &ash::Device) -> vk::Sampler {
    let info = vk::SamplerCreateInfo::default()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .max_lod(0.0);
    unsafe {
        device
            .create_sampler(&info, None)
            .expect("GlyphAtlas: failed to create sampler")
    }
}

fn create_staging_buffer(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    size: vk::DeviceSize,
) -> (vk::Buffer, vk::DeviceMemory) {
    let buf_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(vk::BufferUsageFlags::TRANSFER_SRC)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buf = unsafe {
        device
            .create_buffer(&buf_info, None)
            .expect("GlyphAtlas: failed to create staging buffer")
    };
    let reqs = unsafe { device.get_buffer_memory_requirements(buf) };
    let mem_type = find_memory_type(
        mem_props,
        reqs.memory_type_bits,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    );
    let mem = unsafe {
        device
            .allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(reqs.size)
                    .memory_type_index(mem_type),
                None,
            )
            .expect("GlyphAtlas: failed to allocate staging memory")
    };
    unsafe {
        device
            .bind_buffer_memory(buf, mem, 0)
            .expect("GlyphAtlas: bind staging buffer failed");
    }
    (buf, mem)
}

/// Descriptor pool + layout + single set.
/// Binding 0: SAMPLER, binding 1: SAMPLED_IMAGE (matches the text shader layout).
fn create_descriptor_set(
    device: &ash::Device,
    sampler: vk::Sampler,
    view: vk::ImageView,
) -> (vk::DescriptorPool, vk::DescriptorSetLayout, vk::DescriptorSet) {
    let pool_sizes = [
        vk::DescriptorPoolSize { ty: vk::DescriptorType::SAMPLER, descriptor_count: 1 },
        vk::DescriptorPoolSize { ty: vk::DescriptorType::SAMPLED_IMAGE, descriptor_count: 1 },
    ];
    let pool = unsafe {
        device
            .create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(1)
                    .pool_sizes(&pool_sizes),
                None,
            )
            .expect("GlyphAtlas: failed to create descriptor pool")
    };

    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let layout = unsafe {
        device
            .create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )
            .expect("GlyphAtlas: failed to create descriptor set layout")
    };

    let layouts = [layout];
    let set = unsafe {
        device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(&layouts),
            )
            .expect("GlyphAtlas: failed to allocate descriptor set")[0]
    };

    // Write sampler + image view into the set.
    let sampler_info =
        vk::DescriptorImageInfo::default().sampler(sampler);
    let image_info = vk::DescriptorImageInfo::default()
        .image_view(view)
        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

    let writes = [
        vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::SAMPLER)
            .image_info(std::slice::from_ref(&sampler_info)),
        vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .image_info(std::slice::from_ref(&image_info)),
    ];
    unsafe { device.update_descriptor_sets(&writes, &[]) };

    (pool, layout, set)
}

fn begin_one_time_cb(device: &ash::Device, pool: vk::CommandPool) -> vk::CommandBuffer {
    let cb = unsafe {
        device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )
    }
    .expect("GlyphAtlas: failed to allocate command buffer")[0];
    unsafe {
        device
            .begin_command_buffer(
                cb,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )
            .expect("GlyphAtlas: begin_command_buffer failed");
    }
    cb
}

fn end_and_submit_one_time_cb(
    device: &ash::Device,
    pool: vk::CommandPool,
    queue: vk::Queue,
    cb: vk::CommandBuffer,
) {
    unsafe {
        device.end_command_buffer(cb).expect("GlyphAtlas: end_command_buffer failed");
        let cbs = [cb];
        device
            .queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&cbs)], vk::Fence::null())
            .expect("GlyphAtlas: queue_submit failed");
        device.queue_wait_idle(queue).expect("GlyphAtlas: queue_wait_idle failed");
        device.free_command_buffers(pool, &cbs);
    }
}
