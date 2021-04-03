//! Type translation functions (from azul-css to webrender types)
//!
//! The reason for doing this is so that azul-core doesn't depend on webrender
//! (since webrender is a huge dependency) just to use the types. Only if you depend on
//! azul (not azul-core), you have to depend on webrender.

use webrender::render_api::{
    ResourceUpdate as WrResourceUpdate,
    AddFont as WrAddFont,
    AddImage as WrAddImage,
    UpdateImage as WrUpdateImage,
    AddFontInstance as WrAddFontInstance,
};
use webrender::api::{
    units::{
        LayoutSize as WrLayoutSize,
        LayoutPoint as WrLayoutPoint,
        LayoutRect as WrLayoutRect,
        LayoutSideOffsets as WrLayoutSideOffsets,
        ImageDirtyRect as WrImageDirtyRect,
        LayoutVector2D as WrLayoutVector2D,
        LayoutTransform as WrLayoutTransform,
    },
    TransformStyle as WrTransformStyle,
    PropertyBinding as WrPropertyBinding,
    ReferenceFrameKind as WrReferenceFrameKind,
    ImageBufferKind as WrImageBufferKind,
    CommonItemProperties as WrCommonItemProperties,
    FontKey as WrFontKey,
    FontInstanceKey as WrFontInstanceKey,
    ImageKey as WrImageKey,
    ClipId as WrClipId,
    SpatialId as WrSpatialId,
    IdNamespace as WrIdNamespace,
    PipelineId as WrPipelineId,
    DocumentId as WrDocumentId,
    ColorU as WrColorU,
    ColorF as WrColorF,
    PrimitiveFlags as WrPrimitiveFlags,
    BorderRadius as WrBorderRadius,
    BorderSide as WrBorderSide,
    BoxShadowClipMode as WrBoxShadowClipMode,
    ExtendMode as WrExtendMode,
    BorderStyle as WrBorderStyle,
    ImageFormat as WrImageFormat,
    ImageDescriptor as WrImageDescriptor,
    ImageDescriptorFlags as WrImageDescriptorFlags,
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
    ImageData as WrImageData,
    ExternalImageData as WrExternalImageData,
    ExternalImageId as WrExternalImageId,
    ExternalImageType as WrExternalImageType,
    Epoch as WrEpoch,
    FontVariation as WrFontVariation,
    FontInstanceOptions as WrFontInstanceOptions,
    FontInstancePlatformOptions as WrFontInstancePlatformOptions,
    SyntheticItalics as WrSyntheticItalics,
    ImageMask as WrImageMask,
};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use webrender::api::{
    FontLCDFilter as WrFontLCDFilter,
    FontHinting as WrFontHinting,
};

use azul_core::{
    callbacks::{PipelineId, DocumentId},
    app_resources::{
        FontKey, Au, FontInstanceKey, ImageKey,
        IdNamespace, RawImageFormat as ImageFormat, ImageDescriptor, ImageDescriptorFlags,
        FontInstanceFlags, FontRenderMode, GlyphOptions, ResourceUpdate,
        AddFont, AddImage, ImageData, ExternalImageData, ExternalImageId,
        ExternalImageType, ImageBufferKind, UpdateImage, ImageDirtyRect,
        Epoch, AddFontInstance, FontVariation, FontInstanceOptions,
        FontInstancePlatformOptions, SyntheticItalics, PrimitiveFlags,
        TransformKey,
    },
    display_list::{
        CachedDisplayList, GlyphInstance, DisplayListScrollFrame,
        DisplayListFrame, LayoutRectContent, DisplayListMsg,
        AlphaType, ImageRendering, StyleBorderRadius, BoxShadow,
    },
    dom::TagId,
    display_list::DisplayListImageMask,
    ui_solver::{ExternalScrollId, PositionInfo, ComputedTransform3D},
    window::{LogicalSize, LogicalPosition, LogicalRect, DebugState},
};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use azul_core::app_resources::{FontLCDFilter, FontHinting};
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
use webrender::Renderer;

pub(crate) mod winit_translate {

    use azul_core::{
        window::{
            LogicalSize, PhysicalSize, LogicalPosition, PhysicalPosition,
            WindowIcon, TaskBarIcon, MouseCursorType, VirtualKeyCode, WindowTheme,
        },
    };
    use glutin::{
        event::VirtualKeyCode as WinitVirtualKeyCode,
        window::{
            CursorIcon as WinitCursorIcon,
            BadIcon as WinitBadIcon,
            Icon as WinitIcon,
            Theme as WinitWindowTheme,
        },
    };
    #[cfg(target_os = "linux")]
    use glutin::platform::unix::{
        Theme as WinitWaylandTheme,
        ButtonState as WinitButtonState,
        Element as WinitElement,
        Button as WinitButton,
        XWindowType as WinitXWindowType,
        ARGBColor as WinitARGBColor,
    };
    #[cfg(target_os = "linux")]
    use azul_core::window::{WaylandTheme, XWindowType};

    use glutin::dpi::PhysicalSize as WinitPhysicalSize;
    use glutin::dpi::PhysicalPosition as WinitPhysicalPosition;

    pub(crate) type WinitLogicalSize = glutin::dpi::LogicalSize<f64>;
    pub(crate) type WinitLogicalPosition = glutin::dpi::LogicalPosition<f64>;

    #[cfg(target_os = "linux")]
    const fn translate_color(input: [u8;4]) -> WinitARGBColor {
        WinitARGBColor { r: input[0], g: input[1], b: input[2], a: input[3] }
    }

    #[cfg(target_os = "linux")]
    #[derive(Debug)]
    pub struct WaylandThemeWrapper(pub WaylandTheme);

    #[cfg(target_os = "linux")]
    impl WinitWaylandTheme for WaylandThemeWrapper {
        fn element_color(&self, element: WinitElement, window_active: bool) -> WinitARGBColor {
            if window_active {
                match element {
                    WinitElement::Bar => translate_color(self.0.title_bar_active_background_color),
                    WinitElement::Separator => translate_color(self.0.title_bar_active_separator_color),
                    WinitElement::Text => translate_color(self.0.title_bar_active_text_color),
                }
            } else {
                match element {
                    WinitElement::Bar => translate_color(self.0.title_bar_inactive_background_color),
                    WinitElement::Separator => translate_color(self.0.title_bar_inactive_separator_color),
                    WinitElement::Text => translate_color(self.0.title_bar_inactive_text_color),
                }
            }
        }

        fn button_color(
            &self,
            button: WinitButton,
            button_state: WinitButtonState,
            foreground: bool,
            window_active: bool
        ) -> WinitARGBColor {
            match button {
                WinitButton::Maximize => {
                    match button_state {
                        WinitButtonState::Idle => {
                            if foreground {
                                if window_active {
                                    translate_color(self.0.maximize_idle_foreground_active_color)
                                } else {
                                    translate_color(self.0.maximize_idle_foreground_inactive_color)
                                }
                            } else {
                                if window_active {
                                    translate_color(self.0.maximize_idle_background_active_color)
                                } else {
                                    translate_color(self.0.maximize_idle_background_inactive_color)
                                }
                            }
                        },
                        WinitButtonState::Hovered => {
                            if foreground {
                                if window_active {
                                    translate_color(self.0.maximize_hovered_foreground_active_color)
                                } else {
                                    translate_color(self.0.maximize_hovered_foreground_inactive_color)
                                }
                            } else {
                                if window_active {
                                    translate_color(self.0.maximize_hovered_background_active_color)
                                } else {
                                    translate_color(self.0.maximize_hovered_background_inactive_color)
                                }
                            }
                        },
                        WinitButtonState::Disabled => {
                            if foreground {
                                if window_active {
                                    translate_color(self.0.maximize_disabled_foreground_active_color)
                                } else {
                                    translate_color(self.0.maximize_disabled_foreground_inactive_color)
                                }
                            } else {
                                if window_active {
                                    translate_color(self.0.maximize_disabled_background_active_color)
                                } else {
                                    translate_color(self.0.maximize_disabled_background_inactive_color)
                                }
                            }
                        },
                    }
                },
                WinitButton::Minimize => {
                    match button_state {
                        WinitButtonState::Idle => {
                            if foreground {
                                if window_active {
                                    translate_color(self.0.minimize_idle_foreground_active_color)
                                } else {
                                    translate_color(self.0.minimize_idle_foreground_inactive_color)
                                }
                            } else {
                                if window_active {
                                    translate_color(self.0.minimize_idle_background_active_color)
                                } else {
                                    translate_color(self.0.minimize_idle_background_inactive_color)
                                }
                            }
                        },
                        WinitButtonState::Hovered => {
                            if foreground {
                                if window_active {
                                    translate_color(self.0.minimize_hovered_foreground_active_color)
                                } else {
                                    translate_color(self.0.minimize_hovered_foreground_inactive_color)
                                }
                            } else {
                                if window_active {
                                    translate_color(self.0.minimize_hovered_background_active_color)
                                } else {
                                    translate_color(self.0.minimize_hovered_background_inactive_color)
                                }
                            }
                        },
                        WinitButtonState::Disabled => {
                            if foreground {
                                if window_active {
                                    translate_color(self.0.minimize_disabled_foreground_active_color)
                                } else {
                                    translate_color(self.0.minimize_disabled_foreground_inactive_color)
                                }
                            } else {
                                if window_active {
                                    translate_color(self.0.minimize_disabled_background_active_color)
                                } else {
                                    translate_color(self.0.minimize_disabled_background_inactive_color)
                                }
                            }
                        },
                    }
                },
                WinitButton::Close => {
                    match button_state {
                        WinitButtonState::Idle => {
                            if foreground {
                                if window_active {
                                    translate_color(self.0.close_idle_foreground_active_color)
                                } else {
                                    translate_color(self.0.close_idle_foreground_inactive_color)
                                }
                            } else {
                                if window_active {
                                    translate_color(self.0.close_idle_background_active_color)
                                } else {
                                    translate_color(self.0.close_idle_background_inactive_color)
                                }
                            }
                        },
                        WinitButtonState::Hovered => {
                            if foreground {
                                if window_active {
                                    translate_color(self.0.close_hovered_foreground_active_color)
                                } else {
                                    translate_color(self.0.close_hovered_foreground_inactive_color)
                                }
                            } else {
                                if window_active {
                                    translate_color(self.0.close_hovered_background_active_color)
                                } else {
                                    translate_color(self.0.close_hovered_background_inactive_color)
                                }
                            }
                        },
                        WinitButtonState::Disabled => {
                            if foreground {
                                if window_active {
                                    translate_color(self.0.close_disabled_foreground_active_color)
                                } else {
                                    translate_color(self.0.close_disabled_foreground_inactive_color)
                                }
                            } else {
                                if window_active {
                                    translate_color(self.0.close_disabled_background_active_color)
                                } else {
                                    translate_color(self.0.close_disabled_background_inactive_color)
                                }
                            }
                        },
                    }
                },
            }
        }
    }

    #[inline(always)]
    pub(crate) const fn translate_winit_theme(input: WinitWindowTheme) -> WindowTheme {
        match input {
            WinitWindowTheme::Dark => WindowTheme::DarkMode,
            WinitWindowTheme::Light => WindowTheme::LightMode,
        }
    }

    #[inline(always)]
    pub(crate) const fn translate_theme(input: WindowTheme) -> WinitWindowTheme {
        match input {
            WindowTheme::DarkMode => WinitWindowTheme::Dark,
            WindowTheme::LightMode => WinitWindowTheme::Light,
        }
    }

    #[inline(always)]
    pub(crate) fn translate_logical_position(input: LogicalPosition) -> WinitLogicalPosition {
        WinitLogicalPosition::new(input.x as f64, input.y as f64)
    }

    #[inline(always)]
    pub(crate) fn translate_logical_size(input: LogicalSize) -> WinitLogicalSize {
        WinitLogicalSize::new(input.width as f64, input.height as f64)
    }

    #[inline(always)]
    pub(crate) const fn translate_winit_logical_position(input: WinitLogicalPosition) -> LogicalPosition {
        LogicalPosition::new(input.x as f32, input.y as f32)
    }

    #[inline(always)]
    pub(crate) const fn translate_winit_logical_size(input: WinitLogicalSize) -> LogicalSize {
        LogicalSize::new(input.width as f32, input.height as f32)
    }

    #[inline(always)]
    pub(crate) fn translate_physical_position<T>(input: PhysicalPosition<T>) -> WinitPhysicalPosition<T> {
        WinitPhysicalPosition::new(input.x, input.y)
    }

    #[inline(always)]
    pub(crate) fn translate_physical_size<T>(input: PhysicalSize<T>) -> WinitPhysicalSize<T> {
        WinitPhysicalSize::new(input.width, input.height)
    }

    #[inline(always)]
    pub(crate) fn winit_translate_physical_position<T>(input: WinitPhysicalPosition<T>) -> PhysicalPosition<T> {
        PhysicalPosition::new(input.x, input.y)
    }

    #[inline(always)]
    pub(crate) fn winit_translate_physical_size<T>(input: WinitPhysicalSize<T>) -> PhysicalSize<T> {
        PhysicalSize::new(input.width, input.height)
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
        use azul_core::window::{SmallWindowIconBytes, LargeWindowIconBytes};
        match input {
            WindowIcon::Small(SmallWindowIconBytes { rgba_bytes, .. }) => WinitIcon::from_rgba(rgba_bytes.into_library_owned_vec(), 16, 16),
            WindowIcon::Large(LargeWindowIconBytes { rgba_bytes, .. }) => WinitIcon::from_rgba(rgba_bytes.into_library_owned_vec(), 32, 32),
        }
    }

    #[inline]
    pub(crate) fn translate_taskbar_icon(input: TaskBarIcon) -> Result<WinitIcon, WinitBadIcon> {
        WinitIcon::from_rgba(input.rgba_bytes.into_library_owned_vec(), 256, 256)
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
            WinitVirtualKeyCode::NumpadAdd => VirtualKeyCode::NumpadAdd,
            WinitVirtualKeyCode::NumpadDivide => VirtualKeyCode::NumpadDivide,
            WinitVirtualKeyCode::NumpadDecimal => VirtualKeyCode::NumpadDecimal,
            WinitVirtualKeyCode::NumpadComma => VirtualKeyCode::NumpadComma,
            WinitVirtualKeyCode::NumpadEnter => VirtualKeyCode::NumpadEnter,
            WinitVirtualKeyCode::NumpadEquals => VirtualKeyCode::NumpadEquals,
            WinitVirtualKeyCode::NumpadMultiply => VirtualKeyCode::NumpadMultiply,
            WinitVirtualKeyCode::NumpadSubtract => VirtualKeyCode::NumpadSubtract,
            WinitVirtualKeyCode::AbntC1 => VirtualKeyCode::AbntC1,
            WinitVirtualKeyCode::AbntC2 => VirtualKeyCode::AbntC2,
            WinitVirtualKeyCode::Apostrophe => VirtualKeyCode::Apostrophe,
            WinitVirtualKeyCode::Apps => VirtualKeyCode::Apps,
            WinitVirtualKeyCode::Asterisk => VirtualKeyCode::Asterisk,
            WinitVirtualKeyCode::At => VirtualKeyCode::At,
            WinitVirtualKeyCode::Ax => VirtualKeyCode::Ax,
            WinitVirtualKeyCode::Backslash => VirtualKeyCode::Backslash,
            WinitVirtualKeyCode::Calculator => VirtualKeyCode::Calculator,
            WinitVirtualKeyCode::Capital => VirtualKeyCode::Capital,
            WinitVirtualKeyCode::Colon => VirtualKeyCode::Colon,
            WinitVirtualKeyCode::Comma => VirtualKeyCode::Comma,
            WinitVirtualKeyCode::Convert => VirtualKeyCode::Convert,
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
            WinitVirtualKeyCode::Mute => VirtualKeyCode::Mute,
            WinitVirtualKeyCode::MyComputer => VirtualKeyCode::MyComputer,
            WinitVirtualKeyCode::NavigateForward => VirtualKeyCode::NavigateForward,
            WinitVirtualKeyCode::NavigateBackward => VirtualKeyCode::NavigateBackward,
            WinitVirtualKeyCode::NextTrack => VirtualKeyCode::NextTrack,
            WinitVirtualKeyCode::NoConvert => VirtualKeyCode::NoConvert,
            WinitVirtualKeyCode::OEM102 => VirtualKeyCode::OEM102,
            WinitVirtualKeyCode::Period => VirtualKeyCode::Period,
            WinitVirtualKeyCode::PlayPause => VirtualKeyCode::PlayPause,
            WinitVirtualKeyCode::Plus => VirtualKeyCode::Plus,
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
}

#[inline]
fn wr_translate_image_mask(input: DisplayListImageMask) -> WrImageMask {
    WrImageMask {
        image: wr_translate_image_key(input.image),
        rect: wr_translate_logical_rect(input.rect),
        repeat: input.repeat,
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

// webrender -> core

#[inline(always)]
pub(crate) const fn translate_id_namespace_wr(ns: WrIdNamespace) -> IdNamespace {
    IdNamespace(ns.0)
}

#[inline(always)]
pub(crate) const fn translate_pipeline_id_wr(pipeline_id: WrPipelineId) -> PipelineId {
    PipelineId(pipeline_id.0, pipeline_id.1)
}

#[inline(always)]
pub(crate) const fn translate_document_id_wr(document_id: WrDocumentId) -> DocumentId {
    DocumentId { namespace_id: translate_id_namespace_wr(document_id.namespace_id), id: document_id.id }
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

/*
#[inline]
pub(crate) fn translate_image_descriptor_wr(descriptor: WrImageDescriptor) -> ImageDescriptor {
    ImageDescriptor {
        format: translate_image_format_wr(descriptor.format),
        width: descriptor.size.width as usize,
        height: descriptor.size.height as usize,
        stride: descriptor.stride.into(),
        offset: descriptor.offset,
        flags: translate_image_descriptor_flags_wr(descriptor.flags),
    }
}

#[inline]
pub(crate) fn translate_image_descriptor_flags_wr(flags: WrImageDescriptorFlags) -> ImageDescriptorFlags {
    ImageDescriptorFlags {
        is_opaque: flags.contains(WrImageDescriptorFlags::IS_OPAQUE),
        allow_mipmaps: flags.contains(WrImageDescriptorFlags::ALLOW_MIPMAPS),
    }
}

#[inline]
pub fn translate_image_format_wr(input: WrImageFormat) -> ImageFormat {
    match input {
        WrImageFormat::R8 => ImageFormat::R8,
        WrImageFormat::R16 => ImageFormat::R16,
        WrImageFormat::RG16 => ImageFormat::RG16,
        WrImageFormat::BGRA8 => ImageFormat::BGRA8,
        WrImageFormat::RGBAF32 => ImageFormat::RGBAF32,
        WrImageFormat::RG8 => ImageFormat::RG8,
        WrImageFormat::RGBAI32 => ImageFormat::RGBAI32,
        WrImageFormat::RGBA8 => ImageFormat::RGBA8,
    }
}
*/

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
pub(crate) const fn wr_translate_document_id(document_id: DocumentId) -> WrDocumentId {
    WrDocumentId { namespace_id: wr_translate_id_namespace(document_id.namespace_id), id: document_id.id }
}

#[inline(always)]
pub(crate) fn wr_translate_logical_size(logical_size: LogicalSize) -> WrLayoutSize {
    WrLayoutSize::new(logical_size.width, logical_size.height)
}

#[inline]
pub(crate) fn wr_translate_image_descriptor(descriptor: ImageDescriptor) -> WrImageDescriptor {
    use webrender::api::units::DeviceIntSize;
    WrImageDescriptor {
        format: wr_translate_image_format(descriptor.format),
        size: DeviceIntSize::new(descriptor.width as i32, descriptor.height as i32),
        stride: descriptor.stride.into(),
        offset: descriptor.offset,
        flags: wr_translate_image_descriptor_flags(descriptor.flags),
    }
}

#[inline]
pub(crate) fn wr_translate_image_descriptor_flags(flags: ImageDescriptorFlags) -> WrImageDescriptorFlags {
    let mut f = WrImageDescriptorFlags::empty();
    f.set(WrImageDescriptorFlags::IS_OPAQUE, flags.is_opaque);
    f.set(WrImageDescriptorFlags::ALLOW_MIPMAPS, flags.allow_mipmaps);
    f
}

#[inline(always)]
pub(crate) fn wr_translate_add_font_instance(add_font_instance: AddFontInstance) -> WrAddFontInstance {
    WrAddFontInstance {
        key: wr_translate_font_instance_key(add_font_instance.key),
        font_key: wr_translate_font_key(add_font_instance.font_key),
        glyph_size: add_font_instance.glyph_size.into_px(), // note: Au is now in pixels (f32)
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
        cleartype_level: fio.cleartype_level,
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

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[inline]
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
    // TODO: re-code the image formats !

    /*
        R8,
        RG8,
        RGB8,
        RGBA8,
        R16,
        RG16,
        RGB16,
        RGBA16,
        BGR8,
        BGRA8,
    */

    match input {
        ImageFormat::R8 => WrImageFormat::R8,
        ImageFormat::RG8 => WrImageFormat::RG8,
        ImageFormat::RGBA8 => WrImageFormat::RGBA8,
        ImageFormat::R16 => WrImageFormat::R16,
        ImageFormat::RG16 => WrImageFormat::RG16,
        ImageFormat::BGRA8 => WrImageFormat::BGRA8,

        ImageFormat::RGB16 => panic!("webrender: unsupported image format RGB16"),
        ImageFormat::RGB8 => panic!("webrender: unsupported image format RGB8, need alpha channel"),
        ImageFormat::RGBA16 => panic!("webrender: unsupported image format RGBA16"),
        ImageFormat::BGR8 => panic!("webrender: unsupported image format BGR8"),
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
pub fn wr_translate_border_radius(border_radius: StyleBorderRadius, rect_size: LogicalSize) -> WrBorderRadius {

    let StyleBorderRadius { top_left, top_right, bottom_left, bottom_right } = border_radius;

    let w = rect_size.width;
    let h = rect_size.height;

    // The "w / h" is necessary to convert percentage-based values into pixels, for example "border-radius: 50%;"

    let top_left_px_h = top_left.and_then(|tl| tl.get_property_or_default()).unwrap_or_default().inner.to_pixels(w);
    let top_left_px_v = top_left.and_then(|tl| tl.get_property_or_default()).unwrap_or_default().inner.to_pixels(h);

    let top_right_px_h = top_right.and_then(|tr| tr.get_property_or_default()).unwrap_or_default().inner.to_pixels(w);
    let top_right_px_v = top_right.and_then(|tr| tr.get_property_or_default()).unwrap_or_default().inner.to_pixels(h);

    let bottom_left_px_h = bottom_left.and_then(|bl| bl.get_property_or_default()).unwrap_or_default().inner.to_pixels(w);
    let bottom_left_px_v = bottom_left.and_then(|bl| bl.get_property_or_default()).unwrap_or_default().inner.to_pixels(h);

    let bottom_right_px_h = bottom_right.and_then(|br| br.get_property_or_default()).unwrap_or_default().inner.to_pixels(w);
    let bottom_right_px_v = bottom_right.and_then(|br| br.get_property_or_default()).unwrap_or_default().inner.to_pixels(h);

    WrBorderRadius {
        top_left: WrLayoutSize::new(top_left_px_h as f32, top_left_px_v as f32),
        top_right: WrLayoutSize::new(top_right_px_h as f32, top_right_px_v as f32),
        bottom_left: WrLayoutSize::new(bottom_left_px_h as f32, bottom_left_px_v as f32),
        bottom_right: WrLayoutSize::new(bottom_right_px_h as f32, bottom_right_px_v as f32),
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
pub fn wr_translate_css_layout_rect(input: WrLayoutRect) -> CssLayoutRect {
    CssLayoutRect {
        origin: CssLayoutPoint { x: input.origin.x.round() as isize, y: input.origin.y.round() as isize },
        size: CssLayoutSize { width: input.size.width.round() as isize, height: input.size.height.round() as isize },
    }
}

#[inline]
fn wr_translate_layout_size(input: CssLayoutSize) -> WrLayoutSize {
    WrLayoutSize::new(input.width as f32, input.height as f32)
}

#[inline]
pub(crate) fn wr_translate_layout_point(input: CssLayoutPoint) -> WrLayoutPoint {
    WrLayoutPoint::new(input.x as f32, input.y as f32)
}

#[inline]
pub(crate) fn wr_translate_logical_position(input: LogicalPosition) -> WrLayoutPoint {
    WrLayoutPoint::new(input.x, input.y)
}

#[inline]
fn wr_translate_logical_rect(input: LogicalRect) -> WrLayoutRect {
    WrLayoutRect::new(wr_translate_logical_position(input.origin), wr_translate_logical_size(input.size))
}

#[inline]
fn wr_translate_layout_rect(input: CssLayoutRect) -> WrLayoutRect {
    WrLayoutRect::new(wr_translate_layout_point(input.origin), wr_translate_layout_size(input.size))
}

#[inline]
fn translate_layout_size_wr(input: WrLayoutSize) -> CssLayoutSize {
    CssLayoutSize::new(input.width.round() as isize, input.height.round() as isize)
}

#[inline]
fn translate_layout_point_wr(input: WrLayoutPoint) -> CssLayoutPoint {
    CssLayoutPoint::new(input.x.round() as isize, input.y.round() as isize)
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
fn wr_translate_primitive_flags(flags: PrimitiveFlags) -> WrPrimitiveFlags {
    let mut f = WrPrimitiveFlags::empty();
    f.set(WrPrimitiveFlags::IS_BACKFACE_VISIBLE, flags.is_backface_visible);
    f.set(WrPrimitiveFlags::IS_SCROLLBAR_CONTAINER, flags.is_scrollbar_container);
    f.set(WrPrimitiveFlags::IS_SCROLLBAR_THUMB, flags.is_scrollbar_thumb);
    f.set(WrPrimitiveFlags::PREFER_COMPOSITOR_SURFACE, flags.prefer_compositor_surface);
    f.set(WrPrimitiveFlags::SUPPORTS_EXTERNAL_COMPOSITOR_SURFACE, flags.supports_external_compositor_surface);
    f
}

#[inline]
fn translate_primitive_flags_wr(flags: WrPrimitiveFlags) -> PrimitiveFlags {
    PrimitiveFlags {
        is_backface_visible: flags.contains(WrPrimitiveFlags::IS_BACKFACE_VISIBLE),
        is_scrollbar_container: flags.contains(WrPrimitiveFlags::IS_SCROLLBAR_CONTAINER),
        is_scrollbar_thumb: flags.contains(WrPrimitiveFlags::IS_SCROLLBAR_THUMB),
        prefer_compositor_surface: flags.contains(WrPrimitiveFlags::PREFER_COMPOSITOR_SURFACE),
        supports_external_compositor_surface: flags.contains(WrPrimitiveFlags::SUPPORTS_EXTERNAL_COMPOSITOR_SURFACE),
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
    debug_flags.set(DebugFlags::ECHO_DRIVER_MESSAGES, new_flags.echo_driver_messages);
    debug_flags.set(DebugFlags::SHOW_OVERDRAW, new_flags.show_overdraw);
    debug_flags.set(DebugFlags::GPU_CACHE_DBG, new_flags.gpu_cache_dbg);
    debug_flags.set(DebugFlags::TEXTURE_CACHE_DBG_CLEAR_EVICTED, new_flags.texture_cache_dbg_clear_evicted);
    debug_flags.set(DebugFlags::PICTURE_CACHING_DBG, new_flags.picture_caching_dbg);
    debug_flags.set(DebugFlags::PRIMITIVE_DBG, new_flags.primitive_dbg);
    debug_flags.set(DebugFlags::ZOOM_DBG, new_flags.zoom_dbg);
    debug_flags.set(DebugFlags::SMALL_SCREEN, new_flags.small_screen);
    debug_flags.set(DebugFlags::DISABLE_OPAQUE_PASS, new_flags.disable_opaque_pass);
    debug_flags.set(DebugFlags::DISABLE_ALPHA_PASS, new_flags.disable_alpha_pass);
    debug_flags.set(DebugFlags::DISABLE_CLIP_MASKS, new_flags.disable_clip_masks);
    debug_flags.set(DebugFlags::DISABLE_TEXT_PRIMS, new_flags.disable_text_prims);
    debug_flags.set(DebugFlags::DISABLE_GRADIENT_PRIMS, new_flags.disable_gradient_prims);
    debug_flags.set(DebugFlags::OBSCURE_IMAGES, new_flags.obscure_images);
    debug_flags.set(DebugFlags::GLYPH_FLASHING, new_flags.glyph_flashing);
    debug_flags.set(DebugFlags::SMART_PROFILER, new_flags.smart_profiler);
    debug_flags.set(DebugFlags::INVALIDATION_DBG, new_flags.invalidation_dbg);
    debug_flags.set(DebugFlags::TILE_CACHE_LOGGING_DBG, new_flags.tile_cache_logging_dbg);
    debug_flags.set(DebugFlags::PROFILER_CAPTURE, new_flags.profiler_capture);
    debug_flags.set(DebugFlags::FORCE_PICTURE_INVALIDATION, new_flags.force_picture_invalidation);

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
    use std::sync::Arc;
    match image_data {
        ImageData::Raw(data) => WrImageData::Raw(Arc::new(data.into_library_owned_vec())),
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
    WrExternalImageId(external.inner)
}

#[inline(always)]
pub(crate) const fn translate_external_image_id_wr(external: WrExternalImageId) -> ExternalImageId {
    ExternalImageId { inner: external.0 }
}

#[inline(always)]
fn wr_translate_external_image_type(external: ExternalImageType) -> WrExternalImageType {
    match external {
        ExternalImageType::TextureHandle(tt) => WrExternalImageType::TextureHandle(wr_translate_image_buffer_kind(tt)),
        ExternalImageType::Buffer => WrExternalImageType::Buffer,
    }
}

#[inline(always)]
fn wr_translate_image_buffer_kind(buffer_kind: ImageBufferKind) -> WrImageBufferKind {
    match buffer_kind {
        ImageBufferKind::Texture2D => WrImageBufferKind::Texture2D,
        ImageBufferKind::TextureRect => WrImageBufferKind::TextureRect,
        ImageBufferKind::TextureExternal => WrImageBufferKind::TextureExternal,
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
    use webrender::api::{
        units::{
            DeviceIntRect as WrDeviceIntRect,
            DeviceIntPoint as WrDeviceIntPoint,
            DeviceIntSize as WrDeviceIntSize,
        },
        DirtyRect as WrDirtyRect,
    };
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

#[inline]
pub(crate) const fn wr_translate_transform(t: &ComputedTransform3D) -> WrLayoutTransform {
    WrLayoutTransform::new(
        t.m[0][0], t.m[0][1], t.m[0][2], t.m[0][3],
        t.m[1][0], t.m[1][1], t.m[1][2], t.m[1][3],
        t.m[2][0], t.m[2][1], t.m[2][2], t.m[2][3],
        t.m[3][0], t.m[3][1], t.m[3][2], t.m[3][3],
    )
}


#[inline(always)]
pub(crate) fn wr_translate_external_scroll_id(scroll_id: ExternalScrollId) -> WrExternalScrollId {
    WrExternalScrollId(scroll_id.0, wr_translate_pipeline_id(scroll_id.1))
}

pub(crate) fn wr_translate_display_list(
    input: CachedDisplayList,
    pipeline_id: PipelineId,
    current_hidpi_factor: f32,
) -> WrBuiltDisplayList {
    let root_space_and_clip = WrSpaceAndClipInfo::root_scroll(wr_translate_pipeline_id(pipeline_id));
    let mut positioned_items = Vec::new();
    let mut builder = WrDisplayListBuilder::new(wr_translate_pipeline_id(pipeline_id));
    push_display_list_msg(&mut builder, input.root, root_space_and_clip.spatial_id, root_space_and_clip.clip_id, &mut positioned_items, current_hidpi_factor);
    let (_pipeline_id, built_display_list) = builder.finalize();
    built_display_list
}

#[inline]
fn push_display_list_msg(
    builder: &mut WrDisplayListBuilder,
    msg: DisplayListMsg,
    parent_spatial_id: WrSpatialId,
    parent_clip_id: WrClipId,
    positioned_items: &mut Vec<(WrSpatialId, WrClipId)>,
    current_hidpi_factor: f32,
) {
    use azul_core::display_list::DisplayListMsg::*;
    use azul_core::ui_solver::PositionInfo::*;
    use webrender::api::PropertyBindingKey as WrPropertyBindingKey;

    let msg_position = msg.get_position();

    let relative_x;
    let relative_y;

    let (parent_spatial_id, parent_clip_id) = match msg_position {
        Static { x_offset, y_offset, .. } | Relative { x_offset, y_offset, .. } => {
            relative_x = x_offset;
            relative_y = y_offset;
            (parent_spatial_id, parent_clip_id)
        },
        Absolute { x_offset, y_offset, .. } => {
            let (last_positioned_spatial_id, last_positioned_clip_id) = positioned_items.last().copied()
            .unwrap_or((WrSpatialId::root_scroll_node(builder.pipeline_id), WrClipId::root(builder.pipeline_id)));
            relative_x = x_offset;
            relative_y = y_offset;
            (last_positioned_spatial_id, last_positioned_clip_id)
        },
        Fixed { x_offset, y_offset, .. } => {
            relative_x = x_offset;
            relative_y = y_offset;
            (WrSpatialId::root_scroll_node(builder.pipeline_id), WrClipId::root(builder.pipeline_id))
        },
    };

    // All rectangles are transformed in relation to the parent node,
    // so we have to push the parent as a "reference frame", optionally
    // adding an (animatable) transformation on top
    let transform = msg.get_transform_key();
    let should_push_stacking_context = transform.is_some();

    let property_binding = match transform {
        Some(s) => WrPropertyBinding::Binding(
            WrPropertyBindingKey::new(s.0.id as u64), wr_translate_transform(&s.1)
        ),
        None => WrPropertyBinding::Value(WrLayoutTransform::identity()),
    };

    let rect_spatial_id = builder.push_reference_frame(
        WrLayoutPoint::new(relative_x, relative_y),
        parent_spatial_id,
        WrTransformStyle::Flat,
        property_binding,
        WrReferenceFrameKind::Transform {
            is_2d_scale_translation: false,
            should_snap: false,
        },
    );

    if msg_position.is_positioned() {
        positioned_items.push((rect_spatial_id, parent_clip_id));
    }

    if should_push_stacking_context {
        builder.push_simple_stacking_context_with_filters(
            WrLayoutPoint::zero(),
            rect_spatial_id,
            WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
            &[], // TODO: opacity!
            &[],
            &[]
        );
    }

    match msg {
        Frame(f) => push_frame(builder, f, rect_spatial_id, parent_clip_id, positioned_items, current_hidpi_factor),
        ScrollFrame(sf) => push_scroll_frame(builder, sf, rect_spatial_id, parent_clip_id, positioned_items, current_hidpi_factor),
    }

    if msg_position.is_positioned() {
        positioned_items.pop();
    }

    if should_push_stacking_context {
        builder.pop_stacking_context();
    }

    builder.pop_reference_frame();
}

#[inline]
fn push_frame(
    builder: &mut WrDisplayListBuilder,
    frame: DisplayListFrame,
    rect_spatial_id: WrSpatialId,
    parent_clip_id: WrClipId,
    positioned_items: &mut Vec<(WrSpatialId, WrClipId)>,
    current_hidpi_factor: f32,
) {

    let rect = LogicalRect::new(LogicalPosition::zero(), frame.size);
    let wr_border_radius = wr_translate_border_radius(frame.border_radius, frame.size);
    let clip_content_id = define_border_radius_clip(builder, rect, wr_border_radius, rect_spatial_id, parent_clip_id);

    for item in frame.content {
        push_display_list_content(
            builder,
            &frame.box_shadow,
            item,
            rect,
            frame.border_radius,
            frame.tag,
            frame.flags,
            clip_content_id,
            rect_spatial_id,
            current_hidpi_factor,
        );
    }

    let wr_border_radius = wr_translate_border_radius(frame.border_radius, frame.size);

    // If the rect has an overflow:* property set, clip the children accordingly
    let children_clip_id = match frame.clip_children {
        Some(size) => {
            let clip_rect = LogicalRect::new(LogicalPosition::zero(), size);
            define_border_radius_clip(builder, clip_rect, wr_border_radius, rect_spatial_id, parent_clip_id)
        },
        None => WrClipId::root(builder.pipeline_id), // no clipping
    };

    // if let Some(image_mask) -> define_image_mask_clip()
    for child in frame.children {
        push_display_list_msg(builder, child, rect_spatial_id, children_clip_id, positioned_items, current_hidpi_factor);
    }
}

#[inline]
fn push_scroll_frame(
    builder: &mut WrDisplayListBuilder,
    scroll_frame: DisplayListScrollFrame,
    rect_spatial_id: WrSpatialId,
    parent_clip_id: WrClipId,
    positioned_items: &mut Vec<(WrSpatialId, WrClipId)>,
    current_hidpi_factor: f32,
) {
    use azul_css::ColorU;
    use webrender::api::{
        ClipMode as WrClipMode,
        ScrollSensitivity as WrScrollSensitivity,
        ComplexClipRegion as WrComplexClipRegion,
    };

    // if let Some(image_mask) = scroll_frame.frame.image_mask { push_image_mask_clip() }

    let rect = LogicalRect::new(LogicalPosition::zero(), scroll_frame.frame.size);
    let wr_border_radius = wr_translate_border_radius(scroll_frame.frame.border_radius, scroll_frame.frame.size);
    let clip_content_id = define_border_radius_clip(builder, rect, wr_border_radius, rect_spatial_id, parent_clip_id);

    // Only children should scroll, not the frame itself!
    for item in scroll_frame.frame.content {
        push_display_list_content(
            builder,
            &scroll_frame.frame.box_shadow,
            item,
            rect,
            scroll_frame.frame.border_radius,
            scroll_frame.frame.tag,
            scroll_frame.frame.flags,
            clip_content_id,
            rect_spatial_id,
            current_hidpi_factor,
        );
    }

    // Push hit-testing + scrolling children

    /*
    let hit_testing_clip_id = define_border_radius_clip(builder, clip_rect, wr_border_radius, rect_spatial_id, parent_clip_id);

    let hit_test_info = WrCommonItemProperties {
        clip_rect: wr_translate_logical_rect(clip_rect),
        flags: wr_translate_primitive_flags(scroll_frame.frame.flags),
        clip_id: hit_testing_clip_id,
        spatial_id: parent_spatial_id,
    };

    builder.push_rect(&hit_test_info, hit_test_info.clip_rect, wr_translate_color_u(ColorU::TRANSPARENT).into());

    // scroll frame has the hit-testing clip as a parent
    let scroll_frame_clip_info = builder.define_scroll_frame(
        /* parent_space_and_clip */ &WrSpaceAndClipInfo {
            clip_id: hit_testing_clip_id,
            spatial_id: parent_spatial_id,
        },
        /* external_id */ wr_translate_external_scroll_id(scroll_frame.scroll_id),
        /* content_rect */ wr_translate_layout_rect(scroll_frame.content_rect),
        /* clip_rect */ wr_translate_logical_rect(clip_rect),
        /* sensitivity */ WrScrollSensitivity::Script,
        /* external_scroll_offset */ WrLayoutVector2D::zero(),
    );
    */

    // If the rect has an overflow:* property set, clip the children accordingly
    let children_clip_id = match scroll_frame.frame.clip_children {
        Some(size) => {
            let wr_border_radius = wr_translate_border_radius(scroll_frame.frame.border_radius, scroll_frame.frame.size);
            let clip_rect = LogicalRect::new(LogicalPosition::zero(), size);
            define_border_radius_clip(builder, clip_rect, wr_border_radius, rect_spatial_id, parent_clip_id)
        },
        None => WrClipId::root(builder.pipeline_id), // no clipping
    };

    for child in scroll_frame.frame.children {
        push_display_list_msg(builder, child, rect_spatial_id, children_clip_id, positioned_items, current_hidpi_factor);
    }
}

fn define_border_radius_clip(
    builder: &mut WrDisplayListBuilder,
    layout_rect: LogicalRect,
    wr_border_radius: WrBorderRadius,
    rect_spatial_id: WrSpatialId,
    parent_clip_id: WrClipId,
) -> WrClipId {

    use webrender::api::{
        ClipMode as WrClipMode,
        ComplexClipRegion as WrComplexClipRegion,
    };

    // NOTE: only translate the size, position is always (0.0, 0.0)
    let wr_layout_size = wr_translate_logical_size(layout_rect.size);
    let wr_layout_rect = WrLayoutRect::new(WrLayoutPoint::zero(), wr_layout_size);
    builder.define_clip_rounded_rect( // TODO: optimize - if border radius = 0,
        &WrSpaceAndClipInfo { spatial_id: rect_spatial_id, clip_id: parent_clip_id },
        WrComplexClipRegion::new(wr_layout_rect, wr_border_radius, WrClipMode::Clip),
    )
}

#[inline]
fn push_display_list_content(
    builder: &mut WrDisplayListBuilder,
    box_shadow: &Option<BoxShadow>,
    content: LayoutRectContent,
    clip_rect: LogicalRect,
    border_radius: StyleBorderRadius,
    hit_info: Option<TagId>,
    flags: PrimitiveFlags,
    parent_clip_id: WrClipId,
    rect_spatial_id: WrSpatialId,
    current_hidpi_factor: f32,
) {
    use azul_core::display_list::LayoutRectContent::*;

    let mut normal_info = WrCommonItemProperties {
        clip_rect: wr_translate_logical_rect(clip_rect),
        clip_id: parent_clip_id,
        spatial_id: rect_spatial_id,
        flags: wr_translate_primitive_flags(flags),
    };

    let wr_border_radius = wr_translate_border_radius(border_radius, clip_rect.size);

    if let Some(box_shadow) = box_shadow.as_ref() {
        // push outset box shadow before the item clip is pushed
        if box_shadow.clip_mode == CssBoxShadowClipMode::Outset {
            // If the content is a shadow, it needs to be clipped by the root
            normal_info.clip_id = WrClipId::root(builder.pipeline_id);
            box_shadow::push_box_shadow(builder, clip_rect, CssBoxShadowClipMode::Outset, box_shadow, border_radius, normal_info.spatial_id, normal_info.clip_id);
        }
    }

    // Border and BoxShadow::Outset get a root clip, since they
    // are outside of the rect contents
    // All other content types get the regular clip
    match content {
        Text { glyphs, font_instance_key, color, glyph_options, overflow } => {
            let border_radius_clip_id = if overflow.0 || overflow.1 {
                WrClipId::root(builder.pipeline_id)
            } else {
                define_border_radius_clip(builder, clip_rect, wr_border_radius, rect_spatial_id, parent_clip_id)
            };
            normal_info.clip_id = border_radius_clip_id;
            text::push_text(builder, &normal_info, glyphs, font_instance_key, color, glyph_options);
        },
        Background { content, size, offset, repeat  } => {
            let border_radius_clip_id = define_border_radius_clip(builder, clip_rect, wr_border_radius, rect_spatial_id, parent_clip_id);
            normal_info.clip_id = border_radius_clip_id;
            background::push_background(builder, &normal_info, content, size, offset, repeat);
        },
        Image { size, offset, image_rendering, alpha_type, image_key, background_color } => {
            let border_radius_clip_id = define_border_radius_clip(builder, clip_rect, wr_border_radius, rect_spatial_id, parent_clip_id);
            normal_info.clip_id = border_radius_clip_id;
            image::push_image(builder, &normal_info, size, offset, image_key, alpha_type, image_rendering, background_color);
        },
        Border { widths, colors, styles } => {
            // no clip necessary because item will always be in parent bounds
            border::push_border(builder, &normal_info, border_radius, widths, colors, styles, current_hidpi_factor);
        },
    }

    if let Some(box_shadow) = box_shadow.as_ref() {
        // push outset box shadow before the item clip is pushed
        if box_shadow.clip_mode == CssBoxShadowClipMode::Inset {
            let border_radius_clip_id = define_border_radius_clip(builder, clip_rect, wr_border_radius, rect_spatial_id, parent_clip_id);
            normal_info.clip_id = border_radius_clip_id;
            box_shadow::push_box_shadow(builder, clip_rect, CssBoxShadowClipMode::Inset, box_shadow, border_radius, normal_info.spatial_id, normal_info.clip_id);
        }
    }
}

mod text {

    use webrender::api::{
        DisplayListBuilder as WrDisplayListBuilder,
        CommonItemProperties as WrCommonItemProperties,
    };
    use azul_core::{
        app_resources::{FontInstanceKey, GlyphOptions},
        display_list::GlyphInstance,
        window::LogicalSize,
    };
    use azul_css::ColorU;

    pub(in super) fn push_text(
         builder: &mut WrDisplayListBuilder,
         info: &WrCommonItemProperties,
         glyphs: Vec<GlyphInstance>,
         font_instance_key: FontInstanceKey,
         color: ColorU,
         glyph_options: Option<GlyphOptions>,
    ) {
        use super::{
            wr_translate_layouted_glyphs, wr_translate_font_instance_key,
            wr_translate_color_u, wr_translate_glyph_options,
        };

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
        units::{
            LayoutSize as WrLayoutSize,
            LayoutRect as WrLayoutRect,
        },
        DisplayListBuilder as WrDisplayListBuilder,
        CommonItemProperties as WrCommonItemProperties,
        GradientStop as WrGradientStop,
    };
    use azul_css::{
        StyleBackgroundSize, StyleBackgroundPosition, StyleBackgroundRepeat,
        RadialGradient, LinearGradient, ConicGradient, ColorU, LayoutSize, LayoutPoint,
    };
    use azul_core::{
        display_list::RectBackground,
        window::{LogicalSize, LogicalPosition},
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
    ) {
        use azul_core::display_list::RectBackground::*;

        let content_size = background.get_content_size();

        match background {
            LinearGradient(g)    => push_linear_gradient_background(builder, &info, g, background_position, background_size, content_size),
            RadialGradient(rg)   => push_radial_gradient_background(builder, &info, rg, background_position, background_size, content_size),
            ConicGradient(cg)    => push_conic_gradient_background(builder, &info, cg, background_position, background_size, content_size),
            Image((key, _))    => push_image_background(builder, &info, key, background_position, background_size, background_repeat, content_size),
            Color(col)           => push_color_background(builder, &info, col, background_position, background_size, background_repeat, content_size),
        }
    }

    fn push_conic_gradient_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        conic_gradient: ConicGradient,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        content_size: Option<(f32, f32)>,
    ) {
        use webrender::api::units::LayoutPoint as WrLayoutPoint;
        use super::{wr_translate_color_u, wr_translate_logical_size, wr_translate_extend_mode};

        let width = info.clip_rect.size.width.round();
        let height = info.clip_rect.size.height.round();
        let background_position = background_position.unwrap_or_default();
        let background_size = calculate_background_size(info, background_size, content_size);
        let offset = calculate_background_position(width, height, background_position, background_size);

        let mut offset_info = *info;
        offset_info.clip_rect.origin.x += offset.x;
        offset_info.clip_rect.origin.y += offset.y;

        let radial_gradient_normalized = conic_gradient.stops.get_normalized_radial_stops();

        let stops: Vec<WrGradientStop> = radial_gradient_normalized.iter().map(|gradient_pre|
            WrGradientStop {
                offset: gradient_pre.angle.to_degrees(),
                color: wr_translate_color_u(gradient_pre.color).into(),
            }
        ).collect();

        let center = calculate_background_position(width, height, conic_gradient.center, background_size);
        let center = WrLayoutPoint::new(center.x, center.y);

        let gradient = builder.create_conic_gradient(
            center,
            conic_gradient.angle.to_degrees(),
            stops,
            wr_translate_extend_mode(conic_gradient.extend_mode)
        );

        builder.push_conic_gradient(
            &offset_info,
            offset_info.clip_rect,
            gradient,
            wr_translate_logical_size(background_size),
            WrLayoutSize::zero()
        );
    }

    fn push_radial_gradient_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        radial_gradient: RadialGradient,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        content_size: Option<(f32, f32)>,
    ) {
        use azul_css::Shape;
        use super::{wr_translate_color_u, wr_translate_logical_size, wr_translate_extend_mode};
        use webrender::api::units::LayoutPoint as WrLayoutPoint;

        let width = info.clip_rect.size.width.round();
        let height = info.clip_rect.size.height.round();
        let background_position = background_position.unwrap_or_default();
        let background_size = calculate_background_size(info, background_size, content_size);
        let offset = calculate_background_position(width, height, background_position, background_size);

        let mut offset_info = *info;
        offset_info.clip_rect.origin.x += offset.x;
        offset_info.clip_rect.origin.y += offset.y;

        let center = calculate_background_position(width, height, radial_gradient.position, background_size);
        let center = WrLayoutPoint::new(center.x, center.y);
        let linear_gradient_normalized = radial_gradient.stops.get_normalized_linear_stops();

        let stops: Vec<WrGradientStop> = linear_gradient_normalized.iter().map(|gradient_pre|
            WrGradientStop {
                offset: gradient_pre.offset.get() / 100.0,
                color: wr_translate_color_u(gradient_pre.color).into(),
            }
        ).collect();

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
            wr_translate_logical_size(background_size),
            WrLayoutSize::zero()
        );
    }

    fn push_linear_gradient_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        linear_gradient: LinearGradient,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        content_size: Option<(f32, f32)>,
    ) {
        use super::{
            wr_translate_color_u, wr_translate_extend_mode,
            wr_translate_logical_size, wr_translate_css_layout_rect,
            wr_translate_layout_point,
        };

        let background_position = background_position.unwrap_or_default();
        let background_size = calculate_background_size(info, background_size, content_size);
        let offset = calculate_background_position(info.clip_rect.size.width.round(), info.clip_rect.size.height.round(), background_position, background_size);

        let mut offset_info = *info;
        offset_info.clip_rect.origin.x += offset.x;
        offset_info.clip_rect.origin.y += offset.y;

        let linear_gradient_normalized = linear_gradient.stops.get_normalized_linear_stops();

        let stops: Vec<WrGradientStop> = linear_gradient_normalized.iter().map(|gradient_pre|
            WrGradientStop {
                offset: gradient_pre.offset.get() / 100.0,
                color: wr_translate_color_u(gradient_pre.color).into(),
            }
        ).collect();

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
            wr_translate_logical_size(background_size),
            WrLayoutSize::zero()
        );
    }

    fn push_image_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        image_key: ImageKey,
        background_position: Option<StyleBackgroundPosition>,
        background_size: Option<StyleBackgroundSize>,
        background_repeat: Option<StyleBackgroundRepeat>,
        content_size: Option<(f32, f32)>,
    ) {
        use azul_core::display_list::{AlphaType, ImageRendering};

        let background_position = background_position.unwrap_or_default();
        let background_repeat = background_repeat.unwrap_or_default();
        let background_size = calculate_background_size(info, background_size, content_size);
        let background_position = calculate_background_position(info.clip_rect.size.width.round(), info.clip_rect.size.height.round(), background_position, background_size);
        let background_repeat_info = get_background_repeat_info(info, background_repeat, background_size);

        // TODO: customize this for image backgrounds?
        let alpha_type = AlphaType::PremultipliedAlpha;
        let image_rendering = ImageRendering::Auto;
        let background_color = ColorU { r: 0, g: 0, b: 0, a: 255 };

        image::push_image(builder, &background_repeat_info, background_size, background_position, image_key, alpha_type, image_rendering, background_color);
    }

    fn push_color_background(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
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
        let offset = calculate_background_position(info.clip_rect.size.width.round(), info.clip_rect.size.height.round(), background_position, background_size);

        let mut offset_info = *info;
        offset_info.clip_rect.origin.x += offset.x;
        offset_info.clip_rect.origin.y += offset.y;
        offset_info.clip_rect.size.width = background_size.width;
        offset_info.clip_rect.size.height = background_size.height;

        builder.push_rect(
            &offset_info,
            offset_info.clip_rect,
            wr_translate_color_u(color).into()
        );
    }

    fn get_background_repeat_info(
        info: &WrCommonItemProperties,
        background_repeat: StyleBackgroundRepeat,
        background_size: LogicalSize,
    ) -> WrCommonItemProperties {

        use azul_css::StyleBackgroundRepeat::*;

        match background_repeat {
            NoRepeat => WrCommonItemProperties {
                clip_rect: WrLayoutRect::new(
                    info.clip_rect.origin,
                    WrLayoutSize::new(background_size.width, background_size.height),
                ),
                .. *info
            },
            Repeat => *info,
            RepeatX => WrCommonItemProperties {
                clip_rect: WrLayoutRect::new(
                    info.clip_rect.origin,
                    WrLayoutSize::new(info.clip_rect.size.width, background_size.height),
                ),
                .. *info
            },
            RepeatY => WrCommonItemProperties {
                clip_rect: WrLayoutRect::new(
                    info.clip_rect.origin,
                    WrLayoutSize::new(background_size.width, info.clip_rect.size.height),
                ),
                .. *info
            },
        }
    }

    /// Transform a background size such as "cover" or "contain" into actual pixels
    fn calculate_background_size(
        info: &WrCommonItemProperties,
        bg_size: Option<StyleBackgroundSize>,
        content_size: Option<(f32, f32)>,
    ) -> LogicalSize {

        let content_size = content_size.unwrap_or((info.clip_rect.size.width, info.clip_rect.size.height));

        let bg_size = match bg_size {
            None => return LogicalSize::new(content_size.0, content_size.1),
            Some(s) => s,
        };

        let content_aspect_ratio = Ratio {
            width: info.clip_rect.size.width / content_size.0,
            height: info.clip_rect.size.height / content_size.1,
        };

        let ratio = match bg_size {
            StyleBackgroundSize::ExactSize([w, h]) => {
                let w = w.to_pixels(info.clip_rect.size.width);
                let h = h.to_pixels(info.clip_rect.size.height);
                w.min(h)
            },
            StyleBackgroundSize::Contain => content_aspect_ratio.width.min(content_aspect_ratio.height),
            StyleBackgroundSize::Cover => content_aspect_ratio.width.max(content_aspect_ratio.height),
        };

        LogicalSize::new(content_size.0 * ratio, content_size.1 * ratio)
    }

    /// Transforma background-position attribute into pixel coordinates
    fn calculate_background_position(
        width: f32,
        height: f32,
        background_position: StyleBackgroundPosition,
        background_size: LogicalSize,
    ) -> LogicalPosition {

        use azul_css::BackgroundPositionVertical;
        use azul_css::BackgroundPositionHorizontal;

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

        LogicalPosition { x: horizontal_offset, y: vertical_offset }
    }
}

mod image {

    use webrender::api::{
        DisplayListBuilder as WrDisplayListBuilder,
        CommonItemProperties as WrCommonItemProperties,
    };
    use azul_css::{LayoutPoint, LayoutSize, ColorU};
    use azul_core::{
        app_resources::ImageKey,
        window::{LogicalSize, LogicalPosition},
        display_list::{AlphaType, ImageRendering},
    };

    #[inline]
    pub(in super) fn push_image(
        builder: &mut WrDisplayListBuilder,
        info: &WrCommonItemProperties,
        size: LogicalSize,
        offset: LogicalPosition,
        image_key: ImageKey,
        alpha_type: AlphaType,
        image_rendering: ImageRendering,
        background_color: ColorU,
    ) {
        use super::{
            wr_translate_image_rendering, wr_translate_alpha_type,
            wr_translate_color_u, wr_translate_image_key, wr_translate_logical_size,
        };
        use webrender::api::units::LayoutSize as WrLayoutSize;

        let mut offset_info = *info;
        offset_info.clip_rect.origin.x += offset.x;
        offset_info.clip_rect.origin.y += offset.y;

        let tile_spacing = WrLayoutSize::zero();

        builder.push_repeating_image(
            &offset_info,
            offset_info.clip_rect,
            wr_translate_logical_size(size),
            tile_spacing,
            wr_translate_image_rendering(image_rendering),
            wr_translate_alpha_type(alpha_type),
            wr_translate_image_key(image_key),
            wr_translate_color_u(background_color).into(),
        );
    }
}

mod box_shadow {

    use azul_css::{BoxShadowClipMode, LayoutRect, ColorF, StyleBoxShadow};
    use azul_core::{
        display_list::{BoxShadow, StyleBorderRadius},
        window::LogicalRect,
        window::LogicalSize,
    };
    use webrender::api::{
        ClipId as WrClipId,
        SpatialId as WrSpatialId,
        CommonItemProperties as WrCommonItemProperties,
        DisplayListBuilder as WrDisplayListBuilder,
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
        bounds: LogicalRect,
        shadow_type: BoxShadowClipMode,
        box_shadow: &BoxShadow,
        border_radius: StyleBorderRadius,
        parent_spatial_id: WrSpatialId,
        parent_clip_id: WrClipId,
    ) {
        use self::ShouldPushShadow::*;
        use azul_css::CssPropertyValue;

        let BoxShadow { clip_mode, top, left, bottom, right } = box_shadow;

        fn translate_shadow_side(input: &Option<CssPropertyValue<StyleBoxShadow>>) -> Option<StyleBoxShadow> {
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
                    &top, &bottom, &left, &right, parent_spatial_id, parent_clip_id,
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
                            &top, &None, &None, &None, parent_spatial_id, parent_clip_id,
                        );
                        push_single_box_shadow_edge(
                            builder, &b, bounds, border_radius, shadow_type,
                            &None, &bottom, &None, &None, parent_spatial_id, parent_clip_id,
                        );
                    },
                    // left + right box-shadow pair
                    (None, Some(l), None, Some(r)) => {
                        push_single_box_shadow_edge(
                            builder, &l, bounds, border_radius, shadow_type,
                            &None, &None, &left, &None, parent_spatial_id, parent_clip_id,
                        );
                        push_single_box_shadow_edge(
                            builder, &r, bounds, border_radius, shadow_type,
                            &None, &None, &None, &right, parent_spatial_id, parent_clip_id,
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
                    parent_spatial_id,
                    parent_clip_id,
                );
            }
        }
    }

    #[inline]
    #[allow(clippy::collapsible_if)]
    fn push_single_box_shadow_edge(
        builder: &mut WrDisplayListBuilder,
        current_shadow: &StyleBoxShadow,
        bounds: LogicalRect,
        border_radius: StyleBorderRadius,
        shadow_type: BoxShadowClipMode,
        top: &Option<StyleBoxShadow>,
        bottom: &Option<StyleBoxShadow>,
        left: &Option<StyleBoxShadow>,
        right: &Option<StyleBoxShadow>,
        parent_spatial_id: WrSpatialId,
        parent_clip_id: WrClipId,
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
            parent_spatial_id,
            parent_clip_id,
        );
    }

    #[inline]
    fn push_box_shadow_inner(
        builder: &mut WrDisplayListBuilder,
        pre_shadow: StyleBoxShadow,
        border_radius: StyleBorderRadius,
        bounds: LogicalRect,
        clip_rect: LogicalRect,
        shadow_type: BoxShadowClipMode,
        parent_spatial_id: WrSpatialId,
        parent_clip_id: WrClipId,
    ) {
        use webrender::api::{PrimitiveFlags as WrPrimitiveFlags, units::LayoutVector2D};
        use super::{
            wr_translate_color_f, wr_translate_border_radius,
            wr_translate_box_shadow_clip_mode, wr_translate_logical_rect,
        };

        // The pre_shadow is missing the StyleBorderRadius & LayoutRect
        if pre_shadow.clip_mode != shadow_type {
            return;
        }

        let info = WrCommonItemProperties {
            clip_rect: wr_translate_logical_rect(clip_rect),
            spatial_id: parent_spatial_id,
            clip_id: parent_clip_id,
            flags: WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
        };

        builder.push_box_shadow(
            &info,
            wr_translate_logical_rect(bounds),
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

    fn get_clip_rect(pre_shadow: &StyleBoxShadow, bounds: LogicalRect) -> LogicalRect {
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
        units::LayoutSideOffsets as WrLayoutSideOffsets,
        DisplayListBuilder as WrDisplayListBuilder,
        BorderDetails as WrBorderDetails,
        CommonItemProperties as WrCommonItemProperties,
        BorderStyle as WrBorderStyle,
        BorderSide as WrBorderSide,
    };
    use azul_css::{
        LayoutSize, BorderStyle, BorderStyleNoNone, CssPropertyValue, PixelValue
    };
    use azul_core::{
        display_list::{StyleBorderRadius, StyleBorderWidths, StyleBorderColors, StyleBorderStyles},
        window::LogicalSize,
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
        current_hidpi_factor: f32,
    ) {
        let rect_size = LogicalSize::new(info.clip_rect.size.width, info.clip_rect.size.height);

        if let Some((border_widths, border_details)) = get_webrender_border(rect_size, radii, widths, colors, styles, current_hidpi_factor) {
            builder.push_border(&info, info.clip_rect, border_widths, border_details);
        }
    }

    /// Returns the merged offsets and details for the top, left,
    /// right and bottom styles - necessary, so we can combine `border-top`,
    /// `border-left`, etc. into one border
    fn get_webrender_border(
        rect_size: LogicalSize,
        radii: StyleBorderRadius,
        widths: StyleBorderWidths,
        colors: StyleBorderColors,
        styles: StyleBorderStyles,
        hidpi: f32,
    ) -> Option<(WrLayoutSideOffsets, WrBorderDetails)> {

        use super::{wr_translate_color_u, wr_translate_border_radius};
        use webrender::api::{
            NormalBorder as WrNormalBorder,
            BorderRadius as WrBorderRadius,
        };

        let (width_top, width_right, width_bottom, width_left) = (
            widths.top.map(|w| w.map_property(|w| w.inner)).and_then(CssPropertyValue::get_property_or_default),
            widths.right.map(|w| w.map_property(|w| w.inner)).and_then(CssPropertyValue::get_property_or_default),
            widths.bottom.map(|w| w.map_property(|w| w.inner)).and_then(CssPropertyValue::get_property_or_default),
            widths.left.map(|w| w.map_property(|w| w.inner)).and_then(CssPropertyValue::get_property_or_default),
        );

        let (style_top, style_right, style_bottom, style_left) = (
            get_border_style_normalized(styles.top.map(|s| s.map_property(|s| s.inner))),
            get_border_style_normalized(styles.right.map(|s| s.map_property(|s| s.inner))),
            get_border_style_normalized(styles.bottom.map(|s| s.map_property(|s| s.inner))),
            get_border_style_normalized(styles.left.map(|s| s.map_property(|s| s.inner))),
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

        // NOTE: if the HiDPI factor is not set to an even number, this will result
        // in uneven border widths. In order to reduce this bug, we multiply the border width
        // with the HiDPI factor, then round the result (to get an even number), then divide again
        let border_widths = WrLayoutSideOffsets::new(
            width_top.map(|v| (v.to_pixels(rect_size.height) * hidpi).floor() / hidpi).unwrap_or(0.0),
            width_right.map(|v| (v.to_pixels(rect_size.width) * hidpi).floor() / hidpi).unwrap_or(0.0),
            width_bottom.map(|v| (v.to_pixels(rect_size.height) * hidpi).floor() / hidpi).unwrap_or(0.0),
            width_left.map(|v| (v.to_pixels(rect_size.width) * hidpi).floor() / hidpi).unwrap_or(0.0),
        );

        let border_details = WrBorderDetails::Normal(WrNormalBorder {
            top:    WrBorderSide { color: wr_translate_color_u(color_top.inner).into(), style: translate_wr_border(style_top, width_top) },
            left:   WrBorderSide { color: wr_translate_color_u(color_left.inner).into(), style: translate_wr_border(style_left, width_left) },
            right:  WrBorderSide { color: wr_translate_color_u(color_right.inner).into(), style: translate_wr_border(style_right, width_right) },
            bottom: WrBorderSide { color: wr_translate_color_u(color_bottom.inner).into(), style: translate_wr_border(style_bottom, width_bottom) },
            radius: if has_no_border_radius { WrBorderRadius::zero() } else { wr_translate_border_radius(radii, rect_size) },
            do_aa: true, // it isn't known when it's possible to set this to false
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
