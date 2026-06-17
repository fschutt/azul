#[cfg(vulkan)]
use gpu_video::{
    WgpuRgbaToNv12Converter,
    parameters::{EncoderParametersH264, EncoderParametersH265},
};

#[cfg(vulkan)]
fn main() {
    use gpu_video::{
        InputFrame, VulkanInstance,
        parameters::{
            RateControl, VideoParameters, VulkanAdapterDescriptor, VulkanDeviceDescriptor,
        },
    };
    use std::{io::Write, num::NonZeroU32};

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to initialize tracing");

    let args = std::env::args().collect::<Vec<_>>();

    if args.len() != 4 {
        println!("usage: {} WIDTH HEIGHT FRAME_COUNT", args[0]);
        return;
    }

    let width = args[1].parse::<NonZeroU32>().expect("parse width");
    let height = args[2].parse::<NonZeroU32>().expect("parse height");
    let frame_count = args[3].parse::<u32>().expect("parse frame count");

    let vulkan_instance = VulkanInstance::new().unwrap();
    let vulkan_adapter = vulkan_instance
        .create_adapter(&VulkanAdapterDescriptor::default())
        .unwrap();
    let vulkan_device = vulkan_adapter
        .create_device(&VulkanDeviceDescriptor {
            wgpu_features: wgpu::Features::IMMEDIATES,
            wgpu_limits: wgpu::Limits {
                max_immediate_size: 4,
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();

    let wgpu_state = WgpuState::new(
        vulkan_device.wgpu_device(),
        vulkan_device.wgpu_queue(),
        width,
        height,
    );

    let mut encoder_h264 = vulkan_device
        .create_wgpu_textures_encoder_h264(EncoderParametersH264 {
            input_parameters: VideoParameters {
                width,
                height,
                target_framerate: 30.into(),
            },
            output_parameters: vulkan_device
                .encoder_output_parameters_h264_high_quality(RateControl::VariableBitrate {
                    average_bitrate: 500_000,
                    max_bitrate: 2_000_000,
                    virtual_buffer_size: std::time::Duration::from_secs(2),
                })
                .unwrap(),
        })
        .unwrap();

    let mut encoder_h265 = vulkan_device
        .create_wgpu_textures_encoder_h265(EncoderParametersH265 {
            input_parameters: VideoParameters {
                width,
                height,
                target_framerate: 30.into(),
            },
            output_parameters: vulkan_device
                .encoder_output_parameters_h265_high_quality(RateControl::VariableBitrate {
                    average_bitrate: 500_000,
                    max_bitrate: 2_000_000,
                    virtual_buffer_size: std::time::Duration::from_secs(2),
                })
                .unwrap(),
        })
        .unwrap();

    let mut output_file_h264 = std::fs::File::create("output.h264").unwrap();
    let mut output_file_h265 = std::fs::File::create("output.h265").unwrap();

    for i in 0..frame_count {
        let time = 1.0 / 30.0 * i as f32;
        wgpu_state.render(time);

        let h264 = encoder_h264
            .encode(
                InputFrame {
                    data: wgpu_state.nv12_texture.clone(),
                    pts: None,
                },
                false,
            )
            .unwrap();
        output_file_h264.write_all(&h264.data).unwrap();

        let h265 = encoder_h265
            .encode(
                InputFrame {
                    data: wgpu_state.nv12_texture.clone(),
                    pts: None,
                },
                false,
            )
            .unwrap();
        output_file_h265.write_all(&h265.data).unwrap();
    }
}

#[cfg(vulkan)]
struct WgpuState {
    pipeline: wgpu::RenderPipeline,
    rgba_view: wgpu::TextureView,
    rgba_bg: wgpu::BindGroup,
    nv12_texture: wgpu::Texture,
    y_plane_view: wgpu::TextureView,
    uv_plane_view: wgpu::TextureView,
    rgba_to_nv12_converter: WgpuRgbaToNv12Converter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

#[cfg(vulkan)]
impl WgpuState {
    fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        width: std::num::NonZeroU32,
        height: std::num::NonZeroU32,
    ) -> WgpuState {
        use gpu_video::parameters::{ColorRange, ColorSpace, WgpuConverterParameters};

        let shader = wgpu::include_wgsl!("encode_wgpu.wgsl");
        let shader = device.create_shader_module(shader);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wgpu pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 4,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("wgpu pipeline"),
            layout: Some(&pipeline_layout),
            cache: None,
            vertex: wgpu::VertexState {
                module: &shader,
                buffers: &[],
                entry_point: None,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: None,
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    blend: None,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                front_face: wgpu::FrontFace::Ccw,
                conservative: false,
                unclipped_depth: false,
                strip_index_format: None,
            },
            multiview_mask: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            depth_stencil: None,
        });

        let rgba_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wgpu render target"),
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            dimension: wgpu::TextureDimension::D2,
            sample_count: 1,
            view_formats: &[],
            mip_level_count: 1,
            size: wgpu::Extent3d {
                width: width.get(),
                height: height.get(),
                depth_or_array_layers: 1,
            },
        });
        let rgba_view = rgba_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("wgpu render target view"),
            ..Default::default()
        });

        let nv12_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("encoder input"),
            format: wgpu::TextureFormat::NV12,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
            dimension: wgpu::TextureDimension::D2,
            sample_count: 1,
            view_formats: &[],
            mip_level_count: 1,
            size: wgpu::Extent3d {
                width: width.get(),
                height: height.get(),
                depth_or_array_layers: 1,
            },
        });
        let y_plane_view = nv12_texture.create_view(&wgpu::TextureViewDescriptor {
            aspect: wgpu::TextureAspect::Plane0,
            ..Default::default()
        });
        let uv_plane_view = nv12_texture.create_view(&wgpu::TextureViewDescriptor {
            aspect: wgpu::TextureAspect::Plane1,
            ..Default::default()
        });

        let rgba_to_nv12_converter = WgpuRgbaToNv12Converter::new(
            &device,
            WgpuConverterParameters {
                color_space: ColorSpace::BT709,
                color_range: ColorRange::Limited,
            },
        )
        .unwrap();
        let rgba_bg = rgba_to_nv12_converter.create_input_bind_group(&rgba_texture);

        WgpuState {
            pipeline,
            rgba_view,
            rgba_bg,
            nv12_texture,
            y_plane_view,
            uv_plane_view,
            rgba_to_nv12_converter,
            device,
            queue,
        }
    }

    fn render(&self, time: f32) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wgpu encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("wgpu render pass"),
                timestamp_writes: None,
                occlusion_query_set: None,
                depth_stencil_attachment: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.rgba_view,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    resolve_target: None,
                    depth_slice: None,
                })],
                multiview_mask: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_immediates(0, &time.to_ne_bytes());
            render_pass.draw(0..3, 0..1);
        }
        self.rgba_to_nv12_converter.convert(
            &mut encoder,
            &self.rgba_bg,
            &self.y_plane_view,
            &self.uv_plane_view,
        );

        let buffer = encoder.finish();

        self.queue.submit([buffer]);
    }
}

#[cfg(not(vulkan))]
fn main() {
    println!(
        "This crate doesn't work on your operating system, because it does not support vulkan"
    );
}
