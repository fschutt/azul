//! The app's ONE light: where the light that shades every window comes from.
//!
//! Every 3D-looking thing in an app reads the same light, so all of them
//! agree: the built-in materials (their grain, bevels and pillows), the
//! cursor's sheen on them, the metals' reflection of the desktop, the
//! box-shadows and the pressed text that opt in (`--azul-follow-light`).
//! The light lives in DESKTOP space, not in a window: two windows side by
//! side are lit as one scene, and with a point light moving a window changes
//! its shading.
//!
//! Two kinds of light ([`Light`]):
//!
//! - the sun, far away: one direction for every point (`azimuth`, `elevation`). The default
//!   is the sun at the top right, low (azimuth 45, elevation 36), like daylight through a
//!   window.
//! - a point light hanging over the screen at desktop point (`x`, `y`), `height` points in
//!   front of it: the direction changes across the desktop.
//!
//! Directions are on the SCREEN: x to the right, y DOWN, z toward the
//! viewer. The app sets the light once (`AppConfig::with_light`) and may move
//! it later (`CallbackInfo::set_app_light`); the choice is process-global like
//! the app theme's, so a window opened after a change starts with it.

/// The sun: a light so far away that it comes from one direction everywhere.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct SunLight {
    /// Where the sun stands, degrees counter-clockwise from the right of the
    /// screen: 0 = right, 90 = above, 180 = left, 270 = below. Read by
    /// [`Light::direction_at`].
    pub azimuth_deg: f32,
    /// How high it stands over the screen, degrees: 90 = straight in front
    /// (nothing casts a shadow), small = grazing (long shadows, strong
    /// relief). Clamped to 1..=90 where it is read.
    pub elevation_deg: f32,
}

/// A point light hanging in front of the desktop.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct PointLight {
    /// Its place over the desktop, desktop points from the primary screen's
    /// top-left corner (the same space as a window's position).
    pub x: f32,
    /// See [`Self::x`]; y grows down the screen.
    pub y: f32,
    /// How far in front of the screen it hangs, points. Small = grazing
    /// light near it, large = nearly the sun. Floored at 1 where it is read.
    pub height: f32,
}

/// The app's light (see the module docs): the sun or a point light.
#[repr(C, u8)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum Light {
    /// A light from one direction everywhere.
    Sun(SunLight),
    /// A light at a point in front of the desktop.
    Point(PointLight),
}

impl Light {
    /// The default light: the sun at the top right, low.
    pub const DEFAULT: Self = Self::Sun(SunLight {
        azimuth_deg: 45.0,
        elevation_deg: 36.0,
    });

    /// The sun at `azimuth_deg` (counter-clockwise from the right) and
    /// `elevation_deg` over the screen.
    #[must_use]
    pub const fn sun(azimuth_deg: f32, elevation_deg: f32) -> Self {
        Self::Sun(SunLight {
            azimuth_deg,
            elevation_deg,
        })
    }

    /// A point light at desktop point (`x`, `y`), `height` points in front of
    /// the screen.
    #[must_use]
    pub const fn point(x: f32, y: f32, height: f32) -> Self {
        Self::Point(PointLight { x, y, height })
    }

    /// The default light.
    #[must_use]
    pub const fn create() -> Self {
        Self::DEFAULT
    }

    /// The unit vector from desktop point (`x`, `y`) toward the light (screen
    /// space, y down, z toward the viewer). The sun's is the same everywhere.
    #[must_use]
    pub fn direction_at(&self, x: f32, y: f32) -> [f32; 3] {
        match *self {
            Self::Sun(sun) => sun_vector(sun.azimuth_deg, sun.elevation_deg),
            Self::Point(p) => normalize3([p.x - x, p.y - y, p.height.max(1.0)]),
        }
    }

    /// Which way a shadow falls at desktop point (`x`, `y`) and how long it is:
    /// the unit direction on the screen pointing AWAY from the light, and a
    /// length factor that grows as the light sinks (0.3 for a light straight
    /// in front, up to 2.6 for a grazing one; 1 for a light 45 degrees high).
    #[must_use]
    pub fn shadow_at(&self, x: f32, y: f32) -> ShadowDirection {
        let l = self.direction_at(x, y);
        let flat = libm::hypotf(l[0], l[1]);
        if flat < 1e-6 {
            // Straight in front: the shadow lies under the box, shortest.
            return ShadowDirection {
                x: 0.0,
                y: 1.0,
                length: 0.3,
            };
        }
        ShadowDirection {
            x: -l[0] / flat,
            y: -l[1] / flat,
            length: (flat / l[2].max(0.2)).clamp(0.3, 2.6),
        }
    }

    /// A shadow offset written for a light from straight above (a shadow
    /// falling down the screen, `0 4px` falls 4 px down), turned to fall away
    /// from this light at desktop point `at`. With `stretch` it also grows
    /// with the light's [`ShadowDirection::length`] - a box's shadow; without
    /// it only turns - the 1 px highlight of pressed text.
    #[must_use]
    // `f32::mul_add` needs std; the crate builds without it.
    #[allow(clippy::suboptimal_flops)]
    pub fn follow_offset(&self, offset: [f32; 2], at: [f32; 2], stretch: bool) -> [f32; 2] {
        let s = self.shadow_at(at[0], at[1]);
        let k = if stretch { s.length } else { 1.0 };
        // The rotation that takes "down" (0, 1) to the shadow's direction.
        [
            (offset[0] * s.y + offset[1] * s.x) * k,
            (offset[1] * s.y - offset[0] * s.x) * k,
        ]
    }

    /// The light as bit patterns, for a cache key: a new light is a new key.
    #[must_use]
    pub const fn key_bits(&self) -> [u32; 4] {
        match *self {
            Self::Sun(s) => [0, s.azimuth_deg.to_bits(), s.elevation_deg.to_bits(), 0],
            Self::Point(p) => [1, p.x.to_bits(), p.y.to_bits(), p.height.to_bits()],
        }
    }

    /// Whether the light's direction depends on where on the desktop a
    /// surface is (a point light): a window that moves is then lit anew.
    #[must_use]
    pub const fn depends_on_position(&self) -> bool {
        matches!(self, Self::Point(_))
    }
}

impl Default for Light {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Where a shadow falls ([`Light::shadow_at`]).
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ShadowDirection {
    /// The unit direction on the screen away from the light, x part.
    pub x: f32,
    /// The y part (y down).
    pub y: f32,
    /// How long the shadow is against one cast by a light 45 degrees high.
    pub length: f32,
}

/// Toward the sun at `azimuth_deg` / `elevation_deg`: `(cos el cos az,
/// -cos el sin az, sin el)`, y down the screen.
#[must_use]
pub fn sun_vector(azimuth_deg: f32, elevation_deg: f32) -> [f32; 3] {
    let az = azimuth_deg.to_radians();
    let el = elevation_deg.clamp(1.0, 90.0).to_radians();
    let (cos_el, sin_el) = (libm::cosf(el), libm::sinf(el));
    [cos_el * libm::cosf(az), -cos_el * libm::sinf(az), sin_el]
}

// `f32::mul_add` needs std; the crate builds without it.
#[allow(clippy::suboptimal_flops)]
fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = libm::sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    if len > 1e-12 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        [0.0, 0.0, 1.0]
    }
}

#[cfg(feature = "std")]
mod state {
    use super::Light;

    /// The app's light, and how often it changed: windows compare the count
    /// with the one they last adopted.
    pub(super) static APP_LIGHT: std::sync::RwLock<(Light, u64)> =
        std::sync::RwLock::new((Light::DEFAULT, 0));

    std::thread_local! {
        /// The light and the window origin of the display list being built on
        /// this thread, while a [`super::LightScope`] is entered.
        pub(super) static FRAME: core::cell::Cell<Option<(Light, [f32; 2])>> =
            const { core::cell::Cell::new(None) };
    }
}

/// Publish the app's light. Returns whether it is a new one; every window
/// then adopts it at its next frame (`App::create` publishes
/// `AppConfig::light`, `CallbackInfo::set_app_light` a new one).
pub fn set_app_light(light: Light) -> bool {
    #[cfg(feature = "std")]
    {
        let mut slot = state::APP_LIGHT
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.0 == light {
            return false;
        }
        *slot = (light, slot.1.wrapping_add(1));
        true
    }
    #[cfg(not(feature = "std"))]
    {
        let _ = light;
        false
    }
}

/// The app's light: the last [`set_app_light`], else [`Light::DEFAULT`].
#[must_use]
pub fn app_light() -> Light {
    app_light_with_generation().0
}

/// The app's light and how many times it changed. A window that adopted
/// generation `n` and reads another number adopts the light anew.
#[must_use]
pub fn app_light_with_generation() -> (Light, u64) {
    #[cfg(feature = "std")]
    {
        *state::APP_LIGHT
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    #[cfg(not(feature = "std"))]
    {
        (Light::DEFAULT, 0)
    }
}

/// The light a display list being built on this thread is lit by, and
/// where its window sits on the desktop (desktop points).
///
/// The entered [`LightScope`]'s, else the app's light at the desktop's
/// origin. The light-following box-shadows and text-shadows read it.
#[must_use]
pub fn current_light_frame() -> (Light, [f32; 2]) {
    #[cfg(feature = "std")]
    {
        if let Some(frame) = state::FRAME.with(core::cell::Cell::get) {
            return frame;
        }
    }
    (app_light(), [0.0, 0.0])
}

/// While alive, [`current_light_frame`] answers this scope's light and
/// window origin on this thread.
///
/// Dropping it restores the previous answer. The engine enters one around
/// every display-list build of a window, with the window's light (the app's,
/// or the window's own) and its place on the desktop.
#[must_use = "the light applies only while the scope is alive"]
#[derive(Debug)]
pub struct LightScope {
    /// What to restore when the scope ends (nested scopes unwind in order).
    /// Unused without `std`.
    #[allow(dead_code)]
    previous: Option<(Light, [f32; 2])>,
}

impl LightScope {
    /// Enter a scope lit by `light`, its window at `origin` on the desktop.
    pub fn enter(light: Light, origin: [f32; 2]) -> Self {
        #[cfg(feature = "std")]
        {
            let previous = state::FRAME.with(|f| f.replace(Some((light, origin))));
            Self { previous }
        }
        #[cfg(not(feature = "std"))]
        {
            let _ = (light, origin);
            Self { previous: None }
        }
    }
}

impl Drop for LightScope {
    fn drop(&mut self) {
        #[cfg(feature = "std")]
        state::FRAME.with(|f| f.set(self.previous));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_light_is_the_sun_at_the_top_right_and_low() {
        let l = Light::default().direction_at(0.0, 0.0);
        assert!(l[0] > 0.5 && l[1] < -0.5, "toward the top right: {l:?}");
        assert!((l[2] - 36.0_f32.to_radians().sin()).abs() < 1e-5, "36 degrees high");
        let len = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-5);
    }

    #[test]
    fn a_shadow_falls_away_from_the_light_and_grows_as_the_light_sinks() {
        let s = Light::sun(45.0, 36.0).shadow_at(0.0, 0.0);
        assert!(s.x < 0.0 && s.y > 0.0, "to the bottom left: {s:?}");
        let high = Light::sun(45.0, 70.0).shadow_at(0.0, 0.0);
        assert!(high.length < s.length, "{} vs {}", high.length, s.length);
        // A shadow written as falling 16 px down turns with the light.
        let o = Light::sun(45.0, 36.0).follow_offset([0.0, 16.0], [0.0, 0.0], true);
        assert!((o[0] - 16.0 * s.x * s.length).abs() < 1e-3);
        assert!((o[1] - 16.0 * s.y * s.length).abs() < 1e-3);
        // A light straight above leaves a shadow written for it as it is.
        let above = Light::sun(90.0, 45.0).follow_offset([2.0, 4.0], [0.0, 0.0], false);
        assert!((above[0] - 2.0).abs() < 1e-4 && (above[1] - 4.0).abs() < 1e-4, "{above:?}");
    }

    #[test]
    fn a_point_light_lights_each_place_from_its_own_direction() {
        let light = Light::point(100.0, 100.0, 50.0);
        let left_of_it = light.direction_at(0.0, 100.0);
        let right_of_it = light.direction_at(200.0, 100.0);
        assert!(left_of_it[0] > 0.0 && right_of_it[0] < 0.0);
        assert!(light.depends_on_position() && !Light::DEFAULT.depends_on_position());
    }

    #[test]
    fn the_app_light_round_trips_and_counts_its_changes() {
        let (_, before) = app_light_with_generation();
        let moved = Light::sun(120.0, 30.0);
        assert!(set_app_light(moved));
        let (now, after) = app_light_with_generation();
        assert_eq!(now, moved);
        assert_ne!(before, after);
        assert!(!set_app_light(moved), "the same light again is no change");
        assert!(set_app_light(Light::DEFAULT));
    }
}
