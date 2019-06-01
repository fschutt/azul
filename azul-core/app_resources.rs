use std::{fmt, path::PathBuf};
use azul_css::{LayoutPoint, LayoutSize};
use {
    FastHashMap, FastHashSet,
    ui_solver::{ResolvedTextLayoutOptions},
    display_list::GlyphInstance
};

pub type CssImageId = String;
pub type CssFontId = String;

// since it's repr(C), can be casted directly from a `hb_glyph_info_t`
#[repr(C)]
#[derive(Copy, Clone)]
pub struct GlyphInfo {
    pub codepoint: u32,
    pub mask: u32,
    pub cluster: u32,
    pub var1: HbVarIntT,
    pub var2: HbVarIntT,
}

impl fmt::Debug for GlyphInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "GlyphInfo {{ codepoint: {}, mask: {}, cluster: {} }}", self.codepoint, self.mask, self.cluster)
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct GlyphPosition {
    pub x_advance: i32,
    pub y_advance: i32,
    pub x_offset: i32,
    pub y_offset: i32,
    pub var: HbVarIntT,
}

impl fmt::Debug for GlyphPosition {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
            "GlyphPosition {{ x_advance: {}, y_advance: {}, x_offset: {}, y_offset: {},  }}",
            self.x_advance, self.y_advance, self.x_offset, self.y_offset
        )
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union HbVarIntT {
    pub u32: u32,
    pub i32: i32,
    pub u16: [u16; 2usize],
    pub i16: [i16; 2usize],
    pub u8: [u8; 4usize],
    pub i8: [i8; 4usize],
    _bindgen_union_align: u32,
}

pub type WordIndex = usize;
pub type GlyphIndex = usize;
pub type LineLength = f32;
pub type IndexOfLineBreak = usize;
pub type RemainingSpaceToRight = f32;
pub type LineBreaks = Vec<(GlyphIndex, RemainingSpaceToRight)>;

/// Metadata (but not storage) describing an image In WebRender.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageDescriptor {
    /// Format of the image data.
    pub format: RawImageFormat,
    /// Width and height of the image data, in pixels.
    pub dimensions: (usize, usize),
    /// The number of bytes from the start of one row to the next. If non-None,
    /// `compute_stride` will return this value, otherwise it returns
    /// `width * bpp`. Different source of images have different alignment
    /// constraints for rows, so the stride isn't always equal to width * bpp.
    pub stride: Option<i32>,
    /// Offset in bytes of the first pixel of this image in its backing buffer.
    /// This is used for tiling, wherein WebRender extracts chunks of input images
    /// in order to cache, manipulate, and render them individually. This offset
    /// tells the texture upload machinery where to find the bytes to upload for
    /// this tile. Non-tiled images generally set this to zero.
    pub offset: i32,
    /// Whether this image is opaque, or has an alpha channel. Avoiding blending
    /// for opaque surfaces is an important optimization.
    pub is_opaque: bool,
    /// Whether to allow the driver to automatically generate mipmaps. If images
    /// are already downscaled appropriately, mipmap generation can be wasted
    /// work, and cause performance problems on some cards/drivers.
    ///
    /// See https://github.com/servo/webrender/pull/2555/
    pub allow_mipmaps: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdNamespace(pub u32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RawImageFormat {
    R8,
    R16,
    BGRA8,
    RGBAF32,
    RG8,
    RGBAI32,
    RGBA8,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Au(pub i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageKey {
    pub namespace: IdNamespace,
    pub key: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontInstanceKey {
    pub namespace: IdNamespace,
    pub key: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontKey {
    pub namespace: IdNamespace,
    pub key: u32,
}

/// Stores the resources for the application, souch as fonts, images and cached
/// texts, also clipboard strings
///
/// Images and fonts can be references across window contexts (not yet tested,
/// but should work).
#[derive(Default)]
pub struct AppResources {
    /// The CssImageId is the string used in the CSS, i.e. "my_image" -> ImageId(4)
    pub css_ids_to_image_ids: FastHashMap<CssImageId, ImageId>,
    /// Same as CssImageId -> ImageId, but for fonts, i.e. "Roboto" -> FontId(9)
    pub css_ids_to_font_ids: FastHashMap<CssFontId, FontId>,
    /// Stores where the images were loaded from
    pub image_sources: FastHashMap<ImageId, ImageSource>,
    /// Stores where the fonts were loaded from
    pub font_sources: FastHashMap<FontId, FontSource>,
    /// All image keys currently active in the RenderApi
    pub currently_registered_images: FastHashMap<ImageId, ImageInfo>,
    /// All font keys currently active in the RenderApi
    pub currently_registered_fonts: FastHashMap<ImmediateFontId, LoadedFont>,
    /// If an image isn't displayed, it is deleted from memory, only
    /// the `ImageSource` (i.e. the path / source where the image was loaded from) remains.
    ///
    /// This way the image can be re-loaded if necessary but doesn't have to reside in memory at all times.
    pub last_frame_image_keys: FastHashSet<ImageId>,
    /// If a font does not get used for one frame, the corresponding instance key gets
    /// deleted. If a FontId has no FontInstanceKeys anymore, the font key gets deleted.
    ///
    /// The only thing remaining in memory permanently is the FontSource (which is only
    /// the string of the file path where the font was loaded from, so no huge memory pressure).
    /// The reason for this agressive strategy is that the
    pub last_frame_font_keys: FastHashMap<ImmediateFontId, FastHashSet<Au>>,
    /// Stores long texts across frames
    pub text_cache: TextCache,
}

macro_rules! unique_id {($struct_name:ident, $counter_name:ident) => {

    static $counter_name: ::std::sync::atomic::AtomicUsize = ::std::sync::atomic::AtomicUsize::new(0);

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
    pub struct $struct_name {
        id: usize,
    }

    impl $struct_name {

        pub fn new() -> Self {
            Self { id: $counter_name.fetch_add(1, ::std::sync::atomic::Ordering::SeqCst) }
        }
    }
}}

unique_id!(TextId, TEXT_ID_COUNTER);
unique_id!(ImageId, IMAGE_ID_COUNTER);
unique_id!(FontId, FONT_ID_COUNTER);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageSource {
    /// The image is embedded inside the binary file
    Embedded(&'static [u8]),
    /// The image is already decoded and loaded from a set of bytes
    Raw(RawImage),
    /// The image is loaded from a file
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FontSource {
    /// The font is embedded inside the binary file
    Embedded(&'static [u8]),
    /// The font is loaded from a file
    File(PathBuf),
    /// The font is a system built-in font
    System(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImmediateFontId {
    Resolved(FontId),
    Unresolved(CssFontId),
}

/// Raw image made up of raw pixels (either BRGA8 or A8)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawImage {
    pub pixels: Vec<u8>,
    pub image_dimensions: (u32, u32),
    pub data_format: RawImageFormat,
}

#[derive(Debug, Clone)]
pub struct LoadedFont {
    pub font_key: FontKey,
    pub font_bytes: Vec<u8>,
    /// Index of the font in case the bytes indicate a font collection
    pub font_index: i32,
    pub font_instances: FastHashMap<Au, FontInstanceKey>,
}

impl LoadedFont {

    /// Creates a new loaded font with 0 font instances
    pub fn new(font_key: FontKey, font_bytes: Vec<u8>, font_index: i32) -> Self {
        Self {
            font_key,
            font_bytes,
            font_index,
            font_instances: FastHashMap::default(),
        }
    }

    pub fn delete_font_instance(&mut self, size: &Au) {
        self.font_instances.remove(size);
    }
}

/// Cache for accessing large amounts of text
#[derive(Debug, Default, Clone)]
pub struct TextCache {
    /// Mapping from the TextID to the actual, UTF-8 String
    ///
    /// This is stored outside of the actual glyph calculation, because usually you don't
    /// need the string, except for rebuilding a cached string (for example, when the font is changed)
    pub string_cache: FastHashMap<TextId, Words>,

    // -- for now, don't cache ScaledWords, it's too complicated...

    // /// Caches the layout of the strings / words.
    // ///
    // /// TextId -> FontId (to look up by font)
    // /// FontId -> PixelValue (to categorize by size within a font)
    // /// PixelValue -> layouted words (to cache the glyph widths on a per-font-size basis)
    // pub(crate) layouted_strings_cache: FastHashMap<TextId, FastHashMap<FontInstanceKey, ScaledWords>>,
}

impl TextCache {

    /// Add a new, large text to the resources
    pub fn add_text(&mut self, words: Words) -> TextId {
        let id = TextId::new();
        self.string_cache.insert(id, words);
        id
    }

    pub fn get_text(&self, text_id: &TextId) -> Option<&Words> {
        self.string_cache.get(text_id)
    }

    /// Removes a string from the string cache, but not the layouted text cache
    pub fn delete_text(&mut self, id: TextId) {
        self.string_cache.remove(&id);
    }

    pub fn clear_all_texts(&mut self) {
        self.string_cache.clear();
    }
}

/// Text broken up into `Tab`, `Word()`, `Return` characters
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Words {
    /// Words (and spaces), broken up into semantic items
    pub items: Vec<Word>,
    /// String that makes up this paragraph of words
    pub internal_str: String,
    /// `internal_chars` is used in order to enable copy-paste (since taking a sub-string isn't possible using UTF-8)
    pub internal_chars: Vec<char>,
}

impl Words {

    pub fn get_substr(&self, word: &Word) -> String {
        self.internal_chars[word.start..word.end].iter().collect()
    }

    pub fn get_str(&self) -> &str {
        &self.internal_str
    }

    pub fn get_char(&self, idx: usize) -> Option<char> {
        self.internal_chars.get(idx).cloned()
    }
}

/// Section of a certain type
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Word {
    pub start: usize,
    pub end: usize,
    pub word_type: WordType,
}

/// Either a white-space delimited word, tab or return character
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WordType {
    /// Encountered a word (delimited by spaces)
    Word,
    // `\t` or `x09`
    Tab,
    /// `\r`, `\n` or `\r\n`, escaped: `\x0D`, `\x0A` or `\x0D\x0A`
    Return,
    /// Space character
    Space,
}

/// A paragraph of words that are shaped and scaled (* but not yet layouted / positioned*!)
/// according to their final size in pixels.
#[derive(Debug, Clone)]
pub struct ScaledWords {
    /// Font size (in pixels) that was used to scale these words
    pub font_size_px: f32,
    /// Words scaled to their appropriate font size, but not yet positioned on the screen
    pub items: Vec<ScaledWord>,
    /// Longest word in the `self.scaled_words`, necessary for
    /// calculating overflow rectangles.
    pub longest_word_width: f32,
    /// Horizontal advance of the space glyph
    pub space_advance_px: f32,
    /// Glyph index of the space character
    pub space_codepoint: u32,
}

/// Word that is scaled (to a font / font instance), but not yet positioned
#[derive(Debug, Clone)]
pub struct ScaledWord {
    /// Glyphs, positions are relative to the first character of the word
    pub glyph_infos: Vec<GlyphInfo>,
    /// Horizontal advances of each glyph, necessary for
    /// hit-testing characters later on (for text selection).
    pub glyph_positions: Vec<GlyphPosition>,
    /// The sum of the width of all the characters in this word
    pub word_width: f32,
    /// Height of the largest character in this word
    pub word_height: f32,
    ///
    pub word_descent: f32,
}

/// Stores the positions of the vertically laid out texts
#[derive(Debug, Clone, PartialEq)]
pub struct WordPositions {
    /// Options like word spacing, character spacing, etc. that were
    /// used to layout these glyphs
    pub text_layout_options: ResolvedTextLayoutOptions,
    /// Stores the positions of words.
    pub word_positions: Vec<LayoutPoint>,
    /// Index of the word at which the line breaks + length of line
    /// (useful for text selection + horizontal centering)
    pub line_breaks: Vec<(WordIndex, LineLength)>,
    /// Horizontal width of the last line (in pixels), necessary for inline layout later on,
    /// so that the next text run can contine where the last text run left off.
    ///
    /// Usually, the "trailing" of the current text block is the "leading" of the
    /// next text block, to make it seem like two text runs push into each other.
    pub trailing: f32,
    /// How many words are in the text?
    pub number_of_words: usize,
    /// How many lines (NOTE: virtual lines, meaning line breaks in the layouted text) are there?
    pub number_of_lines: usize,
    /// Horizontal and vertical boundaries of the layouted words.
    ///
    /// Note that the vertical extent can be larger than the last words' position,
    /// because of trailing negative glyph advances.
    pub content_size: LayoutSize,
}

/// Returns the layouted glyph instances
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutedGlyphs {
    pub glyphs: Vec<GlyphInstance>,
}

/// Iterator over glyphs that returns information about the cluster that this glyph belongs to.
/// Returned by the `ScaledWord::cluster_iter()` function.
///
/// For each glyph, returns information about what cluster this glyph belongs to. Useful for
/// doing operations per-cluster instead of per-glyph.
/// *Note*: The iterator returns once-per-glyph, not once-per-cluster, however
/// you can merge the clusters into groups by using the `ClusterInfo.cluster_idx`.
#[derive(Debug, Clone)]
pub struct ClusterIterator<'a> {
    /// What codepoint does the current glyph have - set to `None` if the first character isn't yet processed.
    cur_codepoint: Option<u32>,
    /// What cluster *index* are we currently at - default: 0
    cluster_count: usize,
    word: &'a ScaledWord,
    /// Store what glyph we are currently processing in this word
    cur_glyph_idx: usize,
}

/// Info about what cluster a certain glyph belongs to.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClusterInfo {
    /// Cluster index in this word
    pub cluster_idx: usize,
    /// Codepoint of this cluster
    pub codepoint: u32,
    /// What the glyph index of this cluster is
    pub glyph_idx: usize,
}

impl<'a> Iterator for ClusterIterator<'a> {

    type Item = ClusterInfo;

    /// Returns an iterator over the clusters in this word.
    ///
    /// Note: This will return one `ClusterInfo` per glyph, so you can't just
    /// use `.cluster_iter().count()` to count the glyphs: Instead, use `.cluster_iter().last().cluster_idx`.
    fn next(&mut self) -> Option<ClusterInfo> {

        let next_glyph = self.word.glyph_infos.get(self.cur_glyph_idx)?;

        let glyph_idx = self.cur_glyph_idx;

        if self.cur_codepoint != Some(next_glyph.cluster) {
            self.cur_codepoint = Some(next_glyph.cluster);
            self.cluster_count += 1;
        }

        self.cur_glyph_idx += 1;

        Some(ClusterInfo {
            cluster_idx: self.cluster_count,
            codepoint: self.cur_codepoint.unwrap_or(0),
            glyph_idx,
        })
    }
}

impl ScaledWord {

    /// Creates an iterator over clusters instead of glyphs
    pub fn cluster_iter<'a>(&'a self) -> ClusterIterator<'a> {
        ClusterIterator {
            cur_codepoint: None,
            cluster_count: 0,
            word: &self,
            cur_glyph_idx: 0,
        }
    }

    pub fn number_of_clusters(&self) -> usize {
        self.cluster_iter().last().map(|l| l.cluster_idx).unwrap_or(0)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageInfo {
    pub key: ImageKey,
    pub descriptor: ImageDescriptor,
}

impl ImageInfo {
    /// Returns the (width, height) of this image.
    pub fn get_dimensions(&self) -> (usize, usize) {
        self.descriptor.dimensions
    }
}

impl AppResources {

    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the IDs of all currently loaded fonts in `self.font_data`
    pub fn get_loaded_font_ids(&self) -> Vec<FontId> {
        self.font_sources.keys().cloned().collect()
    }

    pub fn get_loaded_image_ids(&self) -> Vec<ImageId> {
        self.image_sources.keys().cloned().collect()
    }

    pub fn get_loaded_css_image_ids(&self) -> Vec<CssImageId> {
        self.css_ids_to_image_ids.keys().cloned().collect()
    }

    pub fn get_loaded_css_font_ids(&self) -> Vec<CssFontId> {
        self.css_ids_to_font_ids.keys().cloned().collect()
    }

    pub fn get_loaded_text_ids(&self) -> Vec<TextId> {
        self.text_cache.string_cache.keys().cloned().collect()
    }

    // -- ImageId cache

    /// Add an image from a PNG, JPEG or other source.
    ///
    /// Note: For specialized image formats, you'll have to enable them as
    /// features in the Cargo.toml file.
    pub fn add_image_source(&mut self, image_id: ImageId, image_source: ImageSource) {
        self.image_sources.insert(image_id, image_source);
    }

    /// Returns whether the AppResources has currently a certain image ID registered
    pub fn has_image_source(&self, image_id: &ImageId) -> bool {
        self.image_sources.get(image_id).is_some()
    }

    /// Given an `ImageId`, returns the decoded bytes of that image or `None`, if the `ImageId` is invalid.
    /// Returns an error on IO failure / image decoding failure or image
    pub fn get_image_source(&self, image_id: &ImageId) -> Option<&ImageSource> {
        self.image_sources.get(image_id)
    }

    pub fn delete_image(&mut self, image_id: &ImageId) {
        self.image_sources.remove(image_id);
    }

    pub fn add_css_image_id<S: Into<String>>(&mut self, css_id: S) -> ImageId {
        *self.css_ids_to_image_ids.entry(css_id.into()).or_insert_with(|| ImageId::new())
    }

    pub fn has_css_image_id(&self, css_id: &str) -> bool {
        self.get_css_image_id(css_id).is_some()
    }

    pub fn get_css_image_id(&self, css_id: &str) -> Option<&ImageId> {
        self.css_ids_to_image_ids.get(css_id)
    }

    pub fn delete_css_image_id(&mut self, css_id: &str) -> Option<ImageId> {
        self.css_ids_to_image_ids.remove(css_id)
    }

    pub fn get_image_info(&self, key: &ImageId) -> Option<&ImageInfo> {
        self.currently_registered_images.get(key)
    }

    // -- FontId cache

    pub fn add_css_font_id<S: Into<String>>(&mut self, css_id: S) -> FontId {
        *self.css_ids_to_font_ids.entry(css_id.into()).or_insert_with(|| FontId::new())
    }

    pub fn has_css_font_id(&self, css_id: &str) -> bool {
        self.get_css_font_id(css_id).is_some()
    }

    pub fn get_css_font_id(&self, css_id: &str) -> Option<&FontId> {
        self.css_ids_to_font_ids.get(css_id)
    }

    pub fn delete_css_font_id(&mut self, css_id: &str) -> Option<FontId> {
        self.css_ids_to_font_ids.remove(css_id)
    }

    pub fn add_font(&mut self, font_id: FontId, font_source: FontSource) {
        self.font_sources.insert(font_id, font_source);
    }

    /// Given a `FontId`, returns the bytes for that font or `None`, if the `FontId` is invalid.
    pub fn get_font_source(&self, font_id: &FontId) -> Option<&FontSource> {
        self.font_sources.get(font_id)
    }

    /// Checks if a `FontId` is valid, i.e. if a font is currently ready-to-use
    pub fn has_font(&self, id: &FontId) -> bool {
        self.font_sources.get(id).is_some()
    }

    pub fn delete_font(&mut self, id: &FontId) {
        self.font_sources.remove(id);
    }

    // -- TextId cache

    /// Adds a string to the internal text cache, but only store it as a string,
    /// without caching the layout of the string.
    pub fn add_text(&mut self, words: Words) -> TextId {
        self.text_cache.add_text(words)
    }

    pub fn get_text(&self, id: &TextId) -> Option<&Words> {
        self.text_cache.get_text(id)
    }

    /// Removes a string from both the string cache and the layouted text cache
    pub fn delete_text(&mut self, id: TextId) {
        self.text_cache.delete_text(id);
    }

    /// Empties the entire internal text cache, invalidating all `TextId`s. Use with care.
    pub fn clear_all_texts(&mut self) {
        self.text_cache.clear_all_texts();
    }

    pub fn get_loaded_font(&self, font_id: &ImmediateFontId) -> Option<&LoadedFont> {
        self.currently_registered_fonts.get(font_id)
    }
}