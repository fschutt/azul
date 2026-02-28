#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
use alloc::{boxed::Box, collections::btree_map::BTreeMap, vec::Vec};
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::__m256;
use core::{
    fmt,
    sync::atomic::{AtomicBool, Ordering as AtomicOrdering},
};

use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{
            ColorU as StyleColorU, LayoutPoint, LayoutRect, LayoutRectVec, LayoutSize, PixelValue,
            StyleFontSize,
        },
        layout::{
            LayoutBoxSizing, LayoutDisplay, LayoutFlexDirection, LayoutFloat, LayoutHeight,
            LayoutInsetBottom, LayoutJustifyContent, LayoutLeft, LayoutMarginBottom,
            LayoutMarginLeft, LayoutMarginRight, LayoutMarginTop, LayoutMaxHeight, LayoutMaxWidth,
            LayoutMinHeight, LayoutMinWidth, LayoutOverflow, LayoutPaddingBottom,
            LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop, LayoutPosition, LayoutRight,
            LayoutTop, LayoutWidth,
        },
        style::{
            LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
            LayoutBorderTopWidth, OptionStyleTextAlign, StyleBoxShadow, StyleTextAlign,
            StyleTextColor, StyleTransform, StyleTransformOrigin, StyleVerticalAlign,
        },
    },
    AzString, OptionF32,
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::{HidpiAdjustedBounds, VirtualizedViewCallbackInfo, VirtualizedViewCallbackReturn},
    dom::{DomId, DomNodeHash},
    geom::{LogicalPosition, LogicalRect, LogicalRectVec, LogicalSize},
    gl::OptionGlContextPtr,
    gpu::GpuEventChanges,
    hit_test::{ExternalScrollId, HitTestItem, ScrollHitTestItem, ScrollStates, ScrolledNodes},
    id::{NodeDataContainer, NodeDataContainerRef, NodeId},
    resources::{
        Epoch, FontInstanceKey, GlTextureCache, IdNamespace, ImageCache, OpacityKey,
        RendererResources, TransformKey, UpdateImageResult,
    },
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::{OptionChar, WindowSize, WindowTheme},
};

pub const DEFAULT_FONT_SIZE_PX: isize = 16;
pub const DEFAULT_FONT_SIZE: StyleFontSize = StyleFontSize {
    inner: PixelValue::const_px(DEFAULT_FONT_SIZE_PX),
};
pub const DEFAULT_FONT_ID: &str = "serif";
pub const DEFAULT_TEXT_COLOR: StyleTextColor = StyleTextColor {
    inner: StyleColorU {
        r: 0,
        b: 0,
        g: 0,
        a: 255,
    },
};
pub const DEFAULT_LINE_HEIGHT: f32 = 1.0;
pub const DEFAULT_WORD_SPACING: f32 = 1.0;
pub const DEFAULT_LETTER_SPACING: f32 = 0.0;
pub const DEFAULT_TAB_WIDTH: f32 = 4.0;

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ResolvedOffsets {
    pub top: f32,
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
}

impl ResolvedOffsets {
    pub const fn zero() -> Self {
        Self {
            top: 0.0,
            left: 0.0,
            right: 0.0,
            bottom: 0.0,
        }
    }
    pub fn total_vertical(&self) -> f32 {
        self.top + self.bottom
    }
    pub fn total_horizontal(&self) -> f32 {
        self.left + self.right
    }
}

pub type GlyphIndex = u32;

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
pub struct GlyphInstance {
    pub index: GlyphIndex,
    pub point: LogicalPosition,
    pub size: LogicalSize,
}

impl GlyphInstance {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.point.scale_for_dpi(scale_factor);
        self.size.scale_for_dpi(scale_factor);
    }
}
