//! What a metal reflects: the desktop behind the window.
//!
//! A polished metal surface mirrors the room it is in. For a window that room
//! is the desktop: the wallpaper behind the window, at the window's place on
//! the screen, moved by the tilt of the surface and blurred by its
//! roughness. Without a wallpaper (no reader on this platform, a headless
//! test) the metal reflects the system's text highlight colour as a flat
//! light instead.
//!
//! The platform readers (`azul_dll`'s `desktop::wallpaper`) hand the
//! wallpaper of each screen to [`set_desktop_wallpapers`], or register a
//! loader with [`set_wallpaper_loader`] that runs the first time a metal
//! asks. A [`Wallpaper`] keeps a small mip chain (512 px wide at most), so a
//! blurred reflection is a cheap lookup, on the CPU as on the GPU.

use std::sync::{Arc, Mutex, OnceLock};

use super::materials::srgb_to_linear;

/// The widest a wallpaper is kept: a reflection is always blurred (mip
/// level 1.5 and up), so more pixels would only cost memory.
pub const WALLPAPER_WIDTH: usize = 512;

/// One screen's wallpaper, ready to be reflected.
#[derive(Debug, Clone, PartialEq)]
pub struct Wallpaper {
    /// The screen it covers, desktop points: x, y, width, height.
    pub screen: [f32; 4],
    /// The base image in sRGB bytes, RGBA, row by row from the top - what a
    /// GPU uploads (and builds its own mips from).
    pub base_rgba: Vec<u8>,
    /// The base's width and height, pixels.
    pub base_size: [usize; 2],
    /// The mip chain in linear light, from the base down to one pixel: what
    /// the CPU samples.
    mips: Vec<Mip>,
    /// A number that is new for every wallpaper made (a cache key: a render
    /// that reflected an older wallpaper renders again).
    pub id: u64,
}

#[derive(Debug, Clone, PartialEq)]
struct Mip {
    width: usize,
    height: usize,
    texels: Vec<[f32; 3]>,
}

impl Wallpaper {
    /// A wallpaper from RGBA8 pixels (`width` x `height`, straight alpha,
    /// sRGB) covering the screen rect `screen` (desktop points: x, y, width,
    /// height). Scaled down to [`WALLPAPER_WIDTH`] when wider. `None` for an
    /// empty image or too few bytes.
    #[must_use]
    pub fn from_rgba8(width: usize, height: usize, rgba: &[u8], screen: [f32; 4]) -> Option<Self> {
        if width == 0 || height == 0 || rgba.len() < width * height * 4 {
            return None;
        }
        let (base_rgba, base_size) = if width > WALLPAPER_WIDTH {
            let w = WALLPAPER_WIDTH;
            let h = ((height * w) as f32 / width as f32).round().max(1.0) as usize;
            (box_scale(rgba, width, height, w, h), [w, h])
        } else {
            (rgba[..width * height * 4].to_vec(), [width, height])
        };
        let base: Vec<[f32; 3]> = base_rgba
            .chunks_exact(4)
            .map(|p| [p[0], p[1], p[2]].map(|c| srgb_to_linear(f32::from(c) / 255.0)))
            .collect();
        let mut mips = vec![Mip {
            width: base_size[0],
            height: base_size[1],
            texels: base,
        }];
        while let Some(last) = mips.last() {
            if last.width == 1 && last.height == 1 {
                break;
            }
            let next = halve(last);
            mips.push(next);
        }
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Some(Self {
            screen,
            base_rgba,
            base_size,
            mips,
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        })
    }

    /// The colour at `uv` (0..1 across the screen, clamped) blurred to mip
    /// `lod` (0 = the base): trilinear, linear light. The GPU's
    /// `textureLod` on the base with its mips.
    #[must_use]
    pub fn sample(&self, uv: [f32; 2], lod: f32) -> [f32; 3] {
        let top = (self.mips.len() - 1) as f32;
        let lod = lod.clamp(0.0, top);
        let lo = lod.floor() as usize;
        let hi = (lo + 1).min(self.mips.len() - 1);
        let t = lod - lo as f32;
        let a = self.mips[lo].bilinear(uv);
        if hi == lo || t <= 0.0 {
            return a;
        }
        let b = self.mips[hi].bilinear(uv);
        [0, 1, 2].map(|c| a[c] * (1.0 - t) + b[c] * t)
    }
}

impl Mip {
    fn bilinear(&self, uv: [f32; 2]) -> [f32; 3] {
        let u = uv[0].clamp(0.0, 1.0) * self.width as f32 - 0.5;
        let v = uv[1].clamp(0.0, 1.0) * self.height as f32 - 0.5;
        let (fx, fy) = (u.floor(), v.floor());
        let (tx, ty) = (u - fx, v - fy);
        let cx = |i: f32| (i.max(0.0) as usize).min(self.width - 1);
        let cy = |i: f32| (i.max(0.0) as usize).min(self.height - 1);
        let (x0, x1, y0, y1) = (cx(fx), cx(fx + 1.0), cy(fy), cy(fy + 1.0));
        let at = |x: usize, y: usize| self.texels[y * self.width + x];
        let mut out = [0.0; 3];
        for (c, slot) in out.iter_mut().enumerate() {
            let top = at(x0, y0)[c] * (1.0 - tx) + at(x1, y0)[c] * tx;
            let bottom = at(x0, y1)[c] * (1.0 - tx) + at(x1, y1)[c] * tx;
            *slot = top * (1.0 - ty) + bottom * ty;
        }
        out
    }
}

/// The next mip: every 2 x 2 texels averaged (an odd edge keeps its last
/// texel).
fn halve(m: &Mip) -> Mip {
    let (w, h) = ((m.width / 2).max(1), (m.height / 2).max(1));
    let mut texels = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let xs = [(2 * x).min(m.width - 1), (2 * x + 1).min(m.width - 1)];
            let ys = [(2 * y).min(m.height - 1), (2 * y + 1).min(m.height - 1)];
            let mut sum = [0.0; 3];
            for yy in ys {
                for xx in xs {
                    let t = m.texels[yy * m.width + xx];
                    for c in 0..3 {
                        sum[c] += t[c] * 0.25;
                    }
                }
            }
            texels.push(sum);
        }
    }
    Mip {
        width: w,
        height: h,
        texels,
    }
}

/// `rgba` (`w` x `h`) scaled down to `nw` x `nh` by averaging the source
/// pixels each target pixel covers.
fn box_scale(rgba: &[u8], w: usize, h: usize, nw: usize, nh: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(nw * nh * 4);
    for y in 0..nh {
        let y0 = y * h / nh;
        let y1 = ((y + 1) * h / nh).max(y0 + 1).min(h);
        for x in 0..nw {
            let x0 = x * w / nw;
            let x1 = ((x + 1) * w / nw).max(x0 + 1).min(w);
            let mut sum = [0_u32; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let i = (sy * w + sx) * 4;
                    for c in 0..4 {
                        sum[c] += u32::from(rgba[i + c]);
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            out.extend(sum.map(|s| ((s + n / 2) / n) as u8));
        }
    }
    out
}

/// What a surface reflects: the wallpapers of the desktop, or a flat colour.
#[derive(Debug, Clone, PartialEq)]
pub enum Environment {
    /// The system's highlight colour, linear, already dimmed to 90%.
    Highlight([f32; 3]),
    /// The wallpaper of each screen.
    Wallpapers(Arc<Vec<Arc<Wallpaper>>>),
}

impl Default for Environment {
    /// macOS's default text highlight (a pale blue).
    fn default() -> Self {
        Self::highlight([0xB3, 0xD7, 0xFF])
    }
}

impl Environment {
    /// A flat environment of the highlight colour `srgb` at 90%.
    #[must_use]
    pub fn highlight(srgb: [u8; 3]) -> Self {
        Self::Highlight(srgb.map(|c| srgb_to_linear(f32::from(c) / 255.0) * 0.9))
    }

    /// The desktop's wallpapers.
    #[must_use]
    pub fn wallpapers(list: Vec<Arc<Wallpaper>>) -> Self {
        if list.is_empty() {
            Self::default()
        } else {
            Self::Wallpapers(Arc::new(list))
        }
    }

    /// The wallpaper of the screen the desktop point `at` lies on (the
    /// nearest screen when none contains it); `None` for a flat environment.
    #[must_use]
    pub fn wallpaper_at(&self, at: [f32; 2]) -> Option<&Arc<Wallpaper>> {
        let Self::Wallpapers(list) = self else {
            return None;
        };
        let distance = |w: &Wallpaper| {
            let [x, y, sw, sh] = w.screen;
            let dx = (x - at[0]).max(at[0] - (x + sw)).max(0.0);
            let dy = (y - at[1]).max(at[1] - (y + sh)).max(0.0);
            dx * dx + dy * dy
        };
        list.iter()
            .min_by(|a, b| distance(a).total_cmp(&distance(b)))
    }

    /// The flat colour reflected where no wallpaper is (linear): the
    /// highlight, or the default highlight under wallpapers.
    #[must_use]
    pub fn flat_colour(&self) -> [f32; 3] {
        match self {
            Self::Highlight(c) => *c,
            Self::Wallpapers(_) => match Self::default() {
                Self::Highlight(c) => c,
                Self::Wallpapers(_) => [0.6; 3],
            },
        }
    }

    /// A number that changes when what is reflected changes (a cache key).
    #[must_use]
    pub fn key(&self) -> u64 {
        match self {
            Self::Highlight(c) => {
                let b = c.map(f32::to_bits);
                (u64::from(b[0]) << 32) ^ (u64::from(b[1]) << 16) ^ u64::from(b[2])
            }
            Self::Wallpapers(list) => list
                .iter()
                .fold(0x9E37_79B9_7F4A_7C15, |acc, w| acc.rotate_left(7) ^ w.id),
        }
    }
}

/// A function that reads the desktop's wallpapers (a platform reader).
pub type WallpaperLoader = fn() -> Vec<Wallpaper>;

struct Desktop {
    loader: Option<WallpaperLoader>,
    loaded: bool,
    wallpapers: Arc<Vec<Arc<Wallpaper>>>,
    generation: u64,
}

fn desktop() -> &'static Mutex<Desktop> {
    static DESKTOP: OnceLock<Mutex<Desktop>> = OnceLock::new();
    DESKTOP.get_or_init(|| {
        Mutex::new(Desktop {
            loader: None,
            loaded: false,
            wallpapers: Arc::new(Vec::new()),
            generation: 0,
        })
    })
}

/// Set the desktop's wallpapers (one per screen) directly: what a platform
/// reader found, or a test's picture. Every metal reflects them from its
/// next render on.
pub fn set_desktop_wallpapers(list: Vec<Wallpaper>) {
    let mut d = desktop().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    d.wallpapers = Arc::new(list.into_iter().map(Arc::new).collect());
    d.loaded = true;
    d.generation = d.generation.wrapping_add(1);
}

/// Register the platform's reader. It runs the first time a metal asks for
/// the desktop ([`desktop_environment`]) and again after
/// [`refresh_desktop_wallpapers`].
pub fn set_wallpaper_loader(loader: WallpaperLoader) {
    let mut d = desktop().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    d.loader = Some(loader);
    d.loaded = false;
}

/// Read the wallpapers again now (the user may have changed them while the
/// app was in the background). Returns whether they changed: the windows'
/// material frames then render again (their inputs carry
/// [`desktop_generation`]).
pub fn refresh_desktop_wallpapers() -> bool {
    let loader = desktop()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .loader;
    let Some(load) = loader else {
        return false;
    };
    // The reader runs outside the lock: it may decode an image.
    let list = load();
    adopt(list)
}

/// Install `list` as the desktop's wallpapers; the generation moves only
/// when they differ from the ones there (a reader that found the same files
/// hands back the same wallpapers).
fn adopt(list: Vec<Wallpaper>) -> bool {
    let mut d = desktop().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    d.loaded = true;
    let same = d.wallpapers.len() == list.len()
        && d.wallpapers.iter().zip(&list).all(|(a, b)| a.id == b.id);
    if same {
        return false;
    }
    d.wallpapers = Arc::new(list.into_iter().map(Arc::new).collect());
    d.generation = d.generation.wrapping_add(1);
    true
}

/// What the desktop gives a metal to reflect: its wallpapers if any are
/// known (loading them now if a reader is registered and has not run), else
/// the highlight colour `highlight` (sRGB).
#[must_use]
pub fn desktop_environment(highlight: [u8; 3]) -> Environment {
    let loader = {
        let d = desktop().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if d.loaded {
            None
        } else {
            d.loader
        }
    };
    if let Some(load) = loader {
        let list = load();
        adopt(list);
    }
    let d = desktop().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if d.wallpapers.is_empty() {
        Environment::highlight(highlight)
    } else {
        Environment::Wallpapers(d.wallpapers.clone())
    }
}

/// How many times the desktop's wallpapers changed (a window compares it to
/// re-render its metals).
#[must_use]
pub fn desktop_generation() -> u64 {
    desktop()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .generation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wallpaper_is_kept_small_and_blurs_down_its_mips() {
        // A 1024 x 512 picture, left half black, right half white.
        let mut rgba = Vec::with_capacity(1024 * 512 * 4);
        for _y in 0..512 {
            for x in 0..1024 {
                let v = if x < 512 { 0 } else { 255 };
                rgba.extend([v, v, v, 255]);
            }
        }
        let w = Wallpaper::from_rgba8(1024, 512, &rgba, [0.0, 0.0, 1024.0, 512.0])
            .expect("a wallpaper");
        assert_eq!(w.base_size, [WALLPAPER_WIDTH, 256]);
        let sharp_left = w.sample([0.25, 0.5], 0.0);
        let sharp_right = w.sample([0.75, 0.5], 0.0);
        assert!(sharp_left[0] < 0.01 && sharp_right[0] > 0.99);
        // The smallest mip is the average of everything: mid grey.
        let blurred = w.sample([0.25, 0.5], 20.0);
        assert!((blurred[0] - 0.5).abs() < 0.02, "{blurred:?}");
    }

    #[test]
    fn the_nearest_screen_s_wallpaper_is_reflected() {
        let one = |x: f32, v: u8| {
            Arc::new(
                Wallpaper::from_rgba8(1, 1, &[v, v, v, 255], [x, 0.0, 100.0, 100.0])
                    .expect("a wallpaper"),
            )
        };
        let env = Environment::wallpapers(vec![one(0.0, 0), one(100.0, 255)]);
        assert_eq!(env.wallpaper_at([150.0, 50.0]).map(|w| w.screen[0]), Some(100.0));
        assert_eq!(env.wallpaper_at([-20.0, 50.0]).map(|w| w.screen[0]), Some(0.0));
        assert!(Environment::default().wallpaper_at([0.0, 0.0]).is_none());
    }
}
