#[cfg(vulkan)]
fn main() {
    use gpu_video::{
        VulkanInstance,
        parameters::{VulkanAdapterDescriptor, VulkanDeviceDescriptor},
    };

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to initialize tracing");

    let vulkan_instance = VulkanInstance::new().unwrap();
    let vulkan_adapter = vulkan_instance
        .create_adapter(&VulkanAdapterDescriptor::default())
        .unwrap();
    let vulkan_device = vulkan_adapter
        .create_device(&VulkanDeviceDescriptor::default())
        .unwrap();

    std::hint::black_box(vulkan_device);
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
