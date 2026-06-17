use std::{io::Cursor, sync::Arc};

use ash::vk;

use crate::{
    VulkanDevice,
    parameters::ScalingAlgorithm,
    vulkan_decoder::{DecodeSubmission, DecoderTrackerWaitState},
    vulkan_encoder::{EncoderTracker, EncoderTrackerWaitState},
    vulkan_transcoder::TranscoderError,
    wrappers::{
        CommandBufferPool, ComputePipeline, DescriptorPool, DescriptorSet, DescriptorSetLayout,
        Image, ImageView, PipelineLayout, ProfileInfo, SemaphoreWaitValue, ShaderModule,
    },
};

const MAX_OUTPUTS: u32 = 8;
const MAX_FRAMES_IN_FLIGHT: u32 = 16; // The max reorder in h264

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PushConstants {
    output_count: u32,
    width: u32,
    height: u32,
    scaling_algorithm: [u32; MAX_OUTPUTS as usize],
}

impl PushConstants {
    fn new(output_configs: &[OutputConfig], cropped_size: vk::Extent2D) -> Self {
        let mut result = PushConstants {
            output_count: output_configs.len() as u32,
            width: cropped_size.width,
            height: cropped_size.height,
            scaling_algorithm: [0; _],
        };

        for (i, config) in output_configs.iter().enumerate() {
            result.scaling_algorithm[i] = config.scaling_algorithm as u32;
        }

        result
    }
}

pub(crate) struct ResizingImageBundle {
    pub(crate) image: Arc<Image>,
    view_y: ImageView,
    view_uv: ImageView,
}

pub(crate) struct ResizeSubmission {
    pub(crate) outputs: Box<[ResizingImageBundle]>,
    pub(crate) _input: ResizingImageBundle,
    pub(crate) descriptors: Descriptors,
}

impl ResizingImageBundle {
    fn new(image: Arc<Image>, layer: u32) -> Result<Self, TranscoderError> {
        let view_y = image.create_plane_view(
            layer,
            vk::ImageAspectFlags::PLANE_0,
            vk::ImageUsageFlags::STORAGE,
        )?;
        let view_uv = image.create_plane_view(
            layer,
            vk::ImageAspectFlags::PLANE_1,
            vk::ImageUsageFlags::STORAGE,
        )?;

        Ok(Self {
            image,
            view_y,
            view_uv,
        })
    }
}

struct ImageHeap {
    freelist: Vec<Box<[ResizingImageBundle]>>,
    device: Arc<VulkanDevice>,
    configs: Vec<OutputConfig>,
}

impl ImageHeap {
    fn new(device: Arc<VulkanDevice>, configs: Vec<OutputConfig>) -> Self {
        Self {
            device,
            freelist: Vec::new(),
            configs,
        }
    }

    fn free(&mut self, images: Box<[ResizingImageBundle]>) {
        self.freelist.push(images);
    }

    fn allocate(
        &mut self,
        trackers: &mut [&mut EncoderTracker],
    ) -> Result<Box<[ResizingImageBundle]>, TranscoderError> {
        if let Some(images) = self.freelist.pop() {
            return Ok(images);
        }

        let mut result = Vec::with_capacity(self.configs.len());
        for (config, tracker) in self.configs.iter().zip(trackers.iter_mut()) {
            let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
                .profiles(std::slice::from_ref(&config.profile.profile_info));
            let queue_indices = [
                self.device.queues.encode.as_ref().unwrap().family_index as u32,
                self.device.queues.compute.family_index as u32,
            ];
            let create_info = vk::ImageCreateInfo::default()
                .flags(vk::ImageCreateFlags::EXTENDED_USAGE | vk::ImageCreateFlags::MUTABLE_FORMAT)
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
                .extent(vk::Extent3D {
                    width: config.width,
                    height: config.height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR)
                .sharing_mode(vk::SharingMode::CONCURRENT)
                .queue_family_indices(&queue_indices)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .push_next(&mut profile_list_info);

            let image = Arc::new(Image::new(
                self.device.allocator.clone(),
                &create_info,
                tracker.image_layout_tracker.clone(),
            )?);

            self.device
                .device
                .set_label(image.image, Some("resize image"))?;

            result.push(ResizingImageBundle::new(image, 0)?);
        }

        Ok(result.into_boxed_slice())
    }
}

pub(crate) struct OutputConfig {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) profile: ProfileInfo<'static>,
    pub(crate) scaling_algorithm: ScalingAlgorithm,
}

pub(crate) struct Descriptors {
    input: DescriptorSet,
    output_y: DescriptorSet,
    output_uv: DescriptorSet,
}

struct DescriptorHeap {
    pool: Arc<DescriptorPool>,
    freelist: Vec<Descriptors>,
    layout_input: Arc<DescriptorSetLayout>,
    layout_output: Arc<DescriptorSetLayout>,
}

impl DescriptorHeap {
    fn new(
        pool: Arc<DescriptorPool>,
        layout_input: Arc<DescriptorSetLayout>,
        layout_output: Arc<DescriptorSetLayout>,
    ) -> Self {
        Self {
            pool,
            freelist: Vec::new(),
            layout_input,
            layout_output,
        }
    }

    fn free(&mut self, descriptors: Descriptors) {
        self.freelist.push(descriptors);
    }

    fn allocate(&mut self) -> Result<Descriptors, TranscoderError> {
        if let Some(descriptors) = self.freelist.pop() {
            return Ok(descriptors);
        }

        let input = DescriptorSet::new(
            self.pool.clone(),
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(self.pool.pool)
                .set_layouts(&[self.layout_input.set_layout]),
        )?
        .pop()
        .unwrap();

        let mut descriptor_set_outputs = DescriptorSet::new(
            self.pool.clone(),
            &vk::DescriptorSetAllocateInfo::default()
                .set_layouts(&[self.layout_output.set_layout, self.layout_output.set_layout]),
        )?;

        let output_uv = descriptor_set_outputs.pop().unwrap();
        let output_y = descriptor_set_outputs.pop().unwrap();

        Ok(Descriptors {
            input,
            output_y,
            output_uv,
        })
    }
}

pub(crate) struct ResizingPipeline {
    descriptor_heap: DescriptorHeap,
    image_heap: ImageHeap,
    pipeline: ComputePipeline,
    buffer_pool: CommandBufferPool,
    device: Arc<VulkanDevice>,
}

impl ResizingPipeline {
    pub(crate) fn new(
        device: Arc<VulkanDevice>,
        configs: Vec<OutputConfig>,
    ) -> Result<Self, TranscoderError> {
        if configs.is_empty() || configs.len() > MAX_OUTPUTS as usize {
            return Err(TranscoderError::WrongOutputNumber {
                expected_max: MAX_OUTPUTS as usize,
                actual: configs.len(),
            });
        }
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_IMAGE)
            .descriptor_count((2 * MAX_OUTPUTS + 2) * MAX_FRAMES_IN_FLIGHT)];
        let descriptor_pool = Arc::new(DescriptorPool::new(
            device.device.clone(),
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(3 * MAX_FRAMES_IN_FLIGHT)
                .pool_sizes(&pool_sizes),
        )?);

        let bindings_input = [
            vk::DescriptorSetLayoutBinding::default()
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .binding(0),
            vk::DescriptorSetLayoutBinding::default()
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .binding(1),
        ];

        let layout_input = Arc::new(DescriptorSetLayout::new(
            device.device.clone(),
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings_input),
        )?);

        let bindings_output = [vk::DescriptorSetLayoutBinding::default()
            .descriptor_count(MAX_OUTPUTS)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .binding(0)
            .stage_flags(vk::ShaderStageFlags::COMPUTE)];

        let flags = [vk::DescriptorBindingFlags::PARTIALLY_BOUND];
        let mut binding_flags =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&flags);

        let layout_output = Arc::new(DescriptorSetLayout::new(
            device.device.clone(),
            &vk::DescriptorSetLayoutCreateInfo::default()
                .bindings(&bindings_output)
                .push_next(&mut binding_flags),
        )?);

        let descriptor_heap = DescriptorHeap::new(
            descriptor_pool.clone(),
            layout_input.clone(),
            layout_output.clone(),
        );
        let image_heap = ImageHeap::new(device.clone(), configs);

        let layouts = [
            layout_input.set_layout,
            layout_output.set_layout,
            layout_output.set_layout,
        ];
        let push_constants = [vk::PushConstantRange::default()
            .size(std::mem::size_of::<PushConstants>() as u32)
            .offset(0)
            .stage_flags(vk::ShaderStageFlags::COMPUTE)];
        let create_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&layouts)
            .push_constant_ranges(&push_constants);
        let pipeline_layout = Arc::new(PipelineLayout::new(
            device.device.clone(),
            &create_info,
            vec![layout_input.clone(), layout_output.clone()],
        )?);

        const SHADER_SPV: &[u8] =
            include_bytes!(concat!(env!("OUT_DIR"), "/transcoding_shader.spv"));
        let mut shader_bytes_cursor = Cursor::new(SHADER_SPV);
        let compiled_shader = ash::util::read_spv(&mut shader_bytes_cursor).unwrap();

        let shader_module = Arc::new(ShaderModule::new(
            device.device.clone(),
            &vk::ShaderModuleCreateInfo::default().code(&compiled_shader),
        )?);

        let shader = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .name(c"main")
            .module(shader_module.module);
        let create_info = vk::ComputePipelineCreateInfo::default()
            .stage(shader)
            .layout(pipeline_layout.layout);

        let pipeline = ComputePipeline::new(
            device.device.clone(),
            create_info,
            pipeline_layout,
            shader_module,
        )?;

        let buffer_pool =
            CommandBufferPool::new(device.clone(), device.queues.compute.family_index)?;

        Ok(Self {
            image_heap,
            descriptor_heap,
            pipeline,
            buffer_pool,
            device,
        })
    }

    pub(crate) fn free_submission(&mut self, submission: ResizeSubmission) {
        self.descriptor_heap.free(submission.descriptors);
        self.image_heap.free(submission.outputs);
    }

    fn write_descriptors(
        &mut self,
        input: &ResizingImageBundle,
        outputs: &[ResizingImageBundle],
    ) -> Result<Descriptors, TranscoderError> {
        let image_info_input_y = vk::DescriptorImageInfo::default()
            .image_view(input.view_y.view)
            .image_layout(vk::ImageLayout::GENERAL);
        let image_info_input_uv = vk::DescriptorImageInfo::default()
            .image_view(input.view_uv.view)
            .image_layout(vk::ImageLayout::GENERAL);

        let (image_infos_output_y, image_infos_output_uv) = outputs
            .iter()
            .map(|bundle| {
                (
                    vk::DescriptorImageInfo::default()
                        .image_layout(vk::ImageLayout::GENERAL)
                        .image_view(bundle.view_y.view),
                    vk::DescriptorImageInfo::default()
                        .image_layout(vk::ImageLayout::GENERAL)
                        .image_view(bundle.view_uv.view),
                )
            })
            .unzip::<_, _, Vec<_>, Vec<_>>();

        let descriptors = self.descriptor_heap.allocate()?;

        let writes = [
            (
                descriptors.input.descriptor_set,
                std::slice::from_ref(&image_info_input_y),
                0,
            ),
            (
                descriptors.input.descriptor_set,
                std::slice::from_ref(&image_info_input_uv),
                1,
            ),
            (
                descriptors.output_y.descriptor_set,
                &image_infos_output_y,
                0,
            ),
            (
                descriptors.output_uv.descriptor_set,
                &image_infos_output_uv,
                0,
            ),
        ]
        .into_iter()
        .map(|(descriptor_set, image_infos, binding)| {
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(binding)
                .dst_array_element(0)
                .descriptor_count(image_infos.len() as u32)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .image_info(image_infos)
        })
        .collect::<Vec<_>>();
        unsafe { self.device.device.update_descriptor_sets(&writes, &[]) };

        Ok(descriptors)
    }

    pub(crate) fn run(
        &mut self,
        input_submission: &mut DecodeSubmission,
        encoder_trackers: &mut [&mut EncoderTracker],
        input_cropped_extent: vk::Extent2D,
    ) -> Result<ResizeSubmission, TranscoderError> {
        let input = ResizingImageBundle::new(
            input_submission.decode_result.frame.image.clone(),
            input_submission.decode_result.frame.layer,
        )?;
        let outputs = self.image_heap.allocate(encoder_trackers)?;
        let descriptors = self.write_descriptors(&input, &outputs)?;

        let mut buffer = self.buffer_pool.begin_buffer()?;
        self.device
            .device
            .set_label(buffer.buffer(), Some("resize pipeline buffer"))?;

        input.image.transition_layout_single_layer(
            &mut buffer,
            vk::PipelineStageFlags2::NONE..vk::PipelineStageFlags2::COMPUTE_SHADER,
            vk::AccessFlags2::NONE..vk::AccessFlags2::SHADER_STORAGE_READ,
            vk::ImageLayout::GENERAL,
            input_submission.decode_result.frame.layer,
        )?;
        for bundle in outputs.iter() {
            bundle.image.transition_layout_single_layer(
                &mut buffer,
                vk::PipelineStageFlags2::NONE..vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::NONE..vk::AccessFlags2::SHADER_STORAGE_WRITE,
                vk::ImageLayout::GENERAL,
                0,
            )?;
        }

        let dispatch_size = outputs
            .iter()
            .map(|ResizingImageBundle { image, .. }| {
                (image.extent.width.next_multiple_of(16) * image.extent.height.next_multiple_of(16))
                    .div_ceil(256)
            })
            .sum::<u32>();

        unsafe {
            self.device.device.cmd_bind_pipeline(
                buffer.buffer(),
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline.pipeline,
            );
            self.device.device.cmd_bind_descriptor_sets(
                buffer.buffer(),
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline.layout.layout,
                0,
                &[
                    descriptors.input.descriptor_set,
                    descriptors.output_y.descriptor_set,
                    descriptors.output_uv.descriptor_set,
                ],
                &[],
            );

            let push_constants = PushConstants::new(&self.image_heap.configs, input_cropped_extent);
            self.device.device.cmd_push_constants(
                buffer.buffer(),
                self.pipeline.layout.layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                bytemuck::bytes_of(&push_constants),
            );
            self.device
                .device
                .cmd_dispatch(buffer.buffer(), dispatch_size, 1, 1);
        }

        let buffer = buffer.end()?;
        let buffer_info = vk::CommandBufferSubmitInfo::default().command_buffer(buffer.buffer());

        let encoder_semaphore_submit_infos = encoder_trackers
            .iter_mut()
            .map(|tracker| {
                tracker
                    .semaphore_tracker
                    .next_submit_info(EncoderTrackerWaitState::ResizeInput)
            })
            .collect::<Vec<_>>();

        let mut signals = encoder_semaphore_submit_infos
            .iter()
            .map(|c| c.signal_info(vk::PipelineStageFlags2::ALL_COMMANDS))
            .collect::<Vec<_>>();
        let mut waits = encoder_semaphore_submit_infos
            .iter()
            .flat_map(|c| c.wait_info(vk::PipelineStageFlags2::ALL_COMMANDS))
            .collect::<Vec<_>>();

        let decoder_semaphore_submit_info = input_submission
            .decoder
            .tracker
            .semaphore_tracker
            .next_submit_info(DecoderTrackerWaitState::ExternalProcessing);

        if let Some(wait) =
            decoder_semaphore_submit_info.wait_info(vk::PipelineStageFlags2::ALL_COMMANDS)
        {
            waits.push(wait);
        }

        signals
            .push(decoder_semaphore_submit_info.signal_info(vk::PipelineStageFlags2::ALL_COMMANDS));

        let submit_info = vk::SubmitInfo2::default()
            .command_buffer_infos(std::slice::from_ref(&buffer_info))
            .wait_semaphore_infos(&waits)
            .signal_semaphore_infos(&signals);

        unsafe {
            self.device.device.queue_submit2(
                *self.device.queues.compute.queue.lock().unwrap(),
                &[submit_info],
                vk::Fence::null(),
            )?;
        }

        buffer.mark_submitted(input_submission.semaphore_wait_value);
        for semaphore_submit_info in encoder_semaphore_submit_infos {
            semaphore_submit_info.mark_submitted();
        }

        decoder_semaphore_submit_info.mark_submitted();

        Ok(ResizeSubmission {
            outputs,
            _input: input,
            descriptors,
        })
    }

    pub(crate) fn mark_command_buffers_completed(&self, decoder_wait_value: SemaphoreWaitValue) {
        self.buffer_pool.mark_submitted_as_free(decoder_wait_value);
    }
}
