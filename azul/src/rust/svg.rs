    #![allow(dead_code, unused_imports)]
    //! SVG parsing and rendering functions
    use crate::dll::*;
    use std::ffi::c_void;
    use crate::gl::U8VecRef;


    /// `SvgMultiPolygon` struct
    #[doc(inline)] pub use crate::dll::AzSvgMultiPolygon as SvgMultiPolygon;

    impl Clone for SvgMultiPolygon { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_multi_polygon_deep_copy)(self) } }
    impl Drop for SvgMultiPolygon { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_multi_polygon_delete)(self); } }


    /// `SvgNode` struct
    #[doc(inline)] pub use crate::dll::AzSvgNode as SvgNode;

    impl Clone for SvgNode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_node_deep_copy)(self) } }
    impl Drop for SvgNode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_node_delete)(self); } }


    /// `SvgStyledNode` struct
    #[doc(inline)] pub use crate::dll::AzSvgStyledNode as SvgStyledNode;

    impl Clone for SvgStyledNode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_styled_node_deep_copy)(self) } }
    impl Drop for SvgStyledNode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_styled_node_delete)(self); } }


    /// `SvgCircle` struct
    #[doc(inline)] pub use crate::dll::AzSvgCircle as SvgCircle;

    impl Clone for SvgCircle { fn clone(&self) -> Self { *self } }
    impl Copy for SvgCircle { }


    /// `SvgPath` struct
    #[doc(inline)] pub use crate::dll::AzSvgPath as SvgPath;

    impl Clone for SvgPath { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_path_deep_copy)(self) } }
    impl Drop for SvgPath { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_path_delete)(self); } }


    /// `SvgPathElement` struct
    #[doc(inline)] pub use crate::dll::AzSvgPathElement as SvgPathElement;

    impl Clone for SvgPathElement { fn clone(&self) -> Self { *self } }
    impl Copy for SvgPathElement { }


    /// `SvgLine` struct
    #[doc(inline)] pub use crate::dll::AzSvgLine as SvgLine;

    impl Clone for SvgLine { fn clone(&self) -> Self { *self } }
    impl Copy for SvgLine { }


    /// `SvgPoint` struct
    #[doc(inline)] pub use crate::dll::AzSvgPoint as SvgPoint;

    impl Clone for SvgPoint { fn clone(&self) -> Self { *self } }
    impl Copy for SvgPoint { }


    /// `SvgVertex` struct
    #[doc(inline)] pub use crate::dll::AzSvgVertex as SvgVertex;

    impl Clone for SvgVertex { fn clone(&self) -> Self { *self } }
    impl Copy for SvgVertex { }


    /// `SvgQuadraticCurve` struct
    #[doc(inline)] pub use crate::dll::AzSvgQuadraticCurve as SvgQuadraticCurve;

    impl Clone for SvgQuadraticCurve { fn clone(&self) -> Self { *self } }
    impl Copy for SvgQuadraticCurve { }


    /// `SvgCubicCurve` struct
    #[doc(inline)] pub use crate::dll::AzSvgCubicCurve as SvgCubicCurve;

    impl Clone for SvgCubicCurve { fn clone(&self) -> Self { *self } }
    impl Copy for SvgCubicCurve { }


    /// `SvgRect` struct
    #[doc(inline)] pub use crate::dll::AzSvgRect as SvgRect;

    impl Clone for SvgRect { fn clone(&self) -> Self { *self } }
    impl Copy for SvgRect { }


    /// `TesselatedCPUSvgNode` struct
    #[doc(inline)] pub use crate::dll::AzTesselatedCPUSvgNode as TesselatedCPUSvgNode;

    impl Clone for TesselatedCPUSvgNode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_tesselated_cpu_svg_node_deep_copy)(self) } }
    impl Drop for TesselatedCPUSvgNode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_tesselated_cpu_svg_node_delete)(self); } }


    /// `SvgLineCap` struct
    #[doc(inline)] pub use crate::dll::AzSvgLineCap as SvgLineCap;

    impl Clone for SvgLineCap { fn clone(&self) -> Self { *self } }
    impl Copy for SvgLineCap { }


    /// `SvgParseOptions` struct
    #[doc(inline)] pub use crate::dll::AzSvgParseOptions as SvgParseOptions;

    impl SvgParseOptions {
        /// Creates a new `SvgParseOptions` instance.
        pub fn default() -> Self { (crate::dll::get_azul_dll().az_svg_parse_options_default)() }
    }

    impl Clone for SvgParseOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_parse_options_deep_copy)(self) } }
    impl Drop for SvgParseOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_parse_options_delete)(self); } }


    /// `ShapeRendering` struct
    #[doc(inline)] pub use crate::dll::AzShapeRendering as ShapeRendering;

    impl Clone for ShapeRendering { fn clone(&self) -> Self { *self } }
    impl Copy for ShapeRendering { }


    /// `TextRendering` struct
    #[doc(inline)] pub use crate::dll::AzTextRendering as TextRendering;

    impl Clone for TextRendering { fn clone(&self) -> Self { *self } }
    impl Copy for TextRendering { }


    /// `ImageRendering` struct
    #[doc(inline)] pub use crate::dll::AzImageRendering as ImageRendering;

    impl Clone for ImageRendering { fn clone(&self) -> Self { *self } }
    impl Copy for ImageRendering { }


    /// `FontDatabase` struct
    #[doc(inline)] pub use crate::dll::AzFontDatabase as FontDatabase;

    impl Clone for FontDatabase { fn clone(&self) -> Self { *self } }
    impl Copy for FontDatabase { }


    /// `SvgRenderOptions` struct
    #[doc(inline)] pub use crate::dll::AzSvgRenderOptions as SvgRenderOptions;

    impl SvgRenderOptions {
        /// Creates a new `SvgRenderOptions` instance.
        pub fn default() -> Self { (crate::dll::get_azul_dll().az_svg_render_options_default)() }
    }

    impl Clone for SvgRenderOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_render_options_deep_copy)(self) } }
    impl Drop for SvgRenderOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_render_options_delete)(self); } }


    /// `SvgFitTo` struct
    #[doc(inline)] pub use crate::dll::AzSvgFitTo as SvgFitTo;

    impl Clone for SvgFitTo { fn clone(&self) -> Self { *self } }
    impl Copy for SvgFitTo { }


    /// `Svg` struct
    #[doc(inline)] pub use crate::dll::AzSvg as Svg;

    impl Svg {
        /// Creates a new `Svg` instance.
        pub fn parse(svg_bytes: U8VecRef, parse_options: SvgParseOptions) ->  crate::result::ResultSvgSvgParseError { (crate::dll::get_azul_dll().az_svg_parse)(svg_bytes, parse_options) }
    }

    impl Clone for Svg { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_deep_copy)(self) } }
    impl Drop for Svg { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_delete)(self); } }


    /// `SvgXmlNode` struct
    #[doc(inline)] pub use crate::dll::AzSvgXmlNode as SvgXmlNode;

    impl Clone for SvgXmlNode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_xml_node_deep_copy)(self) } }
    impl Drop for SvgXmlNode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_xml_node_delete)(self); } }


    /// `SvgLineJoin` struct
    #[doc(inline)] pub use crate::dll::AzSvgLineJoin as SvgLineJoin;

    impl Clone for SvgLineJoin { fn clone(&self) -> Self { *self } }
    impl Copy for SvgLineJoin { }


    /// `SvgDashPattern` struct
    #[doc(inline)] pub use crate::dll::AzSvgDashPattern as SvgDashPattern;

    impl Clone for SvgDashPattern { fn clone(&self) -> Self { *self } }
    impl Copy for SvgDashPattern { }


    /// `SvgStyle` struct
    #[doc(inline)] pub use crate::dll::AzSvgStyle as SvgStyle;

    impl Clone for SvgStyle { fn clone(&self) -> Self { *self } }
    impl Copy for SvgStyle { }


    /// `SvgFillStyle` struct
    #[doc(inline)] pub use crate::dll::AzSvgFillStyle as SvgFillStyle;

    impl Clone for SvgFillStyle { fn clone(&self) -> Self { *self } }
    impl Copy for SvgFillStyle { }


    /// `SvgStrokeStyle` struct
    #[doc(inline)] pub use crate::dll::AzSvgStrokeStyle as SvgStrokeStyle;

    impl Clone for SvgStrokeStyle { fn clone(&self) -> Self { *self } }
    impl Copy for SvgStrokeStyle { }


    /// `SvgNodeId` struct
    #[doc(inline)] pub use crate::dll::AzSvgNodeId as SvgNodeId;

    impl Clone for SvgNodeId { fn clone(&self) -> Self { *self } }
    impl Copy for SvgNodeId { }
