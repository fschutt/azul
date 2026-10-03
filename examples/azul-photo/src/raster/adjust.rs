//! Adjustments: colour maps applied by adjustment layers to everything below
//! them (non-destructive) - brightness / contrast, levels, curves,
//! hue / saturation, invert, threshold.

use serde::{Deserialize, Serialize};

/// One adjustment and its parameters. Colours are straight RGB in 0..1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Adjustment {
    /// `brightness` and `contrast` in -1..1 (0 = unchanged).
    BrightnessContrast { brightness: f32, contrast: f32 },
    /// Input black / white points, a gamma (1 = linear) and the output range.
    Levels {
        in_black: u8,
        in_white: u8,
        gamma: f32,
        out_black: u8,
        out_white: u8,
    },
    /// A monotone curve through (input, output) points, 0..255.
    Curves { points: Vec<(u8, u8)> },
    /// `hue` in degrees (-180..180), `saturation` and `lightness` in -1..1.
    HueSaturation {
        hue: f32,
        saturation: f32,
        lightness: f32,
    },
    Invert,
    /// Pixels at or above `level` (luminance, 0..255) turn white, the rest black.
    Threshold { level: u8 },
}

impl Adjustment {
    /// The adjustments the Adjustments panel offers, with their neutral settings.
    #[must_use]
    pub fn catalog() -> Vec<Self> {
        vec![
            Self::BrightnessContrast {
                brightness: 0.0,
                contrast: 0.0,
            },
            Self::Levels {
                in_black: 0,
                in_white: 255,
                gamma: 1.0,
                out_black: 0,
                out_white: 255,
            },
            Self::Curves {
                points: vec![(0, 0), (64, 64), (192, 192), (255, 255)],
            },
            Self::HueSaturation {
                hue: 0.0,
                saturation: 0.0,
                lightness: 0.0,
            },
            Self::Invert,
            Self::Threshold { level: 128 },
        ]
    }

    /// The name of the layer and the panel entry.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::BrightnessContrast { .. } => "Brightness/Contrast",
            Self::Levels { .. } => "Levels",
            Self::Curves { .. } => "Curves",
            Self::HueSaturation { .. } => "Hue/Saturation",
            Self::Invert => "Invert",
            Self::Threshold { .. } => "Threshold",
        }
    }

    /// The adjusted colour.
    #[must_use]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        match self {
            Self::BrightnessContrast {
                brightness,
                contrast,
            } => {
                let f = contrast_factor(*contrast);
                c.map(|v| ((v + brightness - 0.5) * f + 0.5).clamp(0.0, 1.0))
            }
            Self::Levels { .. } | Self::Curves { .. } => {
                let curve = ChannelCurve::of(self);
                c.map(|v| curve.eval(v))
            }
            Self::HueSaturation {
                hue,
                saturation,
                lightness,
            } => hue_saturation(c, *hue, *saturation, *lightness),
            Self::Invert => c.map(|v| 1.0 - v),
            Self::Threshold { level } => {
                let lum = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
                if lum * 255.0 >= f32::from(*level) {
                    [1.0, 1.0, 1.0]
                } else {
                    [0.0, 0.0, 0.0]
                }
            }
        }
    }

    /// The adjustment ready for many pixels: per-channel ones become a table.
    #[must_use]
    pub fn prepare(&self) -> Prepared {
        match self {
            Self::BrightnessContrast { .. } | Self::Levels { .. } | Self::Curves { .. } | Self::Invert => {
                let table = (0..LUT_SIZE)
                    .map(|i| {
                        let v = i as f32 / (LUT_SIZE - 1) as f32;
                        self.apply([v, v, v])[0]
                    })
                    .collect();
                Prepared::Table(table)
            }
            other => Prepared::Direct(other.clone()),
        }
    }
}

const LUT_SIZE: usize = 1024;

/// An adjustment prepared for compositing ([`Adjustment::prepare`]).
#[derive(Clone, Debug)]
pub enum Prepared {
    /// The same map on every channel, sampled at `LUT_SIZE` points.
    Table(Vec<f32>),
    Direct(Adjustment),
}

impl Prepared {
    #[inline]
    #[must_use]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        match self {
            Self::Table(t) => c.map(|v| {
                let x = v.clamp(0.0, 1.0) * (LUT_SIZE - 1) as f32;
                let i = (x as usize).min(LUT_SIZE - 2);
                let f = x - i as f32;
                t[i] + (t[i + 1] - t[i]) * f
            }),
            Self::Direct(a) => a.apply(c),
        }
    }
}

/// Contrast -1..1 as a slope around mid grey: -1 flattens to grey, 0 keeps,
/// towards 1 steepens without bound.
fn contrast_factor(contrast: f32) -> f32 {
    let c = contrast.clamp(-1.0, 1.0);
    if c >= 0.0 {
        1.0 / (1.0 - c * 0.99)
    } else {
        1.0 + c
    }
}

/// Levels and Curves as one per-channel map.
struct ChannelCurve {
    /// Sorted (x, y) knots in 0..1, and their tangents (monotone cubic).
    xs: Vec<f32>,
    ys: Vec<f32>,
    ms: Vec<f32>,
    levels: Option<(f32, f32, f32, f32, f32)>,
}

impl ChannelCurve {
    fn of(adj: &Adjustment) -> Self {
        match adj {
            Adjustment::Levels {
                in_black,
                in_white,
                gamma,
                out_black,
                out_white,
            } => Self {
                xs: Vec::new(),
                ys: Vec::new(),
                ms: Vec::new(),
                levels: Some((
                    f32::from(*in_black) / 255.0,
                    f32::from(*in_white) / 255.0,
                    gamma.max(0.01),
                    f32::from(*out_black) / 255.0,
                    f32::from(*out_white) / 255.0,
                )),
            },
            Adjustment::Curves { points } => {
                let mut pts: Vec<(f32, f32)> = points
                    .iter()
                    .map(|(x, y)| (f32::from(*x) / 255.0, f32::from(*y) / 255.0))
                    .collect();
                pts.sort_by(|a, b| a.0.total_cmp(&b.0));
                pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6);
                if pts.is_empty() {
                    pts = vec![(0.0, 0.0), (1.0, 1.0)];
                }
                let xs: Vec<f32> = pts.iter().map(|p| p.0).collect();
                let ys: Vec<f32> = pts.iter().map(|p| p.1).collect();
                let ms = monotone_tangents(&xs, &ys);
                Self {
                    xs,
                    ys,
                    ms,
                    levels: None,
                }
            }
            _ => Self {
                xs: vec![0.0, 1.0],
                ys: vec![0.0, 1.0],
                ms: vec![1.0, 1.0],
                levels: None,
            },
        }
    }

    fn eval(&self, v: f32) -> f32 {
        if let Some((ib, iw, gamma, ob, ow)) = self.levels {
            let span = (iw - ib).max(1.0 / 255.0);
            let t = ((v - ib) / span).clamp(0.0, 1.0).powf(1.0 / gamma);
            return (ob + t * (ow - ob)).clamp(0.0, 1.0);
        }
        let n = self.xs.len();
        if n == 1 {
            return self.ys[0];
        }
        if v <= self.xs[0] {
            return self.ys[0];
        }
        if v >= self.xs[n - 1] {
            return self.ys[n - 1];
        }
        let k = self.xs.partition_point(|x| *x <= v).saturating_sub(1).min(n - 2);
        let h = self.xs[k + 1] - self.xs[k];
        let t = (v - self.xs[k]) / h;
        let (t2, t3) = (t * t, t * t * t);
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        (h00 * self.ys[k] + h10 * h * self.ms[k] + h01 * self.ys[k + 1] + h11 * h * self.ms[k + 1])
            .clamp(0.0, 1.0)
    }
}

/// Fritsch-Carlson tangents: a cubic through the knots that never overshoots.
fn monotone_tangents(xs: &[f32], ys: &[f32]) -> Vec<f32> {
    let n = xs.len();
    if n < 2 {
        return vec![0.0; n];
    }
    let d: Vec<f32> = (0..n - 1)
        .map(|k| (ys[k + 1] - ys[k]) / (xs[k + 1] - xs[k]).max(1e-6))
        .collect();
    let mut m = vec![0.0; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for k in 1..n - 1 {
        m[k] = if d[k - 1] * d[k] <= 0.0 {
            0.0
        } else {
            (d[k - 1] + d[k]) * 0.5
        };
    }
    for k in 0..n - 1 {
        if d[k].abs() < 1e-9 {
            m[k] = 0.0;
            m[k + 1] = 0.0;
            continue;
        }
        let a = m[k] / d[k];
        let b = m[k + 1] / d[k];
        let s = a * a + b * b;
        if s > 9.0 {
            let tau = 3.0 / s.sqrt();
            m[k] = tau * a * d[k];
            m[k + 1] = tau * b * d[k];
        }
    }
    m
}

/// RGB (0..1) to HSL (all 0..1).
#[must_use]
pub fn rgb_to_hsl(c: [f32; 3]) -> [f32; 3] {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let l = (max + min) * 0.5;
    if (max - min).abs() < 1e-6 {
        return [0.0, 0.0, l];
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == c[0] {
        (c[1] - c[2]) / d + if c[1] < c[2] { 6.0 } else { 0.0 }
    } else if max == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    };
    [h / 6.0, s, l]
}

/// HSL (all 0..1) to RGB (0..1).
#[must_use]
pub fn hsl_to_rgb(hsl: [f32; 3]) -> [f32; 3] {
    let [h, s, l] = hsl;
    if s <= 0.0 {
        return [l, l, l];
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let channel = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0)]
}

fn hue_saturation(c: [f32; 3], hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
    let [h, s, l] = rgb_to_hsl(c);
    let h = (h + hue / 360.0).rem_euclid(1.0);
    let sat = saturation.clamp(-1.0, 1.0);
    let s = if sat < 0.0 { s * (1.0 + sat) } else { s + (1.0 - s) * sat };
    let light = lightness.clamp(-1.0, 1.0);
    let l = if light < 0.0 { l * (1.0 + light) } else { l + (1.0 - l) * light };
    hsl_to_rgb([h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0)])
}
