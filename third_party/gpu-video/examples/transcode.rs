#[cfg(vulkan)]
fn main() {
    use std::{
        fs::File,
        io::{Read, Write},
        num::NonZeroU32,
        time::Duration,
    };

    use gpu_video::{
        EncodedInputChunk, VulkanInstance,
        parameters::{
            AnyEncoderParameters, RateControl, ScalingAlgorithm, TranscoderOutputParameters,
            TranscoderParameters, VulkanAdapterDescriptor, VulkanDeviceDescriptor,
        },
    };

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to initialize tracing");

    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 4 || args.len() > 5 {
        print_usage_and_exit(&args[0]);
    }

    let input_file = &args[1];
    let Ok(output_width) = args[2].parse::<NonZeroU32>() else {
        print_usage_and_exit(&args[0]);
    };
    let Ok(output_height) = args[3].parse::<NonZeroU32>() else {
        print_usage_and_exit(&args[0]);
    };

    let scaling_algorithm = if args.len() == 5 {
        match args[4].as_str() {
            "nearest" => ScalingAlgorithm::NearestNeighbor,
            "bilinear" => ScalingAlgorithm::Bilinear,
            "lanczos3" => ScalingAlgorithm::Lanczos3,
            _ => print_usage_and_exit(&args[0]),
        }
    } else {
        ScalingAlgorithm::default()
    };

    let instance = VulkanInstance::new().unwrap();
    let adapter = instance
        .create_adapter(&VulkanAdapterDescriptor::default())
        .unwrap();
    let device = adapter
        .create_device(&VulkanDeviceDescriptor::default())
        .unwrap();

    let average_bitrate = 1_000_000;
    let max_bitrate = 1_200_000;

    let params_h264 = device
        .encoder_output_parameters_h264_high_quality(RateControl::VariableBitrate {
            average_bitrate,
            max_bitrate,
            virtual_buffer_size: Duration::from_secs(2),
        })
        .unwrap();

    let params_h265 = device
        .encoder_output_parameters_h265_high_quality(RateControl::VariableBitrate {
            average_bitrate,
            max_bitrate,
            virtual_buffer_size: Duration::from_secs(2),
        })
        .unwrap();

    let mut transcoder = device
        .create_transcoder(TranscoderParameters {
            input_framerate: 30.into(),
            output_parameters: vec![
                TranscoderOutputParameters {
                    output_width,
                    output_height,
                    encoder_parameters: AnyEncoderParameters::H264(params_h264),
                    scaling_algorithm,
                },
                TranscoderOutputParameters {
                    output_width,
                    output_height,
                    encoder_parameters: AnyEncoderParameters::H265(params_h265),
                    scaling_algorithm,
                },
            ],
        })
        .unwrap();

    let mut input_file = File::open(input_file).unwrap();
    let mut output_file_h264 = File::create("output.h264").unwrap();
    let mut output_file_h265 = File::create("output.h265").unwrap();

    let mut buffer = vec![0; 4096];
    while let Ok(n) = input_file.read(&mut buffer)
        && n > 0
    {
        let input = EncodedInputChunk {
            data: &buffer[..n],
            pts: None,
        };
        let output = transcoder.transcode(input).unwrap();

        for output in output {
            output_file_h264.write_all(&output[0].data).unwrap();
            output_file_h265.write_all(&output[1].data).unwrap();
        }
    }

    let flushed = transcoder.flush().unwrap();
    for output in flushed {
        output_file_h264.write_all(&output[0].data).unwrap();
        output_file_h265.write_all(&output[1].data).unwrap();
    }
}

#[cfg(vulkan)]
fn print_usage_and_exit(executable_name: &str) -> ! {
    eprintln!("usage: {executable_name} INPUT OUT_WIDTH OUT_HEIGHT [nearest|bilinear|lanczos3]");
    std::process::exit(1);
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
