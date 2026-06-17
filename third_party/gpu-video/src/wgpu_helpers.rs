mod nv12_to_rgba;
mod rgba_to_nv12;

pub use nv12_to_rgba::*;
pub use rgba_to_nv12::*;

use crate::device::{ColorRange, ColorSpace};

#[derive(Debug, thiserror::Error)]
pub enum WgpuConverterInitError {
    // TODO: Remove once we add more converters
    #[error("Only limited range BT709 is supported")]
    OnlyLimitedBT709Supported,

    #[error(
        "Provided frame does not match the converter's parameters. Expected {expected:?}, actual {actual:?}"
    )]
    IncompatibleFrame {
        expected: (ColorSpace, ColorRange),
        actual: (ColorSpace, ColorRange),
    },
}

/// Parameters for NV12 <-> RGBA texture conversion.
///
/// Used by [`WgpuNv12ToRgbaConverter`] and [`WgpuRgbaToNv12Converter`] to describe
/// the color properties of the NV12 textures.
#[derive(Debug, Clone, Copy)]
pub struct WgpuConverterParameters {
    /// The color space of the NV12 data.
    pub color_space: ColorSpace,

    /// Whether the NV12 data uses full or limited sample range.
    pub color_range: ColorRange,
}

struct WgpuSampler {
    bgl: wgpu::BindGroupLayout,
    bg: wgpu::BindGroup,
}

impl WgpuSampler {
    fn new(device: &wgpu::Device) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            }],
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(&sampler),
            }],
        });

        Self { bgl, bg }
    }
}
