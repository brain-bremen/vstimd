use ash::vk;

use crate::render::StimulusDisplayInfo;
use crate::render::vk::{VkContext, build_context};

/// Initialise Vulkan for bare-metal display via `VK_KHR_display`.
///
/// Enumerates connected displays, picks a mode, creates the display surface,
/// and returns a fully-initialised `VkContext` plus the `VkDisplayKHR` handle
/// (needed for `VK_EXT_display_control` vblank fences).
pub fn init() -> (VkContext, StimulusDisplayInfo, vk::DisplayKHR) {
    let entry = unsafe { ash::Entry::load().expect("failed to load libvulkan.so") };

    let app_info = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_1);

    // Check which optional instance extensions are available.
    let available_inst_exts: std::collections::HashSet<String> = unsafe {
        entry
            .enumerate_instance_extension_properties(None)
            .unwrap_or_default()
            .into_iter()
            .map(|e| {
                std::ffi::CStr::from_ptr(e.extension_name.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    };
    // VK_EXT_display_surface_counter is an instance extension required by
    // VK_EXT_display_control (device).  Enable it when available.
    let use_display_surface_counter =
        available_inst_exts.contains("VK_EXT_display_surface_counter");

    // VK_KHR_display lets Vulkan enumerate and drive displays directly
    // without requiring a compositor.
    // In debug builds, also attempt VK_EXT_debug_utils for RenderDoc labels.
    // Fall back silently if the driver rejects it (e.g. RenderDoc hook injection).
    let mut base_exts = vec![
        ash::khr::surface::NAME.as_ptr(),
        ash::khr::display::NAME.as_ptr(),
    ];
    if use_display_surface_counter {
        base_exts.push(ash::ext::display_surface_counter::NAME.as_ptr());
    }

    #[cfg(debug_assertions)]
    let (instance, debug_utils_enabled) = {
        let mut exts_with = base_exts.clone();
        exts_with.push(ash::ext::debug_utils::NAME.as_ptr());
        let info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&exts_with);
        match unsafe { entry.create_instance(&info, None) } {
            Ok(inst) => (inst, true),
            Err(vk::Result::ERROR_EXTENSION_NOT_PRESENT) => {
                log::debug!(
                    "vstimd: VK_EXT_debug_utils not accepted at vkCreateInstance — disabling"
                );
                let info_bare = vk::InstanceCreateInfo::default()
                    .application_info(&app_info)
                    .enabled_extension_names(&base_exts);
                let inst = unsafe {
                    entry
                        .create_instance(&info_bare, None)
                        .expect("failed to create Vulkan instance")
                };
                (inst, false)
            }
            Err(e) => panic!("failed to create Vulkan instance: {e}"),
        }
    };
    #[cfg(not(debug_assertions))]
    let (instance, debug_utils_enabled) = {
        let info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&base_exts);
        let inst = unsafe {
            entry
                .create_instance(&info, None)
                .expect("failed to create Vulkan instance")
        };
        (inst, false)
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
    assert!(
        !mode_props.is_empty(),
        "no display modes reported for display — check driver and display connection"
    );
    let (mode_index, chosen) = pick_mode(&mode_props);
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
    let ctx = build_context(
        entry,
        instance,
        surface,
        surface_loader,
        extent,
        debug_utils_enabled,
        use_display_surface_counter,
    );

    let refresh_hz = chosen.parameters.refresh_rate as f64 / 1000.0;
    log::info!("vstimd: display {}×{}  {:.3} Hz", width, height, refresh_hz);

    (
        ctx,
        StimulusDisplayInfo {
            width_px: width,
            height_px: height,
            refresh_hz,
            mode_index: Some(mode_index),
        },
        vk_display,
    )
}

fn pick_mode(modes: &[vk::DisplayModePropertiesKHR]) -> (usize, vk::DisplayModePropertiesKHR) {
    log::info!("vstimd: available display modes:");
    for (i, m) in modes.iter().enumerate() {
        let w = m.parameters.visible_region.width;
        let h = m.parameters.visible_region.height;
        let hz = m.parameters.refresh_rate;
        log::info!("  [{}] {}×{}  {}.{:03} Hz", i, w, h, hz / 1000, hz % 1000);
    }

    // Allow override via VSTIMD_DISPLAY_MODE=<index>.
    if let Ok(s) = std::env::var("VSTIMD_DISPLAY_MODE") {
        match s.trim().parse::<usize>() {
            Ok(i) if i < modes.len() => {
                let m = &modes[i];
                let w = m.parameters.visible_region.width;
                let h = m.parameters.visible_region.height;
                let hz = m.parameters.refresh_rate;
                log::info!(
                    "vstimd: using display mode {} (VSTIMD_DISPLAY_MODE): {}×{}  {}.{:03} Hz",
                    i,
                    w,
                    h,
                    hz / 1000,
                    hz % 1000
                );
                return (i, modes[i]);
            }
            Ok(i) => log::warn!(
                "vstimd: VSTIMD_DISPLAY_MODE={i} out of range (0..{}), using auto-select",
                modes.len()
            ),
            Err(_) => {
                log::warn!("vstimd: VSTIMD_DISPLAY_MODE={s:?} is not a number, using auto-select")
            }
        }
    }

    // Auto-select the mode with the highest refresh rate.
    // modes is guaranteed non-empty by the assert at the call site.
    let (best_idx, best) = modes
        .iter()
        .enumerate()
        .max_by_key(|(_, m)| m.parameters.refresh_rate)
        .expect("mode list is empty");
    let w = best.parameters.visible_region.width;
    let h = best.parameters.visible_region.height;
    let hz = best.parameters.refresh_rate;
    log::info!(
        "vstimd: auto-selected display mode {}×{}  {}.{:03} Hz",
        w,
        h,
        hz / 1000,
        hz % 1000
    );
    (best_idx, *best)
}

fn find_graphics_queue(instance: &ash::Instance, pd: vk::PhysicalDevice) -> Option<u32> {
    let families = unsafe { instance.get_physical_device_queue_family_properties(pd) };
    families.iter().enumerate().find_map(|(i, p)| {
        p.queue_flags
            .contains(vk::QueueFlags::GRAPHICS)
            .then_some(i as u32)
    })
}
