//! Type translation functions (from azul-css to webrender types)
//!
//! The reason for doing this is so that azul-core doesn't depend on webrender
//! (since webrender is a huge dependency) just to use the types. Only if you depend on
//! azul (not azul-core), you have to depend on webrender.


use webrender::api::{
    units::{
        LayoutSideOffsets as WrLayoutSideOffsets,
        LayoutSize as WrLayoutSize,
        LayoutPoint as WrLayoutPoint,
        LayoutRect as WrLayoutRect,
        ImageDirtyRect as WrImageDirtyRect,
    },
    CommonItemProperties as WrCommonItemProperties,
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
    ImageFormat as WrImageFormat,
    ImageDescriptor as WrImageDescriptor,
    GlyphInstance as WrGlyphInstance,
    BuiltDisplayList as WrBuiltDisplayList,
    DisplayListBuilder as WrDisplayListBuilder,
    GlyphOptions as WrGlyphOptions,
    AlphaType as WrAlphaType,
    FontInstanceFlags as WrFontInstanceFlags,
    FontRenderMode as WrFontRenderMode,
    ImageRendering as WrImageRendering,
    ExternalScrollId as WrExternalScrollId,
    SpaceAndClipInfo as WrSpaceAndClipInfo,
    ResourceUpdate as WrResourceUpdate,
    AddFont as WrAddFont,
    AddImage as WrAddImage,
    ImageData as WrImageData,
    ExternalImageData as WrExternalImageData,
    ExternalImageId as WrExternalImageId,
    ExternalImageType as WrExternalImageType,
    TextureTarget as WrTextureTarget,
    UpdateImage as WrUpdateImage,
    Epoch as WrEpoch,
    AddFontInstance as WrAddFontInstance,
    FontVariation as WrFontVariation,
    FontInstanceOptions as WrFontInstanceOptions,
    FontInstancePlatformOptions as WrFontInstancePlatformOptions,
    SyntheticItalics as WrSyntheticItalics,
};

use crate::errors::WrLayoutPrimitiveInfo;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use webrender::api::{
    FontLCDFilter as WrFontLCDFilter,
    FontHinting as WrFontHinting,
};

use azul_core::{
    callbacks::{HitTestItem, PipelineId},
    app_resources::{
        FontKey, Au, FontInstanceKey, ImageKey,
        IdNamespace, RawImageFormat as ImageFormat, ImageDescriptor,
        FontInstanceFlags, FontRenderMode, GlyphOptions, ResourceUpdate,
        AddFont, AddImage, ImageData, ExternalImageData, ExternalImageId,
        ExternalImageType, TextureTarget, UpdateImage, ImageDirtyRect,
        Epoch, AddFontInstance, FontVariation, FontInstanceOptions,
        FontInstancePlatformOptions, FontLCDFilter, FontHinting, SyntheticItalics,
    },
    display_list::{
        CachedDisplayList, GlyphInstance, DisplayListScrollFrame,
        DisplayListFrame, LayoutRectContent, DisplayListMsg,
        AlphaType, ImageRendering, StyleBorderRadius,
    },
    dom::TagId,
    ui_solver::ExternalScrollId,
    window::{LogicalSize, DebugState},
};
use azul_css::{
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
use webrender::Renderer;

pub(crate) mod winit_translate {

    use azul_core::{
        window::{
            LogicalSize, PhysicalSize, LogicalPosition, PhysicalPosition,
            WindowIcon, TaskBarIcon, MouseCursorType, VirtualKeyCode,
        },
    };
    use glutin::{
        event::VirtualKeyCode as WinitVirtualKeyCode,
        dpi::{
            LogicalPosition as WinitLogicalPosition,
            LogicalSize as WinitLogicalSize,
            PhysicalPosition as WinitPhysicalPosition,
            PhysicalSize as WinitPhysicalSize,
        },
        window::{
            CursorIcon as WinitCursorIcon,
            BadIcon as WinitBadIcon,
            Icon as WinitIcon,
        },
    };
    #[cfg(target_os = "linux")]
    use glutin::platform::unix::{
        WaylandTheme as WinitWaylandTheme,
        XWindowType as WinitXWindowType,
    };
    #[cfg(target_os = "linux")]
    use azul_core::window::{WaylandTheme, XWindowType};

    #[inline(always)]
    pub(crate) fn translate_logical_position(input: LogicalPosition) -> WinitLogicalPosition<f64> {
        WinitLogicalPosition::new(input.x as f64, input.y as f64)
    }

    #[inline(always)]
    pub(crate) fn translate_logical_size(input: LogicalSize) -> WinitLogicalSize<f64> {
        WinitLogicalSize::new(input.width as f64, input.height as f64)
    }

    #[inline(always)]
    pub(crate) const fn translate_winit_logical_position(input: WinitLogicalPosition<i32>) -> LogicalPosition {
        LogicalPosition::new(input.x as f32, input.y as f32)
    }

    #[inline(always)]
    pub(crate) const fn translate_winit_logical_size(input: WinitLogicalSize<u32>) -> LogicalSize {
        LogicalSize::new(input.width as f32, input.height as f32)
    }

    #[cfg(target_os = "linux")]
    #[inline(always)]
    pub(crate) fn translate_x_window_type(input: XWindowType) -> WinitXWindowType {
        match input {
            XWindowType::Desktop => WinitXWindowType::Desktop,
            XWindowType::Dock => WinitXWindowType::Dock,
            XWindowType::Toolbar => WinitXWindowType::Toolbar,
            XWindowType::Menu => WinitXWindowType::Menu,
            XWindowType::Utility => WinitXWindowType::Utility,
            XWindowType::Splash => WinitXWindowType::Splash,
            XWindowType::Dialog => WinitXWindowType::Dialog,
            XWindowType::DropdownMenu => WinitXWindowType::DropdownMenu,
            XWindowType::PopupMenu => WinitXWindowType::PopupMenu,
            XWindowType::Tooltip => WinitXWindowType::Tooltip,
            XWindowType::Notification => WinitXWindowType::Notification,
            XWindowType::Combo => WinitXWindowType::Combo,
            XWindowType::Dnd => WinitXWindowType::Dnd,
            XWindowType::Normal => WinitXWindowType::Normal,
        }
    }

    pub(crate) fn translate_physical_position(input: PhysicalPosition) -> WinitPhysicalPosition<f64> {
        WinitPhysicalPosition::new(input.x as f64, input.y as f64)
    }

    pub(crate) fn translate_physical_size(input: PhysicalSize) -> WinitPhysicalSize<f64> {
        WinitPhysicalSize::new(input.width as f64, input.height as f64)
    }

    pub(crate) fn translate_mouse_cursor_type(mouse_cursor_type: MouseCursorType) -> WinitCursorIcon {
        use azul_core::window::MouseCursorType::*;
        match mouse_cursor_type {
            Default => WinitCursorIcon::Default,
            Crosshair => WinitCursorIcon::Crosshair,
            Hand => WinitCursorIcon::Hand,
            Arrow => WinitCursorIcon::Arrow,
            Move => WinitCursorIcon::Move,
            Text => WinitCursorIcon::Text,
            Wait => WinitCursorIcon::Wait,
            Help => WinitCursorIcon::Help,
            Progress => WinitCursorIcon::Progress,
            NotAllowed => WinitCursorIcon::NotAllowed,
            ContextMenu => WinitCursorIcon::ContextMenu,
            Cell => WinitCursorIcon::Cell,
            VerticalText => WinitCursorIcon::VerticalText,
            Alias => WinitCursorIcon::Alias,
            Copy => WinitCursorIcon::Copy,
            NoDrop => WinitCursorIcon::NoDrop,
            Grab => WinitCursorIcon::Grab,
            Grabbing => WinitCursorIcon::Grabbing,
            AllScroll => WinitCursorIcon::AllScroll,
            ZoomIn => WinitCursorIcon::ZoomIn,
            ZoomOut => WinitCursorIcon::ZoomOut,
            EResize => WinitCursorIcon::EResize,
            NResize => WinitCursorIcon::NResize,
            NeResize => WinitCursorIcon::NeResize,
            NwResize => WinitCursorIcon::NwResize,
            SResize => WinitCursorIcon::SResize,
            SeResize => WinitCursorIcon::SeResize,
            SwResize => WinitCursorIcon::SwResize,
            WResize => WinitCursorIcon::WResize,
            EwResize => WinitCursorIcon::EwResize,
            NsResize => WinitCursorIcon::NsResize,
            NeswResize => WinitCursorIcon::NeswResize,
            NwseResize => WinitCursorIcon::NwseResize,
            ColResize => WinitCursorIcon::ColResize,
            RowResize => WinitCursorIcon::RowResize,
        }
    }

    #[inline]
    pub(crate) fn translate_window_icon(input: WindowIcon) -> Result<WinitIcon, WinitBadIcon> {
        match input {
            WindowIcon::Small { rgba_bytes, .. } => WinitIcon::from_rgba(rgba_bytes, 16, 16),
            WindowIcon::Large { rgba_bytes, .. } => WinitIcon::from_rgba(rgba_bytes, 32, 32),
        }
    }

    #[inline]
    pub(crate) fn translate_taskbar_icon(input: TaskBarIcon) -> Result<WinitIcon, WinitBadIcon> {
        WinitIcon::from_rgba(input.rgba_bytes, 256, 256)
    }

    #[cfg(target_os = "linux")]
    #[inline]
    pub(crate) fn translate_wayland_theme(input: WaylandTheme) -> WinitWaylandTheme {
        WinitWaylandTheme {
            primary_active: input.primary_active,
            primary_inactive: input.primary_inactive,
            secondary_active: input.secondary_active,
            secondary_inactive: input.secondary_inactive,
            close_button_hovered: input.close_button_hovered,
            close_button: input.close_button,
            maximize_button_hovered: input.maximize_button_hovered,
            maximize_button: input.maximize_button,
            minimize_button_hovered: input.minimize_button_hovered,
            minimize_button: input.minimize_button,
        }
    }

    #[inline]
    pub(crate) fn translate_cursor_icon(input: MouseCursorType) -> WinitCursorIcon {
        match input {
            MouseCursorType::Default => WinitCursorIcon::Default,
            MouseCursorType::Crosshair => WinitCursorIcon::Crosshair,
            MouseCursorType::Hand => WinitCursorIcon::Hand,
            MouseCursorType::Arrow => WinitCursorIcon::Arrow,
            MouseCursorType::Move => WinitCursorIcon::Move,
            MouseCursorType::Text => WinitCursorIcon::Text,
            MouseCursorType::Wait => WinitCursorIcon::Wait,
            MouseCursorType::Help => WinitCursorIcon::Help,
            MouseCursorType::Progress => WinitCursorIcon::Progress,
            MouseCursorType::NotAllowed => WinitCursorIcon::NotAllowed,
            MouseCursorType::ContextMenu => WinitCursorIcon::ContextMenu,
            MouseCursorType::Cell => WinitCursorIcon::Cell,
            MouseCursorType::VerticalText => WinitCursorIcon::VerticalText,
            MouseCursorType::Alias => WinitCursorIcon::Alias,
            MouseCursorType::Copy => WinitCursorIcon::Copy,
            MouseCursorType::NoDrop => WinitCursorIcon::NoDrop,
            MouseCursorType::Grab => WinitCursorIcon::Grab,
            MouseCursorType::Grabbing => WinitCursorIcon::Grabbing,
            MouseCursorType::AllScroll => WinitCursorIcon::AllScroll,
            MouseCursorType::ZoomIn => WinitCursorIcon::ZoomIn,
            MouseCursorType::ZoomOut => WinitCursorIcon::ZoomOut,
            MouseCursorType::EResize => WinitCursorIcon::EResize,
            MouseCursorType::NResize => WinitCursorIcon::NResize,
            MouseCursorType::NeResize => WinitCursorIcon::NeResize,
            MouseCursorType::NwResize => WinitCursorIcon::NwResize,
            MouseCursorType::SResize => WinitCursorIcon::SResize,
            MouseCursorType::SeResize => WinitCursorIcon::SeResize,
            MouseCursorType::SwResize => WinitCursorIcon::SwResize,
            MouseCursorType::WResize => WinitCursorIcon::WResize,
            MouseCursorType::EwResize => WinitCursorIcon::EwResize,
            MouseCursorType::NsResize => WinitCursorIcon::NsResize,
            MouseCursorType::NeswResize => WinitCursorIcon::NeswResize,
            MouseCursorType::NwseResize => WinitCursorIcon::NwseResize,
            MouseCursorType::ColResize => WinitCursorIcon::ColResize,
            MouseCursorType::RowResize => WinitCursorIcon::RowResize,
        }
    }

    #[inline]
    pub(crate) fn translate_virtual_keycode(input: WinitVirtualKeyCode) -> VirtualKeyCode {
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
            WinitVirtualKeyCode::NumpadAdd => VirtualKeyCode::Add,
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
            WinitVirtualKeyCode::NumpadDecimal => VirtualKeyCode::Decimal,
            WinitVirtualKeyCode::NumpadDivide => VirtualKeyCode::Divide,
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
            WinitVirtualKeyCode::NumpadMultiply => VirtualKeyCode::Multiply,
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
            WinitVirtualKeyCode::NumpadSubtract => VirtualKeyCode::Subtract,
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
            WinitVirtualKeyCode::Asterisk => VirtualKeyCode::Asterisk,
            WinitVirtualKeyCode::Plus => VirtualKeyCode::Plus
        }
    }
}

#[inline]
fn wr_translate_layouted_glyphs(input: Vec<GlyphInstance>) -> Vec<WrGlyphInstance> {
    input.into_iter().map(|glyph| WrGlyphInstance {
        index: glyph.index,
        point: WrLayoutPoint::new(glyph.point.x, glyph.point.y),
    }).collect()
}

#[inline(always)]
pub(crate) const fn wr_translate_epoch(epoch: Epoch) -> WrEpoch {
    WrEpoch(epoch.0)
}

#[inline(always)]
const fn wr_translate_tag_id(input: crate::dom::TagId) -> (u64, u16) {
    (input.0, 0)
}

pub(crate) fn wr_translate_hittest_item(input: WrHitTestItem) -> HitTestItem {
    HitTestItem {
        pipeline: PipelineId(input.pipeline.0, input.pipeline.1),
        tag: TagId(input.tag.0),
        point_in_viewport: CssLayoutPoint::new(input.point_in_viewport.x, input.point_in_viewport.y),
        point_relative_to_item: CssLayoutPoint::new(input.point_relative_to_item.x, input.point_relative_to_item.y),
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

#[inline(always)]
pub(crate) const fn translate_epoch_wr(epoch: WrEpoch) -> Epoch {
    Epoch(epoch.0)
}

#[inline]
pub(crate) fn translate_image_descriptor_wr(descriptor: WrImageDescriptor) -> ImageDescriptor {
    ImageDescriptor {
        format: translate_image_format_wr(descriptor.format),
        dimensions: (descriptor.size.width as usize, descriptor.size.height as usize),
        stride: descriptor.stride,
        offset: descriptor.offset,
        is_opaque: descriptor.is_opaque(),
        allow_mipmaps: descriptor.allow_mipmaps(),
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
        WrImageFormat::RG16 => ImageFormat::RG16
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
pub(crate) fn wr_translate_logical_size(logical_size: LogicalSize) -> WrLayoutSize {
    WrLayoutSize::new(logical_size.width, logical_size.height)
}

#[inline(always)]
pub(crate) const fn translate_logical_size_to_css_layout_size(logical_size: LogicalSize) -> CssLayoutSize {
    CssLayoutSize::new(logical_size.width, logical_size.height)
}

#[inline]
pub(crate) fn wr_translate_image_descriptor(descriptor: ImageDescriptor) -> WrImageDescriptor {
    use webrender::api::units::DeviceIntSize;

    WrImageDescriptor {
        format: wr_translate_image_format(descriptor.format),
        size: DeviceIntSize::new(descriptor.dimensions.0 as i32, descriptor.dimensions.1 as i32),
        stride: descriptor.stride,
        offset: descriptor.offset,
        flags: webrender::webrender_api::ImageDescriptorFlags::ALLOW_MIPMAPS | webrender::webrender_api::ImageDescriptorFlags::IS_OPAQUE
    }

}

#[inline(always)]
pub(crate) const fn wr_translate_au(au: Au) -> WrAu {
    WrAu(au.0)
}

#[inline(always)]
pub(crate) fn wr_translate_add_font_instance(add_font_instance: AddFontInstance) -> WrAddFontInstance {
    WrAddFontInstance {
        key: wr_translate_font_instance_key(add_font_instance.key),
        font_key: wr_translate_font_key(add_font_instance.font_key),
        glyph_size: wr_translate_au(add_font_instance.glyph_size),
        options: add_font_instance.options.map(wr_translate_font_instance_options),
        platform_options: add_font_instance.platform_options.map(wr_translate_font_instance_platform_options),
        variations: add_font_instance.variations.into_iter().map(wr_translate_font_variation).collect(),
    }
}

#[inline(always)]
fn wr_translate_font_instance_options(fio: FontInstanceOptions) -> WrFontInstanceOptions {
    WrFontInstanceOptions {
        render_mode: wr_translate_font_render_mode(fio.render_mode),
        flags: wr_translate_font_instance_flags(fio.flags),
        bg_color: wr_translate_color_u(fio.bg_color),
        synthetic_italics: wr_translate_synthetic_italics(fio.synthetic_italics),
    }
}

const fn wr_translate_synthetic_italics(si: SyntheticItalics) -> WrSyntheticItalics {
    WrSyntheticItalics { angle: si.angle }
}

#[cfg(target_os = "windows")]
#[inline(always)]
const fn wr_translate_font_instance_platform_options(fio: FontInstancePlatformOptions) -> WrFontInstancePlatformOptions {
    WrFontInstancePlatformOptions {
        gamma: fio.gamma,
        contrast: fio.contrast,
        cleartype_level: fio.cleartype_level
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[inline(always)]
fn wr_translate_font_hinting(lcd: FontHinting) -> WrFontHinting {
    match lcd {
        FontHinting::None => WrFontHinting::None,
        FontHinting::Mono => WrFontHinting::Mono,
        FontHinting::Light => WrFontHinting::Light,
        FontHinting::Normal => WrFontHinting::Normal,
        FontHinting::LCD => WrFontHinting::LCD,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[inline(always)]
fn wr_translate_font_lcd_filter(lcd: FontLCDFilter) -> WrFontLCDFilter {
    match lcd {
        FontLCDFilter::None => WrFontLCDFilter::None,
        FontLCDFilter::Default => WrFontLCDFilter::Default,
        FontLCDFilter::Light => WrFontLCDFilter::Light,
        FontLCDFilter::Legacy => WrFontLCDFilter::Legacy,
    }
}

#[cfg(not(target_os = "windows"))]
fn wr_translate_font_instance_platform_options(fio: FontInstancePlatformOptions) -> WrFontInstancePlatformOptions {
    WrFontInstancePlatformOptions {
        lcd_filter: wr_translate_font_lcd_filter(fio.lcd_filter),
        hinting: wr_translate_font_hinting(fio.hinting),
    }
}

#[cfg(target_os = "macos")]
#[inline(always)]
const fn wr_translate_font_instance_platform_options(fio: FontInstancePlatformOptions) -> WrFontInstancePlatformOptions {
    WrFontInstancePlatformOptions {
        unused: fio.unused,
    }
}

#[inline(always)]
const fn wr_translate_font_variation(variation: FontVariation) -> WrFontVariation {
    WrFontVariation { tag: variation.tag, value: variation.value }
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
        ImageFormat::RG16 => WrImageFormat::RG16,
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
pub fn wr_translate_border_radius(border_radius: StyleBorderRadius, rect_size: CssLayoutSize) -> WrBorderRadius {

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
fn wr_translate_layout_size(input: CssLayoutSize) -> WrLayoutSize {
    WrLayoutSize::new(input.width, input.height)
}

#[inline]
pub(crate) fn wr_translate_layout_point(input: CssLayoutPoint) -> WrLayoutPoint {
    WrLayoutPoint::new(input.x, input.y)
}

#[inline]
fn wr_translate_layout_rect(input: CssLayoutRect) -> WrLayoutRect {
    WrLayoutRect::new(wr_translate_layout_point(input.origin), wr_translate_layout_size(input.size))
}

#[inline]
fn translate_layout_size_wr(input: WrLayoutSize) -> CssLayoutSize {
    CssLayoutSize::new(input.width, input.height)
}

#[inline]
fn translate_layout_point_wr(input: WrLayoutPoint) -> CssLayoutPoint {
    CssLayoutPoint::new(input.x, input.y)
}

#[inline]
fn translate_layout_rect_wr(input: WrLayoutRect) -> CssLayoutRect {
    CssLayoutRect::new(translate_layout_point_wr(input.origin), translate_layout_size_wr(input.size))
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

pub(crate) fn set_webrender_debug_flags(r: &mut Renderer, new_flags: &DebugState) {

    use webrender::DebugFlags;

    // Set all flags to false
    let mut debug_flags = DebugFlags::empty();

    debug_flags.set(DebugFlags::PROFILER_DBG, new_flags.profiler_dbg);
    debug_flags.set(DebugFlags::RENDER_TARGET_DBG, new_flags.render_target_dbg);
    debug_flags.set(DebugFlags::TEXTURE_CACHE_DBG, new_flags.texture_cache_dbg);
    debug_flags.set(DebugFlags::GPU_TIME_QUERIES, new_flags.gpu_time_queries);
    debug_flags.set(DebugFlags::GPU_SAMPLE_QUERIES, new_flags.gpu_sample_queries);
    debug_flags.set(DebugFlags::DISABLE_BATCHING, new_flags.disable_batching);
    debug_flags.set(DebugFlags::EPOCHS, new_flags.epochs);
    debug_flags.set(DebugFlags::COMPACT_PROFILER, new_flags.compact_profiler);
    debug_flags.set(DebugFlags::ECHO_DRIVER_MESSAGES, new_flags.echo_driver_messages);
    debug_flags.set(DebugFlags::NEW_FRAME_INDICATOR, new_flags.new_frame_indicator);
    debug_flags.set(DebugFlags::NEW_SCENE_INDICATOR, new_flags.new_scene_indicator);
    debug_flags.set(DebugFlags::SHOW_OVERDRAW, new_flags.show_overdraw);
    debug_flags.set(DebugFlags::GPU_CACHE_DBG, new_flags.gpu_cache_dbg);

    r.set_debug_flags(debug_flags);
}

#[inline(always)]
pub(crate) fn wr_translate_resource_update(resource_update: ResourceUpdate) -> WrResourceUpdate {
    match resource_update {
        ResourceUpdate::AddFont(af) => WrResourceUpdate::AddFont(wr_translate_add_font(af)),
        ResourceUpdate::DeleteFont(fk) => WrResourceUpdate::DeleteFont(wr_translate_font_key(fk)),
        ResourceUpdate::AddFontInstance(fi) => WrResourceUpdate::AddFontInstance(wr_translate_add_font_instance(fi)),
        ResourceUpdate::DeleteFontInstance(fi) => WrResourceUpdate::DeleteFontInstance(wr_translate_font_instance_key(fi)),
        ResourceUpdate::AddImage(ai) => WrResourceUpdate::AddImage(wr_translate_add_image(ai)),
        ResourceUpdate::UpdateImage(ui) => WrResourceUpdate::UpdateImage(wr_translate_update_image(ui)),
        ResourceUpdate::DeleteImage(k) => WrResourceUpdate::DeleteImage(wr_translate_image_key(k)),
    }
}

#[inline(always)]
fn wr_translate_add_font(add_font: AddFont) -> WrAddFont {
    WrAddFont::Raw(wr_translate_font_key(add_font.key), add_font.font_bytes, add_font.font_index)
}

#[inline(always)]
fn wr_translate_add_image(add_image: AddImage) -> WrAddImage {
    WrAddImage {
        key: wr_translate_image_key(add_image.key),
        descriptor: wr_translate_image_descriptor(add_image.descriptor),
        data: wr_translate_image_data(add_image.data),
        tiling: add_image.tiling,
    }
}

#[inline(always)]
fn wr_translate_image_data(image_data: ImageData) -> WrImageData {
    match image_data {
        ImageData::Raw(data) => WrImageData::Raw(data),
        ImageData::External(external) => WrImageData::External(wr_translate_external_image_data(external)),
    }
}

#[inline(always)]
fn wr_translate_external_image_data(external: ExternalImageData) -> WrExternalImageData {
    WrExternalImageData {
        id: wr_translate_external_image_id(external.id),
        channel_index: external.channel_index,
        image_type: wr_translate_external_image_type(external.image_type),
    }
}

#[inline(always)]
pub(crate) const fn wr_translate_external_image_id(external: ExternalImageId) -> WrExternalImageId {
    WrExternalImageId(external.0)
}

#[inline(always)]
pub(crate) const fn translate_external_image_id_wr(external: WrExternalImageId) -> ExternalImageId {
    ExternalImageId(external.0)
}

#[inline(always)]
fn wr_translate_external_image_type(external: ExternalImageType) -> WrExternalImageType {
    match external {
        ExternalImageType::TextureHandle(tt) => WrExternalImageType::TextureHandle(wr_translate_texture_target(tt)),
        ExternalImageType::Buffer => WrExternalImageType::Buffer,
    }
}

#[inline(always)]
fn wr_translate_texture_target(texture_target: TextureTarget) -> WrTextureTarget {
    match texture_target {
        TextureTarget::Default => WrTextureTarget::Default,
        TextureTarget::Array => WrTextureTarget::Array,
        TextureTarget::Rect => WrTextureTarget::Rect,
        TextureTarget::External => WrTextureTarget::External,
    }
}

#[inline(always)]
fn wr_translate_update_image(update_image: UpdateImage) -> WrUpdateImage {
    WrUpdateImage {
        key: wr_translate_image_key(update_image.key),
        descriptor: wr_translate_image_descriptor(update_image.descriptor),
        data: wr_translate_image_data(update_image.data),
        dirty_rect: wr_translate_image_dirty_rect(update_image.dirty_rect),
    }
}

#[inline(always)]
fn wr_translate_image_dirty_rect(dirty_rect: ImageDirtyRect) -> WrImageDirtyRect {
    use webrender::api::units::{
        DeviceIntRect as WrDeviceIntRect,
        DeviceIntPoint as WrDeviceIntPoint,
        DeviceIntSize as WrDeviceIntSize,
    };

    use webrender::api::DirtyRect as WrDirtyRect;

    match dirty_rect {
        ImageDirtyRect::All => WrDirtyRect::All,
        ImageDirtyRect::Partial(rect) => WrDirtyRect::Partial(
            WrDeviceIntRect::new(
                WrDeviceIntPoint::new(rect.origin.x as i32, rect.origin.y as i32),
                WrDeviceIntSize::new(rect.size.width as i32, rect.size.height as i32),
            )
        ),
    }
}

#[inline(always)]
pub(crate) fn wr_translate_external_scroll_id(scroll_id: ExternalScrollId) -> WrExternalScrollId {
    WrExternalScrollId(scroll_id.0, wr_translate_pipeline_id(scroll_id.1))
}

pub(crate) fn wr_translate_display_list(input: CachedDisplayList, pipeline_id: PipelineId) -> WrBuiltDisplayList {
    let root_space_and_clip = WrSpaceAndClipInfo::root_scroll(wr_translate_pipeline_id(pipeline_id));
    let mut builder = WrDisplayListBuilder::new(
        wr_translate_pipeline_id(pipeline_id),
        wr_translate_layout_size(input.root.get_size())
    );
    push_display_list_msg(&mut builder, input.root, &root_space_and_clip);
    builder.finalize().2
}

#[inline]
fn push_display_list_msg(builder: &mut WrDisplayListBuilder, msg: DisplayListMsg, parent_space_and_clip: &WrSpaceAndClipInfo) {
    use azul_core::display_list::DisplayListMsg::*;
    match msg {
        Frame(f) => push_frame(builder, f, parent_space_and_clip),
        ScrollFrame(sf) => push_scroll_frame(builder, sf, parent_space_and_clip),
    }
}

#[inline]
fn push_frame(
    builder: &mut WrDisplayListBuilder,
    frame: DisplayListFrame,
    parent_space_and_clip: &WrSpaceAndClipInfo
) {

    use webrender::api::{
        ClipMode as WrClipMode,
        ComplexClipRegion as WrComplexClipRegion
    };

    let wr_rect = wr_translate_layout_rect(frame.rect);
    let wr_border_radius = wr_translate_border_radius(frame.border_radius, frame.rect.size);

    let content_clip = WrComplexClipRegion::new(wr_rect, wr_border_radius, WrClipMode::Clip);
    let content_clip_id = builder.define_clip(parent_space_and_clip, wr_rect, vec![content_clip], /* image_mask: */ None);
    let content_space_and_clip = WrSpaceAndClipInfo {
        spatial_id: parent_space_and_clip.spatial_id,
        clip_id: content_clip_id,
    };
    let info = WrCommonItemProperties::new(
        wr_translate_layout_rect(frame.clip_rect.unwrap_or(frame.rect)), 
        content_space_and_clip);



    for item in frame.content {
        push_display_list_content(builder, item, &info, frame.border_radius, &content_space_and_clip);
    }

    // If the rect has an overflow:* property set
    let overflow_clip_id = frame.clip_rect.map(|clip_rect| {
        let clip_rect = wr_translate_layout_rect(clip_rect);
        let clip = WrComplexClipRegion::new(clip_rect, wr_border_radius, WrClipMode::Clip);
        let clip_id = builder.define_clip(parent_space_and_clip, clip_rect, vec![clip], /* image_mask: */ None);
        clip_id
    }).unwrap_or(parent_space_and_clip.clip_id);

    let overflow_space_and_clip = WrSpaceAndClipInfo {
        spatial_id: parent_space_and_clip.spatial_id,
        clip_id: overflow_clip_id,
    };

    for child in frame.children {
        push_display_list_msg(builder, child, &overflow_space_and_clip);
    }
}

#[inline]
fn push_scroll_frame(
    builder: &mut WrDisplayListBuilder,
    scroll_frame: DisplayListScrollFrame,
    parent_space_and_clip: &WrSpaceAndClipInfo
) {

    use azul_css::ColorU;
    use webrender::api::{
        ClipMode as WrClipMode,
        ScrollSensitivity as WrScrollSensitivity,
        ComplexClipRegion as WrComplexClipRegion,
        CommonItemProperties
    };

    let wr_rect = wr_translate_layout_rect(scroll_frame.frame.rect);
    let wr_border_radius = wr_translate_border_radius(scroll_frame.frame.border_radius, scroll_frame.frame.rect.size);

    let hit_test_info = WrLayoutPrimitiveInfo {
        rect: wr_rect,
        clip_rect: wr_translate_layout_rect(scroll_frame.frame.clip_rect.unwrap_or(scroll_frame.frame.rect)),
        is_backface_visible: false,
        tag:  wr_translate_tag_id(scroll_frame.scroll_tag.0),
    };
    


    // Push content (overflowing)
    let content_clip = WrComplexClipRegion::new(wr_rect, wr_border_radius, WrClipMode::Clip);
    let content_clip_id = builder.define_clip(parent_space_and_clip, wr_rect, vec![content_clip], /* image_mask: */ None);
    let content_clip_info = WrSpaceAndClipInfo {
        spatial_id: parent_space_and_clip.spatial_id,
        clip_id: content_clip_id,
    };

    let info = WrCommonItemProperties::new(
        wr_rect, 
        content_clip_info);


    for item in scroll_frame.frame.content {
        push_display_list_content(builder, item, &info, scroll_frame.frame.border_radius, &content_clip_info);
    }

    // Push hit-testing + scrolling children
    let scroll_frame_clip_region = WrComplexClipRegion::new(wr_rect, wr_border_radius, WrClipMode::Clip);
    let hit_testing_clip_id = builder.define_clip(parent_space_and_clip, wr_rect, vec![scroll_frame_clip_region], /* image_mask: */  None);
    let hit_testing_clip_info = WrSpaceAndClipInfo {
        spatial_id: parent_space_and_clip.spatial_id,
        clip_id: hit_testing_clip_id,
    };
    
    let hit_test_info_common_item_properties = CommonItemProperties::new(hit_test_info.clip_rect, hit_testing_clip_info);

    builder.push_rect(&hit_test_info_common_item_properties, wr_translate_color_u(ColorU::TRANSPARENT).into()); // push hit-testing rect
    // builder.push_rect(&hit_test_info, &hit_testing_clip_info, wr_translate_color_u(ColorU::TRANSPARENT).into()); // push hit-testing rect
    
    // no idea if this is correct, I strongly assume it is not. Therefore: TODO:
    let layout_vector : webrender::euclid::Vector2D<f32, webrender::webrender_api::units::LayoutPixel> = webrender::api::units::LayoutVector2D::new(0.0, 0.0);

    let scroll_frame_clip_info = builder.define_scroll_frame(
        /* parent clip */ &hit_testing_clip_info, // scroll frame has the hit-testing clip as a parent
        /* external id*/ Some(wr_translate_external_scroll_id(scroll_frame.scroll_id)),
        /* content_rect */ wr_translate_layout_rect(scroll_frame.content_rect),
        /* clip_rect */ wr_translate_layout_rect(scroll_frame.frame.clip_rect.unwrap_or(scroll_frame.frame.rect)),
        /* complex_clips */ vec![scroll_frame_clip_region],
        /* image_mask */ None,
        /* sensitivity */ WrScrollSensitivity::Script,
        /* external_scroll_offset */ layout_vector
    );

    // Only children should scroll, not the frame itself!
    for child in scroll_frame.frame.children {
        push_display_list_msg(builder, child, &scroll_frame_clip_info);
    }
}


#[inline]
fn push_display_list_content(
    builder: &mut WrDisplayListBuilder,
    content: LayoutRectContent,
    info: &WrCommonItemProperties,
    radii: StyleBorderRadius,
    parent_space_and_clip: &WrSpaceAndClipInfo,
) {

    use azul_core::display_list::LayoutRectContent::*;

    match content {
        Text { glyphs, font_instance_key, color, glyph_options, clip } => {
            text::push_text(builder, info, glyphs, font_instance_key, color, glyph_options, clip, parent_space_and_clip);
        },
        Background { content, size, offset, repeat  } => {
            background::push_background(builder, info, content, size, offset, repeat, parent_space_and_clip);
        },
        Image { size, offset, image_rendering, alpha_type, image_key, background_color } => {
            image::push_image(builder, info, size, offset, image_key, alpha_type, image_rendering, background_color, parent_space_and_clip);
        },
        Border { widths, colors, styles } => {
            border::push_border(builder, info, radii, widths, colors, styles, parent_space_and_clip);
        },
        BoxShadow { shadow, clip_mode } => {
            box_shadow::push_box_shadow(builder, translate_layout_rect_wr(info.clip_rect), clip_mode, shadow, radii, parent_space_and_clip);
        },
    }
}

mod text {
    use webrender::api::{
        DisplayListBuilder as WrDisplayListBuilder,
        SpaceAndClipInfo as WrSpaceAndClipInfo,
        CommonItemProperties as WrCommonItemProperties,
    };
    use azul_core::{
        app_resources::{FontInstanceKey, GlyphOptions},
        display_list::GlyphInstance,
    };
    use azul_css::{ColorU, LayoutRect};

    pub(in super) fn push_text(
         builder: &mut WrDisplayListBuilder,
         info: &WrCommonItemProperties,
         glyphs: Vec<GlyphInstance>,
         font_instance_key: FontInstanceKey,
         color: ColorU,
         glyph_options: Option<GlyphOptions>,
         clip: Option<LayoutRect>,
         _parent_space_and_clip: &WrSpaceAndClipInfo,
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
            info.clip_rect,
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
        GradientStop as WrGradientStop,
        SpaceAndClipInfo as WrSpaceAndClipInfo,
        CommonItemProperties as WrCommonItemProperties,
        units::{
            LayoutSize as WrLayoutSize,
            LayoutRect as WrLayoutRect,
        }
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
        info: &WrCommonItemProperties,
        background: RectBackground,
        background_size: Option<StyleBackgroundSize>,
        background_position: Option<StyleBackgroundPosition>,
        background_repeat: Option<StyleBackgroundRepeat>,
        parent_space_and_clip: &WrSpaceAndClipInfo,
    ) {
        use azul_core::display_list::RectBackground::*;

        let content_size = background.get_content_size();

        match background {
            RadialGradient(rg)  => push_radial_gradient_background(builder, info, rg, background_position, background_size, background_repeat, content_size, parent_space_and_clip),
            LinearGradient(g)   => push_linear_gradient_background(builder, info, g, background_position, background_size, background_repeat, content_size, parent_space_and_clip),
            Image(image_info)   => push_image_background(builder, info, image_info, background_position, background_size, background_repeat, content_size, parent_space_and_clip),
            Color(col)          => push_color_background(builder, info, col, background_position, background_size, background_repeat, content_size, parent_space_and_clip),
        }
    }

    fn push_radial_gradient_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        radial_gradient: RadialGradient,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        background_repeat: Option<StyleBackgroundRepeat>,
        content_size: Option<(f32, f32)>,
        _parent_space_and_clip: &WrSpaceAndClipInfo,
    ) {
        use azul_css::Shape;
        use super::{wr_translate_color_u, wr_translate_layout_size, wr_translate_extend_mode};

        let background_position = background_position.unwrap_or_default();
        let _background_repeat = background_repeat.unwrap_or_default();
        let background_size = calculate_background_size(info, background_size, content_size);
        let offset = calculate_background_position(info, background_position, background_size);

        let mut offset_info = *info;
        offset_info.clip_rect.origin.x += offset.x;
        offset_info.clip_rect.origin.y += offset.y;

        let stops: Vec<WrGradientStop> = radial_gradient.stops.iter().map(|gradient_pre|
            WrGradientStop {
                offset: gradient_pre.offset.unwrap().get(),
                color: wr_translate_color_u(gradient_pre.color).into(),
            }).collect();

        let center = info.clip_rect.center();

        // Note: division by 2.0 because it's the radius, not the diameter
        let radius = match radial_gradient.shape {
            Shape::Ellipse => WrLayoutSize::new(background_size.width / 2.0, background_size.height / 2.0),
            Shape::Circle => {
                let largest_bound_size = background_size.width.max(background_size.height);
                WrLayoutSize::new(largest_bound_size / 2.0, largest_bound_size / 2.0)
            },
        };

        let gradient = builder.create_radial_gradient(center, radius, stops, wr_translate_extend_mode(radial_gradient.extend_mode));

        builder.push_radial_gradient(
            &offset_info,
            offset_info.clip_rect,
            gradient,
            wr_translate_layout_size(background_size),
            WrLayoutSize::zero()
        );
    }

    fn push_linear_gradient_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        linear_gradient: LinearGradient,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        background_repeat: Option<StyleBackgroundRepeat>,
        content_size: Option<(f32, f32)>,
        _parent_space_and_clip: &WrSpaceAndClipInfo,
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
        offset_info.clip_rect.origin.x += offset.x;
        offset_info.clip_rect.origin.y += offset.y;

        let stops: Vec<WrGradientStop> = linear_gradient.stops.iter().map(|gradient_pre|
            WrGradientStop {
                offset: gradient_pre.offset.unwrap().get() / 100.0,
                color: wr_translate_color_u(gradient_pre.color).into(),
            }).collect();

        let (begin_pt, end_pt) = linear_gradient.direction.to_points(&wr_translate_css_layout_rect(offset_info.clip_rect));
        let gradient = builder.create_gradient(
            wr_translate_layout_point(begin_pt),
            wr_translate_layout_point(end_pt),
            stops,
            wr_translate_extend_mode(linear_gradient.extend_mode),
        );

        builder.push_gradient(
            &offset_info,
            offset_info.clip_rect,
            gradient,
            wr_translate_layout_size(background_size),
            WrLayoutSize::zero()
        );
    }

    fn push_image_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        image_info: ImageInfo,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        background_repeat: Option<StyleBackgroundRepeat>,
        content_size: Option<(f32, f32)>,
        parent_space_and_clip: &WrSpaceAndClipInfo,
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

        image::push_image(builder, &background_repeat_info, background_size, background_position, image_info.key, alpha_type, image_rendering, background_color, parent_space_and_clip);
    }

    fn push_color_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        color: ColorU,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        background_repeat: Option<StyleBackgroundRepeat>,
        content_size: Option<(f32, f32)>,
        _parent_space_and_clip: &WrSpaceAndClipInfo,
    ) {
        use super::wr_translate_color_u;

        let background_position = background_position.unwrap_or_default();
        let _background_repeat = background_repeat.unwrap_or_default();
        let background_size = calculate_background_size(info, background_size, content_size);
        let offset = calculate_background_position(info, background_position, background_size);

        let mut offset_info = *info;
        offset_info.clip_rect.origin.x += offset.x;
        offset_info.clip_rect.origin.y += offset.y;
        offset_info.clip_rect.size.width = background_size.width;
        offset_info.clip_rect.size.height = background_size.height;

        builder.push_rect(
            &offset_info,
            wr_translate_color_u(color).into()
        );
    }

    fn get_background_repeat_info(
        info: &WrCommonItemProperties,
        background_repeat: StyleBackgroundRepeat,
        background_size: LayoutSize,
    ) -> WrCommonItemProperties {

        // mismatched types
        // expected struct `webrender::euclid::Size2D<f32, webrender::webrender_api::units::LayoutPixel>`
        //    found struct `azul_css::LayoutSize`rustc(E0308)

        use azul_css::StyleBackgroundRepeat::*;
        use webrender::euclid::Size2D;

        let mut info_info = info.clone();
        match background_repeat {
            NoRepeat => {
                info_info.clip_rect.size = Size2D::new(background_size.width, background_size.height);
            },
            Repeat => {
                // when repeating, don't change anything about the clip_rect
                // info_info
            },
            RepeatX => {
                info_info.clip_rect.size = Size2D::new(info.clip_rect.size.width, background_size.height);
            },
            RepeatY => {
                info_info.clip_rect.size = Size2D::new(background_size.width, info.clip_rect.size.height);
            },
        };

        info_info
    }

    /// Transform a background size such as "cover" or "contain" into actual pixels
    fn calculate_background_size(
        info: &WrCommonItemProperties,
        bg_size: Option<StyleBackgroundSize>,
        content_size: Option<(f32, f32)>,
    ) -> LayoutSize {

        let content_size = content_size.unwrap_or((info.clip_rect.size.width, info.clip_rect.size.height));

        let bg_size = match bg_size {
            None => return LayoutSize::new(content_size.0 as f32, content_size.1 as f32),
            Some(s) => s,
        };

        let content_aspect_ratio = Ratio {
            width: info.clip_rect.size.width / content_size.0 as f32,
            height: info.clip_rect.size.height / content_size.1 as f32,
        };

        let ratio = match bg_size {
            StyleBackgroundSize::ExactSize(w, h) => {
                let w = w.to_pixels(info.clip_rect.size.width);
                let h = h.to_pixels(info.clip_rect.size.height);
                w.min(h)
            },
            StyleBackgroundSize::Contain => content_aspect_ratio.width.min(content_aspect_ratio.height),
            StyleBackgroundSize::Cover => content_aspect_ratio.width.max(content_aspect_ratio.height),
        };

        LayoutSize::new(content_size.0 as f32 * ratio, content_size.1 as f32 * ratio)
    }

    /// Transforma background-position attribute into pixel coordinates
    fn calculate_background_position(
        info: &WrCommonItemProperties,
        background_position: StyleBackgroundPosition,
        background_size: LayoutSize,
    ) -> LayoutPoint {

        use azul_css::BackgroundPositionVertical;
        use azul_css::BackgroundPositionHorizontal;

        let width = info.clip_rect.size.width;
        let height = info.clip_rect.size.height;

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
        SpaceAndClipInfo as WrSpaceAndClipInfo,
        CommonItemProperties as WrCommonItemProperties,
    };
    use azul_css::{LayoutPoint, LayoutSize, ColorU};
    use azul_core::{
        app_resources::ImageKey,
        display_list::{AlphaType, ImageRendering}
    };

    #[inline]
    pub(in super) fn push_image(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        _size: LayoutSize,
        offset: LayoutPoint,
        image_key: ImageKey,
        alpha_type: AlphaType,
        image_rendering: ImageRendering,
        background_color: ColorU,
        _parent_space_and_clip: &WrSpaceAndClipInfo,
    ) {
        use super::{
            wr_translate_image_rendering, wr_translate_alpha_type,
            wr_translate_color_u, wr_translate_image_key,
        };
        use webrender::api::units::LayoutSize as WrLayoutSize;

        let mut offset_info = *info;
        offset_info.clip_rect.origin.x += offset.x;
        offset_info.clip_rect.origin.y += offset.y;

        let _tile_spacing = WrLayoutSize::zero();

        builder.push_image(
            &offset_info,
            offset_info.clip_rect,
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
        DisplayListBuilder as WrDisplayListBuilder,
        SpaceAndClipInfo as WrSpaceAndClipInfo,
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
        parent_space_and_clip: &WrSpaceAndClipInfo,
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
                    &top, &bottom, &left, &right, parent_space_and_clip,
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
                            &top, &None, &None, &None, parent_space_and_clip,
                        );
                        push_single_box_shadow_edge(
                            builder, &b, bounds, border_radius, shadow_type,
                            &None, &bottom, &None, &None, parent_space_and_clip,
                        );
                    },
                    // left + right box-shadow pair
                    (None, Some(l), None, Some(r)) => {
                        push_single_box_shadow_edge(
                            builder, &l, bounds, border_radius, shadow_type,
                            &None, &None, &left, &None, parent_space_and_clip,
                        );
                        push_single_box_shadow_edge(
                            builder, &r, bounds, border_radius, shadow_type,
                            &None, &None, &None, &right, parent_space_and_clip,
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
                    parent_space_and_clip,
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
        parent_space_and_clip: &WrSpaceAndClipInfo,
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
            shadow_type,
            parent_space_and_clip,
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
        parent_space_and_clip: &WrSpaceAndClipInfo,
    ) {
        use webrender::api::units::{LayoutRect, LayoutPoint, LayoutVector2D};
        use webrender::api::CommonItemProperties as WrCommonItemProperties;

        use super::{
            wr_translate_color_f, wr_translate_border_radius,
            wr_translate_box_shadow_clip_mode, wr_translate_layout_rect,
        };

        // The pre_shadow is missing the StyleBorderRadius & LayoutRect
        if pre_shadow.clip_mode != shadow_type {
            return;
        }

        let full_screen_rect = LayoutRect::new(LayoutPoint::zero(), builder.content_size());

        // Prevent shadows that are larger than the full screen
        let clip_rect = wr_translate_layout_rect(clip_rect);
        let clip_rect = clip_rect.intersection(&full_screen_rect).unwrap_or(clip_rect);

        let info = WrCommonItemProperties::new(
            clip_rect, *parent_space_and_clip
        );

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
        units::LayoutSideOffsets as WrLayoutSideOffsets,
        BorderDetails as WrBorderDetails,
        BorderStyle as WrBorderStyle,
        BorderSide as WrBorderSide,
        SpaceAndClipInfo as WrSpaceAndClipInfo,
        CommonItemProperties as WrCommonItemProperties,
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
        info: &WrCommonItemProperties,
        radii: StyleBorderRadius,
        widths: StyleBorderWidths,
        colors: StyleBorderColors,
        styles: StyleBorderStyles,
        _parent_space_and_clip: &WrSpaceAndClipInfo,
    ) {
        let rect_size = LayoutSize::new(info.clip_rect.size.width, info.clip_rect.size.height);
        if let Some((border_widths, border_details)) = get_webrender_border(rect_size, radii, widths, colors, styles) {
            builder.push_border(info, info.clip_rect, border_widths, border_details);
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
