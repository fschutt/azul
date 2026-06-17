use std::{
    collections::hash_map::Entry,
    sync::{Arc, Mutex},
};

use ash::vk;
use rustc_hash::FxHashMap;

use crate::{VulkanCommonError, wrappers::ImageKey};

use super::Device;

pub(crate) struct TimelineSemaphore {
    pub(crate) semaphore: vk::Semaphore,
    device: Arc<Device>,
}

impl TimelineSemaphore {
    pub(crate) fn new(
        device: Arc<Device>,
        initial_value: u64,
        label: Option<&str>,
    ) -> Result<Self, VulkanCommonError> {
        let mut create_type_info = vk::SemaphoreTypeCreateInfo::default()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(initial_value);
        let create_info = vk::SemaphoreCreateInfo::default().push_next(&mut create_type_info);
        let semaphore = unsafe { device.create_semaphore(&create_info, None)? };

        device.set_label(semaphore, label)?;

        Ok(Self { semaphore, device })
    }

    pub(crate) fn wait(
        &self,
        timeout: u64,
        value: SemaphoreWaitValue,
    ) -> Result<(), VulkanCommonError> {
        let wait_info = vk::SemaphoreWaitInfo::default()
            .semaphores(std::slice::from_ref(&self.semaphore))
            .values(std::slice::from_ref(&value.0));

        unsafe { self.device.wait_semaphores(&wait_info, timeout)? };

        Ok(())
    }
}

impl Drop for TimelineSemaphore {
    fn drop(&mut self) {
        unsafe { self.device.destroy_semaphore(self.semaphore, None) };
    }
}

pub(crate) trait TrackerKind {
    type WaitState;
    type CommandBufferPools: CommandBufferPoolStorage;
}

pub(crate) trait CommandBufferPoolStorage: Sized {
    fn mark_submitted_as_free(&mut self, last_waited_for: SemaphoreWaitValue);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SemaphoreWaitValue(pub(crate) u64);

pub(crate) struct TrackerWait<S> {
    pub(crate) value: SemaphoreWaitValue,
    pub(crate) _state: S,
}

impl<S: Clone> Clone for TrackerWait<S> {
    fn clone(&self) -> Self {
        Self {
            value: self.value,
            _state: self._state.clone(),
        }
    }
}

impl<S: Copy> Copy for TrackerWait<S> {}

pub(crate) struct Tracker<K: TrackerKind> {
    pub(crate) semaphore_tracker: SemaphoreTracker<K::WaitState>,
    pub(crate) command_buffer_pools: K::CommandBufferPools,
    pub(crate) image_layout_tracker: Arc<Mutex<ImageLayoutTracker>>,
}

impl<K: TrackerKind> Tracker<K> {
    pub(crate) fn new(
        device: Arc<Device>,
        command_buffer_pools: K::CommandBufferPools,
        label: Option<&str>,
    ) -> Result<Self, VulkanCommonError> {
        let semaphore_tracker = SemaphoreTracker::new(
            device,
            label.map(|name| format!("{name} semaphore")).as_deref(),
        )?;

        Ok(Self {
            semaphore_tracker,
            command_buffer_pools,
            image_layout_tracker: Default::default(),
        })
    }

    #[allow(dead_code)]
    pub(crate) fn wait_for_all(&mut self, timeout: u64) -> Result<(), VulkanCommonError> {
        let waited_for = self.semaphore_tracker.wait_for_all(timeout)?;

        if let Some(waited_for) = waited_for {
            self.mark_waited(waited_for);
        }

        Ok(())
    }

    pub(crate) fn wait_for(
        &mut self,
        value: SemaphoreWaitValue,
        timeout: u64,
    ) -> Result<(), VulkanCommonError> {
        self.semaphore_tracker.wait_for(value, timeout)?;
        self.mark_waited(value);
        Ok(())
    }

    /// Call this to mark that this value was waited for already
    pub(crate) fn mark_waited(&mut self, value: SemaphoreWaitValue) {
        self.command_buffer_pools.mark_submitted_as_free(value);
    }
}

pub(crate) struct SemaphoreSubmitInfo<'a, S> {
    pub(crate) signal: TrackerWait<S>,
    tracker: &'a mut SemaphoreTracker<S>,

    #[cfg(feature = "wgpu")]
    wgpu_fence: wgpu::hal::vulkan::Fence,
}

impl<'a, S> SemaphoreSubmitInfo<'a, S> {
    pub(crate) fn wait_info(
        &self,
        stage: vk::PipelineStageFlags2,
    ) -> Option<vk::SemaphoreSubmitInfo<'_>> {
        self.tracker.wait_for.as_ref().map(|w| {
            vk::SemaphoreSubmitInfo::default()
                .stage_mask(stage)
                .value(w.value.0)
                .semaphore(self.tracker.semaphore.semaphore)
        })
    }

    #[cfg(feature = "wgpu")]
    pub(crate) fn wgpu_wait_info(&mut self) -> (&mut wgpu::hal::vulkan::Fence, u64) {
        (&mut self.wgpu_fence, self.signal.value.0)
    }

    pub(crate) fn signal_info(
        &self,
        stage: vk::PipelineStageFlags2,
    ) -> vk::SemaphoreSubmitInfo<'_> {
        vk::SemaphoreSubmitInfo::default()
            .stage_mask(stage)
            .value(self.signal.value.0)
            .semaphore(self.tracker.semaphore.semaphore)
    }

    pub(crate) fn signal_value(&self) -> SemaphoreWaitValue {
        self.signal.value
    }

    pub(crate) fn mark_submitted(self) {
        self.tracker.wait_for = Some(self.signal);
    }
}

pub(crate) struct SemaphoreTracker<S> {
    pub(crate) semaphore: TimelineSemaphore,
    next_value: u64,
    pub(crate) wait_for: Option<TrackerWait<S>>,
    last_waited_for: Option<SemaphoreWaitValue>,
}

impl<S> SemaphoreTracker<S> {
    pub(crate) fn new(device: Arc<Device>, label: Option<&str>) -> Result<Self, VulkanCommonError> {
        Ok(Self {
            next_value: 1,
            wait_for: None,
            last_waited_for: None,
            semaphore: TimelineSemaphore::new(device, 0, label)?,
        })
    }

    pub(crate) fn next_sem_value(&mut self) -> SemaphoreWaitValue {
        let val = self.next_value;
        self.next_value += 1;
        SemaphoreWaitValue(val)
    }

    pub(crate) fn next_submit_info(&mut self, new_state: S) -> SemaphoreSubmitInfo<'_, S> {
        let signal = TrackerWait {
            value: self.next_sem_value(),
            _state: new_state,
        };

        SemaphoreSubmitInfo {
            signal,
            #[cfg(feature = "wgpu")]
            wgpu_fence: wgpu::hal::vulkan::Fence::TimelineSemaphore(self.semaphore.semaphore),
            tracker: self,
        }
    }

    /// This is a noop if there's nothing to wait for
    #[allow(dead_code)]
    pub(crate) fn wait_for_all(
        &mut self,
        timeout: u64,
    ) -> Result<Option<SemaphoreWaitValue>, VulkanCommonError> {
        if let Some(wait_for) = self.wait_for.as_ref() {
            let waited_for = wait_for.value;
            self.semaphore.wait(timeout, waited_for)?;
            self.wait_for = None;

            match self.last_waited_for {
                Some(old_value) => self.last_waited_for = Some(old_value.max(waited_for)),
                None => self.last_waited_for = Some(waited_for),
            }

            return Ok(Some(waited_for));
        }

        Ok(None)
    }

    pub(crate) fn wait_for(
        &mut self,
        value: SemaphoreWaitValue,
        timeout: u64,
    ) -> Result<(), VulkanCommonError> {
        if let Some(last) = self.last_waited_for.as_ref()
            && *last >= value
        {
            return Ok(());
        }

        let Some(final_wait_for) = self.wait_for.as_mut() else {
            return Err(VulkanCommonError::SemaphoreWaitOnUnsignaledValue);
        };

        if final_wait_for.value < value {
            return Err(VulkanCommonError::SemaphoreWaitOnUnsignaledValue);
        }

        self.semaphore.wait(timeout, value)?;

        if final_wait_for.value == value {
            self.wait_for = None;
        }

        self.last_waited_for = Some(value);

        Ok(())
    }
}

#[derive(Debug, Default)]
pub(crate) struct ImageLayoutTracker {
    pub(crate) map: FxHashMap<ImageKey, Box<[vk::ImageLayout]>>,
}

impl ImageLayoutTracker {
    pub(crate) fn register_image(
        &mut self,
        image: ImageKey,
        initial_layout: vk::ImageLayout,
        array_layers: usize,
    ) -> Result<(), VulkanCommonError> {
        match self.map.entry(image) {
            Entry::Occupied(_) => Err(VulkanCommonError::RegisteredNewImageTwice(image)),
            Entry::Vacant(entry) => {
                entry.insert(vec![initial_layout; array_layers].into_boxed_slice());
                Ok(())
            }
        }
    }

    pub(crate) fn unregister_image(&mut self, image: ImageKey) -> Result<(), VulkanCommonError> {
        if self.map.remove(&image).is_none() {
            return Err(VulkanCommonError::UnregisteredNonexistentImage(image));
        }

        Ok(())
    }
}
