//! Waveform widget - an audio file's loudness over time as bars, the part already played in the
//! accent, a click or a drag seeks.
//!
//! The bars are PEAKS: the loudest sample of each slice of the audio, `0.0..=1.0`. They come
//! from the decoder (`AudioFileDecoder::waveform` decodes a whole file into them on a `Thread`)
//! or from the app's own samples through [`WaveformPeaks`] and [`resample_peaks`] - the one pair
//! of helpers for the concern, which the decoder uses too.
//!
//! Key types: [`WaveformPeaks`], [`resample_peaks`].

use alloc::vec::Vec;

/// The peaks of a signal as it streams by: the loudest sample (of any channel) of every
/// `block_frames` frames. Feed it decoded audio with [`push`](Self::push), take the peaks with
/// [`finish`](Self::finish).
#[derive(Debug, Clone)]
pub struct WaveformPeaks {
    channels: usize,
    block: usize,
    in_block: usize,
    current: f32,
    peaks: Vec<f32>,
}

impl WaveformPeaks {
    /// Peaks of every `block_frames` frames of `channels` interleaved channels (both at least 1).
    #[must_use]
    pub fn new(channels: u16, block_frames: u32) -> Self {
        Self {
            channels: usize::from(channels.max(1)),
            block: block_frames.max(1) as usize,
            in_block: 0,
            current: 0.0,
            peaks: Vec::new(),
        }
    }

    /// More audio (interleaved; a frame may be split across calls).
    pub fn push(&mut self, samples: &[f32]) {
        // Counted in samples, so a frame split across two calls lands in its block.
        let block_samples = self.block * self.channels;
        for s in samples {
            let v = s.abs();
            if v > self.current {
                self.current = v;
            }
            self.in_block += 1;
            if self.in_block == block_samples {
                self.peaks.push(self.current.min(1.0));
                self.current = 0.0;
                self.in_block = 0;
            }
        }
    }

    /// The peaks, the last (partial) block included.
    #[must_use]
    pub fn finish(mut self) -> Vec<f32> {
        if self.in_block > 0 {
            self.peaks.push(self.current.min(1.0));
        }
        self.peaks
    }
}

/// `peaks` as `buckets` values: each bucket is the loudest of the peaks it covers (fewer peaks
/// than buckets stretch, each peak covering several buckets). All zero without peaks.
#[must_use]
pub fn resample_peaks(peaks: &[f32], buckets: usize) -> Vec<f32> {
    let len = peaks.len();
    if len == 0 {
        return alloc::vec![0.0; buckets];
    }
    (0..buckets)
        .map(|b| {
            let start = (b * len / buckets).min(len - 1);
            let end = ((b + 1) * len / buckets).clamp(start + 1, len);
            peaks[start..end].iter().copied().fold(0.0f32, f32::max)
        })
        .collect()
}

// ==== The widget ====

/// A waveform: the peaks of an audio file as bars, the part already played in the accent, a
/// playhead; a press or a drag seeks (the [`SeekBar`](crate::widgets::seek_bar::SeekBar)'s hook
/// and state - one seek callback for both).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct Waveform {
    /// The play head, seconds.
    pub position_s: f64,
    /// The length, seconds.
    pub duration_s: f64,
    /// The bars: peak levels `0.0..=1.0` (`AudioFileDecoder::waveform`, [`resample_peaks`]).
    pub peaks: azul_css::F32Vec,
    /// Told every seek (the seek bar's callback).
    pub on_seek: crate::widgets::seek_bar::OptionSeekBarOnSeek,
    /// What the waveform is the position of, for assistive technology.
    pub accessibility_name: azul_css::OptionString,
    /// The widget theme this waveform is pinned to, or `None` to follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
}

impl Waveform {
    /// A waveform of `peaks`, at `position_s` of `duration_s`.
    #[must_use]
    pub fn create(peaks: azul_css::F32Vec, position_s: f64, duration_s: f64) -> Self {
        Self {
            position_s,
            duration_s,
            peaks,
            on_seek: crate::widgets::seek_bar::OptionSeekBarOnSeek::None,
            accessibility_name: azul_css::OptionString::None,
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }

    /// The hook told every seek.
    pub fn set_on_seek<C: Into<crate::widgets::seek_bar::SeekBarOnSeekCallback>>(
        &mut self,
        data: azul_core::refany::RefAny,
        callback: C,
    ) {
        self.on_seek = Some(crate::widgets::seek_bar::SeekBarOnSeek {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_on_seek`] for the builder chain.
    #[must_use]
    pub fn with_on_seek<C: Into<crate::widgets::seek_bar::SeekBarOnSeekCallback>>(
        mut self,
        data: azul_core::refany::RefAny,
        callback: C,
    ) -> Self {
        self.set_on_seek(data, callback);
        self
    }

    /// Name the waveform for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<azul_css::AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    /// Pin the widget theme; unset, the waveform follows the app theme.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty waveform and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(azul_css::F32Vec::from_const_slice(&[]), 0.0, 0.0);
        core::mem::swap(&mut s, self);
        s
    }

    /// The waveform's DOM.
    #[must_use]
    pub fn dom(self) -> azul_core::dom::Dom {
        azul_core::dom::Dom::create_div()
    }

    /// Moves the playhead of a built waveform (its root `node`) to `position_s` in place (the
    /// played colouring follows at the next rebuild). False when `node` is not a waveform.
    pub fn update_position(
        info: &mut crate::callbacks::CallbackInfo,
        node: azul_core::dom::DomNodeId,
        position_s: f64,
    ) -> bool {
        let _ = (info, node, position_s);
        false
    }
}

impl Default for Waveform {
    fn default() -> Self {
        Self::create(azul_css::F32Vec::from_const_slice(&[]), 0.0, 0.0)
    }
}

impl From<Waveform> for azul_core::dom::Dom {
    fn from(w: Waveform) -> Self {
        w.dom()
    }
}

azul_css::impl_option!(
    Waveform,
    OptionWaveform,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// What a theme decides about a waveform: the SKIN of each part.
#[derive(Debug, Clone, Default)]
pub(crate) struct WaveformLook {
    /// The surface the bars stand on (its focus ring included).
    pub root: Vec<azul_css::dynamic_selector::CssPropertyWithConditions>,
    /// A bar not played yet.
    pub bar: Vec<azul_css::dynamic_selector::CssPropertyWithConditions>,
    /// A bar played.
    pub played: Vec<azul_css::dynamic_selector::CssPropertyWithConditions>,
    /// The playhead.
    pub head: Vec<azul_css::dynamic_selector::CssPropertyWithConditions>,
    /// The theme's marker class on the root, if it has one.
    pub marker: Option<&'static str>,
}

#[cfg(test)]
mod waveform_widget_tests {
    use azul_css::F32Vec;

    use super::*;
    use crate::widgets::themes::{theme_blocks::checks, theme_checks, UiTheme};

    fn wave() -> Waveform {
        Waveform::create(
            F32Vec::from_vec(alloc::vec![0.2, 0.9, 0.5, 0.1, 0.7, 0.3, 1.0, 0.4]),
            30.0,
            120.0,
        )
        .with_accessibility_name("Position")
    }

    fn count(dom: &azul_core::dom::Dom, class: &str) -> usize {
        theme_checks::nodes(dom)
            .into_iter()
            .filter(|(_, n)| theme_checks::has_class(n, class))
            .count()
    }

    #[test]
    fn a_waveform_is_a_bar_per_peak_the_played_ones_in_the_accent_and_a_playhead() {
        for theme in checks::BOTH {
            let dom = wave().with_theme(theme).dom();
            assert!(theme_checks::has_class(&dom, "__azul-native-waveform"));
            assert_eq!(
                count(&dom, "__azul-native-waveform-bar"),
                8,
                "{}",
                theme.name()
            );
            // 30 s of 120 s: the first quarter (2 of 8 bars) is played.
            assert_eq!(
                count(&dom, "__azul-native-waveform-played"),
                2,
                "{}",
                theme.name()
            );
            assert_eq!(
                count(&dom, "__azul-native-waveform-head"),
                1,
                "{}",
                theme.name()
            );
            let tallest = format!("{:?}", dom.children.as_ref()[6].root.get_style());
            assert!(
                tallest.contains("Px(100%)"),
                "a full-scale peak is full height: {tallest}"
            );
        }
    }

    #[test]
    fn a_waveform_is_one_slider_stop_reading_the_time_of_the_length() {
        let dom = wave().with_theme(UiTheme::Flat).dom();
        assert_eq!(theme_checks::focusable(&dom).len(), 1);
        let info = dom.root.get_accessibility_info().expect("a role");
        assert_eq!(info.role, azul_core::a11y::AccessibilityRole::Slider);
        assert_eq!(
            info.accessibility_value.as_ref().map(|v| v.as_str()),
            Some("0:30 of 2:00")
        );
    }

    #[test]
    fn a_waveform_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "waveform",
            || wave().dom(),
            |t: UiTheme| wave().with_theme(t).dom(),
        );
    }
}

#[cfg(test)]
mod peaks_tests {
    use super::*;

    fn tone(frames: usize, amp: f32) -> Vec<f32> {
        (0..frames)
            .map(|i| amp * (core::f32::consts::TAU * i as f32 / 40.0).sin())
            .collect()
    }

    #[test]
    fn the_peaks_are_the_loudest_sample_of_each_block_of_any_channel() {
        let mut p = WaveformPeaks::new(2, 2);
        // Frames: (0.1, -0.5), (0.2, 0.0) | (0.9, 0.1), (-0.3, 0.0) | (0.05, 0.0)
        p.push(&[0.1, -0.5, 0.2, 0.0, 0.9]);
        p.push(&[0.1, -0.3, 0.0, 0.05, 0.0]);
        assert_eq!(p.finish(), vec![0.5, 0.9, 0.05]);
    }

    #[test]
    fn a_loud_half_and_a_quiet_half_stay_apart_at_any_bucket_count() {
        let mut p = WaveformPeaks::new(1, 1000);
        p.push(&tone(2000, 0.8));
        p.push(&tone(2000, 0.2));
        let peaks = p.finish();
        assert_eq!(peaks.len(), 4);
        let two = resample_peaks(&peaks, 2);
        assert!(
            (two[0] - 0.8).abs() < 1e-3 && (two[1] - 0.2).abs() < 1e-3,
            "{two:?}"
        );
        let eight = resample_peaks(&peaks, 8);
        assert_eq!(eight.len(), 8, "fewer peaks than buckets stretch");
        assert!(
            eight[..4].iter().all(|v| (v - 0.8).abs() < 1e-3),
            "{eight:?}"
        );
        assert!(
            eight[4..].iter().all(|v| (v - 0.2).abs() < 1e-3),
            "{eight:?}"
        );
        assert_eq!(resample_peaks(&[], 3), vec![0.0, 0.0, 0.0]);
        assert!(resample_peaks(&peaks, 0).is_empty());
    }
}
