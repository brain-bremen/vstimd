use ash::vk;

use crate::render::vk::{VkContext, build_context};
use crate::render::StimulusDisplayInfo;

/// Initialise Vulkan for bare-metal display via `VK_KHR_display`.
///
/// Enumerates connected displays, prompts the user to pick a mode, creates the
/// display surface, and returns a fully initialised `VkContext`.
pub fn init() -> (VkContext, StimulusDisplayInfo) {
    let entry = unsafe { ash::Entry::load().expect("failed to load libvulkan.so") };

    let app_info = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_1);

    // VK_KHR_display lets Vulkan enumerate and drive displays directly
    // without requiring a compositor.
    let instance_exts = vec![
        ash::khr::surface::NAME.as_ptr(),
        ash::khr::display::NAME.as_ptr(),
    ];

    let instance_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(&instance_exts);
    let instance = unsafe {
        entry
            .create_instance(&instance_info, None)
            .expect("failed to create Vulkan instance")
    };

    let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
    let display_loader = ash::khr::display::Instance::new(&entry, &instance);

    // Pick a physical device that has a graphics queue.
    let physical_devices = unsafe {
        instance
            .enumerate_physical_devices()
            .expect("no Vulkan physical devices")
    };
    let (physical_device, _) = physical_devices
        .iter()
        .find_map(|&pd| find_graphics_queue(&instance, pd).map(|qf| (pd, qf)))
        .expect("no Vulkan device with a graphics queue");

    // Enumerate connected displays; render to display[0] for now.
    let all_display_props = unsafe {
        display_loader
            .get_physical_device_display_properties(physical_device)
            .expect("vkGetPhysicalDeviceDisplayPropertiesKHR failed")
    };
    assert!(
        !all_display_props.is_empty(),
        "no Vulkan displays found — is the display connected and the driver loaded?"
    );
    let vk_display = all_display_props[0].display;

    let mode_props = unsafe {
        display_loader
            .get_display_mode_properties(physical_device, vk_display)
            .expect("failed to get display mode properties")
    };
    let chosen = pick_mode(&mode_props);
    let display_mode = chosen.display_mode;
    let width = chosen.parameters.visible_region.width;
    let height = chosen.parameters.visible_region.height;

    let plane_props = unsafe {
        display_loader
            .get_physical_device_display_plane_properties(physical_device)
            .expect("failed to get display plane properties")
    };
    let plane_index = (0..plane_props.len() as u32)
        .find(|&i| unsafe {
            display_loader
                .get_display_plane_supported_displays(physical_device, i)
                .map(|ds| ds.contains(&vk_display))
                .unwrap_or(false)
        })
        .unwrap_or(0);

    let surface = unsafe {
        display_loader
            .create_display_plane_surface(
                &vk::DisplaySurfaceCreateInfoKHR::default()
                    .display_mode(display_mode)
                    .plane_index(plane_index)
                    .plane_stack_index(0)
                    .transform(vk::SurfaceTransformFlagsKHR::IDENTITY)
                    .global_alpha(1.0)
                    .alpha_mode(vk::DisplayPlaneAlphaFlagsKHR::OPAQUE)
                    .image_extent(vk::Extent2D { width, height }),
                None,
            )
            .expect("failed to create Vulkan display surface")
    };

    let extent = vk::Extent2D { width, height };
    let ctx = build_context(entry, instance, surface, surface_loader, extent);

    let refresh_hz = chosen.parameters.refresh_rate as f64 / 1000.0;
    log::info!("wonderlamp: display {}×{}  {:.3} Hz", width, height, refresh_hz);

    (
        ctx,
        StimulusDisplayInfo {
            width_px: width,
            height_px: height,
            refresh_hz,
        },
    )
}

fn pick_mode(modes: &[vk::DisplayModePropertiesKHR]) -> vk::DisplayModePropertiesKHR {
    eprintln!("\nAvailable display modes:");
    for (i, m) in modes.iter().enumerate() {
        let w = m.parameters.visible_region.width;
        let h = m.parameters.visible_region.height;
        let hz = m.parameters.refresh_rate;
        eprintln!("  [{i}] {w}×{h}  {}.{:03} Hz", hz / 1000, hz % 1000);
    }
    loop {
        eprint!("Select mode [0]: ");
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .expect("failed to read stdin");
        let t = line.trim();
        if t.is_empty() {
            return modes[0];
        }
        match t.parse::<usize>() {
            Ok(i) if i < modes.len() => return modes[i],
            _ => eprintln!("  Enter 0–{}", modes.len() - 1),
        }
    }
}

fn find_graphics_queue(instance: &ash::Instance, pd: vk::PhysicalDevice) -> Option<u32> {
    let families = unsafe { instance.get_physical_device_queue_family_properties(pd) };
    families.iter().enumerate().find_map(|(i, p)| {
        p.queue_flags
            .contains(vk::QueueFlags::GRAPHICS)
            .then_some(i as u32)
    })
}
