use ash::vk;

/// Load Vulkan and create an instance requesting `extensions`.
///
/// In debug builds, `VK_EXT_debug_utils` is appended and silently dropped if
/// the driver rejects it (e.g. when RenderDoc is not injected).
///
/// Returns `(Entry, Instance, debug_utils_enabled)`.
pub fn create_vk_instance(
    extensions: &[*const std::ffi::c_char],
) -> (ash::Entry, ash::Instance, bool) {
    let entry = unsafe { ash::Entry::load().expect("failed to load libvulkan.so") };
    let app_info = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_1);

    #[cfg(debug_assertions)]
    let (instance, debug_utils_enabled) = {
        let mut with_debug: Vec<*const std::ffi::c_char> = extensions.to_vec();
        with_debug.push(ash::ext::debug_utils::NAME.as_ptr());
        let info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&with_debug);
        match unsafe { entry.create_instance(&info, None) } {
            Ok(inst) => (inst, true),
            Err(vk::Result::ERROR_EXTENSION_NOT_PRESENT) => {
                log::debug!(
                    "vstimd: VK_EXT_debug_utils not accepted at vkCreateInstance — disabling"
                );
                let info_bare = vk::InstanceCreateInfo::default()
                    .application_info(&app_info)
                    .enabled_extension_names(extensions);
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
            .enabled_extension_names(extensions);
        let inst = unsafe {
            entry
                .create_instance(&info, None)
                .expect("failed to create Vulkan instance")
        };
        (inst, false)
    };

    (entry, instance, debug_utils_enabled)
}
