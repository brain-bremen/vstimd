use ash::vk;

use crate::render::Vertex;

// ── Push-constant layout for the grating pipeline ────────────────────────────

/// Must match the `PushConstants` struct in `shaders/grating.wgsl` (std430).
///
/// Layout (80 bytes):
///   offset  0: screen_half  [f32; 2]
///   offset  8: center_px    [f32; 2]
///   offset 16: half_size    [f32; 2]
///   offset 24: sf           f32
///   offset 28: phase        f32
///   offset 32: ori_rad      f32
///   offset 36: contrast     f32
///   offset 40: _pad_color   [u32; 2]  ← 8-byte gap: vec4 requires 16-byte alignment
///   offset 48: color        [f32; 4]
///   offset 64: waveform     u32
///   offset 68: mask_type    u32
///   offset 72: mask_param   f32   (SD for gauss; fringe width for raisedCos; 0 = use default)
///   offset 76: _pad         u32
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GratingPushConstants {
    pub screen_half : [f32; 2],
    pub center_px   : [f32; 2],
    pub half_size   : [f32; 2],
    pub sf          : f32,
    pub phase       : f32,
    pub ori_rad     : f32,
    pub contrast    : f32,
    pub _pad_color  : [u32; 2],
    pub color       : [f32; 4],
    pub waveform    : u32,
    pub mask_type   : u32,
    pub mask_param  : f32,
    pub _pad        : u32,
}

pub struct VkPipeline {
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
}

impl VkPipeline {
    pub fn new(device: &ash::Device, render_pass: vk::RenderPass, polygon_mode: vk::PolygonMode) -> Self {
        let spv_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/solid.spv"));
        let spv_u32: Vec<u32> = spv_bytes
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        let shader_info = vk::ShaderModuleCreateInfo::default().code(&spv_u32);
        let shader_module = unsafe {
            device.create_shader_module(&shader_info, None).expect("failed to create shader module")
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
            .stride(std::mem::size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX);
        let attributes = [
            vk::VertexInputAttributeDescription::default()
                .location(0)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(0),
            vk::VertexInputAttributeDescription::default()
                .location(1)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(12),
            vk::VertexInputAttributeDescription::default()
                .location(2)
                .binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(24),
            vk::VertexInputAttributeDescription::default()
                .location(3)
                .binding(0)
                .format(vk::Format::R32G32B32A32_SFLOAT)
                .offset(32),
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
            .polygon_mode(polygon_mode)
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

        let layout_info = vk::PipelineLayoutCreateInfo::default();
        let layout = unsafe {
            device.create_pipeline_layout(&layout_info, None).expect("failed to create layout")
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
                .expect("failed to create graphics pipeline")[0]
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

// ── Grating pipeline ──────────────────────────────────────────────────────────

pub struct VkGratingPipeline {
    pub pipeline : vk::Pipeline,
    pub layout   : vk::PipelineLayout,
}

impl VkGratingPipeline {
    pub fn new(device: &ash::Device, render_pass: vk::RenderPass, polygon_mode: vk::PolygonMode) -> Self {
        let spv_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/grating.spv"));
        let spv_u32: Vec<u32> = spv_bytes
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        let shader_info = vk::ShaderModuleCreateInfo::default().code(&spv_u32);
        let shader_module = unsafe {
            device.create_shader_module(&shader_info, None).expect("grating: shader module")
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

        // Vertex input — same layout as the solid pipeline.
        let binding = vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(std::mem::size_of::<Vertex>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX);
        let attributes = [
            vk::VertexInputAttributeDescription::default()
                .location(0).binding(0).format(vk::Format::R32G32B32_SFLOAT).offset(0),
            vk::VertexInputAttributeDescription::default()
                .location(1).binding(0).format(vk::Format::R32G32B32_SFLOAT).offset(12),
            vk::VertexInputAttributeDescription::default()
                .location(2).binding(0).format(vk::Format::R32G32_SFLOAT).offset(24),
            vk::VertexInputAttributeDescription::default()
                .location(3).binding(0).format(vk::Format::R32G32B32A32_SFLOAT).offset(32),
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
            .polygon_mode(polygon_mode)
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

        // Push constant range covers the full GratingPushConstants struct.
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(std::mem::size_of::<GratingPushConstants>() as u32);
        let layout_info = vk::PipelineLayoutCreateInfo::default()
            .push_constant_ranges(std::slice::from_ref(&push_range));
        let layout = unsafe {
            device.create_pipeline_layout(&layout_info, None).expect("grating: pipeline layout")
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
                .expect("grating: graphics pipeline")[0]
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
