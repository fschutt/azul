/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, you can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{
    cmp,
    hash::{Hash, Hasher},
    mem,
    ops::Deref,
    sync::{Arc, Mutex, MutexGuard},
};

use api::{
    channel::crossbeam::{unbounded, Receiver, Sender},
    units::*,
    ColorU, FontInstanceData, FontInstanceFlags, FontInstanceKey, FontInstanceOptions,
    FontInstancePlatformOptions, FontKey, FontRenderMode, FontSize, FontTemplate, FontVariation,
    GlyphDimensions, GlyphIndex, ImageFormat, SyntheticItalics,
};
use azul_css::props::basic::font::FontRef;
use azul_layout::font::parsed::ParsedFont;
use rayon::{prelude::*, ThreadPool};
use smallvec::{smallvec, SmallVec};

use crate::{
    font::FontContext,
    types::{FastHashMap, FastHashSet},
};

const GLYPH_BATCH_SIZE: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
#[cfg_attr(feature = "capture", derive(Serialize))]
#[cfg_attr(feature = "replay", derive(Deserialize))]
pub struct FontTransform {
    pub scale_x: f32,
    pub skew_x: f32,
    pub skew_y: f32,
    pub scale_y: f32,
}

impl Eq for FontTransform {}
impl Ord for FontTransform {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.partial_cmp(other).unwrap_or(cmp::Ordering::Equal)
    }
}
impl Hash for FontTransform {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.scale_x.to_bits().hash(state);
        self.skew_x.to_bits().hash(state);
        self.skew_y.to_bits().hash(state);
        self.scale_y.to_bits().hash(state);
    }
}

impl FontTransform {
    const QUANTIZE_SCALE: f32 = 1024.0;

    pub fn new(scale_x: f32, skew_x: f32, skew_y: f32, scale_y: f32) -> Self {
        FontTransform {
            scale_x,
            skew_x,
            skew_y,
            scale_y,
        }
    }

    pub fn identity() -> Self {
        FontTransform::new(1.0, 0.0, 0.0, 1.0)
    }

    pub fn is_identity(&self) -> bool {
        *self == FontTransform::identity()
    }

    pub fn quantize(&self) -> Self {
        FontTransform::new(
            (self.scale_x * Self::QUANTIZE_SCALE).round() / Self::QUANTIZE_SCALE,
            (self.skew_x * Self::QUANTIZE_SCALE).round() / Self::QUANTIZE_SCALE,
            (self.skew_y * Self::QUANTIZE_SCALE).round() / Self::QUANTIZE_SCALE,
            (self.scale_y * Self::QUANTIZE_SCALE).round() / Self::QUANTIZE_SCALE,
        )
    }

    pub fn get_subpx_dir(&self) -> SubpixelDirection {
        const EPSILON: f32 = 0.001;
        if self.skew_y.abs() < EPSILON {
            SubpixelDirection::Horizontal
        } else if self.scale_x.abs() < EPSILON {
            SubpixelDirection::Vertical
        } else {
            SubpixelDirection::Mixed
        }
    }

    /// Scale the transform by a factor
    pub fn scale(&self, scale: f32) -> Self {
        FontTransform::new(
            self.scale_x * scale,
            self.skew_x * scale,
            self.skew_y * scale,
            self.scale_y * scale,
        )
    }

    /// Transform a point using this font transform
    pub fn transform(
        &self,
        point: &euclid::Point2D<f32, api::units::LayoutPixel>,
    ) -> euclid::Point2D<f32, api::units::DevicePixel> {
        euclid::Point2D::new(
            self.scale_x * point.x + self.skew_x * point.y,
            self.skew_y * point.x + self.scale_y * point.y,
        )
    }
}

// Stub: Accept any transform-like type and convert to FontTransform
impl<Src, Dst> From<euclid::Transform3D<f32, Src, Dst>> for FontTransform {
    fn from(transform: euclid::Transform3D<f32, Src, Dst>) -> Self {
        // Extract 2D affine components from 3D transform
        FontTransform::new(transform.m11, transform.m21, transform.m12, transform.m22)
    }
}

// Also support borrowed transforms
impl<Src, Dst> From<&euclid::Transform3D<f32, Src, Dst>> for FontTransform {
    fn from(transform: &euclid::Transform3D<f32, Src, Dst>) -> Self {
        // Extract 2D affine components from 3D transform
        FontTransform::new(transform.m11, transform.m21, transform.m12, transform.m22)
    }
}

#[derive(Clone, Debug, Ord, PartialOrd)]
#[cfg_attr(feature = "capture", derive(Serialize))]
#[cfg_attr(feature = "replay", derive(Deserialize))]
pub struct BaseFontInstance {
    pub instance_key: FontInstanceKey,
    pub font_key: FontKey,
    pub size: FontSize,
    pub options: FontInstanceOptions,
    #[cfg_attr(any(feature = "capture", feature = "replay"), serde(skip))]
    pub platform_options: Option<FontInstancePlatformOptions>,
    pub variations: Vec<FontVariation>,
}

impl BaseFontInstance {
    pub fn new(
        instance_key: FontInstanceKey,
        font_key: FontKey,
        size: f32,
        options: Option<FontInstanceOptions>,
        platform_options: Option<FontInstancePlatformOptions>,
        variations: Vec<FontVariation>,
    ) -> Self {
        BaseFontInstance {
            instance_key,
            font_key,
            size: size.into(),
            options: options.unwrap_or_default(),
            platform_options,
            variations,
        }
    }
}

impl Deref for BaseFontInstance {
    type Target = FontInstanceOptions;
    fn deref(&self) -> &FontInstanceOptions {
        &self.options
    }
}

impl Hash for BaseFontInstance {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font_key.hash(state);
        self.size.hash(state);
        self.options.hash(state);
        self.platform_options.hash(state);
        self.variations.hash(state);
    }
}

impl PartialEq for BaseFontInstance {
    fn eq(&self, other: &BaseFontInstance) -> bool {
        self.font_key == other.font_key
            && self.size == other.size
            && self.options == other.options
            && self.platform_options == other.platform_options
            && self.variations == other.variations
    }
}
impl Eq for BaseFontInstance {}

#[derive(Clone, Debug, Ord, PartialOrd)]
#[cfg_attr(feature = "capture", derive(Serialize))]
#[cfg_attr(feature = "replay", derive(Deserialize))]
pub struct FontInstance {
    pub base: Arc<BaseFontInstance>,
    pub transform: FontTransform,
    pub render_mode: FontRenderMode,
    pub flags: FontInstanceFlags,
    pub color: ColorU,
    pub size: FontSize,
}

impl Hash for FontInstance {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.base.instance_key.hash(state);
        self.transform.hash(state);
        self.render_mode.hash(state);
        self.flags.hash(state);
        self.color.hash(state);
        self.size.hash(state);
    }
}

impl PartialEq for FontInstance {
    fn eq(&self, other: &FontInstance) -> bool {
        self.base.instance_key == other.base.instance_key
            && self.transform == other.transform
            && self.render_mode == other.render_mode
            && self.flags == other.flags
            && self.color == other.color
            && self.size == other.size
    }
}
impl Eq for FontInstance {}

impl Deref for FontInstance {
    type Target = BaseFontInstance;
    fn deref(&self) -> &BaseFontInstance {
        self.base.as_ref()
    }
}

impl FontInstance {
    pub fn new(
        base: Arc<BaseFontInstance>,
        color: ColorU,
        render_mode: FontRenderMode,
        flags: FontInstanceFlags,
    ) -> Self {
        FontInstance {
            transform: FontTransform::identity(),
            color,
            size: base.size,
            base,
            render_mode,
            flags,
        }
    }

    pub fn from_base(base: Arc<BaseFontInstance>) -> Self {
        let color = ColorU::new(0, 0, 0, 255);
        let render_mode = base.render_mode;
        let flags = base.flags;
        Self::new(base, color, render_mode, flags)
    }

    pub fn use_subpixel_position(&self) -> bool {
        self.flags.contains(FontInstanceFlags::SUBPIXEL_POSITION)
            && self.render_mode != FontRenderMode::Mono
    }

    pub fn get_subpx_offset(&self, glyph: &GlyphKey) -> (f64, f64) {
        if self.use_subpixel_position() {
            let (dx, dy) = glyph.subpixel_offset();
            (dx.into(), dy.into())
        } else {
            (0.0, 0.0)
        }
    }

    /// Get the subpixel direction (stub for anti-aliasing)
    pub fn get_subpx_dir(&self) -> SubpixelDirection {
        if !self.use_subpixel_position() {
            SubpixelDirection::None
        } else {
            // Default to horizontal subpixel rendering
            SubpixelDirection::Horizontal
        }
    }

    /// Disable subpixel AA (stub - modifies flags)
    pub fn disable_subpixel_aa(&mut self) {
        self.flags.remove(FontInstanceFlags::SUBPIXEL_POSITION);
        self.flags.remove(FontInstanceFlags::LCD_VERTICAL);
    }

    /// Disable subpixel positioning (stub - modifies flags)
    pub fn disable_subpixel_position(&mut self) {
        self.flags.remove(FontInstanceFlags::SUBPIXEL_POSITION);
    }
}

#[repr(u32)]
#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug, Ord, PartialOrd)]
pub enum SubpixelDirection {
    None = 0,
    Horizontal,
    Vertical,
    Mixed,
}

impl SubpixelDirection {
    /// Limit subpixel direction based on glyph format
    pub fn limit_by(self, glyph_format: GlyphFormat) -> Self {
        match glyph_format {
            GlyphFormat::TransformedAlpha | GlyphFormat::TransformedSubpixel => {
                SubpixelDirection::None
            }
            _ => self,
        }
    }
}

#[repr(u8)]
#[derive(Hash, Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "capture", derive(Serialize))]
#[cfg_attr(feature = "replay", derive(Deserialize))]
pub enum SubpixelOffset {
    Zero = 0,
    Quarter = 1,
    Half = 2,
    ThreeQuarters = 3,
}

impl SubpixelOffset {
    fn quantize(pos: f32) -> Self {
        let apos = ((pos - pos.floor()) * 8.0) as i32;
        match apos {
            1..=2 => SubpixelOffset::Quarter,
            3..=4 => SubpixelOffset::Half,
            5..=6 => SubpixelOffset::ThreeQuarters,
            _ => SubpixelOffset::Zero,
        }
    }
}

impl Into<f64> for SubpixelOffset {
    fn into(self) -> f64 {
        match self {
            SubpixelOffset::Zero => 0.0,
            SubpixelOffset::Quarter => 0.25,
            SubpixelOffset::Half => 0.5,
            SubpixelOffset::ThreeQuarters => 0.75,
        }
    }
}

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug, Ord, PartialOrd)]
#[cfg_attr(feature = "capture", derive(Serialize))]
#[cfg_attr(feature = "replay", derive(Deserialize))]
pub struct GlyphKey(u32);

impl GlyphKey {
    pub fn new(index: u32, point: DevicePoint, subpx_dir: SubpixelDirection) -> Self {
        let (dx, dy) = match subpx_dir {
            SubpixelDirection::None => (0.0, 0.0),
            SubpixelDirection::Horizontal => (point.x, 0.0),
            SubpixelDirection::Vertical => (0.0, point.y),
            SubpixelDirection::Mixed => (point.x, point.y),
        };
        let sox = SubpixelOffset::quantize(dx);
        let soy = SubpixelOffset::quantize(dy);
        assert_eq!(0, index & 0xF0000000);
        GlyphKey(index | (sox as u32) << 28 | (soy as u32) << 30)
    }

    pub fn index(&self) -> GlyphIndex {
        self.0 & 0x0FFFFFFF
    }

    fn subpixel_offset(&self) -> (SubpixelOffset, SubpixelOffset) {
        let x = (self.0 >> 28) as u8 & 3;
        let y = (self.0 >> 30) as u8 & 3;
        unsafe { (mem::transmute(x), mem::transmute(y)) }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "capture", derive(Serialize))]
#[cfg_attr(feature = "replay", derive(Deserialize))]
pub enum GlyphFormat {
    Alpha,
    Subpixel,            // Note: Not implemented by azul backend, will fall back to Alpha
    Bitmap,              // Note: Not implemented by azul backend, will fall back to Alpha
    ColorBitmap,         // Note: Not implemented by azul backend, will fall back to Alpha
    TransformedAlpha,    // Transformed/rotated glyphs with alpha
    TransformedSubpixel, // Transformed/rotated glyphs with subpixel AA
}

impl GlyphFormat {
    pub fn image_format(&self, can_use_r8_format: bool) -> ImageFormat {
        match *self {
            GlyphFormat::Alpha | GlyphFormat::Bitmap | GlyphFormat::TransformedAlpha => {
                if can_use_r8_format {
                    ImageFormat::R8
                } else {
                    ImageFormat::BGRA8
                }
            }
            GlyphFormat::Subpixel | GlyphFormat::ColorBitmap | GlyphFormat::TransformedSubpixel => {
                ImageFormat::BGRA8
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct RasterizedGlyph {
    pub top: f32,
    pub left: f32,
    pub width: i32,
    pub height: i32,
    pub scale: f32,
    pub format: GlyphFormat,
    pub bytes: Vec<u8>,
}

pub struct GlyphRasterJob {
    pub font: Arc<FontInstance>,
    pub key: GlyphKey,
    pub result: GlyphRasterResult,
}

#[derive(Debug)]
pub enum GlyphRasterError {
    LoadFailed,
}

pub type GlyphRasterResult = Result<RasterizedGlyph, GlyphRasterError>;

pub struct GlyphRasterizer {
    workers: Arc<ThreadPool>,
    font_contexts: Arc<Vec<Mutex<FontContext>>>,
    fonts: FastHashSet<FontKey>,
    pending_glyph_count: usize,
    pending_glyph_jobs: usize,
    pending_glyph_requests: FastHashMap<FontInstance, SmallVec<[GlyphKey; 16]>>,
    glyph_rx: Receiver<GlyphRasterJob>,
    glyph_tx: Sender<GlyphRasterJob>,
    fonts_to_remove: Vec<FontKey>,
    font_instances_to_remove: Vec<FontInstance>,
    can_use_r8_format: bool,
}

impl GlyphRasterizer {
    pub fn new(workers: Arc<ThreadPool>, can_use_r8_format: bool) -> Self {
        let (glyph_tx, glyph_rx) = unbounded();
        let num_workers = workers.current_num_threads();
        let mut contexts = Vec::with_capacity(num_workers);
        for _ in 0..num_workers {
            contexts.push(Mutex::new(FontContext::new()));
        }

        GlyphRasterizer {
            font_contexts: Arc::new(contexts),
            workers,
            fonts: FastHashSet::default(),
            pending_glyph_count: 0,
            pending_glyph_jobs: 0,
            pending_glyph_requests: FastHashMap::default(),
            glyph_rx,
            glyph_tx,
            fonts_to_remove: Vec::new(),
            font_instances_to_remove: Vec::new(),
            can_use_r8_format,
        }
    }

    /// Adds a pre-parsed font directly (for efficiency when font is already parsed).
    pub fn add_parsed_font(&mut self, font_key: FontKey, parsed_font: FontRef) {
        if self.fonts.insert(font_key) {
            for context_mutex in self.font_contexts.iter() {
                let mut context: MutexGuard<FontContext> = context_mutex.lock().unwrap();
                context.add_font(font_key, parsed_font.clone());
            }
        }
    }

    pub fn delete_font(&mut self, font_key: FontKey) {
        self.fonts_to_remove.push(font_key);
    }

    pub fn delete_font_instance(&mut self, instance: &FontInstance) {
        self.font_instances_to_remove.push(instance.clone());
    }

    pub fn prepare_font(&self, font: &mut FontInstance) {
        FontContext::prepare_font(font);
        font.transform = font.transform.quantize();
    }

    pub fn has_font(&self, font_key: FontKey) -> bool {
        self.fonts.contains(&font_key)
    }

    pub fn get_glyph_dimensions(
        &self,
        font: &FontInstance,
        glyph_index: GlyphIndex,
    ) -> Option<GlyphDimensions> {
        let glyph_key = GlyphKey::new(glyph_index, DevicePoint::zero(), SubpixelDirection::None);
        self.font_contexts[0]
            .lock()
            .unwrap()
            .get_glyph_dimensions(font, &glyph_key)
    }

    pub fn get_glyph_index(&self, font_key: FontKey, ch: char) -> Option<u32> {
        self.font_contexts[0]
            .lock()
            .unwrap()
            .get_glyph_index(font_key, ch)
    }

    fn flush_glyph_requests(
        &mut self,
        font: FontInstance,
        glyphs: SmallVec<[GlyphKey; 16]>,
        use_workers: bool,
    ) {
        let font = Arc::new(font);
        let font_contexts = Arc::clone(&self.font_contexts);
        self.pending_glyph_jobs += glyphs.len();
        self.pending_glyph_count -= glyphs.len();
        let can_use_r8_format = self.can_use_r8_format;

        if use_workers {
            let tx = self.glyph_tx.clone();
            self.workers.spawn(move || {
                glyphs.par_iter().for_each(|key| {
                    let worker_id = rayon::current_thread_index().unwrap_or(0);
                    let mut context = font_contexts[worker_id].lock().unwrap();
                    let job = process_glyph(&mut context, can_use_r8_format, font.clone(), *key);
                    let _ = tx.send(job);
                });
            });
        } else {
            let mut context = font_contexts[0].lock().unwrap();
            for key in glyphs {
                let job = process_glyph(&mut context, can_use_r8_format, font.clone(), key);
                let _ = self.glyph_tx.send(job);
            }
        }
    }

    pub fn request_glyphs<F>(&mut self, font: FontInstance, glyph_keys: &[GlyphKey], mut handle: F)
    where
        F: FnMut(&GlyphKey) -> bool,
    {
        println!("wr_api: requesting {} glyphs", glyph_keys.len());
        assert!(self.has_font(font.font_key));
        println!("font found!");
        let mut batch_size = 0;
        for key in glyph_keys {
            if !handle(key) {
                continue;
            }
            self.pending_glyph_count += 1;
            match self.pending_glyph_requests.get_mut(&font) {
                Some(container) => {
                    container.push(*key);
                    batch_size = container.len();
                }
                None => {
                    self.pending_glyph_requests
                        .insert(font.clone(), smallvec![*key]);
                }
            }
        }
        if batch_size >= GLYPH_BATCH_SIZE {
            let container = self.pending_glyph_requests.get_mut(&font).unwrap();
            let glyphs = mem::replace(container, SmallVec::new());
            self.flush_glyph_requests(font, glyphs, true);
        }
    }

    pub fn resolve_glyphs<F>(&mut self, mut handle: F)
    where
        F: FnMut(GlyphRasterJob, bool),
    {
        let mut pending_glyph_requests = mem::take(&mut self.pending_glyph_requests);
        let use_workers = self.pending_glyph_count >= 8;
        for (font, pending_glyphs) in pending_glyph_requests.drain() {
            self.flush_glyph_requests(font, pending_glyphs, use_workers);
        }
        self.pending_glyph_requests = pending_glyph_requests;
        debug_assert_eq!(self.pending_glyph_count, 0);

        let mut jobs = self
            .glyph_rx
            .iter()
            .take(self.pending_glyph_jobs)
            .collect::<Vec<_>>();
        assert_eq!(
            jobs.len(),
            self.pending_glyph_jobs,
            "Didn't receive all pending glyphs!"
        );
        self.pending_glyph_jobs = 0;

        jobs.sort_by(|a, b| (*a.font).cmp(&*b.font).then(a.key.cmp(&b.key)));

        for job in jobs {
            handle(job, self.can_use_r8_format);
        }

        self.remove_dead_fonts();
    }

    fn remove_dead_fonts(&mut self) {
        if self.fonts_to_remove.is_empty() && self.font_instances_to_remove.is_empty() {
            return;
        }

        let mut fonts_to_remove = mem::take(&mut self.fonts_to_remove);
        fonts_to_remove.retain(|font| self.fonts.remove(font));

        for context_mutex in self.font_contexts.iter() {
            let mut context: MutexGuard<FontContext> = context_mutex.lock().unwrap();
            for font_key in &fonts_to_remove {
                context.delete_font(font_key);
            }
        }
    }
}

fn process_glyph(
    context: &mut FontContext,
    can_use_r8_format: bool,
    font: Arc<FontInstance>,
    key: GlyphKey,
) -> GlyphRasterJob {
    let result = context.rasterize_glyph(&font, &key);
    let mut job = GlyphRasterJob { font, key, result };

    if let Ok(ref mut glyph) = job.result {
        // The azul backend produces an alpha mask (Vec<u8>), so we need to convert it
        // to the format WebRender expects.
        if glyph.format.image_format(can_use_r8_format) == ImageFormat::BGRA8 {
            glyph.bytes = glyph
                .bytes
                .iter()
                .flat_map(|&alpha| [alpha, alpha, alpha, alpha])
                .collect();
        }
        // If the target is R8, the bytes are already in the correct format.
    }

    job
}
