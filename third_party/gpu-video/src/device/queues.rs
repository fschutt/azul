use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ash::vk;

use crate::VulkanCommonError;
use crate::wrappers::*;

#[derive(Clone)]
pub(crate) struct Queue {
    pub(crate) queue: Arc<Mutex<vk::Queue>>,
    pub(crate) family_index: usize,
    pub(crate) _video_properties: vk::QueueFamilyVideoPropertiesKHR<'static>,
    pub(crate) query_result_status_properties:
        vk::QueueFamilyQueryResultStatusPropertiesKHR<'static>,
    pub(crate) device: Arc<Device>,
}

impl Queue {
    pub(crate) fn supports_result_status_queries(&self) -> bool {
        self.query_result_status_properties
            .query_result_status_support
            == vk::TRUE
    }

    pub(crate) fn submit_chain_semaphore<K: TrackerKind>(
        &self,
        buffer: RecordedCommandBuffer,
        tracker: &mut Tracker<K>,
        wait_stages: vk::PipelineStageFlags2,
        signal_stages: vk::PipelineStageFlags2,
        new_wait_state: K::WaitState,
    ) -> Result<SemaphoreWaitValue, VulkanCommonError> {
        let buffer_submit_info =
            [vk::CommandBufferSubmitInfo::default().command_buffer(buffer.buffer())];

        let semaphore_submit_info = tracker.semaphore_tracker.next_submit_info(new_wait_state);
        let signal_info = semaphore_submit_info.signal_info(signal_stages);
        let wait_info = semaphore_submit_info.wait_info(wait_stages);

        let mut submit_info = vk::SubmitInfo2::default()
            .signal_semaphore_infos(std::slice::from_ref(&signal_info))
            .command_buffer_infos(&buffer_submit_info);
        if let Some(wait_info) = wait_info.as_ref() {
            submit_info = submit_info.wait_semaphore_infos(std::slice::from_ref(wait_info));
        }

        unsafe {
            self.device.queue_submit2(
                *self.queue.lock().unwrap(),
                &[submit_info],
                vk::Fence::null(),
            )?
        };

        let value = semaphore_submit_info.signal_value();
        buffer.mark_submitted(value);
        semaphore_submit_info.mark_submitted();
        Ok(value)
    }
}

pub(crate) struct Queues {
    pub(crate) transfer: Queue,
    #[cfg_attr(not(feature = "transcoder"), allow(dead_code))]
    pub(crate) compute: Queue,
    pub(crate) h264_decode: Option<Arc<VideoQueues>>,
    pub(crate) encode: Option<Arc<VideoQueues>>,
    pub(crate) wgpu: Queue,
}

pub(crate) struct QueueIndex<'a> {
    pub(crate) family_index: usize,
    pub(crate) queue_count: usize,
    pub(crate) video_properties: vk::QueueFamilyVideoPropertiesKHR<'a>,
    pub(crate) query_result_status_properties: vk::QueueFamilyQueryResultStatusPropertiesKHR<'a>,
}

pub(crate) struct QueueIndices<'a> {
    pub(crate) transfer: QueueIndex<'a>,
    pub(crate) compute: QueueIndex<'a>,
    pub(crate) h264_decode: Option<QueueIndex<'a>>,
    pub(crate) encode: Option<QueueIndex<'a>>,
    pub(crate) graphics_transfer_compute: QueueIndex<'a>,
}

impl QueueIndices<'_> {
    pub(crate) fn queue_create_infos(&self) -> Vec<QueueCreateInfo> {
        [
            self.h264_decode
                .as_ref()
                .map(|q| (q.family_index, q.queue_count)),
            self.encode
                .as_ref()
                .map(|q| (q.family_index, q.queue_count)),
            Some((self.transfer.family_index, self.transfer.queue_count)),
            (self.compute.family_index != self.transfer.family_index)
                .then_some((self.compute.family_index, self.compute.queue_count)),
            Some((
                self.graphics_transfer_compute.family_index,
                self.graphics_transfer_compute.queue_count,
            )),
        ]
        .into_iter()
        .flatten()
        .collect::<HashSet<(usize, usize)>>()
        .into_iter()
        .map(|(family_idx, queue_count)| QueueCreateInfo::new(family_idx, vec![1.0; queue_count]))
        .collect()
    }
}

pub(crate) struct QueueCreateInfo {
    family_idx: usize,
    priorities: Box<[f32]>,
}

impl QueueCreateInfo {
    fn new(family_idx: usize, priorities: Vec<f32>) -> Self {
        let priorities = priorities.into_boxed_slice();

        Self {
            family_idx,
            priorities,
        }
    }
    pub(crate) fn info(&self) -> vk::DeviceQueueCreateInfo<'_> {
        vk::DeviceQueueCreateInfo::default()
            .queue_family_index(self.family_idx as u32)
            .queue_priorities(&self.priorities)
    }
}

pub(crate) struct VideoQueues {
    queues: Box<[Queue]>,
    current_queue_idx: AtomicUsize,
    pub(crate) family_index: usize,
}

impl VideoQueues {
    pub(crate) fn new(queues: Box<[Queue]>) -> Option<Self> {
        if queues.is_empty() {
            return None;
        }

        let family_index = queues[0].family_index;
        Some(Self {
            queues,
            current_queue_idx: AtomicUsize::new(0),
            family_index,
        })
    }

    fn next_queue(&self) -> &Queue {
        let idx = self.current_queue_idx.fetch_add(1, Ordering::Relaxed);
        &self.queues[idx % self.queues.len()]
    }

    pub(crate) fn supports_result_status_queries(&self) -> bool {
        // All queues from the same family share the same properties
        self.queues[0].supports_result_status_queries()
    }

    pub(crate) fn submit_chain_semaphore<K: TrackerKind>(
        &self,
        buffer: RecordedCommandBuffer,
        tracker: &mut Tracker<K>,
        wait_stages: vk::PipelineStageFlags2,
        signal_stages: vk::PipelineStageFlags2,
        new_wait_state: K::WaitState,
    ) -> Result<SemaphoreWaitValue, VulkanCommonError> {
        let queue = self.next_queue();
        queue.submit_chain_semaphore(buffer, tracker, wait_stages, signal_stages, new_wait_state)
    }
}
