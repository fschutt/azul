//! Tests of the parts of `gles_gpu` that need no EGL: what Android's memory levels ask of the GPU path, what a window's
//! environment asks of its renderer, and the two configurations.

use azul_core::window::{HwAcceleration, RendererOptions};
use azul_layout::window_state::WindowCreateOptions;

use super::*;
use crate::desktop::shell_settings::ShellSettings;

/// `ComponentCallbacks2`'s constants.
const RUNNING_MODERATE: i32 = 5;
const RUNNING_LOW: i32 = 10;
const RUNNING_CRITICAL: i32 = 15;
const UI_HIDDEN: i32 = 20;
const BACKGROUND: i32 = 40;
const MODERATE: i32 = 60;
const COMPLETE: i32 = 80;

#[test]
fn memory_levels_below_running_low_ask_nothing() {
    assert_eq!(trim_level_for_android(RUNNING_MODERATE), None);
    assert_eq!(trim_level_for_android(0), None);
    assert_eq!(trim_level_for_android(-1), None);
}

/// The app is still on screen at `RUNNING_LOW` and `RUNNING_CRITICAL`: the caches go, the renderer stays.
#[test]
fn running_levels_drop_the_caches_only() {
    assert_eq!(trim_level_for_android(RUNNING_LOW), Some(TrimLevel::Light));
    assert_eq!(trim_level_for_android(RUNNING_CRITICAL), Some(TrimLevel::Light));
}

/// From `UI_HIDDEN` on the UI is not visible: the renderer and the context may go when no surface needs them (the reducer
/// turns a heavy trim with a surface into a light one).
#[test]
fn hidden_and_background_levels_are_heavy() {
    for level in [UI_HIDDEN, BACKGROUND, MODERATE, COMPLETE, 1000] {
        assert_eq!(trim_level_for_android(level), Some(TrimLevel::Heavy), "{level}");
    }
}

fn shell(backend: Option<&str>) -> ShellSettings {
    // Not `..default()`: the settings keep a private field, which struct update syntax cannot copy from outside the module.
    let mut settings = ShellSettings::default();
    settings.backend = backend.map(str::to_owned);
    settings
}

fn options_with(hw_accel: HwAcceleration) -> WindowCreateOptions {
    let mut options = WindowCreateOptions::default();
    options.window_state.renderer_options = RendererOptions {
        hw_accel,
        ..options.window_state.renderer_options
    };
    options
}

/// Nothing asked is the CPU renderer, as it was before the GPU path existed: a phone's default does not change.
#[test]
fn a_window_that_asks_nothing_gets_the_cpu_renderer() {
    assert_eq!(request_for(&shell(None), &options_with(HwAcceleration::DontCare)), GpuRequest::Cpu);
}

#[test]
fn the_environment_asks_before_the_window_does() {
    let enabled = options_with(HwAcceleration::Enabled);
    let disabled = options_with(HwAcceleration::Disabled);
    assert_eq!(request_for(&shell(Some("gpu")), &disabled), GpuRequest::Gpu);
    assert_eq!(request_for(&shell(Some("auto")), &disabled), GpuRequest::Auto);
    assert_eq!(request_for(&shell(Some("cpu")), &enabled), GpuRequest::Cpu);
    // No environment: the window's own option.
    assert_eq!(request_for(&shell(None), &enabled), GpuRequest::Gpu);
    assert_eq!(request_for(&shell(None), &disabled), GpuRequest::Cpu);
    // A word that is no render backend is not one.
    assert_eq!(request_for(&shell(Some("quantum")), &enabled), GpuRequest::Gpu);
    assert_eq!(request_for(&shell(Some("quantum")), &options_with(HwAcceleration::DontCare)), GpuRequest::Cpu);
}

#[test]
fn the_phone_configuration_makes_a_window_surface_and_a_pbuffer() {
    let config = GlesGpuConfig::android();
    assert_eq!(config.egl_libraries, vec!["libEGL.so"]);
    assert_eq!(config.gles_libraries, vec!["libGLESv3.so", "libGLESv2.so"]);
    assert_eq!(config.display, DisplayKind::Default);
    // Best first: a config that can make the window's surface AND a pbuffer (the context then runs without the window),
    // then one that makes the window's only.
    assert_eq!(config.surface_types, vec![EGL_WINDOW_BIT | EGL_PBUFFER_BIT, EGL_WINDOW_BIT]);
    assert!(!config.offscreen, "a phone draws to its window");
}

#[test]
fn the_host_configuration_draws_to_a_pbuffer_on_mesas_surfaceless_display() {
    let config = GlesGpuConfig::host_offscreen();
    assert_eq!(config.display, DisplayKind::SurfacelessMesa);
    assert_eq!(config.surface_types, vec![EGL_PBUFFER_BIT]);
    assert!(config.offscreen);
    assert!(config.egl_libraries.contains(&"libEGL.so.1"));
}
