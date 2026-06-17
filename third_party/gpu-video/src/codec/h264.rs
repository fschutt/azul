use std::ptr::null_mut;

use ash::vk;

use crate::{
    codec::{
        Codec, CodecCapabilities, CodecSpecificDecodeCapabilities, CodecSpecificEncodeCapabilities,
        CodecSpecificEncoderQualityLevelProperties,
        h264::parameters::{VkH264PictureParameterSet, VkH264SequenceParameterSet},
    },
    device::caps::NativeEncodeH264Capabilities,
    parameters::H264Profile,
};

pub(crate) mod encode;
pub(crate) mod parameters;

#[derive(Debug, Clone, Copy)]
pub(crate) struct H264Codec;

pub(crate) struct H264CodecParameters {
    pub(crate) sps: Vec<VkH264SequenceParameterSet>,
    pub(crate) pps: Vec<VkH264PictureParameterSet>,
}

pub(crate) struct H264VkParameters {
    pub(crate) sps: Vec<vk::native::StdVideoH264SequenceParameterSet>,
    pub(crate) pps: Vec<vk::native::StdVideoH264PictureParameterSet>,
}

impl Codec for H264Codec {
    type Profile = H264Profile;

    type OwnedParameters = H264CodecParameters;
    type VkParameters<'a> = H264VkParameters;

    type VideoDecodeSessionParametersAddInfo<'a> =
        vk::VideoDecodeH264SessionParametersAddInfoKHR<'a>;
    type VideoDecodeSessionParametersCreateInfo<'a> =
        vk::VideoDecodeH264SessionParametersCreateInfoKHR<'a>;

    type VideoEncodeSessionParametersAddInfo<'a> =
        vk::VideoEncodeH264SessionParametersAddInfoKHR<'a>;
    type VideoEncodeSessionParametersCreateInfo<'a> =
        vk::VideoEncodeH264SessionParametersCreateInfoKHR<'a>;

    fn decode_parameters_add_info<'a: 'b, 'b>(
        parameters: &'b Self::VkParameters<'a>,
    ) -> Self::VideoDecodeSessionParametersAddInfo<'b> {
        vk::VideoDecodeH264SessionParametersAddInfoKHR::default()
            .std_sp_ss(&parameters.sps)
            .std_pp_ss(&parameters.pps)
    }

    fn decode_parameters_create_info<'a: 'b, 'b>(
        add_info: &'b Self::VideoDecodeSessionParametersAddInfo<'a>,
    ) -> Self::VideoDecodeSessionParametersCreateInfo<'b> {
        vk::VideoDecodeH264SessionParametersCreateInfoKHR::default()
            .max_std_sps_count(32)
            .max_std_pps_count(32)
            .parameters_add_info(add_info)
    }

    fn encode_parameters_add_info<'a: 'b, 'b>(
        parameters: &'b Self::VkParameters<'a>,
    ) -> Self::VideoEncodeSessionParametersAddInfo<'b> {
        vk::VideoEncodeH264SessionParametersAddInfoKHR::default()
            .std_sp_ss(&parameters.sps)
            .std_pp_ss(&parameters.pps)
    }

    fn encode_parameters_create_info<'a: 'b, 'b>(
        add_info: &'b Self::VideoEncodeSessionParametersAddInfo<'a>,
    ) -> Self::VideoEncodeSessionParametersCreateInfo<'b> {
        vk::VideoEncodeH264SessionParametersCreateInfoKHR::default()
            .max_std_sps_count(32)
            .max_std_pps_count(32)
            .parameters_add_info(add_info)
    }
}

impl CodecCapabilities for H264Codec {
    type CodecSpecificDecodeCapabilities<'a> = vk::VideoDecodeH264CapabilitiesKHR<'a>;
    type CodecSpecificEncodeCapabilities<'a> = vk::VideoEncodeH264CapabilitiesKHR<'a>;
    type CodecSpecificEncodeQualityLevelProperties<'a> =
        vk::VideoEncodeH264QualityLevelPropertiesKHR<'a>;
    type NativeEncodeCodecCapabilities = NativeEncodeH264Capabilities;

    fn static_decode_capabilities<'a>(
        codec_caps: &Self::CodecSpecificDecodeCapabilities<'a>,
    ) -> Self::CodecSpecificDecodeCapabilities<'static> {
        vk::VideoDecodeH264CapabilitiesKHR {
            p_next: null_mut(),
            _marker: Default::default(),
            ..*codec_caps
        }
    }

    fn static_encode_capabilities<'a>(
        codec_caps: &Self::CodecSpecificEncodeCapabilities<'a>,
    ) -> Self::CodecSpecificEncodeCapabilities<'static> {
        vk::VideoEncodeH264CapabilitiesKHR {
            p_next: null_mut(),
            _marker: Default::default(),
            ..*codec_caps
        }
    }

    fn static_encode_qlp<'a>(
        codec_qlp: &Self::CodecSpecificEncodeQualityLevelProperties<'a>,
    ) -> Self::CodecSpecificEncodeQualityLevelProperties<'static> {
        vk::VideoEncodeH264QualityLevelPropertiesKHR {
            p_next: null_mut(),
            _marker: Default::default(),
            ..*codec_qlp
        }
    }

    fn encode_codec_capabilities(
        capabilities: &crate::device::caps::NativeEncodeCapabilities,
    ) -> Option<&Self::NativeEncodeCodecCapabilities> {
        capabilities.h264.as_ref()
    }
}

impl<'a> CodecSpecificDecodeCapabilities for vk::VideoDecodeH264CapabilitiesKHR<'a> {}
impl<'a> CodecSpecificEncodeCapabilities for vk::VideoEncodeH264CapabilitiesKHR<'a> {}
impl<'a> CodecSpecificEncoderQualityLevelProperties
    for vk::VideoEncodeH264QualityLevelPropertiesKHR<'a>
{
    fn zeroed(&self) -> bool {
        self.preferred_rate_control_flags.as_raw() == 0
            && self.preferred_gop_frame_count == 0
            && self.preferred_idr_period == 0
            && self.preferred_consecutive_b_frame_count == 0
            && self.preferred_temporal_layer_count == 0
            && self.preferred_constant_qp.qp_i == 0
            && self.preferred_constant_qp.qp_p == 0
            && self.preferred_constant_qp.qp_b == 0
            && self.preferred_max_l0_reference_count == 0
            && self.preferred_max_l1_reference_count == 0
            && self.preferred_std_entropy_coding_mode_flag == 0
    }
}
