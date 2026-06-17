use std::ffi::CStr;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::sync::Arc;

use ash::vk;

use crate::adapter::VulkanAdapter;
use crate::capabilities::AdapterInfo;
use crate::codec::EncodeCodec;
use crate::codec::h264::H264Codec;
use crate::device::caps::{
    DecodeCapabilities, EncodeCapabilities, NativeDecodeCapabilities,
    NativeDecodeProfileCapabilities, NativeEncodeCapabilities,
};
use crate::device::queues::{Queue, QueueIndex, Queues, VideoQueues};
use crate::parameters::{
    EncoderContentFlags, EncoderTuningMode, EncoderUsageFlags, H264Profile, H265Profile,
    RateControl,
};
use crate::parser::{h264::H264Parser, reference_manager::ReferenceContext};
use crate::vulkan_decoder::{FrameSorter, ImageModifiers, VulkanDecoder};
use crate::vulkan_encoder::{FullEncoderParameters, VulkanEncoder};
#[cfg(feature = "transcoder")]
use crate::vulkan_transcoder::TranscoderParameters;
use crate::{
    BytesDecoder, BytesEncoderH264, BytesEncoderH265, DecoderError, RawFrameData,
    VulkanDecoderError, VulkanEncoderError, VulkanInitError, VulkanInstance, wrappers::*,
};

pub(crate) mod caps;
pub(crate) mod queues;

#[cfg(feature = "wgpu")]
mod wgpu_api;
#[cfg(feature = "wgpu")]
pub(crate) use wgpu_api::*;

pub(crate) const REQUIRED_EXTENSIONS: &[&CStr] =
    &[vk::KHR_VIDEO_QUEUE_NAME, vk::KHR_VIDEO_MAINTENANCE1_NAME];

pub(crate) const DECODE_EXTENSIONS: &[&CStr] = &[vk::KHR_VIDEO_DECODE_QUEUE_NAME];

pub(crate) const DECODE_CODEC_EXTENSIONS: &[&CStr] = &[
    vk::KHR_VIDEO_DECODE_H264_NAME,
    vk::KHR_VIDEO_DECODE_H265_NAME,
];

pub(crate) const ENCODE_EXTENSIONS: &[&CStr] = &[vk::KHR_VIDEO_ENCODE_QUEUE_NAME];

pub(crate) const ENCODE_CODEC_EXTENSIONS: &[&CStr] = &[
    vk::KHR_VIDEO_ENCODE_H264_NAME,
    vk::KHR_VIDEO_ENCODE_H265_NAME,
];

/// Describes a [`VulkanDevice`].
/// Used by [`VulkanAdapter::create_device`]
#[derive(Default, Clone)]
pub struct VulkanDeviceDescriptor {
    #[cfg(feature = "wgpu")]
    pub wgpu_features: wgpu::Features,

    #[cfg(feature = "wgpu")]
    pub wgpu_experimental_features: wgpu::ExperimentalFeatures,

    #[cfg(feature = "wgpu")]
    pub wgpu_limits: wgpu::Limits,
}

/// A fraction
#[derive(Debug, Clone, Copy)]
pub struct Rational {
    pub numerator: u32,
    pub denominator: NonZeroU32,
}

impl From<u32> for Rational {
    fn from(value: u32) -> Self {
        Rational {
            numerator: value,
            denominator: std::num::NonZeroU32::new(1).unwrap(),
        }
    }
}

/// An enum used to specify how the decoder should handle missing frames
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissedFrameHandling {
    /// When missed frames are detected, error on every subsequent frame that depends on them
    /// (i. e. fail on every frame until an IDR frame arrives)
    #[default]
    Strict,

    /// When missed frames are detected, try to decode later frames that depend on them anyway.
    /// This can produce decoded frames with very visible artifacts.
    Tolerant,
}

/// Parameters for decoder creation
#[derive(Debug, Default, Clone, Copy)]
pub struct DecoderParameters {
    /// See [`MissedFrameHandling`] for description of different handling approaches.
    ///
    /// **Defaults to [`MissedFrameHandling::Strict`]**
    pub missed_frame_handling: MissedFrameHandling,

    /// A hint indicating what kind of content the decoder is going to be used for.
    ///
    /// Multiple flags can be combined using the `|` operator to indicate multiple usages.
    pub usage_flags: crate::parameters::DecoderUsageFlags,
}

/// Things the encoder needs to know about the video
#[derive(Debug, Clone, Copy)]
pub struct VideoParameters {
    pub width: NonZeroU32,
    pub height: NonZeroU32,
    /// The expected/approximate framerate of the encoded video
    pub target_framerate: Rational,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorSpace {
    #[default]
    Unspecified,
    BT709,
    BT601Ntsc,
    BT601Pal,
}

impl From<&h264_reader::nal::sps::SeqParameterSet> for ColorSpace {
    fn from(sps: &h264_reader::nal::sps::SeqParameterSet) -> Self {
        let Some(vui) = &sps.vui_parameters else {
            return ColorSpace::Unspecified;
        };
        let Some(vst) = &vui.video_signal_type else {
            return ColorSpace::Unspecified;
        };
        let Some(cd) = &vst.colour_description else {
            return ColorSpace::Unspecified;
        };

        match (
            cd.colour_primaries,
            cd.transfer_characteristics,
            cd.matrix_coefficients,
        ) {
            (1, 1, 1) => ColorSpace::BT709,
            (6, 6, 6) => ColorSpace::BT601Ntsc,
            (5, 6, 5) => ColorSpace::BT601Pal,
            _ => ColorSpace::Unspecified,
        }
    }
}

/// Whether the video signal uses the full or limited range of sample values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorRange {
    /// Luma and chroma use the full [0, 255] range.
    Full,
    /// Luma is restricted to [16, 235] and chroma to [16, 240].
    Limited,
}

impl From<&h264_reader::nal::sps::SeqParameterSet> for ColorRange {
    fn from(sps: &h264_reader::nal::sps::SeqParameterSet) -> Self {
        sps.vui_parameters
            .as_ref()
            .and_then(|v| v.video_signal_type.as_ref())
            .map(|vst| {
                if vst.video_full_range_flag {
                    ColorRange::Full
                } else {
                    ColorRange::Limited
                }
            })
            .unwrap_or(ColorRange::Limited)
    }
}

/// Parameters that describe an encoded output.
#[derive(Debug, Clone, Copy)]
pub struct EncoderOutputParameters<P> {
    /// Number of frames between IDRs. If [`None`], this will be set to an encoder preferred value,
    /// or, if the encoder doesn't provide a preferred value, to 30.
    pub idr_period: Option<NonZeroU32>,
    /// See [`RateControl`] for description of different rate control modes. The selected mode must
    /// be supported by the device.
    pub rate_control: RateControl,
    /// Max number of references a P-frame can have. This value will be clamped to the max number the
    /// GPU supports. If [`None`], this value will be set to the max value supported by the device.
    pub max_references: Option<NonZeroU32>,
    /// The profile must be supported by the device
    pub profile: P,
    /// The value must be less than
    /// [`EncodeProfileCapabilities::quality_levels`](crate::capabilities::EncodeProfileCapabilities::quality_levels)
    pub quality_level: u32,
    /// A hint indicating what the encoded content is going to be used for.
    ///
    /// Multiple flags can be combined using the `|` operator to indicate multiple usages.
    pub usage_flags: Option<EncoderUsageFlags>,
    /// A hint indicating how to tune the encoder implementation.
    pub tuning_mode: Option<EncoderTuningMode>,
    /// A hint indicating what kind of content the encoder is going to be used for.
    ///
    /// Multiple flags can be combined using the `|` operator to indicate multiple usages.
    pub content_flags: Option<EncoderContentFlags>,
    /// Whether to prepend SPS/PPS NAL units inline before IDR frames.
    /// If `false`, SPS/PPS can be retrieved separately using methods defined on the encoder.
    /// If [`None`], defaults to `true`.
    pub inline_stream_params: Option<bool>,
    /// Color space of the encoded output.
    /// If [`None`], defaults to [`ColorSpace::Unspecified`].
    pub color_space: Option<ColorSpace>,
    /// Color range of the encoded output.
    /// If [`None`], defaults to [`ColorRange::Limited`].
    pub color_range: Option<ColorRange>,
}

/// Parameters for H.264 encoder creation
#[derive(Debug, Clone, Copy)]
pub struct EncoderParametersH264 {
    pub input_parameters: VideoParameters,
    pub output_parameters: EncoderOutputParameters<H264Profile>,
}

/// Parameters for H.265 encoder creation
#[derive(Debug, Clone, Copy)]
pub struct EncoderParametersH265 {
    pub input_parameters: VideoParameters,
    pub output_parameters: EncoderOutputParameters<H265Profile>,
}

/// Open connection to a coding-capable device. Also contains a [`wgpu::Device`], a [`wgpu::Queue`] and
/// a [`wgpu::Adapter`].
pub struct VulkanDevice {
    #[cfg(feature = "wgpu")]
    pub(crate) wgpu_ctx: WgpuContext,

    pub(crate) _physical_device: vk::PhysicalDevice,
    pub(crate) allocator: Arc<Allocator>,
    pub(crate) queues: Queues,
    pub(crate) native_decode_capabilities: Option<NativeDecodeCapabilities>,
    pub(crate) native_encode_capabilities: Option<NativeEncodeCapabilities>,
    pub(crate) adapter_info: AdapterInfo,
    pub(crate) device: Arc<Device>,
}

impl VulkanDevice {
    pub(crate) fn new(
        instance: &VulkanInstance,
        adapter: VulkanAdapter<'_>,
        #[allow(unused)] descriptor: &VulkanDeviceDescriptor,
    ) -> Result<Self, VulkanInitError> {
        let mut required_extensions = REQUIRED_EXTENSIONS
            .iter()
            .copied()
            .chain(match adapter.supports_decoding() {
                true => DECODE_EXTENSIONS.iter().copied(),
                false => [].iter().copied(),
            })
            .chain(match adapter.supports_decoding() {
                true => DECODE_CODEC_EXTENSIONS.iter().copied(),
                false => [].iter().copied(),
            })
            .chain(match adapter.supports_encoding() {
                true => ENCODE_EXTENSIONS.iter().copied(),
                false => [].iter().copied(),
            })
            .chain(match adapter.supports_encoding() {
                true => ENCODE_CODEC_EXTENSIONS.iter().copied(),
                false => [].iter().copied(),
            })
            .collect::<Vec<_>>();

        #[cfg(feature = "wgpu")]
        append_wgpu_device_extensions(&adapter, descriptor.wgpu_features, &mut required_extensions);

        #[cfg(not(feature = "wgpu"))]
        required_extensions.push(ash::khr::timeline_semaphore::NAME);

        let required_extensions_as_ptrs = required_extensions
            .iter()
            .map(|e| e.as_ptr())
            .collect::<Vec<_>>();

        let VulkanAdapter {
            physical_device,
            queue_indices,
            decode_capabilities,
            encode_capabilities,
            info,
            ..
        } = adapter;

        let queue_create_infos = queue_indices.queue_create_infos();
        let queue_create_infos = queue_create_infos
            .iter()
            .map(|q| q.info())
            .collect::<Vec<_>>();

        let mut vk_synch_2_feature =
            vk::PhysicalDeviceSynchronization2Features::default().synchronization2(true);
        let mut vk_video_maintenance1_feature =
            vk::PhysicalDeviceVideoMaintenance1FeaturesKHR::default().video_maintenance1(true);

        let mut vk_descriptor_feature = vk::PhysicalDeviceDescriptorIndexingFeatures::default()
            .descriptor_binding_partially_bound(true);

        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&required_extensions_as_ptrs);

        let device_create_info = device_create_info
            .push_next(&mut vk_synch_2_feature)
            .push_next(&mut vk_video_maintenance1_feature)
            .push_next(&mut vk_descriptor_feature);

        #[cfg(feature = "wgpu")]
        let mut wgpu_physical_device_features = adapter
            .wgpu_adapter
            .adapter
            .physical_device_features(&required_extensions, descriptor.wgpu_features);
        #[cfg(feature = "wgpu")]
        let device_create_info =
            wgpu_physical_device_features.add_to_device_create(device_create_info);

        #[cfg(not(feature = "wgpu"))]
        let mut timeline_semaphore_feature =
            vk::PhysicalDeviceTimelineSemaphoreFeatures::default().timeline_semaphore(true);
        #[cfg(not(feature = "wgpu"))]
        let device_create_info = device_create_info.push_next(&mut timeline_semaphore_feature);

        let device = unsafe {
            instance
                .instance
                .create_device(physical_device, &device_create_info, None)?
        };

        let video_queue_ext = ash::khr::video_queue::Device::new(&instance.instance, &device);
        let video_decode_queue_ext =
            ash::khr::video_decode_queue::Device::new(&instance.instance, &device);

        let video_encode_queue_ext =
            ash::khr::video_encode_queue::Device::new(&instance.instance, &device);

        #[cfg(feature = "vk-validation")]
        let debug_utils_ext = ash::ext::debug_utils::Device::new(&instance.instance, &device);

        let device = Arc::new(Device {
            device,
            video_queue_ext,
            video_decode_queue_ext,
            video_encode_queue_ext,
            #[cfg(feature = "vk-validation")]
            debug_utils_ext,
            _instance: instance.instance.clone(),
        });

        let h264_decode_queues =
            queue_indices
                .h264_decode
                .as_ref()
                .map_or(Vec::new(), |queue_family_index| {
                    (0..queue_family_index.queue_count)
                        .map(|idx| queue_from_device(device.clone(), queue_family_index, idx))
                        .collect::<Vec<_>>()
                });
        let h264_encode_queues =
            queue_indices
                .encode
                .as_ref()
                .map_or(Vec::new(), |queue_family_index| {
                    (0..queue_family_index.queue_count)
                        .map(|idx| queue_from_device(device.clone(), queue_family_index, idx))
                        .collect::<Vec<_>>()
                });
        let transfer_queue = queue_from_device(device.clone(), &queue_indices.transfer, 0);
        let compute_queue =
            if queue_indices.compute.family_index == queue_indices.transfer.family_index {
                if queue_indices.transfer.queue_count > 1 {
                    queue_from_device(device.clone(), &queue_indices.transfer, 1)
                } else {
                    transfer_queue.clone()
                }
            } else {
                queue_from_device(device.clone(), &queue_indices.compute, 0)
            };
        let wgpu_queue =
            queue_from_device(device.clone(), &queue_indices.graphics_transfer_compute, 0);

        let queues = Queues {
            transfer: transfer_queue,
            compute: compute_queue,
            h264_decode: VideoQueues::new(h264_decode_queues.into_boxed_slice()).map(Arc::new),
            encode: VideoQueues::new(h264_encode_queues.into_boxed_slice()).map(Arc::new),
            wgpu: wgpu_queue,
        };

        let allocator = Arc::new(Allocator::new(
            instance.instance.clone(),
            physical_device,
            device.clone(),
        )?);

        #[cfg(feature = "wgpu")]
        let wgpu_ctx = WgpuContext::new(
            adapter.instance,
            adapter.wgpu_adapter,
            queue_indices.graphics_transfer_compute.family_index as u32,
            descriptor,
            device.clone(),
            required_extensions,
        )?;

        Ok(VulkanDevice {
            #[cfg(feature = "wgpu")]
            wgpu_ctx,
            _physical_device: physical_device,
            device,
            allocator,
            queues,
            native_decode_capabilities: decode_capabilities,
            native_encode_capabilities: encode_capabilities,
            adapter_info: info,
        })
    }

    pub(crate) fn decoding_device(self: &Arc<Self>) -> Result<DecodingDevice, VulkanDecoderError> {
        let decode_caps = self
            .native_decode_capabilities
            .as_ref()
            .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?
            .h264
            .as_ref()
            .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?;

        let max_profile = decode_caps
            .max_profile()
            .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?;

        Ok(DecodingDevice {
            vulkan_device: self.clone(),
            h264_decode_queues: self
                .queues
                .h264_decode
                .clone()
                .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?,
            profile_capabilities: decode_caps
                .profile(max_profile)
                .cloned()
                .ok_or(VulkanDecoderError::VulkanDecoderUnsupported)?,
        })
    }

    pub fn create_bytes_decoder_h264(
        self: &Arc<Self>,
        parameters: DecoderParameters,
    ) -> Result<BytesDecoder, DecoderError> {
        let parser = H264Parser::default();
        let reference_ctx = ReferenceContext::new(parameters.missed_frame_handling);

        let vulkan_decoder = VulkanDecoder::new(
            Arc::new(self.decoding_device()?),
            parameters.usage_flags,
            ImageModifiers {
                additional_queue_index: self.queues.transfer.family_index,
                create_flags: Default::default(),
                usage_flags: Default::default(),
            },
        )?;
        let frame_sorter = FrameSorter::<RawFrameData>::new();

        Ok(BytesDecoder {
            parser,
            reference_ctx,
            vulkan_decoder,
            frame_sorter,
        })
    }

    /// Create a single-input multiple-output transcoder.
    /// Each item in `parameters.output_parameters` corresponds to one output.
    #[cfg(feature = "transcoder")]
    pub fn create_transcoder(
        self: &Arc<Self>,
        parameters: TranscoderParameters,
    ) -> Result<crate::vulkan_transcoder::Transcoder, crate::vulkan_transcoder::TranscoderError>
    {
        crate::vulkan_transcoder::Transcoder::new(self.clone(), parameters)
    }

    pub(crate) fn encoding_device(self: &Arc<Self>) -> Result<EncodingDevice, VulkanEncoderError> {
        Ok(EncodingDevice {
            vulkan_device: self.clone(),
            encode_queues: self
                .queues
                .encode
                .clone()
                .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?,
            native_encode_capabilities: self
                .native_encode_capabilities
                .clone()
                .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?,
        })
    }

    pub fn create_bytes_encoder_h264(
        self: &Arc<Self>,
        parameters: EncoderParametersH264,
    ) -> Result<BytesEncoderH264, VulkanEncoderError> {
        let parameters = self.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;
        let encoder = VulkanEncoder::new(Arc::new(self.encoding_device()?), parameters)?;

        Ok(BytesEncoderH264 {
            vulkan_encoder: encoder,
        })
    }

    pub fn create_bytes_encoder_h265(
        self: &Arc<Self>,
        parameters: EncoderParametersH265,
    ) -> Result<BytesEncoderH265, VulkanEncoderError> {
        let parameters = self.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;
        let encoder = VulkanEncoder::new(Arc::new(self.encoding_device()?), parameters)?;

        Ok(BytesEncoderH265 {
            vulkan_encoder: encoder,
        })
    }

    pub fn decode_capabilities(&self) -> DecodeCapabilities {
        self.adapter_info.decode_capabilities
    }

    pub fn encode_capabilities(&self) -> EncodeCapabilities {
        self.adapter_info.encode_capabilities
    }

    fn encoder_output_parameters_low_latency<P>(
        profile: P,
        rate_control: RateControl,
    ) -> EncoderOutputParameters<P> {
        EncoderOutputParameters {
            profile,
            idr_period: None,
            max_references: None,
            rate_control,
            quality_level: 0,
            usage_flags: Some(EncoderUsageFlags::DEFAULT),
            content_flags: Some(EncoderContentFlags::DEFAULT),
            tuning_mode: Some(EncoderTuningMode::LOW_LATENCY),
            inline_stream_params: None,
            color_space: None,
            color_range: None,
        }
    }

    fn encoder_output_parameters_high_quality<P>(
        profile: P,
        rate_control: RateControl,
        quality_level: u32,
    ) -> EncoderOutputParameters<P> {
        EncoderOutputParameters {
            profile,
            idr_period: None,
            max_references: None,
            rate_control,
            quality_level,
            usage_flags: Some(EncoderUsageFlags::DEFAULT),
            content_flags: Some(EncoderContentFlags::DEFAULT),
            tuning_mode: Some(EncoderTuningMode::HIGH_QUALITY),
            inline_stream_params: None,
            color_space: None,
            color_range: None,
        }
    }

    pub fn encoder_output_parameters_h265_low_latency(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H265Profile>, VulkanEncoderError> {
        let Some(caps) = self.native_encode_capabilities.as_ref() else {
            return Err(VulkanEncoderError::VulkanEncoderUnsupported);
        };

        let caps = caps
            .h265
            .as_ref()
            .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?;

        Ok(Self::encoder_output_parameters_low_latency(
            caps.max_profile()
                .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?,
            rate_control,
        ))
    }

    pub fn encoder_output_parameters_h264_low_latency(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H264Profile>, VulkanEncoderError> {
        let Some(caps) = self.native_encode_capabilities.as_ref() else {
            return Err(VulkanEncoderError::VulkanEncoderUnsupported);
        };

        let caps = caps
            .h264
            .as_ref()
            .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?;

        Ok(Self::encoder_output_parameters_low_latency(
            caps.max_profile()
                .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?,
            rate_control,
        ))
    }

    pub fn encoder_output_parameters_h265_high_quality(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H265Profile>, VulkanEncoderError> {
        let Some(caps) = self.native_encode_capabilities.as_ref() else {
            return Err(VulkanEncoderError::VulkanEncoderUnsupported);
        };

        let caps = caps
            .h265
            .as_ref()
            .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?;

        let quality_level = caps
            .profile(
                caps.max_profile()
                    .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?,
            )
            .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?
            .encode_capabilities
            .max_quality_levels
            - 1;

        Ok(Self::encoder_output_parameters_high_quality(
            caps.max_profile()
                .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?,
            rate_control,
            quality_level,
        ))
    }

    pub fn encoder_output_parameters_h264_high_quality(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H264Profile>, VulkanEncoderError> {
        let Some(caps) = self.native_encode_capabilities.as_ref() else {
            return Err(VulkanEncoderError::VulkanEncoderUnsupported);
        };

        let caps = caps
            .h264
            .as_ref()
            .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?;

        let quality_level = caps
            .profile(
                caps.max_profile()
                    .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?,
            )
            .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?
            .encode_capabilities
            .max_quality_levels
            - 1;

        Ok(Self::encoder_output_parameters_high_quality(
            caps.max_profile()
                .ok_or(VulkanEncoderError::VulkanEncoderUnsupported)?,
            rate_control,
            quality_level,
        ))
    }

    pub(crate) fn validate_and_fill_encoder_parameters<C: EncodeCodec>(
        &self,
        encoder_parameters: EncoderOutputParameters<C::Profile>,
        width: NonZeroU32,
        height: NonZeroU32,
        framerate: Rational,
    ) -> Result<FullEncoderParameters<C>, VulkanEncoderError> {
        let Some(caps) = self.native_encode_capabilities.as_ref() else {
            return Err(VulkanEncoderError::VulkanEncoderUnsupported);
        };
        let native_profile_caps =
            C::encode_codec_profile_capabilities(caps, encoder_parameters.profile)?;

        let native_quality_level_properties = native_profile_caps
            .quality_level_properties
            .get(encoder_parameters.quality_level as usize)
            .ok_or(VulkanEncoderError::ParametersError {
                field: "quality_level",
                problem: format!(
                    "Quality level is {}, should be < {}",
                    encoder_parameters.quality_level,
                    native_profile_caps.quality_level_properties.len()
                ),
            })?;

        let idr_period = C::resolve_idr_period(
            &native_quality_level_properties.codec_quality_level_properties,
            encoder_parameters.idr_period,
        );

        let min_extent = native_profile_caps.video_capabilities.min_coded_extent;
        let max_extent = native_profile_caps.video_capabilities.max_coded_extent;

        if width.get() < min_extent.width || width.get() > max_extent.width {
            return Err(VulkanEncoderError::ParametersError {
                field: "width",
                problem: format!(
                    "Width is {}, should be between {} and {}.",
                    width, min_extent.width, max_extent.width
                ),
            });
        }

        if height.get() < min_extent.height || height.get() > max_extent.height {
            return Err(VulkanEncoderError::ParametersError {
                field: "height",
                problem: format!(
                    "Height is {}, should be between {} and {}.",
                    height, min_extent.height, max_extent.height
                ),
            });
        }

        let rate_control = encoder_parameters.rate_control;
        if !native_profile_caps
            .encode_capabilities
            .rate_control_modes
            .contains(rate_control.to_vk())
        {
            return Err(VulkanEncoderError::ParametersError {
                field: "rate_control",
                problem: format!(
                    "Rate control has mode {:?}. Supported modes are: {:?}.",
                    rate_control.to_vk(),
                    native_profile_caps.encode_capabilities.rate_control_modes
                ),
            });
        }

        let max_references = C::resolve_max_references(
            &native_quality_level_properties.codec_quality_level_properties,
            &native_profile_caps.codec_encode_capabilities,
            encoder_parameters.max_references,
        );

        if framerate.numerator == 0 {
            return Err(VulkanEncoderError::ParametersError {
                field: "framerate",
                problem: format!("Framerate is {framerate:?}. The numerator should be != 0.",),
            });
        }
        let usage_flags = encoder_parameters
            .usage_flags
            .unwrap_or(vk::VideoEncodeUsageFlagsKHR::DEFAULT);
        let tuning_mode = encoder_parameters
            .tuning_mode
            .unwrap_or(vk::VideoEncodeTuningModeKHR::DEFAULT);
        let content_flags = encoder_parameters
            .content_flags
            .unwrap_or(vk::VideoEncodeContentFlagsKHR::DEFAULT);
        let color_space = encoder_parameters.color_space.unwrap_or_default();
        let color_range = encoder_parameters
            .color_range
            .unwrap_or(ColorRange::Limited);

        Ok(FullEncoderParameters {
            idr_period,
            width,
            height,
            rate_control,
            max_references,
            quality_level: encoder_parameters.quality_level,
            profile: encoder_parameters.profile,
            framerate,
            usage_flags,
            tuning_mode,
            content_flags,
            inline_stream_params: encoder_parameters.inline_stream_params.unwrap_or(true),
            color_space,
            color_range,
        })
    }

    pub fn supports_decoding(&self) -> bool {
        self.adapter_info.supports_decoding
    }

    pub fn supports_encoding(&self) -> bool {
        self.adapter_info.supports_encoding
    }
}

impl std::fmt::Debug for VulkanDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanDevice").finish()
    }
}

pub(crate) struct DecodingDevice {
    pub(crate) vulkan_device: Arc<VulkanDevice>,
    pub(crate) h264_decode_queues: Arc<VideoQueues>,
    pub(crate) profile_capabilities: NativeDecodeProfileCapabilities<H264Codec>,
}

impl Deref for DecodingDevice {
    type Target = VulkanDevice;

    fn deref(&self) -> &Self::Target {
        &self.vulkan_device
    }
}

pub(crate) struct EncodingDevice {
    pub(crate) vulkan_device: Arc<VulkanDevice>,
    pub(crate) encode_queues: Arc<VideoQueues>,
    pub(crate) native_encode_capabilities: NativeEncodeCapabilities,
}

impl Deref for EncodingDevice {
    type Target = VulkanDevice;

    fn deref(&self) -> &Self::Target {
        &self.vulkan_device
    }
}

fn queue_from_device(
    device: Arc<Device>,
    queue_family_index: &QueueIndex<'static>,
    queue_index: usize,
) -> Queue {
    let queue = unsafe {
        device.get_device_queue(queue_family_index.family_index as u32, queue_index as u32)
    };
    Queue {
        queue: Arc::new(queue.into()),
        family_index: queue_family_index.family_index,
        _video_properties: queue_family_index.video_properties,
        query_result_status_properties: queue_family_index.query_result_status_properties,
        device,
    }
}
