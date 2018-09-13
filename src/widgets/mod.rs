#[cfg(feature = "svg")]
pub mod svg;
pub mod button;
pub mod label;
pub mod text_input;
pub mod table_view;

// Re-export widgets
#[cfg(feature = "svg")]
pub use self::svg::{
	Svg, SvgLayerId, SvgLayer, LayerType,
	SvgStyle, SvgLayerType, SvgWorldPixel, SvgLayerResource,
	SvgCache, VectorizedFont, VectorizedFontCache, VerticesIndicesBuffer,
    SvgStrokeOptions, VertexBuffers, SvgVert, GlyphId,
    SvgCircle, SvgRect, BezierControlPoint, SampledBezierCurve,
    SvgText, SvgTextPlacement, SvgTextLayout, SvgBbox,
    BezierNormalVector, BezierCharacterRotation, SvgPosition,

    join_vertex_buffers, get_fill_vertices, get_stroke_vertices,
    scale_vertex_buffer, transform_vertex_buffer, rotate_vertex_buffer,
    quick_circle, quick_circles, quick_lines, cubic_interpolate_bezier,
    quadratic_interpolate_bezier,
};
pub use self::button::{Button, ButtonContent};
pub use self::label::Label;
pub use self::text_input::{TextInput, TextInputOutcome};
pub use self::table_view::{TableView, TableViewOutcome};

pub mod errors {
    #[cfg(all(feature = "svg", feature = "svg_parsing"))]
    pub use super::svg::SvgParseError;
}