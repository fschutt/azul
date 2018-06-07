#![allow(non_snake_case)]

use svg::SvgCache;
use svg::SvgLayerId;
use window::ReadOnlyWindow;
use traits::GetDom;
use traits::Layout;
use dom::{Dom, NodeType};
use images::ImageId;

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
}

impl GetDom for Button {
    fn dom<T: Layout>(self) -> Dom<T> {
        use self::ButtonContent::*;
        let mut button_root = Dom::new(NodeType::Div).with_class("__azul-native-button");
        button_root.add_child(match self.content {
            Image(i) => Dom::new(NodeType::Image(i)),
            Text(s) => Dom::new(NodeType::Label(s)),
        });
        button_root
    }
}

// --- svg

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Svg {
    pub layers: Vec<SvgLayerId>,
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct SvgVert {
    pub(crate) xy: (f32, f32),
}

implement_vertex!(SvgVert, xy);

#[derive(Debug, Copy, Clone)]
pub struct SvgWorldPixel;

use glium::{Texture2d, draw_parameters::DrawParameters,
            index::PrimitiveType, IndexBuffer, Surface};
use std::sync::Mutex;
use svg::SvgShader;
use webrender::api::ColorF;
use euclid::{TypedRect, TypedSize2D, TypedPoint2D};

impl Svg {

    // todo: remove this later
    pub fn empty() -> Self {
        Self { layers: Vec::new() }
    }

    pub fn with_layers(layers: &Vec<SvgLayerId>) -> Self {
        Self { layers: layers.clone() }
    }

    pub fn dom<T: Layout>(&self, window: &ReadOnlyWindow, svg_cache: &SvgCache<T>) -> Dom<T> {

        window.make_current();
        let tex = window.create_texture(800, 600);
        tex.as_surface().clear_color(1.0, 1.0, 1.0, 1.0);

        // TODO: cache the vertex buffers / index buffers
        let vertex_buffer = window.make_vertex_buffer(&[
            SvgVert { xy: (500.0, 400.0) },
            SvgVert { xy: (500.0, 0.0) },
            SvgVert { xy: (0.0, 300.0) },
        ]).unwrap();

        let index_buffer = window.make_index_buffer(PrimitiveType::TrianglesList, &[0_u32, 1, 2]).unwrap();

        let draw_options = DrawParameters {
            primitive_restart_index: true,
            .. Default::default()
        };

        let z_index: f32 = 0.5;
        let bbox= Svg::make_bbox((0.0, 0.0), (800.0, 600.0));
        let color = ColorF { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };

        let uniforms = uniform! {
            bbox_origin: (bbox.origin.x, bbox.origin.y),
            bbox_size: (bbox.size.width / 2.0, bbox.size.height / 2.0),
            z_index: z_index,
            color: (color.r, color.g, color.b, color.a),
        };

        tex.as_surface().draw(&vertex_buffer, &index_buffer, &svg_cache.init_shader(window).program, &uniforms, &draw_options).unwrap();

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
}

impl GetDom for Label {
    fn dom<T: Layout>(self) -> Dom<T> {
        Dom::new(NodeType::Label(self.text))
    }
}