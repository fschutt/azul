use std::{collections::HashMap, sync::Arc};

use ash::vk;
use h264_reader::nal::{
    pps::PicParameterSet,
    sps::{Profile, SeqParameterSet},
};
use images::DecodingImages;
use parameters::{SessionParams, VideoSessionParametersManager};
use rustc_hash::FxHashMap;

use crate::{
    VulkanDecoderError,
    codec::h264::parameters::{
        H264DecodeProfileInfo, SeqParameterSetExt as _, h264_level_idc_to_max_dpb_mbs,
        vk_to_h264_level_idc,
    },
    device::DecodingDevice,
    vulkan_decoder::{DecoderTracker, DecoderTrackerWaitState, ImageModifiers},
    wrappers::{DecodeInputBufferPool, DecodingQueryPool, OpenCommandBuffer, VideoSession},
};

mod images;
mod parameters;

pub(super) struct VideoSessionResources<'a> {
    pub(crate) video_session: Arc<VideoSession>,
    pub(crate) parameters: SessionParams<'a>,
    pub(crate) parameters_manager: VideoSessionParametersManager,
    pub(crate) decoding_images: DecodingImages<'a>,
    pub(crate) sps: FxHashMap<u8, SeqParameterSet>,
    pub(crate) pps: FxHashMap<(u8, u8), PicParameterSet>,
    pub(crate) decode_query_pool: Option<Arc<DecodingQueryPool>>,
    pub(crate) decode_buffer_pool: DecodeInputBufferPool<'a>,
    parameters_scheduled_for_reset: Option<SessionParams<'a>>,
    image_modifiers: ImageModifiers,
}

fn calculate_max_num_reorder_frames(sps: &SeqParameterSet) -> Result<u64, VulkanDecoderError> {
    let fallback_max_num_reorder_frames = if [44u8, 86, 100, 110, 122, 244]
        .contains(&sps.profile_idc.into())
        && sps.constraint_flags.flag3()
    {
        0
    } else if let Profile::Baseline = sps.profile() {
        0
    } else {
        h264_level_idc_to_max_dpb_mbs(sps.level_idc)?
            / ((sps.pic_width_in_mbs_minus1 as u64 + 1)
                * (sps.pic_height_in_map_units_minus1 as u64 + 1))
    };

    let max_num_reorder_frames = sps
        .vui_parameters
        .as_ref()
        .and_then(|v| v.bitstream_restrictions.as_ref())
        .map(|b| b.max_num_reorder_frames as u64)
        .unwrap_or(fallback_max_num_reorder_frames)
        .min(16);

    Ok(max_num_reorder_frames)
}

impl<'a> VideoSessionResources<'a> {
    pub(crate) fn new_from_sps(
        decoding_device: &DecodingDevice,
        decode_buffer: OpenCommandBuffer,
        sps: SeqParameterSet,
        usage_info: vk::VideoDecodeUsageInfoKHR<'a>,
        tracker: &mut DecoderTracker,
        image_modifiers: ImageModifiers,
    ) -> Result<Self, VulkanDecoderError> {
        let profile_info = Arc::new(H264DecodeProfileInfo::from_sps_decode(&sps, usage_info)?);

        let level_idc = sps.level_idc;
        let max_level_idc = vk_to_h264_level_idc(
            decoding_device
                .profile_capabilities
                .codec_decode_capabilities
                .max_level_idc,
        )?;

        if level_idc > max_level_idc {
            return Err(VulkanDecoderError::InvalidInputData(format!(
                "stream has level_idc = {level_idc}, while the GPU can decode at most {max_level_idc}"
            )));
        }

        let max_coded_extent = sps.size()?;
        // +1 for current frame
        let max_dpb_slots = sps.max_num_ref_frames + 1;
        let max_active_references = sps.max_num_ref_frames;
        let max_num_reorder_frames = calculate_max_num_reorder_frames(&sps)?;

        let video_session = Arc::new(VideoSession::new(
            &decoding_device.vulkan_device,
            &decoding_device.h264_decode_queues,
            &profile_info.profile_info.profile_info,
            max_coded_extent,
            max_dpb_slots,
            max_active_references,
            vk::VideoSessionCreateFlagsKHR::empty(),
            &decoding_device
                .profile_capabilities
                .video_capabilities
                .std_header_version,
        )?);

        let mut parameters_manager =
            VideoSessionParametersManager::new(decoding_device, video_session.session)?;

        parameters_manager.put_sps(&sps)?;

        let decoding_images = Self::new_decoding_images(
            decoding_device,
            &profile_info,
            max_coded_extent,
            max_dpb_slots,
            decode_buffer,
            tracker,
            image_modifiers,
        )?;

        let sps = HashMap::from_iter([(sps.id().id(), sps)]);
        let decode_query_pool = if decoding_device
            .h264_decode_queues
            .supports_result_status_queries()
        {
            Some(Arc::new(DecodingQueryPool::new(
                decoding_device.vulkan_device.device.clone(),
                profile_info.profile_info.profile_info,
            )?))
        } else {
            None
        };

        let decode_buffer_pool =
            DecodeInputBufferPool::new(decoding_device.allocator.clone(), profile_info.clone());

        let parameters = SessionParams {
            max_coded_extent,
            max_dpb_slots,
            max_active_references,
            max_num_reorder_frames,
            profile_info,
            level_idc,
        };

        Ok(VideoSessionResources {
            parameters,
            video_session,
            parameters_manager,
            decoding_images,
            sps,
            pps: FxHashMap::default(),
            decode_query_pool,
            decode_buffer_pool,
            parameters_scheduled_for_reset: None,
            image_modifiers,
        })
    }

    pub(crate) fn process_sps(
        &mut self,
        sps: SeqParameterSet,
        usage_info: vk::VideoDecodeUsageInfoKHR<'a>,
    ) -> Result<(), VulkanDecoderError> {
        let new_session_params = SessionParams {
            max_coded_extent: sps.size()?,
            max_dpb_slots: sps.max_num_ref_frames + 1, // +1 for current frame
            max_active_references: sps.max_num_ref_frames,
            max_num_reorder_frames: calculate_max_num_reorder_frames(&sps)?,
            profile_info: Arc::new(H264DecodeProfileInfo::from_sps_decode(&sps, usage_info)?),
            level_idc: sps.level_idc,
        };
        let current_session_params = self
            .parameters_scheduled_for_reset
            .take()
            .unwrap_or_else(|| self.parameters.clone());

        self.parameters_scheduled_for_reset = Some(SessionParams::combine(
            current_session_params,
            new_session_params,
        ));

        self.parameters_manager.put_sps(&sps)?;
        self.sps.insert(sps.id().id(), sps);

        Ok(())
    }

    pub(crate) fn process_pps(&mut self, pps: PicParameterSet) -> Result<(), VulkanDecoderError> {
        self.parameters_manager.put_pps(&pps)?;
        self.pps.insert(
            (pps.seq_parameter_set_id.id(), pps.pic_parameter_set_id.id()),
            pps,
        );
        Ok(())
    }

    pub(crate) fn ensure_session(
        &mut self,
        decoding_device: &DecodingDevice,
        decode_buffer: OpenCommandBuffer,
        tracker: &mut DecoderTracker,
    ) -> Result<(), VulkanDecoderError> {
        let Some(new_params) = self.parameters_scheduled_for_reset.take() else {
            return Ok(());
        };

        if self.parameters.is_valid(&new_params) {
            // no need to change the session
            self.parameters.max_num_reorder_frames = new_params.max_num_reorder_frames;
            return Ok(());
        }

        let max_level_idc = vk_to_h264_level_idc(
            decoding_device
                .profile_capabilities
                .codec_decode_capabilities
                .max_level_idc,
        )?;

        if new_params.level_idc > max_level_idc {
            return Err(VulkanDecoderError::InvalidInputData(format!(
                "stream has level_idc = {}, while the GPU can decode at most {}",
                new_params.level_idc, max_level_idc
            )));
        }

        if self.parameters.profile_info != new_params.profile_info {
            self.decode_query_pool = match decoding_device
                .h264_decode_queues
                .supports_result_status_queries()
            {
                true => Some(Arc::new(DecodingQueryPool::new(
                    decoding_device.vulkan_device.device.clone(),
                    new_params.profile_info.profile_info.profile_info,
                )?)),
                false => None,
            };
            self.decode_buffer_pool = DecodeInputBufferPool::new(
                decoding_device.allocator.clone(),
                new_params.profile_info.clone(),
            );
        }

        self.video_session = Arc::new(VideoSession::new(
            &decoding_device.vulkan_device,
            &decoding_device.h264_decode_queues,
            &new_params.profile_info.profile_info.profile_info,
            new_params.max_coded_extent,
            new_params.max_dpb_slots,
            new_params.max_active_references,
            vk::VideoSessionCreateFlagsKHR::empty(),
            &decoding_device
                .profile_capabilities
                .video_capabilities
                .std_header_version,
        )?);

        self.parameters_manager
            .change_session(self.video_session.session)?;

        self.decoding_images = Self::new_decoding_images(
            decoding_device,
            &new_params.profile_info,
            self.video_session.max_coded_extent,
            self.video_session.max_dpb_slots,
            decode_buffer,
            tracker,
            self.image_modifiers,
        )?;

        self.parameters = new_params;

        Ok(())
    }

    /// Creates a new buffer of reference images used for decoding.
    ///
    /// If you're replacing existing decoding images, make sure the old references won't be used,
    /// e.g. do it right before decoding IDR. Otherwise it may result in error because the decoder
    /// could try to use references which no longer exist if there's a non IDR frame after SPS.
    fn new_decoding_images(
        decoding_device: &DecodingDevice,
        profile: &H264DecodeProfileInfo,
        max_coded_extent: vk::Extent2D,
        max_dpb_slots: u32,
        mut decode_buffer: OpenCommandBuffer,
        tracker: &mut DecoderTracker,
        image_modifiers: ImageModifiers,
    ) -> Result<DecodingImages<'a>, VulkanDecoderError> {
        let mut dpb_format = decoding_device.profile_capabilities.dpb_format_properties;
        // image modifiers are only applied to the output picture, which is the dst_image if it
        // exists, dpb otherwise
        if decoding_device
            .profile_capabilities
            .dst_format_properties
            .is_none()
        {
            dpb_format.image_create_flags |= image_modifiers.create_flags;
            dpb_format.image_usage_flags |= image_modifiers.usage_flags;
        }
        let dst_format = decoding_device
            .profile_capabilities
            .dst_format_properties
            .map(|p| {
                p.image_create_flags(p.image_create_flags | image_modifiers.create_flags)
                    .image_usage_flags(p.image_usage_flags | image_modifiers.usage_flags)
            });

        let decoding_images = DecodingImages::new(
            decoding_device,
            &mut decode_buffer,
            tracker.image_layout_tracker.clone(),
            profile,
            &dpb_format,
            &dst_format,
            max_coded_extent,
            max_dpb_slots,
            image_modifiers.additional_queue_index as u32,
        )?;

        decoding_device.h264_decode_queues.submit_chain_semaphore(
            decode_buffer.end()?,
            tracker,
            vk::PipelineStageFlags2::ALL_COMMANDS,
            vk::PipelineStageFlags2::ALL_COMMANDS,
            DecoderTrackerWaitState::NewDecodingImagesLayoutTransition,
        )?;

        Ok(decoding_images)
    }

    pub(crate) fn free_reference_picture(&mut self, i: usize) {
        self.decoding_images.free_reference_picture(i);
    }
}
