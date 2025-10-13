use azul_css::props::{basic::AnimationInterpolationFunction, property::CssProperty};

use crate::task::{Duration as AzDuration, GetSystemTimeCallback, Instant as AzInstant};

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum UpdateImageType {
    Background,
    Content,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationData {
    pub from: CssProperty,
    pub to: CssProperty,
    pub start: AzInstant,
    pub duration: AzDuration,
    pub repeat: AnimationRepeat,
    pub interpolate: AnimationInterpolationFunction,
    pub relayout_on_finish: bool,
    pub parent_rect_width: f32,
    pub parent_rect_height: f32,
    pub current_rect_width: f32,
    pub current_rect_height: f32,
    pub get_system_time_fn: GetSystemTimeCallback,
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct Animation {
    pub from: CssProperty,
    pub to: CssProperty,
    pub duration: AzDuration,
    pub repeat: AnimationRepeat,
    pub repeat_times: AnimationRepeatCount,
    pub easing: AnimationInterpolationFunction,
    pub relayout_on_finish: bool,
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub enum AnimationRepeat {
    NoRepeat,
    Loop,
    PingPong,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash)]
#[repr(C, u8)]
pub enum AnimationRepeatCount {
    Times(usize),
    Infinite,
}
