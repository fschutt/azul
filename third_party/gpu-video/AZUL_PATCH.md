# Vendored gpu-video 0.4.0 — azul local patch

This is an unmodified copy of [`gpu-video` 0.4.0](https://crates.io/crates/gpu-video)
(Software Mansion, MIT) **except** for one targeted change, wired in via
`[patch.crates-io]` in the workspace root `Cargo.toml`.

## The patch

`src/adapter.rs`, `VulkanAdapter::new`: the adapter-selection code required a
queue family with `COMPUTE && !GRAPHICS` (a dedicated async-compute queue) and
returned `None` for the whole adapter otherwise. NVIDIA **Maxwell** GPUs (e.g.
GTX 950/960/970/980) only expose `COMPUTE` on the combined graphics family, so
upstream rejects them with *"Cannot find a suitable physical device"* even
though they fully support `VK_KHR_video_decode_h264`.

The compute queue is only ever submitted to by the optional `transcoder`
feature (off here) — the `BytesDecoder` decode path uses the video-decode and
transfer queues only. So the patch keeps the preference for a dedicated compute
queue but **falls back to any compute-capable queue** instead of failing.

Two linked edits, both in `src/adapter.rs` (search `azul patch`):

1. `compute_queue_idx`: `.or_else(...)` fallback to any compute-capable queue.
2. `compute` `QueueIndex.queue_count`: hard-capped to `1`. Required by (1): when
   the fallback picks the graphics family, `compute` and
   `graphics_transfer_compute` share that family. `QueueIndices::queue_create_infos`
   dedups by the `(family_index, queue_count)` **tuple**, and gtc already uses
   count 1 while compute used the family's full count (16 on this GPU) — so family
   0 appeared **twice** in `VkDeviceCreateInfo::queue_create_infos`, an illegal
   duplicate that **segfaults the NVIDIA driver inside `vkCreateDevice`**. Only
   compute queue index 0 is ever retrieved (`device.rs`), so capping to 1 makes the
   two entries identical and they dedup to one.

## Updating

When bumping the upstream version, re-extract the crate and re-apply the single
`.or_else(...)` fallback on `compute_queue_idx`. Remove this vendor entirely once
upstream relaxes the requirement (a PR to that effect is the right long-term fix).
