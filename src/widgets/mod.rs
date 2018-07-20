pub mod svg;
pub mod button;
pub mod label;

// Re-export widgets
pub use self::svg::{
	Svg, SvgLayerId, SvgLayer, LayerType,
	SvgStyle, SvgLayerType, SvgWorldPixel, SvgLayerResource,
	SvgCache, VectorizedFont, VectorizedFontCache, VerticesIndicesBuffer,
    join_vertex_buffers
};
pub use self::button::{Button, ButtonContent};
pub use self::label::Label;