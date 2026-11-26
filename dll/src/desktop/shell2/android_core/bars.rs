//! Which icon colour the system bars need over what the shell draws behind them.
//!
//! A NativeActivity's window is the app's own surface and it reaches under the status and navigation
//! bars. The bars draw their icons and the clock on top, in WHITE unless the window says its bars have a
//! light background (`WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS` / `..._LIGHT_NAVIGATION_BARS`,
//! API 30; `View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR`, API 23, and `..._LIGHT_NAVIGATION_BAR`, API 26). The
//! shell never said: on an app with a light page the clock, the battery and the navigation buttons were
//! white on white, and the first emulator screenshot of AzWidgets showed no status bar at all.
//!
//! What is behind a bar is what the shell RENDERED there, so that is what decides: the average luma of the
//! top rows of the frame (behind the status bar) and of the bottom rows (behind the navigation bar). A
//! change is told to the activity, once, with a hysteresis so a page whose top edge is mid grey does not
//! make the icons flicker between frames.

/// Rec. 601 luma of an opaque colour, 0 (black) to 255 (white), in integers.
#[must_use]
pub const fn luma(r: u8, g: u8, b: u8) -> u8 {
    ((299 * r as u32 + 587 * g as u32 + 114 * b as u32) / 1000) as u8
}

/// The average luma of the rows `rows` of an RGBA8 frame `width` pixels wide whose rows are `stride`
/// bytes apart, from every `STEP`-th pixel of every other row (a band of a 1080 x 2400 frame is a few
/// hundred thousand pixels: a sample is plenty to tell light from dark). `None` when the band has no
/// pixel in `pixels`.
#[must_use]
pub fn band_luma(pixels: &[u8], stride: usize, width: usize, rows: (usize, usize)) -> Option<u8> {
    const STEP: usize = 8;
    let (top, bottom) = rows;
    let mut sum = 0u64;
    let mut count = 0u64;
    let mut y = top;
    while y < bottom {
        let row = y.checked_mul(stride)?;
        let mut x = 0;
        while x < width {
            let at = row + x * 4;
            if let Some(px) = pixels.get(at..at + 3) {
                sum += u64::from(luma(px[0], px[1], px[2]));
                count += 1;
            }
            x += STEP;
        }
        y += 2;
    }
    (count > 0).then(|| (sum / count) as u8)
}

/// Above this a bar is over a LIGHT background (dark icons); below [`DARK_BELOW`] over a dark one.
pub const LIGHT_ABOVE: u8 = 150;
/// See [`LIGHT_ABOVE`].
pub const DARK_BELOW: u8 = 105;

/// Whether the bar is over a light background, given the luma behind it and what was told last: between
/// the two thresholds the last answer stands (a mid grey does not flip the icons every frame); with no
/// last answer the middle of the range decides.
#[must_use]
pub const fn light_background(previous: Option<bool>, luma: u8) -> bool {
    if luma > LIGHT_ABOVE {
        true
    } else if luma < DARK_BELOW {
        false
    } else {
        match previous {
            Some(light) => light,
            None => luma >= 128,
        }
    }
}

/// What the shell last told the activity about each bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BarsState {
    status_light: Option<bool>,
    navigation_light: Option<bool>,
}

impl BarsState {
    /// Nothing told yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            status_light: None,
            navigation_light: None,
        }
    }

    /// The luma behind the status bar and the navigation bar of the frame just posted (`None` for a bar
    /// that has no rows or no pixels: it keeps what it had). Returns what to tell the activity, or `None`
    /// when neither bar changed.
    pub fn update(&mut self, status_luma: Option<u8>, navigation_luma: Option<u8>) -> Option<(bool, bool)> {
        let status = status_luma.map_or(self.status_light, |l| Some(light_background(self.status_light, l)));
        let navigation =
            navigation_luma.map_or(self.navigation_light, |l| Some(light_background(self.navigation_light, l)));
        if status == self.status_light && navigation == self.navigation_light {
            return None;
        }
        self.status_light = status;
        self.navigation_light = navigation;
        // a bar never sampled is told as it is today: light icons on dark, the system's default
        Some((status.unwrap_or(false), navigation.unwrap_or(false)))
    }

    /// The activity was recreated or the window lost its flags: tell it again at the next frame.
    pub fn forget(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
#[path = "bars_test.rs"]
mod tests;
