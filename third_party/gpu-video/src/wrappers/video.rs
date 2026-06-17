use std::sync::{Arc, Mutex};

use ash::vk;

use crate::{
    VulkanCommonError, VulkanDevice,
    codec::Codec,
    device::queues::VideoQueues,
    parser::reference_manager::{PictureInfo, ReferencePictureInfo},
    wrappers::{ImageLayoutTracker, OpenCommandBuffer},
};

use super::{Device, Image, ImageView, MemoryAllocation, VideoQueueExt};

pub(crate) struct VideoSessionParameters {
    pub(crate) parameters: vk::VideoSessionParametersKHR,
    device: Arc<Device>,
}

impl VideoSessionParameters {
    pub(crate) fn new<C: Codec>(
        device: Arc<Device>,
        session: vk::VideoSessionKHR,
        initial_parameters: C::VkParameters<'_>,
        template: Option<&Self>,
        encode_quality_level: Option<u32>,
    ) -> Result<Self, VulkanCommonError> {
        let decode_add_info = C::decode_parameters_add_info(&initial_parameters);

        let encode_add_info = C::encode_parameters_add_info(&initial_parameters);

        let mut quality_level = vk::VideoEncodeQualityLevelInfoKHR::default();

        let mut create_info = vk::VideoSessionParametersCreateInfoKHR::default()
            .flags(vk::VideoSessionParametersCreateFlagsKHR::empty())
            .video_session_parameters_template(
                template
                    .map(|t| t.parameters)
                    .unwrap_or_else(vk::VideoSessionParametersKHR::null),
            )
            .video_session(session);

        let mut decode_create_info = C::decode_parameters_create_info(&decode_add_info);

        let mut encode_create_info = C::encode_parameters_create_info(&encode_add_info);

        if let Some(encode_quality_level) = encode_quality_level {
            quality_level = quality_level.quality_level(encode_quality_level);
            create_info = create_info
                .push_next(&mut encode_create_info)
                .push_next(&mut quality_level);
        } else {
            create_info = create_info.push_next(&mut decode_create_info);
        }

        let parameters = unsafe {
            device
                .video_queue_ext
                .create_video_session_parameters_khr(&create_info, None)?
        };

        Ok(Self {
            parameters,
            device: device.clone(),
        })
    }

    pub(crate) fn add(
        &self,
        sps: &[vk::native::StdVideoH264SequenceParameterSet],
        pps: &[vk::native::StdVideoH264PictureParameterSet],
        update_sequence_count: u32,
    ) -> Result<(), VulkanCommonError> {
        let mut parameters_add_info = vk::VideoDecodeH264SessionParametersAddInfoKHR::default()
            .std_sp_ss(sps)
            .std_pp_ss(pps);

        let update_info = vk::VideoSessionParametersUpdateInfoKHR::default()
            .update_sequence_count(update_sequence_count)
            .push_next(&mut parameters_add_info);

        unsafe {
            self.device
                .video_queue_ext
                .update_video_session_parameters_khr(self.parameters, &update_info)?
        };

        Ok(())
    }
}

impl Drop for VideoSessionParameters {
    fn drop(&mut self) {
        unsafe {
            self.device
                .video_queue_ext
                .destroy_video_session_parameters_khr(self.parameters, None)
        }
    }
}

pub(crate) struct VideoSession {
    pub(crate) session: vk::VideoSessionKHR,
    pub(crate) device: Arc<Device>,
    pub(crate) _allocations: Vec<MemoryAllocation>,
    pub(crate) max_coded_extent: vk::Extent2D,
    pub(crate) max_dpb_slots: u32,
}

impl VideoSession {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        vulkan_ctx: &VulkanDevice,
        queue: &VideoQueues,
        profile_info: &vk::VideoProfileInfoKHR,
        max_coded_extent: vk::Extent2D,
        max_dpb_slots: u32,
        max_active_references: u32,
        flags: vk::VideoSessionCreateFlagsKHR,
        std_header_version: &vk::ExtensionProperties,
    ) -> Result<Self, VulkanCommonError> {
        // TODO: this probably works, but this format needs to be detected and set
        // based on what the GPU supports
        let format = vk::Format::G8_B8R8_2PLANE_420_UNORM;

        let session_create_info = vk::VideoSessionCreateInfoKHR::default()
            .queue_family_index(queue.family_index as u32)
            .video_profile(profile_info)
            .picture_format(format)
            .flags(flags)
            .max_coded_extent(max_coded_extent)
            .reference_picture_format(format)
            .max_dpb_slots(max_dpb_slots)
            .max_active_reference_pictures(max_active_references)
            .std_header_version(std_header_version);

        let video_session = unsafe {
            vulkan_ctx
                .device
                .video_queue_ext
                .create_video_session_khr(&session_create_info, None)?
        };

        let memory_requirements = unsafe {
            vulkan_ctx
                .device
                .video_queue_ext
                .get_video_session_memory_requirements_khr(video_session)?
        };

        let allocations = memory_requirements
            .iter()
            .map(|req| {
                MemoryAllocation::new(
                    vulkan_ctx.allocator.clone(),
                    &req.memory_requirements,
                    &vk_mem::AllocationCreateInfo {
                        usage: vk_mem::MemoryUsage::Unknown,
                        // Mesa driver returns alignment 0 which means that each allocation must have memory offset set to 0,
                        // so every allocation must be a separate memory block
                        flags: vk_mem::AllocationCreateFlags::DEDICATED_MEMORY,
                        ..Default::default()
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let memory_bind_infos = memory_requirements
            .into_iter()
            .zip(allocations.iter())
            .map(|(req, allocation)| {
                let allocation_info = allocation.allocation_info();
                vk::BindVideoSessionMemoryInfoKHR::default()
                    .memory_bind_index(req.memory_bind_index)
                    .memory(allocation_info.device_memory)
                    .memory_offset(allocation_info.offset)
                    .memory_size(allocation_info.size)
            })
            .collect::<Vec<_>>();

        unsafe {
            vulkan_ctx
                .device
                .video_queue_ext
                .bind_video_session_memory_khr(video_session, &memory_bind_infos)?
        };

        Ok(VideoSession {
            session: video_session,
            _allocations: allocations,
            device: vulkan_ctx.device.clone(),
            max_coded_extent,
            max_dpb_slots,
        })
    }
}

impl Drop for VideoSession {
    fn drop(&mut self) {
        unsafe {
            self.device
                .video_queue_ext
                .destroy_video_session_khr(self.session, None)
        };
    }
}

impl From<ReferencePictureInfo> for vk::native::StdVideoDecodeH264ReferenceInfo {
    fn from(picture_info: ReferencePictureInfo) -> Self {
        vk::native::StdVideoDecodeH264ReferenceInfo {
            flags: vk::native::StdVideoDecodeH264ReferenceInfoFlags {
                __bindgen_padding_0: [0; 3],
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoDecodeH264ReferenceInfoFlags::new_bitfield_1(
                    0,
                    0,
                    picture_info.is_long_term().into(),
                    picture_info.non_existing.into(),
                ),
            },
            FrameNum: picture_info.FrameNum,
            PicOrderCnt: picture_info.PicOrderCnt,
            reserved: 0,
        }
    }
}

impl From<PictureInfo> for vk::native::StdVideoDecodeH264ReferenceInfo {
    fn from(picture_info: PictureInfo) -> Self {
        vk::native::StdVideoDecodeH264ReferenceInfo {
            flags: vk::native::StdVideoDecodeH264ReferenceInfoFlags {
                __bindgen_padding_0: [0; 3],
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoDecodeH264ReferenceInfoFlags::new_bitfield_1(
                    0,
                    0,
                    picture_info.used_for_long_term_reference.into(),
                    picture_info.non_existing.into(),
                ),
            },
            FrameNum: picture_info.FrameNum,
            PicOrderCnt: picture_info.PicOrderCnt_as_reference_pic,
            reserved: 0,
        }
    }
}

pub(crate) enum ImageWithView {
    Single {
        image: Arc<Image>,
        image_view: ImageView,
    },

    Multiple {
        images: Vec<Arc<Image>>,
        image_views: Vec<ImageView>,
    },
}

impl ImageWithView {
    fn extent(&self) -> vk::Extent3D {
        match self {
            ImageWithView::Single { image, .. } => image.extent,
            ImageWithView::Multiple { images, .. } => images[0].extent,
        }
    }

    pub(crate) fn target_info(&self, index: usize) -> Arc<Image> {
        match self {
            ImageWithView::Single { image, .. } => image.clone(),
            ImageWithView::Multiple { images, .. } => images[index].clone(),
        }
    }

    fn base_array_layer(&self, index: u32) -> u32 {
        match self {
            ImageWithView::Single { .. } => index,
            ImageWithView::Multiple { .. } => 0,
        }
    }

    fn image_view(&self, index: u32) -> &ImageView {
        match self {
            ImageWithView::Single { image_view, .. } => image_view,
            ImageWithView::Multiple { image_views, .. } => &image_views[index as usize],
        }
    }

    pub(crate) fn transition_layout(
        &self,
        command_buffer: &mut OpenCommandBuffer,
        stages: std::ops::Range<vk::PipelineStageFlags2>,
        accesses: std::ops::Range<vk::AccessFlags2>,
        new_layout: vk::ImageLayout,
        subresource_range: vk::ImageSubresourceRange,
    ) -> Result<(), VulkanCommonError> {
        match self {
            ImageWithView::Single { image, .. } => image.transition_layout(
                command_buffer,
                stages,
                accesses,
                new_layout,
                subresource_range,
            ),

            ImageWithView::Multiple { images, .. } => {
                let start_layer = subresource_range.base_array_layer as usize;
                let end_layer = if subresource_range.layer_count == vk::REMAINING_ARRAY_LAYERS {
                    images.len()
                } else {
                    start_layer + subresource_range.layer_count as usize
                };

                for image in &images[start_layer..end_layer] {
                    let subresource_range = subresource_range.base_array_layer(0).layer_count(1);
                    image.transition_layout(
                        command_buffer,
                        stages.clone(),
                        accesses.clone(),
                        new_layout,
                        subresource_range,
                    )?;
                }

                Ok(())
            }
        }
    }
}

pub(crate) struct CodingImageBundle<'a> {
    pub(crate) image_with_view: Arc<ImageWithView>,
    pub(crate) video_resource_info: Vec<vk::VideoPictureResourceInfoKHR<'a>>,
}

impl<'a> CodingImageBundle<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        vulkan_ctx: &VulkanDevice,
        command_buffer: &mut OpenCommandBuffer,
        image_tracker: Arc<Mutex<ImageLayoutTracker>>,
        format: &vk::VideoFormatPropertiesKHR<'a>,
        dimensions: vk::Extent2D,
        image_usage: vk::ImageUsageFlags,
        use_separate_images: bool,
        profile_info: &vk::VideoProfileInfoKHR,
        array_layer_count: u32,
        queue_indices: Option<&[u32]>,
        layout: vk::ImageLayout,
    ) -> Result<Self, VulkanCommonError> {
        let mut profile_list_info =
            vk::VideoProfileListInfoKHR::default().profiles(std::slice::from_ref(profile_info));

        let mut image_create_info = vk::ImageCreateInfo::default()
            .flags(format.image_create_flags)
            .image_type(format.image_type)
            .format(format.format)
            .extent(vk::Extent3D {
                width: dimensions.width,
                height: dimensions.height,
                depth: 1,
            })
            .mip_levels(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(format.image_tiling)
            .usage(image_usage)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut profile_list_info);

        match queue_indices {
            Some(indices) => {
                image_create_info = image_create_info
                    .sharing_mode(vk::SharingMode::CONCURRENT)
                    .queue_family_indices(indices);
            }
            None => {
                image_create_info = image_create_info.sharing_mode(vk::SharingMode::EXCLUSIVE);
            }
        }

        let mut image_view_usage_info = vk::ImageViewUsageCreateInfo::default()
            .usage(image_usage & (!vk::ImageUsageFlags::STORAGE));

        let mut image_view_create_info = vk::ImageViewCreateInfo::default()
            .flags(vk::ImageViewCreateFlags::empty())
            .components(vk::ComponentMapping::default())
            .format(format.format)
            .push_next(&mut image_view_usage_info);

        let subresource_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: vk::REMAINING_ARRAY_LAYERS,
        };

        let accesses = vk::AccessFlags2::NONE..vk::AccessFlags2::NONE;
        let stages = vk::PipelineStageFlags2::NONE..vk::PipelineStageFlags2::NONE;

        let image_with_view = if use_separate_images {
            let images = (0..array_layer_count)
                .map(|_| {
                    image_create_info = image_create_info.array_layers(1);
                    Image::new(
                        vulkan_ctx.allocator.clone(),
                        &image_create_info,
                        image_tracker.clone(),
                    )
                    .map(Arc::new)
                    .and_then(|i| {
                        vulkan_ctx
                            .device
                            .set_label(i.image, Some("decoding image"))?;
                        Ok(i)
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            let image_views = (0..array_layer_count)
                .map(|i| {
                    let subresource_range = vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    };

                    let image_view_create_info = image_view_create_info
                        .image(images[i as usize].image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .subresource_range(subresource_range);

                    ImageView::new(
                        vulkan_ctx.device.clone(),
                        images[i as usize].clone(),
                        &image_view_create_info,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;

            for image in &images {
                image.transition_layout(
                    command_buffer,
                    stages.clone(),
                    accesses.clone(),
                    layout,
                    subresource_range,
                )?;
            }

            ImageWithView::Multiple {
                images,
                image_views,
            }
        } else {
            image_create_info = image_create_info.array_layers(array_layer_count);
            let image = Arc::new(Image::new(
                vulkan_ctx.allocator.clone(),
                &image_create_info,
                image_tracker.clone(),
            )?);

            vulkan_ctx
                .device
                .set_label(image.image, Some("decoding image"))?;

            image_view_create_info = image_view_create_info
                .image(image.image)
                .view_type(vk::ImageViewType::TYPE_2D_ARRAY)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: vk::REMAINING_ARRAY_LAYERS,
                });

            let image_view = ImageView::new(
                vulkan_ctx.device.clone(),
                image.clone(),
                &image_view_create_info,
            )?;

            image.transition_layout(
                command_buffer,
                stages.clone(),
                accesses.clone(),
                layout,
                subresource_range,
            )?;

            ImageWithView::Single { image, image_view }
        };

        let video_resource_info = (0..array_layer_count)
            .map(|i| {
                vk::VideoPictureResourceInfoKHR::default()
                    .coded_offset(vk::Offset2D { x: 0, y: 0 })
                    .coded_extent(dimensions)
                    .base_array_layer(image_with_view.base_array_layer(i))
                    .image_view_binding(image_with_view.image_view(i).view)
            })
            .collect();

        Ok(Self {
            image_with_view: Arc::new(image_with_view),
            video_resource_info,
        })
    }

    pub(crate) fn extent(&self) -> vk::Extent3D {
        self.image_with_view.extent()
    }
}

pub(crate) struct DecodedPicturesBuffer<'a> {
    pub(crate) image: CodingImageBundle<'a>,
    pub(crate) slot_active_bitmap: u32,
    pub(crate) len: u8,
}

impl<'a> DecodedPicturesBuffer<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        vulkan_ctx: &VulkanDevice,
        command_buffer: &mut OpenCommandBuffer,
        image_tracker: Arc<Mutex<ImageLayoutTracker>>,
        use_separate_images: bool,
        profile_info: &vk::VideoProfileInfoKHR,
        image_usage: vk::ImageUsageFlags,
        format: &vk::VideoFormatPropertiesKHR<'a>,
        dimensions: vk::Extent2D,
        max_dpb_slots: u32,
        queue_indices: Option<&'_ [u32]>,
        layout: vk::ImageLayout,
    ) -> Result<Self, VulkanCommonError> {
        if max_dpb_slots > 32 {
            return Err(VulkanCommonError::DpbTooLong(max_dpb_slots));
        }

        let image = CodingImageBundle::new(
            vulkan_ctx,
            command_buffer,
            image_tracker,
            format,
            dimensions,
            image_usage,
            use_separate_images,
            profile_info,
            max_dpb_slots,
            queue_indices,
            layout,
        )?;

        Ok(Self {
            image,
            slot_active_bitmap: 0,
            len: max_dpb_slots as u8,
        })
    }

    pub(crate) fn reference_slot_info(&self) -> Vec<vk::VideoReferenceSlotInfoKHR<'_>> {
        self.image
            .video_resource_info
            .iter()
            .enumerate()
            .map(|(i, info)| {
                vk::VideoReferenceSlotInfoKHR::default()
                    .picture_resource(info)
                    .slot_index(if self.slot_active(i) { i as i32 } else { -1 })
            })
            .collect()
    }

    pub(crate) fn allocate_reference_picture(&mut self) -> Result<usize, VulkanCommonError> {
        let i = self.slot_active_bitmap.trailing_ones();

        if i >= self.len.into() {
            return Err(VulkanCommonError::NoFreeSlotsInDpb);
        }

        self.slot_active_bitmap |= 1 << i;

        Ok(i as usize)
    }

    pub(crate) fn video_resource_info(
        &self,
        i: usize,
    ) -> Option<&vk::VideoPictureResourceInfoKHR<'_>> {
        self.image.video_resource_info.get(i)
    }

    #[inline(always)]
    pub(crate) fn free_reference_picture(&mut self, i: usize) {
        self.slot_active_bitmap &= !(1 << i);
    }

    #[inline(always)]
    pub(crate) fn reset_all_allocations(&mut self) {
        self.slot_active_bitmap = 0;
    }

    #[inline(always)]
    pub(crate) fn slot_active(&self, i: usize) -> bool {
        self.slot_active_bitmap & (1 << i) != 0
    }
}
