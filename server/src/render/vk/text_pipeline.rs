use std::collections::HashMap;

use ash::vk;

use super::buffers::{VkMesh, alloc_upload_bytes};

// ── Vertex type ───────────────────────────────────────────────────────────────

/// One corner of a text glyph quad.
///
/// `position` is in NDC ([-1,1]×[-1,1], Y-up).
/// `uv` is the atlas texture coordinate ([0,1]×[0,1], Y-down).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TextVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}

// ── Push constants ────────────────────────────────────────────────────────────

/// Must match `PushConstants` in `shaders/text.wgsl`.
///
/// Layout (16 bytes):
///   offset  0: color  [f32; 4]  rgba tint
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TextPushConstants {
    pub color: [f32; 4],
}

// ── Pipeline ──────────────────────────────────────────────────────────────────

pub struct VkTextPipeline {
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
}

impl VkTextPipeline {
    /// Create the text pipeline.
    ///
    /// `atlas_dsl` is borrowed from `GlyphAtlas::descriptor_set_layout` — the
    /// atlas owns the layout object; this pipeline only references it during
    /// pipeline-layout construction and does not free it.
    pub fn new(
        device: &ash::Device,
        render_pass: vk::RenderPass,
        atlas_dsl: vk::DescriptorSetLayout,
    ) -> Self {
        let spv_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/text.spv"));
        let spv_u32: Vec<u32> = spv_bytes
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        let shader_module = unsafe {
            device
                .create_shader_module(
                    &vk::ShaderModuleCreateInfo::default().code(&spv_u32),
                    None,
                )
                .expect("text: shader module")
        };

        let entry_vs = c"vs_main";
        let entry_fs = c"fs_main";
        let shader_stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(shader_module)
                .name(entry_vs),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(shader_module)
                .name(entry_fs),
        ];

        let binding = vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(std::mem::size_of::<TextVertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX);
        let attributes = [
            vk::VertexInputAttributeDescription::default()
                .location(0)
                .binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(0),
            vk::VertexInputAttributeDescription::default()
                .location(1)
                .binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(8),
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(std::slice::from_ref(&binding))
            .vertex_attribute_descriptions(&attributes);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let rasteriser = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(vk::ColorComponentFlags::RGBA);
        let blend_state = vk::PipelineColorBlendStateCreateInfo::default()
            .attachments(std::slice::from_ref(&blend_attachment));

        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(std::mem::size_of::<TextPushConstants>() as u32);
        let set_layouts = [atlas_dsl];
        let layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&set_layouts)
            .push_constant_ranges(std::slice::from_ref(&push_range));
        let layout = unsafe {
            device
                .create_pipeline_layout(&layout_info, None)
                .expect("text: pipeline layout")
        };

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasteriser)
            .multisample_state(&multisample)
            .color_blend_state(&blend_state)
            .dynamic_state(&dynamic_state)
            .layout(layout)
            .render_pass(render_pass)
            .subpass(0);
        let pipeline = unsafe {
            device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
                .expect("text: graphics pipeline")[0]
        };

        unsafe { device.destroy_shader_module(shader_module, None) };
        Self { pipeline, layout }
    }

    pub fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

// ── TextMeshCache ─────────────────────────────────────────────────────────────

/// Per-stimulus `TextVertex` quad buffers, keyed by stimulus handle.
/// Rebuilt when the stimulus is dirty; reused across frames otherwise.
pub struct TextMeshCache {
    pub meshes: HashMap<u32, VkMesh>,
    mem_props: vk::PhysicalDeviceMemoryProperties,
}

impl TextMeshCache {
    pub fn new(instance: &ash::Instance, physical_device: vk::PhysicalDevice) -> Self {
        let mem_props =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };
        Self { meshes: HashMap::new(), mem_props }
    }

    /// Upload glyph quad vertices for `handle`, replacing any existing buffers.
    /// `vert_bytes` is a `bytemuck::cast_slice` of `&[TextVertex]`.
    pub fn upload(&mut self, handle: u32, device: &ash::Device, vert_bytes: &[u8], idxs: &[u32]) {
        if let Some(old) = self.meshes.remove(&handle) {
            unsafe { old.destroy(device) };
        }
        if vert_bytes.is_empty() || idxs.is_empty() {
            return;
        }
        let (vb, vm) = alloc_upload_bytes(
            &self.mem_props, device, vk::BufferUsageFlags::VERTEX_BUFFER, vert_bytes,
        );
        let (ib, im) = alloc_upload_bytes(
            &self.mem_props, device, vk::BufferUsageFlags::INDEX_BUFFER,
            bytemuck::cast_slice(idxs),
        );
        self.meshes.insert(handle, VkMesh::from_raw(vb, vm, ib, im, idxs.len() as u32));
    }

    pub fn destroy_all(&mut self, device: &ash::Device) {
        for mesh in self.meshes.values() {
            unsafe { mesh.destroy(device) };
        }
        self.meshes.clear();
    }
}
