use std::{path::PathBuf, sync::mpsc, time::Duration};

use clap::Parser;
use gpu_video::VulkanInstance;
use winit::{event_loop::EventLoop, window::WindowBuilder};

mod decoder;
mod renderer;

const FRAMES_BUFFER_LEN: usize = 3;

#[derive(Parser)]
#[command(version, about, long_about=None)]
struct Args {
    /// an .h264 file to play
    filename: PathBuf,

    /// framerate to play the video at
    framerate: u64,
}

struct FrameWithPts {
    frame: wgpu::Texture,
    /// Presentation timestamp
    pts: Duration,
}

pub fn run() {
    let args = Args::parse();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();

    let file = std::fs::File::open(&args.filename).expect("open file");

    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new().build(&event_loop).unwrap();

    let vulkan_instance = VulkanInstance::new().unwrap();

    let surface = vulkan_instance
        .wgpu_instance()
        .create_surface(&window)
        .unwrap();

    let vulkan_adapter = vulkan_instance
        .create_adapter(&gpu_video::parameters::VulkanAdapterDescriptor {
            compatible_surface: Some(&surface),
            ..Default::default()
        })
        .unwrap();
    let vulkan_device = vulkan_adapter
        .create_device(&gpu_video::parameters::VulkanDeviceDescriptor::default())
        .unwrap();

    let (tx, rx) = mpsc::sync_channel(FRAMES_BUFFER_LEN);
    let vulkan_device_clone = vulkan_device.clone();

    std::thread::spawn(move || {
        decoder::run_decoder(tx, args.framerate, vulkan_device_clone, file);
    });

    renderer::run_renderer(event_loop, &window, surface, &vulkan_device, rx);
}
