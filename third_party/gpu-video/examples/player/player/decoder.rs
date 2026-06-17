use std::{
    io::Read,
    sync::{Arc, mpsc::SyncSender},
    time::Duration,
};

use bytes::BytesMut;
use gpu_video::{EncodedInputChunk, OutputFrame, VulkanDevice, parameters::DecoderParameters};

use super::FrameWithPts;

pub fn run_decoder(
    tx: SyncSender<super::FrameWithPts>,
    framerate: u64,
    vulkan_device: Arc<VulkanDevice>,
    mut bytestream_reader: impl Read,
) {
    let mut decoder = vulkan_device
        .create_wgpu_textures_decoder_h264(DecoderParameters::default())
        .unwrap();
    let frame_interval = 1.0 / (framerate as f64);
    let mut frame_number = 0u64;
    let mut buffer = BytesMut::zeroed(4096);

    let send_frame = move |frame: OutputFrame<wgpu::Texture>, frame_number: &mut u64| {
        let result = FrameWithPts {
            frame: frame.data,
            pts: Duration::from_secs_f64(*frame_number as f64 * frame_interval),
        };

        *frame_number += 1;

        tx.send(result)
    };

    while let Ok(n) = bytestream_reader.read(&mut buffer) {
        if n == 0 {
            return;
        }

        let frame = EncodedInputChunk {
            data: &buffer[..n],
            pts: None,
        };

        let decoded = decoder.decode(frame).unwrap();

        for f in decoded {
            if send_frame(f, &mut frame_number).is_err() {
                return;
            }
        }
    }

    for f in decoder.flush().unwrap() {
        if send_frame(f, &mut frame_number).is_err() {
            return;
        }
    }
}
