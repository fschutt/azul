#[cfg(vulkan)]
fn main() {
    use std::io::Write;

    use gpu_video::{
        EncodedInputChunk, OutputFrame, VulkanInstance,
        parameters::{DecoderParameters, VulkanAdapterDescriptor, VulkanDeviceDescriptor},
    };

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to initialize tracing");

    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 2 {
        println!("usage: {} FILENAME", args[0]);
        return;
    }

    let h264_bytestream = std::fs::read(&args[1]).unwrap_or_else(|_| panic!("read {}", args[1]));

    let vulkan_instance = VulkanInstance::new().unwrap();
    let vulkan_adapter = vulkan_instance
        .create_adapter(&VulkanAdapterDescriptor::default())
        .unwrap();
    let vulkan_device = vulkan_adapter
        .create_device(&VulkanDeviceDescriptor::default())
        .unwrap();

    let mut decoder = vulkan_device
        .create_bytes_decoder_h264(DecoderParameters::default())
        .unwrap();

    let mut output_file = std::fs::File::create("output.nv12").unwrap();

    for chunk in h264_bytestream.chunks(256) {
        let data = EncodedInputChunk {
            data: chunk,
            pts: None,
        };

        let frames = decoder.decode(data).unwrap();

        for OutputFrame { data, .. } in frames {
            output_file.write_all(&data.frame).unwrap();
        }
    }

    let remaining_frames = decoder.flush().unwrap();
    for OutputFrame { data, .. } in remaining_frames {
        output_file.write_all(&data.frame).unwrap();
    }
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
