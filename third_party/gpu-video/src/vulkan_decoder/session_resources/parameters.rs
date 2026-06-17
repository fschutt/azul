use std::{cmp, collections::HashMap, sync::Arc};

use ash::vk;
use h264_reader::nal::{pps::PicParameterSet, sps::SeqParameterSet};

use crate::{
    VulkanDecoderError, VulkanDevice,
    codec::h264::{
        H264Codec, H264VkParameters,
        parameters::{VkH264PictureParameterSet, VkH264SequenceParameterSet},
    },
    vulkan_decoder::{Device, VideoSessionParameters},
};

use super::H264DecodeProfileInfo;

/// Since `VideoSessionParameters` can only add sps and pps values (inserting sps or pps with an
/// existing id is prohibited), this is an abstraction which provides the capability to replace an
/// existing sps or pps.
pub(crate) struct VideoSessionParametersManager {
    pub(crate) parameters: Arc<VideoSessionParameters>,
    sps: HashMap<u8, VkH264SequenceParameterSet>,
    pps: HashMap<(u8, u8), VkH264PictureParameterSet>,
    device: Arc<Device>,
    session: vk::VideoSessionKHR,
    update_sequence_count: u32,
}

impl VideoSessionParametersManager {
    pub(crate) fn new(
        vulkan_ctx: &VulkanDevice,
        session: vk::VideoSessionKHR,
    ) -> Result<Self, VulkanDecoderError> {
        Ok(Self {
            parameters: Arc::new(VideoSessionParameters::new::<H264Codec>(
                vulkan_ctx.device.clone(),
                session,
                H264VkParameters {
                    sps: Vec::new(),
                    pps: Vec::new(),
                },
                None,
                None,
            )?),
            sps: HashMap::new(),
            pps: HashMap::new(),
            device: vulkan_ctx.device.clone(),
            session,
            update_sequence_count: 0,
        })
    }

    pub(crate) fn parameters(&self) -> vk::VideoSessionParametersKHR {
        self.parameters.parameters
    }

    pub(crate) fn change_session(
        &mut self,
        session: vk::VideoSessionKHR,
    ) -> Result<(), VulkanDecoderError> {
        if self.session == session {
            return Ok(());
        }
        self.session = session;

        let sps = self.sps.values().map(|sps| sps.sps).collect::<Vec<_>>();
        let pps = self.pps.values().map(|pps| pps.pps).collect::<Vec<_>>();

        self.parameters = Arc::new(VideoSessionParameters::new::<H264Codec>(
            self.device.clone(),
            session,
            H264VkParameters { sps, pps },
            None,
            None,
        )?);

        Ok(())
    }

    // it is probably not optimal to insert sps and pps searately. this could be optimized, so that
    // the insertion happens lazily when the parameters are bound to a session.
    pub(crate) fn put_sps(&mut self, sps: &SeqParameterSet) -> Result<(), VulkanDecoderError> {
        let key = sps.seq_parameter_set_id.id();
        match self.sps.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                e.insert(sps.into());
                self.recreate_parameters(vec![self.sps[&key].sps], Vec::new())
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(sps.into());
                self.add_parameters(&[self.sps[&key].sps], &[])
            }
        }
    }

    pub(crate) fn put_pps(&mut self, pps: &PicParameterSet) -> Result<(), VulkanDecoderError> {
        let key = (pps.seq_parameter_set_id.id(), pps.pic_parameter_set_id.id());
        match self.pps.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                e.insert(pps.into());
                self.recreate_parameters(Vec::new(), vec![self.pps[&key].pps])
            }

            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(pps.into());
                self.add_parameters(&[], &[self.pps[&key].pps])
            }
        }
    }

    fn add_parameters(
        &mut self,
        sps: &[vk::native::StdVideoH264SequenceParameterSet],
        pps: &[vk::native::StdVideoH264PictureParameterSet],
    ) -> Result<(), VulkanDecoderError> {
        self.update_sequence_count += 1;
        self.parameters.add(sps, pps, self.update_sequence_count)?;
        Ok(())
    }

    fn recreate_parameters(
        &mut self,
        initial_sps: Vec<vk::native::StdVideoH264SequenceParameterSet>,
        initial_pps: Vec<vk::native::StdVideoH264PictureParameterSet>,
    ) -> Result<(), VulkanDecoderError> {
        self.update_sequence_count = 0;
        self.parameters = Arc::new(VideoSessionParameters::new::<H264Codec>(
            self.device.clone(),
            self.session,
            H264VkParameters {
                sps: initial_sps,
                pps: initial_pps,
            },
            Some(&self.parameters),
            None,
        )?);
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct SessionParams<'a> {
    pub(crate) max_coded_extent: vk::Extent2D,
    pub(crate) max_dpb_slots: u32,
    pub(crate) max_active_references: u32,
    pub(crate) max_num_reorder_frames: u64,
    pub(crate) profile_info: Arc<H264DecodeProfileInfo<'a>>,
    pub(crate) level_idc: u8,
}

impl SessionParams<'_> {
    pub(crate) fn combine(current_params: Self, new_params: Self) -> Self {
        Self {
            max_coded_extent: vk::Extent2D {
                width: u32::max(
                    current_params.max_coded_extent.width,
                    new_params.max_coded_extent.width,
                ),
                height: u32::max(
                    current_params.max_coded_extent.height,
                    new_params.max_coded_extent.height,
                ),
            },
            max_dpb_slots: u32::max(current_params.max_dpb_slots, new_params.max_dpb_slots),
            max_active_references: u32::max(
                current_params.max_active_references,
                new_params.max_active_references,
            ),
            // max_num_reorder_frames has to come from the new_params
            max_num_reorder_frames: new_params.max_num_reorder_frames,
            profile_info: cmp::max_by(
                current_params.profile_info,
                new_params.profile_info,
                |p1, p2| p1.profile_idc.cmp(&p2.profile_idc),
            ),
            level_idc: u8::max(current_params.level_idc, new_params.level_idc),
        }
    }

    pub(crate) fn is_valid(&self, new_params: &Self) -> bool {
        self.max_coded_extent.width >= new_params.max_coded_extent.width
            && self.max_coded_extent.height >= new_params.max_coded_extent.height
            && self.max_dpb_slots >= new_params.max_dpb_slots
            && self.profile_info.profile_idc >= new_params.profile_info.profile_idc
    }
}
