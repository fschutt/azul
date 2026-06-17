use crate::{
    WgpuConverterInitError,
    device::{ColorRange, ColorSpace},
    parameters::WgpuConverterParameters,
    wgpu_helpers::WgpuSampler,
};

/// Helper that lets you convert RGBA [`wgpu::Texture`] into NV12 [`wgpu::Texture`].
/// Use [`WgpuRgbaToNv12Converter::create_input_bind_group`] to create [`wgpu::BindGroup`] which represents
/// RGBA bind group acceptable by the converter.
pub struct WgpuRgbaToNv12Converter {
    y_plane_renderer: PlaneRenderer,
    uv_plane_renderer: PlaneRenderer,

    rgba_view_bgl: wgpu::BindGroupLayout,
    sampler: WgpuSampler,

    device: wgpu::Device,
}

impl WgpuRgbaToNv12Converter {
    pub fn new(
        device: &wgpu::Device,
        params: WgpuConverterParameters,
    ) -> Result<Self, WgpuConverterInitError> {
        match (params.color_space, params.color_range) {
            (ColorSpace::BT709, ColorRange::Limited) => {}
            _ => return Err(WgpuConverterInitError::OnlyLimitedBT709Supported),
        }

        let rgba_view_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

        let sampler = WgpuSampler::new(device);
        let shader_module =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/rgba_to_nv12.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu-video rgba to nv12 converter pipeline layout"),
            bind_group_layouts: &[Some(&rgba_view_bgl), Some(&sampler.bgl)],
            immediate_size: 0,
        });

        let y_plane_renderer = PlaneRenderer::new(
            device,
            &pipeline_layout,
            &shader_module,
            wgpu::TextureAspect::Plane0,
        );
        let uv_plane_renderer = PlaneRenderer::new(
            device,
            &pipeline_layout,
            &shader_module,
            wgpu::TextureAspect::Plane1,
        );

        Ok(Self {
            y_plane_renderer,
            uv_plane_renderer,
            rgba_view_bgl,
            sampler,
            device: device.clone(),
        })
    }

    /// Creates [`wgpu::BindGroup`] for RGBA [`wgpu::Texture`].
    /// The texture's usage must contain [`wgpu::TextureUsages::TEXTURE_BINDING`].
    pub fn create_input_bind_group(&self, rgba_texture: &wgpu::Texture) -> wgpu::BindGroup {
        let rgba_view = rgba_texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.rgba_view_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&rgba_view),
            }],
        })
    }

    /// Converts RGBA texture into NV12 texture.
    /// NV12 texture's usage must contain [`wgpu::TextureUsages::RENDER_ATTACHMENT`].
    pub fn convert(
        &self,
        command_encoder: &mut wgpu::CommandEncoder,
        src_rgba_bind_group: &wgpu::BindGroup,
        dst_y_plane_view: &wgpu::TextureView,
        dst_uv_plane_view: &wgpu::TextureView,
    ) {
        self.y_plane_renderer.draw(
            command_encoder,
            src_rgba_bind_group,
            &self.sampler.bg,
            dst_y_plane_view,
        );
        self.uv_plane_renderer.draw(
            command_encoder,
            src_rgba_bind_group,
            &self.sampler.bg,
            dst_uv_plane_view,
        );
    }
}

struct PlaneRenderer {
    pipeline: wgpu::RenderPipeline,
}

impl PlaneRenderer {
    fn new(
        device: &wgpu::Device,
        pipeline_layout: &wgpu::PipelineLayout,
        shader_module: &wgpu::ShaderModule,
        plane: wgpu::TextureAspect,
    ) -> Self {
        let (format, fragment_entry_point) = match plane {
            wgpu::TextureAspect::Plane0 => (wgpu::TextureFormat::R8Unorm, "fs_main_y"),
            wgpu::TextureAspect::Plane1 => (wgpu::TextureFormat::Rg8Unorm, "fs_main_uv"),
            aspect => unreachable!("Not a NV12 plane: {aspect:?}"),
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gpu-video nv12 plane renderer"),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader_module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader_module,
                entry_point: Some(fragment_entry_point),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        Self { pipeline }
    }

    fn draw(
        &self,
        command_encoder: &mut wgpu::CommandEncoder,
        texture_bg: &wgpu::BindGroup,
        sampler_bg: &wgpu::BindGroup,
        plane_view: &wgpu::TextureView,
    ) {
        let mut render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: plane_view,
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

        render_pass.set_bind_group(0, texture_bg, &[]);
        render_pass.set_bind_group(1, sampler_bg, &[]);
        render_pass.set_pipeline(&self.pipeline);
        render_pass.draw(0..3, 0..1);
    }
}
