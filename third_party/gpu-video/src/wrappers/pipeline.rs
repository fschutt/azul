use std::sync::Arc;

use ash::vk;

use crate::VulkanCommonError;

use super::Device;

pub(crate) struct DescriptorSetLayout {
    device: Arc<Device>,
    pub(crate) set_layout: vk::DescriptorSetLayout,
}

impl DescriptorSetLayout {
    pub(crate) fn new(
        device: Arc<Device>,
        create_info: &vk::DescriptorSetLayoutCreateInfo,
    ) -> Result<Self, VulkanCommonError> {
        let set_layout = unsafe { device.create_descriptor_set_layout(create_info, None)? };

        Ok(Self { device, set_layout })
    }
}

impl Drop for DescriptorSetLayout {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_descriptor_set_layout(self.set_layout, None)
        };
    }
}

pub(crate) struct DescriptorPool {
    device: Arc<Device>,
    pub(crate) pool: vk::DescriptorPool,
}

impl DescriptorPool {
    pub(crate) fn new(
        device: Arc<Device>,
        create_info: &vk::DescriptorPoolCreateInfo,
    ) -> Result<Self, VulkanCommonError> {
        let pool = unsafe { device.create_descriptor_pool(create_info, None)? };
        Ok(Self { device, pool })
    }
}

impl Drop for DescriptorPool {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_descriptor_pool(self.pool, None);
        }
    }
}

pub(crate) struct DescriptorSet {
    pub(crate) descriptor_set: vk::DescriptorSet,
    _pool: Arc<DescriptorPool>,
}

impl DescriptorSet {
    pub(crate) fn new(
        pool: Arc<DescriptorPool>,
        allocate_info: &vk::DescriptorSetAllocateInfo,
    ) -> Result<Vec<Self>, VulkanCommonError> {
        let allocate_info = allocate_info.descriptor_pool(pool.pool);
        let result = unsafe { pool.device.allocate_descriptor_sets(&allocate_info)? };
        Ok(result
            .into_iter()
            .map(|set| DescriptorSet {
                descriptor_set: set,
                _pool: pool.clone(),
            })
            .collect())
    }
}

pub(crate) struct PipelineLayout {
    pub(crate) layout: vk::PipelineLayout,
    device: Arc<Device>,
    _descriptor_set_layouts: Vec<Arc<DescriptorSetLayout>>,
}

impl PipelineLayout {
    pub(crate) fn new(
        device: Arc<Device>,
        create_info: &vk::PipelineLayoutCreateInfo,
        descriptor_set_layouts: Vec<Arc<DescriptorSetLayout>>,
    ) -> Result<Self, VulkanCommonError> {
        let layout = unsafe { device.create_pipeline_layout(create_info, None)? };

        Ok(Self {
            layout,
            device,
            _descriptor_set_layouts: descriptor_set_layouts,
        })
    }
}

impl Drop for PipelineLayout {
    fn drop(&mut self) {
        unsafe { self.device.destroy_pipeline_layout(self.layout, None) };
    }
}

pub(crate) struct ShaderModule {
    device: Arc<Device>,
    pub(crate) module: vk::ShaderModule,
}

impl ShaderModule {
    pub(crate) fn new(
        device: Arc<Device>,
        create_info: &vk::ShaderModuleCreateInfo,
    ) -> Result<Self, VulkanCommonError> {
        let module = unsafe { device.create_shader_module(create_info, None)? };
        Ok(Self { device, module })
    }
}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        unsafe { self.device.destroy_shader_module(self.module, None) };
    }
}

pub(crate) struct ComputePipeline {
    pub(crate) pipeline: vk::Pipeline,
    pub(crate) layout: Arc<PipelineLayout>,
    _shader_module: Arc<ShaderModule>,
    device: Arc<Device>,
}

impl ComputePipeline {
    pub(crate) fn new(
        device: Arc<Device>,
        create_info: vk::ComputePipelineCreateInfo,
        layout: Arc<PipelineLayout>,
        shader_module: Arc<ShaderModule>,
    ) -> Result<Self, VulkanCommonError> {
        let pipeline = unsafe {
            device.create_compute_pipelines(vk::PipelineCache::null(), &[create_info], None)
        }
        .map_err(|(_, e)| e)?[0];

        Ok(Self {
            pipeline,
            layout,
            _shader_module: shader_module,
            device,
        })
    }
}

impl Drop for ComputePipeline {
    fn drop(&mut self) {
        unsafe { self.device.destroy_pipeline(self.pipeline, None) };
    }
}
