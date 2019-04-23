//! Type translation functions (from azul-css to webrender types)
//!
//! The reason for doing this is so that azul-css doesn't depend on webrender or euclid
//! (since webrender is a huge dependency) just to use the types. Only if you depend on
//! azul, you have to depend on webrender.

use webrender::api::{
    LayoutRect as WrLayoutRect,
    HitTestItem as WrHitTestItem,
    FontKey as WrFontKey,
    FontInstanceKey as WrFontInstanceKey,
    ImageKey as WrImageKey,
    IdNamespace as WrIdNamespace,
    PipelineId as WrPipelineId,
    ColorU as WrColorU,
    BorderRadius as WrBorderRadius,
    BorderSide as WrBorderSide,
    NormalBorder as WrNormalBorder,
    LayoutPoint as WrLayoutPoint,
    BorderDetails as WrBorderDetails,
    BoxShadowClipMode as WrBoxShadowClipMode,
    ExtendMode as WrExtendMode,
    BorderStyle as WrBorderStyle,
    LayoutSideOffsets as WrLayoutSideOffsets,
    ImageFormat as WrImageFormat,
    ImageDescriptor as WrImageDescriptor,
};
use azul_core::{
    callbacks::{HidpiAdjustedBounds, HitTestItem, PipelineId},
    window::{LogicalPosition, LogicalSize, MouseCursorType},
    app_resources::{FontKey, Au, FontInstanceKey, ImageKey, IdNamespace, RawImageFormat as ImageFormat, ImageDescriptor},
};
use azul_css::{
    ColorU as CssColorU,
    BorderRadius as CssBorderRadius,
    BorderSide as CssBorderSide,
    NormalBorder as CssNormalBorder,
    LayoutPoint as CssLayoutPoint,
    LayoutRect as CssLayoutRect,
    LayoutSize as CssLayoutSize,
    BorderDetails as CssBorderDetails,
    BoxShadowClipMode as CssBoxShadowClipMode,
    ExtendMode as CssExtendMode,
    BorderStyle as CssBorderStyle,
    LayoutSideOffsets as CssLayoutSideOffsets,
};
use app_units::Au as WrAu;
use glium::glutin::MouseCursor as WinitCursorType;

#[inline(always)]
pub(crate) const fn wr_translate_hittest_item(input: WrHitTestItem) -> HitTestItem {
    HitTestItem {
        pipeline: PipelineId(input.pipeline.0, input.pipeline.1),
        tag: input.tag,
        point_in_viewport: LogicalPosition::new(input.point_in_viewport.x, input.point_in_viewport.y),
        point_relative_to_item: LogicalPosition::new(input.point_relative_to_item.x, input.point_relative_to_item.y),
    }
}

#[inline(always)]
pub(crate) const fn hidpi_rect_from_bounds(bounds: WrLayoutRect, hidpi_factor: f32, winit_hidpi_factor: f32) -> HidpiAdjustedBounds {
    let logical_size = LogicalSize::new(bounds.size.width, bounds.size.height);
    HidpiAdjustedBounds {
        logical_size,
        hidpi_factor,
        winit_hidpi_factor,
    }
}

// webrender -> core

#[inline(always)]
const fn translate_id_namespace_wr(ns: WrIdNamespace) -> IdNamespace {
    IdNamespace(ns.0)
}

#[inline(always)]
pub(crate) const fn translate_pipeline_id_wr(pipeline_id: WrPipelineId) -> PipelineId {
    PipelineId(pipeline_id.0, pipeline_id.1)
}

#[inline(always)]
pub(crate) const fn translate_font_key_wr(font_key: WrFontKey) -> FontKey {
    FontKey { key: font_key.1, namespace: translate_id_namespace_wr(font_key.0) }
}

#[inline(always)]
pub(crate) const fn translate_font_instance_key_wr(font_instance_key: WrFontInstanceKey) -> FontInstanceKey {
    FontInstanceKey { key: font_instance_key.1, namespace: translate_id_namespace_wr(font_instance_key.0) }
}

#[inline(always)]
pub(crate) const fn translate_image_key_wr(image_key: WrImageKey) -> ImageKey {
    ImageKey { key: image_key.1, namespace: translate_id_namespace_wr(image_key.0) }
}

#[inline]
pub(crate) fn translate_image_descriptor_wr(descriptor: WrImageDescriptor) -> ImageDescriptor {
    ImageDescriptor {
        format: translate_image_format_wr(descriptor.format),
        dimensions: (descriptor.size.width as usize, descriptor.size.height as usize),
        stride: descriptor.stride,
        offset: descriptor.offset,
        is_opaque: descriptor.is_opaque,
        allow_mipmaps: descriptor.allow_mipmaps,
    }
}

#[inline]
pub const fn translate_image_format_wr(input: WrImageFormat) -> ImageFormat {
    match input {
        WrImageFormat::R8 => ImageFormat::R8,
        WrImageFormat::R16 => ImageFormat::R16,
        WrImageFormat::BGRA8 => ImageFormat::BGRA8,
        WrImageFormat::RGBAF32 => ImageFormat::RGBAF32,
        WrImageFormat::RG8 => ImageFormat::RG8,
        WrImageFormat::RGBAI32 => ImageFormat::RGBAI32,
        WrImageFormat::RGBA8 => ImageFormat::RGBA8,
    }
}

// core -> webrender

#[inline(always)]
const fn wr_translate_id_namespace(ns: IdNamespace) -> WrIdNamespace {
    WrIdNamespace(ns.0)
}

#[inline(always)]
pub(crate) const fn wr_translate_font_key(font_key: FontKey) -> WrFontKey {
    WrFontKey(wr_translate_id_namespace(font_key.namespace), font_key.key)
}

#[inline(always)]
pub(crate) const fn wr_translate_font_instance_key(font_instance_key: FontInstanceKey) -> WrFontInstanceKey {
    WrFontInstanceKey(wr_translate_id_namespace(font_instance_key.namespace), font_instance_key.key)
}

#[inline(always)]
pub(crate) const fn wr_translate_image_key(image_key: ImageKey) -> WrImageKey {
    WrImageKey(wr_translate_id_namespace(image_key.namespace), image_key.key)
}

#[inline(always)]
pub(crate) const fn wr_translate_pipeline_id(pipeline_id: PipelineId) -> WrPipelineId {
    WrPipelineId(pipeline_id.0, pipeline_id.1)
}

#[inline]
pub(crate) fn wr_translate_image_descriptor(descriptor: ImageDescriptor) -> WrImageDescriptor {
    use webrender::api::DeviceIntSize;
    WrImageDescriptor {
        format: wr_translate_image_format(descriptor.format),
        size: DeviceIntSize::new(descriptor.dimensions.0 as i32, descriptor.dimensions.1 as i32),
        stride: descriptor.stride,
        offset: descriptor.offset,
        is_opaque: descriptor.is_opaque,
        allow_mipmaps: descriptor.allow_mipmaps,
    }
}

#[inline(always)]
pub(crate) const fn translate_au(au: Au) -> WrAu {
    WrAu(au.0)
}

#[inline(always)]
pub const fn wr_translate_box_shadow_clip_mode(input: CssBoxShadowClipMode) -> WrBoxShadowClipMode {
    match input {
        CssBoxShadowClipMode::Outset => WrBoxShadowClipMode::Outset,
        CssBoxShadowClipMode::Inset => WrBoxShadowClipMode::Inset,
    }
}

#[inline(always)]
pub const fn wr_translate_extend_mode(input: CssExtendMode) -> WrExtendMode {
    match input {
        CssExtendMode::Clamp => WrExtendMode::Clamp,
        CssExtendMode::Repeat => WrExtendMode::Repeat,
    }
}

#[inline(always)]
pub const fn wr_translate_border_style(input: CssBorderStyle) -> WrBorderStyle {
    match input {
        CssBorderStyle::None => WrBorderStyle::None,
        CssBorderStyle::Solid => WrBorderStyle::Solid,
        CssBorderStyle::Double => WrBorderStyle::Double,
        CssBorderStyle::Dotted => WrBorderStyle::Dotted,
        CssBorderStyle::Dashed => WrBorderStyle::Dashed,
        CssBorderStyle::Hidden => WrBorderStyle::Hidden,
        CssBorderStyle::Groove => WrBorderStyle::Groove,
        CssBorderStyle::Ridge => WrBorderStyle::Ridge,
        CssBorderStyle::Inset => WrBorderStyle::Inset,
        CssBorderStyle::Outset => WrBorderStyle::Outset,
    }
}

#[inline(always)]
pub const fn wr_translate_image_format(input: ImageFormat) -> WrImageFormat {
    match input {
        ImageFormat::R8 => WrImageFormat::R8,
        ImageFormat::R16 => WrImageFormat::R16,
        ImageFormat::BGRA8 => WrImageFormat::BGRA8,
        ImageFormat::RGBAF32 => WrImageFormat::RGBAF32,
        ImageFormat::RG8 => WrImageFormat::RG8,
        ImageFormat::RGBAI32 => WrImageFormat::RGBAI32,
        ImageFormat::RGBA8 => WrImageFormat::RGBA8,
    }
}

#[inline(always)]
pub fn wr_translate_layout_side_offsets(input: CssLayoutSideOffsets) -> WrLayoutSideOffsets {
    WrLayoutSideOffsets::new(
        input.top.get(),
        input.right.get(),
        input.bottom.get(),
        input.left.get(),
    )
}

#[inline(always)]
pub const fn wr_translate_color_u(input: CssColorU) -> WrColorU {
    WrColorU { r: input.r, g: input.g, b: input.b, a: input.a }
}

#[inline(always)]
pub const fn wr_translate_border_radius(input: CssBorderRadius) -> WrBorderRadius {
    use webrender::api::LayoutSize;
    let CssBorderRadius { top_left, top_right, bottom_left, bottom_right } = input;
    WrBorderRadius {
        top_left: LayoutSize::new(top_left.width.to_pixels(), top_left.height.to_pixels()),
        top_right: LayoutSize::new(top_right.width.to_pixels(), top_right.height.to_pixels()),
        bottom_left: LayoutSize::new(bottom_left.width.to_pixels(), bottom_left.height.to_pixels()),
        bottom_right: LayoutSize::new(bottom_right.width.to_pixels(), bottom_right.height.to_pixels()),
    }
}

#[inline(always)]
pub const fn wr_translate_border_side(input: CssBorderSide) -> WrBorderSide {
    WrBorderSide {
        color: wr_translate_color_u(input.color).into(),
        style: wr_translate_border_style(input.style),
    }
}

#[inline(always)]
pub const fn wr_translate_normal_border(input: CssNormalBorder) -> WrNormalBorder {

    // Webrender crashes if anti-aliasing is disabled and the border isn't pure-solid
    let is_not_solid = [input.top.style, input.bottom.style, input.left.style, input.right.style].iter().any(|style| {
        *style != CssBorderStyle::Solid
    });
    let do_aa = input.radius.is_some() || is_not_solid;

    WrNormalBorder {
        left: wr_translate_border_side(input.left),
        right: wr_translate_border_side(input.right),
        top: wr_translate_border_side(input.top),
        bottom: wr_translate_border_side(input.bottom),
        radius: wr_translate_border_radius(input.radius.unwrap_or_default()),
        do_aa,
    }
}

#[inline(always)]
pub const fn wr_translate_layout_point(input: CssLayoutPoint) -> WrLayoutPoint {
    WrLayoutPoint::new(input.x, input.y)
}

// NOTE: Reverse direction: Translate from webrender::LayoutRect to css::LayoutRect
#[inline(always)]
pub const fn wr_translate_layout_rect(input: WrLayoutRect) -> CssLayoutRect {
    CssLayoutRect {
        origin: CssLayoutPoint { x: input.origin.x, y: input.origin.y },
        size: CssLayoutSize { width: input.size.width, height: input.size.height },
    }
}

// NOTE: Reverse direction: Translate from webrender::LayoutRect to css::LayoutRect
#[inline(always)]
pub const fn wr_translate_border_details(input: CssBorderDetails) -> WrBorderDetails {
    let zero_border_side = WrBorderSide {
        color: WrColorU { r: 0, g: 0, b: 0, a: 0 }.into(),
        style: WrBorderStyle::None
    };

    match input {
        CssBorderDetails::Normal(normal) => WrBorderDetails::Normal(wr_translate_normal_border(normal)),
        // TODO: Do 9patch border properly - currently this can't be reached since there
        // is no parsing for 9patch border yet!
        CssBorderDetails::NinePatch(_) => WrBorderDetails::Normal(WrNormalBorder {
            left: zero_border_side,
            right: zero_border_side,
            bottom: zero_border_side,
            top: zero_border_side,
            radius: WrBorderRadius::zero(),
            do_aa: false,
        })
    }
}

pub const fn winit_translate_cursor(input: MouseCursorType) -> WinitCursorType {
    match input {
        MouseCursorType::Alias              => WinitCursorType::Alias,
        MouseCursorType::AllScroll          => WinitCursorType::AllScroll,
        MouseCursorType::Cell               => WinitCursorType::Cell,
        MouseCursorType::ColResize          => WinitCursorType::ColResize,
        MouseCursorType::ContextMenu        => WinitCursorType::ContextMenu,
        MouseCursorType::Copy               => WinitCursorType::Copy,
        MouseCursorType::Crosshair          => WinitCursorType::Crosshair,
        MouseCursorType::Arrow              => WinitCursorType::Arrow,
        MouseCursorType::EResize            => WinitCursorType::EResize,
        MouseCursorType::EwResize           => WinitCursorType::EwResize,
        MouseCursorType::Grab               => WinitCursorType::Grab,
        MouseCursorType::Grabbing           => WinitCursorType::Grabbing,
        MouseCursorType::Help               => WinitCursorType::Help,
        MouseCursorType::Move               => WinitCursorType::Move,
        MouseCursorType::NResize            => WinitCursorType::NResize,
        MouseCursorType::NsResize           => WinitCursorType::NsResize,
        MouseCursorType::NeswResize         => WinitCursorType::NeswResize,
        MouseCursorType::NwseResize         => WinitCursorType::NwseResize,
        MouseCursorType::Hand               => WinitCursorType::Hand,
        MouseCursorType::Progress           => WinitCursorType::Progress,
        MouseCursorType::RowResize          => WinitCursorType::RowResize,
        MouseCursorType::SResize            => WinitCursorType::SResize,
        MouseCursorType::SeResize           => WinitCursorType::SeResize,
        MouseCursorType::Text               => WinitCursorType::Text,
        MouseCursorType::Arrow              => WinitCursorType::Arrow,
        MouseCursorType::VerticalText       => WinitCursorType::VerticalText,
        MouseCursorType::WResize            => WinitCursorType::WResize,
        MouseCursorType::Wait               => WinitCursorType::Wait,
        MouseCursorType::ZoomIn             => WinitCursorType::ZoomIn,
        MouseCursorType::ZoomOut            => WinitCursorType::ZoomOut,
    }
}