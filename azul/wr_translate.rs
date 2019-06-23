//! Type translation functions (from azul-css to webrender types)
//!
//! The reason for doing this is so that azul-css doesn't depend on webrender or euclid
//! (since webrender is a huge dependency) just to use the types. Only if you depend on
//! azul, you have to depend on webrender.

use webrender::api::{
    LayoutPrimitiveInfo as WrLayoutPrimitiveInfo,
    HitTestItem as WrHitTestItem,
    FontKey as WrFontKey,
    FontInstanceKey as WrFontInstanceKey,
    ImageKey as WrImageKey,
    IdNamespace as WrIdNamespace,
    PipelineId as WrPipelineId,
    ColorU as WrColorU,
    ColorF as WrColorF,
    BorderRadius as WrBorderRadius,
    BorderSide as WrBorderSide,
    BoxShadowClipMode as WrBoxShadowClipMode,
    ExtendMode as WrExtendMode,
    BorderStyle as WrBorderStyle,
    LayoutSideOffsets as WrLayoutSideOffsets,
    ImageFormat as WrImageFormat,
    ImageDescriptor as WrImageDescriptor,
    GlyphInstance as WrGlyphInstance,
    BuiltDisplayList as WrBuiltDisplayList,
    DisplayListBuilder as WrDisplayListBuilder,
    LayoutSize as WrLayoutSize,
    LayoutPoint as WrLayoutPoint,
    LayoutRect as WrLayoutRect,
    GlyphOptions as WrGlyphOptions,
    AlphaType as WrAlphaType,
    FontInstanceFlags as WrFontInstanceFlags,
    FontRenderMode as WrFontRenderMode,
    ImageRendering as WrImageRendering,
    ExternalScrollId as WrExternalScrollId,
};
use azul_core::{
    callbacks::{HidpiAdjustedBounds, HitTestItem, PipelineId},
    window::{MouseCursorType, VirtualKeyCode},
    app_resources::{
        FontKey, Au, FontInstanceKey, ImageKey,
        IdNamespace, RawImageFormat as ImageFormat, ImageDescriptor
    },
    display_list::{
        CachedDisplayList, GlyphInstance, DisplayListScrollFrame,
        DisplayListFrame, LayoutRectContent, DisplayListMsg,
        FontInstanceFlags, GlyphOptions, AlphaType, FontRenderMode, ImageRendering,
        StyleBorderRadius,
    },
    ui_solver::ExternalScrollId,
    window::LogicalSize,
};
use azul_css::{
    LayoutSize, LayoutPoint, LayoutRect,
    ColorU as CssColorU,
    ColorF as CssColorF,
    BorderSide as CssBorderSide,
    LayoutPoint as CssLayoutPoint,
    LayoutRect as CssLayoutRect,
    LayoutSize as CssLayoutSize,
    BoxShadowClipMode as CssBoxShadowClipMode,
    ExtendMode as CssExtendMode,
    BorderStyle as CssBorderStyle,
    LayoutSideOffsets as CssLayoutSideOffsets,
};
use app_units::Au as WrAu;
use glutin::{VirtualKeyCode as WinitVirtualKeyCode, MouseCursor as WinitCursorType};

#[inline(always)]
pub(crate) fn wr_translate_hittest_item(input: WrHitTestItem) -> HitTestItem {
    HitTestItem {
        pipeline: PipelineId(input.pipeline.0, input.pipeline.1),
        tag: input.tag,
        point_in_viewport: LayoutPoint::new(input.point_in_viewport.x, input.point_in_viewport.y),
        point_relative_to_item: LayoutPoint::new(input.point_relative_to_item.x, input.point_relative_to_item.y),
    }
}

#[inline(always)]
pub(crate) fn hidpi_rect_from_bounds(bounds: CssLayoutRect, hidpi_factor: f32, winit_hidpi_factor: f32) -> HidpiAdjustedBounds {
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
pub fn translate_image_format_wr(input: WrImageFormat) -> ImageFormat {
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

#[inline(always)]
pub(crate) const fn wr_translate_logical_size(logical_size: LogicalSize) -> LayoutSize {
    LayoutSize::new(logical_size.width, logical_size.height)
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
pub fn wr_translate_box_shadow_clip_mode(input: CssBoxShadowClipMode) -> WrBoxShadowClipMode {
    match input {
        CssBoxShadowClipMode::Outset => WrBoxShadowClipMode::Outset,
        CssBoxShadowClipMode::Inset => WrBoxShadowClipMode::Inset,
    }
}

#[inline(always)]
pub fn wr_translate_extend_mode(input: CssExtendMode) -> WrExtendMode {
    match input {
        CssExtendMode::Clamp => WrExtendMode::Clamp,
        CssExtendMode::Repeat => WrExtendMode::Repeat,
    }
}

#[inline(always)]
pub fn wr_translate_border_style(input: CssBorderStyle) -> WrBorderStyle {
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
pub fn wr_translate_image_format(input: ImageFormat) -> WrImageFormat {
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
pub const fn wr_translate_color_f(input: CssColorF) -> WrColorF {
    WrColorF { r: input.r, g: input.g, b: input.b, a: input.a }
}

#[inline]
pub fn wr_translate_border_radius(border_radius: StyleBorderRadius, rect_size: LayoutSize) -> WrBorderRadius {

    let StyleBorderRadius { top_left, top_right, bottom_left, bottom_right } = border_radius;

    let w = rect_size.width;
    let h = rect_size.height;

    // The "w / h" is necessary to convert percentage-based values into pixels, for example "border-radius: 50%;"

    let top_left_px_h = top_left.and_then(|tl| tl.get_property_or_default()).unwrap_or_default().0.to_pixels(w);
    let top_left_px_v = top_left.and_then(|tl| tl.get_property_or_default()).unwrap_or_default().0.to_pixels(h);

    let top_right_px_h = top_right.and_then(|tr| tr.get_property_or_default()).unwrap_or_default().0.to_pixels(w);
    let top_right_px_v = top_right.and_then(|tr| tr.get_property_or_default()).unwrap_or_default().0.to_pixels(h);

    let bottom_left_px_h = bottom_left.and_then(|bl| bl.get_property_or_default()).unwrap_or_default().0.to_pixels(w);
    let bottom_left_px_v = bottom_left.and_then(|bl| bl.get_property_or_default()).unwrap_or_default().0.to_pixels(h);

    let bottom_right_px_h = bottom_right.and_then(|br| br.get_property_or_default()).unwrap_or_default().0.to_pixels(w);
    let bottom_right_px_v = bottom_right.and_then(|br| br.get_property_or_default()).unwrap_or_default().0.to_pixels(h);

    WrBorderRadius {
        top_left: WrLayoutSize::new(top_left_px_h, top_left_px_v),
        top_right: WrLayoutSize::new(top_right_px_h, top_right_px_v),
        bottom_left: WrLayoutSize::new(bottom_left_px_h, bottom_left_px_v),
        bottom_right: WrLayoutSize::new(bottom_right_px_h, bottom_right_px_v),
    }
}

#[inline]
pub fn wr_translate_border_side(input: CssBorderSide) -> WrBorderSide {
    WrBorderSide {
        color: wr_translate_color_u(input.color).into(),
        style: wr_translate_border_style(input.style),
    }
}

// NOTE: Reverse direction: Translate from webrender::LayoutRect to css::LayoutRect
#[inline(always)]
pub const fn wr_translate_css_layout_rect(input: WrLayoutRect) -> CssLayoutRect {
    CssLayoutRect {
        origin: CssLayoutPoint { x: input.origin.x, y: input.origin.y },
        size: CssLayoutSize { width: input.size.width, height: input.size.height },
    }
}

#[inline]
pub(crate) fn winit_translate_cursor(input: MouseCursorType) -> WinitCursorType {
    match input {
        MouseCursorType::Default => WinitCursorType::Default,
        MouseCursorType::Crosshair => WinitCursorType::Crosshair,
        MouseCursorType::Hand => WinitCursorType::Hand,
        MouseCursorType::Arrow => WinitCursorType::Arrow,
        MouseCursorType::Move => WinitCursorType::Move,
        MouseCursorType::Text => WinitCursorType::Text,
        MouseCursorType::Wait => WinitCursorType::Wait,
        MouseCursorType::Help => WinitCursorType::Help,
        MouseCursorType::Progress => WinitCursorType::Progress,
        MouseCursorType::NotAllowed => WinitCursorType::NotAllowed,
        MouseCursorType::ContextMenu => WinitCursorType::ContextMenu,
        MouseCursorType::Cell => WinitCursorType::Cell,
        MouseCursorType::VerticalText => WinitCursorType::VerticalText,
        MouseCursorType::Alias => WinitCursorType::Alias,
        MouseCursorType::Copy => WinitCursorType::Copy,
        MouseCursorType::NoDrop => WinitCursorType::NoDrop,
        MouseCursorType::Grab => WinitCursorType::Grab,
        MouseCursorType::Grabbing => WinitCursorType::Grabbing,
        MouseCursorType::AllScroll => WinitCursorType::AllScroll,
        MouseCursorType::ZoomIn => WinitCursorType::ZoomIn,
        MouseCursorType::ZoomOut => WinitCursorType::ZoomOut,
        MouseCursorType::EResize => WinitCursorType::EResize,
        MouseCursorType::NResize => WinitCursorType::NResize,
        MouseCursorType::NeResize => WinitCursorType::NeResize,
        MouseCursorType::NwResize => WinitCursorType::NwResize,
        MouseCursorType::SResize => WinitCursorType::SResize,
        MouseCursorType::SeResize => WinitCursorType::SeResize,
        MouseCursorType::SwResize => WinitCursorType::SwResize,
        MouseCursorType::WResize => WinitCursorType::WResize,
        MouseCursorType::EwResize => WinitCursorType::EwResize,
        MouseCursorType::NsResize => WinitCursorType::NsResize,
        MouseCursorType::NeswResize => WinitCursorType::NeswResize,
        MouseCursorType::NwseResize => WinitCursorType::NwseResize,
        MouseCursorType::ColResize => WinitCursorType::ColResize,
        MouseCursorType::RowResize => WinitCursorType::RowResize,
    }
}

#[inline]
pub(crate) fn winit_translate_virtual_keycode(input: WinitVirtualKeyCode) -> VirtualKeyCode {
    match input {
        WinitVirtualKeyCode::Key1 => VirtualKeyCode::Key1,
        WinitVirtualKeyCode::Key2 => VirtualKeyCode::Key2,
        WinitVirtualKeyCode::Key3 => VirtualKeyCode::Key3,
        WinitVirtualKeyCode::Key4 => VirtualKeyCode::Key4,
        WinitVirtualKeyCode::Key5 => VirtualKeyCode::Key5,
        WinitVirtualKeyCode::Key6 => VirtualKeyCode::Key6,
        WinitVirtualKeyCode::Key7 => VirtualKeyCode::Key7,
        WinitVirtualKeyCode::Key8 => VirtualKeyCode::Key8,
        WinitVirtualKeyCode::Key9 => VirtualKeyCode::Key9,
        WinitVirtualKeyCode::Key0 => VirtualKeyCode::Key0,
        WinitVirtualKeyCode::A => VirtualKeyCode::A,
        WinitVirtualKeyCode::B => VirtualKeyCode::B,
        WinitVirtualKeyCode::C => VirtualKeyCode::C,
        WinitVirtualKeyCode::D => VirtualKeyCode::D,
        WinitVirtualKeyCode::E => VirtualKeyCode::E,
        WinitVirtualKeyCode::F => VirtualKeyCode::F,
        WinitVirtualKeyCode::G => VirtualKeyCode::G,
        WinitVirtualKeyCode::H => VirtualKeyCode::H,
        WinitVirtualKeyCode::I => VirtualKeyCode::I,
        WinitVirtualKeyCode::J => VirtualKeyCode::J,
        WinitVirtualKeyCode::K => VirtualKeyCode::K,
        WinitVirtualKeyCode::L => VirtualKeyCode::L,
        WinitVirtualKeyCode::M => VirtualKeyCode::M,
        WinitVirtualKeyCode::N => VirtualKeyCode::N,
        WinitVirtualKeyCode::O => VirtualKeyCode::O,
        WinitVirtualKeyCode::P => VirtualKeyCode::P,
        WinitVirtualKeyCode::Q => VirtualKeyCode::Q,
        WinitVirtualKeyCode::R => VirtualKeyCode::R,
        WinitVirtualKeyCode::S => VirtualKeyCode::S,
        WinitVirtualKeyCode::T => VirtualKeyCode::T,
        WinitVirtualKeyCode::U => VirtualKeyCode::U,
        WinitVirtualKeyCode::V => VirtualKeyCode::V,
        WinitVirtualKeyCode::W => VirtualKeyCode::W,
        WinitVirtualKeyCode::X => VirtualKeyCode::X,
        WinitVirtualKeyCode::Y => VirtualKeyCode::Y,
        WinitVirtualKeyCode::Z => VirtualKeyCode::Z,
        WinitVirtualKeyCode::Escape => VirtualKeyCode::Escape,
        WinitVirtualKeyCode::F1 => VirtualKeyCode::F1,
        WinitVirtualKeyCode::F2 => VirtualKeyCode::F2,
        WinitVirtualKeyCode::F3 => VirtualKeyCode::F3,
        WinitVirtualKeyCode::F4 => VirtualKeyCode::F4,
        WinitVirtualKeyCode::F5 => VirtualKeyCode::F5,
        WinitVirtualKeyCode::F6 => VirtualKeyCode::F6,
        WinitVirtualKeyCode::F7 => VirtualKeyCode::F7,
        WinitVirtualKeyCode::F8 => VirtualKeyCode::F8,
        WinitVirtualKeyCode::F9 => VirtualKeyCode::F9,
        WinitVirtualKeyCode::F10 => VirtualKeyCode::F10,
        WinitVirtualKeyCode::F11 => VirtualKeyCode::F11,
        WinitVirtualKeyCode::F12 => VirtualKeyCode::F12,
        WinitVirtualKeyCode::F13 => VirtualKeyCode::F13,
        WinitVirtualKeyCode::F14 => VirtualKeyCode::F14,
        WinitVirtualKeyCode::F15 => VirtualKeyCode::F15,
        WinitVirtualKeyCode::F16 => VirtualKeyCode::F16,
        WinitVirtualKeyCode::F17 => VirtualKeyCode::F17,
        WinitVirtualKeyCode::F18 => VirtualKeyCode::F18,
        WinitVirtualKeyCode::F19 => VirtualKeyCode::F19,
        WinitVirtualKeyCode::F20 => VirtualKeyCode::F20,
        WinitVirtualKeyCode::F21 => VirtualKeyCode::F21,
        WinitVirtualKeyCode::F22 => VirtualKeyCode::F22,
        WinitVirtualKeyCode::F23 => VirtualKeyCode::F23,
        WinitVirtualKeyCode::F24 => VirtualKeyCode::F24,
        WinitVirtualKeyCode::Snapshot => VirtualKeyCode::Snapshot,
        WinitVirtualKeyCode::Scroll => VirtualKeyCode::Scroll,
        WinitVirtualKeyCode::Pause => VirtualKeyCode::Pause,
        WinitVirtualKeyCode::Insert => VirtualKeyCode::Insert,
        WinitVirtualKeyCode::Home => VirtualKeyCode::Home,
        WinitVirtualKeyCode::Delete => VirtualKeyCode::Delete,
        WinitVirtualKeyCode::End => VirtualKeyCode::End,
        WinitVirtualKeyCode::PageDown => VirtualKeyCode::PageDown,
        WinitVirtualKeyCode::PageUp => VirtualKeyCode::PageUp,
        WinitVirtualKeyCode::Left => VirtualKeyCode::Left,
        WinitVirtualKeyCode::Up => VirtualKeyCode::Up,
        WinitVirtualKeyCode::Right => VirtualKeyCode::Right,
        WinitVirtualKeyCode::Down => VirtualKeyCode::Down,
        WinitVirtualKeyCode::Back => VirtualKeyCode::Back,
        WinitVirtualKeyCode::Return => VirtualKeyCode::Return,
        WinitVirtualKeyCode::Space => VirtualKeyCode::Space,
        WinitVirtualKeyCode::Compose => VirtualKeyCode::Compose,
        WinitVirtualKeyCode::Caret => VirtualKeyCode::Caret,
        WinitVirtualKeyCode::Numlock => VirtualKeyCode::Numlock,
        WinitVirtualKeyCode::Numpad0 => VirtualKeyCode::Numpad0,
        WinitVirtualKeyCode::Numpad1 => VirtualKeyCode::Numpad1,
        WinitVirtualKeyCode::Numpad2 => VirtualKeyCode::Numpad2,
        WinitVirtualKeyCode::Numpad3 => VirtualKeyCode::Numpad3,
        WinitVirtualKeyCode::Numpad4 => VirtualKeyCode::Numpad4,
        WinitVirtualKeyCode::Numpad5 => VirtualKeyCode::Numpad5,
        WinitVirtualKeyCode::Numpad6 => VirtualKeyCode::Numpad6,
        WinitVirtualKeyCode::Numpad7 => VirtualKeyCode::Numpad7,
        WinitVirtualKeyCode::Numpad8 => VirtualKeyCode::Numpad8,
        WinitVirtualKeyCode::Numpad9 => VirtualKeyCode::Numpad9,
        WinitVirtualKeyCode::AbntC1 => VirtualKeyCode::AbntC1,
        WinitVirtualKeyCode::AbntC2 => VirtualKeyCode::AbntC2,
        WinitVirtualKeyCode::Add => VirtualKeyCode::Add,
        WinitVirtualKeyCode::Apostrophe => VirtualKeyCode::Apostrophe,
        WinitVirtualKeyCode::Apps => VirtualKeyCode::Apps,
        WinitVirtualKeyCode::At => VirtualKeyCode::At,
        WinitVirtualKeyCode::Ax => VirtualKeyCode::Ax,
        WinitVirtualKeyCode::Backslash => VirtualKeyCode::Backslash,
        WinitVirtualKeyCode::Calculator => VirtualKeyCode::Calculator,
        WinitVirtualKeyCode::Capital => VirtualKeyCode::Capital,
        WinitVirtualKeyCode::Colon => VirtualKeyCode::Colon,
        WinitVirtualKeyCode::Comma => VirtualKeyCode::Comma,
        WinitVirtualKeyCode::Convert => VirtualKeyCode::Convert,
        WinitVirtualKeyCode::Decimal => VirtualKeyCode::Decimal,
        WinitVirtualKeyCode::Divide => VirtualKeyCode::Divide,
        WinitVirtualKeyCode::Equals => VirtualKeyCode::Equals,
        WinitVirtualKeyCode::Grave => VirtualKeyCode::Grave,
        WinitVirtualKeyCode::Kana => VirtualKeyCode::Kana,
        WinitVirtualKeyCode::Kanji => VirtualKeyCode::Kanji,
        WinitVirtualKeyCode::LAlt => VirtualKeyCode::LAlt,
        WinitVirtualKeyCode::LBracket => VirtualKeyCode::LBracket,
        WinitVirtualKeyCode::LControl => VirtualKeyCode::LControl,
        WinitVirtualKeyCode::LShift => VirtualKeyCode::LShift,
        WinitVirtualKeyCode::LWin => VirtualKeyCode::LWin,
        WinitVirtualKeyCode::Mail => VirtualKeyCode::Mail,
        WinitVirtualKeyCode::MediaSelect => VirtualKeyCode::MediaSelect,
        WinitVirtualKeyCode::MediaStop => VirtualKeyCode::MediaStop,
        WinitVirtualKeyCode::Minus => VirtualKeyCode::Minus,
        WinitVirtualKeyCode::Multiply => VirtualKeyCode::Multiply,
        WinitVirtualKeyCode::Mute => VirtualKeyCode::Mute,
        WinitVirtualKeyCode::MyComputer => VirtualKeyCode::MyComputer,
        WinitVirtualKeyCode::NavigateForward => VirtualKeyCode::NavigateForward,
        WinitVirtualKeyCode::NavigateBackward => VirtualKeyCode::NavigateBackward,
        WinitVirtualKeyCode::NextTrack => VirtualKeyCode::NextTrack,
        WinitVirtualKeyCode::NoConvert => VirtualKeyCode::NoConvert,
        WinitVirtualKeyCode::NumpadComma => VirtualKeyCode::NumpadComma,
        WinitVirtualKeyCode::NumpadEnter => VirtualKeyCode::NumpadEnter,
        WinitVirtualKeyCode::NumpadEquals => VirtualKeyCode::NumpadEquals,
        WinitVirtualKeyCode::OEM102 => VirtualKeyCode::OEM102,
        WinitVirtualKeyCode::Period => VirtualKeyCode::Period,
        WinitVirtualKeyCode::PlayPause => VirtualKeyCode::PlayPause,
        WinitVirtualKeyCode::Power => VirtualKeyCode::Power,
        WinitVirtualKeyCode::PrevTrack => VirtualKeyCode::PrevTrack,
        WinitVirtualKeyCode::RAlt => VirtualKeyCode::RAlt,
        WinitVirtualKeyCode::RBracket => VirtualKeyCode::RBracket,
        WinitVirtualKeyCode::RControl => VirtualKeyCode::RControl,
        WinitVirtualKeyCode::RShift => VirtualKeyCode::RShift,
        WinitVirtualKeyCode::RWin => VirtualKeyCode::RWin,
        WinitVirtualKeyCode::Semicolon => VirtualKeyCode::Semicolon,
        WinitVirtualKeyCode::Slash => VirtualKeyCode::Slash,
        WinitVirtualKeyCode::Sleep => VirtualKeyCode::Sleep,
        WinitVirtualKeyCode::Stop => VirtualKeyCode::Stop,
        WinitVirtualKeyCode::Subtract => VirtualKeyCode::Subtract,
        WinitVirtualKeyCode::Sysrq => VirtualKeyCode::Sysrq,
        WinitVirtualKeyCode::Tab => VirtualKeyCode::Tab,
        WinitVirtualKeyCode::Underline => VirtualKeyCode::Underline,
        WinitVirtualKeyCode::Unlabeled => VirtualKeyCode::Unlabeled,
        WinitVirtualKeyCode::VolumeDown => VirtualKeyCode::VolumeDown,
        WinitVirtualKeyCode::VolumeUp => VirtualKeyCode::VolumeUp,
        WinitVirtualKeyCode::Wake => VirtualKeyCode::Wake,
        WinitVirtualKeyCode::WebBack => VirtualKeyCode::WebBack,
        WinitVirtualKeyCode::WebFavorites => VirtualKeyCode::WebFavorites,
        WinitVirtualKeyCode::WebForward => VirtualKeyCode::WebForward,
        WinitVirtualKeyCode::WebHome => VirtualKeyCode::WebHome,
        WinitVirtualKeyCode::WebRefresh => VirtualKeyCode::WebRefresh,
        WinitVirtualKeyCode::WebSearch => VirtualKeyCode::WebSearch,
        WinitVirtualKeyCode::WebStop => VirtualKeyCode::WebStop,
        WinitVirtualKeyCode::Yen => VirtualKeyCode::Yen,
        WinitVirtualKeyCode::Copy => VirtualKeyCode::Copy,
        WinitVirtualKeyCode::Paste => VirtualKeyCode::Paste,
        WinitVirtualKeyCode::Cut => VirtualKeyCode::Cut,
    }
}

#[inline]
fn wr_translate_layouted_glyphs(input: Vec<GlyphInstance>) -> Vec<WrGlyphInstance> {
    input.into_iter().map(|glyph| WrGlyphInstance {
        index: glyph.index,
        point: WrLayoutPoint::new(glyph.point.x, glyph.point.y),
    }).collect()
}

#[inline]
fn wr_translate_layout_size(input: LayoutSize) -> WrLayoutSize {
    WrLayoutSize::new(input.width, input.height)
}

#[inline]
pub(crate) fn wr_translate_layout_point(input: LayoutPoint) -> WrLayoutPoint {
    WrLayoutPoint::new(input.x, input.y)
}

#[inline]
fn wr_translate_layout_rect(input: LayoutRect) -> WrLayoutRect {
    WrLayoutRect::new(wr_translate_layout_point(input.origin), wr_translate_layout_size(input.size))
}

#[inline]
fn translate_layout_size_wr(input: WrLayoutSize) -> LayoutSize {
    LayoutSize::new(input.width, input.height)
}

#[inline]
fn translate_layout_point_wr(input: WrLayoutPoint) -> LayoutPoint {
    LayoutPoint::new(input.x, input.y)
}

#[inline]
fn translate_layout_rect_wr(input: WrLayoutRect) -> LayoutRect {
    LayoutRect::new(translate_layout_point_wr(input.origin), translate_layout_size_wr(input.size))
}

#[inline]
fn wr_translate_font_instance_flags(font_instance_flags: FontInstanceFlags) -> WrFontInstanceFlags {
    WrFontInstanceFlags::from_bits_truncate(font_instance_flags)
}

#[inline]
fn wr_translate_font_render_mode(font_render_mode: FontRenderMode) -> WrFontRenderMode {
    match font_render_mode {
        FontRenderMode::Mono => WrFontRenderMode::Mono,
        FontRenderMode::Alpha => WrFontRenderMode::Alpha,
        FontRenderMode::Subpixel => WrFontRenderMode::Subpixel,
    }
}

#[inline]
fn wr_translate_glyph_options(glyph_options: GlyphOptions) -> WrGlyphOptions {
    WrGlyphOptions {
        render_mode: wr_translate_font_render_mode(glyph_options.render_mode),
        flags: wr_translate_font_instance_flags(glyph_options.flags),
    }
}

#[inline]
fn wr_translate_image_rendering(image_rendering: ImageRendering) -> WrImageRendering {
    match image_rendering {
        ImageRendering::Auto => WrImageRendering::Auto,
        ImageRendering::CrispEdges => WrImageRendering::CrispEdges,
        ImageRendering::Pixelated => WrImageRendering::Pixelated,
    }
}

#[inline]
fn wr_translate_alpha_type(alpha_type: AlphaType) -> WrAlphaType {
    match alpha_type {
        AlphaType::Alpha => WrAlphaType::Alpha,
        AlphaType::PremultipliedAlpha => WrAlphaType::PremultipliedAlpha,
    }
}

#[inline(always)]
pub(crate) fn wr_translate_external_scroll_id(scroll_id: ExternalScrollId) -> WrExternalScrollId {
    WrExternalScrollId(scroll_id.0, wr_translate_pipeline_id(scroll_id.1))
}

pub(crate) fn wr_translate_display_list(input: CachedDisplayList, pipeline_id: PipelineId) -> WrBuiltDisplayList {
    let mut builder = WrDisplayListBuilder::new(
        wr_translate_pipeline_id(pipeline_id),
        wr_translate_layout_size(input.root.get_size())
    );
    push_display_list_msg(&mut builder, input.root);
    builder.finalize().2
}

#[inline]
fn push_display_list_msg(builder: &mut WrDisplayListBuilder, msg: DisplayListMsg) {
    use azul_core::display_list::DisplayListMsg::*;
    match msg {
        Frame(f) => push_frame(builder, f),
        ScrollFrame(sf) => push_scroll_frame(builder, sf),
    }
}

#[inline]
fn push_frame(builder: &mut WrDisplayListBuilder, frame: DisplayListFrame) {

    use webrender::api::{
        ClipMode as WrClipMode,
        ComplexClipRegion as WrComplexClipRegion
    };

    let wr_rect = wr_translate_layout_rect(frame.rect);
    let wr_border_radius = wr_translate_border_radius(frame.border_radius, frame.rect.size);

    let info = WrLayoutPrimitiveInfo {
        rect: wr_rect,
        clip_rect: wr_translate_layout_rect(frame.clip_rect.unwrap_or(frame.rect)),
        is_backface_visible: false,
        tag: frame.tag,
    };

    let content_clip = WrComplexClipRegion::new(wr_rect, wr_border_radius, WrClipMode::Clip);
    let content_clip_id = builder.define_clip(wr_rect, vec![content_clip], /* image_mask: */ None);
    builder.push_clip_id(content_clip_id);

    for item in frame.content {
        push_display_list_content(builder, item, &info, frame.border_radius);
    }

    // pop content clip
    builder.pop_clip_id();

    // If the rect has an overflow:* property set
    let overflow_clip_id = frame.clip_rect.map(|clip_rect| {
        let clip_rect = wr_translate_layout_rect(clip_rect);
        let clip = WrComplexClipRegion::new(clip_rect, wr_border_radius, WrClipMode::Clip);
        let clip_id = builder.define_clip(clip_rect, vec![clip], /* image_mask: */ None);
        builder.push_clip_id(clip_id);
        clip_id
    });

    for child in frame.children {
        push_display_list_msg(builder, child);
    }

    // pop overflow clip
    if overflow_clip_id.is_some() {
        builder.pop_clip_id();
    }
}

#[inline]
fn push_scroll_frame(builder: &mut WrDisplayListBuilder, scroll_frame: DisplayListScrollFrame) {

    use azul_css::ColorU;
    use webrender::api::{
        ClipMode as WrClipMode,
        ScrollSensitivity as WrScrollSensitivity,
        ComplexClipRegion as WrComplexClipRegion
    };

    let wr_rect = wr_translate_layout_rect(scroll_frame.frame.rect);
    let wr_border_radius = wr_translate_border_radius(scroll_frame.frame.border_radius, scroll_frame.frame.rect.size);

    let scroll_frame_clip_region = WrComplexClipRegion::new(wr_rect, wr_border_radius, WrClipMode::Clip);

    let hit_test_info = WrLayoutPrimitiveInfo {
        rect: wr_rect,
        clip_rect: wr_translate_layout_rect(scroll_frame.frame.clip_rect.unwrap_or(scroll_frame.frame.rect)),
        is_backface_visible: false,
        tag: Some((scroll_frame.scroll_tag.0, 0)),
    };

    let info = WrLayoutPrimitiveInfo {
        rect: wr_rect,
        clip_rect: wr_rect,
        is_backface_visible: false,
        tag: scroll_frame.frame.tag,
    };

    let scroll_frame_clip_id = builder.define_scroll_frame(
        /* external id*/ Some(wr_translate_external_scroll_id(scroll_frame.scroll_id)),
        /* content_rect */ wr_translate_layout_rect(scroll_frame.content_rect),
        /* clip_rect */ wr_translate_layout_rect(scroll_frame.frame.clip_rect.unwrap_or(scroll_frame.frame.rect)),
        /* complex_clips */ vec![scroll_frame_clip_region],
        /* image_mask */ None,
        /* sensitivity */ WrScrollSensitivity::Script,
    );

    let hit_testing_clip_id = builder.define_clip(wr_rect, vec![scroll_frame_clip_region], None);

    // Push content (overflowing)
    let content_clip = WrComplexClipRegion::new(wr_rect, wr_border_radius, WrClipMode::Clip);
    let content_clip_id = builder.define_clip(wr_rect, vec![content_clip], /* image_mask: */ None);
    builder.push_clip_id(content_clip_id);

    for item in scroll_frame.frame.content {
        push_display_list_content(builder, item, &info, scroll_frame.frame.border_radius);
    }

    builder.pop_clip_id();
    // End pushing content

    builder.push_clip_id(hit_testing_clip_id); // push hit-testing clip
    builder.push_rect(&hit_test_info, wr_translate_color_u(ColorU::TRANSPARENT).into()); // push hit-testing rect
    builder.push_clip_id(scroll_frame_clip_id); // push scroll frame clip

    // only children should scroll, not the frame itself
    for child in scroll_frame.frame.children {
        push_display_list_msg(builder, child);
    }

    builder.pop_clip_id(); // pop scroll frame
    builder.pop_clip_id(); // pop hit-testing clip
}

#[inline]
fn push_display_list_content(
    builder: &mut WrDisplayListBuilder,
    content: LayoutRectContent,
    info: &WrLayoutPrimitiveInfo,
    radii: StyleBorderRadius,
) {

    use azul_core::display_list::LayoutRectContent::*;

    match content {
        Text { glyphs, font_instance_key, color, glyph_options, clip } => {
            text::push_text(builder, info, glyphs, font_instance_key, color, glyph_options, clip);
        },
        Background { content, size, offset, repeat  } => {
            background::push_background(builder, info, content, size, offset, repeat);
        },
        Image { size, offset, image_rendering, alpha_type, image_key, background_color } => {
            image::push_image(builder, info, size, offset, image_key, alpha_type, image_rendering, background_color);
        },
        Border { widths, colors, styles } => {
            border::push_border(builder, info, radii, widths, colors, styles);
        },
        BoxShadow { shadow, clip_mode } => {
            box_shadow::push_box_shadow(builder, translate_layout_rect_wr(info.rect), clip_mode, shadow, radii);
        },
    }
}

mod text {

    use webrender::api::{
        DisplayListBuilder as WrDisplayListBuilder,
        LayoutPrimitiveInfo as WrLayoutPrimitiveInfo,
    };
    use azul_core::{
        app_resources::FontInstanceKey,
        display_list::{GlyphOptions, GlyphInstance},
    };
    use azul_css::{ColorU, LayoutRect};

    pub(in super) fn push_text(
         builder: &mut WrDisplayListBuilder,
         info: &WrLayoutPrimitiveInfo,
         glyphs: Vec<GlyphInstance>,
         font_instance_key: FontInstanceKey,
         color: ColorU,
         glyph_options: Option<GlyphOptions>,
         clip: Option<LayoutRect>,
    ) {
        use super::{
            wr_translate_layouted_glyphs, wr_translate_font_instance_key,
            wr_translate_color_u, wr_translate_glyph_options,
            wr_translate_layout_size,
        };

        let mut info = *info;
        if let Some(clip_rect) = clip {
            info.clip_rect.origin.x = clip_rect.origin.x;
            info.clip_rect.origin.y = clip_rect.origin.y;
            info.clip_rect.size = wr_translate_layout_size(clip_rect.size);
        }

        builder.push_text(
            &info,
            &wr_translate_layouted_glyphs(glyphs),
            wr_translate_font_instance_key(font_instance_key),
            wr_translate_color_u(color).into(),
            glyph_options.map(wr_translate_glyph_options),
        );
    }
}

mod background {

    use webrender::api::{
        DisplayListBuilder as WrDisplayListBuilder,
        LayoutPrimitiveInfo as WrLayoutPrimitiveInfo,
        LayoutSize as WrLayoutSize,
        LayoutRect as WrLayoutRect,
        GradientStop as WrGradientStop,
    };
    use azul_css::{
        StyleBackgroundSize, StyleBackgroundPosition, StyleBackgroundRepeat,
        RadialGradient, LinearGradient, ColorU, LayoutSize, LayoutPoint,
    };
    use azul_core::{
        app_resources::ImageInfo,
        display_list::RectBackground,
    };
    use super::image;

    struct Ratio {
        width: f32,
        height: f32,
    }

    #[inline]
    pub(in super) fn push_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrLayoutPrimitiveInfo,
        background: RectBackground,
        background_size: Option<StyleBackgroundSize>,
        background_position: Option<StyleBackgroundPosition>,
        background_repeat: Option<StyleBackgroundRepeat>,
    ) {
        use azul_core::display_list::RectBackground::*;

        let content_size = background.get_content_size();

        match background {
            RadialGradient(rg)  => push_radial_gradient_background(builder, info, rg, background_position, background_size, background_repeat, content_size),
            LinearGradient(g)   => push_linear_gradient_background(builder, info, g, background_position, background_size, background_repeat, content_size),
            Image(image_info)   => push_image_background(builder, info, image_info, background_position, background_size, background_repeat, content_size),
            Color(col)          => push_color_background(builder, info, col, background_position, background_size, background_repeat, content_size),
        }
    }

    fn push_radial_gradient_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrLayoutPrimitiveInfo,
        radial_gradient: RadialGradient,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        background_repeat: Option<StyleBackgroundRepeat>,
        content_size: Option<(f32, f32)>,
    ) {
        use azul_css::Shape;
        use super::{wr_translate_color_u, wr_translate_layout_size, wr_translate_extend_mode};

        let background_position = background_position.unwrap_or_default();
        let _background_repeat = background_repeat.unwrap_or_default();
        let background_size = calculate_background_size(info, background_size, content_size);
        let offset = calculate_background_position(info, background_position, background_size);

        let mut offset_info = *info;
        offset_info.rect.origin.x += offset.x;
        offset_info.rect.origin.y += offset.y;

        let stops: Vec<WrGradientStop> = radial_gradient.stops.iter().map(|gradient_pre|
            WrGradientStop {
                offset: gradient_pre.offset.unwrap().get(),
                color: wr_translate_color_u(gradient_pre.color).into(),
            }).collect();

        let center = info.rect.center();

        // Note: division by 2.0 because it's the radius, not the diameter
        let radius = match radial_gradient.shape {
            Shape::Ellipse => WrLayoutSize::new(background_size.width / 2.0, background_size.height / 2.0),
            Shape::Circle => {
                let largest_bound_size = background_size.width.max(background_size.height);
                WrLayoutSize::new(largest_bound_size / 2.0, largest_bound_size / 2.0)
            },
        };

        let gradient = builder.create_radial_gradient(center, radius, stops, wr_translate_extend_mode(radial_gradient.extend_mode));

        builder.push_radial_gradient(&offset_info, gradient, wr_translate_layout_size(background_size), WrLayoutSize::zero());
    }

    fn push_linear_gradient_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrLayoutPrimitiveInfo,
        linear_gradient: LinearGradient,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        background_repeat: Option<StyleBackgroundRepeat>,
        content_size: Option<(f32, f32)>,
    ) {
        use super::{
            wr_translate_color_u, wr_translate_extend_mode,
            wr_translate_layout_size, wr_translate_css_layout_rect,
            wr_translate_layout_point,
        };

        let background_position = background_position.unwrap_or_default();
        let _background_repeat = background_repeat.unwrap_or_default();
        let background_size = calculate_background_size(info, background_size, content_size);
        let offset = calculate_background_position(info, background_position, background_size);

        let mut offset_info = *info;
        offset_info.rect.origin.x += offset.x;
        offset_info.rect.origin.y += offset.y;

        let stops: Vec<WrGradientStop> = linear_gradient.stops.iter().map(|gradient_pre|
            WrGradientStop {
                offset: gradient_pre.offset.unwrap().get() / 100.0,
                color: wr_translate_color_u(gradient_pre.color).into(),
            }).collect();

        let (begin_pt, end_pt) = linear_gradient.direction.to_points(&wr_translate_css_layout_rect(offset_info.rect));
        let gradient = builder.create_gradient(
            wr_translate_layout_point(begin_pt),
            wr_translate_layout_point(end_pt),
            stops,
            wr_translate_extend_mode(linear_gradient.extend_mode),
        );

        builder.push_gradient(&offset_info, gradient, wr_translate_layout_size(background_size), WrLayoutSize::zero());
    }

    fn push_image_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrLayoutPrimitiveInfo,
        image_info: ImageInfo,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        background_repeat: Option<StyleBackgroundRepeat>,
        content_size: Option<(f32, f32)>,
    ) {
        use azul_core::display_list::{AlphaType, ImageRendering};

        let background_position = background_position.unwrap_or_default();
        let background_repeat = background_repeat.unwrap_or_default();
        let background_size = calculate_background_size(info, background_size, content_size);
        let background_position = calculate_background_position(info, background_position, background_size);
        let background_repeat_info = get_background_repeat_info(info, background_repeat, background_size);

        // TODO: customize this for image backgrounds?
        let alpha_type = AlphaType::PremultipliedAlpha;
        let image_rendering = ImageRendering::Auto;
        let background_color = ColorU { r: 0, g: 0, b: 0, a: 255 };

        image::push_image(builder, &background_repeat_info, background_size, background_position, image_info.key, alpha_type, image_rendering, background_color);
    }

    fn push_color_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrLayoutPrimitiveInfo,
        color: ColorU,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        background_repeat: Option<StyleBackgroundRepeat>,
        content_size: Option<(f32, f32)>,
    ) {
        use super::wr_translate_color_u;

        let background_position = background_position.unwrap_or_default();
        let _background_repeat = background_repeat.unwrap_or_default();
        let background_size = calculate_background_size(info, background_size, content_size);
        let offset = calculate_background_position(info, background_position, background_size);

        let mut offset_info = *info;
        offset_info.rect.origin.x += offset.x;
        offset_info.rect.origin.y += offset.y;
        offset_info.rect.size.width = background_size.width;
        offset_info.rect.size.height = background_size.height;

        builder.push_rect(&offset_info, wr_translate_color_u(color).into());
    }

    fn get_background_repeat_info(
        info: &WrLayoutPrimitiveInfo,
        background_repeat: StyleBackgroundRepeat,
        background_size: LayoutSize,
    ) -> WrLayoutPrimitiveInfo {

        use azul_css::StyleBackgroundRepeat::*;

        match background_repeat {
            NoRepeat => WrLayoutPrimitiveInfo::with_clip_rect(
                info.rect,
                WrLayoutRect::new(
                    info.rect.origin,
                    WrLayoutSize::new(background_size.width, background_size.height),
                ),
            ),
            Repeat => *info,
            RepeatX => WrLayoutPrimitiveInfo::with_clip_rect(
                info.rect,
                WrLayoutRect::new(
                    info.rect.origin,
                    WrLayoutSize::new(info.rect.size.width, background_size.height),
                ),
            ),
            RepeatY => WrLayoutPrimitiveInfo::with_clip_rect(
                info.rect,
                WrLayoutRect::new(
                    info.rect.origin,
                    WrLayoutSize::new(background_size.width, info.rect.size.height),
                ),
            ),
        }
    }

    /// Transform a background size such as "cover" or "contain" into actual pixels
    fn calculate_background_size(
        info: &WrLayoutPrimitiveInfo,
        bg_size: Option<StyleBackgroundSize>,
        content_size: Option<(f32, f32)>,
    ) -> LayoutSize {

        let content_size = content_size.unwrap_or((info.rect.size.width, info.rect.size.height));

        let bg_size = match bg_size {
            None => return LayoutSize::new(content_size.0 as f32, content_size.1 as f32),
            Some(s) => s,
        };

        let content_aspect_ratio = Ratio {
            width: info.rect.size.width / content_size.0 as f32,
            height: info.rect.size.height / content_size.1 as f32,
        };

        let ratio = match bg_size {
            StyleBackgroundSize::ExactSize(w, h) => {
                let w = w.to_pixels(info.rect.size.width);
                let h = h.to_pixels(info.rect.size.height);
                w.min(h)
            },
            StyleBackgroundSize::Contain => content_aspect_ratio.width.min(content_aspect_ratio.height),
            StyleBackgroundSize::Cover => content_aspect_ratio.width.max(content_aspect_ratio.height),
        };

        LayoutSize::new(content_size.0 as f32 * ratio, content_size.1 as f32 * ratio)
    }

    /// Transforma background-position attribute into pixel coordinates
    fn calculate_background_position(
        info: &WrLayoutPrimitiveInfo,
        background_position: StyleBackgroundPosition,
        background_size: LayoutSize,
    ) -> LayoutPoint {

        use azul_css::BackgroundPositionVertical;
        use azul_css::BackgroundPositionHorizontal;

        let width = info.rect.size.width;
        let height = info.rect.size.height;

        let horizontal_offset = match background_position.horizontal {
            BackgroundPositionHorizontal::Right => 0.0,
            BackgroundPositionHorizontal::Center => (width - background_size.width) / 2.0,
            BackgroundPositionHorizontal::Left => (width - background_size.width),
            BackgroundPositionHorizontal::Exact(e) => e.to_pixels(width),
        };

        let vertical_offset = match background_position.vertical {
            BackgroundPositionVertical::Top => 0.0,
            BackgroundPositionVertical::Center => (height - background_size.height) / 2.0,
            BackgroundPositionVertical::Bottom => (height - background_size.height),
            BackgroundPositionVertical::Exact(e) => e.to_pixels(height),
        };

        LayoutPoint { x: horizontal_offset, y: vertical_offset }
    }
}

mod image {

    use webrender::api::{
        DisplayListBuilder as WrDisplayListBuilder,
        LayoutPrimitiveInfo as WrLayoutPrimitiveInfo,
    };
    use azul_css::{LayoutPoint, LayoutSize, ColorU};
    use azul_core::{
        app_resources::ImageKey,
        display_list::{AlphaType, ImageRendering}
    };

    #[inline]
    pub(in super) fn push_image(
        builder: &mut WrDisplayListBuilder,
        info: &WrLayoutPrimitiveInfo,
        size: LayoutSize,
        offset: LayoutPoint,
        image_key: ImageKey,
        alpha_type: AlphaType,
        image_rendering: ImageRendering,
        background_color: ColorU,
    ) {
        use super::{
            wr_translate_image_rendering, wr_translate_alpha_type,
            wr_translate_color_u, wr_translate_image_key, wr_translate_layout_size,
        };
        use webrender::api::LayoutSize as WrLayoutSize;

        let mut offset_info = *info;
        offset_info.rect.origin.x += offset.x;
        offset_info.rect.origin.y += offset.y;

        let tile_spacing = WrLayoutSize::zero();

        builder.push_image(
            &offset_info,
            wr_translate_layout_size(size),
            tile_spacing,
            wr_translate_image_rendering(image_rendering),
            wr_translate_alpha_type(alpha_type),
            wr_translate_image_key(image_key),
            wr_translate_color_u(background_color).into(),
        );
    }
}

mod box_shadow {

    use azul_css::{BoxShadowClipMode, LayoutRect, ColorF, BoxShadowPreDisplayItem};
    use azul_core::{
        display_list::{StyleBoxShadow, StyleBorderRadius},
    };
    use webrender::api::{
        LayoutPrimitiveInfo as WrLayoutPrimitiveInfo,
        DisplayListBuilder as WrDisplayListBuilder
    };

    enum ShouldPushShadow {
        OneShadow,
        TwoShadows,
        AllShadows,
    }

    /// WARNING: For "inset" shadows, you must push a clip ID first, otherwise the
    /// shadow will not show up.
    ///
    /// To prevent a shadow from being pushed twice, you have to annotate the clip
    /// mode for this - outset or inset.
    #[inline]
    pub(in super) fn push_box_shadow(
        builder: &mut WrDisplayListBuilder,
        bounds: LayoutRect,
        shadow_type: BoxShadowClipMode,
        box_shadow: StyleBoxShadow,
        border_radius: StyleBorderRadius,
    ) {
        use self::ShouldPushShadow::*;
        use azul_css::CssPropertyValue;

        let StyleBoxShadow { top, left, bottom, right } = &box_shadow;

        fn translate_shadow_side(input: &Option<CssPropertyValue<BoxShadowPreDisplayItem>>) -> Option<BoxShadowPreDisplayItem> {
            input.and_then(|prop| prop.get_property().cloned())
        }

        let (top, left, bottom, right) = (
            translate_shadow_side(top),
            translate_shadow_side(left),
            translate_shadow_side(bottom),
            translate_shadow_side(right),
        );

        let what_shadow_to_push = match [top, left, bottom, right].iter().filter(|x| x.is_some()).count() {
            1 => OneShadow,
            2 => TwoShadows,
            4 => AllShadows,
            _ => return,
        };

        match what_shadow_to_push {
            OneShadow => {
                let current_shadow = match (top, left, bottom, right) {
                     | (Some(shadow), None, None, None)
                     | (None, Some(shadow), None, None)
                     | (None, None, Some(shadow), None)
                     | (None, None, None, Some(shadow))
                     => shadow,
                     _ => return, // reachable, but invalid box-shadow
                };

                push_single_box_shadow_edge(
                    builder, &current_shadow, bounds, border_radius, shadow_type,
                    &top, &bottom, &left, &right
                );
            },
            // Two shadows in opposite directions:
            //
            // box-shadow-top: 0px 0px 5px red;
            // box-shadow-bottom: 0px 0px 5px blue;
            TwoShadows => {
                match (top, left, bottom, right) {
                    // top + bottom box-shadow pair
                    (Some(t), None, Some(b), None) => {
                        push_single_box_shadow_edge(
                            builder, &t, bounds, border_radius, shadow_type,
                            &top, &None, &None, &None
                        );
                        push_single_box_shadow_edge(
                            builder, &b, bounds, border_radius, shadow_type,
                            &None, &bottom, &None, &None
                        );
                    },
                    // left + right box-shadow pair
                    (None, Some(l), None, Some(r)) => {
                        push_single_box_shadow_edge(
                            builder, &l, bounds, border_radius, shadow_type,
                            &None, &None, &left, &None
                        );
                        push_single_box_shadow_edge(
                            builder, &r, bounds, border_radius, shadow_type,
                            &None, &None, &None, &right
                        );
                    }
                    _ => return, // reachable, but invalid
                }
            },
            AllShadows => {

                // Assumes that all box shadows are the same, so just use the top shadow
                let top_shadow = top.unwrap();
                let clip_rect = get_clip_rect(&top_shadow, bounds);

                push_box_shadow_inner(
                    builder,
                    top_shadow,
                    border_radius,
                    bounds,
                    clip_rect,
                    shadow_type,
                );
            }
        }
    }

    #[inline]
    #[allow(clippy::collapsible_if)]
    fn push_single_box_shadow_edge(
            builder: &mut WrDisplayListBuilder,
            current_shadow: &BoxShadowPreDisplayItem,
            bounds: LayoutRect,
            border_radius: StyleBorderRadius,
            shadow_type: BoxShadowClipMode,
            top: &Option<BoxShadowPreDisplayItem>,
            bottom: &Option<BoxShadowPreDisplayItem>,
            left: &Option<BoxShadowPreDisplayItem>,
            right: &Option<BoxShadowPreDisplayItem>,
    ) {
        let is_inset_shadow = current_shadow.clip_mode == BoxShadowClipMode::Inset;
        let origin_displace = (current_shadow.spread_radius.to_pixels() + current_shadow.blur_radius.to_pixels()) * 2.0;

        let mut shadow_bounds = bounds;
        let mut clip_rect = bounds;

        if is_inset_shadow {
            // If the shadow is inset, we adjust the clip rect to be
            // exactly the amount of the shadow
            if let Some(_top) = top {
                clip_rect.size.height = origin_displace;
                shadow_bounds.size.width += origin_displace;
                shadow_bounds.origin.x -= origin_displace / 2.0;
            } else if let Some(_bottom) = bottom {
                clip_rect.size.height = origin_displace;
                clip_rect.origin.y += bounds.size.height - origin_displace;
                shadow_bounds.size.width += origin_displace;
                shadow_bounds.origin.x -= origin_displace / 2.0;
            } else if let Some(_left) = left {
                clip_rect.size.width = origin_displace;
                shadow_bounds.size.height += origin_displace;
                shadow_bounds.origin.y -= origin_displace / 2.0;
            } else if let Some(_right) = right {
                clip_rect.size.width = origin_displace;
                clip_rect.origin.x += bounds.size.width - origin_displace;
                shadow_bounds.size.height += origin_displace;
                shadow_bounds.origin.y -= origin_displace / 2.0;
            }
        } else {
            if let Some(_top) = top {
                clip_rect.size.height = origin_displace;
                clip_rect.origin.y -= origin_displace;
                shadow_bounds.size.width += origin_displace;
                shadow_bounds.origin.x -= origin_displace / 2.0;
            } else if let Some(_bottom) = bottom {
                clip_rect.size.height = origin_displace;
                clip_rect.origin.y += bounds.size.height;
                shadow_bounds.size.width += origin_displace;
                shadow_bounds.origin.x -= origin_displace / 2.0;
            } else if let Some(_left) = left {
                clip_rect.size.width = origin_displace;
                clip_rect.origin.x -= origin_displace;
                shadow_bounds.size.height += origin_displace;
                shadow_bounds.origin.y -= origin_displace / 2.0;
            } else if let Some(_right) = right {
                clip_rect.size.width = origin_displace;
                clip_rect.origin.x += bounds.size.width;
                shadow_bounds.size.height += origin_displace;
                shadow_bounds.origin.y -= origin_displace / 2.0;
            }
        }

        push_box_shadow_inner(
            builder,
            *current_shadow,
            border_radius,
            shadow_bounds,
            clip_rect,
            shadow_type
        );
    }

    #[inline]
    fn push_box_shadow_inner(
        builder: &mut WrDisplayListBuilder,
        pre_shadow: BoxShadowPreDisplayItem,
        border_radius: StyleBorderRadius,
        bounds: LayoutRect,
        clip_rect: LayoutRect,
        shadow_type: BoxShadowClipMode,
    ) {
        use webrender::api::{LayoutRect, LayoutPoint, LayoutVector2D};
        use super::{
            wr_translate_color_f, wr_translate_border_radius,
            wr_translate_box_shadow_clip_mode, wr_translate_layout_rect,
        };

        // The pre_shadow is missing the StyleBorderRadius & LayoutRect
        if pre_shadow.clip_mode != shadow_type {
            return;
        }

        let full_screen_rect = LayoutRect::new(LayoutPoint::zero(), builder.content_size());;

        // Prevent shadows that are larger than the full screen
        let clip_rect = wr_translate_layout_rect(clip_rect);
        let clip_rect = clip_rect.intersection(&full_screen_rect).unwrap_or(clip_rect);

        let info = WrLayoutPrimitiveInfo::with_clip_rect(LayoutRect::zero(), clip_rect);

        builder.push_box_shadow(
            &info,
            wr_translate_layout_rect(bounds),
            LayoutVector2D::new(pre_shadow.offset[0].to_pixels(), pre_shadow.offset[1].to_pixels()),
            wr_translate_color_f(apply_gamma(pre_shadow.color.into())),
            pre_shadow.blur_radius.to_pixels(),
            pre_shadow.spread_radius.to_pixels(),
            wr_translate_border_radius(border_radius, bounds.size),
            wr_translate_box_shadow_clip_mode(pre_shadow.clip_mode)
        );
    }

    // Apply a gamma of 2.2 to the original value
    //
    // NOTE: strangely box-shadow is the only thing that needs to be gamma-corrected...
    #[inline(always)]
    fn apply_gamma(color: ColorF) -> ColorF {

        const GAMMA: f32 = 2.2;
        const GAMMA_F: f32 = 1.0 / GAMMA;

        ColorF {
            r: color.r.powf(GAMMA_F),
            g: color.g.powf(GAMMA_F),
            b: color.b.powf(GAMMA_F),
            a: color.a,
        }
    }

    fn get_clip_rect(pre_shadow: &BoxShadowPreDisplayItem, bounds: LayoutRect) -> LayoutRect {
        if pre_shadow.clip_mode == BoxShadowClipMode::Inset {
            // inset shadows do not work like outset shadows
            // for inset shadows, you have to push a clip ID first, so that they are
            // clipped to the bounds -we trust that the calling function knows to do this
            bounds
        } else {
            // calculate the maximum extent of the outset shadow
            let mut clip_rect = bounds;

            let origin_displace = (pre_shadow.spread_radius.to_pixels() + pre_shadow.blur_radius.to_pixels()) * 2.0;
            clip_rect.origin.x = clip_rect.origin.x - pre_shadow.offset[0].to_pixels() - origin_displace;
            clip_rect.origin.y = clip_rect.origin.y - pre_shadow.offset[1].to_pixels() - origin_displace;

            clip_rect.size.height = clip_rect.size.height + (origin_displace * 2.0);
            clip_rect.size.width = clip_rect.size.width + (origin_displace * 2.0);
            clip_rect
        }
    }

}

mod border {

    use webrender::api::{
        DisplayListBuilder as WrDisplayListBuilder,
        LayoutSideOffsets as WrLayoutSideOffsets,
        BorderDetails as WrBorderDetails,
        LayoutPrimitiveInfo as WrLayoutPrimitiveInfo,
        BorderStyle as WrBorderStyle,
        BorderSide as WrBorderSide,
    };
    use azul_css::{
        LayoutSize, BorderStyle, BorderStyleNoNone, CssPropertyValue, PixelValue
    };
    use azul_core::{
        display_list::{StyleBorderRadius, StyleBorderWidths, StyleBorderColors, StyleBorderStyles},
    };

    pub(in super) fn is_zero_border_radius(border_radius: &StyleBorderRadius) -> bool {
        border_radius.top_left.is_none() &&
        border_radius.top_right.is_none() &&
        border_radius.bottom_left.is_none() &&
        border_radius.bottom_right.is_none()
    }

    pub(in super) fn push_border(
        builder: &mut WrDisplayListBuilder,
        info: &WrLayoutPrimitiveInfo,
        radii: StyleBorderRadius,
        widths: StyleBorderWidths,
        colors: StyleBorderColors,
        styles: StyleBorderStyles,
    ) {
        let rect_size = LayoutSize::new(info.rect.size.width, info.rect.size.height);
        if let Some((border_widths, border_details)) = get_webrender_border(rect_size, radii, widths, colors, styles) {
            builder.push_border(info, border_widths, border_details);
        }
    }

    /// Returns the merged offsets and details for the top, left,
    /// right and bottom styles - necessary, so we can combine `border-top`,
    /// `border-left`, etc. into one border
    fn get_webrender_border(
        rect_size: LayoutSize,
        radii: StyleBorderRadius,
        widths: StyleBorderWidths,
        colors: StyleBorderColors,
        styles: StyleBorderStyles,
    ) -> Option<(WrLayoutSideOffsets, WrBorderDetails)> {

        use super::{wr_translate_color_u, wr_translate_border_radius};
        use webrender::api::{
            NormalBorder as WrNormalBorder,
            BorderRadius as WrBorderRadius,
        };

        let (width_top, width_right, width_bottom, width_left) = (
            widths.top.map(|w| w.map_property(|w| w.0)).and_then(CssPropertyValue::get_property_or_default),
            widths.right.map(|w| w.map_property(|w| w.0)).and_then(CssPropertyValue::get_property_or_default),
            widths.bottom.map(|w| w.map_property(|w| w.0)).and_then(CssPropertyValue::get_property_or_default),
            widths.left.map(|w| w.map_property(|w| w.0)).and_then(CssPropertyValue::get_property_or_default),
        );

        let (style_top, style_right, style_bottom, style_left) = (
            get_border_style_normalized(styles.top.map(|s| s.map_property(|s| s.0))),
            get_border_style_normalized(styles.right.map(|s| s.map_property(|s| s.0))),
            get_border_style_normalized(styles.bottom.map(|s| s.map_property(|s| s.0))),
            get_border_style_normalized(styles.left.map(|s| s.map_property(|s| s.0))),
        );

        let no_border_style =
            style_top.is_none() &&
            style_right.is_none() &&
            style_bottom.is_none() &&
            style_left.is_none();

        let no_border_width =
            width_top.is_none() &&
            width_right.is_none() &&
            width_bottom.is_none() &&
            width_left.is_none();

        // border has all borders set to border: none; or all border-widths set to none
        if no_border_style || no_border_width {
            return None;
        }

        let has_no_border_radius = radii.top_left.is_none() &&
                                   radii.top_right.is_none() &&
                                   radii.bottom_left.is_none() &&
                                   radii.bottom_right.is_none();

        let (color_top, color_right, color_bottom, color_left) = (
           colors.top.and_then(|ct| ct.get_property_or_default()).unwrap_or_default(),
           colors.right.and_then(|cr| cr.get_property_or_default()).unwrap_or_default(),
           colors.bottom.and_then(|cb| cb.get_property_or_default()).unwrap_or_default(),
           colors.left.and_then(|cl| cl.get_property_or_default()).unwrap_or_default(),
        );

        let border_widths = WrLayoutSideOffsets::new(
            width_top.map(|v| v.to_pixels(rect_size.height)).unwrap_or(0.0),
            width_right.map(|v| v.to_pixels(rect_size.width)).unwrap_or(0.0),
            width_bottom.map(|v| v.to_pixels(rect_size.height)).unwrap_or(0.0),
            width_left.map(|v| v.to_pixels(rect_size.width)).unwrap_or(0.0),
        );

        let border_details = WrBorderDetails::Normal(WrNormalBorder {
            top:    WrBorderSide { color: wr_translate_color_u(color_top.0).into(), style: translate_wr_border(style_top, width_top) },
            left:   WrBorderSide { color: wr_translate_color_u(color_left.0).into(), style: translate_wr_border(style_left, width_left) },
            right:  WrBorderSide { color: wr_translate_color_u(color_right.0).into(), style: translate_wr_border(style_right, width_right) },
            bottom: WrBorderSide { color: wr_translate_color_u(color_bottom.0).into(), style: translate_wr_border(style_bottom, width_bottom) },
            radius: if has_no_border_radius { WrBorderRadius::zero() } else { wr_translate_border_radius(radii, rect_size) },
            do_aa: !has_no_border_radius,
        });

        Some((border_widths, border_details))
    }

    #[inline]
    fn get_border_style_normalized(style: Option<CssPropertyValue<BorderStyle>>) -> Option<BorderStyleNoNone> {
        match style {
            None => None,
            Some(s) => s.get_property_or_default().and_then(|prop| prop.normalize_border()),
        }
    }

    #[inline]
    fn translate_wr_border(style: Option<BorderStyleNoNone>, border_width: Option<PixelValue>) -> WrBorderStyle {
        if border_width.is_none() {
            WrBorderStyle::None
        } else {
            match style {
                None => WrBorderStyle::None,
                Some(BorderStyleNoNone::Solid) => WrBorderStyle::Solid,
                Some(BorderStyleNoNone::Double) => WrBorderStyle::Double,
                Some(BorderStyleNoNone::Dotted) => WrBorderStyle::Dotted,
                Some(BorderStyleNoNone::Dashed) => WrBorderStyle::Dashed,
                Some(BorderStyleNoNone::Hidden) => WrBorderStyle::Hidden,
                Some(BorderStyleNoNone::Groove) => WrBorderStyle::Groove,
                Some(BorderStyleNoNone::Ridge) => WrBorderStyle::Ridge,
                Some(BorderStyleNoNone::Inset) => WrBorderStyle::Inset,
                Some(BorderStyleNoNone::Outset) => WrBorderStyle::Outset,
            }
        }
    }
}
