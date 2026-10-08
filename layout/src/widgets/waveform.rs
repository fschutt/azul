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

    /// The waveform's DOM: the pinned theme's look, or (unpinned) both looks in their `@theme`
    /// blocks, the app theme picking.
    #[must_use]
    pub fn dom(self) -> azul_core::dom::Dom {
        use crate::widgets::themes::{flat, flora, theme_blocks, UiTheme};
        match self.theme.into_option() {
            Some(UiTheme::Flat) => flat::waveform(self),
            Some(UiTheme::Flora) => flora::waveform(self),
            None => theme_blocks::follow_app_theme(self, flat::waveform, flora::waveform),
        }
    }

    /// Moves the playhead of a built waveform (its root `node`) to `position_s` in place (the
    /// played colouring follows at the next rebuild). False when `node` is not a waveform.
    pub fn update_position(
        info: &mut crate::callbacks::CallbackInfo,
        node: azul_core::dom::DomNodeId,
        position_s: f64,
    ) -> bool {
        crate::widgets::seek_bar::move_surface(info, node, position_s)
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

/// The waveform: bars side by side, centred on their midline, the playhead's positioning context.
pub(crate) static WAVEFORM_BASE: &[azul_css::dynamic_selector::CssPropertyWithConditions] = {
    use azul_css::{
        dynamic_selector::CssPropertyWithConditions as P,
        props::{
            layout::{
                LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutHeight, LayoutMinWidth,
                LayoutPosition,
            },
            property::CssProperty,
            style::{StyleCursor, StyleUserSelect},
        },
    };
    &[
        P::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        P::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
        P::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        P::simple(CssProperty::const_position(LayoutPosition::Relative)),
        P::simple(CssProperty::const_height(LayoutHeight::const_px(48))),
        P::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(40))),
        P::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
        P::simple(CssProperty::user_select(StyleUserSelect::None)),
    ]
};

/// A bar: an equal share of the width.
pub(crate) static WAVEFORM_BAR_BASE: &[azul_css::dynamic_selector::CssPropertyWithConditions] = {
    use azul_css::{
        dynamic_selector::CssPropertyWithConditions as P,
        props::{layout::LayoutFlexGrow, property::CssProperty},
    };
    &[P::simple(CssProperty::const_flex_grow(
        LayoutFlexGrow::const_new(1),
    ))]
};

/// The playhead: a 2 px line the waveform's height, centred on the play position.
pub(crate) static WAVEFORM_HEAD_BASE: &[azul_css::dynamic_selector::CssPropertyWithConditions] = {
    use azul_css::{
        dynamic_selector::CssPropertyWithConditions as P,
        props::{
            layout::{LayoutHeight, LayoutMarginLeft, LayoutPosition, LayoutTop, LayoutWidth},
            property::CssProperty,
        },
    };
    &[
        P::simple(CssProperty::const_position(LayoutPosition::Absolute)),
        P::simple(CssProperty::const_top(LayoutTop::const_px(0))),
        P::simple(CssProperty::const_width(LayoutWidth::const_px(2))),
        P::simple(CssProperty::const_height(LayoutHeight::const_px(48))),
        P::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(
            -1,
        ))),
    ]
};

/// The waveform's DOM in `look`: [bar.., playhead] on a seek surface (the seek bar's dataset,
/// callbacks and merge, so a press, a drag and the keys seek exactly as on a seek bar).
#[allow(clippy::cast_precision_loss)]
pub(crate) fn build(wave: Waveform, look: &WaveformLook) -> azul_core::dom::Dom {
    use azul_core::{
        dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
        refany::{OptionRefAny, RefAny},
    };
    use azul_css::{
        dynamic_selector::{CssPropertyWithConditions as P, CssPropertyWithConditionsVec},
        props::{
            basic::pixel::PixelValue,
            layout::{LayoutHeight, LayoutLeft},
            property::CssProperty,
        },
        AzString,
    };

    use crate::widgets::seek_bar::{
        merge_seek_bar_state, seek_callbacks, seek_fraction, value_text, SeekBarState,
        SeekBarWrapper, SeekSurface,
    };

    static ROOT: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-waveform"))];
    static HEAD: &[IdOrClass] = &[Class(AzString::from_const_str(
        "__azul-native-waveform-head",
    ))];
    const BAR: AzString = AzString::from_const_str("__azul-native-waveform-bar");
    const PLAYED: AzString = AzString::from_const_str("__azul-native-waveform-played");

    let Waveform {
        position_s,
        duration_s,
        peaks,
        on_seek,
        accessibility_name,
        theme: _,
    } = wave;
    crate::widgets::warn_widget_needs_a_name("Waveform", accessibility_name.is_some());
    let state = SeekBarState {
        position_s,
        duration_s,
        dragging: false,
    };
    let fraction = seek_fraction(position_s, duration_s);
    let part = |base: &[P], skin: &[P]| crate::widgets::themes::decl::on_base(base, skin);
    let peaks = peaks.as_ref();
    let n = peaks.len().max(1) as f32;
    let mut children: Vec<Dom> = Vec::with_capacity(peaks.len() + 1);
    for (i, peak) in peaks.iter().enumerate() {
        let played = (i as f32 + 0.5) / n <= fraction;
        let height = if peak.is_finite() {
            (peak.clamp(0.0, 1.0) * 100.0).max(2.0)
        } else {
            2.0
        };
        let mut props = part(
            WAVEFORM_BAR_BASE,
            if played { &look.played } else { &look.bar },
        );
        props.push(P::simple(CssProperty::const_height(LayoutHeight::Px(
            PixelValue::percent(height),
        ))));
        let mut classes = alloc::vec![Class(BAR)];
        if played {
            classes.push(Class(PLAYED));
        }
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_vec(classes))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(props)),
        );
    }
    let mut head = part(WAVEFORM_HEAD_BASE, &look.head);
    head.push(P::simple(CssProperty::const_left(LayoutLeft::percent(
        fraction * 100.0,
    ))));
    children.push(
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(HEAD))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(head)),
    );

    let data = RefAny::new(SeekBarWrapper {
        on_seek,
        inner: state,
        surface: SeekSurface::Waveform,
    });
    let mut classes: Vec<IdOrClass> = ROOT.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(part(
            WAVEFORM_BASE,
            &look.root,
        )))
        .with_callbacks(seek_callbacks(&data).into())
        .with_dataset(OptionRefAny::Some(data))
        .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(
            merge_seek_bar_state,
        ))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Slider,
            accessibility_name,
            accessibility_value: Some(AzString::from(value_text(state))).into(),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(children))
}

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
