    #![allow(dead_code, unused_imports)]
    //! SVG parsing and rendering functions
    use crate::dll::*;
    use std::ffi::c_void;


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
    impl Clone for TesselatedGPUSvgNode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_tesselated_gpu_svg_node_deep_copy)(self) } }
    impl Drop for TesselatedGPUSvgNode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_tesselated_gpu_svg_node_delete)(self); } }


    /// `SvgLineCap` struct
    pub use crate::dll::AzSvgLineCap as SvgLineCap;

    impl std::fmt::Debug for SvgLineCap { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_svg_line_cap_fmt_debug)(self)) } }
    impl Clone for SvgLineCap { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_svg_line_cap_deep_copy)(self) } }
    impl Drop for SvgLineCap { fn drop(&mut self) { (crate::dll::get_azul_dll().az_svg_line_cap_delete)(self); } }


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
