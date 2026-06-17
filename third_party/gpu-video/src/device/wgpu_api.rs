use std::sync::Arc;

use crate::{
    DecoderError, VulkanAdapter, VulkanDevice, VulkanEncoderError, VulkanInstance, WgpuInitError,
    WgpuTexturesDecoder, WgpuTexturesEncoderH264, WgpuTexturesEncoderH265,
    device::{
        DecoderParameters, EncoderParametersH264, EncoderParametersH265, VulkanDeviceDescriptor,
    },
    parser::{h264::H264Parser, reference_manager::ReferenceContext},
    vulkan_decoder::{FrameSorter, ImageModifiers, VulkanDecoder},
    vulkan_encoder::VulkanEncoder,
    wrappers::Device,
};

impl VulkanDevice {
    pub fn create_wgpu_textures_decoder_h264(
        self: &Arc<Self>,
        parameters: DecoderParameters,
    ) -> Result<WgpuTexturesDecoder, DecoderError> {
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
        let frame_sorter = FrameSorter::<wgpu::Texture>::new();

        Ok(WgpuTexturesDecoder {
            parser,
            reference_ctx,
            vulkan_decoder,
            frame_sorter,
        })
    }

    pub fn create_wgpu_textures_encoder_h264(
        self: &Arc<Self>,
        parameters: EncoderParametersH264,
    ) -> Result<WgpuTexturesEncoderH264, VulkanEncoderError> {
        let parameters = self.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;
        let encoder = VulkanEncoder::new(Arc::new(self.encoding_device()?), parameters)?;
        Ok(WgpuTexturesEncoderH264 {
            vulkan_encoder: encoder,
        })
    }

    pub fn create_wgpu_textures_encoder_h265(
        self: &Arc<Self>,
        parameters: EncoderParametersH265,
    ) -> Result<WgpuTexturesEncoderH265, VulkanEncoderError> {
        let parameters = self.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;
        let encoder = VulkanEncoder::new(Arc::new(self.encoding_device()?), parameters)?;
        Ok(WgpuTexturesEncoderH265 {
            vulkan_encoder: encoder,
        })
    }

    pub fn wgpu_device(&self) -> wgpu::Device {
        self.wgpu_ctx.wgpu_device.clone()
    }

    pub fn wgpu_queue(&self) -> wgpu::Queue {
        self.wgpu_ctx.wgpu_queue.clone()
    }

    pub fn wgpu_adapter(&self) -> wgpu::Adapter {
        self.wgpu_ctx.wgpu_adapter.clone()
    }
}

pub(crate) struct WgpuContext {
    pub(crate) wgpu_device: wgpu::Device,
    pub(crate) wgpu_queue: wgpu::Queue,
    pub(crate) wgpu_adapter: wgpu::Adapter,
}

impl WgpuContext {
    pub(crate) fn new(
        instance: &VulkanInstance,
        wgpu_adapter: wgpu::hal::ExposedAdapter<wgpu::hal::vulkan::Api>,
        wgpu_queue_family_index: u32,
        device_descriptor: &VulkanDeviceDescriptor,
        device: Arc<Device>,
        required_extensions: Vec<&'static std::ffi::CStr>,
    ) -> Result<Self, WgpuInitError> {
        let VulkanDeviceDescriptor {
            wgpu_features,
            wgpu_experimental_features,
            wgpu_limits,
        } = device_descriptor.clone();

        let wgpu_features = wgpu_features | wgpu::Features::TEXTURE_FORMAT_NV12;
        let wgpu_device = unsafe {
            wgpu_adapter.adapter.device_from_raw(
                device.device.clone(),
                Some(Box::new(move || {
                    drop(device);
                })),
                &required_extensions,
                wgpu_features,
                &wgpu_limits,
                &wgpu::MemoryHints::default(),
                wgpu_queue_family_index,
                0,
            )?
        };

        let wgpu_adapter = unsafe { instance.wgpu_instance.create_adapter_from_hal(wgpu_adapter) };
        let (wgpu_device, wgpu_queue) = unsafe {
            wgpu_adapter.create_device_from_hal(
                wgpu_device,
                &wgpu::DeviceDescriptor {
                    label: Some("wgpu device created by the vulkan video decoder"),
                    memory_hints: wgpu::MemoryHints::default(),
                    required_limits: wgpu_limits,
                    required_features: wgpu_features,
                    trace: wgpu::Trace::Off,
                    experimental_features: wgpu_experimental_features,
                },
            )?
        };

        Ok(Self {
            wgpu_device,
            wgpu_queue,
            wgpu_adapter,
        })
    }
}

pub(crate) fn append_wgpu_device_extensions(
    adapter: &VulkanAdapter<'_>,
    wgpu_features: wgpu::Features,
    required_extensions: &mut Vec<&'static std::ffi::CStr>,
) {
    let wgpu_features = wgpu_features | wgpu::Features::TEXTURE_FORMAT_NV12;
    let mut wgpu_extensions = adapter
        .wgpu_adapter
        .adapter
        .required_device_extensions(wgpu_features);

    required_extensions.append(&mut wgpu_extensions);
}
