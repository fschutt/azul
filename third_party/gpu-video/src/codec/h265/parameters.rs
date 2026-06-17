use ash::vk;

use crate::{VulkanDecoderError, codec::h265::H265Codec, vulkan_encoder::FullEncoderParameters};

pub(crate) struct VkH265VideoParameterSet {
    pub(crate) vps: vk::native::StdVideoH265VideoParameterSet,
    _profile_tier_level: Box<vk::native::StdVideoH265ProfileTierLevel>,
    _dec_pic_buf_mgr: Box<vk::native::StdVideoH265DecPicBufMgr>,
}

fn profile_tier_level(
    params: &FullEncoderParameters<H265Codec>,
) -> vk::native::StdVideoH265ProfileTierLevel {
    vk::native::StdVideoH265ProfileTierLevel {
        flags: vk::native::StdVideoH265ProfileTierLevelFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH265ProfileTierLevelFlags::new_bitfield_1(
                1, 1, 0, 1, 1,
            ),
            __bindgen_padding_0: [0; 3],
        },
        general_profile_idc: params.profile.to_profile_idc(),
        general_level_idc: vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_1,
    }
}

fn dec_pic_buf_mgr(
    params: &FullEncoderParameters<H265Codec>,
) -> vk::native::StdVideoH265DecPicBufMgr {
    let mut dec_pic_buf_mgr = vk::native::StdVideoH265DecPicBufMgr {
        max_num_reorder_pics: [0; 7],
        max_dec_pic_buffering_minus1: [0; 7],
        max_latency_increase_plus1: [0; 7],
    };
    dec_pic_buf_mgr.max_dec_pic_buffering_minus1[0] = params.max_references.get() as u8;
    dec_pic_buf_mgr.max_latency_increase_plus1[0] = 1;
    dec_pic_buf_mgr.max_num_reorder_pics[0] = 0;

    dec_pic_buf_mgr
}

impl VkH265VideoParameterSet {
    pub(crate) fn new_encode(params: &FullEncoderParameters<H265Codec>) -> Self {
        let profile_tier_level = Box::new(profile_tier_level(params));

        let dec_pic_buf_mgr = Box::new(dec_pic_buf_mgr(params));

        Self {
            vps: vk::native::StdVideoH265VideoParameterSet {
                reserved1: 0,
                flags: vk::native::StdVideoH265VpsFlags {
                    _bitfield_align_1: [],
                    _bitfield_1: vk::native::StdVideoH265VpsFlags::new_bitfield_1(1, 1, 0, 0),
                    __bindgen_padding_0: [0; 3],
                },
                vps_video_parameter_set_id: 0,
                vps_max_sub_layers_minus1: 0,
                reserved2: 0,
                vps_num_units_in_tick: 0,
                vps_time_scale: 0,
                vps_num_ticks_poc_diff_one_minus1: 0,
                reserved3: 0,
                pHrdParameters: std::ptr::null(),
                pDecPicBufMgr: dec_pic_buf_mgr.as_ref(),
                pProfileTierLevel: profile_tier_level.as_ref(),
            },
            _profile_tier_level: profile_tier_level,
            _dec_pic_buf_mgr: dec_pic_buf_mgr,
        }
    }
}

pub(crate) struct VkH265SequenceParameterSet {
    pub(crate) sps: vk::native::StdVideoH265SequenceParameterSet,
    _profile_tier_level: Option<Box<vk::native::StdVideoH265ProfileTierLevel>>,
    _dec_pic_buf_mgr: Option<Box<vk::native::StdVideoH265DecPicBufMgr>>,
}

impl VkH265SequenceParameterSet {
    pub(crate) fn new_encode(
        params: &FullEncoderParameters<H265Codec>,
        caps: &vk::VideoEncodeH265CapabilitiesKHR<'_>,
    ) -> Self {
        // TODO: VUI
        let profile_tier_level = Box::new(profile_tier_level(params));
        let dec_pic_buf_mgr = Box::new(dec_pic_buf_mgr(params));

        let ctb_log2_size = largest_supported_ctb_log2_size(caps.ctb_sizes);

        let min_cb_log2_size: u32 = 3; // MinCbSizeY = 8
        let log2_diff_max_min_luma_coding_block_size = ctb_log2_size - min_cb_log2_size;

        let min_tb_log2_size: u32 = 2; // MinTbSizeY = 4
        // MaxTbLog2SizeY = min(5, ctb_log2_size) per H.265 spec: MaxTbSizeY can be at most 32
        let max_tb_log2_size = ctb_log2_size.min(5);
        let log2_diff_max_min_luma_transform_block_size = max_tb_log2_size - min_tb_log2_size;

        let sample_adaptive_offset_enabled_flag = caps
            .std_syntax_flags
            .contains(vk::VideoEncodeH265StdFlagsKHR::SAMPLE_ADAPTIVE_OFFSET_ENABLED_FLAG_SET)
            as u32;
        let sps_temporal_mvp_enabled_flag = caps
            .std_syntax_flags
            .contains(vk::VideoEncodeH265StdFlagsKHR::SPS_TEMPORAL_MVP_ENABLED_FLAG_SET)
            as u32;

        Self {
            sps: vk::native::StdVideoH265SequenceParameterSet {
                flags: vk::native::StdVideoH265SpsFlags {
                    _bitfield_align_1: [],
                    _bitfield_1: vk::native::StdVideoH265SpsFlags::new_bitfield_1(
                        1, // sps_temporal_id_nesting_flag
                        0, // separate_colour_plane_flag
                        0, // conformance_window_flag (driver will override if needed)
                        0, // sps_sub_layer_ordering_info_present_flag
                        0, // scaling_list_enabled_flag
                        0, // sps_scaling_list_data_present_flag
                        1, // amp_enabled_flag
                        sample_adaptive_offset_enabled_flag,
                        0, // pcm_enabled_flag
                        0, // pcm_loop_filter_disabled_flag (irrelevant when pcm disabled)
                        0, // long_term_ref_pics_present_flag
                        sps_temporal_mvp_enabled_flag,
                        1, // strong_intra_smoothing_enabled_flag
                        0, // vui_parameters_present_flag
                        0, // sps_extension_present_flag
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0, // range extension
                        0,
                        0,
                        0,
                        0,
                        0, // scc extension
                    ),
                },
                chroma_format_idc:
                    vk::native::StdVideoH265ChromaFormatIdc_STD_VIDEO_H265_CHROMA_FORMAT_IDC_420,
                pic_width_in_luma_samples: params.width.get(),
                pic_height_in_luma_samples: params.height.get(),
                sps_video_parameter_set_id: 0,
                sps_max_sub_layers_minus1: 0,
                sps_seq_parameter_set_id: 0,
                bit_depth_luma_minus8: 0,
                bit_depth_chroma_minus8: 0,
                log2_max_pic_order_cnt_lsb_minus4: 4,
                log2_min_luma_coding_block_size_minus3: (min_cb_log2_size - 3) as u8,
                log2_diff_max_min_luma_coding_block_size: log2_diff_max_min_luma_coding_block_size
                    as u8,
                log2_min_luma_transform_block_size_minus2: (min_tb_log2_size - 2) as u8,
                log2_diff_max_min_luma_transform_block_size:
                    log2_diff_max_min_luma_transform_block_size as u8,
                // max depth = CtbLog2SizeY - MinTbLog2SizeY = ctb_log2_size - min_tb_log2_size
                max_transform_hierarchy_depth_inter: (ctb_log2_size - min_tb_log2_size) as u8,
                max_transform_hierarchy_depth_intra: (ctb_log2_size - min_tb_log2_size) as u8,
                num_short_term_ref_pic_sets: 0, // ref sets are in each slice header
                num_long_term_ref_pics_sps: 0,
                pcm_sample_bit_depth_luma_minus1: 0,   // disabled
                pcm_sample_bit_depth_chroma_minus1: 0, // disabled
                log2_min_pcm_luma_coding_block_size_minus3: 0, // disabled
                log2_diff_max_min_pcm_luma_coding_block_size: 0, //disabled
                reserved1: 0,
                reserved2: 0,
                palette_max_size: 0,                              //disabled
                delta_palette_max_predictor_size: 0,              //disabled
                motion_vector_resolution_control_idc: 0,          //disabled
                sps_num_palette_predictor_initializers_minus1: 0, //disabled
                conf_win_left_offset: 0,
                conf_win_right_offset: 0,
                conf_win_top_offset: 0,
                conf_win_bottom_offset: 0,
                pProfileTierLevel: profile_tier_level.as_ref(),
                pDecPicBufMgr: dec_pic_buf_mgr.as_ref(),
                pScalingLists: std::ptr::null(),
                pShortTermRefPicSet: std::ptr::null(),
                pLongTermRefPicsSps: std::ptr::null(),
                pSequenceParameterSetVui: std::ptr::null(), // TODO
                pPredictorPaletteEntries: std::ptr::null(),
            },

            _profile_tier_level: Some(profile_tier_level),
            _dec_pic_buf_mgr: Some(dec_pic_buf_mgr),
        }
    }
}

fn largest_supported_ctb_log2_size(ctb_sizes: vk::VideoEncodeH265CtbSizeFlagsKHR) -> u32 {
    if ctb_sizes.contains(vk::VideoEncodeH265CtbSizeFlagsKHR::TYPE_64) {
        6
    } else if ctb_sizes.contains(vk::VideoEncodeH265CtbSizeFlagsKHR::TYPE_32) {
        5
    } else {
        4
    }
}

pub(crate) struct VkH265PictureParameterSet {
    pub(crate) pps: vk::native::StdVideoH265PictureParameterSet,
}

impl VkH265PictureParameterSet {
    pub(crate) fn new_encode(caps: &vk::VideoEncodeH265CapabilitiesKHR<'_>) -> Self {
        let sign_data_hiding_enabled_flag = caps
            .std_syntax_flags
            .contains(vk::VideoEncodeH265StdFlagsKHR::SIGN_DATA_HIDING_ENABLED_FLAG_SET)
            as u32;
        let transform_skip_enabled_flag = caps
            .std_syntax_flags
            .contains(vk::VideoEncodeH265StdFlagsKHR::TRANSFORM_SKIP_ENABLED_FLAG_SET)
            as u32;

        Self {
            pps: vk::native::StdVideoH265PictureParameterSet {
                flags: vk::native::StdVideoH265PpsFlags {
                    _bitfield_align_1: [],
                    _bitfield_1: vk::native::StdVideoH265PpsFlags::new_bitfield_1(
                        0, // dependent_slice_segments_enabled_flag
                        0, // output_flag_present_flag
                        sign_data_hiding_enabled_flag,
                        0, // cabac_init_present_flag
                        0, // constrained_intra_pred_flag
                        transform_skip_enabled_flag,
                        1, // cu_qp_delta_enabled_flag
                        0, // pps_slice_chroma_qp_offsets_present_flag
                        0, // weighted_pred_flag
                        0, // weighted_bipred_flag
                        0, // transquant_bypass_enabled_flag
                        0, // tiles_enabled_flag
                        0, // entropy_coding_sync_enabled_flag
                        0, // uniform_spacing_flag
                        0, // loop_filter_across_tiles_enabled_flag
                        1, // pps_loop_filter_across_slices_enabled_flag
                        0, // deblocking_filter_control_present_flag
                        0, // deblocking_filter_override_enabled_flag
                        0, // pps_deblocking_filter_disabled_flag
                        0, // pps_scaling_list_data_present_flag
                        0, // lists_modification_present_flag
                        0, // slice_segment_header_extension_present_flag
                        0, // pps_extension_present_flag
                        0, // cross_component_prediction_enabled_flag
                        0, // chroma_qp_offset_list_enabled_flag
                        0, // pps_curr_pic_ref_enabled_flag
                        0, // residual_adaptive_colour_transform_enabled_flag
                        0, // pps_slice_act_qp_offsets_present_flag
                        0, // pps_palette_predictor_initializers_present_flag
                        0, // monochrome_palette_flag
                        0, // pps_range_extension_flag
                    ),
                },
                sps_video_parameter_set_id: 0,
                pps_seq_parameter_set_id: 0,
                pps_pic_parameter_set_id: 0,
                reserved1: 0,
                reserved2: 0,
                num_extra_slice_header_bits: 0,
                num_ref_idx_l0_default_active_minus1: 0,
                num_ref_idx_l1_default_active_minus1: 0,
                init_qp_minus26: 0,
                diff_cu_qp_delta_depth: 1,
                pps_cb_qp_offset: 0,
                pps_cr_qp_offset: 0,
                pps_beta_offset_div2: 0,
                pps_tc_offset_div2: 0,
                log2_parallel_merge_level_minus2: 0,
                log2_max_transform_skip_block_size_minus2: 0,
                diff_cu_chroma_qp_offset_depth: 0,
                chroma_qp_offset_list_len_minus1: 0,
                cb_qp_offset_list: [0; 6],
                cr_qp_offset_list: [0; 6],
                log2_sao_offset_scale_luma: 0,
                log2_sao_offset_scale_chroma: 0,
                pps_act_y_qp_offset_plus5: 0,
                pps_act_cb_qp_offset_plus5: 0,
                pps_act_cr_qp_offset_plus3: 0,
                pps_num_palette_predictor_initializers: 0,
                luma_bit_depth_entry_minus8: 0,
                chroma_bit_depth_entry_minus8: 0,
                num_tile_columns_minus1: 0,
                num_tile_rows_minus1: 0,
                column_width_minus1: [0; 19],
                row_height_minus1: [0; 21],
                reserved3: 0,
                pScalingLists: std::ptr::null(),
                pPredictorPaletteEntries: std::ptr::null(),
            },
        }
    }
}

pub(crate) fn vk_to_h265_level_idc(
    level_idc: vk::native::StdVideoH265LevelIdc,
) -> Result<u8, VulkanDecoderError> {
    match level_idc {
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_1_0 => Ok(30),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_2_0 => Ok(60),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_2_1 => Ok(63),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_3_0 => Ok(90),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_3_1 => Ok(93),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_4_0 => Ok(120),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_4_1 => Ok(123),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_0 => Ok(150),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_1 => Ok(153),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_2 => Ok(156),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_0 => Ok(180),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_1 => Ok(183),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_2 => Ok(186),
        _ => Err(VulkanDecoderError::InvalidInputData(format!(
            "unknown StdVideoH265LevelIdc: {level_idc}"
        ))),
    }
}
