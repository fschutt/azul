pub mod capabilities {
    pub use crate::adapter::AdapterInfo;
    pub use crate::device::caps::{
        DecodeCapabilities, DecodeH264Capabilities, DecodeH264ProfileCapabilities,
        DecodeH265Capabilities, DecodeH265ProfileCapabilities, EncodeCapabilities,
        EncodeH264Capabilities, EncodeH265Capabilities, EncodeProfileCapabilities,
    };

    pub use ash::vk::PhysicalDeviceType as VulkanDeviceType;
}

pub mod parameters {
    pub use crate::adapter::VulkanAdapterDescriptor;
    pub use crate::device::{
        ColorRange, ColorSpace, DecoderParameters, EncoderOutputParameters, EncoderParametersH264,
        EncoderParametersH265, MissedFrameHandling, Rational, VideoParameters,
        VulkanDeviceDescriptor,
    };

    pub type EncoderOutputParametersH264 = crate::device::EncoderOutputParameters<H264Profile>;

    pub use crate::vulkan_encoder::RateControl;
    #[cfg(feature = "transcoder")]
    pub use crate::vulkan_transcoder::{
        AnyEncoderParameters, TranscoderOutputParameters, TranscoderParameters,
    };

    #[cfg(feature = "wgpu")]
    pub use crate::wgpu_helpers::WgpuConverterParameters;

    pub use ash::vk::VideoDecodeUsageFlagsKHR as DecoderUsageFlags;

    pub use ash::vk::VideoEncodeContentFlagsKHR as EncoderContentFlags;
    pub use ash::vk::VideoEncodeTuningModeKHR as EncoderTuningMode;
    pub use ash::vk::VideoEncodeUsageFlagsKHR as EncoderUsageFlags;

    /// Scaling algorithm used when resizing frames in the transcoder.
    #[derive(Debug, Clone, Copy, Default)]
    #[repr(u32)]
    pub enum ScalingAlgorithm {
        NearestNeighbor,
        #[default]
        Bilinear,
        Lanczos3,
    }

    /// A profile in H.264 is a set of codec features used while encoding a specific video.
    /// Baseline uses the fewest features, Main can use more and High even more than Main.
    #[derive(Debug, Clone, Copy)]
    pub enum H264Profile {
        Baseline,
        Main,
        High,
    }

    impl H264Profile {
        pub(crate) fn to_profile_idc(self) -> ash::vk::native::StdVideoH264ProfileIdc {
            match self {
                H264Profile::Baseline => {
                    ash::vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_BASELINE
                }
                H264Profile::Main => {
                    ash::vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN
                }
                H264Profile::High => {
                    ash::vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH
                }
            }
        }
    }

    /// A profile in H.265 is a set of codec features used while encoding a specific video.
    /// Right now, only Main is available.
    #[derive(Debug, Clone, Copy)]
    pub enum H265Profile {
        Main,
    }

    impl H265Profile {
        pub(crate) fn to_profile_idc(self) -> ash::vk::native::StdVideoH265ProfileIdc {
            match self {
                H265Profile::Main => {
                    ash::vk::native::StdVideoH265ProfileIdc_STD_VIDEO_H265_PROFILE_IDC_MAIN
                }
            }
        }
    }
}

#[cfg(feature = "wgpu")]
mod wgpu_api;
#[cfg(feature = "wgpu")]
pub use wgpu_api::*;

use crate::codec::h264::H264Codec;
use crate::codec::h264::encode::H264WriteParametersInfo;
use crate::codec::h265::H265Codec;
use crate::codec::h265::encode::H265WriteParametersInfo;
use crate::device::{ColorRange, ColorSpace};
use crate::parser::h264::AccessUnit;
use crate::vulkan_decoder::{FrameSorter, VulkanDecoder};
use ash::vk;

pub use crate::adapter::VulkanAdapter;
pub use crate::device::VulkanDevice;
pub use crate::instance::VulkanInstance;
pub use crate::parser::{h264::H264ParserError, reference_manager::ReferenceManagementError};
pub use crate::vulkan_decoder::VulkanDecoderError;
pub use crate::vulkan_encoder::VulkanEncoderError;
#[cfg(feature = "transcoder")]
pub use crate::vulkan_transcoder::{Transcoder, TranscoderError};

#[cfg(feature = "wgpu")]
pub use crate::wgpu_helpers::{
    WgpuConverterInitError, WgpuNv12ToRgbaConverter, WgpuRgbaToNv12Converter,
};

use crate::parser::{
    decoder_instructions::compile_to_decoder_instructions, h264::H264Parser,
    reference_manager::ReferenceContext,
};
use crate::vulkan_encoder::VulkanEncoder;
use crate::wrappers::ImageKey;

#[derive(Debug, thiserror::Error)]
pub enum DecoderError {
    #[error("Decoder error: {0}")]
    VulkanDecoderError(#[from] VulkanDecoderError),

    #[error("H264 parser error: {0}")]
    ParserError(#[from] H264ParserError),

    #[error("Reference management error: {0}")]
    ReferenceManagementError(#[from] ReferenceManagementError),
}

#[derive(thiserror::Error, Debug)]
pub enum VulkanInitError {
    #[error("Error loading vulkan: {0}")]
    LoadingError(#[from] ash::LoadingError),

    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),

    #[cfg(feature = "wgpu")]
    #[error(transparent)]
    WgpuError(#[from] WgpuInitError),

    #[error("Cannot find a suitable physical device")]
    NoDevice,

    #[error("String conversion error: {0}")]
    StringConversionError(#[from] std::ffi::FromBytesUntilNulError),

    #[error("Profile does not support NV12 texture format")]
    NoNV12ProfileSupport,
}

#[derive(thiserror::Error, Debug)]
pub enum VulkanCommonError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),

    #[error("Cannot find a queue with index {0}")]
    NoQueue(usize),

    #[error("Memory copy requested to a buffer that is not set up for receiving input")]
    UploadToImproperBuffer,

    #[error("A slot in the Decoded Pictures Buffer was requested, but all slots are taken")]
    NoFreeSlotsInDpb,

    #[error("DPB can have at most 32 slots, {0} was requested")]
    DpbTooLong(u32),

    #[error("Tried to wait for an unsignaled semaphore value")]
    SemaphoreWaitOnUnsignaledValue,

    #[error("Tried to register {0:x?} as a new image, while it already exists")]
    RegisteredNewImageTwice(ImageKey),

    #[error("Tried to access state of image {0:x?}, which does not exist")]
    TriedToAccessNonexistentImageState(ImageKey),

    #[error("Tried to unregister image {0:x?} that was not registered")]
    UnregisteredNonexistentImage(ImageKey),

    #[error("Unsupported image aspect: {0:?}")]
    UnsupportedImageAspect(vk::ImageAspectFlags),
}

/// Represents a chunk of encoded video data used for decoding.
///
/// `pts` is the presentation timestamp -- a number, which describes when the given frame
/// should be presented, used for synchronization with other tracks, e.g. with audio
///
/// If `pts` is [`Option::Some`], it is inferred that the chunk contains bytestream that belongs to
/// one output frame.
/// If `pts` is [`Option::None`], the chunk can contain bytestream from multiple consecutive
/// frames.
pub struct EncodedInputChunk<'a> {
    pub data: &'a [u8],
    pub pts: Option<u64>,
}

pub type H264DecoderEvent<'a> = DecoderEvent<'a, AccessUnit>;

/// Represents all events that can be sent to the decoder
#[non_exhaustive]
pub enum DecoderEvent<'a, ParsedFrame> {
    /// Submit encoded chunk for decoding
    DecodeChunk(EncodedInputChunk<'a>),

    /// Submit parsed frame for decoding
    DecodeParsedFrame(ParsedFrame),

    /// Signal the end of the current frame and flush any buffered bitstream units in the parser.
    ///
    /// You should send this event only if you need to minimize the codec parsing latency.
    /// The decoder does not require it to work.
    ///
    /// Send this only after submitting all bitstream units belonging to a single frame.
    /// Any incomplete bitstream units buffered in the parser will be flushed and decoded,
    /// which may lead to artifacts.
    SignalFrameEnd,

    /// Signal the decoder that a chunk of the bitstream was lost.
    ///
    /// What the decoder will do depends on the set [`parameters::MissedFrameHandling`]
    SignalDataLoss,

    /// Flush all frames from the decoder.
    ///
    /// Make sure that this is done when you have the knowledge that no more frames will be coming
    /// that need to be presented before the already decoded frames.
    Flush,
}

/// Represents a chunk of encoded video data returned by the encoder.
///
/// `pts` is the presentation timestamp -- a number, which describes when the given frame
/// should be presented, used for synchronization with other tracks, e.g. with audio
pub struct EncodedOutputChunk<T> {
    pub data: T,
    pub pts: Option<u64>,
    pub is_keyframe: bool,
}

/// Represents a frame to be encoded.
pub struct InputFrame<T> {
    pub data: T,
    pub pts: Option<u64>,
}

/// Additional information about the decoded frame.
pub struct FrameMetadata {
    pub pts: Option<u64>,
    pub color_space: ColorSpace,
    pub color_range: ColorRange,
}

/// Represents a single decoded frame.
pub struct OutputFrame<T> {
    pub data: T,
    pub metadata: FrameMetadata,
}

pub struct RawFrameData {
    pub frame: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A decoder that outputs frames stored as [`Vec<u8>`] with the raw pixel data.
pub struct BytesDecoder {
    pub(crate) vulkan_decoder: VulkanDecoder<'static>,
    pub(crate) parser: H264Parser,
    pub(crate) reference_ctx: ReferenceContext,
    pub(crate) frame_sorter: FrameSorter<RawFrameData>,
}

impl BytesDecoder {
    /// The result is a sequence of frames. The payload of each [`OutputFrame`] struct is a [`Vec<u8>`]. Each [`Vec<u8>`] contains a single
    /// decoded frame in the [NV12 format](https://en.wikipedia.org/wiki/YCbCr#4:2:0).
    pub fn decode(
        &mut self,
        frame: EncodedInputChunk<'_>,
    ) -> Result<Vec<OutputFrame<RawFrameData>>, DecoderError> {
        self.process_event(DecoderEvent::DecodeChunk(frame))
    }

    /// Flush all frames from the decoder.
    ///
    /// Make sure that this is done when you have the knowledge that no more frames will be coming
    /// that need to be presented before the already decoded frames.
    pub fn flush(&mut self) -> Result<Vec<OutputFrame<RawFrameData>>, DecoderError> {
        self.process_event(DecoderEvent::Flush)
    }

    /// Process a [`DecoderEvent`]. For most use cases, using [`Self::decode`] and [`Self::flush`] is enough.
    /// Use this only when you need more fine-grained control.
    /// May return a sequence of decoded frames in the [NV12 format](https://en.wikipedia.org/wiki/YCbCr#4:2:0).
    pub fn process_event(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
    ) -> Result<Vec<OutputFrame<RawFrameData>>, DecoderError> {
        match event {
            DecoderEvent::DecodeChunk(chunk) => {
                let nalus = self.parser.parse(chunk.data, chunk.pts)?;
                self.decode_access_units(nalus)
            }
            DecoderEvent::DecodeParsedFrame(au) => self.decode_access_units(vec![au]),
            DecoderEvent::SignalFrameEnd => {
                let access_units = self.parser.flush()?;
                self.decode_access_units(access_units)
            }
            DecoderEvent::SignalDataLoss => {
                self.reference_ctx.mark_missed_frames();
                Ok(Vec::new())
            }
            DecoderEvent::Flush => {
                let access_units = self.parser.flush()?;
                let mut frames = self.decode_access_units(access_units)?;
                frames.append(&mut self.frame_sorter.flush());
                Ok(frames)
            }
        }
    }

    fn decode_access_units(
        &mut self,
        access_units: Vec<AccessUnit>,
    ) -> Result<Vec<OutputFrame<RawFrameData>>, DecoderError> {
        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, access_units)?;
        let unsorted_frames = self.vulkan_decoder.decode_to_bytes(&instructions)?;
        let sorted_frames = self.frame_sorter.put_frames(unsorted_frames);
        Ok(sorted_frames)
    }
}

/// An H.265 (HEVC) encoder that takes input frames as [`Vec<u8>`] with raw pixel data (in NV12)
pub struct BytesEncoderH265 {
    pub(crate) vulkan_encoder: VulkanEncoder<'static, H265Codec>,
}

impl BytesEncoderH265 {
    /// The result is a chunk of H265 bitstream.
    ///
    /// If the `force_keyframe` option is set to `true`, the encoder will encode this frame as a
    /// [keyframe](https://en.wikipedia.org/wiki/Video_compression_picture_types#Intra-coded_(I)_frames/slices_(key_frames)).
    /// Otherwise, the encoder will decide which frames should be coded this way.
    pub fn encode(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        force_keyframe: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VulkanEncoderError> {
        self.vulkan_encoder.encode_bytes(frame, force_keyframe)
    }

    /// Retrieve encoded VPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn vps(&self) -> Result<Vec<u8>, VulkanEncoderError> {
        self.vulkan_encoder
            .stream_parameters(H265WriteParametersInfo {
                write_vps: true,
                write_sps: false,
                write_pps: false,
            })
    }

    /// Retrieve encoded SPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn sps(&self) -> Result<Vec<u8>, VulkanEncoderError> {
        self.vulkan_encoder
            .stream_parameters(H265WriteParametersInfo {
                write_vps: false,
                write_sps: true,
                write_pps: false,
            })
    }

    /// Retrieve encoded PPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn pps(&self) -> Result<Vec<u8>, VulkanEncoderError> {
        self.vulkan_encoder
            .stream_parameters(H265WriteParametersInfo {
                write_vps: false,
                write_sps: false,
                write_pps: true,
            })
    }
}

/// An H.264 (AVC) encoder that takes input frames as [`Vec<u8>`] with raw pixel data (in NV12)
pub struct BytesEncoderH264 {
    pub(crate) vulkan_encoder: VulkanEncoder<'static, H264Codec>,
}

impl BytesEncoderH264 {
    /// The result is a chunk of H264 bitstream.
    ///
    /// If the `force_keyframe` option is set to `true`, the encoder will encode this frame as a
    /// [keyframe](https://en.wikipedia.org/wiki/Video_compression_picture_types#Intra-coded_(I)_frames/slices_(key_frames)).
    /// Otherwise, the encoder will decide which frames should be coded this way.
    pub fn encode(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        force_keyframe: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VulkanEncoderError> {
        self.vulkan_encoder.encode_bytes(frame, force_keyframe)
    }

    /// Retrieve encoded SPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn sps(&self) -> Result<Vec<u8>, VulkanEncoderError> {
        self.vulkan_encoder
            .stream_parameters(H264WriteParametersInfo {
                write_sps: true,
                write_pps: false,
            })
    }

    /// Retrieve encoded PPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn pps(&self) -> Result<Vec<u8>, VulkanEncoderError> {
        self.vulkan_encoder
            .stream_parameters(H264WriteParametersInfo {
                write_sps: false,
                write_pps: true,
            })
    }
}
