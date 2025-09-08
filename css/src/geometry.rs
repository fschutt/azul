//! Geometric primitives used for layout calculations

use core::fmt;

/// Rectangle type for layout calculations
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutRect {
    pub origin: LayoutPoint,
    pub size: LayoutSize,
}

/// Size type for layout calculations
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutSize {
    pub width: f32,
    pub height: f32,
}

/// Point type for layout calculations
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutPoint {
    pub x: f32,
    pub y: f32,
}

/// Point type for SVG calculations
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgPoint {
    pub x: f32,
    pub y: f32,
}

/// Rectangle type for SVG calculations
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Cubic bezier curve for SVG paths
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgCubicCurve {
    pub start: SvgPoint,
    pub ctrl1: SvgPoint,
    pub ctrl2: SvgPoint,
    pub end: SvgPoint,
}

impl Default for LayoutRect {
    fn default() -> Self {
        Self {
            origin: LayoutPoint::default(),
            size: LayoutSize::default(),
        }
    }
}

impl Default for LayoutSize {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
        }
    }
}

impl Default for LayoutPoint {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

impl Default for SvgPoint {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

impl Default for SvgRect {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }
    }
}

impl Default for SvgCubicCurve {
    fn default() -> Self {
        Self {
            start: SvgPoint::default(),
            ctrl1: SvgPoint::default(),
            ctrl2: SvgPoint::default(),
            end: SvgPoint::default(),
        }
    }
}

impl LayoutRect {
    pub fn new(origin: LayoutPoint, size: LayoutSize) -> Self {
        Self { origin, size }
    }

    pub fn with_size(size: LayoutSize) -> Self {
        Self {
            origin: LayoutPoint::default(),
            size,
        }
    }
}

impl LayoutSize {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

impl LayoutPoint {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl SvgPoint {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl SvgRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}
