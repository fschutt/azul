#[cfg(vulkan)]
fn main() {
    use std::{
        io::{Read, Write},
        num::NonZeroU32,
    };

    use gpu_video::{
        InputFrame, RawFrameData, VulkanInstance,
        parameters::{
            EncoderParametersH264, EncoderParametersH265, RateControl, VideoParameters,
            VulkanAdapterDescriptor, VulkanDeviceDescriptor,
        },
    };

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to initialize tracing");

    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        println!("usage: {} FILENAME WIDTH HEIGHT", args[0]);
        return;
    }

    let width = args[2].parse::<NonZeroU32>().expect("parse video width");
    let height = args[3].parse::<NonZeroU32>().expect("parse video height");
    let mut nv12 =
        std::fs::File::open(&args[1]).unwrap_or_else(|e| panic!("open {}: {}", args[1], e));

    let vulkan_instance = VulkanInstance::new().unwrap();
    let vulkan_adapter = vulkan_instance
        .create_adapter(&VulkanAdapterDescriptor::default())
        .unwrap();
    let vulkan_device = vulkan_adapter
        .create_device(&VulkanDeviceDescriptor::default())
        .unwrap();

    let mut encoder_h264 = vulkan_device
        .create_bytes_encoder_h264(EncoderParametersH264 {
            input_parameters: VideoParameters {
                width,
                height,
                target_framerate: 24.into(),
            },
            output_parameters: vulkan_device
                .encoder_output_parameters_h264_high_quality(RateControl::VariableBitrate {
                    average_bitrate: 1_000_000,
                    max_bitrate: 2_000_000,
                    virtual_buffer_size: std::time::Duration::from_secs(2),
                })
                .unwrap(),
        })
        .expect("create encoder");

    let mut encoder_h265 = vulkan_device
        .create_bytes_encoder_h265(EncoderParametersH265 {
            input_parameters: VideoParameters {
                width,
                height,
                target_framerate: 24.into(),
            },
            output_parameters: vulkan_device
                .encoder_output_parameters_h265_high_quality(RateControl::VariableBitrate {
                    average_bitrate: 1_000_000,
                    max_bitrate: 2_000_000,
                    virtual_buffer_size: std::time::Duration::from_secs(2),
                })
                .unwrap(),
        })
        .expect("create encoder");

    let mut output_file_h264 = std::fs::File::create("output.h264").unwrap();
    let mut output_file_h265 = std::fs::File::create("output.h265").unwrap();

    let mut frame = InputFrame {
        data: RawFrameData {
            frame: vec![0; width.get() as usize * height.get() as usize * 3 / 2],
            width: width.get(),
            height: height.get(),
        },
        pts: None,
    };

    while let Ok(()) = nv12.read_exact(&mut frame.data.frame) {
        let h264 = encoder_h264.encode(&frame, false).expect("encode");
        output_file_h264.write_all(&h264.data).expect("write");
        let h265 = encoder_h265.encode(&frame, false).expect("encode");
        output_file_h265.write_all(&h265.data).expect("write");
    }
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
