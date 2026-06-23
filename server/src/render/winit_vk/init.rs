use ash::vk;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

use crate::render::vk::{VkContext, build_context};

/// Create a `VkContext` from a winit window using `VK_KHR_surface`.
pub fn init(window: &Window) -> VkContext {
    let entry = unsafe { ash::Entry::load().expect("failed to load libvulkan.so") };

    let display_handle = window.display_handle().unwrap().as_raw();
    let window_handle = window.window_handle().unwrap().as_raw();

    // ash-window provides the right platform surface extension(s) for this display.
    let surface_exts = {
        ash_window::enumerate_required_extensions(display_handle)
            .expect("failed to enumerate required Vulkan surface extensions")
    };

    // Log requested extensions (debug builds only)
    #[cfg(debug_assertions)]
    {
        log::debug!("vstimd: Vulkan instance extensions requested by ash_window:");
        for ext in surface_exts {
            let name = unsafe { std::ffi::CStr::from_ptr(*ext) };
            log::debug!("  {:?}", name);
        }
    }

    let app_info = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_1);

    // Log available instance extensions (debug builds only)
    #[cfg(debug_assertions)]
    let _available_ext_names = {
        let available = unsafe {
            entry
                .enumerate_instance_extension_properties(None)
                .unwrap_or_default()
        };
        log::debug!("vstimd: Available Vulkan instance extensions:");
        let names: std::collections::HashSet<String> = available
            .iter()
            .map(|ext| {
                let name = unsafe { std::ffi::CStr::from_ptr(ext.extension_name.as_ptr()) };
                let name_str = name.to_string_lossy().into_owned();
                log::debug!("  {:?}", name);
                name_str
            })
            .collect();

        // Check which requested extensions are available
        log::debug!("vstimd: Extension availability check:");
        for ext in surface_exts {
            let name = unsafe { std::ffi::CStr::from_ptr(*ext) };
            let name_str = name.to_string_lossy();
            let available = names.contains(name_str.as_ref());
            log::debug!(
                "  {:?}: {}",
                name,
                if available {
                    "✓ available"
                } else {
                    "✗ MISSING"
                }
            );
        }

        names
    };
    #[cfg(not(debug_assertions))]
    let _available_ext_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Attempt to enable VK_EXT_debug_utils in debug builds (needed for RenderDoc
    // labels and object names). Some drivers/injectors (e.g. RenderDoc via LD_PRELOAD)
    // advertise the extension through vkEnumerateInstanceExtensionProperties but then
    // reject it at vkCreateInstance time. We therefore try with it first and silently
    // fall back to without if we get ERROR_EXTENSION_NOT_PRESENT.
    #[cfg(debug_assertions)]
    let (instance, debug_utils_enabled) = {
        let mut exts_with: Vec<*const std::ffi::c_char> = surface_exts.to_vec();
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
                    .enabled_extension_names(surface_exts);
                let inst = unsafe {
                    entry.create_instance(&info_bare, None).unwrap_or_else(|e| {
                        log::error!("vstimd: Failed to create Vulkan instance even without VK_EXT_debug_utils");
                        log::error!("vstimd: Error: {:?}", e);
                        log::error!("vstimd: This suggests one of the required surface extensions is not available.");
                        panic!("failed to create Vulkan instance: {:?}", e);
                    })
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
            .enabled_extension_names(surface_exts);
        let inst = unsafe {
            entry.create_instance(&info, None).unwrap_or_else(|e| {
                log::error!("vstimd: Failed to create Vulkan instance");
                log::error!("vstimd: Error: {:?}", e);
                panic!("failed to create Vulkan instance: {:?}", e);
            })
        };
        (inst, false)
    };

    let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
    let surface = unsafe {
        ash_window::create_surface(&entry, &instance, display_handle, window_handle, None)
            .expect("failed to create Vulkan window surface")
    };

    let size = window.inner_size();
    let extent = vk::Extent2D {
        width: size.width.max(1),
        height: size.height.max(1),
    };

    build_context(
        entry,
        instance,
        surface,
        surface_loader,
        extent,
        debug_utils_enabled,
        false,
    )
}
