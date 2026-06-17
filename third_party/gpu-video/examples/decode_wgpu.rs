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
        .create_device(&VulkanDeviceDescriptor {
            wgpu_limits: wgpu::Limits {
                max_binding_array_elements_per_shader_stage: 128,
                max_immediate_size: 128,
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();

    let mut decoder = vulkan_device
        .create_wgpu_textures_decoder_h264(DecoderParameters::default())
        .unwrap();

    let mut output_file = std::fs::File::create("output.nv12").unwrap();

    let device = vulkan_device.wgpu_device();
    let queue = &vulkan_device.wgpu_queue();

    for chunk in h264_bytestream.chunks(256) {
        let chunk = EncodedInputChunk {
            data: chunk,
            pts: None,
        };

        let frames = decoder.decode(chunk).unwrap();

        for OutputFrame { data, .. } in frames {
            let decoded_frame = download_wgpu_texture(&device, queue, data);
            output_file.write_all(&decoded_frame).unwrap();
        }
    }

    let remaining_frames = decoder.flush().unwrap();
    for OutputFrame { data, .. } in remaining_frames {
        let decoded_frame = download_wgpu_texture(&device, queue, data);
        output_file.write_all(&decoded_frame).unwrap();
    }
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}

#[cfg(vulkan)]
fn download_wgpu_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frame: wgpu::Texture,
) -> Vec<u8> {
    use std::io::Write;

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let y_plane_bytes_per_row = (frame.width() as u64).div_ceil(256) * 256;
    let y_plane_size = y_plane_bytes_per_row * frame.height() as u64;

    let uv_plane_bytes_per_row = y_plane_bytes_per_row;
    let uv_plane_size = uv_plane_bytes_per_row * frame.height() as u64 / 2;

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: y_plane_size + uv_plane_size,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            aspect: wgpu::TextureAspect::Plane0,
            origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
            texture: &frame,
            mip_level: 0,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(y_plane_bytes_per_row as u32),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: frame.width(),
            height: frame.height(),
            depth_or_array_layers: 1,
        },
    );

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            aspect: wgpu::TextureAspect::Plane1,
            origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
            texture: &frame,
            mip_level: 0,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: y_plane_size,
                bytes_per_row: Some(uv_plane_bytes_per_row as u32),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: frame.width() / 2,
            height: frame.height() / 2,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(Some(encoder.finish()));

    let (y_tx, y_rx) = std::sync::mpsc::channel();
    let (uv_tx, uv_rx) = std::sync::mpsc::channel();
    let width = frame.width() as usize;

    wgpu::util::DownloadBuffer::read_buffer(
        device,
        queue,
        &buffer.slice(..y_plane_size),
        move |buf| {
            let buf = buf.unwrap();
            let mut result = Vec::new();

            for chunk in buf
                .chunks(y_plane_bytes_per_row as usize)
                .map(|chunk| &chunk[..width])
            {
                result.write_all(chunk).unwrap();
            }

            y_tx.send(result).unwrap();
        },
    );

    wgpu::util::DownloadBuffer::read_buffer(
        device,
        queue,
        &buffer.slice(y_plane_size..),
        move |buf| {
            let buf = buf.unwrap();
            let mut result = Vec::new();

            for chunk in buf
                .chunks(uv_plane_bytes_per_row as usize)
                .map(|chunk| &chunk[..width])
            {
                result.write_all(chunk).unwrap();
            }

            uv_tx.send(result).unwrap();
        },
    );

    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

    let mut result = Vec::new();
    result.append(&mut y_rx.recv().unwrap());
    result.append(&mut uv_rx.recv().unwrap());

    result
}
