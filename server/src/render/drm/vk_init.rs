use ash::vk;

const FRAMES_IN_FLIGHT: usize = 2;

// ── Frame sync primitives ─────────────────────────────────────────────────────

pub struct FrameSync {
    pub image_available: vk::Semaphore,
    pub render_done: vk::Semaphore,
    pub in_flight: vk::Fence,
    pub command_buffer: vk::CommandBuffer,
}

// ── VkContext ─────────────────────────────────────────────────────────────────

/// All long-lived Vulkan handles for the bare-metal display path.
///
/// Fields are declared in drop order (first declared → first dropped in Rust).
/// The explicit `Drop` impl destroys them in the correct reverse-initialisation
/// order, so the field ordering here is just documentation.
pub struct VkContext {
    pub frames: Vec<FrameSync>,
    pub framebuffers: Vec<vk::Framebuffer>,
    pub render_pass: vk::RenderPass,
    pub swapchain_image_views: Vec<vk::ImageView>,
    pub swapchain_images: Vec<vk::Image>,
    pub command_pool: vk::CommandPool,
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_loader: ash::khr::swapchain::Device,
    pub graphics_queue: vk::Queue,
    pub graphics_queue_family: u32,
    pub device: ash::Device,
    surface_loader: ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    pub physical_device: vk::PhysicalDevice,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    pub instance: ash::Instance,
    pub entry: ash::Entry,
}

impl Drop for VkContext {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().ok();

            for frame in &self.frames {
                self.device.destroy_semaphore(frame.image_available, None);
                self.device.destroy_semaphore(frame.render_done, None);
                self.device.destroy_fence(frame.in_flight, None);
                // command buffers are freed when the pool is destroyed
            }

            for &fb in &self.framebuffers {
                self.device.destroy_framebuffer(fb, None);
            }
            self.device.destroy_render_pass(self.render_pass, None);
            for &view in &self.swapchain_image_views {
                self.device.destroy_image_view(view, None);
            }
            self.device.destroy_command_pool(self.command_pool, None);
            self.swapchain_loader.destroy_swapchain(self.swapchain, None);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}

// ── Initialisation ────────────────────────────────────────────────────────────

/// Display geometry returned to the caller for logging.
pub struct DisplayInfo {
    pub width: u32,
    pub height: u32,
}

pub fn init() -> (VkContext, DisplayInfo) {
    // -- Entry (loads libvulkan.so) --------------------------------------------
    let entry = unsafe { ash::Entry::load().expect("failed to load libvulkan.so") };

    // -- Instance -------------------------------------------------------------
    let app_info = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_1);

    // VK_KHR_display lets Vulkan enumerate and drive displays directly,
    // without going through VK_EXT_acquire_drm_display (which requires the
    // Vulkan device and DRM display controller to be the same hardware node —
    // not the case on Tegra where nvgpu and the display controller are separate).
    let instance_extensions = [
        ash::khr::surface::NAME.as_ptr(),
        ash::khr::display::NAME.as_ptr(),
    ];

    let instance_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(&instance_extensions);

    let instance = unsafe {
        entry.create_instance(&instance_info, None).expect("failed to create Vulkan instance")
    };

    // -- Extension loaders ----------------------------------------------------
    let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
    let display_loader = ash::khr::display::Instance::new(&entry, &instance);

    // -- Physical device selection --------------------------------------------
    let physical_devices = unsafe {
        instance.enumerate_physical_devices().expect("no Vulkan physical devices found")
    };

    let (physical_device, graphics_queue_family) = physical_devices
        .iter()
        .find_map(|&pd| find_graphics_queue(&instance, pd).map(|qf| (pd, qf)))
        .expect("no Vulkan device with a graphics queue found");

    // -- Enumerate displays via VK_KHR_display --------------------------------
    // On Tegra, the Vulkan driver enumerates the display controller's outputs
    // directly — no DRM fd or VK_EXT_acquire_drm_display needed.
    let display_props = unsafe {
        display_loader
            .get_physical_device_display_properties(physical_device)
            .expect("vkGetPhysicalDeviceDisplayPropertiesKHR failed")
    };

    assert!(
        !display_props.is_empty(),
        "no Vulkan displays found — is the display connected and the driver loaded?"
    );

    let vk_display = display_props[0].display;
    let width = display_props[0].physical_resolution.width;
    let height = display_props[0].physical_resolution.height;

    // -- Surface via VK_KHR_display -------------------------------------------
    let mode_props = unsafe {
        display_loader
            .get_display_mode_properties(physical_device, vk_display)
            .expect("failed to get display mode properties")
    };

    // Prefer the mode matching the display's native resolution; fall back to first.
    let display_mode = mode_props
        .iter()
        .find(|m| {
            m.parameters.visible_region.width == width
                && m.parameters.visible_region.height == height
        })
        .unwrap_or(&mode_props[0])
        .display_mode;

    // Find a display plane that supports this display.
    let plane_props = unsafe {
        display_loader
            .get_physical_device_display_plane_properties(physical_device)
            .expect("failed to get display plane properties")
    };

    let plane_index = (0..plane_props.len() as u32)
        .find(|&i| unsafe {
            display_loader
                .get_display_plane_supported_displays(physical_device, i)
                .map(|displays| displays.contains(&vk_display))
                .unwrap_or(false)
        })
        .unwrap_or(0);

    let surface_info = vk::DisplaySurfaceCreateInfoKHR::default()
        .display_mode(display_mode)
        .plane_index(plane_index)
        .plane_stack_index(0)
        .transform(vk::SurfaceTransformFlagsKHR::IDENTITY)
        .global_alpha(1.0)
        .alpha_mode(vk::DisplayPlaneAlphaFlagsKHR::OPAQUE)
        .image_extent(vk::Extent2D { width, height });

    let surface = unsafe {
        display_loader
            .create_display_plane_surface(&surface_info, None)
            .expect("failed to create Vulkan display surface")
    };

    // -- Logical device -------------------------------------------------------
    let queue_priorities = [1.0_f32];
    let queue_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(graphics_queue_family)
        .queue_priorities(&queue_priorities);

    let device_extensions = [ash::khr::swapchain::NAME.as_ptr()];

    let device_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(std::slice::from_ref(&queue_info))
        .enabled_extension_names(&device_extensions);

    let device = unsafe {
        instance
            .create_device(physical_device, &device_info, None)
            .expect("failed to create Vulkan logical device")
    };

    let graphics_queue = unsafe { device.get_device_queue(graphics_queue_family, 0) };
    let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);

    // -- Swapchain ------------------------------------------------------------
    let surface_caps = unsafe {
        surface_loader
            .get_physical_device_surface_capabilities(physical_device, surface)
            .expect("failed to query surface capabilities")
    };

    let surface_formats = unsafe {
        surface_loader
            .get_physical_device_surface_formats(physical_device, surface)
            .expect("failed to query surface formats")
    };

    let surface_format = surface_formats
        .iter()
        .find(|f| {
            f.format == vk::Format::B8G8R8A8_UNORM
                && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .copied()
        .unwrap_or(surface_formats[0]);

    let image_count = 2.max(surface_caps.min_image_count).min(
        if surface_caps.max_image_count == 0 {
            u32::MAX
        } else {
            surface_caps.max_image_count
        },
    );

    let extent = vk::Extent2D { width, height };

    let swapchain_info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(surface_format.format)
        .image_color_space(surface_format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(vk::SurfaceTransformFlagsKHR::IDENTITY)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(vk::PresentModeKHR::FIFO)
        .clipped(true);

    let swapchain = unsafe {
        swapchain_loader
            .create_swapchain(&swapchain_info, None)
            .expect("failed to create swapchain")
    };

    let swapchain_images = unsafe {
        swapchain_loader.get_swapchain_images(swapchain).expect("failed to get swapchain images")
    };

    // -- Image views ----------------------------------------------------------
    let swapchain_image_views: Vec<vk::ImageView> = swapchain_images
        .iter()
        .map(|&image| {
            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(surface_format.format)
                .components(vk::ComponentMapping::default())
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            unsafe {
                device.create_image_view(&view_info, None).expect("failed to create image view")
            }
        })
        .collect();

    // -- Command pool ---------------------------------------------------------
    let pool_info = vk::CommandPoolCreateInfo::default()
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
        .queue_family_index(graphics_queue_family);

    let command_pool = unsafe {
        device.create_command_pool(&pool_info, None).expect("failed to create command pool")
    };

    // -- Per-frame sync objects + command buffers -----------------------------
    let cmd_alloc_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(FRAMES_IN_FLIGHT as u32);

    let command_buffers = unsafe {
        device
            .allocate_command_buffers(&cmd_alloc_info)
            .expect("failed to allocate command buffers")
    };

    let frames: Vec<FrameSync> = (0..FRAMES_IN_FLIGHT)
        .map(|i| {
            let sem_info = vk::SemaphoreCreateInfo::default();
            let fence_info =
                vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
            FrameSync {
                image_available: unsafe {
                    device.create_semaphore(&sem_info, None).unwrap()
                },
                render_done: unsafe { device.create_semaphore(&sem_info, None).unwrap() },
                in_flight: unsafe { device.create_fence(&fence_info, None).unwrap() },
                command_buffer: command_buffers[i],
            }
        })
        .collect();

    // Render pass and framebuffers are created by vk_pipeline after this; the
    // VkContext is returned with placeholder values and filled in by the caller.
    // Instead, we build them here with the known format and extent so the caller
    // receives a fully initialised context.
    let render_pass = create_render_pass(&device, surface_format.format);
    let framebuffers =
        create_framebuffers(&device, render_pass, &swapchain_image_views, extent);

    let ctx = VkContext {
        frames,
        framebuffers,
        render_pass,
        swapchain_image_views,
        swapchain_images,
        command_pool,
        swapchain,
        swapchain_loader,
        graphics_queue,
        graphics_queue_family,
        device,
        surface_loader,
        surface,
        physical_device,
        format: surface_format.format,
        extent,
        instance,
        entry,
    };

    (ctx, DisplayInfo { width, height })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn find_graphics_queue(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> Option<u32> {
    let queue_families =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
    queue_families.iter().enumerate().find_map(|(i, props)| {
        props.queue_flags.contains(vk::QueueFlags::GRAPHICS).then_some(i as u32)
    })
}

pub fn create_render_pass(device: &ash::Device, format: vk::Format) -> vk::RenderPass {
    let attachment = vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

    let color_ref =
        vk::AttachmentReference::default().layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);

    let subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(std::slice::from_ref(&color_ref));

    let dependency = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);

    let render_pass_info = vk::RenderPassCreateInfo::default()
        .attachments(std::slice::from_ref(&attachment))
        .subpasses(std::slice::from_ref(&subpass))
        .dependencies(std::slice::from_ref(&dependency));

    unsafe {
        device
            .create_render_pass(&render_pass_info, None)
            .expect("failed to create render pass")
    }
}

pub fn create_framebuffers(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    views: &[vk::ImageView],
    extent: vk::Extent2D,
) -> Vec<vk::Framebuffer> {
    views
        .iter()
        .map(|view| {
            let fb_info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(std::slice::from_ref(view))
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            unsafe {
                device.create_framebuffer(&fb_info, None).expect("failed to create framebuffer")
            }
        })
        .collect()
}
