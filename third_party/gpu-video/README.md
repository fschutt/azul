# gpu-video

A library for hardware video decoding and encoding using Vulkan Video, with [wgpu] integration.

[![Crates.io][crates-badge]][crates-url]
[![docs.rs][docs-badge]][docs-url]
[![MIT licensed][mit-badge]][mit-url]

[crates-badge]: https://img.shields.io/crates/v/gpu-video
[crates-url]: https://crates.io/crates/gpu-video
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/software-mansion/smelter/blob/master/gpu-video/LICENSE
[docs-badge]: https://img.shields.io/docsrs/gpu-video
[docs-url]: https://docs.rs/gpu-video/latest/gpu_video/

## Overview

The goal of this library is to provide easy access to hardware video coding. You can use it to decode and encode a video frame to/from `Vec<u8>` with pixel data, or [`wgpu::Texture`]. Currently, we support the following codecs:

|            | Decode | Encode |
|:----------:|:------:|:------:|
| H.264/AVC  | ✅      | ✅      |
| H.265/HEVC | ❌      | ✅      |
| AV1        | ❌      | 🚧     |

- ✅ - should work, file issues if there are problems
- 🚧 - working on this currently
- ❌ - not supported yet, but support is planned

An advantage of using this library with wgpu is that decoded video frames never leave the GPU memory. There's no copying the frames to RAM and back to the GPU, so it should be quite fast if you want to use them for rendering.

This library was developed as a part of [smelter, a tool for video composition](https://smelter.dev/).

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/software-mansion/smelter/f70f6087d53ec046824c0c41dc8a64a19bd943cf/tools/assets/smelter-logo-transparent.svg">
  <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/software-mansion/smelter/f70f6087d53ec046824c0c41dc8a64a19bd943cf/tools/assets/smelter-logo-background.svg">
  <img height="60" alt="Smelter" src="https://raw.githubusercontent.com/software-mansion/smelter/f70f6087d53ec046824c0c41dc8a64a19bd943cf/tools/assets/smelter-logo-background.svg">
</picture>

## Code samples

### Decode video frame to [`wgpu::Texture`]

```rust
fn decode_video(
    window: &winit::window::Window,
    mut encoded_video_reader: impl std::io::Read,
) {
    let instance = gpu_video::VulkanInstance::new().unwrap();
    let surface = instance.wgpu_instance().create_surface(window).unwrap();
    let adapter = instance.create_adapter(&gpu_video::parameters::VulkanAdapterDescriptor {
        compatible_surface: Some(&surface),
        ..Default::default()
    }).unwrap();
    let device = adapter
        .create_device(&gpu_video::parameters::VulkanDeviceDescriptor::default())
        .unwrap();

    let mut decoder = device
        .create_wgpu_textures_decoder_h264(
            gpu_video::parameters::DecoderParameters::default()
        ).unwrap();

    let mut buffer = vec![0; 4096];

    while let Ok(n) = encoded_video_reader.read(&mut buffer) {
        if n == 0 {
            return;
        }

        let decoded_frames = decoder.decode(gpu_video::EncodedInputChunk {
            data: &buffer[..n],
            pts: None
        }).unwrap();

        for frame in decoded_frames {
            // Each frame contains a wgpu::Texture you can sample for drawing.
            // device.wgpu_device() will give you a wgpu::Device and device.wgpu_queue()
            // a wgpu::Queue. You can use these for interacting with the frames.
        }
    }
}
```

### Encode video frame from [`wgpu::Texture`]

```rust
fn encode_video(
    window: &winit::window::Window,
    frame_receiver: std::sync::mpsc::Receiver<wgpu::Texture>,
) {
    use std::num::NonZeroU32;

    let instance = gpu_video::VulkanInstance::new().unwrap();
    let surface = instance.wgpu_instance().create_surface(window).unwrap();
    let adapter = instance
        .create_adapter(&gpu_video::parameters::VulkanAdapterDescriptor {
            compatible_surface: Some(&surface),
            ..Default::default()
        })
        .unwrap();
    let device = adapter
        .create_device(&gpu_video::parameters::VulkanDeviceDescriptor::default())
        .unwrap();

    let mut encoder = device
        .create_wgpu_textures_encoder_h264(
            gpu_video::parameters::EncoderParametersH264 {
                output_parameters: device
                    .encoder_output_parameters_h264_high_quality(
                        gpu_video::parameters::RateControl::VariableBitrate {
                            average_bitrate: 500_000,
                            max_bitrate: 2_000_000,
                            virtual_buffer_size: std::time::Duration::from_secs(2),
                        },
                    )
                    .unwrap(),
                input_parameters: gpu_video::parameters::VideoParameters {
                    width: NonZeroU32::new(1920).unwrap(),
                    height: NonZeroU32::new(1080).unwrap(),
                    target_framerate: 30.into(),
                },
            }
        )
        .unwrap();

    for frame in frame_receiver.iter() {
        // Encodes NV12 texture and returns encoded frame bytes
        let encoded_frame = encoder
            .encode(
                gpu_video::InputFrame {
                    data: frame,
                    pts: None,
                },
                false,
            )
            .unwrap();
    }
}
```

Be sure to check out our examples, especially the `player` example, which is a simple video player built using this library and wgpu. Because the player is very simple, you need to extract the raw h264 data from a container before usage. Here's an example on how to extract the h264 bytestream out of an mp4 file using ffmpeg:

```sh
ffmpeg -i input.mp4 -c:v copy -bsf:v h264_mp4toannexb -an output.h264
```

Then you can run the example with:

```sh
git clone https://github.com/software-mansion/smelter.git
cd smelter/gpu-video
cargo run --example player -- output.h264 FRAMERATE
```

## Compatibility

On Linux, the library should work on NVIDIA and AMD GPUs out of the box with recent Mesa drivers. For AMD GPUs with a bit older Mesa drivers, you may need to set the `RADV_PERFTEST=video_decode,video_encode` environment variable:

```sh
RADV_PERFTEST=video_decode,video_encode cargo run
```

It should work on Windows with recent drivers out of the box. Be sure to submit an issue if it doesn't.

[wgpu]: https://wgpu.rs/
[`wgpu::Texture`]: https://docs.rs/wgpu/latest/wgpu/struct.Texture.html

## gpu-video is created by Software Mansion

<a href="https://swmansion.com"><img width="150" height="80" alt="Software Mansion" src="https://github.com/user-attachments/assets/cacd6185-78b0-4e76-8767-016d6389bb2b" /></a>

Since 2012 [Software Mansion](https://swmansion.com) is a software agency with experience in building web and mobile apps as well as complex multimedia solutions. We are Core React Native Contributors and experts in live streaming and broadcasting technologies. We can help you build your next dream product – [Hire us](https://swmansion.com/contact/projects?utm_source=smelter-gpu-video&utm_medium=readme).
