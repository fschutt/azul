use std::{collections::VecDeque, num::NonZeroU32};

use ash::vk;

use crate::{
    VulkanEncoderError,
    device::caps::{
        NativeEncodeCapabilities, NativeEncodeProfileCapabilities,
        NativeEncodeQualityLevelProperties,
    },
    parameters::RateControl,
    vulkan_encoder::FullEncoderParameters,
    wrappers::ProfileInfo,
};

pub(crate) mod h264;
pub(crate) mod h265;

pub(crate) trait EncodeCodec: Codec {
    fn profile_info<'a>(params: &FullEncoderParameters<Self>) -> ProfileInfo<'a>;
    fn encode_profile_capabilities(
        caps: &Self::NativeEncodeCodecCapabilities,
        profile: Self::Profile,
    ) -> Option<&NativeEncodeProfileCapabilities<Self>>;
    fn encode_codec_profile_capabilities(
        caps: &NativeEncodeCapabilities,
        profile: Self::Profile,
    ) -> Result<&NativeEncodeProfileCapabilities<Self>, VulkanEncoderError> {
        let codec_caps = Self::encode_codec_capabilities(caps)
            .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?;
        Self::encode_profile_capabilities(codec_caps, profile)
            .ok_or_else(|| VulkanEncoderError::ProfileUnsupported(format!("{profile:?}")))
    }
    fn codec_parameters(
        parameters: &FullEncoderParameters<Self>,
        codec_capabilities: &Self::CodecSpecificEncodeCapabilities<'_>,
    ) -> Result<Self::OwnedParameters, VulkanEncoderError>;
    fn vk_parameters<'a>(parameters: &'a Self::OwnedParameters) -> Self::VkParameters<'a>;

    type BitstreamUnitData;
    fn bitstream_unit_data(
        codec_capabilities: &Self::CodecSpecificEncodeCapabilities<'_>,
        is_idr: bool,
    ) -> Self::BitstreamUnitData;
    type BitstreamUnitInfo<'a>;
    fn bitstream_unit_info<'a>(
        data: &'a Self::BitstreamUnitData,
        rate_control: RateControl,
        capabilities: &NativeEncodeQualityLevelProperties<Self>,
        is_idr: bool,
    ) -> Self::BitstreamUnitInfo<'a>;

    type ReferenceInfo: Copy + 'static;
    type ReferenceListInfo;
    fn reference_list_info(
        counters: &Self::EncodingCounters,
        active_reference_slots: &VecDeque<(usize, Self::ReferenceInfo)>,
    ) -> Self::ReferenceListInfo;
    fn new_slot_reference_info(
        counters: &Self::EncodingCounters,
        is_idr: bool,
    ) -> Self::ReferenceInfo;

    type PictureInfoData;
    fn picture_info_data(
        counters: &Self::EncodingCounters,
        codec_capabilities: &Self::CodecSpecificEncodeCapabilities<'_>,
        is_idr: bool,
        ref_lists: &Self::ReferenceListInfo,
    ) -> Self::PictureInfoData;
    type PictureInfo<'a>: vk::ExtendsVideoEncodeInfoKHR;
    fn picture_info<'a, 'b: 'a>(
        data: &'a Self::PictureInfoData,
        bitstream_unit_infos: &'a [Self::BitstreamUnitInfo<'b>],
    ) -> Self::PictureInfo<'a>;

    type DpbSlotInfo<'a>: vk::ExtendsVideoReferenceSlotInfoKHR;
    fn dpb_slot_info<'a>(reference_info: &'a Self::ReferenceInfo) -> Self::DpbSlotInfo<'a>;
    fn new_slot_dpb_slot_info<'a>(
        reference_info: &'a Self::ReferenceInfo,
    ) -> Self::DpbSlotInfo<'a> {
        Self::dpb_slot_info(reference_info)
    }

    type EncodingCounters: Default + Clone + Copy;
    fn advance_counters(counters: &mut Self::EncodingCounters, is_idr: bool);
    fn counters_idr(counters: &mut Self::EncodingCounters);

    type CodecRateControlLayerInfo<'a>: vk::ExtendsVideoEncodeRateControlLayerInfoKHR;
    type CodecRateControlInfo<'a>: vk::ExtendsVideoBeginCodingInfoKHR
        + vk::ExtendsVideoCodingControlInfoKHR;
    fn codec_rate_control_layer_info<'a>(
        rate_control: RateControl,
    ) -> Option<Vec<Self::CodecRateControlLayerInfo<'a>>>;
    fn codec_rate_control_info<'a>(
        layers: Option<&'a [vk::VideoEncodeRateControlLayerInfoKHR<'a>]>,
        idr_period: u32,
    ) -> Option<Self::CodecRateControlInfo<'a>>;

    type CodecWriteParametersInfo: Copy;
    type CodecEncodeSessionParametersGetInfo<'a>: vk::ExtendsVideoEncodeSessionParametersGetInfoKHR;
    fn codec_session_parameters_get_info<'a>(
        info: Self::CodecWriteParametersInfo,
    ) -> Self::CodecEncodeSessionParametersGetInfo<'a>;
    fn codec_write_parameters_info_all() -> Self::CodecWriteParametersInfo;

    fn resolve_idr_period<'a>(
        quality_level_properties: &Self::CodecSpecificEncodeQualityLevelProperties<'a>,
        user_provided: Option<NonZeroU32>,
    ) -> NonZeroU32;

    fn resolve_max_references<'a>(
        quality_level_properties: &Self::CodecSpecificEncodeQualityLevelProperties<'a>,
        codec_capabilities: &Self::CodecSpecificEncodeCapabilities<'a>,
        user_provided: Option<NonZeroU32>,
    ) -> NonZeroU32;
}

pub(crate) trait Codec: CodecCapabilities + std::fmt::Debug + Clone {
    type Profile: Copy + std::fmt::Debug;

    // Parameters
    type OwnedParameters;
    type VkParameters<'a>;

    type VideoDecodeSessionParametersAddInfo<'a>;
    type VideoDecodeSessionParametersCreateInfo<'a>: vk::ExtendsVideoSessionParametersCreateInfoKHR;

    type VideoEncodeSessionParametersAddInfo<'a>;
    type VideoEncodeSessionParametersCreateInfo<'a>: vk::ExtendsVideoSessionParametersCreateInfoKHR;

    fn decode_parameters_add_info<'a: 'b, 'b>(
        parameters: &'b Self::VkParameters<'a>,
    ) -> Self::VideoDecodeSessionParametersAddInfo<'b>;
    fn decode_parameters_create_info<'a: 'b, 'b>(
        add_info: &'b Self::VideoDecodeSessionParametersAddInfo<'a>,
    ) -> Self::VideoDecodeSessionParametersCreateInfo<'b>;

    fn encode_parameters_add_info<'a: 'b, 'b>(
        parameters: &'b Self::VkParameters<'a>,
    ) -> Self::VideoEncodeSessionParametersAddInfo<'b>;
    fn encode_parameters_create_info<'a: 'b, 'b>(
        add_info: &'b Self::VideoEncodeSessionParametersAddInfo<'a>,
    ) -> Self::VideoEncodeSessionParametersCreateInfo<'b>;
}

pub(crate) trait CodecCapabilities: std::fmt::Debug + Clone {
    type CodecSpecificDecodeCapabilities<'a>: CodecSpecificDecodeCapabilities;
    type CodecSpecificEncodeCapabilities<'a>: CodecSpecificEncodeCapabilities;
    type CodecSpecificEncodeQualityLevelProperties<'a>: CodecSpecificEncoderQualityLevelProperties;
    type NativeEncodeCodecCapabilities;

    fn static_decode_capabilities<'a>(
        codec_caps: &Self::CodecSpecificDecodeCapabilities<'a>,
    ) -> Self::CodecSpecificDecodeCapabilities<'static>;
    fn static_encode_capabilities<'a>(
        codec_caps: &Self::CodecSpecificEncodeCapabilities<'a>,
    ) -> Self::CodecSpecificEncodeCapabilities<'static>;
    fn static_encode_qlp<'a>(
        codec_qlp: &Self::CodecSpecificEncodeQualityLevelProperties<'a>,
    ) -> Self::CodecSpecificEncodeQualityLevelProperties<'static>;
    fn encode_codec_capabilities(
        capabilities: &NativeEncodeCapabilities,
    ) -> Option<&Self::NativeEncodeCodecCapabilities>;
}

pub(crate) trait CodecSpecificDecodeCapabilities:
    std::fmt::Debug + Clone + Default + vk::ExtendsVideoCapabilitiesKHR
{
}

pub(crate) trait CodecSpecificEncodeCapabilities:
    std::fmt::Debug + Clone + Default + vk::ExtendsVideoCapabilitiesKHR
{
}

pub(crate) trait CodecSpecificEncoderQualityLevelProperties:
    std::fmt::Debug + Clone + Default + vk::ExtendsVideoEncodeQualityLevelPropertiesKHR
{
    fn zeroed(&self) -> bool;
}
