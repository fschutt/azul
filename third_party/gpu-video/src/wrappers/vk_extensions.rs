use ash::{RawPtr, prelude::VkResult, vk};

pub(crate) trait VideoQueueExt {
    unsafe fn cmd_begin_video_coding_khr(
        &self,
        command_buffer: vk::CommandBuffer,
        begin_info: &vk::VideoBeginCodingInfoKHR,
    );

    unsafe fn cmd_end_video_coding_khr(
        &self,
        command_buffer: vk::CommandBuffer,
        end_info: &vk::VideoEndCodingInfoKHR,
    );

    unsafe fn cmd_control_video_coding_khr(
        &self,
        command_buffer: vk::CommandBuffer,
        control_info: &vk::VideoCodingControlInfoKHR,
    );

    unsafe fn get_video_session_memory_requirements_khr(
        &self,
        video_session: vk::VideoSessionKHR,
    ) -> VkResult<Vec<vk::VideoSessionMemoryRequirementsKHR<'_>>>;

    unsafe fn create_video_session_khr(
        &self,
        create_info: &vk::VideoSessionCreateInfoKHR,
        allocation_callbacks: Option<&vk::AllocationCallbacks>,
    ) -> VkResult<vk::VideoSessionKHR>;

    unsafe fn bind_video_session_memory_khr(
        &self,
        video_session: vk::VideoSessionKHR,
        memory_bind_infos: &[vk::BindVideoSessionMemoryInfoKHR],
    ) -> VkResult<()>;

    unsafe fn destroy_video_session_khr(
        &self,
        video_session: vk::VideoSessionKHR,
        allocation_callbacks: Option<&vk::AllocationCallbacks>,
    );

    unsafe fn create_video_session_parameters_khr(
        &self,
        create_info: &vk::VideoSessionParametersCreateInfoKHR,
        allocation_callbacks: Option<&vk::AllocationCallbacks>,
    ) -> VkResult<vk::VideoSessionParametersKHR>;

    unsafe fn destroy_video_session_parameters_khr(
        &self,
        parameters: vk::VideoSessionParametersKHR,
        allocation_callbacks: Option<&vk::AllocationCallbacks>,
    );

    unsafe fn update_video_session_parameters_khr(
        &self,
        parameters: vk::VideoSessionParametersKHR,
        update_info: &vk::VideoSessionParametersUpdateInfoKHR,
    ) -> VkResult<()>;
}

impl VideoQueueExt for ash::khr::video_queue::Device {
    unsafe fn cmd_begin_video_coding_khr(
        &self,
        command_buffer: vk::CommandBuffer,
        begin_info: &vk::VideoBeginCodingInfoKHR,
    ) {
        unsafe { (self.fp().cmd_begin_video_coding_khr)(command_buffer, begin_info) }
    }

    unsafe fn cmd_end_video_coding_khr(
        &self,
        command_buffer: vk::CommandBuffer,
        end_info: &vk::VideoEndCodingInfoKHR,
    ) {
        unsafe { (self.fp().cmd_end_video_coding_khr)(command_buffer, end_info) }
    }

    unsafe fn cmd_control_video_coding_khr(
        &self,
        command_buffer: vk::CommandBuffer,
        control_info: &vk::VideoCodingControlInfoKHR,
    ) {
        unsafe { (self.fp().cmd_control_video_coding_khr)(command_buffer, control_info) }
    }

    unsafe fn get_video_session_memory_requirements_khr(
        &self,
        video_session: vk::VideoSessionKHR,
    ) -> VkResult<Vec<vk::VideoSessionMemoryRequirementsKHR<'_>>> {
        let mut memory_requirements_len = 0;
        unsafe {
            (self.fp().get_video_session_memory_requirements_khr)(
                self.device(),
                video_session,
                &mut memory_requirements_len,
                std::ptr::null_mut(),
            )
            .result()?;
        }

        let mut memory_requirements = vec![
            vk::VideoSessionMemoryRequirementsKHR::default();
            memory_requirements_len as usize
        ];

        unsafe {
            (self.fp().get_video_session_memory_requirements_khr)(
                self.device(),
                video_session,
                &mut memory_requirements_len,
                memory_requirements.as_mut_ptr(),
            )
            .result_with_success(memory_requirements)
        }
    }

    unsafe fn create_video_session_khr(
        &self,
        create_info: &vk::VideoSessionCreateInfoKHR,
        allocation_callbacks: Option<&vk::AllocationCallbacks>,
    ) -> VkResult<vk::VideoSessionKHR> {
        let mut video_session = vk::VideoSessionKHR::default();

        unsafe {
            (self.fp().create_video_session_khr)(
                self.device(),
                create_info,
                allocation_callbacks.as_raw_ptr(),
                &mut video_session,
            )
            .result_with_success(video_session)
        }
    }

    unsafe fn bind_video_session_memory_khr(
        &self,
        video_session: vk::VideoSessionKHR,
        memory_bind_infos: &[vk::BindVideoSessionMemoryInfoKHR],
    ) -> VkResult<()> {
        unsafe {
            (self.fp().bind_video_session_memory_khr)(
                self.device(),
                video_session,
                memory_bind_infos.len() as u32,
                memory_bind_infos.as_ptr(),
            )
            .result()
        }
    }

    unsafe fn destroy_video_session_khr(
        &self,
        video_session: vk::VideoSessionKHR,
        allocation_callbacks: Option<&vk::AllocationCallbacks>,
    ) {
        unsafe {
            (self.fp().destroy_video_session_khr)(
                self.device(),
                video_session,
                allocation_callbacks.as_raw_ptr(),
            )
        }
    }

    unsafe fn create_video_session_parameters_khr(
        &self,
        create_info: &vk::VideoSessionParametersCreateInfoKHR,
        allocation_callbacks: Option<&vk::AllocationCallbacks>,
    ) -> VkResult<vk::VideoSessionParametersKHR> {
        let mut parameters = vk::VideoSessionParametersKHR::default();

        unsafe {
            (self.fp().create_video_session_parameters_khr)(
                self.device(),
                create_info,
                allocation_callbacks.as_raw_ptr(),
                &mut parameters,
            )
            .result_with_success(parameters)
        }
    }

    unsafe fn destroy_video_session_parameters_khr(
        &self,
        parameters: vk::VideoSessionParametersKHR,
        allocation_callbacks: Option<&vk::AllocationCallbacks>,
    ) {
        unsafe {
            (self.fp().destroy_video_session_parameters_khr)(
                self.device(),
                parameters,
                allocation_callbacks.as_raw_ptr(),
            )
        }
    }

    unsafe fn update_video_session_parameters_khr(
        &self,
        parameters: vk::VideoSessionParametersKHR,
        update_info: &vk::VideoSessionParametersUpdateInfoKHR,
    ) -> VkResult<()> {
        unsafe {
            (self.fp().update_video_session_parameters_khr)(self.device(), parameters, update_info)
                .result()
        }
    }
}

pub(crate) trait VideoDecodeQueueExt {
    unsafe fn cmd_decode_video_khr(
        &self,
        command_buffer: vk::CommandBuffer,
        decode_info: &vk::VideoDecodeInfoKHR,
    );
}

impl VideoDecodeQueueExt for ash::khr::video_decode_queue::Device {
    unsafe fn cmd_decode_video_khr(
        &self,
        command_buffer: vk::CommandBuffer,
        decode_info: &vk::VideoDecodeInfoKHR,
    ) {
        unsafe { (self.fp().cmd_decode_video_khr)(command_buffer, decode_info) }
    }
}

pub(crate) trait VideoEncodeQueueExt {
    unsafe fn get_encoded_video_session_parameters_khr(
        &self,
        video_session_parameters_info: &vk::VideoEncodeSessionParametersGetInfoKHR,
        feedback_info: Option<&mut vk::VideoEncodeSessionParametersFeedbackInfoKHR>,
    ) -> VkResult<Vec<u8>>;

    unsafe fn cmd_encode_video_khr(
        &self,
        command_buffer: vk::CommandBuffer,
        encode_info: &vk::VideoEncodeInfoKHR,
    );
}

impl VideoEncodeQueueExt for ash::khr::video_encode_queue::Device {
    unsafe fn get_encoded_video_session_parameters_khr(
        &self,
        video_session_parameters_info: &vk::VideoEncodeSessionParametersGetInfoKHR,
        feedback_info: Option<&mut vk::VideoEncodeSessionParametersFeedbackInfoKHR>,
    ) -> VkResult<Vec<u8>> {
        let feedback_info = match feedback_info {
            Some(f) => f as *mut _,
            None => std::ptr::null_mut(),
        };

        let mut len = 0;

        unsafe {
            (self.fp().get_encoded_video_session_parameters_khr)(
                self.device(),
                video_session_parameters_info,
                feedback_info,
                &mut len,
                std::ptr::null_mut(),
            )
            .result()?;
        }

        let mut data = vec![0u8; len];

        unsafe {
            (self.fp().get_encoded_video_session_parameters_khr)(
                self.device(),
                video_session_parameters_info,
                feedback_info,
                &mut len,
                data.as_mut_ptr() as *mut _,
            )
            .result_with_success(data)
        }
    }

    unsafe fn cmd_encode_video_khr(
        &self,
        command_buffer: vk::CommandBuffer,
        encode_info: &vk::VideoEncodeInfoKHR,
    ) {
        unsafe { (self.fp().cmd_encode_video_khr)(command_buffer, encode_info) }
    }
}
