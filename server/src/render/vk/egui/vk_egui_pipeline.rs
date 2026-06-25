use ash::vk;

/// Vulkan pipeline for egui overlay rendering
pub struct VkEguiPipeline {
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub sampler: vk::Sampler,
}

impl VkEguiPipeline {
    pub fn new(device: &ash::Device, render_pass: vk::RenderPass) -> Self {
        unsafe {
            // ── Descriptor set layout: one combined image sampler ───────────
            let sampler_binding = vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT);

            let texture_binding = vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT);

            let bindings = [sampler_binding, texture_binding];
            let descriptor_set_layout_info =
                vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);

            let descriptor_set_layout = device
                .create_descriptor_set_layout(&descriptor_set_layout_info, None)
                .expect("failed to create egui descriptor set layout");

            // ── Pipeline layout: push constant (screen_size: vec2<f32>) ─────
            let push_constant_range = vk::PushConstantRange::default()
                .stage_flags(vk::ShaderStageFlags::VERTEX)
                .offset(0)
                .size(8); // vec2<f32> = 8 bytes

            let set_layouts = [descriptor_set_layout];
            let layout_info = vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&set_layouts)
                .push_constant_ranges(std::slice::from_ref(&push_constant_range));

            let layout = device
                .create_pipeline_layout(&layout_info, None)
                .expect("failed to create egui pipeline layout");

            // ── Sampler: linear filter, clamp to edge ───────────────────────
            let sampler_info = vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .anisotropy_enable(false)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .min_lod(0.0)
                .max_lod(0.0);

            let sampler = device
                .create_sampler(&sampler_info, None)
                .expect("failed to create egui sampler");

            // ── Shader modules ───────────────────────────────────────────────
            let spv_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/egui.spv"));
            let spv_u32: Vec<u32> = spv_bytes
                .chunks_exact(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();

            let shader_info = vk::ShaderModuleCreateInfo::default().code(&spv_u32);
            let shader_module = device
                .create_shader_module(&shader_info, None)
                .expect("failed to create egui shader module");

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

            // ── Vertex input: pos, uv, color ────────────────────────────────
            // egui's epaint::Vertex layout:
            //   pos: [f32; 2]   offset 0
            //   uv: [f32; 2]    offset 8
            //   color: [u8; 4]  offset 16 → normalized to [f32; 4]
            let binding = vk::VertexInputBindingDescription::default()
                .binding(0)
                .stride(20) // 2*f32 + 2*f32 + 4*u8 = 8 + 8 + 4 = 20
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
                vk::VertexInputAttributeDescription::default()
                    .location(2)
                    .binding(0)
                    .format(vk::Format::R8G8B8A8_UNORM) // u8 → f32 (0..255 → 0..1)
                    .offset(16),
            ];

            let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
                .vertex_binding_descriptions(std::slice::from_ref(&binding))
                .vertex_attribute_descriptions(&attributes);

            let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

            // ── Dynamic state: viewport and scissor ──────────────────────────
            let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
            let dynamic_state =
                vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

            let viewport_state = vk::PipelineViewportStateCreateInfo::default()
                .viewport_count(1)
                .scissor_count(1);

            // ── Rasterization: no culling ────────────────────────────────────
            let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
                .polygon_mode(vk::PolygonMode::FILL)
                .cull_mode(vk::CullModeFlags::NONE)
                .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                .line_width(1.0);

            let multisample = vk::PipelineMultisampleStateCreateInfo::default()
                .rasterization_samples(vk::SampleCountFlags::TYPE_1);

            // ── Blending: premultiplied alpha ────────────────────────────────
            // Blend equation: (ONE * src) + (ONE_MINUS_SRC_ALPHA * dst)
            let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
                .blend_enable(true)
                .src_color_blend_factor(vk::BlendFactor::ONE)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .alpha_blend_op(vk::BlendOp::ADD)
                .color_write_mask(vk::ColorComponentFlags::RGBA);

            let blend_state = vk::PipelineColorBlendStateCreateInfo::default()
                .attachments(std::slice::from_ref(&blend_attachment));

            // ── Create pipeline ──────────────────────────────────────────────
            let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
                .stages(&shader_stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterizer)
                .multisample_state(&multisample)
                .color_blend_state(&blend_state)
                .dynamic_state(&dynamic_state)
                .layout(layout)
                .render_pass(render_pass)
                .subpass(0);

            let pipeline = device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
                .expect("failed to create egui graphics pipeline")[0];

            device.destroy_shader_module(shader_module, None);

            Self {
                pipeline,
                layout,
                descriptor_set_layout,
                sampler,
            }
        }
    }

    pub fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            device.destroy_sampler(self.sampler, None);
        }
    }
}
