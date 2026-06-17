use std::num::NonZeroU32;

use ash::vk;

use crate::{
    codec::{
        EncodeCodec,
        h265::{
            H265Codec, H265VkParameters,
            parameters::{
                VkH265PictureParameterSet, VkH265SequenceParameterSet, VkH265VideoParameterSet,
            },
        },
    },
    parameters::RateControl,
    wrappers::ProfileInfo,
};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct H265EncodingCounters {
    pic_order_cnt: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct H265WriteParametersInfo {
    pub(crate) write_vps: bool,
    pub(crate) write_sps: bool,
    pub(crate) write_pps: bool,
}

impl EncodeCodec for H265Codec {
    fn profile_info<'a>(
        params: &crate::vulkan_encoder::FullEncoderParameters<Self>,
    ) -> crate::wrappers::ProfileInfo<'a> {
        let h265_profile = vk::VideoEncodeH265ProfileInfoKHR::default()
            .std_profile_idc(params.profile.to_profile_idc());
        let h265_profile = Box::new(h265_profile);

        let usage_info: vk::VideoEncodeUsageInfoKHR = params.into();
        let usage_info = Box::new(usage_info);

        let profile = vk::VideoProfileInfoKHR::default()
            .video_codec_operation(vk::VideoCodecOperationFlagsKHR::ENCODE_H265)
            .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420)
            .luma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
            .chroma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8);

        ProfileInfo::new(profile, vec![h265_profile, usage_info])
    }

    fn encode_profile_capabilities(
        caps: &Self::NativeEncodeCodecCapabilities,
        profile: Self::Profile,
    ) -> Option<&crate::device::caps::NativeEncodeProfileCapabilities<Self>> {
        caps.profile(profile)
    }

    fn codec_parameters(
        parameters: &crate::vulkan_encoder::FullEncoderParameters<Self>,
        codec_capabilities: &Self::CodecSpecificEncodeCapabilities<'_>,
    ) -> Result<Self::OwnedParameters, crate::VulkanEncoderError> {
        Ok(Self::OwnedParameters {
            vps: vec![VkH265VideoParameterSet::new_encode(parameters)],
            sps: vec![VkH265SequenceParameterSet::new_encode(
                parameters,
                codec_capabilities,
            )],
            pps: vec![VkH265PictureParameterSet::new_encode(codec_capabilities)],
        })
    }

    fn vk_parameters<'a>(parameters: &'a Self::OwnedParameters) -> Self::VkParameters<'a> {
        H265VkParameters {
            vps: parameters.vps.iter().map(|p| p.vps).collect(),
            sps: parameters.sps.iter().map(|p| p.sps).collect(),
            pps: parameters.pps.iter().map(|p| p.pps).collect(),
        }
    }

    type BitstreamUnitData = vk::native::StdVideoEncodeH265SliceSegmentHeader;
    fn bitstream_unit_data(
        codec_capabilities: &Self::CodecSpecificEncodeCapabilities<'_>,
        is_idr: bool,
    ) -> Self::BitstreamUnitData {
        let slice_sao = codec_capabilities
            .std_syntax_flags
            .contains(vk::VideoEncodeH265StdFlagsKHR::SAMPLE_ADAPTIVE_OFFSET_ENABLED_FLAG_SET)
            as u32;

        vk::native::StdVideoEncodeH265SliceSegmentHeader {
            flags: vk::native::StdVideoEncodeH265SliceSegmentHeaderFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoEncodeH265SliceSegmentHeaderFlags::new_bitfield_1(
                    1,                          // first_slice_segment_in_pic_flag
                    0,                          // dependent_slice_segment_flag
                    slice_sao,                  // slice_sao_luma_flag
                    slice_sao,                  // slice_sao_chroma_flag
                    if is_idr { 0 } else { 1 }, // num_ref_idx_active_override_flag
                    0,                          // mvd_l1_zero_flag
                    0,                          // cabac_init_flag
                    0,                          // cu_chroma_qp_offset_enabled_flag
                    0,                          // deblocking_filter_override_flag
                    0,                          // slice_deblocking_filter_disabled_flag
                    1, // collocated_from_l0_flag (use L0 for collocated picture)
                    0, // slice_loop_filter_across_slices_enabled_flag
                    0, // reserved
                ),
            },
            slice_type: if is_idr {
                vk::native::StdVideoH265SliceType_STD_VIDEO_H265_SLICE_TYPE_I
            } else {
                vk::native::StdVideoH265SliceType_STD_VIDEO_H265_SLICE_TYPE_P
            },
            slice_segment_address: 0,
            collocated_ref_idx: 0, // collocate with previous ref frame (I hope that's what it means)
            MaxNumMergeCand: 5,    // anything different and amd breaks
            slice_qp_delta: 0,
            slice_cb_qp_offset: 0,
            slice_cr_qp_offset: 0,
            slice_beta_offset_div2: 0,
            slice_tc_offset_div2: 0,
            slice_act_y_qp_offset: 0,
            slice_act_cb_qp_offset: 0,
            slice_act_cr_qp_offset: 0,
            reserved1: 0,
            pWeightTable: std::ptr::null(),
        }
    }

    type BitstreamUnitInfo<'a> = vk::VideoEncodeH265NaluSliceSegmentInfoKHR<'a>;
    fn bitstream_unit_info<'a>(
        data: &'a Self::BitstreamUnitData,
        rate_control: crate::parameters::RateControl,
        capabilities: &crate::device::caps::NativeEncodeQualityLevelProperties<Self>,
        is_idr: bool,
    ) -> Self::BitstreamUnitInfo<'a> {
        let mut slice_info =
            vk::VideoEncodeH265NaluSliceSegmentInfoKHR::default().std_slice_segment_header(data);

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

    type ReferenceInfo = vk::native::StdVideoEncodeH265ReferenceInfo;
    type ReferenceListInfo = ReferenceListInfoH265;
    fn reference_list_info(
        counters: &Self::EncodingCounters,
        active_reference_slots: &std::collections::VecDeque<(usize, Self::ReferenceInfo)>,
    ) -> Self::ReferenceListInfo {
        let mut ref_list0 = [0xff; 15];
        for (i, (slot, _)) in active_reference_slots.iter().rev().enumerate() {
            ref_list0[i] = *slot as u8;
        }

        let list_info = vk::native::StdVideoEncodeH265ReferenceListsInfo {
            flags: vk::native::StdVideoEncodeH265ReferenceListsInfoFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoEncodeH265ReferenceListsInfoFlags::new_bitfield_1(
                    0, 0, 0,
                ),
            },
            num_ref_idx_l0_active_minus1: active_reference_slots.len().saturating_sub(1) as u8,
            num_ref_idx_l1_active_minus1: 0,
            RefPicList0: ref_list0,
            RefPicList1: [0xff; 15],
            list_entry_l0: [0; 15],
            list_entry_l1: [0; 15],
        };

        let mut delta_poc_s0_minus1 = [0; 16];
        let mut previous_poc = counters.pic_order_cnt as i32;
        let mut used_by_curr_pic_s0_flag = 0;

        for (i, reference) in active_reference_slots.iter().rev().enumerate() {
            assert!(reference.1.PicOrderCntVal < previous_poc);

            delta_poc_s0_minus1[i] = (previous_poc - reference.1.PicOrderCntVal - 1) as u16;
            used_by_curr_pic_s0_flag |= 1 << i;
            previous_poc = reference.1.PicOrderCntVal;
        }

        let short_term_ref_pic_set = vk::native::StdVideoH265ShortTermRefPicSet {
            flags: vk::native::StdVideoH265ShortTermRefPicSetFlags {
                _bitfield_align_1: [],
                __bindgen_padding_0: [0; 3],
                _bitfield_1: vk::native::StdVideoH265ShortTermRefPicSetFlags::new_bitfield_1(0, 0),
            },
            // for inter-ref set prediction, which is used to base this ref-set on one from sps
            delta_idx_minus1: 0,
            use_delta_flag: 0,
            abs_delta_rps_minus1: 0,
            used_by_curr_pic_flag: 0,

            num_negative_pics: active_reference_slots.len() as u8,
            used_by_curr_pic_s0_flag,
            delta_poc_s0_minus1,

            num_positive_pics: 0,
            used_by_curr_pic_s1_flag: 0,
            delta_poc_s1_minus1: [0; 16],

            reserved1: 0,
            reserved2: 0,
            reserved3: 0,
        };

        ReferenceListInfoH265 {
            list_info,
            short_term_ref_pic_set,
        }
    }
    fn new_slot_reference_info(
        counters: &Self::EncodingCounters,
        is_idr: bool,
    ) -> Self::ReferenceInfo {
        vk::native::StdVideoEncodeH265ReferenceInfo {
            flags: vk::native::StdVideoEncodeH265ReferenceInfoFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoEncodeH265ReferenceInfoFlags::new_bitfield_1(
                    0, 0, 0,
                ),
            },
            pic_type: pic_type(is_idr),
            PicOrderCntVal: counters.pic_order_cnt as i32,
            TemporalId: 0,
        }
    }

    type PictureInfoData = vk::native::StdVideoEncodeH265PictureInfo;
    fn picture_info_data(
        counters: &Self::EncodingCounters,
        codec_capabilities: &Self::CodecSpecificEncodeCapabilities<'_>,
        is_idr: bool,
        ref_lists: &Self::ReferenceListInfo,
    ) -> Self::PictureInfoData {
        // Must be 0 when sps_temporal_mvp_enabled_flag is 0 (H.265 7.4.7.1).
        // For IDR slices there are no reference pictures to derive MV candidates from.
        let slice_temporal_mvp_enabled_flag = (codec_capabilities
            .std_syntax_flags
            .contains(vk::VideoEncodeH265StdFlagsKHR::SPS_TEMPORAL_MVP_ENABLED_FLAG_SET)
            && !is_idr) as u32;

        vk::native::StdVideoEncodeH265PictureInfo {
            flags: vk::native::StdVideoEncodeH265PictureInfoFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoEncodeH265PictureInfoFlags::new_bitfield_1(
                    1,             // is_reference
                    is_idr as u32, // IrapPicFlag
                    0,             // used_for_long_term_reference
                    0,             // discardable_flag
                    0,             // cross_layer_bla_flag
                    1,             // pic_output_flag
                    0,             // no_output_of_prior_pics_flag
                    0,             // short_term_ref_pic_set_sps_flag
                    slice_temporal_mvp_enabled_flag,
                    0, // reserved
                ),
            },
            pic_type: pic_type(is_idr),
            sps_video_parameter_set_id: 0,
            pps_seq_parameter_set_id: 0,
            pps_pic_parameter_set_id: 0,
            PicOrderCntVal: counters.pic_order_cnt as i32,
            TemporalId: 0,
            reserved1: [0; 7],
            short_term_ref_pic_set_idx: 0,
            pRefLists: &ref_lists.list_info,
            pShortTermRefPicSet: &ref_lists.short_term_ref_pic_set,
            pLongTermRefPics: std::ptr::null(),
        }
    }

    type PictureInfo<'a> = vk::VideoEncodeH265PictureInfoKHR<'a>;
    fn picture_info<'a, 'b: 'a>(
        data: &'a Self::PictureInfoData,
        bitstream_unit_infos: &'a [Self::BitstreamUnitInfo<'b>],
    ) -> Self::PictureInfo<'a> {
        vk::VideoEncodeH265PictureInfoKHR::default()
            .nalu_slice_segment_entries(bitstream_unit_infos)
            .std_picture_info(data)
    }

    type DpbSlotInfo<'a> = vk::VideoEncodeH265DpbSlotInfoKHR<'a>;
    fn dpb_slot_info<'a>(reference_info: &'a Self::ReferenceInfo) -> Self::DpbSlotInfo<'a> {
        vk::VideoEncodeH265DpbSlotInfoKHR::default().std_reference_info(reference_info)
    }

    type EncodingCounters = H265EncodingCounters;
    fn advance_counters(counters: &mut Self::EncodingCounters, _is_idr: bool) {
        counters.pic_order_cnt = counters.pic_order_cnt.wrapping_add(1);
    }
    fn counters_idr(counters: &mut Self::EncodingCounters) {
        counters.pic_order_cnt = 0;
    }

    type CodecRateControlLayerInfo<'a> = vk::VideoEncodeH265RateControlLayerInfoKHR<'a>;
    type CodecRateControlInfo<'a> = vk::VideoEncodeH265RateControlInfoKHR<'a>;
    fn codec_rate_control_layer_info<'a>(
        rate_control: crate::parameters::RateControl,
    ) -> Option<Vec<Self::CodecRateControlLayerInfo<'a>>> {
        let layer_info = vk::VideoEncodeH265RateControlLayerInfoKHR::default()
            .use_max_qp(false)
            .use_min_qp(false)
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
        layers: Option<&'a [ash::vk::VideoEncodeRateControlLayerInfoKHR<'a>]>,
        idr_period: u32,
    ) -> Option<Self::CodecRateControlInfo<'a>> {
        let layers = layers?;

        Some(
            vk::VideoEncodeH265RateControlInfoKHR::default()
                .sub_layer_count(layers.len() as u32)
                .idr_period(idr_period)
                .gop_frame_count(idr_period)
                .consecutive_b_frame_count(0)
                .flags(
                    vk::VideoEncodeH265RateControlFlagsKHR::REGULAR_GOP
                        | vk::VideoEncodeH265RateControlFlagsKHR::REFERENCE_PATTERN_FLAT,
                ),
        )
    }

    type CodecWriteParametersInfo = H265WriteParametersInfo;
    type CodecEncodeSessionParametersGetInfo<'a> =
        vk::VideoEncodeH265SessionParametersGetInfoKHR<'a>;
    fn codec_session_parameters_get_info<'a>(
        info: Self::CodecWriteParametersInfo,
    ) -> Self::CodecEncodeSessionParametersGetInfo<'a> {
        Self::CodecEncodeSessionParametersGetInfo::default()
            .write_std_vps(info.write_vps)
            .write_std_sps(info.write_sps)
            .write_std_pps(info.write_pps)
            .std_vps_id(0)
            .std_sps_id(0)
            .std_pps_id(0)
    }
    fn codec_write_parameters_info_all() -> Self::CodecWriteParametersInfo {
        Self::CodecWriteParametersInfo {
            write_vps: true,
            write_sps: true,
            write_pps: true,
        }
    }

    fn resolve_idr_period<'a>(
        quality_level_properties: &Self::CodecSpecificEncodeQualityLevelProperties<'a>,
        user_provided: Option<std::num::NonZeroU32>,
    ) -> std::num::NonZeroU32 {
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
        user_provided: Option<std::num::NonZeroU32>,
    ) -> std::num::NonZeroU32 {
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

fn pic_type(is_idr: bool) -> u32 {
    if is_idr {
        vk::native::StdVideoH265PictureType_STD_VIDEO_H265_PICTURE_TYPE_IDR
    } else {
        vk::native::StdVideoH265PictureType_STD_VIDEO_H265_PICTURE_TYPE_P
    }
}

pub(crate) struct ReferenceListInfoH265 {
    list_info: vk::native::StdVideoEncodeH265ReferenceListsInfo,
    short_term_ref_pic_set: vk::native::StdVideoH265ShortTermRefPicSet,
}
