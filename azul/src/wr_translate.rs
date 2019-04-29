//! Type translation functions (from azul-css to webrender types)
//!
//! The reason for doing this is so that azul-css doesn't depend on webrender or euclid
//! (since webrender is a huge dependency) just to use the types. Only if you depend on
//! azul, you have to depend on webrender.

use webrender::api::{
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
    NormalBorder as WrNormalBorder,
    BorderDetails as WrBorderDetails,
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
};
use azul_core::{
    callbacks::{HidpiAdjustedBounds, HitTestItem, PipelineId},
    window::{LogicalPosition, LogicalSize, MouseCursorType, VirtualKeyCode},
    app_resources::{
        FontKey, Au, FontInstanceKey, ImageKey,
        IdNamespace, RawImageFormat as ImageFormat, ImageDescriptor
    },
    display_list::{
        CachedDisplayList, GlyphInstance, DisplayListScrollFrame,
        DisplayListFrame, DisplayListRect, DisplayListRectContent, DisplayListMsg,
    },
};
use azul_css::{
    ColorU as CssColorU,
    ColorF as CssColorF,
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
use glium::glutin::{VirtualKeyCode as WinitVirtualKeyCode, MouseCursor as WinitCursorType};

#[inline(always)]
pub(crate) fn wr_translate_hittest_item(input: WrHitTestItem) -> HitTestItem {
    HitTestItem {
        pipeline: PipelineId(input.pipeline.0, input.pipeline.1),
        tag: input.tag,
        point_in_viewport: LogicalPosition::new(input.point_in_viewport.x, input.point_in_viewport.y),
        point_relative_to_item: LogicalPosition::new(input.point_relative_to_item.x, input.point_relative_to_item.y),
    }
}

#[inline(always)]
pub(crate) fn hidpi_rect_from_bounds(bounds: WrLayoutRect, hidpi_factor: f32, winit_hidpi_factor: f32) -> HidpiAdjustedBounds {
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
pub fn wr_translate_border_radius(input: CssBorderRadius) -> WrBorderRadius {
    use webrender::api::LayoutSize;
    let CssBorderRadius { top_left, top_right, bottom_left, bottom_right } = input;
    WrBorderRadius {
        top_left: LayoutSize::new(top_left.width.to_pixels(), top_left.height.to_pixels()),
        top_right: LayoutSize::new(top_right.width.to_pixels(), top_right.height.to_pixels()),
        bottom_left: LayoutSize::new(bottom_left.width.to_pixels(), bottom_left.height.to_pixels()),
        bottom_right: LayoutSize::new(bottom_right.width.to_pixels(), bottom_right.height.to_pixels()),
    }
}

#[inline]
pub fn wr_translate_border_side(input: CssBorderSide) -> WrBorderSide {
    WrBorderSide {
        color: wr_translate_color_u(input.color).into(),
        style: wr_translate_border_style(input.style),
    }
}

#[inline]
pub fn wr_translate_normal_border(input: CssNormalBorder) -> WrNormalBorder {

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

#[inline]
pub fn wr_translate_layout_point(input: CssLayoutPoint) -> WrLayoutPoint {
    WrLayoutPoint::new(input.x, input.y)
}

// NOTE: Reverse direction: Translate from webrender::LayoutRect to css::LayoutRect
#[inline(always)]
pub const fn wr_translate_css_layout_rect(input: WrLayoutRect) -> CssLayoutRect {
    CssLayoutRect {
        origin: CssLayoutPoint { x: input.origin.x, y: input.origin.y },
        size: CssLayoutSize { width: input.size.width, height: input.size.height },
    }
}

// NOTE: Reverse direction: Translate from webrender::LayoutRect to css::LayoutRect
#[inline]
pub fn wr_translate_border_details(input: CssBorderDetails) -> WrBorderDetails {
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
fn wr_translate_layout_size(input: LogicalSize) -> WrLayoutSize {
    WrLayoutSize::new(input.width, input.height)
}

#[inline]
fn wr_translate_layout_position(input: LogicalPosition) -> WrLayoutPoint {
    WrLayoutPoint::new(input.x, input.y)
}

#[inline]
fn wr_translate_layout_rect(input: DisplayListRect) -> WrLayoutRect {
    WrLayoutRect::new(wr_translate_layout_position(input.origin), wr_translate_layout_size(input.size))
}

pub(crate) fn wr_translate_display_list(input: CachedDisplayList) -> WrBuiltDisplayList {
    let mut builder = WrDisplayListBuilder::new(
        wr_translate_pipeline_id(input.pipeline_id),
        wr_translate_layout_size(input.root.get_size())
    );
    push_display_list_msg(input.root, &mut builder);
    builder.finalize().2
}

fn push_display_list_msg(msg: DisplayListMsg, builder: &mut WrDisplayListBuilder) {
    use azul_core::display_list::DisplayListMsg::*;
    match msg {
        Frame(f) => push_frame(f, builder),
        ScrollFrame(sf) => push_scroll_frame(sf, builder),
    }
}

fn push_frame(frame: DisplayListFrame, builder: &mut WrDisplayListBuilder) {

    use webrender::api::LayoutPrimitiveInfo;

    //  if let Some(clip_rect) = frame.clip_rect {
    //
    //      let clip = get_clip_region(solved_rect.bounds, &styled_node)
    //          .unwrap_or(ComplexClipRegion::new(solved_rect.bounds, BorderRadius::zero(), ClipMode::Clip));
    //      let clip_id = builder.define_clip(solved_rect.bounds, vec![clip], /* image_mask: */ None);
    //      builder.push_clip_id(clip_id);
    //
    //  }

    let wr_rect = wr_translate_layout_rect(frame.rect);

    let info = LayoutPrimitiveInfo {
        rect: wr_rect,
        clip_rect: wr_rect,
        is_backface_visible: false,
        tag: frame.tag,
    };

    for item in frame.content {
        push_display_list_content(item, builder);
    }

    for child in frame.children {
        push_display_list_msg(child, builder);
    }

    // if frame.clip_rect.is_some() {
    //     builder.pop_clip_id();
    // }
}

fn push_scroll_frame(scroll_frame: DisplayListScrollFrame, builder: &mut WrDisplayListBuilder) {

    // let DisplayListScrollFrame {
    //     pub scroll_position: LogicalPosition,
    //     pub scroll_frame_size: LogicalSize,
    //     pub content_size: LogicalSize,
    //     pub overlay_scrollbars: bool,
    //     pub rect: DisplayListRect,
    //     pub tag: Option<ItemTag>,
    //     pub content: Vec<DisplayListRectContent>,
    //     pub children: Vec<DisplayListMsg>,
    // }
}

fn push_display_list_content(content: DisplayListRectContent, builder: &mut WrDisplayListBuilder) {

}