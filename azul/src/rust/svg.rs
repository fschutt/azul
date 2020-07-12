    #![allow(dead_code, unused_imports)]
    //! SVG parsing and rendering functions
    use crate::dll::*;
    use std::ffi::c_void;
    use crate::gl::U8VecRef;


    /// `SvgMultiPolygon` struct
    pub use crate::dll::AzSvgMultiPolygon as SvgMultiPolygon;

    impl std::fmt::Debug for SvgMultiPolygon { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_multi_polygon_fmt_debug)(self)) } }
    impl Clone for SvgMultiPolygon { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_multi_polygon_deep_copy)(self) } }
    impl Drop for SvgMultiPolygon { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_multi_polygon_delete)(self); } }


    /// `SvgNode` struct
    pub use crate::dll::AzSvgNode as SvgNode;

    impl std::fmt::Debug for SvgNode { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_node_fmt_debug)(self)) } }
    impl Clone for SvgNode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_node_deep_copy)(self) } }
    impl Drop for SvgNode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_node_delete)(self); } }


    /// `SvgStyledNode` struct
    pub use crate::dll::AzSvgStyledNode as SvgStyledNode;

    impl std::fmt::Debug for SvgStyledNode { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_styled_node_fmt_debug)(self)) } }
    impl Clone for SvgStyledNode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_styled_node_deep_copy)(self) } }
    impl Drop for SvgStyledNode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_styled_node_delete)(self); } }


    /// `SvgCircle` struct
    pub use crate::dll::AzSvgCircle as SvgCircle;

    impl std::fmt::Debug for SvgCircle { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_circle_fmt_debug)(self)) } }
    impl Clone for SvgCircle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_circle_deep_copy)(self) } }
    impl Drop for SvgCircle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_circle_delete)(self); } }


    /// `SvgPath` struct
    pub use crate::dll::AzSvgPath as SvgPath;

    impl std::fmt::Debug for SvgPath { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_path_fmt_debug)(self)) } }
    impl Clone for SvgPath { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_path_deep_copy)(self) } }
    impl Drop for SvgPath { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_path_delete)(self); } }


    /// `SvgPathElement` struct
    pub use crate::dll::AzSvgPathElement as SvgPathElement;

    impl std::fmt::Debug for SvgPathElement { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_path_element_fmt_debug)(self)) } }
    impl Clone for SvgPathElement { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_path_element_deep_copy)(self) } }
    impl Drop for SvgPathElement { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_path_element_delete)(self); } }


    /// `SvgLine` struct
    pub use crate::dll::AzSvgLine as SvgLine;

    impl std::fmt::Debug for SvgLine { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_line_fmt_debug)(self)) } }
    impl Clone for SvgLine { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_line_deep_copy)(self) } }
    impl Drop for SvgLine { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_line_delete)(self); } }


    /// `SvgPoint` struct
    pub use crate::dll::AzSvgPoint as SvgPoint;

    impl std::fmt::Debug for SvgPoint { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_point_fmt_debug)(self)) } }
    impl Clone for SvgPoint { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_point_deep_copy)(self) } }
    impl Drop for SvgPoint { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_point_delete)(self); } }


    /// `SvgVertex` struct
    pub use crate::dll::AzSvgVertex as SvgVertex;

    impl std::fmt::Debug for SvgVertex { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_vertex_fmt_debug)(self)) } }
    impl Clone for SvgVertex { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_vertex_deep_copy)(self) } }
    impl Drop for SvgVertex { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_vertex_delete)(self); } }


    /// `SvgQuadraticCurve` struct
    pub use crate::dll::AzSvgQuadraticCurve as SvgQuadraticCurve;

    impl std::fmt::Debug for SvgQuadraticCurve { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_quadratic_curve_fmt_debug)(self)) } }
    impl Clone for SvgQuadraticCurve { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_quadratic_curve_deep_copy)(self) } }
    impl Drop for SvgQuadraticCurve { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_quadratic_curve_delete)(self); } }


    /// `SvgCubicCurve` struct
    pub use crate::dll::AzSvgCubicCurve as SvgCubicCurve;

    impl std::fmt::Debug for SvgCubicCurve { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_cubic_curve_fmt_debug)(self)) } }
    impl Clone for SvgCubicCurve { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_cubic_curve_deep_copy)(self) } }
    impl Drop for SvgCubicCurve { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_cubic_curve_delete)(self); } }


    /// `SvgRect` struct
    pub use crate::dll::AzSvgRect as SvgRect;

    impl std::fmt::Debug for SvgRect { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_rect_fmt_debug)(self)) } }
    impl Clone for SvgRect { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_rect_deep_copy)(self) } }
    impl Drop for SvgRect { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_rect_delete)(self); } }


    /// `TesselatedCPUSvgNode` struct
    pub use crate::dll::AzTesselatedCPUSvgNode as TesselatedCPUSvgNode;

    impl std::fmt::Debug for TesselatedCPUSvgNode { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_tesselated_cpu_svg_node_fmt_debug)(self)) } }
    impl Clone for TesselatedCPUSvgNode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_tesselated_cpu_svg_node_deep_copy)(self) } }
    impl Drop for TesselatedCPUSvgNode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_tesselated_cpu_svg_node_delete)(self); } }


    /// `TesselatedGPUSvgNode` struct
    pub use crate::dll::AzTesselatedGPUSvgNode as TesselatedGPUSvgNode;

    impl std::fmt::Debug for TesselatedGPUSvgNode { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_tesselated_gpu_svg_node_fmt_debug)(self)) } }
    impl Drop for TesselatedGPUSvgNode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_tesselated_gpu_svg_node_delete)(self); } }


    /// `SvgLineCap` struct
    pub use crate::dll::AzSvgLineCap as SvgLineCap;

    impl std::fmt::Debug for SvgLineCap { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_line_cap_fmt_debug)(self)) } }
    impl Clone for SvgLineCap { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_line_cap_deep_copy)(self) } }
    impl Drop for SvgLineCap { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_line_cap_delete)(self); } }


    /// `SvgParseOptions` struct
    pub use crate::dll::AzSvgParseOptions as SvgParseOptions;

    impl SvgParseOptions {
        /// Creates a new `SvgParseOptions` instance.
        pub fn default() -> Self { (crate::dll::get_azul_dll().az_svg_parse_options_default)() }
    }

    impl std::fmt::Debug for SvgParseOptions { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_parse_options_fmt_debug)(self)) } }
    impl Clone for SvgParseOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_parse_options_deep_copy)(self) } }
    impl Drop for SvgParseOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_parse_options_delete)(self); } }


    /// `ShapeRendering` struct
    pub use crate::dll::AzShapeRendering as ShapeRendering;

    impl std::fmt::Debug for ShapeRendering { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_shape_rendering_fmt_debug)(self)) } }
    impl Clone for ShapeRendering { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_shape_rendering_deep_copy)(self) } }
    impl Drop for ShapeRendering { fn drop(&mut self) { (crate::dll::get_azul_dll().az_shape_rendering_delete)(self); } }


    /// `TextRendering` struct
    pub use crate::dll::AzTextRendering as TextRendering;

    impl std::fmt::Debug for TextRendering { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_text_rendering_fmt_debug)(self)) } }
    impl Clone for TextRendering { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_text_rendering_deep_copy)(self) } }
    impl Drop for TextRendering { fn drop(&mut self) { (crate::dll::get_azul_dll().az_text_rendering_delete)(self); } }


    /// `ImageRendering` struct
    pub use crate::dll::AzImageRendering as ImageRendering;

    impl std::fmt::Debug for ImageRendering { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_image_rendering_fmt_debug)(self)) } }
    impl Clone for ImageRendering { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_image_rendering_deep_copy)(self) } }
    impl Drop for ImageRendering { fn drop(&mut self) { (crate::dll::get_azul_dll().az_image_rendering_delete)(self); } }


    /// `FontDatabase` struct
    pub use crate::dll::AzFontDatabase as FontDatabase;

    impl std::fmt::Debug for FontDatabase { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_font_database_fmt_debug)(self)) } }
    impl Clone for FontDatabase { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_font_database_deep_copy)(self) } }
    impl Drop for FontDatabase { fn drop(&mut self) { (crate::dll::get_azul_dll().az_font_database_delete)(self); } }


    /// `SvgRenderOptions` struct
    pub use crate::dll::AzSvgRenderOptions as SvgRenderOptions;

    impl SvgRenderOptions {
        /// Creates a new `SvgRenderOptions` instance.
        pub fn default() -> Self { (crate::dll::get_azul_dll().az_svg_render_options_default)() }
    }

    impl std::fmt::Debug for SvgRenderOptions { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_render_options_fmt_debug)(self)) } }
    impl Clone for SvgRenderOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_render_options_deep_copy)(self) } }
    impl Drop for SvgRenderOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_render_options_delete)(self); } }


    /// `SvgFitTo` struct
    pub use crate::dll::AzSvgFitTo as SvgFitTo;

    impl std::fmt::Debug for SvgFitTo { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_fit_to_fmt_debug)(self)) } }
    impl Clone for SvgFitTo { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_fit_to_deep_copy)(self) } }
    impl Drop for SvgFitTo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_fit_to_delete)(self); } }


    /// `Svg` struct
    pub use crate::dll::AzSvg as Svg;

    impl Svg {
        /// Creates a new `Svg` instance.
        pub fn parse(svg_bytes: U8VecRef, parse_options: SvgParseOptions) -> Self { (crate::dll::get_azul_dll().az_svg_parse)(svg_bytes, parse_options) }
        /// Calls the `Svg::render_to_image` function.
        pub fn render_to_image(&self, render_options: SvgRenderOptions)  -> crate::option::OptionRawImage { (crate::dll::get_azul_dll().az_svg_render_to_image)(self, render_options) }
    }

    impl std::fmt::Debug for Svg { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_fmt_debug)(self)) } }
    impl Clone for Svg { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_deep_copy)(self) } }
    impl Drop for Svg { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_delete)(self); } }


    /// `SvgXmlNode` struct
    pub use crate::dll::AzSvgXmlNode as SvgXmlNode;

    impl std::fmt::Debug for SvgXmlNode { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_xml_node_fmt_debug)(self)) } }
    impl Clone for SvgXmlNode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_xml_node_deep_copy)(self) } }
    impl Drop for SvgXmlNode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_xml_node_delete)(self); } }


    /// `SvgLineJoin` struct
    pub use crate::dll::AzSvgLineJoin as SvgLineJoin;

    impl std::fmt::Debug for SvgLineJoin { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_line_join_fmt_debug)(self)) } }
    impl Clone for SvgLineJoin { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_line_join_deep_copy)(self) } }
    impl Drop for SvgLineJoin { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_line_join_delete)(self); } }


    /// `SvgDashPattern` struct
    pub use crate::dll::AzSvgDashPattern as SvgDashPattern;

    impl std::fmt::Debug for SvgDashPattern { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_dash_pattern_fmt_debug)(self)) } }
    impl Clone for SvgDashPattern { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_dash_pattern_deep_copy)(self) } }
    impl Drop for SvgDashPattern { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_dash_pattern_delete)(self); } }


    /// `SvgStyle` struct
    pub use crate::dll::AzSvgStyle as SvgStyle;

    impl std::fmt::Debug for SvgStyle { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_style_fmt_debug)(self)) } }
    impl Clone for SvgStyle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_style_deep_copy)(self) } }
    impl Drop for SvgStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_style_delete)(self); } }


    /// `SvgFillStyle` struct
    pub use crate::dll::AzSvgFillStyle as SvgFillStyle;

    impl std::fmt::Debug for SvgFillStyle { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_fill_style_fmt_debug)(self)) } }
    impl Clone for SvgFillStyle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_fill_style_deep_copy)(self) } }
    impl Drop for SvgFillStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_fill_style_delete)(self); } }


    /// `SvgStrokeStyle` struct
    pub use crate::dll::AzSvgStrokeStyle as SvgStrokeStyle;

    impl std::fmt::Debug for SvgStrokeStyle { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_stroke_style_fmt_debug)(self)) } }
    impl Clone for SvgStrokeStyle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_stroke_style_deep_copy)(self) } }
    impl Drop for SvgStrokeStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_stroke_style_delete)(self); } }


    /// `SvgNodeId` struct
    pub use crate::dll::AzSvgNodeId as SvgNodeId;

    impl std::fmt::Debug for SvgNodeId { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_node_id_fmt_debug)(self)) } }
    impl Clone for SvgNodeId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_node_id_deep_copy)(self) } }
    impl Drop for SvgNodeId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_node_id_delete)(self); } }
