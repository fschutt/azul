use std::{ffi::c_void, sync::Arc};

use ash::vk::{self, QueryType};
use tracing::{error, info, trace, warn};

use crate::{VulkanCommonError, VulkanDecoderError, VulkanInitError};

use super::{Device, Instance};

pub(crate) struct DebugMessenger {
    messenger: vk::DebugUtilsMessengerEXT,
    instance: Arc<Instance>,
}

impl DebugMessenger {
    pub(crate) fn new(instance: Arc<Instance>) -> Result<Self, VulkanInitError> {
        let debug_messenger_create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                    | vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(debug_messenger_callback));

        let messenger = unsafe {
            instance
                .debug_utils_instance_ext
                .create_debug_utils_messenger(&debug_messenger_create_info, None)?
        };

        Ok(Self {
            instance,
            messenger,
        })
    }
}

impl Drop for DebugMessenger {
    fn drop(&mut self) {
        unsafe {
            self.instance
                .debug_utils_instance_ext
                .destroy_debug_utils_messenger(self.messenger, None)
        };
    }
}

unsafe extern "system" fn debug_messenger_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_types: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _p_user_data: *mut c_void,
) -> vk::Bool32 {
    let callback_data = unsafe { *p_callback_data };

    // FIXME: This is a bug in wgpu:
    // https://github.com/gfx-rs/wgpu/issues/7696
    // Until it's fixed upstream, let's silence `VUID-StandaloneSpirv-None-10684`.
    if callback_data.message_id_number == 0xb210f7c2u32 as i32 {
        return vk::FALSE;
    }

    // This is an error about creating an image for video coding that has usage flags not
    // advertised as supported by the GPU. We use this extensively on Nvidia and it works fine.
    // Thread on Nvidia developer forum: https://forums.developer.nvidia.com/t/vkimagecreateflags-and-vulkan-encode/284369
    // The VUID for this message is `VUID-VkImageCreateInfo-pNext-06811`.
    if callback_data.message_id_number == 0x30f4ac70u32 as i32 {
        return vk::FALSE;
    }

    let message_id = unsafe {
        callback_data
            .message_id_name_as_c_str()
            .unwrap_or(c"")
            .to_string_lossy()
    };

    let message = unsafe {
        callback_data
            .message_as_c_str()
            .unwrap_or(c"")
            .to_string_lossy()
    };

    let t = format!("{message_types:?}");
    match message_severity {
        vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE => {
            trace!("[{t}][{message_id}] {message}");
        }

        vk::DebugUtilsMessageSeverityFlagsEXT::INFO => {
            info!("[{t}][{message_id}] {message}");
        }

        vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => {
            warn!("[{t}][{message_id}] {message}");
        }

        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => {
            error!("[{t}][{message_id}] {message}");
        }
        _ => {}
    }

    vk::FALSE
}

pub(crate) struct DecodingQueryPool {
    pool: QueryPool,
}

impl std::ops::Deref for DecodingQueryPool {
    type Target = QueryPool;

    fn deref(&self) -> &Self::Target {
        &self.pool
    }
}

impl DecodingQueryPool {
    pub(crate) fn new(
        device: Arc<Device>,
        profile: vk::VideoProfileInfoKHR,
    ) -> Result<Self, VulkanDecoderError> {
        let pool = QueryPool::new(
            device,
            QueryType::RESULT_STATUS_ONLY_KHR,
            1,
            Some(profile),
            None::<vk::VideoProfileInfoKHR>, // ugh.....
        )?;
        Ok(Self { pool })
    }

    #[allow(dead_code)]
    pub(crate) fn get_result_blocking(
        &self,
    ) -> Result<vk::QueryResultStatusKHR, VulkanDecoderError> {
        let mut result = vk::QueryResultStatusKHR::NOT_READY;
        unsafe {
            self.pool.device.get_query_pool_results(
                self.pool.pool,
                0,
                std::slice::from_mut(&mut result),
                vk::QueryResultFlags::WAIT | vk::QueryResultFlags::WITH_STATUS_KHR,
            )?
        };

        Ok(result)
    }

    pub(crate) fn check_results_blocking(&self) -> Result<(), VulkanDecoderError> {
        let result = self.get_result_blocking()?;
        if result.as_raw() < 0 {
            return Err(VulkanDecoderError::DecodeOperationFailed(result));
        }

        Ok(())
    }
}

pub(crate) struct QueryPool {
    pub(crate) pool: vk::QueryPool,
    pub(crate) device: Arc<Device>,
}

impl QueryPool {
    pub(crate) fn new<T: vk::ExtendsQueryPoolCreateInfo>(
        device: Arc<Device>,
        ty: vk::QueryType,
        count: u32,
        mut profile: Option<vk::VideoProfileInfoKHR>,
        mut p_next: Option<T>,
    ) -> Result<Self, VulkanCommonError> {
        let mut create_info = vk::QueryPoolCreateInfo::default()
            .query_type(ty)
            .query_count(count);

        if let Some(profile) = profile.as_mut() {
            create_info = create_info.push_next(profile);
        }

        if let Some(p_next) = p_next.as_mut() {
            create_info = create_info.push_next(p_next);
        }
        let pool = unsafe { device.create_query_pool(&create_info, None)? };

        Ok(Self { pool, device })
    }

    pub(crate) fn reset(&self, buffer: vk::CommandBuffer) {
        unsafe { self.device.cmd_reset_query_pool(buffer, self.pool, 0, 1) };
    }

    // if we want to switch to inline queries we can use this, but we need to check how many
    // implementations support them
    pub(crate) fn _inline_query(&self) -> vk::VideoInlineQueryInfoKHR<'_> {
        vk::VideoInlineQueryInfoKHR::default()
            .query_pool(self.pool)
            .first_query(0)
            .query_count(1)
    }

    pub(crate) fn begin_query(&self, buffer: vk::CommandBuffer) {
        unsafe {
            self.device
                .cmd_begin_query(buffer, self.pool, 0, vk::QueryControlFlags::empty())
        }
    }

    pub(crate) fn end_query(&self, buffer: vk::CommandBuffer) {
        unsafe { self.device.cmd_end_query(buffer, self.pool, 0) }
    }
}

impl Drop for QueryPool {
    fn drop(&mut self) {
        unsafe { self.device.destroy_query_pool(self.pool, None) };
    }
}
