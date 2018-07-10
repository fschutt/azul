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
    pub fn with_label<S>(text: S)
    -> Self where S: Into<String>
    {
        Self {
            content: ButtonContent::Text(text.into()),
        }
    }

    pub fn with_image(image: ImageId)
    -> Self
    {
        Self {
            content: ButtonContent::Image(image),
        }
    }

    pub fn dom<T>(self)
    -> Dom<T> where T: Layout
    {
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

#[derive(Debug, Clone, PartialEq)]
pub struct Svg {
    /// Currently active layers
    pub layers: Vec<SvgLayerId>,
    /// Pan (horizontal, vertical) in pixels
    pub pan: (f32, f32),
    /// 1.0 = default zoom
    pub zoom: f32,
    /// Whether an FXAA shader should be applied to the resulting OpenGL texture
    pub enable_fxaa: bool,
}

impl Default for Svg {
    fn default() -> Self {
        Self {
            layers: Vec::new(),
            pan: (0.0, 0.0),
            zoom: 1.0,
            enable_fxaa: false,
        }
    }
}

use glium::{Texture2d, draw_parameters::DrawParameters,
            index::PrimitiveType, IndexBuffer, Surface};
use std::sync::Mutex;
use svg::{SvgVert, SvgWorldPixel, SvgShader};
use webrender::api::{ColorU, ColorF};
use euclid::{TypedRect, TypedSize2D, TypedPoint2D};

impl Svg {

    #[inline]
    pub fn with_layers(layers: Vec<SvgLayerId>)
    -> Self
    {
        Self { layers: layers, .. Default::default() }
    }

    #[inline]
    pub fn with_pan(mut self, horz: f32, vert: f32)
    -> Self
    {
        self.pan = (horz, vert);
        self
    }

    #[inline]
    pub fn with_zoom(mut self, zoom: f32)
    -> Self
    {
        self.zoom = zoom;
        self
    }

    #[inline]
    pub fn with_fxaa(mut self, enable_fxaa: bool)
    -> Self
    {
        self.enable_fxaa = enable_fxaa;
        self
    }

    /// Renders the SVG to an OpenGL texture and creates the DOM
    pub fn dom<T>(&self, window: &ReadOnlyWindow, svg_cache: &SvgCache<T>)
    -> Dom<T> where T: Layout
    {
        const DEFAULT_COLOR: ColorU = ColorU { r: 0, b: 0, g: 0, a: 255 };

        let tex = window.create_texture(800, 600);
        tex.as_surface().clear_color(1.0, 1.0, 1.0, 1.0);

        let draw_options = DrawParameters {
            primitive_restart_index: true,
            .. Default::default()
        };

        let z_index: f32 = 0.5;
        let bbox: TypedRect<f32, SvgWorldPixel> = TypedRect {
                origin: TypedPoint2D::new(0.0, 0.0),
                size: TypedSize2D::new(800.0, 600.0),
        };
        let shader = svg_cache.init_shader(window);

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
                        offset: (self.pan.0, self.pan.1)
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
                        offset: (self.pan.0, self.pan.1)
                    };

                    surface.draw(stroke_vertex_buffer, stroke_index_buffer, &shader.program, &uniforms, &draw_options).unwrap();
                }
            }
        }

        if self.enable_fxaa {
            // TODO: apply FXAA shader
        }

        Dom::new(NodeType::GlTexture(tex))
    }
}

// --- label

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Label {
    pub text: String,
}

impl Label {
    pub fn new<S>(text: S)
    -> Self where S: Into<String>
    {
        Self { text: text.into() }
    }

    pub fn dom<T>(self)
    -> Dom<T> where T: Layout
    {
        Dom::new(NodeType::Label(self.text))
    }
}

// -- checkbox (TODO)

/// State of a checkbox (disabled, checked, etc.)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum CheckboxState {
    /// `[■]`
    Active,
    /// `[✔]`
    Checked,
    /// Greyed out checkbox
    Disabled {
        /// Should the checkbox fire on a mouseover / mouseup, etc. event
        ///
        /// This can be useful for showing warnings / tooltips / help messages
        /// as to why this checkbox is disabled
        fire_on_click: bool,
    },
    /// `[ ]`
    Unchecked
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_widgets_file() {

}