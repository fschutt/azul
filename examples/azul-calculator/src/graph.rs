//! The graph's arithmetic: the viewport (where the plot looks, how far it
//! is zoomed), the grid's ticks, the functions compiled to `f64` and
//! sampled per pixel column, and the curves as an SVG the window turns into
//! DOM nodes (`Dom::create_from_parsed_xml_fragment` - azul draws `<svg>`
//! paths itself, crisp at any size).
//!
//! The viewport keeps the units SQUARE: one `scale` (units per pixel) for
//! both axes, so a circle is a circle whatever shape the plot has. A resize
//! shows more or less of the plane around the same centre; the wheel zooms
//! about the pointer; a drag moves the plane under it.
//!
//! The graph plots in RADIANS whatever the calculator's angle unit is (a
//! sine over -10..10 in degrees is a straight line); the plot says so.
//!
//! Pure: every function here is a unit test.

use crate::expr::{BinOp, Const, Expr, Func, Post};

/// Where the plot looks: its centre and its zoom (units per pixel).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub cx: f64,
    pub cy: f64,
    /// Units per pixel, the same on both axes.
    pub scale: f64,
}

/// The x range a fresh graph shows across its width.
pub const DEFAULT_SPAN: f64 = 20.0;

/// The zoom's limits (units per pixel).
const MIN_SCALE: f64 = 1e-9;
const MAX_SCALE: f64 = 1e9;

impl Viewport {
    /// -10..10 across a plot `width` px wide, the origin in the middle.
    #[must_use]
    pub fn fresh(width: f32) -> Viewport {
        Viewport {
            cx: 0.0,
            cy: 0.0,
            scale: DEFAULT_SPAN / f64::from(width.max(1.0)),
        }
    }

    /// The x range across `w` px.
    #[must_use]
    pub fn x_range(&self, w: f32) -> (f64, f64) {
        let half = f64::from(w) * self.scale / 2.0;
        (self.cx - half, self.cx + half)
    }

    /// The y range across `h` px (bottom, top).
    #[must_use]
    pub fn y_range(&self, h: f32) -> (f64, f64) {
        let half = f64::from(h) * self.scale / 2.0;
        (self.cy - half, self.cy + half)
    }

    /// The pixel of the point `(x, y)` in a `w` x `h` plot (y grows down).
    #[must_use]
    pub fn to_px(&self, x: f64, y: f64, w: f32, h: f32) -> (f64, f64) {
        (
            (x - self.cx) / self.scale + f64::from(w) / 2.0,
            (self.cy - y) / self.scale + f64::from(h) / 2.0,
        )
    }

    /// The point under the pixel `(px, py)`.
    #[must_use]
    pub fn from_px(&self, px: f64, py: f64, w: f32, h: f32) -> (f64, f64) {
        (
            self.cx + (px - f64::from(w) / 2.0) * self.scale,
            self.cy - (py - f64::from(h) / 2.0) * self.scale,
        )
    }

    /// Zooms by `factor` (< 1 closer) about the pixel `(px, py)`: the point
    /// under it stays under it.
    pub fn zoom_at(&mut self, factor: f64, px: f64, py: f64, w: f32, h: f32) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let (x0, y0) = self.from_px(px, py, w, h);
        self.scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        self.cx = x0 - (px - f64::from(w) / 2.0) * self.scale;
        self.cy = y0 + (py - f64::from(h) / 2.0) * self.scale;
    }

    /// The plane moved by `(dx, dy)` px under the pointer, from `start`.
    #[must_use]
    pub fn dragged(start: Viewport, dx: f64, dy: f64) -> Viewport {
        Viewport {
            cx: start.cx - dx * start.scale,
            cy: start.cy + dy * start.scale,
            scale: start.scale,
        }
    }

    /// The viewport as a script reads it: `x -10..10 y -6.25..6.25`.
    #[must_use]
    pub fn describe(&self, w: f32, h: f32) -> String {
        let (x0, x1) = self.x_range(w);
        let (y0, y1) = self.y_range(h);
        format!("x {}..{} y {}..{}", short(x0), short(x1), short(y0), short(y1))
    }
}

/// A number in four significant digits, for scripts and labels.
fn short(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return "0".to_string();
    }
    let digits = (4 - 1 - v.abs().log10().floor() as i32).clamp(0, 12) as usize;
    let text = format!("{v:.digits$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    };
    if text == "-0" {
        "0".to_string()
    } else {
        text
    }
}

/// The grid step for about `px_per_step` pixels between lines: 1, 2 or 5
/// times a power of ten.
#[must_use]
pub fn nice_step(scale: f64, px_per_step: f64) -> f64 {
    let raw = (scale * px_per_step).max(f64::MIN_POSITIVE);
    let power = 10f64.powf(raw.log10().floor());
    let m = raw / power;
    let nice = if m <= 1.0 {
        1.0
    } else if m <= 2.0 {
        2.0
    } else if m <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * power
}

/// The multiples of `step` inside `lo..=hi` (at most 400 of them).
#[must_use]
pub fn ticks(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    if !(step > 0.0) || !lo.is_finite() || !hi.is_finite() || hi < lo {
        return Vec::new();
    }
    let first = (lo / step).ceil() as i64;
    let last = (hi / step).floor() as i64;
    if last < first || last - first > 400 {
        return Vec::new();
    }
    (first..=last).map(|k| k as f64 * step).collect()
}

/// A tick's label: as many decimals as the step needs, a true minus sign,
/// scientific notation far from 1.
#[must_use]
pub fn tick_label(v: f64, step: f64) -> String {
    if v.abs() < step * 1e-6 {
        return "0".to_string();
    }
    let text = if v.abs() >= 1e6 || v.abs() < 1e-4 {
        let exp = v.abs().log10().floor() as i32;
        let mantissa = v / 10f64.powi(exp);
        let m = format!("{mantissa:.2}");
        let m = m.trim_end_matches('0').trim_end_matches('.').to_string();
        format!("{m}e{exp}")
    } else {
        let decimals = (-step.log10().floor()).max(0.0) as usize;
        format!("{v:.decimals$}")
    };
    text.replacen('-', "\u{2212}", 1)
}

// ==== Functions as f64 ====

/// An expression compiled for plotting: numbers parsed once, `x` a slot.
#[derive(Clone, Debug, PartialEq)]
pub enum Fx {
    C(f64),
    X,
    Neg(Box<Fx>),
    Bin(BinOp, Box<Fx>, Box<Fx>),
    /// `a + b%` and `a - b%`: `b` percent OF `a`.
    PercentOf(bool, Box<Fx>, Box<Fx>),
    Func(Func, Box<Fx>),
    Post(Post, Box<Fx>),
}

/// `e` compiled; `None` if it has what a plot cannot evaluate (a bitwise
/// operator, a number that does not parse).
#[must_use]
pub fn compile(e: &Expr) -> Option<Fx> {
    Some(match e {
        Expr::Num(text) => Fx::C(parse_f64(text)?),
        Expr::Const(Const::Pi) => Fx::C(std::f64::consts::PI),
        Expr::Const(Const::E) => Fx::C(std::f64::consts::E),
        Expr::Var => Fx::X,
        Expr::Neg(x) => Fx::Neg(Box::new(compile(x)?)),
        Expr::Not(_) => return None,
        Expr::Bin(op, l, r) => {
            if op.is_bitwise() {
                return None;
            }
            if let (BinOp::Add | BinOp::Sub, Expr::Post(Post::Percent, pct)) = (op, r.as_ref()) {
                return Some(Fx::PercentOf(
                    *op == BinOp::Add,
                    Box::new(compile(l)?),
                    Box::new(compile(pct)?),
                ));
            }
            Fx::Bin(*op, Box::new(compile(l)?), Box::new(compile(r)?))
        }
        Expr::Func(f, x) => Fx::Func(*f, Box::new(compile(x)?)),
        Expr::Post(p, x) => Fx::Post(*p, Box::new(compile(x)?)),
    })
}

/// A typed number (`1,280`, `1.5E3`, `-2`) as `f64`.
fn parse_f64(text: &str) -> Option<f64> {
    let t: String = text
        .chars()
        .filter(|c| !matches!(c, ',' | ' ' | '_'))
        .map(|c| if c == '\u{2212}' { '-' } else { c })
        .collect();
    let t = t.trim_end_matches(['E', 'e', '.']);
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Γ(z) by Lanczos (g = 7): `n!` between the integers.
fn gamma(z: f64) -> f64 {
    const G: f64 = 7.0;
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if z < 0.5 {
        // Reflection: Γ(z) Γ(1 - z) = π / sin(πz).
        let s = (std::f64::consts::PI * z).sin();
        if s == 0.0 {
            return f64::NAN;
        }
        return std::f64::consts::PI / (s * gamma(1.0 - z));
    }
    let z = z - 1.0;
    let mut a = C[0];
    let t = z + G + 0.5;
    for (i, c) in C.iter().enumerate().skip(1) {
        a += c / (z + i as f64);
    }
    (2.0 * std::f64::consts::PI).sqrt() * t.powf(z + 0.5) * (-t).exp() * a
}

/// `x!`: exact for small whole numbers, Γ(x + 1) between them, undefined
/// at the negative integers.
fn factorial(x: f64) -> f64 {
    if x.fract() == 0.0 {
        if x < 0.0 {
            return f64::NAN;
        }
        if x <= 170.0 {
            return (2..=x as u32).fold(1.0, |acc, k| acc * f64::from(k));
        }
        return f64::INFINITY;
    }
    gamma(x + 1.0)
}

impl Fx {
    /// The value at `x` (NaN where it is undefined).
    #[must_use]
    pub fn eval(&self, x: f64) -> f64 {
        match self {
            Fx::C(c) => *c,
            Fx::X => x,
            Fx::Neg(a) => -a.eval(x),
            Fx::PercentOf(add, a, b) => {
                let base = a.eval(x);
                let delta = base * b.eval(x) / 100.0;
                if *add {
                    base + delta
                } else {
                    base - delta
                }
            }
            Fx::Bin(op, a, b) => {
                let (a, b) = (a.eval(x), b.eval(x));
                match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    BinOp::Div => a / b,
                    BinOp::Mod => a % b,
                    BinOp::Pow => pow(a, b),
                    _ => f64::NAN,
                }
            }
            Fx::Func(f, a) => func(*f, a.eval(x)),
            Fx::Post(p, a) => {
                let v = a.eval(x);
                match p {
                    Post::Percent => v / 100.0,
                    Post::Factorial => factorial(v),
                    Post::Square => v * v,
                    Post::Cube => v * v * v,
                }
            }
        }
    }
}

/// `a^b`, with the odd roots of negative numbers (`x^(1/3)`) real.
fn pow(a: f64, b: f64) -> f64 {
    if a < 0.0 && b.fract() != 0.0 {
        let recip = 1.0 / b;
        if recip.fract() == 0.0 && (recip as i64) % 2 != 0 {
            return -(-a).powf(b);
        }
    }
    a.powf(b)
}

fn func(f: Func, v: f64) -> f64 {
    match f {
        Func::Sin => v.sin(),
        Func::Cos => v.cos(),
        Func::Tan => v.tan(),
        Func::Asin => v.asin(),
        Func::Acos => v.acos(),
        Func::Atan => v.atan(),
        Func::Sinh => v.sinh(),
        Func::Cosh => v.cosh(),
        Func::Tanh => v.tanh(),
        Func::Asinh => v.asinh(),
        Func::Acosh => v.acosh(),
        Func::Atanh => v.atanh(),
        Func::Ln => v.ln(),
        Func::Log => v.log10(),
        Func::Log2 => v.log2(),
        Func::Sqrt => v.sqrt(),
        Func::Cbrt => v.cbrt(),
        Func::Abs => v.abs(),
        Func::Exp => v.exp(),
        Func::Pow10 => 10f64.powf(v),
        Func::Recip => 1.0 / v,
    }
}

// ==== Sampling ====

/// The curve of `fx` over a `w` x `h` plot, in pixels: one point per
/// `step_px` column, split into runs where the function is undefined or
/// jumps across the whole plot (an asymptote: `tan(x)`, `1/x`). Points far
/// outside the plot are pulled in to a margin, so a steep line still runs
/// off the edge without huge coordinates.
#[must_use]
pub fn sample(fx: &Fx, vp: &Viewport, w: f32, h: f32, step_px: f32) -> Vec<Vec<(f32, f32)>> {
    let mut runs: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut run: Vec<(f32, f32)> = Vec::new();
    let margin = f64::from(h);
    let (top, bottom) = (-margin, f64::from(h) + margin);
    let columns = (w / step_px.max(0.5)).ceil() as usize;
    let mut previous: Option<f64> = None;
    for i in 0..=columns {
        let px = (i as f32 * step_px).min(w);
        let (x, _) = vp.from_px(f64::from(px), 0.0, w, h);
        let y = fx.eval(x);
        if !y.is_finite() {
            if run.len() > 1 {
                runs.push(std::mem::take(&mut run));
            }
            run.clear();
            previous = None;
            continue;
        }
        let (_, py) = vp.to_px(x, y, w, h);
        if let Some(prev) = previous {
            // An asymptote: from beyond one edge to beyond the other.
            let jumped = (prev < 0.0 && py > f64::from(h)) || (prev > f64::from(h) && py < 0.0);
            if jumped {
                if run.len() > 1 {
                    runs.push(std::mem::take(&mut run));
                }
                run.clear();
            }
        }
        previous = Some(py);
        #[allow(clippy::cast_possible_truncation)] // pixels
        run.push((px, py.clamp(top, bottom) as f32));
    }
    if run.len() > 1 {
        runs.push(run);
    }
    runs
}

/// An SVG path's `d` for the runs (`M x y L x y ...`), at a tenth of a pixel.
#[must_use]
pub fn path_d(runs: &[Vec<(f32, f32)>]) -> String {
    let mut d = String::new();
    for run in runs {
        for (i, (x, y)) in run.iter().enumerate() {
            if !d.is_empty() {
                d.push(' ');
            }
            d.push(if i == 0 { 'M' } else { 'L' });
            d.push_str(&format!("{x:.1} {y:.1}"));
        }
    }
    d
}

/// One curve of the SVG.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    /// The `<path>`'s DOM id.
    pub id: String,
    pub d: String,
    /// `#rrggbb`.
    pub color: String,
    pub width: f32,
    /// Drawn fainter: the entry being typed, not yet a committed function.
    pub draft: bool,
}

/// The `<svg>` of the curves over a `w` x `h` plot: user units are pixels.
#[must_use]
pub fn svg(w: f32, h: f32, curves: &[Curve]) -> String {
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w:.0} {h:.0}\" \
         style=\"position: absolute; left: 0px; top: 0px; width: 100%; height: 100%;\">"
    );
    for c in curves {
        if c.d.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "<path id=\"{}\" d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.1}\"{}/>",
            c.id,
            c.d,
            c.color,
            c.width,
            if c.draft { " style=\"opacity: 0.55;\"" } else { "" }
        ));
    }
    out.push_str("</svg>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{parse, tokenize, Domain};

    fn fx(text: &str) -> Fx {
        compile(&parse(&tokenize(text, Domain::Decimal).unwrap()).unwrap()).unwrap()
    }

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn the_viewport_maps_points_to_pixels_and_back() {
        let vp = Viewport::fresh(400.0);
        assert!(close(vp.x_range(400.0), (-10.0, 10.0)));
        assert!(close(vp.y_range(200.0), (-5.0, 5.0)), "square units");
        assert!(close(vp.to_px(0.0, 0.0, 400.0, 200.0), (200.0, 100.0)));
        assert!(close(vp.to_px(10.0, 5.0, 400.0, 200.0), (400.0, 0.0)), "y grows up");
        assert!(close(vp.from_px(300.0, 50.0, 400.0, 200.0), (5.0, 2.5)));
        assert_eq!(vp.describe(400.0, 200.0), "x -10..10 y -5..5");
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer() {
        let mut vp = Viewport::fresh(400.0);
        let before = vp.from_px(300.0, 50.0, 400.0, 200.0);
        vp.zoom_at(0.5, 300.0, 50.0, 400.0, 200.0);
        let after = vp.from_px(300.0, 50.0, 400.0, 200.0);
        assert!((before.0 - after.0).abs() < 1e-9 && (before.1 - after.1).abs() < 1e-9);
        assert!((vp.x_range(400.0).1 - vp.x_range(400.0).0 - 10.0).abs() < 1e-9, "half the span");
    }

    #[test]
    fn a_drag_moves_the_plane_with_the_pointer() {
        let start = Viewport::fresh(400.0);
        let vp = Viewport::dragged(start, 100.0, -50.0);
        // Dragged right by 100 px (5 units): the centre is now 5 units left.
        assert!((vp.cx + 5.0).abs() < 1e-12);
        assert!((vp.cy + 2.5).abs() < 1e-12, "dragged up: the centre goes down");
    }

    #[test]
    fn grid_steps_are_one_two_or_five() {
        assert_eq!(nice_step(0.05, 80.0), 5.0);
        assert_eq!(nice_step(0.05, 30.0), 2.0);
        assert_eq!(nice_step(0.001, 80.0), 0.1);
        assert_eq!(ticks(-1.0, 1.0, 0.5), vec![-1.0, -0.5, 0.0, 0.5, 1.0]);
        assert_eq!(ticks(0.1, 0.9, 1.0), Vec::<f64>::new());
        assert_eq!(tick_label(-2.0, 1.0), "\u{2212}2");
        assert_eq!(tick_label(0.5, 0.5), "0.5");
        assert_eq!(tick_label(1e-12, 1.0), "0");
        assert_eq!(tick_label(2_000_000.0, 1_000_000.0), "2e6");
    }

    #[test]
    fn compiled_functions_evaluate_in_radians() {
        assert!((fx("sin(x)*x^2").eval(2.0) - 2f64.sin() * 4.0).abs() < 1e-12);
        assert_eq!(fx("2x+1").eval(3.0), 7.0);
        assert!(fx("sqrt(x)").eval(-1.0).is_nan());
        assert!((fx("x!").eval(4.0) - 24.0).abs() < 1e-9);
        assert!((fx("x!").eval(0.5) - 0.886_226_925_452_758).abs() < 1e-9, "Γ(1.5)");
        assert!((fx("x^(1/3)").eval(-8.0) + 2.0).abs() < 1e-12, "odd roots of negatives");
        assert_eq!(fx("50 + x%").eval(10.0), 55.0);
        assert!((fx("pi").eval(0.0) - std::f64::consts::PI).abs() < 1e-15);
    }

    #[test]
    fn sampling_breaks_at_asymptotes_and_gaps() {
        let vp = Viewport::fresh(400.0);
        let runs = sample(&fx("1/x"), &vp, 400.0, 200.0, 2.0);
        assert_eq!(runs.len(), 2, "1/x: one branch each side of 0");
        let runs = sample(&fx("sqrt(x)"), &vp, 400.0, 200.0, 2.0);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].first().unwrap().0 >= 200.0, "nothing left of x = 0");
        let runs = sample(&fx("x"), &vp, 400.0, 200.0, 2.0);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].len(), 201);
        let d = path_d(&runs);
        assert!(d.starts_with("M0.0 300.0 L2.0 298.0"), "{}", &d[..40]);
    }

    #[test]
    fn the_svg_has_a_path_per_curve() {
        let s = svg(
            400.0,
            200.0,
            &[Curve {
                id: "c0".into(),
                d: "M0 0 L1 1".into(),
                color: "#2A63B8".into(),
                width: 2.0,
                draft: false,
            }],
        );
        assert!(s.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 400 200\""));
        assert!(s.contains("<path id=\"c0\" d=\"M0 0 L1 1\" fill=\"none\" stroke=\"#2A63B8\" stroke-width=\"2.0\"/>"));
        assert!(s.ends_with("</svg>"));
    }
}
