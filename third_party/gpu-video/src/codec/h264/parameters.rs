use ash::vk;
use h264_reader::nal::sps::{FrameMbsFlags, SeqParameterSet};

use crate::{
    VulkanDecoderError, VulkanEncoderError,
    device::{ColorRange, ColorSpace, Rational},
    parameters::H264Profile,
    wrappers::ProfileInfo,
};

const MACROBLOCK_SIZE: u32 = 16;
const LOG2_MAX_FRAME_NUM: u8 = 7;

pub(crate) trait SeqParameterSetExt {
    fn size(&self) -> Result<vk::Extent2D, VulkanDecoderError>;
}

impl SeqParameterSetExt for SeqParameterSet {
    #[allow(non_snake_case)]
    fn size(&self) -> Result<vk::Extent2D, VulkanDecoderError> {
        let chroma_array_type = if self.chroma_info.separate_colour_plane_flag {
            0
        } else {
            self.chroma_info.chroma_format.to_chroma_format_idc()
        };

        let (SubWidthC, SubHeightC) = match self.chroma_info.chroma_format {
            h264_reader::nal::sps::ChromaFormat::Monochrome => {
                return Err(VulkanDecoderError::MonochromeChromaFormatUnsupported);
            }
            h264_reader::nal::sps::ChromaFormat::YUV420 => (2, 2),
            h264_reader::nal::sps::ChromaFormat::YUV422 => (2, 1),
            h264_reader::nal::sps::ChromaFormat::YUV444 => (1, 1),
            h264_reader::nal::sps::ChromaFormat::Invalid(x) => {
                return Err(VulkanDecoderError::InvalidInputData(format!(
                    "Invalid chroma_format_idc: {x}"
                )));
            }
        };

        let (CropUnitX, CropUnitY) = match chroma_array_type {
            0 => (
                1,
                2 - (self.frame_mbs_flags == FrameMbsFlags::Frames) as u32,
            ),

            _ => (
                SubWidthC,
                SubHeightC * (2 - (self.frame_mbs_flags == FrameMbsFlags::Frames) as u32),
            ),
        };

        let (width_offset, height_offset) = match &self.frame_cropping {
            None => (0, 0),
            Some(frame_cropping) => (
                (frame_cropping.left_offset + frame_cropping.right_offset) * CropUnitX,
                (frame_cropping.top_offset + frame_cropping.bottom_offset) * CropUnitY,
            ),
        };

        let width = (self.pic_width_in_mbs_minus1 + 1) * MACROBLOCK_SIZE - width_offset;
        let height = (self.pic_height_in_map_units_minus1 + 1)
            * (2 - (self.frame_mbs_flags == FrameMbsFlags::Frames) as u32)
            * MACROBLOCK_SIZE
            - height_offset;

        Ok(vk::Extent2D { width, height })
    }
}

pub(crate) struct VkH264SequenceParameterSet {
    pub(crate) sps: vk::native::StdVideoH264SequenceParameterSet,
    _scaling_lists: Option<Box<H264ScalingLists>>,
    _offset_for_ref_frame: Option<Box<[i32]>>,
    _vui: Option<Box<vk::native::StdVideoH264SequenceParameterSetVui>>,
}

impl From<&'_ SeqParameterSet> for VkH264SequenceParameterSet {
    #[allow(non_snake_case)]
    fn from(sps: &SeqParameterSet) -> VkH264SequenceParameterSet {
        let flags = vk::native::StdVideoH264SpsFlags {
            _bitfield_1: vk::native::StdVideoH264SpsFlags::new_bitfield_1(
                sps.constraint_flags.flag0().into(),
                sps.constraint_flags.flag1().into(),
                sps.constraint_flags.flag2().into(),
                sps.constraint_flags.flag3().into(),
                sps.constraint_flags.flag4().into(),
                sps.constraint_flags.flag5().into(),
                sps.direct_8x8_inference_flag.into(),
                match sps.frame_mbs_flags {
                    h264_reader::nal::sps::FrameMbsFlags::Frames => 0,
                    h264_reader::nal::sps::FrameMbsFlags::Fields {
                        mb_adaptive_frame_field_flag,
                    } => mb_adaptive_frame_field_flag.into(),
                },
                matches!(
                    sps.frame_mbs_flags,
                    h264_reader::nal::sps::FrameMbsFlags::Frames
                )
                .into(),
                match sps.pic_order_cnt {
                    h264_reader::nal::sps::PicOrderCntType::TypeOne {
                        delta_pic_order_always_zero_flag,
                        ..
                    } => delta_pic_order_always_zero_flag.into(),
                    // The spec doesn't say what to do if this flag is not present...
                    h264_reader::nal::sps::PicOrderCntType::TypeZero { .. }
                    | h264_reader::nal::sps::PicOrderCntType::TypeTwo => 0,
                },
                sps.chroma_info.separate_colour_plane_flag.into(),
                sps.gaps_in_frame_num_value_allowed_flag.into(),
                sps.chroma_info.qpprime_y_zero_transform_bypass_flag.into(),
                sps.frame_cropping.is_some().into(),
                sps.chroma_info.scaling_matrix.is_some().into(),
                0,
            ),
            _bitfield_align_1: [],
            __bindgen_padding_0: 0,
        };

        let profile_idc: u8 = sps.profile_idc.into();

        let pic_order_cnt_type = match sps.pic_order_cnt {
            h264_reader::nal::sps::PicOrderCntType::TypeZero { .. } => 0,
            h264_reader::nal::sps::PicOrderCntType::TypeOne { .. } => 1,
            h264_reader::nal::sps::PicOrderCntType::TypeTwo => 2,
        };

        let (
            offset_for_non_ref_pic,
            offset_for_top_to_bottom_field,
            num_ref_frames_in_pic_order_cnt_cycle,
        ) = match &sps.pic_order_cnt {
            h264_reader::nal::sps::PicOrderCntType::TypeOne {
                offset_for_non_ref_pic,
                offset_for_top_to_bottom_field,
                offsets_for_ref_frame,
                ..
            } => (
                *offset_for_non_ref_pic,
                *offset_for_top_to_bottom_field,
                offsets_for_ref_frame.len() as u8,
            ),
            h264_reader::nal::sps::PicOrderCntType::TypeZero { .. } => (0, 0, 0),
            h264_reader::nal::sps::PicOrderCntType::TypeTwo => (0, 0, 0),
        };

        let log2_max_pic_order_cnt_lsb_minus4 = match &sps.pic_order_cnt {
            h264_reader::nal::sps::PicOrderCntType::TypeZero {
                log2_max_pic_order_cnt_lsb_minus4,
            } => *log2_max_pic_order_cnt_lsb_minus4,
            h264_reader::nal::sps::PicOrderCntType::TypeOne { .. }
            | h264_reader::nal::sps::PicOrderCntType::TypeTwo => 0,
        };

        let (
            frame_crop_left_offset,
            frame_crop_right_offset,
            frame_crop_top_offset,
            frame_crop_bottom_offset,
        ) = match sps.frame_cropping {
            Some(h264_reader::nal::sps::FrameCropping {
                left_offset,
                right_offset,
                top_offset,
                bottom_offset,
            }) => (left_offset, right_offset, top_offset, bottom_offset),
            None => (0, 0, 0, 0),
        };

        let offset_for_ref_frame = match &sps.pic_order_cnt {
            h264_reader::nal::sps::PicOrderCntType::TypeOne {
                offsets_for_ref_frame,
                ..
            } => Some(offsets_for_ref_frame.clone()),
            h264_reader::nal::sps::PicOrderCntType::TypeZero { .. }
            | h264_reader::nal::sps::PicOrderCntType::TypeTwo => None,
        };

        let offset_for_ref_frame = offset_for_ref_frame.map(|o| o.into_boxed_slice());

        let pOffsetForRefFrame = match offset_for_ref_frame.as_ref() {
            Some(o) => o.as_ptr(),
            None => std::ptr::null(),
        };

        let scaling_lists: Option<Box<H264ScalingLists>> = sps
            .chroma_info
            .scaling_matrix
            .as_ref()
            .map(|matrix| Box::new(matrix.into()));

        let pScalingLists = match scaling_lists.as_ref() {
            Some(l) => &l.list,
            None => std::ptr::null(),
        };

        // TODO: this is not necessary to reconstruct samples. I don't know why the decoder would
        // need this. Maybe we can do this in the future.
        let pSequenceParameterSetVui = std::ptr::null();

        Self {
            sps: vk::native::StdVideoH264SequenceParameterSet {
                flags,
                profile_idc: profile_idc as u32,
                level_idc: h264_level_idc_to_vk(sps.level_idc),
                chroma_format_idc: sps.chroma_info.chroma_format.to_chroma_format_idc(),
                seq_parameter_set_id: sps.seq_parameter_set_id.id(),
                bit_depth_luma_minus8: sps.chroma_info.bit_depth_luma_minus8,
                bit_depth_chroma_minus8: sps.chroma_info.bit_depth_chroma_minus8,
                log2_max_frame_num_minus4: sps.log2_max_frame_num_minus4,
                pic_order_cnt_type,
                offset_for_non_ref_pic,
                offset_for_top_to_bottom_field,
                num_ref_frames_in_pic_order_cnt_cycle,
                log2_max_pic_order_cnt_lsb_minus4,
                max_num_ref_frames: sps.max_num_ref_frames as u8,
                reserved1: 0,
                pic_width_in_mbs_minus1: sps.pic_width_in_mbs_minus1,
                pic_height_in_map_units_minus1: sps.pic_height_in_map_units_minus1,
                frame_crop_left_offset,
                frame_crop_right_offset,
                frame_crop_top_offset,
                frame_crop_bottom_offset,
                reserved2: 0,
                pOffsetForRefFrame,
                pScalingLists,
                pSequenceParameterSetVui,
            },
            _scaling_lists: scaling_lists,
            _offset_for_ref_frame: offset_for_ref_frame,
            _vui: None,
        }
    }
}

impl VkH264SequenceParameterSet {
    #[allow(non_snake_case)]
    pub(crate) fn new_encode(
        profile: H264Profile,
        width: u32,
        height: u32,
        max_references: u32,
        color_space: ColorSpace,
        color_range: ColorRange,
        framerate: Rational,
    ) -> Result<VkH264SequenceParameterSet, VulkanEncoderError> {
        // separate_colour_plane_flag is 0 so the crop units are based on SubWidthC and SubHeightC for YUV420
        // with enabled frame_mbs_only_flag
        let (CropUnitX, CropUnitY) = (2, 2);

        let width_offset = (MACROBLOCK_SIZE - (width % MACROBLOCK_SIZE)) % MACROBLOCK_SIZE;
        let height_offset = (MACROBLOCK_SIZE - (height % MACROBLOCK_SIZE)) % MACROBLOCK_SIZE;

        let pic_width_in_mbs_minus1 = (width + width_offset) / MACROBLOCK_SIZE - 1;
        let pic_height_in_map_units_minus1 = (height + height_offset) / MACROBLOCK_SIZE - 1;
        let frame_crop_right_offset = width_offset / CropUnitX;
        let frame_crop_bottom_offset = height_offset / CropUnitY;

        let video_full_range_flag = match color_range {
            ColorRange::Full => 1,
            ColorRange::Limited => 0,
        };
        let color_description: H264ColorDescription = color_space.into();
        let time_scale = framerate
            .numerator
            .checked_mul(2)
            .ok_or(VulkanEncoderError::FramerateOverflow)?;

        let vui = Box::new(vk::native::StdVideoH264SequenceParameterSetVui {
            flags: vk::native::StdVideoH264SpsVuiFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoH264SpsVuiFlags::new_bitfield_1(
                    0,
                    0,
                    0,
                    1, // video_signal_type_present_flag
                    video_full_range_flag,
                    1, // color_description_present_flag
                    0,
                    1, // timing_info_present_flag
                    0,
                    1, // bitstream_restriction_flag
                    0,
                    0,
                ),
                __bindgen_padding_0: 0,
            },
            aspect_ratio_idc: 0,
            sar_width: 0,
            sar_height: 0,
            video_format: 5, // unspecified
            colour_primaries: color_description.colour_primaries,
            transfer_characteristics: color_description.transfer_characteristics,
            matrix_coefficients: color_description.matrix_coefficients,
            num_units_in_tick: framerate.denominator.get(),
            time_scale,
            max_num_reorder_frames: 0, // TODO: B frames
            max_dec_frame_buffering: max_references as u8,
            chroma_sample_loc_type_top_field: 0,
            chroma_sample_loc_type_bottom_field: 0,
            reserved1: 0,
            pHrdParameters: std::ptr::null(),
        });

        let sps = vk::native::StdVideoH264SequenceParameterSet {
            flags: vk::native::StdVideoH264SpsFlags {
                _bitfield_align_1: [0; 0],
                __bindgen_padding_0: 0,
                _bitfield_1: vk::native::StdVideoH264SpsFlags::new_bitfield_1(
                    0, 0, 0, 0, 0, 1, // flag 5 equal to 1 turns off B-slices
                    1, // ffmpeg
                    0, 1, // 1 - no fields
                    0, // only for pic_order_cnt_type 1
                    0, 0, 0, // ffmpeg
                    1, // use frame cropping
                    0, 1, // vui
                ),
            },
            profile_idc: profile.to_profile_idc(),
            level_idc: vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_1,
            chroma_format_idc:
                vk::native::StdVideoH264ChromaFormatIdc_STD_VIDEO_H264_CHROMA_FORMAT_IDC_420,
            seq_parameter_set_id: 0,
            bit_depth_luma_minus8: 0,
            bit_depth_chroma_minus8: 0,
            log2_max_frame_num_minus4: LOG2_MAX_FRAME_NUM - 4, // TODO: see how this impacts output
            pic_order_cnt_type: vk::native::StdVideoH264PocType_STD_VIDEO_H264_POC_TYPE_0,
            offset_for_non_ref_pic: 0, // only for pic_order_cnt_type 1
            offset_for_top_to_bottom_field: 0, // only for pic_order_cnt_type 1
            log2_max_pic_order_cnt_lsb_minus4: 4, // only for pic_order_cnt_type 0
            num_ref_frames_in_pic_order_cnt_cycle: 0, // only for pic_order_cnt_type 1
            max_num_ref_frames: max_references as u8,
            reserved1: 0,
            pic_width_in_mbs_minus1,
            pic_height_in_map_units_minus1,
            frame_crop_left_offset: 0,
            frame_crop_right_offset,
            frame_crop_top_offset: 0,
            frame_crop_bottom_offset,
            reserved2: 0,
            pOffsetForRefFrame: std::ptr::null(),
            pScalingLists: std::ptr::null(),
            pSequenceParameterSetVui: vui.as_ref(),
        };

        Ok(Self {
            sps,
            _scaling_lists: None,
            _offset_for_ref_frame: None,
            _vui: Some(vui),
        })
    }
}

unsafe impl Send for VkH264SequenceParameterSet {}
unsafe impl Sync for VkH264SequenceParameterSet {}

pub(crate) struct H264ScalingLists {
    pub(crate) list: vk::native::StdVideoH264ScalingLists,
}

impl Default for H264ScalingLists {
    fn default() -> Self {
        Self {
            list: vk::native::StdVideoH264ScalingLists {
                scaling_list_present_mask: !0,
                use_default_scaling_matrix_mask: 0,
                ScalingList4x4: [[0; 16]; 6],
                ScalingList8x8: [[0; 64]; 6],
            },
        }
    }
}

impl H264ScalingLists {
    fn insert_4x4(&mut self, list: &[h264_reader::nal::sps::ScalingList<16>]) {
        for (i, list) in list.iter().enumerate() {
            match list {
                h264_reader::nal::sps::ScalingList::NotPresent => {
                    self.list.scaling_list_present_mask &= !(1 << i)
                }
                h264_reader::nal::sps::ScalingList::UseDefault => {
                    self.list.use_default_scaling_matrix_mask |= 1 << i
                }
                h264_reader::nal::sps::ScalingList::List(l) => {
                    self.list.ScalingList4x4[i] = l.map(|n| n.get())
                }
            }
        }
    }

    fn insert_8x8(&mut self, list: &[h264_reader::nal::sps::ScalingList<64>]) {
        for (i, list) in list.iter().enumerate() {
            match list {
                h264_reader::nal::sps::ScalingList::NotPresent => {
                    self.list.scaling_list_present_mask &= !(1 << (i + 6))
                }
                h264_reader::nal::sps::ScalingList::UseDefault => {
                    self.list.use_default_scaling_matrix_mask |= 1 << (i + 6)
                }
                h264_reader::nal::sps::ScalingList::List(l) => {
                    self.list.ScalingList8x8[i] = l.map(|n| n.get())
                }
            }
        }
    }
}

impl From<&h264_reader::nal::sps::SeqScalingMatrix> for H264ScalingLists {
    fn from(value: &h264_reader::nal::sps::SeqScalingMatrix) -> Self {
        let mut result = H264ScalingLists::default();

        result.insert_4x4(&value.scaling_list4x4);
        result.insert_8x8(&value.scaling_list8x8);

        result
    }
}

impl From<&h264_reader::nal::pps::PicScalingMatrix> for H264ScalingLists {
    fn from(value: &h264_reader::nal::pps::PicScalingMatrix) -> Self {
        let mut result = H264ScalingLists::default();

        result.insert_4x4(&value.scaling_list4x4);

        if let Some(v) = &value.scaling_list8x8 {
            result.insert_8x8(v);
        }

        result
    }
}

trait ChromaFormatExt {
    fn to_chroma_format_idc(&self) -> u32;
}

impl ChromaFormatExt for h264_reader::nal::sps::ChromaFormat {
    fn to_chroma_format_idc(&self) -> u32 {
        match self {
            h264_reader::nal::sps::ChromaFormat::Monochrome => 0,
            h264_reader::nal::sps::ChromaFormat::YUV420 => 1,
            h264_reader::nal::sps::ChromaFormat::YUV422 => 2,
            h264_reader::nal::sps::ChromaFormat::YUV444 => 3,
            h264_reader::nal::sps::ChromaFormat::Invalid(v) => *v,
        }
    }
}

pub(crate) fn vk_to_h264_level_idc(
    level_idc: vk::native::StdVideoH264LevelIdc,
) -> Result<u8, VulkanDecoderError> {
    match level_idc {
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_0 => Ok(10),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_1 => Ok(11),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_2 => Ok(12),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_3 => Ok(13),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_0 => Ok(20),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_1 => Ok(21),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_2 => Ok(22),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_0 => Ok(30),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_1 => Ok(31),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_2 => Ok(32),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_0 => Ok(40),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_1 => Ok(41),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_2 => Ok(42),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_0 => Ok(50),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_1 => Ok(51),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_2 => Ok(52),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_0 => Ok(60),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_1 => Ok(61),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_2 => Ok(62),
        _ => Err(VulkanDecoderError::InvalidInputData(format!(
            "unknown StdVideoH264LevelIdc: {level_idc}"
        ))),
    }
}

/// As per __Table A-1 Level limits__ in the H.264 spec
/// `mbs` means macroblocks here
pub(crate) fn h264_level_idc_to_max_dpb_mbs(level_idc: u8) -> Result<u64, VulkanDecoderError> {
    match level_idc {
        10 => Ok(396),
        11 => Ok(900),
        12 => Ok(2_376),
        13 => Ok(2_376),
        20 => Ok(2_376),
        21 => Ok(4_752),
        22 => Ok(8_100),
        30 => Ok(8_100),
        31 => Ok(18_000),
        32 => Ok(20_480),
        40 => Ok(32_768),
        41 => Ok(32_768),
        42 => Ok(34_816),
        50 => Ok(110_400),
        51 => Ok(184_320),
        52 => Ok(184_320),
        60 => Ok(696_320),
        61 => Ok(696_320),
        62 => Ok(696_320),
        _ => Err(VulkanDecoderError::InvalidInputData(format!(
            "unknown h264 level_idc: {level_idc}"
        ))),
    }
}

fn h264_level_idc_to_vk(level_idc: u8) -> u32 {
    match level_idc {
        10 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_0,
        11 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_1,
        12 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_2,
        13 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_3,
        20 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_0,
        21 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_1,
        22 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_2,
        30 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_0,
        31 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_1,
        32 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_2,
        40 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_0,
        41 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_1,
        42 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_2,
        50 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_0,
        51 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_1,
        52 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_2,
        60 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_0,
        61 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_1,
        62 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_2,
        _ => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_INVALID,
    }
}

fn h264_profile_idc_to_vk(
    profile: h264_reader::nal::sps::Profile,
) -> vk::native::StdVideoH264ProfileIdc {
    match profile {
        h264_reader::nal::sps::Profile::Baseline => {
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_BASELINE
        }
        h264_reader::nal::sps::Profile::Main => {
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN
        }
        h264_reader::nal::sps::Profile::High => {
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH
        }
        h264_reader::nal::sps::Profile::High444 => {
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH_444_PREDICTIVE
        }
        h264_reader::nal::sps::Profile::High422
        | h264_reader::nal::sps::Profile::High10
        | h264_reader::nal::sps::Profile::Extended
        | h264_reader::nal::sps::Profile::ScalableBase
        | h264_reader::nal::sps::Profile::ScalableHigh
        | h264_reader::nal::sps::Profile::MultiviewHigh
        | h264_reader::nal::sps::Profile::StereoHigh
        | h264_reader::nal::sps::Profile::MFCDepthHigh
        | h264_reader::nal::sps::Profile::MultiviewDepthHigh
        | h264_reader::nal::sps::Profile::EnhancedMultiviewDepthHigh
        | h264_reader::nal::sps::Profile::Unknown(_) => {
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_INVALID
        }
    }
}

pub(crate) struct VkH264PictureParameterSet {
    pub(crate) pps: vk::native::StdVideoH264PictureParameterSet,
    _scaling_list: Option<Box<H264ScalingLists>>,
}

impl From<&'_ h264_reader::nal::pps::PicParameterSet> for VkH264PictureParameterSet {
    #[allow(non_snake_case)]
    fn from(pps: &h264_reader::nal::pps::PicParameterSet) -> Self {
        let flags = vk::native::StdVideoH264PpsFlags {
            _bitfield_align_1: [],
            __bindgen_padding_0: [0; 3],
            _bitfield_1: vk::native::StdVideoH264PpsFlags::new_bitfield_1(
                pps.extension
                    .as_ref()
                    .map(|ext| ext.transform_8x8_mode_flag.into())
                    .unwrap_or(0),
                pps.redundant_pic_cnt_present_flag.into(),
                pps.constrained_intra_pred_flag.into(),
                pps.deblocking_filter_control_present_flag.into(),
                pps.weighted_pred_flag.into(),
                pps.bottom_field_pic_order_in_frame_present_flag.into(),
                pps.entropy_coding_mode_flag.into(),
                pps.extension
                    .as_ref()
                    .map(|ext| ext.pic_scaling_matrix.is_some().into())
                    .unwrap_or(0),
            ),
        };

        let chroma_qp_index_offset = pps.chroma_qp_index_offset as i8;

        let second_chroma_qp_index_offset = pps
            .extension
            .as_ref()
            .map(|ext| ext.second_chroma_qp_index_offset as i8)
            .unwrap_or(chroma_qp_index_offset);

        let scaling_list: Option<Box<H264ScalingLists>> = pps
            .extension
            .as_ref()
            .and_then(|e| e.pic_scaling_matrix.as_ref())
            .map(|matrix| Box::new(matrix.into()));

        let pScalingLists = match scaling_list.as_ref() {
            Some(l) => &l.list,
            None => std::ptr::null(),
        };

        Self {
            pps: vk::native::StdVideoH264PictureParameterSet {
                flags,
                seq_parameter_set_id: pps.seq_parameter_set_id.id(),
                pic_parameter_set_id: pps.pic_parameter_set_id.id(),
                num_ref_idx_l0_default_active_minus1: pps.num_ref_idx_l0_default_active_minus1
                    as u8,
                num_ref_idx_l1_default_active_minus1: pps.num_ref_idx_l1_default_active_minus1
                    as u8,
                weighted_bipred_idc: pps.weighted_bipred_idc.into(),
                pic_init_qp_minus26: pps.pic_init_qp_minus26 as i8,
                pic_init_qs_minus26: pps.pic_init_qs_minus26 as i8,
                chroma_qp_index_offset,
                second_chroma_qp_index_offset,
                pScalingLists,
            },
            _scaling_list: scaling_list,
        }
    }
}

unsafe impl Send for VkH264PictureParameterSet {}
unsafe impl Sync for VkH264PictureParameterSet {}

impl VkH264PictureParameterSet {
    pub(crate) fn new_encode(
        caps: &vk::VideoEncodeH264CapabilitiesKHR<'_>,
        profile: H264Profile,
    ) -> Self {
        let transform_8x8_mode_flag = (caps
            .std_syntax_flags
            .contains(vk::VideoEncodeH264StdFlagsKHR::TRANSFORM_8X8_MODE_FLAG_SET)
            && matches!(profile, H264Profile::High)) as u32;

        let pps = vk::native::StdVideoH264PictureParameterSet {
            flags: vk::native::StdVideoH264PpsFlags {
                __bindgen_padding_0: [0; 3],
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoH264PpsFlags::new_bitfield_1(
                    transform_8x8_mode_flag,
                    0,
                    0,
                    1, // maybe turn off to enable superfast decoding
                    0, // think about this -- think really hard, it seems this
                    // means you need to supply the weights yourself
                    0,
                    1,
                    0,
                ),
            },
            seq_parameter_set_id: 0,
            pic_parameter_set_id: 0,
            num_ref_idx_l0_default_active_minus1: 0,
            num_ref_idx_l1_default_active_minus1: 0,
            weighted_bipred_idc:
                vk::native::StdVideoH264WeightedBipredIdc_STD_VIDEO_H264_WEIGHTED_BIPRED_IDC_DEFAULT, // for b frames
            pic_init_qp_minus26: 0, // no idea what this is, ffmpeg sets this to -4, BBB has 0
            pic_init_qs_minus26: 0, // no idea what this is, ffmpeg sets this to 0, BBB has 0
            chroma_qp_index_offset: 0, // no idea what this is, ffmpeg sets this to 0, BBB has 0
            second_chroma_qp_index_offset: 0, // no idea what this is, ffmpeg sets this to 0, BBB has 0
            pScalingLists: std::ptr::null(),
        };

        Self {
            pps,
            _scaling_list: None,
        }
    }
}

/// Color description for H.264 VUI parameters.
#[derive(Debug, Clone, Copy)]
pub(crate) struct H264ColorDescription {
    pub colour_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
}

impl From<ColorSpace> for H264ColorDescription {
    fn from(color_space: ColorSpace) -> Self {
        // Values correspond to ITU-T H.264 Tables E-3, E-4, E-5.
        match color_space {
            ColorSpace::Unspecified => Self {
                colour_primaries: 2,
                transfer_characteristics: 2,
                matrix_coefficients: 2,
            },
            ColorSpace::BT709 => Self {
                colour_primaries: 1,
                transfer_characteristics: 1,
                matrix_coefficients: 1,
            },
            ColorSpace::BT601Ntsc => Self {
                colour_primaries: 6,
                transfer_characteristics: 6,
                matrix_coefficients: 6,
            },
            ColorSpace::BT601Pal => Self {
                colour_primaries: 5,
                transfer_characteristics: 6,
                matrix_coefficients: 5,
            },
        }
    }
}

pub(crate) struct H264DecodeProfileInfo<'a> {
    pub(crate) profile_info: ProfileInfo<'a>,
    pub(crate) profile_idc: vk::native::StdVideoH264ProfileIdc,
    pub(crate) picture_layout: vk::VideoDecodeH264PictureLayoutFlagsKHR,
}

impl PartialEq for H264DecodeProfileInfo<'_> {
    fn eq(&self, other: &Self) -> bool {
        other.profile_info.profile_info.chroma_subsampling
            == self.profile_info.profile_info.chroma_subsampling
            && other.profile_info.profile_info.luma_bit_depth
                == self.profile_info.profile_info.luma_bit_depth
            && other.profile_info.profile_info.chroma_bit_depth
                == self.profile_info.profile_info.chroma_bit_depth
            && other.profile_idc == self.profile_idc
            && other.picture_layout == self.picture_layout
    }
}

impl Eq for H264DecodeProfileInfo<'_> {}

impl<'a> H264DecodeProfileInfo<'a> {
    pub(crate) fn from_sps_decode(
        sps: &SeqParameterSet,
        decode_usage_info: vk::VideoDecodeUsageInfoKHR<'a>,
    ) -> Result<Self, VulkanDecoderError> {
        let profile_idc = h264_profile_idc_to_vk(sps.profile());

        if profile_idc == vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_INVALID {
            return Err(VulkanDecoderError::InvalidInputData(
                "unsupported h264 profile".into(),
            ));
        }

        let picture_layout = vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE;

        let h264_profile_info = vk::VideoDecodeH264ProfileInfoKHR::default()
            .std_profile_idc(profile_idc)
            .picture_layout(picture_layout);

        let h264_profile_info: Box<dyn vk::ExtendsVideoProfileInfoKHR + Send + Sync> =
            Box::new(h264_profile_info);
        let decode_usage_info: Box<dyn vk::ExtendsVideoProfileInfoKHR + Send + Sync> =
            Box::new(decode_usage_info);

        let chroma_subsampling = match sps.chroma_info.chroma_format {
            h264_reader::nal::sps::ChromaFormat::YUV420 => {
                vk::VideoChromaSubsamplingFlagsKHR::TYPE_420
            }
            h264_reader::nal::sps::ChromaFormat::Monochrome
            | h264_reader::nal::sps::ChromaFormat::YUV422
            | h264_reader::nal::sps::ChromaFormat::YUV444
            | h264_reader::nal::sps::ChromaFormat::Invalid(_) => {
                return Err(VulkanDecoderError::InvalidInputData(format!(
                    "unsupported chroma format: {:?}",
                    sps.chroma_info.chroma_format
                )));
            }
        };

        let luma_bit_depth = if sps.chroma_info.bit_depth_luma_minus8 + 8 == 8 {
            vk::VideoComponentBitDepthFlagsKHR::TYPE_8
        } else {
            return Err(VulkanDecoderError::InvalidInputData(format!(
                "unsupported luma bit length: {}",
                sps.chroma_info.bit_depth_luma_minus8 + 8
            )));
        };

        let chroma_bit_depth = if sps.chroma_info.bit_depth_chroma_minus8 + 8 == 8 {
            vk::VideoComponentBitDepthFlagsKHR::TYPE_8
        } else {
            return Err(VulkanDecoderError::InvalidInputData(format!(
                "unsupported chroma bit length: {}",
                sps.chroma_info.bit_depth_chroma_minus8 + 8
            )));
        };

        let profile_info = vk::VideoProfileInfoKHR::default()
            .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
            .chroma_subsampling(chroma_subsampling)
            .luma_bit_depth(luma_bit_depth)
            .chroma_bit_depth(chroma_bit_depth);

        Ok(Self {
            profile_info: ProfileInfo::new(
                profile_info,
                vec![h264_profile_info, decode_usage_info],
            ),
            profile_idc,
            picture_layout,
        })
    }
}
