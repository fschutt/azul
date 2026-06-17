use crate::{
    DecoderError, DecoderEvent, EncodedInputChunk, EncodedOutputChunk, InputFrame, OutputFrame,
    VulkanEncoderError,
    codec::{
        h264::{H264Codec, encode::H264WriteParametersInfo},
        h265::{H265Codec, encode::H265WriteParametersInfo},
    },
    parser::{
        decoder_instructions::compile_to_decoder_instructions,
        h264::{AccessUnit, H264Parser},
        reference_manager::ReferenceContext,
    },
    vulkan_decoder::{FrameSorter, VulkanDecoder},
    vulkan_encoder::VulkanEncoder,
};

/// A decoder that outputs frames stored as [`wgpu::Texture`]s
pub struct WgpuTexturesDecoder {
    pub(crate) vulkan_decoder: VulkanDecoder<'static>,
    pub(crate) parser: H264Parser,
    pub(crate) reference_ctx: ReferenceContext,
    pub(crate) frame_sorter: FrameSorter<wgpu::Texture>,
}

impl WgpuTexturesDecoder {
    /// The produced textures have the [`wgpu::TextureFormat::NV12`] format and can be used as a texture binding.
    pub fn decode(
        &mut self,
        frame: EncodedInputChunk<'_>,
    ) -> Result<Vec<OutputFrame<wgpu::Texture>>, DecoderError> {
        self.process_event(DecoderEvent::DecodeChunk(frame))
    }

    /// Flush all frames from the decoder.
    ///
    /// Make sure that this is done when you have the knowledge that no more frames will be coming
    /// that need to be presented before the already decoded frames.
    pub fn flush(&mut self) -> Result<Vec<OutputFrame<wgpu::Texture>>, DecoderError> {
        self.process_event(DecoderEvent::Flush)
    }

    /// Process a [`DecoderEvent`]. For most use cases, using [`Self::decode`] and [`Self::flush`] is enough.
    /// Use this only when you need more fine-grained control.
    /// May return a sequence of decoded frames in the [NV12 format](https://en.wikipedia.org/wiki/YCbCr#4:2:0).
    pub fn process_event(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
    ) -> Result<Vec<OutputFrame<wgpu::Texture>>, DecoderError> {
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
    ) -> Result<Vec<OutputFrame<wgpu::Texture>>, DecoderError> {
        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, access_units)?;
        let unsorted_frames = self.vulkan_decoder.decode_to_wgpu_textures(&instructions)?;
        let sorted_frames = self.frame_sorter.put_frames(unsorted_frames);
        Ok(sorted_frames)
    }
}

/// An H.265 (HEVC) encoder that takes input frames as [`wgpu::Texture`]s (in [`wgpu::TextureFormat::NV12`])
pub struct WgpuTexturesEncoderH265 {
    pub(crate) vulkan_encoder: VulkanEncoder<'static, H265Codec>,
}

impl WgpuTexturesEncoderH265 {
    /// The result is a chunk of H265 bitstream.
    ///
    /// If the `force_keyframe` option is set to `true`, the encoder will encode this frame as a
    /// [keyframe](https://en.wikipedia.org/wiki/Video_compression_picture_types#Intra-coded_(I)_frames/slices_(key_frames)).
    /// Otherwise, the encoder will decide which frames should be coded this way.
    pub fn encode(
        &mut self,
        frame: InputFrame<wgpu::Texture>,
        force_keyframe: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VulkanEncoderError> {
        self.vulkan_encoder.encode_texture(frame, force_keyframe)
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

/// An H.264 (AVC) encoder that takes input frames as [`wgpu::Texture`]s (in [`wgpu::TextureFormat::NV12`])
pub struct WgpuTexturesEncoderH264 {
    pub(crate) vulkan_encoder: VulkanEncoder<'static, H264Codec>,
}

impl WgpuTexturesEncoderH264 {
    /// The result is a chunk of H264 bitstream.
    ///
    /// If the `force_keyframe` option is set to `true`, the encoder will encode this frame as a
    /// [keyframe](https://en.wikipedia.org/wiki/Video_compression_picture_types#Intra-coded_(I)_frames/slices_(key_frames)).
    /// Otherwise, the encoder will decide which frames should be coded this way.
    pub fn encode(
        &mut self,
        frame: InputFrame<wgpu::Texture>,
        force_keyframe: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VulkanEncoderError> {
        self.vulkan_encoder.encode_texture(frame, force_keyframe)
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

#[derive(thiserror::Error, Debug)]
pub enum WgpuInitError {
    #[error("Wgpu instance error: {0}")]
    WgpuInstanceError(#[from] wgpu::hal::InstanceError),

    #[error("Wgpu device error: {0}")]
    WgpuDeviceError(#[from] wgpu::hal::DeviceError),

    #[error("Wgpu request device error: {0}")]
    WgpuRequestDeviceError(#[from] wgpu::RequestDeviceError),

    #[error("Cannot create a wgpu adapter")]
    WgpuAdapterNotCreated,
}
