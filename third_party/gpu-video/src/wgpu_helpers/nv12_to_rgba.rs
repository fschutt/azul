use crate::{
    OutputFrame, WgpuConverterInitError,
    device::{ColorRange, ColorSpace},
    parameters::WgpuConverterParameters,
    wgpu_helpers::WgpuSampler,
};

/// Helper that lets you convert [`OutputFrame<wgpu::Texture>`] into RGBA [`wgpu::Texture`].
/// Use [`WgpuNv12ToRgbaConverter::create_input_bind_group`] to create [`wgpu::BindGroup`] which represents
/// NV12 bind group acceptable by the converter.
pub struct WgpuNv12ToRgbaConverter {
    pipeline: wgpu::RenderPipeline,
    params: WgpuConverterParameters,

    nv12_planes_bgl: wgpu::BindGroupLayout,
    sampler: WgpuSampler,

    device: wgpu::Device,
}

impl WgpuNv12ToRgbaConverter {
    pub fn new(
        device: &wgpu::Device,
        params: WgpuConverterParameters,
    ) -> Result<Self, WgpuConverterInitError> {
        match (params.color_space, params.color_range) {
            (ColorSpace::BT709, ColorRange::Limited) => {}
            _ => return Err(WgpuConverterInitError::OnlyLimitedBT709Supported),
        }

        let nv12_planes_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let sampler = WgpuSampler::new(device);
        let shader_module =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/nv12_to_rgba.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu-video nv12 to rgba converter pipeline layout"),
            bind_group_layouts: &[Some(&nv12_planes_bgl), Some(&sampler.bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gpu-video nv12 to rgba converter pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        Ok(Self {
            pipeline,
            params,
            nv12_planes_bgl,
            sampler,
            device: device.clone(),
        })
    }

    /// Creates [`wgpu::BindGroup`] for [`OutputFrame<wgpu::Texture>`].
    pub fn create_input_bind_group(
        &self,
        decoded_frame: &OutputFrame<wgpu::Texture>,
    ) -> Result<wgpu::BindGroup, WgpuConverterInitError> {
        let OutputFrame { data, metadata } = decoded_frame;
        if (metadata.color_space != ColorSpace::Unspecified
            && metadata.color_space != self.params.color_space)
            || metadata.color_range != self.params.color_range
        {
            return Err(WgpuConverterInitError::IncompatibleFrame {
                expected: (self.params.color_space, self.params.color_range),
                actual: (metadata.color_space, metadata.color_range),
            });
        }

        let y_plane_view = data.create_view(&wgpu::TextureViewDescriptor {
            label: Some("gpu-video nv12 to rgba converter y plane view"),
            format: Some(wgpu::TextureFormat::R8Unorm),
            aspect: wgpu::TextureAspect::Plane0,
            ..Default::default()
        });
        let uv_plane_view = data.create_view(&wgpu::TextureViewDescriptor {
            label: Some("gpu-video nv12 to rgba converter uv plane view"),
            format: Some(wgpu::TextureFormat::Rg8Unorm),
            aspect: wgpu::TextureAspect::Plane1,
            ..Default::default()
        });

        Ok(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.nv12_planes_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&y_plane_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&uv_plane_view),
                },
            ],
        }))
    }

    /// Converts NV12 texture into RGBA texture.
    /// RGBA texture's usage must contain [`wgpu::TextureUsages::RENDER_ATTACHMENT`].
    pub fn convert(
        &self,
        command_encoder: &mut wgpu::CommandEncoder,
        src_nv12_bind_group: &wgpu::BindGroup,
        dst_rgba_view: &wgpu::TextureView,
    ) {
        let mut render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst_rgba_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::DontCare(unsafe { wgpu::LoadOpDontCare::enabled() }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        render_pass.set_bind_group(0, src_nv12_bind_group, &[]);
        render_pass.set_bind_group(1, &self.sampler.bg, &[]);
        render_pass.set_pipeline(&self.pipeline);
        render_pass.draw(0..3, 0..1);
    }
}
