//! What the display of a phone has built in, as the Java side reports it: the cutouts, the rounded corners, the system bars
//! and the kind of fingerprint sensor, in PHYSICAL px of the window (Android's insets and rects always are).
//!
//! `NativeTextBridge.featuresArray` flattens `WindowInsets` into ONE `int[]`, and this module reads it back: pure
//! std, so the contract between the two halves is tested on every host (the Java half is checked by
//! `dll/tests/android_display_features_contract.rs`). The shell turns the result into the engine's `DisplayFeatures`
//! (logical px, origin top left) with [`Px::to_logical`].
//!
//! The layout of the array, all values `int`, version 1:
//!
//! ```text
//! [0]            1                       the format version
//! [1]            n                       the number of cutouts
//! [2 .. 2+4n]    left, top, right, bottom of each cutout (DisplayCutout.getBoundingRects())
//! then 4         the rounded corners' radii: top left, top right, bottom right, bottom left (RoundedCorner.getRadius(), API 31; 0 = none)
//! then 6         the status bar: present (0/1), left, top, right, bottom, visible (0/1)
//! then 6         the navigation bar: the same
//! then 1         the fingerprint sensor: -1 none, 0 there is one and the system does not say what kind
//! ```
//!
//! The kind of an under-display sensor and its place are not public API on Android (`FingerprintSensorPropertiesInternal` is
//! hidden): the array has no field for a place, and the kind is only ever "there is one".

/// A rectangle in physical px, `left < right`, `top < bottom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Px {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Px {
    #[must_use]
    pub const fn width(self) -> i32 {
        self.right - self.left
    }

    #[must_use]
    pub const fn height(self) -> i32 {
        self.bottom - self.top
    }

    /// The rect as `(x, y, width, height)` in logical px for a window whose device pixels per logical px is `scale`.
    #[must_use]
    pub fn to_logical(self, scale: f32) -> (f32, f32, f32, f32) {
        let s = if scale > 0.0 { scale } else { 1.0 };
        (self.left as f32 / s, self.top as f32 / s, self.width() as f32 / s, self.height() as f32 / s)
    }
}

/// What a cutout looks like, guessed from its rect: a hole about as wide as it is high is a punch hole (a circle), anything else
/// a rounded rectangle. A notch (open to the edge) is not told apart: the platform gives only the bounding rect (the path is API 31).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeGuess {
    Circle,
    RoundedRect,
}

/// The shape of a cutout of `rect`.
#[must_use]
pub fn guess_shape(rect: Px) -> ShapeGuess {
    let (w, h) = (rect.width().max(1), rect.height().max(1));
    // within a fifth of square
    if w * 5 >= h * 4 && h * 5 >= w * 4 {
        ShapeGuess::Circle
    } else {
        ShapeGuess::RoundedRect
    }
}

/// One system bar: where it is when shown, and whether it is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bar {
    pub rect: Px,
    pub visible: bool,
}

/// The display features of one dispatch, in physical px.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayFeaturesPx {
    pub cutouts: Vec<Px>,
    /// Top left, top right, bottom right, bottom left.
    pub corners: [i32; 4],
    pub status: Option<Bar>,
    pub navigation: Option<Bar>,
    /// Whether the device has a fingerprint sensor (the kind and the place are not public API).
    pub fingerprint: bool,
}

/// The only version of the array there is.
pub const VERSION: i32 = 1;

fn rect(flat: &[i32], at: usize) -> Option<Px> {
    let r = flat.get(at..at + 4)?;
    Some(Px {
        left: r[0],
        top: r[1],
        right: r[2],
        bottom: r[3],
    })
}

fn bar(flat: &[i32], at: usize) -> Option<Option<Bar>> {
    let b = flat.get(at..at + 6)?;
    if b[0] == 0 {
        return Some(None);
    }
    let rect = Px {
        left: b[1],
        top: b[2],
        right: b[3],
        bottom: b[4],
    };
    Some(Some(Bar { rect, visible: b[5] != 0 }))
}

/// Read the array of `NativeTextBridge.featuresArray`. `None` for a version this code does not know or an array that is
/// cut short (a half-written report is no report). A cutout without an area is dropped (the platform reports empty rects for
/// edges that have none).
#[must_use]
pub fn decode(flat: &[i32]) -> Option<DisplayFeaturesPx> {
    if *flat.first()? != VERSION {
        return None;
    }
    let n = usize::try_from(*flat.get(1)?).ok()?;
    // a count the array cannot hold is a cut array (and keeps `4 * n` from overflowing)
    if n > flat.len() {
        return None;
    }
    let mut at = 2;
    let mut cutouts = Vec::with_capacity(n);
    for _ in 0..n {
        let r = rect(flat, at)?;
        if r.width() > 0 && r.height() > 0 {
            cutouts.push(r);
        }
        at += 4;
    }
    let c = flat.get(at..at + 4)?;
    let corners = [c[0].max(0), c[1].max(0), c[2].max(0), c[3].max(0)];
    at += 4;
    let status = bar(flat, at)?;
    at += 6;
    let navigation = bar(flat, at)?;
    at += 6;
    let fingerprint = *flat.get(at)? >= 0;
    Some(DisplayFeaturesPx {
        cutouts,
        corners,
        status,
        navigation,
        fingerprint,
    })
}

#[cfg(test)]
#[path = "display_test.rs"]
mod tests;
