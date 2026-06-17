use std::{
    collections::{VecDeque, hash_map::Entry},
    sync::{Arc, Mutex},
};

use ash::vk::{self, Handle};
use rustc_hash::FxHashMap;

use crate::{
    VulkanCommonError, VulkanDevice,
    wrappers::{ImageKey, ImageLayoutTracker, SemaphoreWaitValue},
};

struct CommandPool {
    command_pool: vk::CommandPool,
    device: Arc<VulkanDevice>,
}

impl CommandPool {
    fn new(
        device: Arc<VulkanDevice>,
        queue_family_index: usize,
    ) -> Result<Self, VulkanCommonError> {
        let create_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index as u32);

        let command_pool = unsafe { device.device.create_command_pool(&create_info, None)? };

        Ok(Self {
            device,
            command_pool,
        })
    }

    fn new_primary(&self) -> Result<vk::CommandBuffer, VulkanCommonError> {
        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(**self)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);

        let buffer = unsafe {
            self.device
                .device
                .allocate_command_buffers(&allocate_info)?[0]
        };

        Ok(buffer)
    }
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        unsafe {
            self.device
                .device
                .destroy_command_pool(self.command_pool, None);
        }
    }
}

impl std::ops::Deref for CommandPool {
    type Target = vk::CommandPool;

    fn deref(&self) -> &Self::Target {
        &self.command_pool
    }
}

pub(crate) struct SubmittedCommandBuffer {
    semaphore_value: SemaphoreWaitValue,
    buffer: vk::CommandBuffer,
}

pub(crate) struct CommandBufferPool(Arc<Mutex<CommandBufferPoolInner>>);

pub(crate) struct CommandBufferPoolInner {
    command_pool: CommandPool,
    free: Vec<vk::CommandBuffer>,
    submitted: VecDeque<SubmittedCommandBuffer>,
}

impl CommandBufferPool {
    pub(crate) fn new(
        device: Arc<VulkanDevice>,
        queue_family_index: usize,
    ) -> Result<Self, VulkanCommonError> {
        let command_pool = CommandPool::new(device, queue_family_index)?;

        Ok(Self(Arc::new(Mutex::new(CommandBufferPoolInner {
            command_pool,
            free: Vec::new(),
            submitted: VecDeque::new(),
        }))))
    }

    pub(crate) fn begin_buffer(&self) -> Result<OpenCommandBuffer, VulkanCommonError> {
        let mut inner = self.0.lock().unwrap();
        let buffer = match inner.free.pop() {
            Some(buffer) => buffer,
            None => inner.command_pool.new_primary()?,
        };

        unsafe {
            inner.command_pool.device.device.begin_command_buffer(
                buffer,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?
        };

        Ok(OpenCommandBuffer(UnfinishedCommandBuffer {
            buffer,
            pool: self.0.clone(),
            image_layout_transitions: Default::default(),
            reset_on_drop: true,
        }))
    }

    pub(crate) fn mark_submitted_as_free(&self, last_waited_semaphore: SemaphoreWaitValue) {
        let mut guard = self.0.lock().unwrap();
        let inner = &mut *guard;

        let Some(last) = inner
            .submitted
            .iter()
            .enumerate()
            .filter(|(_, b)| b.semaphore_value <= last_waited_semaphore)
            .map(|(i, _)| i)
            .next_back()
        else {
            return;
        };

        inner
            .free
            .extend(inner.submitted.drain(..=last).map(|b| b.buffer));
    }
}

struct ImageLayoutChange {
    tracker: Arc<Mutex<ImageLayoutTracker>>,
    new_layout: Box<[vk::ImageLayout]>,
}

struct UnfinishedCommandBuffer {
    buffer: vk::CommandBuffer,
    pool: Arc<Mutex<CommandBufferPoolInner>>,
    image_layout_transitions: FxHashMap<ImageKey, ImageLayoutChange>,
    reset_on_drop: bool,
}

impl UnfinishedCommandBuffer {
    fn destroy_without_reset(mut self) {
        self.reset_on_drop = false;
    }
}

impl Drop for UnfinishedCommandBuffer {
    fn drop(&mut self) {
        if !self.reset_on_drop {
            return;
        }

        let mut locked = self.pool.lock().unwrap();
        unsafe {
            if let Err(e) = locked
                .command_pool
                .device
                .device
                .reset_command_buffer(self.buffer, vk::CommandBufferResetFlags::empty())
            {
                tracing::error!(
                    "Open command buffer {:x} failed when resetting: {e}. Something is very wrong",
                    self.buffer.as_raw()
                );
            }
        }

        locked.free.push(self.buffer);
    }
}

pub(crate) struct OpenCommandBuffer(UnfinishedCommandBuffer);

impl OpenCommandBuffer {
    pub(crate) fn end(self) -> Result<RecordedCommandBuffer, VulkanCommonError> {
        let buffer = self.0.buffer;
        unsafe {
            self.0
                .pool
                .lock()
                .unwrap()
                .command_pool
                .device
                .device
                .end_command_buffer(buffer)?
        }

        Ok(RecordedCommandBuffer(self.0))
    }

    pub(crate) fn buffer(&self) -> vk::CommandBuffer {
        self.0.buffer
    }

    pub(crate) fn image_layout(
        &mut self,
        image: ImageKey,
        tracker: &Arc<Mutex<ImageLayoutTracker>>,
    ) -> Result<&mut [vk::ImageLayout], VulkanCommonError> {
        let entry = self.0.image_layout_transitions.entry(image);

        match entry {
            Entry::Occupied(entry) => Ok(&mut entry.into_mut().new_layout),
            Entry::Vacant(entry) => Ok(&mut entry
                .insert(ImageLayoutChange {
                    tracker: tracker.clone(),
                    new_layout: tracker
                        .lock()
                        .unwrap()
                        .map
                        .get(&image)
                        .ok_or(VulkanCommonError::TriedToAccessNonexistentImageState(image))?
                        .clone(),
                })
                .new_layout),
        }
    }
}

pub(crate) struct RecordedCommandBuffer(UnfinishedCommandBuffer);

impl RecordedCommandBuffer {
    pub(crate) fn mark_submitted(mut self, semaphore_value: SemaphoreWaitValue) {
        self.0
            .pool
            .lock()
            .unwrap()
            .submitted
            .push_back(SubmittedCommandBuffer {
                semaphore_value,
                buffer: self.0.buffer,
            });

        for (key, layout_change) in self.0.image_layout_transitions.drain() {
            layout_change
                .tracker
                .lock()
                .unwrap()
                .map
                .insert(key, layout_change.new_layout);
        }

        self.0.destroy_without_reset();
    }

    pub(crate) fn buffer(&self) -> vk::CommandBuffer {
        self.0.buffer
    }
}
