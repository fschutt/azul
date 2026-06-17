use ash::vk;
use std::{
    ffi::CStr,
    fmt::{self, Debug},
    sync::Arc,
};
use tracing::{debug, debug_span, warn};

#[cfg(feature = "wgpu")]
use wgpu::hal::{DynAdapter, vulkan::Api as VkApi};

use crate::{
    VulkanDevice, VulkanInitError, VulkanInstance,
    capabilities::EncodeCapabilities,
    device::{
        DECODE_CODEC_EXTENSIONS, DECODE_EXTENSIONS, ENCODE_CODEC_EXTENSIONS, ENCODE_EXTENSIONS,
        REQUIRED_EXTENSIONS, VulkanDeviceDescriptor,
        caps::{DecodeCapabilities, NativeDecodeCapabilities, NativeEncodeCapabilities},
        queues::{QueueIndex, QueueIndices},
    },
};

/// Represents a handle to a physical device.
/// Can be used to create [`VulkanDevice`].
pub struct VulkanAdapter<'a> {
    #[cfg(feature = "wgpu")]
    pub(crate) wgpu_adapter: wgpu::hal::ExposedAdapter<VkApi>,

    pub(crate) instance: &'a VulkanInstance,
    pub(crate) physical_device: vk::PhysicalDevice,
    pub(crate) queue_indices: QueueIndices<'static>,
    pub(crate) decode_capabilities: Option<NativeDecodeCapabilities>,
    pub(crate) encode_capabilities: Option<NativeEncodeCapabilities>,
    pub(crate) info: AdapterInfo,
}

impl<'a> VulkanAdapter<'a> {
    fn new(vulkan_instance: &'a VulkanInstance, device: vk::PhysicalDevice) -> Option<Self> {
        let instance = &vulkan_instance.instance;

        #[cfg(feature = "wgpu")]
        let wgpu_adapter = expose_wgpu_adapter(vulkan_instance, device)?;

        let properties = unsafe { instance.get_physical_device_properties(device) };
        let device_name = properties
            .device_name_as_c_str()
            .map(CStr::to_string_lossy)
            .unwrap_or("unknown".into());

        let _span = debug_span!("creating adapter", device_name = %device_name).entered();

        let mut vk_13_features = vk::PhysicalDeviceVulkan13Features::default();
        let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut vk_13_features);

        unsafe { instance.get_physical_device_features2(device, &mut features) };
        let extensions = match unsafe { instance.enumerate_device_extension_properties(device) } {
            Ok(ext) => ext,
            Err(err) => {
                warn!("Couldn't enumerate device extension properties: {err}");
                return None;
            }
        };

        if vk_13_features.synchronization2 == vk::FALSE {
            debug!("device does not support the required synchronization2 feature");
            return None;
        }

        if let Err(missing) = check_extensions(REQUIRED_EXTENSIONS, &extensions) {
            debug!(missing_extensions = ?missing, "device is missing some required extensions",);
            return None;
        }

        let has_decode_extensions = check_extensions(DECODE_EXTENSIONS, &extensions).is_ok();
        let supported_decode_codec_extensions =
            supported_extensions(DECODE_CODEC_EXTENSIONS, &extensions);
        let supports_any_decoding =
            has_decode_extensions && !supported_decode_codec_extensions.is_empty();
        let supported_decode_operations =
            extensions_to_codec_operations(&supported_decode_codec_extensions);

        let has_encode_extensions = check_extensions(ENCODE_EXTENSIONS, &extensions).is_ok();
        let supported_encode_codec_extensions =
            supported_extensions(ENCODE_CODEC_EXTENSIONS, &extensions);
        let supports_any_encoding =
            has_encode_extensions && !supported_encode_codec_extensions.is_empty();
        let supported_encode_operations =
            extensions_to_codec_operations(&supported_encode_codec_extensions);

        if !supports_any_decoding && !supports_any_encoding {
            debug!("device does not support encoding or decoding extensions");
            return None;
        }

        let queues_len =
            unsafe { instance.get_physical_device_queue_family_properties2_len(device) };
        let mut queues = vec![vk::QueueFamilyProperties2::default(); queues_len];
        let mut video_properties = vec![vk::QueueFamilyVideoPropertiesKHR::default(); queues_len];
        let mut query_result_status_properties =
            vec![vk::QueueFamilyQueryResultStatusPropertiesKHR::default(); queues_len];

        for ((queue, video_properties), query_result_properties) in queues
            .iter_mut()
            .zip(video_properties.iter_mut())
            .zip(query_result_status_properties.iter_mut())
        {
            *queue = queue
                .push_next(query_result_properties)
                .push_next(video_properties);
        }

        unsafe { instance.get_physical_device_queue_family_properties2(device, &mut queues) };

        let decode_capabilities =
            NativeDecodeCapabilities::query(instance, device, supported_decode_operations);

        let encode_capabilities =
            NativeEncodeCapabilities::query(instance, device, supported_encode_operations);

        let queue_counts = queues
            .iter()
            .map(|q| q.queue_family_properties.queue_count)
            .collect::<Vec<_>>();

        let transfer_queue_idx = queues
            .iter()
            .enumerate()
            .find(|(_, q)| {
                q.queue_family_properties
                    .queue_flags
                    .contains(vk::QueueFlags::TRANSFER)
                    && !q
                        .queue_family_properties
                        .queue_flags
                        .intersects(vk::QueueFlags::GRAPHICS)
            })
            .map(|(i, _)| i)?;

        let compute_queue_idx = queues
            .iter()
            .enumerate()
            .find(|(_, q)| {
                q.queue_family_properties
                    .queue_flags
                    .contains(vk::QueueFlags::COMPUTE)
                    && !q
                        .queue_family_properties
                        .queue_flags
                        .intersects(vk::QueueFlags::GRAPHICS)
            })
            // azul patch (third_party vendor): GPUs without a dedicated
            // async-compute queue — e.g. NVIDIA Maxwell / GTX 9xx — only expose
            // COMPUTE on the graphics family, so the original
            // `COMPUTE && !GRAPHICS` search returns None and the whole adapter is
            // rejected. The bytes-decode path never submits to the compute queue
            // (only the optional `transcoder` feature does), so fall back to any
            // compute-capable queue instead of failing. See third_party/README.
            .or_else(|| {
                queues.iter().enumerate().find(|(_, q)| {
                    q.queue_family_properties
                        .queue_flags
                        .contains(vk::QueueFlags::COMPUTE)
                })
            })
            .map(|(i, _)| i)?;

        let graphics_transfer_compute_queue_idx = queues
            .iter()
            .enumerate()
            .find(|(_, q)| {
                q.queue_family_properties.queue_flags.contains(
                    vk::QueueFlags::GRAPHICS | vk::QueueFlags::TRANSFER | vk::QueueFlags::COMPUTE,
                )
            })
            .map(|(i, _)| i)?;

        let decode_queue_idx = match supports_any_decoding {
            true => find_video_queue_idx(
                &queues,
                vk::QueueFlags::VIDEO_DECODE_KHR,
                // TODO: for now, we only look for a single queue that supports all decoding
                supported_decode_operations,
            ),
            false => None,
        };
        let encode_queue_idx = match supports_any_encoding {
            true => find_video_queue_idx(
                &queues,
                vk::QueueFlags::VIDEO_ENCODE_KHR,
                // TODO: for now, we only look for a single queue that supports all encoding
                supported_encode_operations,
            ),
            false => None,
        };

        if decode_queue_idx.is_none() && encode_queue_idx.is_none() {
            debug!("device does not have any queues that support video operations");
            return None;
        }

        debug!("decode capabilities: {decode_capabilities:#?}");
        debug!("encode capabilities: {encode_capabilities:#?}");

        let (driver_name, driver_info) = match properties.api_version >= vk::API_VERSION_1_2 {
            true => {
                let mut driver_properties = vk::PhysicalDeviceDriverProperties::default();
                let mut properties2 =
                    vk::PhysicalDeviceProperties2::default().push_next(&mut driver_properties);
                unsafe {
                    instance.get_physical_device_properties2(device, &mut properties2);
                }

                let driver_name = driver_properties
                    .driver_name_as_c_str()
                    .map(CStr::to_string_lossy)
                    .unwrap_or("unknown".into())
                    .into_owned();
                let driver_info = driver_properties
                    .driver_info_as_c_str()
                    .map(CStr::to_string_lossy)
                    .unwrap_or_default()
                    .into_owned();
                (driver_name, driver_info)
            }
            false => ("unknown".to_owned(), "".to_owned()),
        };

        let info = AdapterInfo {
            name: device_name.into_owned(),
            driver_name,
            driver_info,
            device_type: properties.device_type,
            device_properties: properties,
            supports_decoding: decode_queue_idx.is_some(),
            supports_encoding: encode_queue_idx.is_some(),
            decode_capabilities: decode_capabilities.user_facing(),
            encode_capabilities: encode_capabilities.user_facing(),
        };

        Some(Self {
            #[cfg(feature = "wgpu")]
            wgpu_adapter,

            instance: vulkan_instance,
            physical_device: device,
            queue_indices: QueueIndices {
                transfer: QueueIndex {
                    family_index: transfer_queue_idx,
                    queue_count: queue_counts[transfer_queue_idx] as usize,
                    video_properties: video_properties[transfer_queue_idx],
                    query_result_status_properties: query_result_status_properties
                        [transfer_queue_idx],
                },
                compute: QueueIndex {
                    family_index: compute_queue_idx,
                    // azul patch: was `queue_counts[compute_queue_idx]`. When the
                    // compute fallback above picks the graphics family (no dedicated
                    // async-compute queue, e.g. Maxwell), compute and
                    // graphics_transfer_compute share that family but with different
                    // counts (16 vs 1). queue_create_infos() dedups by (family,count)
                    // *tuple*, so the family appeared TWICE in VkDeviceCreateInfo —
                    // an illegal duplicate queue family that segfaults the NVIDIA
                    // driver in vkCreateDevice. Only queue index 0 is ever retrieved
                    // for compute (see device.rs), so cap to 1 to match gtc and dedup.
                    queue_count: 1,
                    video_properties: video_properties[compute_queue_idx],
                    query_result_status_properties: query_result_status_properties
                        [compute_queue_idx],
                },
                h264_decode: decode_queue_idx.map(|idx| QueueIndex {
                    family_index: idx,
                    queue_count: queue_counts[idx] as usize,
                    video_properties: video_properties[idx],
                    query_result_status_properties: query_result_status_properties[idx],
                }),
                encode: encode_queue_idx.map(|idx| QueueIndex {
                    family_index: idx,
                    queue_count: queue_counts[idx] as usize,
                    video_properties: video_properties[idx],
                    query_result_status_properties: query_result_status_properties[idx],
                }),
                graphics_transfer_compute: QueueIndex {
                    family_index: graphics_transfer_compute_queue_idx,
                    queue_count: 1, // Currently we can only handle 1 queue
                    video_properties: video_properties[graphics_transfer_compute_queue_idx],
                    query_result_status_properties: query_result_status_properties
                        [graphics_transfer_compute_queue_idx],
                },
            },
            decode_capabilities: if supports_any_decoding {
                Some(decode_capabilities)
            } else {
                None
            },
            encode_capabilities: if supports_any_encoding {
                Some(encode_capabilities)
            } else {
                None
            },
            info,
        })
    }

    pub fn supports_decoding(&self) -> bool {
        self.info.supports_decoding
    }

    pub fn supports_encoding(&self) -> bool {
        self.info.supports_encoding
    }

    #[cfg(feature = "wgpu")]
    pub fn supports_surface(&self, surface: &wgpu::Surface<'_>) -> bool {
        unsafe {
            surface
                .as_hal::<VkApi>()
                .and_then(|surface| {
                    self.wgpu_adapter
                        .adapter
                        .surface_capabilities(&surface as &wgpu::hal::vulkan::Surface)
                })
                .is_some()
        }
    }

    pub fn create_device(
        self,
        descriptor: &VulkanDeviceDescriptor,
    ) -> Result<Arc<VulkanDevice>, VulkanInitError> {
        Ok(VulkanDevice::new(self.instance, self, descriptor)?.into())
    }

    pub fn info(&self) -> &AdapterInfo {
        &self.info
    }
}

// TODO: maybe there should be a way of specifying power preference / device preference (like wgpu)
/// Describes a [`VulkanAdapter`].
/// Used by [`VulkanInstance::create_adapter`]
#[cfg(feature = "wgpu")]
pub struct VulkanAdapterDescriptor<'a> {
    pub supports_decoding: bool,
    pub supports_encoding: bool,
    pub compatible_surface: Option<&'a wgpu::Surface<'a>>,
}

#[cfg(not(feature = "wgpu"))]
pub struct VulkanAdapterDescriptor {
    pub supports_decoding: bool,
    pub supports_encoding: bool,
}

#[cfg(feature = "wgpu")]
impl Default for VulkanAdapterDescriptor<'_> {
    fn default() -> Self {
        Self {
            supports_decoding: true,
            supports_encoding: true,
            compatible_surface: None,
        }
    }
}

#[cfg(not(feature = "wgpu"))]
impl Default for VulkanAdapterDescriptor {
    fn default() -> Self {
        Self {
            supports_decoding: true,
            supports_encoding: true,
        }
    }
}

pub struct AdapterInfo {
    pub name: String,
    pub driver_name: String,
    pub driver_info: String,
    pub device_type: vk::PhysicalDeviceType,
    pub supports_decoding: bool,
    pub supports_encoding: bool,
    pub device_properties: vk::PhysicalDeviceProperties,
    pub decode_capabilities: DecodeCapabilities,
    pub encode_capabilities: EncodeCapabilities,
}

impl Debug for AdapterInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        let version = {
            let version = self.device_properties.api_version;
            let major = vk::api_version_major(version);
            let minor = vk::api_version_minor(version);
            let patch = vk::api_version_patch(version);

            format!("{major}.{minor}.{patch}")
        };
        f.debug_struct("AdapterInfo")
            .field("name", &self.name)
            .field("device_type", &self.device_type)
            .field("api_version", &version)
            .field("driver", &self.driver_name)
            .field("driver_info", &self.driver_info)
            .field("vendor", &self.device_properties.vendor_id)
            .field("device", &self.device_properties.device_id)
            .field("supports_decoding", &self.supports_decoding)
            .field("supports_encoding", &self.supports_encoding)
            .finish()
    }
}

/// This macro will iterate over the `p_next` chain of the base struct until it finds a struct,
/// which matches the given type. After that it will execute the given action on the found struct.
///
/// # Example
/// ```ignore
/// unsafe {
///     find_ext!(queue_family_properties, found_extension @ ash::vk::QueueFamilyVideoPropertiesKHR => {
///         dbg!(found_extension)
///     });
/// }
/// ```
#[cfg_attr(doctest, macro_export)]
macro_rules! find_ext {
    ($base:expr, $var:ident @ $ext:ty => $action:stmt) => {
        let mut next = $base.p_next.cast::<ash::vk::BaseOutStructure>();
        while !next.is_null() {
            ash::match_out_struct!(match next {
                $var @ $ext => {
                    $action
                    break;
                }
            });

            next = (*next).p_next;
        }
    };
}

pub(crate) fn iter_adapters<'a>(
    vulkan_instance: &'a VulkanInstance,
) -> Result<impl Iterator<Item = VulkanAdapter<'a>> + 'a, VulkanInitError> {
    let physical_devices = unsafe { vulkan_instance.instance.enumerate_physical_devices()? };
    Ok(physical_devices
        .into_iter()
        .filter_map(move |device| VulkanAdapter::new(vulkan_instance, device)))
}

/// Returns the list of missing extensions
fn check_extensions<'a>(
    required_extensions: &'a [&'a CStr],
    available_extensions: &'a [vk::ExtensionProperties],
) -> Result<(), Vec<&'a CStr>> {
    let missing = required_extensions
        .iter()
        .copied()
        .filter(|&required_name| {
            !available_extensions.iter().any(|ext| {
                let Ok(name) = ext.extension_name_as_c_str() else {
                    return false;
                };

                name == required_name
            })
        })
        .collect::<Vec<_>>();

    if !missing.is_empty() {
        return Err(missing);
    }

    Ok(())
}

fn supported_extensions<'a>(
    required_extensions: &'a [&'a CStr],
    available_extensions: &'a [vk::ExtensionProperties],
) -> Vec<&'a CStr> {
    required_extensions
        .iter()
        .copied()
        .filter(|&required_name| {
            available_extensions.iter().any(|ext| {
                let Ok(name) = ext.extension_name_as_c_str() else {
                    return false;
                };

                name == required_name
            })
        })
        .collect()
}

fn extensions_to_codec_operations(extensions: &[&CStr]) -> vk::VideoCodecOperationFlagsKHR {
    extensions
        .iter()
        .copied()
        .fold(vk::VideoCodecOperationFlagsKHR::empty(), |acc, ext| {
            acc | match ext {
                name if name == vk::KHR_VIDEO_ENCODE_H264_NAME => {
                    vk::VideoCodecOperationFlagsKHR::ENCODE_H264
                }
                name if name == vk::KHR_VIDEO_ENCODE_H265_NAME => {
                    vk::VideoCodecOperationFlagsKHR::ENCODE_H265
                }
                name if name == vk::KHR_VIDEO_DECODE_H264_NAME => {
                    vk::VideoCodecOperationFlagsKHR::DECODE_H264
                }
                name if name == vk::KHR_VIDEO_DECODE_H265_NAME => {
                    vk::VideoCodecOperationFlagsKHR::DECODE_H265
                }
                _ => vk::VideoCodecOperationFlagsKHR::empty(),
            }
        })
}

fn find_video_queue_idx(
    queues: &[vk::QueueFamilyProperties2<'_>],
    queue_flag: vk::QueueFlags,
    video_codec_operation: vk::VideoCodecOperationFlagsKHR,
) -> Option<usize> {
    for (i, queue) in queues.iter().enumerate() {
        if !queue
            .queue_family_properties
            .queue_flags
            .contains(queue_flag)
        {
            continue;
        }

        unsafe {
            find_ext!(queue, video_properties @ vk::QueueFamilyVideoPropertiesKHR =>
                if video_properties
                    .video_codec_operations
                    .contains(video_codec_operation)
                {
                    return Some(i);
                }
            );
        }
    }

    None
}

#[cfg(feature = "wgpu")]
fn expose_wgpu_adapter(
    vulkan_instance: &VulkanInstance,
    device: vk::PhysicalDevice,
) -> Option<wgpu::hal::ExposedAdapter<VkApi>> {
    let wgpu_instance = &vulkan_instance.wgpu_instance;
    let wgpu_instance = unsafe { wgpu_instance.as_hal::<VkApi>() }.unwrap();
    wgpu_instance.expose_adapter(device)
}
