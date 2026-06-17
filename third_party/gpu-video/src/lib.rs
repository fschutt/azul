#![doc = include_str!("../README.md")]

#[cfg(vulkan)]
mod adapter;
#[cfg(vulkan)]
pub(crate) mod codec;
#[cfg(vulkan)]
mod device;
#[cfg(vulkan)]
mod instance;
#[cfg(vulkan)]
mod vulkan_decoder;
#[cfg(vulkan)]
mod vulkan_encoder;
#[cfg(all(vulkan, feature = "transcoder"))]
mod vulkan_transcoder;
#[cfg(all(vulkan, feature = "wgpu"))]
pub(crate) mod wgpu_helpers;
#[cfg(vulkan)]
pub(crate) mod wrappers;

#[cfg(vulkan)]
mod vulkan_video;
#[cfg(vulkan)]
pub use vulkan_video::*;

#[cfg(feature = "expose-parsers")]
pub mod parser;
#[cfg(not(feature = "expose-parsers"))]
pub(crate) mod parser;

// If vulkan is unsupported and parsers are not exposed
#[cfg(not(any(vulkan, feature = "expose-parsers")))]
compile_error!("gpu-video can be only compiled on platforms supported by vulkan.");
