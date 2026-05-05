use ash::vk;

pub const FRAMES_IN_FLIGHT: usize = 2;

pub struct FrameSync {
    pub image_available: vk::Semaphore,
    pub render_done: vk::Semaphore,
    pub in_flight: vk::Fence,
    pub command_buffer: vk::CommandBuffer,
}

/// All long-lived Vulkan handles shared by both rendering backends.
///
/// Fields are declared in logical drop order; the explicit `Drop` impl destroys
/// them in the correct reverse-initialisation order.
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
    pub surface_loader: ash::khr::surface::Instance,
    pub surface: vk::SurfaceKHR,
    pub physical_device: vk::PhysicalDevice,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    /// Present mode currently used by the swapchain.
    /// Change this field and call `recreate_swapchain` to switch modes.
    pub present_mode: vk::PresentModeKHR,
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
            }
            for &fb in &self.framebuffers {
                self.device.destroy_framebuffer(fb, None);
            }
            self.device.destroy_render_pass(self.render_pass, None);
            for &view in &self.swapchain_image_views {
                self.device.destroy_image_view(view, None);
            }
            self.device.destroy_command_pool(self.command_pool, None);
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}

impl VkContext {
    /// Recreate swapchain, image views, and framebuffers for a new window size
    /// or after changing `self.present_mode`. Call after a resize event or
    /// `VK_ERROR_OUT_OF_DATE_KHR`.
    pub fn recreate_swapchain(&mut self, new_extent: vk::Extent2D) {
        unsafe {
            self.device.device_wait_idle().unwrap();
            for &fb in &self.framebuffers {
                self.device.destroy_framebuffer(fb, None);
            }
            for &view in &self.swapchain_image_views {
                self.device.destroy_image_view(view, None);
            }
        }

        let old_swapchain = self.swapchain;
        let (swapchain, images, views, extent) = create_swapchain(
            &self.swapchain_loader,
            &self.surface_loader,
            self.surface,
            self.physical_device,
            &self.device,
            self.format,
            new_extent,
            self.present_mode,
            old_swapchain,
        );

        unsafe {
            self.swapchain_loader.destroy_swapchain(old_swapchain, None);
        }

        self.framebuffers = create_framebuffers(&self.device, self.render_pass, &views, extent);
        self.swapchain = swapchain;
        self.swapchain_images = images;
        self.swapchain_image_views = views;
        self.extent = extent;
    }
}

/// Pick the best available present mode from a priority list.
/// Falls back to `FIFO` (always guaranteed by the Vulkan spec) if none match.
pub fn select_present_mode(
    surface_loader: &ash::khr::surface::Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    preferred: &[vk::PresentModeKHR],
) -> vk::PresentModeKHR {
    let available = unsafe {
        surface_loader
            .get_physical_device_surface_present_modes(physical_device, surface)
            .unwrap_or_default()
    };
    for &mode in preferred {
        if available.contains(&mode) {
            return mode;
        }
    }
    vk::PresentModeKHR::FIFO
}

/// Build a `VkContext` from an already-created surface.
///
/// Selects a physical device that supports graphics + present on `surface`,
/// creates the logical device, swapchain, command pool, frame sync objects,
/// render pass, and framebuffers.  Both backends call this after they create
/// their backend-specific surface (VK_KHR_display or VK_KHR_surface).
pub fn build_context(
    entry: ash::Entry,
    instance: ash::Instance,
    surface: vk::SurfaceKHR,
    surface_loader: ash::khr::surface::Instance,
    desired_extent: vk::Extent2D,
) -> VkContext {
    // -- Physical device + queue family ---------------------------------------
    let physical_devices = unsafe {
        instance
            .enumerate_physical_devices()
            .expect("no Vulkan physical devices")
    };
    let (physical_device, graphics_queue_family) = physical_devices
        .iter()
        .find_map(|&pd| {
            graphics_queue_with_present(&instance, &surface_loader, pd, surface).map(|qf| (pd, qf))
        })
        .expect("no Vulkan device with graphics+present support");

    // -- Logical device -------------------------------------------------------
    let queue_priorities = [1.0_f32];
    let queue_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(graphics_queue_family)
        .queue_priorities(&queue_priorities);
    let device_exts = [ash::khr::swapchain::NAME.as_ptr()];
    let device_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(std::slice::from_ref(&queue_info))
        .enabled_extension_names(&device_exts);
    let device = unsafe {
        instance
            .create_device(physical_device, &device_info, None)
            .expect("failed to create Vulkan logical device")
    };
    let graphics_queue = unsafe { device.get_device_queue(graphics_queue_family, 0) };
    let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);

    // -- Surface format -------------------------------------------------------
    let formats = unsafe {
        surface_loader
            .get_physical_device_surface_formats(physical_device, surface)
            .expect("failed to query surface formats")
    };
    let format = formats
        .iter()
        .find(|f| {
            f.format == vk::Format::B8G8R8A8_UNORM
                && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .copied()
        .unwrap_or(formats[0])
        .format;

    // -- Swapchain + image views ----------------------------------------------
    let initial_present_mode = vk::PresentModeKHR::FIFO;
    let (swapchain, swapchain_images, swapchain_image_views, extent) = create_swapchain(
        &swapchain_loader,
        &surface_loader,
        surface,
        physical_device,
        &device,
        format,
        desired_extent,
        initial_present_mode,
        vk::SwapchainKHR::null(),
    );

    // -- Command pool ---------------------------------------------------------
    let pool_info = vk::CommandPoolCreateInfo::default()
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
        .queue_family_index(graphics_queue_family);
    let command_pool = unsafe {
        device
            .create_command_pool(&pool_info, None)
            .expect("failed to create command pool")
    };

    // -- Frame sync objects + command buffers ---------------------------------
    let cmd_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(FRAMES_IN_FLIGHT as u32);
    let cbs = unsafe {
        device
            .allocate_command_buffers(&cmd_info)
            .expect("failed to allocate command buffers")
    };
    let frames: Vec<FrameSync> = (0..FRAMES_IN_FLIGHT)
        .map(|i| {
            let sem = vk::SemaphoreCreateInfo::default();
            let fence = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
            FrameSync {
                image_available: unsafe { device.create_semaphore(&sem, None).unwrap() },
                render_done: unsafe { device.create_semaphore(&sem, None).unwrap() },
                in_flight: unsafe { device.create_fence(&fence, None).unwrap() },
                command_buffer: cbs[i],
            }
        })
        .collect();

    // -- Render pass + framebuffers -------------------------------------------
    let render_pass = create_render_pass(&device, format);
    let framebuffers = create_framebuffers(&device, render_pass, &swapchain_image_views, extent);

    VkContext {
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
        format,
        extent,
        present_mode: initial_present_mode,
        instance,
        entry,
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

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
    let dep = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);
    let info = vk::RenderPassCreateInfo::default()
        .attachments(std::slice::from_ref(&attachment))
        .subpasses(std::slice::from_ref(&subpass))
        .dependencies(std::slice::from_ref(&dep));
    unsafe {
        device
            .create_render_pass(&info, None)
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
            let info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(std::slice::from_ref(view))
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            unsafe {
                device
                    .create_framebuffer(&info, None)
                    .expect("failed to create framebuffer")
            }
        })
        .collect()
}

fn create_swapchain(
    swapchain_loader: &ash::khr::swapchain::Device,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    physical_device: vk::PhysicalDevice,
    device: &ash::Device,
    format: vk::Format,
    desired_extent: vk::Extent2D,
    present_mode: vk::PresentModeKHR,
    old_swapchain: vk::SwapchainKHR,
) -> (
    vk::SwapchainKHR,
    Vec<vk::Image>,
    Vec<vk::ImageView>,
    vk::Extent2D,
) {
    let caps = unsafe {
        surface_loader
            .get_physical_device_surface_capabilities(physical_device, surface)
            .expect("failed to query surface capabilities")
    };
    let image_count = 2
        .max(caps.min_image_count)
        .min(if caps.max_image_count == 0 {
            u32::MAX
        } else {
            caps.max_image_count
        });
    let extent = if caps.current_extent.width != u32::MAX {
        caps.current_extent
    } else {
        vk::Extent2D {
            width: desired_extent
                .width
                .clamp(caps.min_image_extent.width, caps.max_image_extent.width),
            height: desired_extent
                .height
                .clamp(caps.min_image_extent.height, caps.max_image_extent.height),
        }
    };

    let info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(format)
        .image_color_space(vk::ColorSpaceKHR::SRGB_NONLINEAR)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(vk::SurfaceTransformFlagsKHR::IDENTITY)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(present_mode)
        .clipped(true)
        .old_swapchain(old_swapchain);

    let swapchain = unsafe {
        swapchain_loader
            .create_swapchain(&info, None)
            .expect("failed to create swapchain")
    };
    let images = unsafe {
        swapchain_loader
            .get_swapchain_images(swapchain)
            .expect("failed to get swapchain images")
    };
    let views: Vec<vk::ImageView> = images
        .iter()
        .map(|&image| {
            let info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
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
                    .expect("failed to create image view")
            }
        })
        .collect();
    (swapchain, images, views, extent)
}

fn graphics_queue_with_present(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    pd: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
) -> Option<u32> {
    let families = unsafe { instance.get_physical_device_queue_family_properties(pd) };
    families.iter().enumerate().find_map(|(i, props)| {
        let gfx = props.queue_flags.contains(vk::QueueFlags::GRAPHICS);
        let present = unsafe {
            surface_loader
                .get_physical_device_surface_support(pd, i as u32, surface)
                .unwrap_or(false)
        };
        (gfx && present).then_some(i as u32)
    })
}
