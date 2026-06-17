use std::{collections::VecDeque, num::NonZeroU32};

use ash::vk;

use crate::{
    VulkanEncoderError,
    codec::{
        EncodeCodec,
        h264::{
            H264Codec,
            parameters::{VkH264PictureParameterSet, VkH264SequenceParameterSet},
        },
    },
    device::caps::{NativeEncodeProfileCapabilities, NativeEncodeQualityLevelProperties},
    parameters::RateControl,
    wrappers::ProfileInfo,
};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct H264EncodingCounters {
    frame_num: u32,
    pic_order_cnt: u8,
    idr_pic_id: u16,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct H264WriteParametersInfo {
    pub(crate) write_sps: bool,
    pub(crate) write_pps: bool,
}

impl EncodeCodec for H264Codec {
    fn encode_profile_capabilities(
        caps: &Self::NativeEncodeCodecCapabilities,
        profile: Self::Profile,
    ) -> Option<&NativeEncodeProfileCapabilities<Self>> {
        caps.profile(profile)
    }

    fn profile_info<'a>(
        params: &crate::vulkan_encoder::FullEncoderParameters<Self>,
    ) -> crate::wrappers::ProfileInfo<'a> {
        let h264_profile = vk::VideoEncodeH264ProfileInfoKHR::default()
            .std_profile_idc(params.profile.to_profile_idc());

        let profile = vk::VideoProfileInfoKHR::default()
            .video_codec_operation(vk::VideoCodecOperationFlagsKHR::ENCODE_H264)
            .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420)
            .luma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
            .chroma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8);

        let h264_profile = Box::new(h264_profile);

        let usage_info: vk::VideoEncodeUsageInfoKHR = params.into();

        let usage_info = Box::new(usage_info);

        ProfileInfo::new(profile, vec![h264_profile, usage_info])
    }

    fn codec_parameters(
        parameters: &crate::vulkan_encoder::FullEncoderParameters<Self>,
        codec_capabilities: &Self::CodecSpecificEncodeCapabilities<'_>,
    ) -> Result<Self::OwnedParameters, VulkanEncoderError> {
        let sps = VkH264SequenceParameterSet::new_encode(
            parameters.profile,
            parameters.width.get(),
            parameters.height.get(),
            parameters.max_references.get(),
            parameters.color_space,
            parameters.color_range,
            parameters.framerate,
        )?;
        let pps = VkH264PictureParameterSet::new_encode(codec_capabilities, parameters.profile);

        Ok(Self::OwnedParameters {
            sps: vec![sps],
            pps: vec![pps],
        })
    }

    fn vk_parameters<'a>(parameters: &'a Self::OwnedParameters) -> Self::VkParameters<'a> {
        Self::VkParameters {
            sps: parameters.sps.iter().map(|p| p.sps).collect(),
            pps: parameters.pps.iter().map(|p| p.pps).collect(),
        }
    }

    type BitstreamUnitData = vk::native::StdVideoEncodeH264SliceHeader;
    fn bitstream_unit_data(
        _codec_capabilities: &Self::CodecSpecificEncodeCapabilities<'_>,
        is_idr: bool,
    ) -> Self::BitstreamUnitData {
        vk::native::StdVideoEncodeH264SliceHeader {
            flags: vk::native::StdVideoEncodeH264SliceHeaderFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoEncodeH264SliceHeaderFlags::new_bitfield_1(
                    1, // TODO: b-frames
                    1, // TODO: don't override always
                    0,
                ),
            },
            first_mb_in_slice: 0,
            slice_type: if is_idr {
                vk::native::StdVideoH264SliceType_STD_VIDEO_H264_SLICE_TYPE_I
            } else {
                vk::native::StdVideoH264SliceType_STD_VIDEO_H264_SLICE_TYPE_P
            }, // TODO: b-frames
            slice_alpha_c0_offset_div2: 0,
            slice_beta_offset_div2: 0,
            slice_qp_delta: 0,
            reserved1: 0,
            cabac_init_idc: vk::native::StdVideoH264CabacInitIdc_STD_VIDEO_H264_CABAC_INIT_IDC_0,
            disable_deblocking_filter_idc: 0,
            pWeightTable: std::ptr::null(),
        }
    }

    type BitstreamUnitInfo<'a> = vk::VideoEncodeH264NaluSliceInfoKHR<'a>;
    fn bitstream_unit_info<'a>(
        data: &'a Self::BitstreamUnitData,
        rate_control: RateControl,
        capabilities: &NativeEncodeQualityLevelProperties<Self>,
        is_idr: bool,
    ) -> Self::BitstreamUnitInfo<'a> {
        let mut slice_info = vk::VideoEncodeH264NaluSliceInfoKHR::default().std_slice_header(data);

        if let RateControl::Disabled = rate_control {
            if !capabilities.zeroed() {
                let qp = capabilities
                    .codec_quality_level_properties
                    .preferred_constant_qp;

                if is_idr {
                    slice_info.constant_qp = qp.qp_i;
                } else {
                    slice_info.constant_qp = qp.qp_p;
                }
            }
        }

        slice_info
    }

    type ReferenceInfo = vk::native::StdVideoEncodeH264ReferenceInfo;
    type ReferenceListInfo = vk::native::StdVideoEncodeH264ReferenceListsInfo;
    fn reference_list_info(
        _counters: &Self::EncodingCounters,
        active_reference_slots: &VecDeque<(usize, Self::ReferenceInfo)>,
    ) -> Self::ReferenceListInfo {
        let mut ref_list0 = [0xff; 32];
        for (i, (slot, _)) in active_reference_slots.iter().rev().enumerate() {
            ref_list0[i] = *slot as u8;
        }

        vk::native::StdVideoEncodeH264ReferenceListsInfo {
            flags: vk::native::StdVideoEncodeH264ReferenceListsInfoFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoEncodeH264ReferenceListsInfoFlags::new_bitfield_1(
                    0, 0, 0,
                ),
            },
            num_ref_idx_l0_active_minus1: active_reference_slots.len().saturating_sub(1) as u8,
            num_ref_idx_l1_active_minus1: 0,
            RefPicList0: ref_list0,
            RefPicList1: [0xff; 32],
            refList0ModOpCount: 0,
            refList1ModOpCount: 0,
            refPicMarkingOpCount: 0,
            reserved1: [0; 7],
            pRefList0ModOperations: std::ptr::null(),
            pRefList1ModOperations: std::ptr::null(),
            pRefPicMarkingOperations: std::ptr::null(),
        }
    }
    fn new_slot_reference_info(
        counters: &Self::EncodingCounters,
        is_idr: bool,
    ) -> Self::ReferenceInfo {
        vk::native::StdVideoEncodeH264ReferenceInfo {
            flags: vk::native::StdVideoEncodeH264ReferenceInfoFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoEncodeH264ReferenceInfoFlags::new_bitfield_1(0, 0),
            },
            primary_pic_type: primary_pic_type(is_idr),
            FrameNum: counters.frame_num,
            PicOrderCnt: counters.pic_order_cnt as i32,
            long_term_pic_num: 0,
            long_term_frame_idx: 0,
            temporal_id: 0,
        }
    }

    type PictureInfoData = vk::native::StdVideoEncodeH264PictureInfo;
    fn picture_info_data(
        counters: &Self::EncodingCounters,
        _codec_capabilities: &Self::CodecSpecificEncodeCapabilities<'_>,
        is_idr: bool,
        ref_lists: &Self::ReferenceListInfo,
    ) -> Self::PictureInfoData {
        vk::native::StdVideoEncodeH264PictureInfo {
            flags: vk::native::StdVideoEncodeH264PictureInfoFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoEncodeH264PictureInfoFlags::new_bitfield_1(
                    is_idr as u32,
                    1, // TODO: must be the same as nal_ref_idc != 0
                    0,
                    0, // long term refs
                    0, // adaptive reference control
                    0,
                ),
            },
            seq_parameter_set_id: 0,
            pic_parameter_set_id: 0,
            idr_pic_id: counters.idr_pic_id,
            primary_pic_type: primary_pic_type(is_idr),
            frame_num: counters.frame_num,
            PicOrderCnt: counters.pic_order_cnt as i32,
            temporal_id: 0,
            reserved1: [0; 3],
            pRefLists: ref_lists,
        }
    }

    type PictureInfo<'a> = vk::VideoEncodeH264PictureInfoKHR<'a>;
    fn picture_info<'a, 'b: 'a>(
        data: &'a Self::PictureInfoData,
        bitstream_unit_infos: &'a [Self::BitstreamUnitInfo<'b>],
    ) -> Self::PictureInfo<'a> {
        vk::VideoEncodeH264PictureInfoKHR::default()
            .std_picture_info(data)
            .nalu_slice_entries(bitstream_unit_infos)
            .generate_prefix_nalu(false)
    }

    type DpbSlotInfo<'a> = vk::VideoEncodeH264DpbSlotInfoKHR<'a>;
    fn dpb_slot_info<'a>(reference_info: &'a Self::ReferenceInfo) -> Self::DpbSlotInfo<'a> {
        vk::VideoEncodeH264DpbSlotInfoKHR::default().std_reference_info(reference_info)
    }

    type EncodingCounters = H264EncodingCounters;
    fn advance_counters(counters: &mut Self::EncodingCounters, is_idr: bool) {
        counters.frame_num = counters.frame_num.wrapping_add(1);
        counters.pic_order_cnt = counters.pic_order_cnt.wrapping_add(2);
        if is_idr {
            counters.idr_pic_id = counters.idr_pic_id.wrapping_add(1);
        }
    }
    fn counters_idr(counters: &mut Self::EncodingCounters) {
        counters.frame_num = 0;
        counters.pic_order_cnt = 0;
    }

    type CodecRateControlLayerInfo<'a> = vk::VideoEncodeH264RateControlLayerInfoKHR<'a>;
    type CodecRateControlInfo<'a> = vk::VideoEncodeH264RateControlInfoKHR<'a>;
    fn codec_rate_control_layer_info<'a>(
        rate_control: RateControl,
    ) -> Option<Vec<Self::CodecRateControlLayerInfo<'a>>> {
        let layer_info = vk::VideoEncodeH264RateControlLayerInfoKHR::default()
            .use_min_qp(false)
            .use_max_qp(false)
            .use_max_frame_size(false);

        match rate_control {
            RateControl::EncoderDefault => return None,
            RateControl::VariableBitrate { .. } => {}
            RateControl::ConstantBitrate { .. } => {}
            RateControl::Disabled => {}
        }

        Some(vec![layer_info])
    }
    fn codec_rate_control_info<'a>(
        layers: Option<&'a [vk::VideoEncodeRateControlLayerInfoKHR<'a>]>,
        idr_period: u32,
    ) -> Option<Self::CodecRateControlInfo<'a>> {
        let layers = layers?;

        Some(
            vk::VideoEncodeH264RateControlInfoKHR::default()
                .temporal_layer_count(layers.len() as u32)
                .flags(
                    vk::VideoEncodeH264RateControlFlagsKHR::REGULAR_GOP
                        | vk::VideoEncodeH264RateControlFlagsKHR::REFERENCE_PATTERN_FLAT,
                )
                .consecutive_b_frame_count(0)
                .gop_frame_count(idr_period)
                .idr_period(idr_period),
        )
    }

    type CodecWriteParametersInfo = H264WriteParametersInfo;
    type CodecEncodeSessionParametersGetInfo<'a> =
        vk::VideoEncodeH264SessionParametersGetInfoKHR<'a>;
    fn codec_session_parameters_get_info<'a>(
        info: Self::CodecWriteParametersInfo,
    ) -> Self::CodecEncodeSessionParametersGetInfo<'a> {
        vk::VideoEncodeH264SessionParametersGetInfoKHR::default()
            .write_std_sps(info.write_sps)
            .write_std_pps(info.write_pps)
            .std_sps_id(0)
            .std_pps_id(0)
    }
    fn codec_write_parameters_info_all() -> Self::CodecWriteParametersInfo {
        H264WriteParametersInfo {
            write_sps: true,
            write_pps: true,
        }
    }

    fn resolve_idr_period<'a>(
        quality_level_properties: &Self::CodecSpecificEncodeQualityLevelProperties<'a>,
        user_provided: Option<NonZeroU32>,
    ) -> NonZeroU32 {
        if let Some(user_provided) = user_provided {
            return user_provided;
        }

        if quality_level_properties.preferred_idr_period > 0 {
            NonZeroU32::new(quality_level_properties.preferred_idr_period).unwrap()
        } else {
            NonZeroU32::new(30).unwrap()
        }
    }

    fn resolve_max_references<'a>(
        quality_level_properties: &Self::CodecSpecificEncodeQualityLevelProperties<'a>,
        codec_capabilities: &Self::CodecSpecificEncodeCapabilities<'a>,
        user_provided: Option<NonZeroU32>,
    ) -> NonZeroU32 {
        let max = NonZeroU32::new(codec_capabilities.max_p_picture_l0_reference_count).unwrap();
        if let Some(user_provided) = user_provided {
            return user_provided.min(max);
        }

        if quality_level_properties.preferred_max_l0_reference_count > 0 {
            NonZeroU32::new(quality_level_properties.preferred_max_l0_reference_count).unwrap()
        } else {
            max
        }
    }
}

fn primary_pic_type(is_idr: bool) -> vk::native::StdVideoH264PictureType {
    if is_idr {
        vk::native::StdVideoH264PictureType_STD_VIDEO_H264_PICTURE_TYPE_IDR
    } else {
        vk::native::StdVideoH264PictureType_STD_VIDEO_H264_PICTURE_TYPE_P
    }
}
