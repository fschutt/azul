use std::{ptr::NonNull, sync::Arc};

use ash::{Entry, vk};

mod command;
mod debug;
mod mem;
#[cfg(feature = "transcoder")]
mod pipeline;
mod sync;
mod video;
mod vk_extensions;

pub(crate) use command::*;
pub(crate) use debug::*;
pub(crate) use mem::*;
#[cfg(feature = "transcoder")]
pub(crate) use pipeline::*;
pub(crate) use sync::*;
pub(crate) use video::*;
pub(crate) use vk_extensions::*;

use crate::VulkanCommonError;

pub(crate) struct Instance {
    pub(crate) instance: ash::Instance,
    pub(crate) _entry: Arc<Entry>,
    pub(crate) video_queue_instance_ext: ash::khr::video_queue::Instance,
    pub(crate) video_encode_queue_instance_ext: ash::khr::video_encode_queue::Instance,
    pub(crate) debug_utils_instance_ext: ash::ext::debug_utils::Instance,
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe { self.destroy_instance(None) };
    }
}

impl std::ops::Deref for Instance {
    type Target = ash::Instance;

    fn deref(&self) -> &Self::Target {
        &self.instance
    }
}

pub(crate) struct Device {
    pub(crate) device: ash::Device,
    pub(crate) video_queue_ext: ash::khr::video_queue::Device,
    pub(crate) video_decode_queue_ext: ash::khr::video_decode_queue::Device,
    pub(crate) video_encode_queue_ext: ash::khr::video_encode_queue::Device,
    #[cfg(feature = "vk-validation")]
    pub(crate) debug_utils_ext: ash::ext::debug_utils::Device,
    pub(crate) _instance: Arc<Instance>,
}

impl Device {
    #[cfg(feature = "vk-validation")]
    pub(crate) fn set_label<T: vk::Handle>(
        &self,
        object: T,
        label: Option<&str>,
    ) -> Result<(), VulkanCommonError> {
        use std::ffi::CStr;

        if let Some(label) = label {
            let mut text = [0; 64];
            let mut long_text = Vec::new();

            let label = if label.len() < text.len() {
                text[..label.len()].copy_from_slice(label.as_bytes());
                CStr::from_bytes_until_nul(&text).unwrap()
            } else {
                long_text.extend_from_slice(label.as_bytes());
                long_text.push(0);
                CStr::from_bytes_until_nul(&long_text).unwrap()
            };

            unsafe {
                self.debug_utils_ext.set_debug_utils_object_name(
                    &vk::DebugUtilsObjectNameInfoEXT::default()
                        .object_handle(object)
                        .object_name(label),
                )?
            }
        }

        Ok(())
    }

    #[cfg(not(feature = "vk-validation"))]
    pub(crate) fn set_label<T: vk::Handle>(
        &self,
        _object: T,
        _label: Option<&str>,
    ) -> Result<(), VulkanCommonError> {
        Ok(())
    }
}

impl std::ops::Deref for Device {
    type Target = ash::Device;

    fn deref(&self) -> &Self::Target {
        &self.device
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { self.destroy_device(None) };
    }
}

unsafe impl<'a> Send for ProfileInfo<'a> {}
unsafe impl<'a> Sync for ProfileInfo<'a> {}

pub(crate) struct ProfileInfo<'a> {
    pub(crate) profile_info: vk::VideoProfileInfoKHR<'a>,
    additional_infos_ptr: Vec<NonNull<dyn vk::ExtendsVideoProfileInfoKHR + Send + Sync + 'a>>,
}

impl<'a> ProfileInfo<'a> {
    pub(crate) fn new(
        mut profile_info: vk::VideoProfileInfoKHR<'a>,
        additional_info: Vec<Box<dyn vk::ExtendsVideoProfileInfoKHR + Send + Sync + 'a>>,
    ) -> Self {
        let (refs, ptrs) = additional_info
            .into_iter()
            .map(|i| {
                let r = Box::leak(i);
                let p = NonNull::from(&mut *r);
                (r, p)
            })
            .unzip::<_, _, Vec<_>, Vec<_>>();

        for r in refs {
            profile_info = profile_info.push_next(r);
        }

        Self {
            profile_info,
            additional_infos_ptr: ptrs,
        }
    }
}

impl Drop for ProfileInfo<'_> {
    fn drop(&mut self) {
        unsafe {
            for ptr in self.additional_infos_ptr.drain(..) {
                let _ = Box::from_raw(ptr.as_ptr());
            }
        }
    }
}
