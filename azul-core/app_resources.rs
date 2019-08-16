use std::{fmt, path::PathBuf};
use azul_css::{LayoutPoint, LayoutSize};
use crate::{
    FastHashMap, FastHashSet,
    ui_solver::{ResolvedTextLayoutOptions},
    display_list::{DisplayList, GlyphInstance},
    callbacks::PipelineId,
    id_tree::NodeDataContainer,
    dom::NodeData,
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
    pub currently_registered_images: FastHashMap<PipelineId, FastHashMap<ImageId, ImageInfo>>,
    /// All font keys currently active in the RenderApi
    pub currently_registered_fonts: FastHashMap<PipelineId, FastHashMap<ImmediateFontId, LoadedFont>>,
    /// If an image isn't displayed, it is deleted from memory, only
    /// the `ImageSource` (i.e. the path / source where the image was loaded from) remains.
    ///
    /// This way the image can be re-loaded if necessary but doesn't have to reside in memory at all times.
    pub last_frame_image_keys: FastHashMap<PipelineId, FastHashSet<ImageId>>,
    /// If a font does not get used for one frame, the corresponding instance key gets
    /// deleted. If a FontId has no FontInstanceKeys anymore, the font key gets deleted.
    ///
    /// The only thing remaining in memory permanently is the FontSource (which is only
    /// the string of the file path where the font was loaded from, so no huge memory pressure).
    /// The reason for this agressive strategy is that the
    pub last_frame_font_keys: FastHashMap<PipelineId, FastHashMap<ImmediateFontId, FastHashSet<Au>>>,
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontSource {
    /// The font is embedded inside the binary file
    Embedded(&'static [u8]),
    /// The font is loaded from a file
    File(PathBuf),
    /// The font is a system built-in font
    System(String),
}

impl fmt::Display for FontSource {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::FontSource::*;
        match self {
            Embedded(e) => write!(f, "Embedded(0x{:x})", e as *const _ as usize),
            File(p) => write!(f, "\"{}\"", p.as_path().to_string_lossy()),
            System(id) => write!(f, "\"{}\"", id),
        }
    }
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
    /// Baseline of the font (usually lower than the font size)
    pub baseline_px: f32,
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

    pub fn get_image_info(&self, pipeline_id: &PipelineId, image_key: &ImageId) -> Option<&ImageInfo> {
        self.currently_registered_images.get(pipeline_id).and_then(|map| map.get(image_key))
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

    pub fn add_font_source(&mut self, font_id: FontId, font_source: FontSource) {
        self.font_sources.insert(font_id, font_source);
    }

    /// Given a `FontId`, returns the bytes for that font or `None`, if the `FontId` is invalid.
    pub fn get_font_source(&self, font_id: &FontId) -> Option<&FontSource> {
        self.font_sources.get(font_id)
    }

    /// Checks if a `FontId` is valid, i.e. if a font is currently ready-to-use
    pub fn has_font_source(&self, id: &FontId) -> bool {
        self.font_sources.get(id).is_some()
    }

    pub fn delete_font_source(&mut self, id: &FontId) {
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

    pub fn get_loaded_font(&self, pipeline_id: &PipelineId, font_id: &ImmediateFontId) -> Option<&LoadedFont> {
        self.currently_registered_fonts.get(pipeline_id).and_then(|map| map.get(font_id))
    }
}

pub trait FontImageApi {
    fn new_image_key(&self) -> ImageKey;
    fn new_font_key(&self) -> FontKey;
    fn new_font_instance_key(&self) -> FontInstanceKey;
    fn update_resources(&self, _: Vec<ResourceUpdate>);
    fn flush_scene_builder(&self);
}

/// Used only for debugging, so that the AppResource garbage
/// collection tests can run without a real RenderApi
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct FakeRenderApi { }

impl FakeRenderApi { pub fn new() -> Self { Self { } } }

// Fake RenderApi for unit testing
impl FontImageApi for FakeRenderApi {
    fn new_image_key(&self) -> ImageKey { ImageKey { key: 0, namespace: IdNamespace(0) } }
    fn new_font_key(&self) -> FontKey { FontKey { key: 0, namespace: IdNamespace(0) } }
    fn new_font_instance_key(&self) -> FontInstanceKey { FontInstanceKey { key: 0, namespace: IdNamespace(0) } }
    fn update_resources(&self, _: Vec<ResourceUpdate>) { }
    fn flush_scene_builder(&self) { }
}

/// Scans the DisplayList for new images and fonts. After this call, the RenderApi is
/// guaranteed to know about all FontKeys and FontInstanceKey
pub fn add_fonts_and_images<T, U: FontImageApi>(
    app_resources: &mut AppResources,
    render_api: &mut U,
    pipeline_id: &PipelineId,
    display_list: &DisplayList,
    node_data: &NodeDataContainer<NodeData<T>>,
) {
    let font_keys = scan_ui_description_for_font_keys(&app_resources, display_list, node_data);
    let image_keys = scan_ui_description_for_image_keys(&app_resources, display_list, node_data);

    app_resources.last_frame_font_keys.get_mut(pipeline_id).unwrap().extend(font_keys.clone().into_iter());
    app_resources.last_frame_image_keys.get_mut(pipeline_id).unwrap().extend(image_keys.clone().into_iter());

    let add_font_resource_updates = build_add_font_resource_updates(app_resources, render_api, pipeline_id, &font_keys);
    let add_image_resource_updates = build_add_image_resource_updates(app_resources, render_api, pipeline_id, &image_keys);

    add_resources(app_resources, render_api, pipeline_id, add_font_resource_updates, add_image_resource_updates);
}

/// To be called at the end of a frame (after the UI has rendered):
/// Deletes all FontKeys and FontImageKeys that weren't used in
/// the last frame, to save on memory. If the font needs to be recreated, it
/// needs to be reloaded from the `FontSource`.
pub fn garbage_collect_fonts_and_images<U: FontImageApi>(
    app_resources: &mut AppResources,
    render_api: &mut U,
    pipeline_id: &PipelineId,
) {
    let delete_font_resource_updates = build_delete_font_resource_updates(app_resources, pipeline_id);
    let delete_image_resource_updates = build_delete_image_resource_updates(app_resources, pipeline_id);

    delete_resources(app_resources, render_api, pipeline_id, delete_font_resource_updates, delete_image_resource_updates);

    app_resources.last_frame_font_keys.get_mut(pipeline_id).unwrap().clear();
    app_resources.last_frame_image_keys.get_mut(pipeline_id).unwrap().clear();
}

/// Scans the display list for all font IDs + their font size
pub fn scan_ui_description_for_font_keys<T>(
    app_resources: &AppResources,
    display_list: &DisplayList,
    node_data: &NodeDataContainer<NodeData<T>>,
) -> FastHashMap<ImmediateFontId, FastHashSet<Au>> {

    use crate::dom::NodeType::*;
    use crate::ui_solver;

    let mut font_keys = FastHashMap::default();

    for node_id in display_list.rectangles.linear_iter() {

        let display_rect = &display_list.rectangles[node_id];
        let node_data = &node_data[node_id];

        match node_data.get_node_type() {
            Text(_) | Label(_) => {
                let css_font_id = ui_solver::get_font_id(&display_rect.style);
                let font_id = match app_resources.css_ids_to_font_ids.get(css_font_id) {
                    Some(s) => ImmediateFontId::Resolved(*s),
                    None => ImmediateFontId::Unresolved(css_font_id.to_string()),
                };
                let font_size = ui_solver::get_font_size(&display_rect.style);
                font_keys
                    .entry(font_id)
                    .or_insert_with(|| FastHashSet::default())
                    .insert(ui_solver::font_size_to_au(font_size));
            },
            _ => { }
        }
    }

    font_keys
}

/// Scans the display list for all image keys
pub fn scan_ui_description_for_image_keys<T>(
    app_resources: &AppResources,
    display_list: &DisplayList,
    node_data: &NodeDataContainer<NodeData<T>>,
) -> FastHashSet<ImageId> {

    use crate::dom::NodeType::*;

    display_list.rectangles
    .iter()
    .zip(node_data.iter())
    .filter_map(|(display_rect, node_data)| {
        match node_data.get_node_type() {
            Image(id) => Some(*id),
            _ => {
                let background = display_rect.style.background.as_ref().and_then(|bg| bg.get_property())?;
                let css_image_id = background.get_css_image_id()?;
                let image_id = app_resources.get_css_image_id(&css_image_id.0)?;
                Some(*image_id)
            }
        }
    }).collect()
}

// Debug, PartialEq, Eq, PartialOrd, Ord
#[derive(Debug, Clone)]
pub(crate) enum AddFontMsg {
    Font(LoadedFont),
    Instance(AddFontInstance, Au),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum DeleteFontMsg {
    Font(FontKey),
    Instance(FontInstanceKey, Au),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct AddImageMsg(pub AddImage, pub ImageInfo);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct DeleteImageMsg(ImageKey, ImageInfo);

/// Given the fonts of the current frame, returns `AddFont` and `AddFontInstance`s of
/// which fonts / instances are currently not in the `current_registered_fonts` and
/// need to be added.
///
/// Deleting fonts can only be done after the entire frame has finished drawing,
/// otherwise (if removing fonts would happen after every DOM) we'd constantly
/// add-and-remove fonts after every IFrameCallback, which would cause a lot of
/// I/O waiting.
pub fn build_add_font_resource_updates<T: FontImageApi>(
    app_resources: &AppResources,
    render_api: &mut T,
    pipeline_id: &PipelineId,
    fonts_in_dom: &FastHashMap<ImmediateFontId, FastHashSet<Au>>,
) -> Vec<(ImmediateFontId, AddFontMsg)> {

    use crate::display_list::{FontRenderMode, FontInstanceFlags};

    let mut resource_updates = Vec::new();

    for (im_font_id, font_sizes) in fonts_in_dom {

        macro_rules! insert_font_instances {($font_id:expr, $font_key:expr, $font_index:expr, $font_size:expr) => ({

            let font_instance_key_exists = app_resources.currently_registered_fonts[pipeline_id]
                .get(&$font_id)
                .and_then(|loaded_font| loaded_font.font_instances.get(&$font_size))
                .is_some();

            if !font_instance_key_exists {

                let font_instance_key = render_api.new_font_instance_key();

                use crate::display_list::{
                    FontInstanceOptions, FontRenderMode,
                    FontInstancePlatformOptions, FONT_INSTANCE_FLAG_NO_AUTOHINT,
                };

                // For some reason the gamma is way to low on Windows
                #[cfg(target_os = "windows")]
                let platform_options = FontInstancePlatformOptions {
                    gamma: 300,
                    contrast: 100,
                };

                #[cfg(target_os = "linux")]
                use crate::display_list::{FontLCDFilter, FontHinting};

                #[cfg(target_os = "linux")]
                let platform_options = FontInstancePlatformOptions {
                    lcd_filter: FontLCDFilter::Default,
                    hinting: FontHinting::LCD,
                };

                #[cfg(target_os = "macos")]
                let platform_options = FontInstancePlatformOptions::default();

                let options = FontInstanceOptions {
                    render_mode: FontRenderMode::Subpixel,
                    flags: 0 | FONT_INSTANCE_FLAG_NO_AUTOHINT,
                    .. Default::default()
                };

                resource_updates.push(($font_id, AddFontMsg::Instance(AddFontInstance {
                    key: font_instance_key,
                    font_key: $font_key,
                    glyph_size: $font_size,
                    options: Some(options),
                    platform_options: Some(platform_options),
                    variations: Vec::new(),
                }, $font_size)));
            }
        })}

        match app_resources.currently_registered_fonts[pipeline_id].get(im_font_id) {
            Some(loaded_font) => {
                for font_size in font_sizes.iter() {
                    insert_font_instances!(im_font_id.clone(), loaded_font.font_key, loaded_font.font_index, *font_size);
                }
            },
            None => {
                use self::ImmediateFontId::*;

                // If there is no font key, that means there's also no font instances
                let font_source = match im_font_id {
                    Resolved(font_id) => {
                        match app_resources.font_sources.get(font_id) {
                            Some(s) => s.clone(),
                            None => continue,
                        }
                    },
                    Unresolved(css_font_id) => FontSource::System(css_font_id.clone()),
                };

                let (font_bytes, font_index) = match font_source_get_bytes(&font_source) {
                    Ok(o) => o,
                    Err(e) => {
                        #[cfg(feature = "logging")] {
                            warn!("Could not load font with ID: {:?} - error: {}", im_font_id, e);
                        }
                        continue;
                    }
                };

                if !font_sizes.is_empty() {
                    let font_key = render_api.new_font_key();

                    resource_updates.push((im_font_id.clone(), AddFontMsg::Font(LoadedFont::new(font_key, font_bytes, font_index))));

                    for font_size in font_sizes {
                        insert_font_instances!(im_font_id.clone(), font_key, font_index, *font_size);
                    }
                }
            }
        }
    }

    resource_updates
}

/// Given the images of the current frame, returns `AddImage`s of
/// which image keys are currently not in the `current_registered_fonts` and
/// need to be added. Modifies `last_frame_image_keys` to contain the added image keys
///
/// Deleting images can only be done after the entire frame has finished drawing,
/// otherwise (if removing images would happen after every DOM) we'd constantly
/// add-and-remove images after every IFrameCallback, which would cause a lot of
/// I/O waiting.
#[allow(unused_variables)]
pub fn build_add_image_resource_updates<T: FontImageApi>(
    app_resources: &AppResources,
    render_api: &mut T,
    pipeline_id: &PipelineId,
    images_in_dom: &FastHashSet<ImageId>,
) -> Vec<(ImageId, AddImageMsg)> {

    images_in_dom.iter()
    .filter(|image_id| !app_resources.currently_registered_images[pipeline_id].contains_key(*image_id))
    .filter_map(|image_id| {
        let (data, descriptor) = match image_source_get_bytes(app_resources.image_sources.get(image_id)?) {
            Ok(o) => o,
            Err(e) => {
                #[cfg(feature = "logging")] {
                    warn!("Could not load image with ID: {:?} - error: {}", image_id, e);
                }
                return None;
            }
        };

        let key = render_api.new_image_key();
        let add_image = AddImage { key, data, descriptor, tiling: None };
        Some((*image_id, AddImageMsg(add_image, ImageInfo { key, descriptor })))
    }).collect()
}

/// Submits the `AddFont`, `AddFontInstance` and `AddImage` resources to the RenderApi.
/// Extends `currently_registered_images` and `currently_registered_fonts` by the
/// `last_frame_image_keys` and `last_frame_font_keys`, so that we don't lose track of
/// what font and image keys are currently in the API.
pub fn add_resources<T: FontImageApi>(
    app_resources: &mut AppResources,
    render_api: &mut T,
    pipeline_id: &PipelineId,
    add_font_resources: Vec<(ImmediateFontId, AddFontMsg)>,
    add_image_resources: Vec<(ImageId, AddImageMsg)>,
) {
    let mut merged_resource_updates = Vec::new();

    merged_resource_updates.extend(add_font_resources.iter().map(|(_, f)| f.into_resource_update()));
    merged_resource_updates.extend(add_image_resources.iter().map(|(_, i)| i.into_resource_update()));

    if !merged_resource_updates.is_empty() {
        render_api.update_resources(merged_resource_updates);
        // Assure that the AddFont / AddImage updates get processed immediately
        render_api.flush_scene_builder();
    }

    for (image_id, add_image_msg) in add_image_resources.iter() {
        app_resources.currently_registered_images
        .get_mut(pipeline_id).unwrap()
        .insert(*image_id, add_image_msg.1);
    }

    for (font_id, add_font_msg) in add_font_resources {
        use self::AddFontMsg::*;
        match add_font_msg {
            Font(f) => {
                app_resources.currently_registered_fonts
                .get_mut(pipeline_id).unwrap()
                .insert(font_id, LoadedFont::new(f.font_key, f.font_bytes, f.font_index));
            },
            Instance(fi, size) => {
                app_resources.currently_registered_fonts
                    .get_mut(pipeline_id).unwrap()
                    .get_mut(&font_id).unwrap()
                    .font_instances.insert(size, fi.key);
            },
        }
    }
}

/// Add a new pipeline to the app resources
pub fn register_new_pipeline(
    app_resources: &mut AppResources,
    pipeline_id: PipelineId,
) {
    app_resources.currently_registered_fonts.insert(pipeline_id, FastHashMap::default());
    app_resources.currently_registered_images.insert(pipeline_id, FastHashMap::default());
    app_resources.last_frame_font_keys.insert(pipeline_id, FastHashMap::default());
    app_resources.last_frame_image_keys.insert(pipeline_id, FastHashSet::default());
}

/// Delete and remove all fonts & font instance keys from a given pipeline
pub fn delete_pipeline<T: FontImageApi>(
    app_resources: &mut AppResources,
    render_api: &mut T,
    pipeline_id: &PipelineId,
) {
    let mut delete_font_resources = Vec::new();

    for (font_id, loaded_font) in app_resources.currently_registered_fonts[&pipeline_id].iter() {
        delete_font_resources.extend(
            loaded_font.font_instances.iter()
            .map(|(au, font_instance_key)| (font_id.clone(), DeleteFontMsg::Instance(*font_instance_key, *au)))
        );
        delete_font_resources.push((font_id.clone(), DeleteFontMsg::Font(loaded_font.font_key)));
    }

    let delete_image_resources = app_resources.currently_registered_images[&pipeline_id].iter()
    .map(|(id, info)| (*id, DeleteImageMsg(info.key, *info)))
    .collect();

    delete_resources(app_resources, render_api, pipeline_id, delete_font_resources, delete_image_resources);

    app_resources.currently_registered_fonts.remove(pipeline_id);
    app_resources.currently_registered_images.remove(pipeline_id);
    app_resources.last_frame_font_keys.remove(pipeline_id);
    app_resources.last_frame_image_keys.remove(pipeline_id);
}

pub fn build_delete_font_resource_updates(
    app_resources: &AppResources,
    pipeline_id: &PipelineId,
) -> Vec<(ImmediateFontId, DeleteFontMsg)> {

    let mut resource_updates = Vec::new();

    // Delete fonts that were not used in the last frame or have zero font instances
    for (font_id, loaded_font) in app_resources.currently_registered_fonts[pipeline_id].iter() {
        resource_updates.extend(
            loaded_font.font_instances.iter()
            .filter(|(au, _)| app_resources.last_frame_font_keys[pipeline_id][font_id].contains(au))
            .map(|(au, font_instance_key)| (font_id.clone(), DeleteFontMsg::Instance(*font_instance_key, *au)))
        );
        if !app_resources.last_frame_font_keys[&pipeline_id].contains_key(font_id) || loaded_font.font_instances.is_empty() {
            // Delete the font and all instances if there are no more instances of the font
            resource_updates.push((font_id.clone(), DeleteFontMsg::Font(loaded_font.font_key)));
        }
    }

    resource_updates
}

/// At the end of the frame, all images that are registered, but weren't used in the last frame
pub fn build_delete_image_resource_updates(
    app_resources: &AppResources,
    pipeline_id: &PipelineId,
) -> Vec<(ImageId, DeleteImageMsg)> {
    app_resources.currently_registered_images[&pipeline_id].iter()
    .filter(|(id, _info)| !app_resources.last_frame_image_keys[&pipeline_id].contains(id))
    .map(|(id, info)| (*id, DeleteImageMsg(info.key, *info)))
    .collect()
}

pub fn delete_resources<T: FontImageApi>(
    app_resources: &mut AppResources,
    render_api: &mut T,
    pipeline_id: &PipelineId,
    delete_font_resources: Vec<(ImmediateFontId, DeleteFontMsg)>,
    delete_image_resources: Vec<(ImageId, DeleteImageMsg)>,
) {
    let mut merged_resource_updates = Vec::new();

    merged_resource_updates.extend(delete_font_resources.iter().map(|(_, f)| f.into_resource_update()));
    merged_resource_updates.extend(delete_image_resources.iter().map(|(_, i)| i.into_resource_update()));

    if !merged_resource_updates.is_empty() {
        render_api.update_resources(merged_resource_updates);
    }

    for (removed_id, _removed_info) in delete_image_resources {
        app_resources.currently_registered_images
        .get_mut(pipeline_id).unwrap()
        .remove(&removed_id);
    }

    for (font_id, delete_font_msg) in delete_font_resources {
        use self::DeleteFontMsg::*;
        match delete_font_msg {
            Font(_) => {
                app_resources.currently_registered_fonts
                .get_mut(pipeline_id).unwrap()
                .remove(&font_id);
            },
            Instance(_, size) => {
                app_resources.currently_registered_fonts
                .get_mut(pipeline_id).unwrap()
                .get_mut(&font_id).unwrap()
                .delete_font_instance(&size);
            },
        }
    }
}

pub fn is_image_opaque(format: RawImageFormat, bytes: &[u8]) -> bool {
    match format {
        RawImageFormat::BGRA8 => {
            let mut is_opaque = true;
            for i in 0..(bytes.len() / 4) {
                if bytes[i * 4 + 3] != 255 {
                    is_opaque = false;
                    break;
                }
            }
            is_opaque
        }
        RawImageFormat::R8 => true,
        _ => unreachable!(),
    }
}

// From webrender/wrench
// These are slow. Gecko's gfx/2d/Swizzle.cpp has better versions
pub fn premultiply(data: &mut [u8]) {
    for pixel in data.chunks_mut(4) {
        let a = u32::from(pixel[3]);
        pixel[0] = (((pixel[0] as u32 * a) + 128) / 255) as u8;
        pixel[1] = (((pixel[1] as u32 * a) + 128) / 255) as u8;
        pixel[2] = (((pixel[2] as u32 * a) + 128) / 255) as u8;
    }
}

#[test]
fn test_premultiply() {
    let mut color = [255, 0, 0, 127];
    premultiply(&mut color);
    assert_eq!(color, [127, 0, 0, 127]);
}