use {
    svg::{SvgCache, SvgLayerId},
    window::ReadOnlyWindow,
    traits::Layout,
    dom::{Dom, NodeType},
    images::ImageId,
};

// --- button

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Button {
    pub content: ButtonContent,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ButtonContent {
    Image(ImageId),
    // Buttons should only contain short amounts of text
    Text(String),
}

impl Button {
    pub fn with_label<S: Into<String>>(text: S) -> Self {
        Self {
            content: ButtonContent::Text(text.into()),
        }
    }

    pub fn with_image(image: ImageId) -> Self {
        Self {
            content: ButtonContent::Image(image),
        }
    }

    pub fn dom<T: Layout>(self) -> Dom<T> {
        use self::ButtonContent::*;
        let mut button_root = Dom::new(NodeType::Div).with_class("__azul-native-button");
        button_root.add_child(match self.content {
            Text(s) => Dom::new(NodeType::Label(s)),
            Image(i) => Dom::new(NodeType::Image(i)),
        });
        button_root
    }
}

// --- svg

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Svg {
    pub layers: Vec<SvgLayerId>,
    pub enable_fxaa: bool,
}

use glium::{Texture2d, draw_parameters::DrawParameters,
            index::PrimitiveType, IndexBuffer, Surface};
use std::sync::Mutex;
use svg::{SvgVert, SvgWorldPixel, SvgShader};
use webrender::api::{ColorU, ColorF};
use euclid::{TypedRect, TypedSize2D, TypedPoint2D};

impl Svg {

    // todo: remove this later
    #[inline]
    pub fn empty() -> Self {
        Self { layers: Vec::new(), enable_fxaa: true }
    }

    #[inline]
    pub fn with_layers(layers: &Vec<SvgLayerId>) -> Self {
        Self { layers: layers.clone(), enable_fxaa: true }
    }

    #[inline]
    pub fn with_fxaa(mut self, enable_fxaa: bool) -> Self {
        self.enable_fxaa = enable_fxaa;
        self
    }

    pub fn dom<T: Layout>(&self, window: &ReadOnlyWindow, svg_cache: &SvgCache<T>) -> Dom<T> {

        const DEFAULT_COLOR: ColorU = ColorU { r: 0, b: 0, g: 0, a: 255 };

        window.make_current();

        let tex = window.create_texture(800, 600);
        tex.as_surface().clear_color(1.0, 1.0, 1.0, 1.0);

        let draw_options = DrawParameters {
            primitive_restart_index: true,
            .. Default::default()
        };

        let z_index: f32 = 0.5;
        let bbox = Svg::make_bbox((0.0, 0.0), (800.0, 600.0));
        let shader = svg_cache.init_shader(window);
        let offset = (400.0_f32, 200.0_f32);

        {
            let mut surface = tex.as_surface();

            for layer_id in &self.layers {

                use palette::Srgba;
                let style = svg_cache.get_style(layer_id);

                if let Some(color) = style.fill {
                    let color: ColorF = color.into();
                    let (vertex_buffer, index_buffer) = svg_cache.get_vertices_and_indices(window, layer_id);
                    let color = Srgba::new(color.r, color.g, color.b, color.a).into_linear();

                    let uniforms = uniform! {
                        bbox_origin: (bbox.origin.x, bbox.origin.y),
                        bbox_size: (bbox.size.width / 2.0, bbox.size.height / 2.0),
                        z_index: z_index,
                        color: (
                            color.color.red as f32,
                            color.color.green as f32,
                            color.color.blue as f32,
                            color.alpha as f32
                        ),
                        offset: (offset.0 as f32, offset.1 as f32)
                    };

                    surface.draw(vertex_buffer, index_buffer, &shader.program, &uniforms, &draw_options).unwrap();
                }

                if let Some((stroke_color, _)) = style.stroke {
                    let stroke_color: ColorF = stroke_color.into();
                    let (stroke_vertex_buffer, stroke_index_buffer) = svg_cache.get_stroke_vertices_and_indices(window, layer_id);
                    let stroke_color = Srgba::new(stroke_color.r, stroke_color.g, stroke_color.b, stroke_color.a).into_linear();

                    let uniforms = uniform! {
                        bbox_origin: (bbox.origin.x, bbox.origin.y),
                        bbox_size: (bbox.size.width / 2.0, bbox.size.height / 2.0),
                        z_index: z_index,
                        color: (
                            stroke_color.color.red as f32,
                            stroke_color.color.green as f32,
                            stroke_color.color.blue as f32,
                            stroke_color.alpha as f32
                        ),
                        offset: (offset.0 as f32, offset.1 as f32)
                    };

                    surface.draw(stroke_vertex_buffer, stroke_index_buffer, &shader.program, &uniforms, &draw_options).unwrap();
                }
            }
        }

        if self.enable_fxaa {
            // TODO: apply FXAA shader
        }

        window.unbind_framebuffer();

        Dom::new(NodeType::GlTexture(tex))
    }

    pub fn make_bbox((origin_x, origin_y): (f32, f32), (size_x, size_y): (f32, f32)) -> TypedRect<f32, SvgWorldPixel> {
        TypedRect::<f32, SvgWorldPixel>::new(TypedPoint2D::new(origin_x, origin_y), TypedSize2D::new(size_x, size_y))
    }
}

// --- label

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Label {
    pub text: String,
}

impl Label {
    pub fn new<S: Into<String>>(text: S) -> Self {
        Self { text: text.into() }
    }

    pub fn dom<T: Layout>(self) -> Dom<T> {
        Dom::new(NodeType::Label(self.text))
    }
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_widgets_file() {

}