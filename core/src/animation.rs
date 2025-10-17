//! Core data structures for configuring and tracking CSS animations

use azul_css::props::{basic::AnimationInterpolationFunction, property::CssProperty};

use crate::task::{Duration as AzDuration, GetSystemTimeCallback, Instant as AzInstant};

/// Specifies which image layer of an element an animation should apply to.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum UpdateImageType {
    /// The animation targets the element's background.
    Background,
    /// The animation targets the element's main content.
    Content,
}

/// Holds the dynamic, runtime state of an active animation instance.
/// This is created when an `Animation` begins playing.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationData {
    /// The starting `CssProperty` value of the animation.
    pub from: CssProperty,
    /// The target `CssProperty` value at the end of the animation.
    pub to: CssProperty,
    /// The timestamp marking when the animation began.
    pub start: AzInstant,
    /// The total time the animation takes to complete one cycle.
    pub duration: AzDuration,
    /// The repetition behavior of the animation (e.g., loop, ping-pong).
    pub repeat: AnimationRepeat,
    /// The easing function used to control the animation's pacing.
    pub interpolate: AnimationInterpolationFunction,
    /// If `true`, a relayout is triggered after the animation finishes.
    pub relayout_on_finish: bool,
    /// The width of the parent's bounding rectangle at the animation's start.
    pub parent_rect_width: f32,
    /// The height of the parent's bounding rectangle at the animation's start.
    pub parent_rect_height: f32,
    /// The width of the animated element's bounding rectangle at the animation's start.
    pub current_rect_width: f32,
    /// The height of the animated element's bounding rectangle at the animation's start.
    pub current_rect_height: f32,
    /// A callback function used to get the current system time for synchronized timing.
    pub get_system_time_fn: GetSystemTimeCallback,
}

/// Defines the static configuration for a CSS animation, parsed from a stylesheet.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct Animation {
    /// The `CssProperty` at the beginning of the animation (0% keyframe).
    pub from: CssProperty,
    /// The `CssProperty` at the end of the animation (100% keyframe).
    pub to: CssProperty,
    /// The time it takes for the animation to complete one cycle.
    pub duration: AzDuration,
    /// The repetition behavior to apply when a cycle finishes.
    pub repeat: AnimationRepeat,
    /// How many times the animation should repeat.
    pub repeat_times: AnimationRepeatCount,
    /// The easing function that dictates the animation's rate of change.
    pub easing: AnimationInterpolationFunction,
    /// If `true`, a full relayout is performed after the animation concludes.
    pub relayout_on_finish: bool,
}

/// Describes the behavior of an animation when it reaches the end of a cycle.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub enum AnimationRepeat {
    /// The animation plays once and then stops.
    NoRepeat,
    /// The animation restarts from the beginning after finishing.
    Loop,
    /// The animation plays forwards, then backwards, alternating each cycle.
    PingPong,
}

/// Specifies how many times an animation cycle should repeat.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash)]
#[repr(C, u8)]
pub enum AnimationRepeatCount {
    /// The animation repeats for a specific number of cycles.
    Times(usize),
    /// The animation repeats indefinitely.
    Infinite,
}
