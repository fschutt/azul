use std::ptr::null_mut;

use ash::vk;

use crate::{
    codec::{
        Codec, CodecCapabilities, CodecSpecificDecodeCapabilities, CodecSpecificEncodeCapabilities,
        CodecSpecificEncoderQualityLevelProperties,
        h265::parameters::{
            VkH265PictureParameterSet, VkH265SequenceParameterSet, VkH265VideoParameterSet,
        },
    },
    device::caps::NativeEncodeH265Capabilities,
    parameters::H265Profile,
};

pub(crate) mod encode;
pub(crate) mod parameters;

pub(crate) struct H265CodecParameters {
    pub(crate) vps: Vec<VkH265VideoParameterSet>,
    pub(crate) sps: Vec<VkH265SequenceParameterSet>,
    pub(crate) pps: Vec<VkH265PictureParameterSet>,
}

pub(crate) struct H265VkParameters {
    pub(crate) vps: Vec<vk::native::StdVideoH265VideoParameterSet>,
    pub(crate) sps: Vec<vk::native::StdVideoH265SequenceParameterSet>,
    pub(crate) pps: Vec<vk::native::StdVideoH265PictureParameterSet>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct H265Codec;

impl Codec for H265Codec {
    type Profile = H265Profile;

    type OwnedParameters = H265CodecParameters;

    type VkParameters<'a> = H265VkParameters;

    type VideoDecodeSessionParametersAddInfo<'a> =
        vk::VideoDecodeH265SessionParametersAddInfoKHR<'a>;
    type VideoDecodeSessionParametersCreateInfo<'a> =
        vk::VideoDecodeH265SessionParametersCreateInfoKHR<'a>;

    type VideoEncodeSessionParametersAddInfo<'a> =
        vk::VideoEncodeH265SessionParametersAddInfoKHR<'a>;
    type VideoEncodeSessionParametersCreateInfo<'a> =
        vk::VideoEncodeH265SessionParametersCreateInfoKHR<'a>;

    fn decode_parameters_add_info<'a: 'b, 'b>(
        parameters: &'b Self::VkParameters<'a>,
    ) -> Self::VideoDecodeSessionParametersAddInfo<'b> {
        vk::VideoDecodeH265SessionParametersAddInfoKHR::default()
            .std_vp_ss(&parameters.vps)
            .std_sp_ss(&parameters.sps)
            .std_pp_ss(&parameters.pps)
    }

    fn decode_parameters_create_info<'a: 'b, 'b>(
        add_info: &'b Self::VideoDecodeSessionParametersAddInfo<'a>,
    ) -> Self::VideoDecodeSessionParametersCreateInfo<'b> {
        vk::VideoDecodeH265SessionParametersCreateInfoKHR::default()
            .max_std_vps_count(32)
            .max_std_sps_count(32)
            .max_std_pps_count(32)
            .parameters_add_info(add_info)
    }

    fn encode_parameters_add_info<'a: 'b, 'b>(
        parameters: &'b Self::VkParameters<'a>,
    ) -> Self::VideoEncodeSessionParametersAddInfo<'b> {
        vk::VideoEncodeH265SessionParametersAddInfoKHR::default()
            .std_vp_ss(&parameters.vps)
            .std_sp_ss(&parameters.sps)
            .std_pp_ss(&parameters.pps)
    }

    fn encode_parameters_create_info<'a: 'b, 'b>(
        add_info: &'b Self::VideoEncodeSessionParametersAddInfo<'a>,
    ) -> Self::VideoEncodeSessionParametersCreateInfo<'b> {
        vk::VideoEncodeH265SessionParametersCreateInfoKHR::default()
            .max_std_vps_count(32)
            .max_std_sps_count(32)
            .max_std_pps_count(32)
            .parameters_add_info(add_info)
    }
}

impl CodecCapabilities for H265Codec {
    type CodecSpecificDecodeCapabilities<'a> = vk::VideoDecodeH265CapabilitiesKHR<'a>;
    type CodecSpecificEncodeCapabilities<'a> = vk::VideoEncodeH265CapabilitiesKHR<'a>;
    type CodecSpecificEncodeQualityLevelProperties<'a> =
        vk::VideoEncodeH265QualityLevelPropertiesKHR<'a>;
    type NativeEncodeCodecCapabilities = NativeEncodeH265Capabilities;

    fn static_decode_capabilities<'a>(
        codec_caps: &Self::CodecSpecificDecodeCapabilities<'a>,
    ) -> Self::CodecSpecificDecodeCapabilities<'static> {
        vk::VideoDecodeH265CapabilitiesKHR {
            p_next: null_mut(),
            _marker: Default::default(),
            ..*codec_caps
        }
    }

    fn static_encode_capabilities<'a>(
        codec_caps: &Self::CodecSpecificEncodeCapabilities<'a>,
    ) -> Self::CodecSpecificEncodeCapabilities<'static> {
        vk::VideoEncodeH265CapabilitiesKHR {
            p_next: null_mut(),
            _marker: Default::default(),
            ..*codec_caps
        }
    }

    fn static_encode_qlp<'a>(
        codec_qlp: &Self::CodecSpecificEncodeQualityLevelProperties<'a>,
    ) -> Self::CodecSpecificEncodeQualityLevelProperties<'static> {
        vk::VideoEncodeH265QualityLevelPropertiesKHR {
            p_next: null_mut(),
            _marker: Default::default(),
            ..*codec_qlp
        }
    }

    fn encode_codec_capabilities(
        capabilities: &crate::device::caps::NativeEncodeCapabilities,
    ) -> Option<&Self::NativeEncodeCodecCapabilities> {
        capabilities.h265.as_ref()
    }
}

impl<'a> CodecSpecificDecodeCapabilities for vk::VideoDecodeH265CapabilitiesKHR<'a> {}
impl<'a> CodecSpecificEncodeCapabilities for vk::VideoEncodeH265CapabilitiesKHR<'a> {}
impl<'a> CodecSpecificEncoderQualityLevelProperties
    for vk::VideoEncodeH265QualityLevelPropertiesKHR<'a>
{
    fn zeroed(&self) -> bool {
        self.preferred_rate_control_flags.as_raw() == 0
            && self.preferred_gop_frame_count == 0
            && self.preferred_idr_period == 0
            && self.preferred_consecutive_b_frame_count == 0
            && self.preferred_sub_layer_count == 0
            && self.preferred_constant_qp.qp_i == 0
            && self.preferred_constant_qp.qp_p == 0
            && self.preferred_constant_qp.qp_b == 0
            && self.preferred_max_l0_reference_count == 0
            && self.preferred_max_l1_reference_count == 0
    }
}
