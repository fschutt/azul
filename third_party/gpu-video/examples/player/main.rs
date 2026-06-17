#[cfg(vulkan)]
mod player;

#[cfg(vulkan)]
fn main() {
    player::run()
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
