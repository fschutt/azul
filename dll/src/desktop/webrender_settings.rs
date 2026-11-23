//! What the environment asks of the WebRender path (the compositor translation, the renderer's
//! texture cache and the transaction), and the one place of the dll that names those variables
//! (the group D2 of the env pass).
//!
//! Nothing here reads the process: [`WebRenderSettings::from_lookup`] takes a `&dyn EnvSource`. The
//! app start builds it once, inside [`super::app_settings::AppSettings`], and the code that builds
//! the renderer and translates the display lists reads a plain field of it.
//! [`WebRenderSettings::default`] is the behaviour of an EMPTY environment: the switches that are on
//! unless the variable says `0` are on, the numbers have the defaults the variables name.

use azul_css::env::EnvSource;

/// `AZ_WR_HIT_TEST=1`: hand the hit-test areas of a display list to WebRender.
const HIT_TEST_ITEMS: &str = "AZ_WR_HIT_TEST";
/// `AZ_WR_VIEW_SLICE=0`: no scroll frame of its own under a view's content.
const VIEW_SLICE: &str = "AZ_WR_VIEW_SLICE";
/// `AZ_WR_ZOOM_TILES=0`: no zoom frame (picture cache slice) for a view's zoom.
const ZOOM_TILES: &str = "AZ_WR_ZOOM_TILES";
/// `AZ_WR_ATLAS_SIZE`: the size of the large texture atlases.
const ATLAS_SIZE: &str = "AZ_WR_ATLAS_SIZE";
/// `AZ_WR_IMAGE_BUDGET_MB`: how much of the images WebRender's texture cache keeps.
const IMAGE_BUDGET_MB: &str = "AZ_WR_IMAGE_BUDGET_MB";
/// `AZ_WR_GLYPH_BUDGET_MB`: how much of the glyphs WebRender's texture cache keeps.
const GLYPH_BUDGET_MB: &str = "AZ_WR_GLYPH_BUDGET_MB";
/// `AZ_WALK_GPU_VALUES`: walk and hash the bound GPU values on every frame.
const WALK_GPU_VALUES: &str = "AZ_WALK_GPU_VALUES";
/// `AZ_WR_SYNC_SCENE`: synchronise the WebRender scene on every frame.
const SYNC_SCENE: &str = "AZ_WR_SYNC_SCENE";
/// `AZ_WR_HIT_TESTER`: create WebRender's hit tester at window creation (macOS).
const HIT_TESTER: &str = "AZ_WR_HIT_TESTER";

/// The atlas size when the environment names none.
const DEFAULT_ATLAS_SIZE: i32 = 1024;
/// The smallest atlas size the variable can ask for.
const MIN_ATLAS_SIZE: i32 = 512;
/// The largest atlas size the variable can ask for.
const MAX_ATLAS_SIZE: i32 = 4096;
/// The image budget when the environment names none, in megabytes.
const DEFAULT_IMAGE_BUDGET_MB: usize = 256;
/// The glyph budget when the environment names none, in megabytes.
const DEFAULT_GLYPH_BUDGET_MB: usize = 16;
/// No budget is larger than this many megabytes, whatever the variable says.
const MAX_BUDGET_MB: usize = 1024;

/// The settings of the WebRender path.
// Each flag is its own switch of a debugging session: a set of enums would only rename them.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebRenderSettings {
    /// `AZ_WR_HIT_TEST=1`: the hit-test areas of a display list are translated and handed to
    /// WebRender (nothing in azul asks its hit tester any more; off by default).
    pub hit_test_items: bool,
    /// A view's content is put under an explicit scroll frame of its own. On unless
    /// `AZ_WR_VIEW_SLICE=0`.
    pub view_slice: bool,
    /// A view's zoom is put above a picture cache slice of its own. On unless
    /// `AZ_WR_ZOOM_TILES=0`.
    pub zoom_tiles: bool,
    /// `AZ_WR_ATLAS_SIZE` clamped to 512..=4096: the size of the large texture atlases (1024 when
    /// the variable is unset or not a number).
    pub atlas_size: i32,
    /// `AZ_WR_IMAGE_BUDGET_MB` (at most 1024): megabytes of images the texture cache keeps; 256
    /// when the variable is unset or not a number, `0` gives WebRender's own rule.
    pub image_budget_mb: usize,
    /// `AZ_WR_GLYPH_BUDGET_MB` (at most 1024): megabytes of glyphs the texture cache keeps; 16
    /// when the variable is unset or not a number, `0` gives WebRender's own rule.
    pub glyph_budget_mb: usize,
    /// `AZ_WALK_GPU_VALUES` is set, not empty and not `0`: the GPU values are walked and hashed on
    /// every frame.
    pub walk_gpu_values: bool,
    /// `AZ_WR_SYNC_SCENE` is set, not empty and not `0`: the scene is synchronised on every frame.
    pub sync_scene: bool,
    /// `AZ_WR_HIT_TESTER` is set (to anything): WebRender's hit tester is created with the window.
    pub hit_tester: bool,
}

/// A switch that is on when the variable is set to something that is not empty and not `0`.
fn flag(env: &dyn EnvSource, name: &str) -> bool {
    env.var(name).is_some_and(|v| !v.is_empty() && v != "0")
}

/// A switch that is on unless the variable is `0`.
fn on_unless_zero(env: &dyn EnvSource, name: &str) -> bool {
    !env.var(name).is_some_and(|v| v == "0")
}

/// A number of megabytes: the variable's, or `default`, at most [`MAX_BUDGET_MB`].
fn megabytes(env: &dyn EnvSource, name: &str, default: usize) -> usize {
    env.var(name)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(default)
        .min(MAX_BUDGET_MB)
}

impl WebRenderSettings {
    /// What the environment `env` asks for; an empty one asks for nothing.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            hit_test_items: env.var(HIT_TEST_ITEMS).is_some_and(|v| v == "1"),
            view_slice: on_unless_zero(env, VIEW_SLICE),
            zoom_tiles: on_unless_zero(env, ZOOM_TILES),
            atlas_size: env
                .var(ATLAS_SIZE)
                .and_then(|s| s.parse::<i32>().ok())
                .map_or(DEFAULT_ATLAS_SIZE, |v| {
                    v.clamp(MIN_ATLAS_SIZE, MAX_ATLAS_SIZE)
                }),
            image_budget_mb: megabytes(env, IMAGE_BUDGET_MB, DEFAULT_IMAGE_BUDGET_MB),
            glyph_budget_mb: megabytes(env, GLYPH_BUDGET_MB, DEFAULT_GLYPH_BUDGET_MB),
            walk_gpu_values: flag(env, WALK_GPU_VALUES),
            sync_scene: flag(env, SYNC_SCENE),
            hit_tester: env.is_set(HIT_TESTER),
        }
    }
}

impl Default for WebRenderSettings {
    /// The empty environment's.
    fn default() -> Self {
        Self::from_lookup(&azul_css::env::NoEnv)
    }
}

#[cfg(test)]
#[path = "webrender_settings_test.rs"]
mod tests;
