pub mod svg;
pub mod button;
pub mod label;

// Re-export widgets
pub use self::svg::{
	Svg, SvgLayerId, SvgLayer, LayerType,
	SvgStyle, SvgLayerType, SvgWorldPixel, SvgLayerResource,
	SvgCache, VectorizedFont, VectorizedFontCache, VerticesIndicesBuffer,
    VertexBuffers, SvgVert, GlyphId,
    join_vertex_buffers, get_fill_vertices, get_stroke_vertices,
    scale_vertex_buffer, transform_vertex_buffer,
};
pub use self::button::{Button, ButtonContent};
pub use self::label::Label;