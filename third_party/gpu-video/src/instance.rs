use std::sync::Arc;

use ash::{Entry, vk};

use crate::{
    VulkanInitError,
    adapter::{VulkanAdapter, VulkanAdapterDescriptor},
    wrappers::*,
};

/// Context for all encoders and decoders. Also contains a [`wgpu::Instance`].
pub struct VulkanInstance {
    #[cfg(feature = "wgpu")]
    pub(crate) wgpu_instance: wgpu::Instance,

    _entry: Arc<Entry>,
    pub(crate) instance: Arc<Instance>,
    _debug_messenger: Option<DebugMessenger>,
}

impl VulkanInstance {
    pub fn new() -> Result<Arc<Self>, VulkanInitError> {
        let entry = Arc::new(unsafe { Entry::load()? });
        Self::new_from_entry(entry)
    }

    pub fn new_from(
        vulkan_library_path: impl AsRef<std::ffi::OsStr>,
    ) -> Result<Arc<Self>, VulkanInitError> {
        let entry = Arc::new(unsafe { Entry::load_from(vulkan_library_path)? });
        Self::new_from_entry(entry)
    }

    fn new_from_entry(entry: Arc<Entry>) -> Result<Arc<Self>, VulkanInitError> {
        let api_version = vk::make_api_version(0, 1, 3, 0);
        let app_info = vk::ApplicationInfo {
            api_version,
            ..Default::default()
        };

        let mut requested_layers = Vec::new();

        if cfg!(feature = "vk-validation") {
            requested_layers.push(c"VK_LAYER_KHRONOS_validation");
        }

        if cfg!(feature = "vk-api-dump") {
            requested_layers.push(c"VK_LAYER_LUNARG_api_dump");
        }

        let instance_layer_properties = unsafe { entry.enumerate_instance_layer_properties()? };
        let instance_layer_names = instance_layer_properties
            .iter()
            .map(|layer| layer.layer_name_as_c_str())
            .collect::<Result<Vec<_>, _>>()?;

        let layers = requested_layers
            .into_iter()
            .filter(|requested_layer_name| {
                instance_layer_names
                    .iter()
                    .any(|instance_layer_name| instance_layer_name == requested_layer_name)
            })
            .map(|layer| layer.as_ptr())
            .collect::<Vec<_>>();

        let extensions = vec![vk::EXT_DEBUG_UTILS_NAME];

        let extensions = extensions.into_iter().collect::<Vec<_>>();
        #[cfg(feature = "wgpu")]
        let extensions = merge_with_wgpu_instance_extensions(&entry, api_version, extensions)?;

        let extension_ptrs = extensions.iter().map(|e| e.as_ptr()).collect::<Vec<_>>();

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layers)
            .enabled_extension_names(&extension_ptrs);

        let instance = unsafe { entry.create_instance(&create_info, None) }?;
        let video_queue_instance_ext = ash::khr::video_queue::Instance::new(&entry, &instance);
        let video_encode_queue_instance_ext =
            ash::khr::video_encode_queue::Instance::new(&entry, &instance);
        let debug_utils_instance_ext = ash::ext::debug_utils::Instance::new(&entry, &instance);

        let instance = Arc::new(Instance {
            instance,
            _entry: entry.clone(),
            video_queue_instance_ext,
            debug_utils_instance_ext,
            video_encode_queue_instance_ext,
        });

        let debug_messenger = if cfg!(debug_assertions) {
            Some(DebugMessenger::new(instance.clone())?)
        } else {
            None
        };

        #[cfg(feature = "wgpu")]
        let wgpu_instance =
            create_wgpu_instance(&entry, instance.clone(), api_version, extensions)?;

        Ok(Self {
            _entry: entry,
            instance,
            _debug_messenger: debug_messenger,

            #[cfg(feature = "wgpu")]
            wgpu_instance,
        }
        .into())
    }

    #[cfg(feature = "wgpu")]
    pub fn wgpu_instance(&self) -> wgpu::Instance {
        self.wgpu_instance.clone()
    }

    /// Creates an adapter that meets requirements specified in the descriptor.
    pub fn create_adapter<'a>(
        &'a self,
        descriptor: &VulkanAdapterDescriptor,
    ) -> Result<VulkanAdapter<'a>, VulkanInitError> {
        self.iter_adapters()?
            .find(|adapter| {
                if (descriptor.supports_decoding && !adapter.supports_decoding())
                    || (descriptor.supports_encoding && !adapter.supports_encoding())
                {
                    return false;
                }

                #[cfg(feature = "wgpu")]
                if let Some(surface) = descriptor.compatible_surface
                    && !adapter.supports_surface(surface)
                {
                    return false;
                }

                true
            })
            .ok_or(VulkanInitError::NoDevice)
    }

    /// Iterator over all available [`VulkanAdapter`]s that support at least decoding or encoding.
    pub fn iter_adapters<'a>(
        &'a self,
    ) -> Result<impl Iterator<Item = VulkanAdapter<'a>> + 'a, VulkanInitError> {
        crate::adapter::iter_adapters(self)
    }
}

impl std::fmt::Debug for VulkanInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanInstance").finish()
    }
}

#[cfg(feature = "wgpu")]
fn merge_with_wgpu_instance_extensions(
    entry: &Entry,
    api_version: u32,
    extensions: Vec<&'static std::ffi::CStr>,
) -> Result<Vec<&'static std::ffi::CStr>, crate::WgpuInitError> {
    let wgpu_extensions = wgpu::hal::vulkan::Instance::desired_extensions(
        entry,
        api_version,
        wgpu::InstanceFlags::empty(),
    )?;

    Ok([extensions, wgpu_extensions].concat())
}

#[cfg(feature = "wgpu")]
fn create_wgpu_instance(
    entry: &Entry,
    instance: Arc<Instance>,
    api_version: u32,
    extensions: Vec<&'static std::ffi::CStr>,
) -> Result<wgpu::Instance, crate::WgpuInitError> {
    let wgpu_instance = unsafe {
        wgpu::hal::vulkan::Instance::from_raw(
            (*entry).clone(),
            instance.instance.clone(),
            api_version,
            0,
            None,
            extensions,
            wgpu::InstanceFlags::ALLOW_UNDERLYING_NONCOMPLIANT_ADAPTER,
            wgpu::MemoryBudgetThresholds::default(),
            false,
            Some(Box::new(move || {
                drop(instance);
            })),
        )?
    };

    Ok(unsafe { wgpu::Instance::from_hal::<wgpu::hal::vulkan::Api>(wgpu_instance) })
}
