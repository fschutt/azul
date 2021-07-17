use core::{
    fmt,
    sync::atomic::{AtomicUsize, AtomicU32, Ordering as AtomicOrdering},
    hash::{Hash, Hasher},
};
use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::string::String;
use azul_css::{
    LayoutRect, StyleFontSize, LayoutSize,
    ColorU, U8Vec, U16Vec, F32Vec, U32Vec, AzString, OptionI32,
    FontRef, StyleFontFamilyVec, StyleFontFamily,
};
use crate::{
    FastHashMap, FastBTreeSet,
    display_list::GlStoreImageFn,
    callbacks::{RenderImageCallback, RefAny},
    ui_solver::{InlineTextLine, ResolvedTextLayoutOptions, InlineTextLayout},
    display_list::GlyphInstance,
    styled_dom::{StyledDom, StyleFontFamilyHash, StyleFontFamiliesHash},
    callbacks::{DocumentId, InlineText},
    task::ExternalSystemCallbacks,
    gl::Texture,
    window::{LogicalPosition, LogicalSize, OptionChar, LogicalRect},
};
use rust_fontconfig::FcFontCache;
pub use azul_css::FontMetrics;

/// Configuration for optional features, such as whether to enable logging or panic hooks
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct AppConfig {
    /// Which layout model to use - used for versioning changes in the layout
    /// solver so that upgrading azul won't break existing apps
    pub layout_solver: LayoutSolverVersion,
    /// If enabled, logs error and info messages.
    ///
    /// Default is `LevelFilter::Error` to log all errors by default
    pub log_level: AppLogLevel,
    /// If the app crashes / panics, a window with a message box pops up.
    /// Setting this to `false` disables the popup box.
    pub enable_visual_panic_hook: bool,
    /// If this is set to `true` (the default), a backtrace + error information
    /// gets logged to stdout and the logging file (only if logging is enabled).
    pub enable_logging_on_panic: bool,
    /// (STUB) Whether keyboard navigation should be enabled (default: true).
    /// Currently not implemented.
    pub enable_tab_navigation: bool,
    /// External callbacks to create a thread or get the curent time
    pub system_callbacks: ExternalSystemCallbacks,
}

impl AppConfig {
    pub fn default(layout_solver: LayoutSolverVersion) -> Self {
        Self {
            layout_solver,
            log_level: AppLogLevel::Error,
            enable_visual_panic_hook: true,
            enable_logging_on_panic: true,
            enable_tab_navigation: true,
            system_callbacks: ExternalSystemCallbacks::rust_internal(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutSolverVersion {
    /// Current layout model
    March2021,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum AppLogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

pub type WordIndex = usize;
pub type GlyphIndex = usize;
pub type LineLength = f32;
pub type IndexOfLineBreak = usize;
pub type RemainingSpaceToRight = f32;
pub type LineBreaks = Vec<(GlyphIndex, RemainingSpaceToRight)>;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrimitiveFlags {
    /// The CSS backface-visibility property (yes, it can be really granular)
    pub is_backface_visible: bool,
    /// If set, this primitive represents a scroll bar container
    pub is_scrollbar_container: bool,
    /// If set, this primitive represents a scroll bar thumb
    pub is_scrollbar_thumb: bool,
    /// This is used as a performance hint - this primitive may be promoted to a native
    /// compositor surface under certain (implementation specific) conditions. This
    /// is typically used for large videos, and canvas elements.
    pub prefer_compositor_surface: bool,
    /// If set, this primitive can be passed directly to the compositor via its
    /// ExternalImageId, and the compositor will use the native image directly.
    /// Used as a further extension on top of PREFER_COMPOSITOR_SURFACE.
    pub supports_external_compositor_surface: bool,
}

/// Metadata (but not storage) describing an image In WebRender.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ImageDescriptor {
    /// Format of the image data.
    pub format: RawImageFormat,
    /// Width and height of the image data, in pixels.
    pub width: usize,
    pub height: usize,
    /// The number of bytes from the start of one row to the next. If non-None,
    /// `compute_stride` will return this value, otherwise it returns
    /// `width * bpp`. Different source of images have different alignment
    /// constraints for rows, so the stride isn't always equal to width * bpp.
    pub stride: OptionI32,
    /// Offset in bytes of the first pixel of this image in its backing buffer.
    /// This is used for tiling, wherein WebRender extracts chunks of input images
    /// in order to cache, manipulate, and render them individually. This offset
    /// tells the texture upload machinery where to find the bytes to upload for
    /// this tile. Non-tiled images generally set this to zero.
    pub offset: i32,
    /// Various bool flags related to this descriptor.
    pub flags: ImageDescriptorFlags,
}

/// Various flags that are part of an image descriptor.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ImageDescriptorFlags {
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

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdNamespace(pub u32);

impl ::core::fmt::Display for IdNamespace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "IdNamespace({})", self.0)
    }
}

impl ::core::fmt::Debug for IdNamespace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum RawImageFormat {
    R8,
    RG8,
    RGB8,
    RGBA8,
    R16,
    RG16,
    RGB16,
    RGBA16,
    BGR8,
    BGRA8,
}

static IMAGE_KEY: AtomicU32 = AtomicU32::new(1); // NOTE: starts at 1 (0 = DUMMY)
static FONT_KEY: AtomicU32 = AtomicU32::new(0);
static FONT_INSTANCE_KEY: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageKey {
    pub namespace: IdNamespace,
    pub key: u32,
}

impl ImageKey {

    pub const DUMMY: Self = Self {
        namespace: IdNamespace(0),
        key: 0,
    };

    pub fn unique(render_api_namespace: IdNamespace) -> Self {
        Self {
            namespace: render_api_namespace,
            key: IMAGE_KEY.fetch_add(1, AtomicOrdering::SeqCst)
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontKey {
    pub namespace: IdNamespace,
    pub key: u32,
}

impl FontKey {
    pub fn unique(render_api_namespace: IdNamespace) -> Self {
        Self {
            namespace: render_api_namespace,
            key: FONT_KEY.fetch_add(1, AtomicOrdering::SeqCst)
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontInstanceKey {
    pub namespace: IdNamespace,
    pub key: u32,
}

impl FontInstanceKey {
    pub fn unique(render_api_namespace: IdNamespace) -> Self {
        Self {
            namespace: render_api_namespace,
            key: FONT_INSTANCE_KEY.fetch_add(1, AtomicOrdering::SeqCst)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ImageCallback {
    pub callback: RenderImageCallback,
    pub data: RefAny,
}

// NOTE: This type should NOT be exposed in the API!
// The only public functions are the constructors
#[derive(Debug)]
pub enum DecodedImage {
    /// Image that has a reserved key, but no data, i.e it is not yet rendered
    /// or there was an error during rendering
    NullImage {
        width: usize,
        height: usize,
        format: RawImageFormat
    },
    // OpenGl texture
    Gl(Texture),
    // Image backed by CPU-rendered pixels
    Raw((ImageDescriptor, ImageData)),
    // Same as `Texture`, but rendered AFTER the layout has been done
    Callback(ImageCallback),
    // YUVImage(...)
    // VulkanSurface(...)
    // MetalSurface(...),
    // DirectXSurface(...)
}

#[derive(Debug)]
#[repr(C)]
pub struct ImageRef {
    /// Shared pointer to an opaque implementation of the decoded image
    pub data: *const DecodedImage,
    /// How many copies does this image have (if 0, the font data will be deleted on drop)
    pub copies: *const AtomicUsize,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash, Ord, Eq)]
pub struct ImageRefHash(pub usize);

impl_option!(ImageRef, OptionImageRef, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl ImageRef {

    /// If *copies = 1, returns the internal image data
    pub fn into_inner(self) -> Option<DecodedImage> {
        unsafe {
            if self.copies.as_ref().map(|m| m.load(AtomicOrdering::SeqCst)) == Some(1) {
                let data = Box::from_raw(self.data as *mut DecodedImage);
                let _ = Box::from_raw(self.copies as *mut AtomicUsize);
                core::mem::forget(self); // do not run the destructor
                Some(*data)
            } else {
                None
            }
        }
    }

    pub fn get_data<'a>(&'a self) -> &'a DecodedImage { unsafe { &*self.data } }

    pub fn get_image_callback<'a>(&'a self) -> Option<&'a ImageCallback> {
        if unsafe { self.copies.as_ref().map(|m| m.load(AtomicOrdering::SeqCst)) != Some(1) } {
            return None; // not safe
        }

        match unsafe { &*self.data } {
            DecodedImage::Callback(gl_texture_callback) => Some(gl_texture_callback),
            _ => None,
        }
    }

    pub fn get_image_callback_mut<'a>(&'a mut self) -> Option<&'a mut ImageCallback> {
        if unsafe { self.copies.as_ref().map(|m| m.load(AtomicOrdering::SeqCst)) != Some(1) } {
            return None; // not safe
        }

        match unsafe { &mut *(self.data as *mut DecodedImage) } {
            DecodedImage::Callback(gl_texture_callback) => Some(gl_texture_callback),
            _ => None,
        }
    }

    /// In difference to the default shallow copy, creates a new image ref
    pub fn deep_copy(&self) -> Self {

        let new_data = match self.get_data() {
            DecodedImage::NullImage { width, height, format } => DecodedImage::NullImage { width: *width, height: *height, format: *format },
            // NOTE: textures cannot be deep-copied yet (since the OpenGL calls for that are missing from the trait),
            // so calling clone() on a GL texture will result in an empty image
            DecodedImage::Gl(tex) => DecodedImage::NullImage { width: tex.size.width as usize, height: tex.size.height as usize, format: tex.format },
            // WARNING: the data may still be a U8Vec<'static> - the data may still not be
            // actually cloned. The data only gets cloned on a write operation
            DecodedImage::Raw((descriptor, data)) => DecodedImage::Raw((descriptor.clone(), data.clone())),
            DecodedImage::Callback(cb) => DecodedImage::Callback(cb.clone()),
        };

        Self::new(new_data)
    }

    pub fn is_null_image(&self) -> bool {
        match self.get_data() {
            DecodedImage::NullImage { .. } => true,
            _ => false,
        }
    }

    pub fn is_gl_texture(&self) -> bool {
        match self.get_data() {
            DecodedImage::Gl(_) => true,
            _ => false,
        }
    }

    pub fn is_raw_image(&self) -> bool {
        match self.get_data() {
            DecodedImage::Raw((_, _)) => true,
            _ => false,
        }
    }

    pub fn is_callback(&self) -> bool {
        match self.get_data() {
            DecodedImage::Callback(_) => true,
            _ => false,
        }
    }

    /// NOTE: returns (0, 0) for a Callback
    pub fn get_size(&self) -> LogicalSize {
        match self.get_data() {
            DecodedImage::NullImage { width, height, .. } => LogicalSize::new(*width as f32, *height as f32),
            DecodedImage::Gl(tex) => LogicalSize::new(tex.size.width as f32, tex.size.height as f32),
            DecodedImage::Raw((image_descriptor, _)) => LogicalSize::new(image_descriptor.width as f32, image_descriptor.height as f32),
            DecodedImage::Callback(_) => LogicalSize::new(0.0, 0.0),
        }
    }

    pub fn get_hash(&self) -> ImageRefHash {
        ImageRefHash(self.data as usize)
    }

    pub fn invalid(width: usize, height: usize, format: RawImageFormat) -> Self {
        Self::new(DecodedImage::NullImage { width, height, format })
    }

    pub fn callback(gl_callback: RenderImageCallback, data: RefAny) -> Self {
        Self::new(DecodedImage::Callback(ImageCallback { callback: gl_callback, data }))
    }

    pub fn new_rawimage(image_data: RawImage) -> Option<Self> {
        let (image_data, image_descriptor) = image_data.into_loaded_image_source()?;
        Some(Self::new(DecodedImage::Raw((image_descriptor, image_data))))
    }

    pub fn new_gltexture(texture: Texture) -> Self {
        Self::new(DecodedImage::Gl(texture))
    }

    fn new(data: DecodedImage) -> Self {
        Self {
            data: Box::into_raw(Box::new(data)),
            copies: Box::into_raw(Box::new(AtomicUsize::new(1))),
        }
    }

    // pub fn new_vulkan(...) -> Self
}

unsafe impl Send for ImageRef { }
unsafe impl Sync for ImageRef { }

impl PartialEq for ImageRef {
    fn eq(&self, rhs: &Self) -> bool {
        self.data as usize == rhs.data as usize
    }
}

impl PartialOrd for ImageRef {
    fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
        Some((self.data as usize).cmp(&(other.data as usize)))
    }
}

impl Ord for ImageRef {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        let self_data = self.data as usize;
        let other_data = other.data as usize;
        self_data.cmp(&other_data)
    }
}

impl Eq for ImageRef { }

impl Hash for ImageRef {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        let self_data = self.data as usize;
        self_data.hash(state)
    }
}

impl Clone for ImageRef {
    fn clone(&self) -> Self {
        unsafe { self.copies.as_ref().map(|m| m.fetch_add(1, AtomicOrdering::SeqCst)); }
        Self {
            data: self.data, // copy the pointer
            copies: self.copies, // copy the pointer
        }
    }
}

impl Drop for ImageRef {
    fn drop(&mut self) {
        unsafe {
            let new_copies = self.copies.as_ref().map(|m| m.fetch_sub(1, AtomicOrdering::SeqCst));
            if new_copies == Some(0) {
                let _ = Box::from_raw(self.data as *mut DecodedImage);
                let _ = Box::from_raw(self.copies as *mut AtomicUsize);
            }
        }
    }
}


/// Stores the resources for the application, souch as fonts, images and cached
/// texts, also clipboard strings
///
/// Images and fonts can be references across window contexts (not yet tested,
/// but should work).
#[derive(Debug)]
pub struct ImageCache {
    /// The AzString is the string used in the CSS, i.e. url("my_image") = "my_image" -> ImageId(4)
    ///
    /// NOTE: This is the only map that is modifiable by the user and that has to be manually managed
    /// all other maps are library-internal only and automatically delete their resources once they
    /// aren't needed anymore
    pub image_id_map: FastHashMap<AzString, ImageRef>,
}

impl Default for ImageCache {
    fn default() -> Self {
        Self {
            image_id_map: FastHashMap::default(),
        }
    }
}

impl ImageCache {

    pub fn new() -> Self {
        Self::default()
    }

    // -- ImageId cache

    pub fn add_css_image_id(&mut self, css_id: AzString, image: ImageRef) {
        self.image_id_map.insert(css_id, image);
    }

    pub fn get_css_image_id(&self, css_id: &AzString) -> Option<&ImageRef> {
        self.image_id_map.get(css_id)
    }

    pub fn delete_css_image_id(&mut self, css_id: &AzString) {
        self.image_id_map.remove(css_id);
    }
}

/// What type of image is this?
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ImageType {
    /// CSS background-image
    Background,
    /// DOM node content
    Content,
    /// DOM node clip-mask
    ClipMask,
}

/// Renderer resources that manage font, image and font instance keys.
/// RendererResources are local to each renderer / window, since the
/// keys are not shared across renderers
///
/// The resources are automatically managed, meaning that they each new frame
/// (signified by start_frame_gc and end_frame_gc)
#[derive(Debug)]
pub struct RendererResources {
    /// All image keys currently active in the RenderApi
    pub currently_registered_images: FastHashMap<ImageRefHash, (ImageKey, ImageDescriptor)>,
    /// All image keys from the last frame, used for automatically
    /// deleting images once they aren't needed anymore
    pub last_frame_registered_images: FastHashMap<ImageRefHash, (ImageKey, ImageDescriptor)>,

    /// Map from the calculated families vec (["Arial", "Helvectia"])
    /// to the final loaded font that could be loaded
    /// (in this case "Arial" on Windows and "Helvetica" on Mac,
    /// because the fonts are loaded in fallback-order)
    pub font_families_map: FastHashMap<StyleFontFamiliesHash, StyleFontFamilyHash>,
    /// Same as AzString -> ImageId, but for fonts, i.e. "Roboto" -> FontId(9)
    pub font_id_map: FastHashMap<StyleFontFamilyHash, FontKey>,
    /// All font keys currently active in the RenderApi
    pub currently_registered_fonts: FastHashMap<FontKey, (FontRef, FastHashMap<Au, FontInstanceKey>)>,
    /// All font keys from the last frame, used for automatically
    /// deleting fonts once they aren't needed anymore
    pub last_frame_registered_fonts: FastHashMap<FontKey, FastHashMap<Au, FontInstanceKey>>,
}

impl Default for RendererResources {
    fn default() -> Self {
        Self {
            currently_registered_images: FastHashMap::default(),
            last_frame_registered_images: FastHashMap::default(),
            font_families_map: FastHashMap::default(),
            font_id_map: FastHashMap::default(),
            currently_registered_fonts: FastHashMap::default(),
            last_frame_registered_fonts: FastHashMap::default(),
        }
    }
}

impl RendererResources {

    /// Updates the internal cache, adds `ResourceUpdate::Remove()` to the `all_resource_updates`
    #[cfg(feature = "multithreading")]
    pub fn do_gc(&mut self, all_resource_updates: &mut Vec<ResourceUpdate>) {

        use crate::rayon::iter::IntoParallelRefIterator;
        use crate::rayon::iter::ParallelIterator;

        // If the last frame contains fonts but the current frame doesn't, delete them
        let mut delete_font_resources = Vec::new();
        for (font_key, font_instances) in self.last_frame_registered_fonts.iter() {
            delete_font_resources.extend(
                font_instances.iter()
                .filter(|(au, _)| !(self.currently_registered_fonts.get(font_key).map(|f| f.1.contains_key(au)).unwrap_or(false)))
                .map(|(au, font_instance_key)| (font_key.clone(), DeleteFontMsg::Instance(*font_instance_key, *au)))
            );
            // Delete the font and all instances if there are no more instances of the font
            // NOTE: deletion is in reverse order - instances are deleted first, then the font is deleted
            if !self.currently_registered_fonts.contains_key(font_key) || font_instances.is_empty() {
                delete_font_resources.push((font_key.clone(), DeleteFontMsg::Font(font_key.clone())));
            }
        }

        // If the last frame contains an image, but the current frame does not, delete it
        let delete_image_resources = self.last_frame_registered_images
        .par_iter()
        .filter(|image_key| !self.currently_registered_images.contains_key(image_key.0))
        .map(|image_key| ((*image_key.0).clone(), DeleteImageMsg(image_key.1.0.clone())))
        .collect::<Vec<_>>();

        all_resource_updates.extend(delete_font_resources.iter().map(|(_, f)| f.into_resource_update()));
        all_resource_updates.extend(delete_image_resources.iter().map(|(_, i)| i.into_resource_update()));

        // Delete all font family hashes that do not have a font key anymore
        let font_family_to_delete = self.font_id_map.iter()
        .filter_map(|(font_family, font_key)| {
            if !self.currently_registered_fonts.contains_key(font_key) {
                Some(font_family.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

        for f in font_family_to_delete {
            self.font_id_map.remove(&f); // font key does not exist anymore
        }

        let font_families_to_delete = self.font_families_map.iter()
        .filter_map(|(font_families, font_family)| {
            if !self.font_id_map.contains_key(font_family) {
                Some(font_families.clone())
            } else {
                None
            }
        }).collect::<Vec<_>>();

        for f in font_families_to_delete {
            self.font_families_map.remove(&f); // font family does not exist anymore
        }

        // Reset the GC for the next cycle
        //
        // NOTE: This system will retain fonts / images for one frame
        // long than they need to - technically this could be solved
        // via hashes
        self.last_frame_registered_fonts = self.currently_registered_fonts
        .par_iter()
        .map(|(fk, (_, fi))| (fk.clone(), fi.clone()))
        .collect();

        self.last_frame_registered_images = self.currently_registered_images.clone();
    }
}

macro_rules! unique_id {($struct_name:ident, $counter_name:ident) => {

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
    #[repr(C)]
    pub struct $struct_name {
        pub id: usize,
    }

    impl $struct_name {

        pub fn unique() -> Self {
            Self { id: $counter_name.fetch_add(1, AtomicOrdering::SeqCst) }
        }
    }
}}

// NOTE: the property key is unique across transform, color and opacity properties
static PROPERTY_KEY_COUNTER: AtomicUsize = AtomicUsize::new(0);
unique_id!(TransformKey, PROPERTY_KEY_COUNTER);
unique_id!(ColorKey, PROPERTY_KEY_COUNTER);
unique_id!(OpacityKey, PROPERTY_KEY_COUNTER);

static IMAGE_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);
unique_id!(ImageId, IMAGE_ID_COUNTER);
static FONT_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);
unique_id!(FontId, FONT_ID_COUNTER);

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct ImageMask {
    pub image: ImageRef,
    pub rect: LogicalRect,
    pub repeat: bool,
}

impl_option!(ImageMask, OptionImageMask, copy = false, [Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash]);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImmediateFontId {
    Resolved((StyleFontFamilyHash, FontKey)),
    Unresolved(StyleFontFamilyVec),
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub enum RawImageData {
    // 8-bit image data
    U8(U8Vec),
    // 16-bit image data
    U16(U16Vec),
    // HDR image data
    F32(F32Vec),
}

impl RawImageData {

    pub fn get_u8_vec_ref(&self) -> Option<&U8Vec> {
        match self {
            RawImageData::U8(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_u16_vec_ref(&self) -> Option<&U16Vec> {
        match self {
            RawImageData::U16(v) => Some(v),
            _ => None,
        }
    }

    pub fn get_f32_vec_ref(&self) -> Option<&F32Vec> {
        match self {
            RawImageData::F32(v) => Some(v),
            _ => None,
        }
    }

    fn get_u8_vec(self) -> Option<U8Vec> {
        match self {
            RawImageData::U8(v) => Some(v),
            _ => None,
        }
    }

    fn get_u16_vec(self) -> Option<U16Vec> {
        match self {
            RawImageData::U16(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct RawImage {
    pub pixels: RawImageData,
    pub width: usize,
    pub height: usize,
    pub premultiplied_alpha: bool,
    pub data_format: RawImageFormat,
}

impl RawImage {

    /// Returns a null / empty image
    pub fn null_image() -> Self {
        Self {
            pixels: RawImageData::U8(Vec::new().into()),
            width: 0,
            height: 0,
            premultiplied_alpha: true,
            data_format: RawImageFormat::BGRA8,
        }
    }

    /// Allocates a width * height, single-channel mask, used for drawing CPU image masks
    pub fn allocate_mask(size: LayoutSize) -> Self {
        Self {
            pixels: RawImageData::U8(vec![0;size.width.max(0) as usize * size.height.max(0) as usize].into()),
            width: size.width as usize,
            height: size.height as usize,
            premultiplied_alpha: true,
            data_format: RawImageFormat::R8,
        }
    }

    /// Encodes a RawImage as BGRA8 bytes and premultiplies it if the alpha is not premultiplied
    ///
    /// Returns None if the width * height * BPP does not match
    ///
    /// TODO: autovectorization fails spectacularly, need to manually optimize!
    pub fn into_loaded_image_source(self) -> Option<(ImageData, ImageDescriptor)> {

        // From webrender/wrench
        // These are slow. Gecko's gfx/2d/Swizzle.cpp has better versions
        #[inline(always)]
        fn premultiply_alpha(array: &mut [u8]) {
            if array.len() != 4 { return; }
            let a = u32::from(array[3]);
            array[0] = (((array[0] as u32 * a) + 128) / 255) as u8;
            array[1] = (((array[1] as u32 * a) + 128) / 255) as u8;
            array[2] = (((array[2] as u32 * a) + 128) / 255) as u8;
        }

        #[inline(always)]
        fn normalize_u16(i: u16) -> u8 {
            ((core::u16::MAX as f32 / i as f32) * core::u8::MAX as f32) as u8
        }

        let RawImage {
            width,
            height,
            pixels,
            mut data_format,
            premultiplied_alpha,
        } = self;

        const FOUR_BPP: usize = 4;
        const TWO_CHANNELS: usize = 2;
        const THREE_CHANNELS: usize = 3;
        const FOUR_CHANNELS: usize = 4;

        let mut is_opaque = true;

        let expected_len = width * height;

        let bytes: U8Vec = match data_format {
            RawImageFormat::R8 => {
                // just return the vec
                let pixels = pixels.get_u8_vec()?;

                if pixels.len() != expected_len {
                    return None;
                }

                data_format = RawImageFormat::R8;
                pixels
            },
            RawImageFormat::RG8 => {
                let pixels = pixels.get_u8_vec()?;

                if pixels.len() != expected_len * TWO_CHANNELS {
                    return None;
                }

                let mut px = vec![0; expected_len * FOUR_BPP];

                // TODO: premultiply alpha!
                // TODO: check that this function is SIMD optimized
                for (pixel_index, greyalpha) in pixels.as_ref().chunks_exact(TWO_CHANNELS).enumerate() {

                    let grey = greyalpha[0];
                    let alpha = greyalpha[1];

                    if alpha != 255 { is_opaque = false; }

                    px[pixel_index * FOUR_BPP] = grey;
                    px[(pixel_index * FOUR_BPP) + 1] = grey;
                    px[(pixel_index * FOUR_BPP) + 2] = grey;
                    px[(pixel_index * FOUR_BPP) + 3] = alpha;
                }

                data_format = RawImageFormat::BGRA8;
                px.into()
            },
            RawImageFormat::RGB8 => {
                let pixels = pixels.get_u8_vec()?;

                if pixels.len() != expected_len * THREE_CHANNELS {
                    return None;
                }

                let mut px = vec![0; expected_len * FOUR_BPP];

                // TODO: check that this function is SIMD optimized
                for (pixel_index, rgb) in pixels.as_ref().chunks_exact(THREE_CHANNELS).enumerate() {

                    let red = rgb[0];
                    let green = rgb[1];
                    let blue = rgb[2];

                    px[pixel_index * FOUR_BPP] = blue;
                    px[(pixel_index * FOUR_BPP) + 1] = green;
                    px[(pixel_index * FOUR_BPP) + 2] = red;
                    px[(pixel_index * FOUR_BPP) + 3] = 0xff;
                }

                data_format = RawImageFormat::BGRA8;
                px.into()
            },
            RawImageFormat::RGBA8 => {
                let mut pixels: Vec<u8> = pixels.get_u8_vec()?.into_library_owned_vec();

                if pixels.len() != expected_len * FOUR_CHANNELS {
                    return None;
                }

                // TODO: check that this function is SIMD optimized
                // no extra allocation necessary, but swizzling
                if premultiplied_alpha {
                    for rgba in pixels.chunks_exact_mut(4) {
                        let r = rgba[0];
                        let g = rgba[1];
                        let b = rgba[2];
                        let a = rgba[3];
                        if a != 255 { is_opaque = false; }
                        rgba[0] = b;
                        rgba[1] = r;
                        rgba[2] = g;
                        rgba[3] = a;
                    }
                } else {
                    for rgba in pixels.chunks_exact_mut(4) {
                        if rgba[3] != 255 { is_opaque = false; }
                        rgba[0] = rgba[3];
                        rgba[1] = rgba[2];
                        rgba[2] = rgba[1];
                        rgba[3] = rgba[0];
                        premultiply_alpha(rgba); // <-
                    }
                }

                data_format = RawImageFormat::BGRA8;
                pixels.into()
            },
            RawImageFormat::R16 => {
                let pixels = pixels.get_u16_vec()?;

                if pixels.len() != expected_len {
                    return None;
                }

                let mut px = vec![0; expected_len * FOUR_BPP];

                // TODO: check that this function is SIMD optimized
                for (pixel_index, grey_u16) in pixels.as_ref().iter().enumerate() {
                    let grey_u8 = normalize_u16(*grey_u16);
                    px[pixel_index * FOUR_BPP] = grey_u8;
                    px[(pixel_index * FOUR_BPP) + 1] = grey_u8;
                    px[(pixel_index * FOUR_BPP) + 2] = grey_u8;
                    px[(pixel_index * FOUR_BPP) + 3] = 0xff;
                }

                data_format = RawImageFormat::BGRA8;
                px.into()
            },
            RawImageFormat::RG16 => {
                let pixels = pixels.get_u16_vec()?;

                if pixels.len() != expected_len * TWO_CHANNELS  {
                    return None;
                }

                let mut px = vec![0; expected_len * FOUR_BPP];

                // TODO: check that this function is SIMD optimized
                for (pixel_index, greyalpha) in pixels.as_ref().chunks_exact(TWO_CHANNELS).enumerate() {

                    let grey_u8 = normalize_u16(greyalpha[0]);
                    let alpha_u8 = normalize_u16(greyalpha[1]);

                    if alpha_u8 != 255 { is_opaque = false; }

                    px[pixel_index * FOUR_BPP] = grey_u8;
                    px[(pixel_index * FOUR_BPP) + 1] = grey_u8;
                    px[(pixel_index * FOUR_BPP) + 2] = grey_u8;
                    px[(pixel_index * FOUR_BPP) + 3] = alpha_u8;
                }

                data_format = RawImageFormat::BGRA8;
                px.into()
            },
            RawImageFormat::RGB16 => {
                let pixels = pixels.get_u16_vec()?;

                if pixels.len() != expected_len * THREE_CHANNELS  {
                    return None;
                }

                let mut px = vec![0; expected_len * FOUR_BPP];

                // TODO: check that this function is SIMD optimized
                for (pixel_index, rgb) in pixels.as_ref().chunks_exact(THREE_CHANNELS).enumerate() {

                    let red_u8 = normalize_u16(rgb[0]);
                    let green_u8 = normalize_u16(rgb[1]);
                    let blue_u8 = normalize_u16(rgb[2]);

                    px[pixel_index * FOUR_BPP] = blue_u8;
                    px[(pixel_index * FOUR_BPP) + 1] = green_u8;
                    px[(pixel_index * FOUR_BPP) + 2] = red_u8;
                    px[(pixel_index * FOUR_BPP) + 3] = 0xff;
                }

                data_format = RawImageFormat::BGRA8;
                px.into()
            },
            RawImageFormat::RGBA16 => {
                let pixels = pixels.get_u16_vec()?;

                if pixels.len() != expected_len * FOUR_CHANNELS {
                    return None;
                }

                let mut px = vec![0; expected_len * FOUR_BPP];

                // TODO: check that this function is SIMD optimized
                if premultiplied_alpha {
                    for (pixel_index, rgba) in pixels.as_ref().chunks_exact(FOUR_CHANNELS).enumerate() {

                        let red_u8 = normalize_u16(rgba[0]);
                        let green_u8 = normalize_u16(rgba[1]);
                        let blue_u8 = normalize_u16(rgba[2]);
                        let alpha_u8 = normalize_u16(rgba[3]);

                        if alpha_u8 != 255 { is_opaque = false; }

                        px[pixel_index * FOUR_BPP] = blue_u8;
                        px[(pixel_index * FOUR_BPP) + 1] = green_u8;
                        px[(pixel_index * FOUR_BPP) + 2] = red_u8;
                        px[(pixel_index * FOUR_BPP) + 3] = alpha_u8;
                    }
                } else {
                    for (pixel_index, rgba) in pixels.as_ref().chunks_exact(FOUR_CHANNELS).enumerate() {

                        let red_u8 = normalize_u16(rgba[0]);
                        let green_u8 = normalize_u16(rgba[1]);
                        let blue_u8 = normalize_u16(rgba[2]);
                        let alpha_u8 = normalize_u16(rgba[3]);

                        if alpha_u8 != 255 { is_opaque = false; }

                        px[pixel_index * FOUR_BPP] = blue_u8;
                        px[(pixel_index * FOUR_BPP) + 1] = green_u8;
                        px[(pixel_index * FOUR_BPP) + 2] = red_u8;
                        px[(pixel_index * FOUR_BPP) + 3] = alpha_u8;
                        premultiply_alpha(&mut px[(pixel_index * FOUR_BPP)..((pixel_index * FOUR_BPP) + FOUR_BPP)]);
                    }
                }

                data_format = RawImageFormat::BGRA8;
                px.into()
            },
            RawImageFormat::BGR8 => {
                let pixels = pixels.get_u8_vec()?;

                if pixels.len() != expected_len * THREE_CHANNELS {
                    return None;
                }

                let mut px = vec![0; expected_len * FOUR_BPP];

                // TODO: check that this function is SIMD optimized
                for (pixel_index, bgr) in pixels.as_ref().chunks_exact(THREE_CHANNELS).enumerate() {

                    let blue = bgr[0];
                    let green = bgr[1];
                    let red = bgr[2];

                    px[pixel_index * FOUR_BPP] = blue;
                    px[(pixel_index * FOUR_BPP) + 1] = green;
                    px[(pixel_index * FOUR_BPP) + 2] = red;
                    px[(pixel_index * FOUR_BPP) + 3] = 0xff;
                }

                data_format = RawImageFormat::BGRA8;
                px.into()
            },
            RawImageFormat::BGRA8 => {
                if premultiplied_alpha {
                    // DO NOT CLONE THE IMAGE HERE!
                    let pixels = pixels.get_u8_vec()?;

                    is_opaque = pixels
                    .as_ref()
                    .chunks_exact(FOUR_CHANNELS)
                    .all(|bgra| bgra[3] == 255);

                    pixels
                } else {
                    let mut pixels: Vec<u8> = pixels.get_u8_vec()?.into_library_owned_vec();

                    if pixels.len() != expected_len * FOUR_BPP {
                        return None;
                    }

                    for bgra in pixels.chunks_exact_mut(FOUR_CHANNELS) {
                        if bgra[3] != 255 { is_opaque = false; }
                        premultiply_alpha(bgra);
                    }
                    data_format = RawImageFormat::BGRA8;
                    pixels.into()
                }
            },
        };

        let image_data = ImageData::Raw(bytes);
        let image_descriptor = ImageDescriptor {
            format: data_format,
            width,
            height,
            offset: 0,
            stride: None.into(),
            flags: ImageDescriptorFlags {
                is_opaque,
                allow_mipmaps: true,
            }
        };

        Some((image_data, image_descriptor))
    }
}

impl_option!(RawImage, OptionRawImage, copy = false, [Debug, Clone, PartialEq, PartialOrd]);

/// Text broken up into `Tab`, `Word()`, `Return` characters
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct Words {
    /// Words (and spaces), broken up into semantic items
    pub items: WordVec,
    /// String that makes up this paragraph of words
    pub internal_str: AzString,
    /// `internal_chars` is used in order to enable copy-paste (since taking a sub-string isn't possible using UTF-8)
    pub internal_chars: U32Vec,
}

impl Words {

    pub fn get_substr(&self, word: &Word) -> String {
        self.internal_chars.as_ref()[word.start..word.end].iter().filter_map(|c| core::char::from_u32(*c)).collect()
    }

    pub fn get_str(&self) -> &str {
        &self.internal_str.as_str()
    }

    pub fn get_char(&self, idx: usize) -> Option<char> {
        self.internal_chars.as_ref().get(idx).and_then(|c| core::char::from_u32(*c))
    }
}

/// Section of a certain type
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct Word {
    pub start: usize,
    pub end: usize,
    pub word_type: WordType,
}

impl_vec!(Word, WordVec, WordVecDestructor);
impl_vec_clone!(Word, WordVec, WordVecDestructor);
impl_vec_debug!(Word, WordVec);
impl_vec_partialeq!(Word, WordVec);
impl_vec_eq!(Word, WordVec);
impl_vec_ord!(Word, WordVec);
impl_vec_partialord!(Word, WordVec);
impl_vec_hash!(Word, WordVec);

/// Either a white-space delimited word, tab or return character
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
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
#[repr(C)]
pub struct ShapedWords {
    /// Words scaled to their appropriate font size, but not yet positioned on the screen
    pub items: ShapedWordVec,
    /// Longest word in the `self.scaled_words`, necessary for
    /// calculating overflow rectangles.
    pub longest_word_width: usize,
    /// Horizontal advance of the space glyph
    pub space_advance: usize,
    /// Units per EM square
    pub font_metrics_units_per_em: u16,
    /// Descender of the font
    pub font_metrics_ascender: i16,
    pub font_metrics_descender: i16,
    pub font_metrics_line_gap: i16,
}

impl ShapedWords {
    pub fn get_longest_word_width_px(&self, target_font_size: f32) -> f32 {
        self.longest_word_width as f32 / self.font_metrics_units_per_em as f32 * target_font_size
    }
    pub fn get_space_advance_px(&self, target_font_size: f32) -> f32 {
        self.space_advance as f32 / self.font_metrics_units_per_em as f32 * target_font_size
    }
    /// Get the distance from the top of the text to the baseline of the text (= ascender)
    pub fn get_baseline_px(&self, target_font_size: f32) -> f32 {
        target_font_size + self.get_descender(target_font_size)
    }
    /// NOTE: descender is NEGATIVE
    pub fn get_descender(&self, target_font_size: f32) -> f32 {
        self.font_metrics_descender as f32 / self.font_metrics_units_per_em as f32 * target_font_size
    }

    /// `height = sTypoAscender - sTypoDescender + sTypoLineGap`
    pub fn get_line_height(&self, target_font_size: f32) -> f32 {
        self.font_metrics_ascender as f32 / self.font_metrics_units_per_em as f32 -
        self.font_metrics_descender as f32 / self.font_metrics_units_per_em as f32 +
        self.font_metrics_line_gap as f32 / self.font_metrics_units_per_em as f32 * target_font_size
    }

    pub fn get_ascender(&self, target_font_size: f32) -> f32 {
        self.font_metrics_ascender as f32 / self.font_metrics_units_per_em as f32 * target_font_size
    }
}

/// A Unicode variation selector.
///
/// VS04-VS14 are omitted as they aren't currently used.
#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub enum VariationSelector {
    /// VARIATION SELECTOR-1
    VS01 = 1,
    /// VARIATION SELECTOR-2
    VS02 = 2,
    /// VARIATION SELECTOR-3
    VS03 = 3,
    /// Text presentation
    VS15 = 15,
    /// Emoji presentation
    VS16 = 16,
}

impl_option!(VariationSelector, OptionVariationSelector, [Debug, Copy, PartialEq, PartialOrd, Clone, Hash]);

#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C, u8)]
pub enum GlyphOrigin {
    Char(char),
    Direct,
}

#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub enum Placement {
    None,
    Distance(PlacementDistance),
}

#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub struct PlacementDistance {
    pub x: i32,
    pub y: i32,
}

impl Placement {
    #[inline]
    pub fn get_placement_relative(&self, units_per_em: u16, target_font_size: f32) -> LogicalPosition {
        let font_metrics_divisor = units_per_em as f32 / target_font_size;
        match self {
            Placement::None => LogicalPosition::new(0.0, 0.0),
            Placement::Distance(PlacementDistance { x, y }) => LogicalPosition::new(*x as f32 / font_metrics_divisor, *y as f32 / font_metrics_divisor),
        }
    }
}

/// When not Attachment::None indicates that this glyph
/// is an attachment with placement indicated by the variant.
#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C, u8)]
pub enum Attachment {
    None,
    MarkAnchor(MarkAnchorPlacement),
    /// An overprint mark.
    ///
    /// This mark is shown at the same position as the base glyph.
    ///
    /// Fields: (base glyph index in `Vec<GlyphInfo>`)
    MarkOverprint(usize),
    CursiveAnchor(CursiveAnchorPlacement),
}

/// Cursive anchored placement.
///
/// https://docs.microsoft.com/en-us/typography/opentype/spec/gpos#lookup-type-3-cursive-attachment-positioning-subtable
#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub struct CursiveAnchorPlacement {
    /// exit glyph index in the `Vec<GlyphInfo>`
    pub exit_glyph_index: usize,
    /// exit glyph anchor
    pub exit_glyph_anchor: Anchor,
    /// entry glyph anchor
    pub entry_glyph_anchor: Anchor,
}

/// An anchored mark.
///
/// This is a mark where its anchor is aligned with the base glyph anchor.
#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub struct MarkAnchorPlacement {
    /// base glyph index in `Vec<GlyphInfo>`
    pub base_glyph_index: usize,
    /// base glyph anchor
    pub base_glyph_anchor: Anchor,
    /// mark anchor
    pub mark_anchor: Anchor,
}

impl Attachment {
    #[inline]
    pub fn get_placement_relative(&self, units_per_em: u16, target_font_size: f32) -> (f32, f32) {
        match self {
            Attachment::None => (0.0, 0.0),
            Attachment::MarkAnchor(anchor) => {
                let font_metrics_divisor = units_per_em as f32 / target_font_size;
                (anchor.mark_anchor.x as f32 / font_metrics_divisor, anchor.mark_anchor.y as f32 / font_metrics_divisor)
            },
            Attachment::MarkOverprint(_) | Attachment::CursiveAnchor(_) => (0.0, 0.0),
        }
    }
}

#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub struct Anchor {
    pub x: i16,
    pub y: i16,
}

#[derive(Debug, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub struct RawGlyph {
    pub unicode_codepoint: OptionChar, // Option<char>
    pub glyph_index: u16,
    pub liga_component_pos: u16,
    pub glyph_origin: GlyphOrigin,
    pub small_caps: bool,
    pub multi_subst_dup: bool,
    pub is_vert_alt: bool,
    pub fake_bold: bool,
    pub fake_italic: bool,
    pub variation: OptionVariationSelector,
}

impl RawGlyph {

    pub fn has_codepoint(&self) -> bool {
        self.unicode_codepoint.is_some()
    }

    pub fn get_codepoint(&self) -> Option<char> {
        self.unicode_codepoint.as_ref().and_then(|u| core::char::from_u32(*u))
    }
}

#[derive(Debug, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub struct GlyphInfo {
    pub glyph: RawGlyph,
    pub size: Advance,
    pub placement: Placement,
    pub attachment: Attachment,
}

#[cfg(feature = "multithreading")]
pub fn get_inline_text(words: &Words, shaped_words: &ShapedWords, word_positions: &WordPositions, inline_text_layout: &InlineTextLayout) -> InlineText {

    use crate::callbacks::{
        InlineWord, InlineLine,
        InlineTextContents, InlineGlyph
    };
    use rayon::prelude::*;

    // check the range so that in the worst case there isn't a random crash here
    fn get_range_checked_inclusive_end(input: &[Word], word_start: usize, word_end: usize) -> Option<&[Word]> {
        if word_start < input.len() &&
           word_end < input.len() &&
           word_start <= word_end {
            Some(&input[word_start..=word_end])
        } else {
            None
        }
    }

    let font_size_px = word_positions.text_layout_options.font_size_px;
    let descender_px = &shaped_words.get_descender(font_size_px); // descender is NEGATIVE
    let letter_spacing_px = word_positions.text_layout_options.letter_spacing.as_ref().copied().unwrap_or(0.0);
    let units_per_em = shaped_words.font_metrics_units_per_em;

    let inline_lines = inline_text_layout.lines
    .as_ref()
    .par_iter()
    .filter_map(|line| {

        let word_items = words.items.as_ref();
        let word_start = line.word_start.min(line.word_end);
        let word_end = line.word_end.max(line.word_start);

        let words = get_range_checked_inclusive_end(word_items, word_start, word_end)?
        .par_iter()
        .enumerate()
        .filter_map(|(word_idx, word)| {
            let word_idx = word_start + word_idx;
            match word.word_type {
                WordType::Word => {

                    let word_position = word_positions.word_positions.get(word_idx)?;
                    let shaped_word_index = word_position.shaped_word_index?;
                    let shaped_word = shaped_words.items.get(shaped_word_index)?;

                    // most words are less than 16 chars, avg length of an english word is 4.7 chars
                    let mut all_glyphs_in_this_word = Vec::<InlineGlyph>::with_capacity(16);
                    let mut x_pos_in_word_px = 0.0;

                    // all words only store the unscaled horizontal advance + horizontal kerning
                    for glyph_info in shaped_word.glyph_infos.iter() {

                        // local x and y displacement of the glyph - does NOT advance the horizontal cursor!
                        let displacement = glyph_info.placement.get_placement_relative(units_per_em, font_size_px);

                        // if the character is a mark, the mark displacement has to be added ON TOP OF the existing displacement
                        // the origin should be relative to the word, not the final text
                        let (letter_spacing_for_glyph, origin) = match glyph_info.attachment {
                            Attachment::None => {
                                (letter_spacing_px, LogicalPosition::new(x_pos_in_word_px + displacement.x, displacement.y))
                            },
                            Attachment::MarkAnchor(MarkAnchorPlacement { base_glyph_index, .. }) => {
                                let anchor = &all_glyphs_in_this_word[base_glyph_index];
                                (0.0, anchor.bounds.origin + displacement) // TODO: wrong
                            },
                            Attachment::MarkOverprint(index) => {
                                let anchor = &all_glyphs_in_this_word[index];
                                (0.0, anchor.bounds.origin + displacement)
                            },
                            Attachment::CursiveAnchor(CursiveAnchorPlacement { exit_glyph_index, .. }) => {
                                let anchor = &all_glyphs_in_this_word[exit_glyph_index];
                                (0.0, anchor.bounds.origin + displacement) // TODO: wrong
                            },
                        };

                        let glyph_scale_x = glyph_info.size.get_x_size_scaled(units_per_em, font_size_px);
                        let glyph_scale_y = glyph_info.size.get_y_size_scaled(units_per_em, font_size_px);

                        let glyph_advance_x = glyph_info.size.get_x_advance_scaled(units_per_em, font_size_px);
                        let kerning_x = glyph_info.size.get_kerning_scaled(units_per_em, font_size_px);

                        let inline_char = InlineGlyph {
                            bounds: LogicalRect::new(origin, LogicalSize::new(glyph_scale_x, glyph_scale_y)),
                            unicode_codepoint: glyph_info.glyph.unicode_codepoint,
                            glyph_index: glyph_info.glyph.glyph_index as u32,
                        };

                        x_pos_in_word_px += glyph_advance_x + kerning_x + letter_spacing_for_glyph;

                        all_glyphs_in_this_word.push(inline_char);
                    }

                    let inline_word = InlineWord::Word(InlineTextContents {
                        glyphs: all_glyphs_in_this_word.into(),
                        bounds: LogicalRect::new(
                            word_position.position,
                            word_position.size
                        ),
                    });

                    Some(inline_word)
                },
                WordType::Tab => Some(InlineWord::Tab),
                WordType::Return => Some(InlineWord::Return),
                WordType::Space => Some(InlineWord::Space),
            }
        }).collect::<Vec<InlineWord>>();

        Some(InlineLine {
            words: words.into(),
            bounds: line.bounds,
        })
    }).collect::<Vec<InlineLine>>();

    InlineText {
        lines: inline_lines.into(), // relative to 0, 0
        content_size: word_positions.content_size,
        font_size_px,
        last_word_index: word_positions.number_of_shaped_words,
        baseline_descender_px: *descender_px,
    }
}

impl_vec!(GlyphInfo, GlyphInfoVec, GlyphInfoVecDestructor);
impl_vec_clone!(GlyphInfo, GlyphInfoVec, GlyphInfoVecDestructor);
impl_vec_debug!(GlyphInfo, GlyphInfoVec);
impl_vec_partialeq!(GlyphInfo, GlyphInfoVec);
impl_vec_partialord!(GlyphInfo, GlyphInfoVec);
impl_vec_hash!(GlyphInfo, GlyphInfoVec);

#[derive(Debug, Default, Copy, PartialEq, PartialOrd, Clone, Hash)]
#[repr(C)]
pub struct Advance {
    pub advance_x: u16,
    pub size_x: i32,
    pub size_y: i32,
    pub kerning: i16,
}

impl Advance {

    #[inline]
    pub const fn get_x_advance_total_unscaled(&self) -> i32 { self.advance_x as i32 + self.kerning as i32 }
    #[inline]
    pub const fn get_x_advance_unscaled(&self) -> u16 { self.advance_x }
    #[inline]
    pub const fn get_x_size_unscaled(&self) -> i32 { self.size_x }
    #[inline]
    pub const fn get_y_size_unscaled(&self) -> i32 { self.size_y }
    #[inline]
    pub const fn get_kerning_unscaled(&self) -> i16 { self.kerning }

    #[inline]
    pub fn get_x_advance_total_scaled(&self, units_per_em: u16, target_font_size: f32) -> f32 {
        self.get_x_advance_total_unscaled() as f32 / units_per_em as f32 * target_font_size
    }
    #[inline]
    pub fn get_x_advance_scaled(&self, units_per_em: u16, target_font_size: f32) -> f32 {
        self.get_x_advance_unscaled() as f32 / units_per_em as f32 * target_font_size
    }
    #[inline]
    pub fn get_x_size_scaled(&self, units_per_em: u16, target_font_size: f32) -> f32 {
        self.get_x_size_unscaled() as f32 / units_per_em as f32 * target_font_size
    }
    #[inline]
    pub fn get_y_size_scaled(&self, units_per_em: u16, target_font_size: f32) -> f32 {
        self.get_y_size_unscaled() as f32 / units_per_em as f32 * target_font_size
    }
    #[inline]
    pub fn get_kerning_scaled(&self, units_per_em: u16, target_font_size: f32) -> f32 {
        self.get_kerning_unscaled() as f32 / units_per_em as f32 * target_font_size
    }
}

/// Word that is scaled (to a font / font instance), but not yet positioned
#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct ShapedWord {
    /// Glyph codepoint, glyph ID + kerning data
    pub glyph_infos: GlyphInfoVec,
    /// The sum of the width of all the characters in this word
    pub word_width: usize,
}

impl_vec!(ShapedWord, ShapedWordVec, ShapedWordVecDestructor);
impl_vec_clone!(ShapedWord, ShapedWordVec, ShapedWordVecDestructor);
impl_vec_partialeq!(ShapedWord, ShapedWordVec);
impl_vec_partialord!(ShapedWord, ShapedWordVec);
impl_vec_debug!(ShapedWord, ShapedWordVec);

impl ShapedWord {
    pub fn get_word_width(&self, units_per_em: u16, target_font_size: f32) -> f32 {
        self.word_width as f32 / units_per_em as f32 * target_font_size
    }
    /// Returns the number of glyphs THAT ARE NOT DIACRITIC MARKS
    pub fn number_of_glyphs(&self) -> usize {
        self.glyph_infos.iter().filter(|i| i.attachment == Attachment::None).count()
    }
}

/// Stores the positions of the vertically laid out texts
#[derive(Debug, Clone, PartialEq)]
pub struct WordPositions {
    /// Options like word spacing, character spacing, etc. that were
    /// used to layout these glyphs
    pub text_layout_options: ResolvedTextLayoutOptions,
    /// Stores the positions of words.
    pub word_positions: Vec<WordPosition>,
    /// Index of the word at which the line breaks + length of line
    /// (useful for text selection + horizontal centering)
    pub line_breaks: Vec<InlineTextLine>,
    /// Horizontal width of the last line (in pixels), necessary for inline layout later on,
    /// so that the next text run can contine where the last text run left off.
    ///
    /// Usually, the "trailing" of the current text block is the "leading" of the
    /// next text block, to make it seem like two text runs push into each other.
    pub trailing: f32,
    /// How many words are in the text?
    pub number_of_shaped_words: usize,
    /// How many lines (NOTE: virtual lines, meaning line breaks in the layouted text) are there?
    pub number_of_lines: usize,
    /// Horizontal and vertical boundaries of the layouted words.
    ///
    /// Note that the vertical extent can be larger than the last words' position,
    /// because of trailing negative glyph advances.
    pub content_size: LogicalSize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WordPosition {
    pub shaped_word_index: Option<usize>,
    pub position: LogicalPosition,
    pub size: LogicalSize,
}

/// Returns the layouted glyph instances
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutedGlyphs {
    pub glyphs: Vec<GlyphInstance>,
}

/// Scans the `StyledDom` for new images and fonts. After this call,
/// the `all_resource_updates` contains all the `AddFont` / `AddImage`
/// / `AddFontInstance` messages.
#[cfg(feature = "multithreading")]
pub fn add_fonts_and_images(
    image_cache: &ImageCache,
    renderer_resources: &mut RendererResources,
    fc_cache: &FcFontCache,
    render_api_namespace: IdNamespace,
    epoch: Epoch,
    document_id: &DocumentId,
    all_resource_updates: &mut Vec<ResourceUpdate>,
    styled_dom: &StyledDom,
    load_font_fn: LoadFontFn,
    parse_font_fn: ParseFontFn,
    insert_into_active_gl_textures: GlStoreImageFn,
) {
    let new_image_keys = styled_dom.scan_for_image_keys(&image_cache);
    let new_font_keys = styled_dom.scan_for_font_keys(&renderer_resources);

    let add_image_resource_updates = build_add_image_resource_updates(
        renderer_resources,
        render_api_namespace,
        epoch,
        document_id,
        &new_image_keys,
        insert_into_active_gl_textures
    );

    let add_font_resource_updates = build_add_font_resource_updates(
        renderer_resources,
        fc_cache,
        render_api_namespace,
        &new_font_keys,
        load_font_fn,
        parse_font_fn
    );

    add_resources(
        renderer_resources,
        all_resource_updates,
        add_font_resource_updates,
        add_image_resource_updates
    );
}

pub fn font_size_to_au(font_size: StyleFontSize) -> Au {
    use crate::ui_solver::DEFAULT_FONT_SIZE_PX;
    Au::from_px(font_size.inner.to_pixels(DEFAULT_FONT_SIZE_PX as f32))
}

pub type FontInstanceFlags = u32;

// Common flags
pub const FONT_INSTANCE_FLAG_SYNTHETIC_BOLD: u32    = 1 << 1;
pub const FONT_INSTANCE_FLAG_EMBEDDED_BITMAPS: u32  = 1 << 2;
pub const FONT_INSTANCE_FLAG_SUBPIXEL_BGR: u32      = 1 << 3;
pub const FONT_INSTANCE_FLAG_TRANSPOSE: u32         = 1 << 4;
pub const FONT_INSTANCE_FLAG_FLIP_X: u32            = 1 << 5;
pub const FONT_INSTANCE_FLAG_FLIP_Y: u32            = 1 << 6;
pub const FONT_INSTANCE_FLAG_SUBPIXEL_POSITION: u32 = 1 << 7;

// Windows flags
pub const FONT_INSTANCE_FLAG_FORCE_GDI: u32         = 1 << 16;

// Mac flags
pub const FONT_INSTANCE_FLAG_FONT_SMOOTHING: u32    = 1 << 16;

// FreeType flags
pub const FONT_INSTANCE_FLAG_FORCE_AUTOHINT: u32    = 1 << 16;
pub const FONT_INSTANCE_FLAG_NO_AUTOHINT: u32       = 1 << 17;
pub const FONT_INSTANCE_FLAG_VERTICAL_LAYOUT: u32   = 1 << 18;
pub const FONT_INSTANCE_FLAG_LCD_VERTICAL: u32      = 1 << 19;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct GlyphOptions {
    pub render_mode: FontRenderMode,
    pub flags: FontInstanceFlags,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum FontRenderMode {
    Mono,
    Alpha,
    Subpixel,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct FontInstancePlatformOptions {
    // empty for now
}

#[cfg(target_os = "windows")]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct FontInstancePlatformOptions {
    pub gamma: u16,
    pub contrast: u8,
    pub cleartype_level: u8,
}

#[cfg(target_os = "macos")]
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct FontInstancePlatformOptions {
    pub unused: u32,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct FontInstancePlatformOptions {
    pub lcd_filter: FontLCDFilter,
    pub hinting: FontHinting,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum FontHinting {
    None,
    Mono,
    Light,
    Normal,
    LCD,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum FontLCDFilter {
    None,
    Default,
    Light,
    Legacy,
}

impl Default for FontLCDFilter {
    fn default() -> Self { FontLCDFilter::Default }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct FontInstanceOptions {
    pub render_mode: FontRenderMode,
    pub flags: FontInstanceFlags,
    pub bg_color: ColorU,
    /// When bg_color.a is != 0 and render_mode is FontRenderMode::Subpixel,
    /// the text will be rendered with bg_color.r/g/b as an opaque estimated
    /// background color.
    pub synthetic_italics: SyntheticItalics,
}

impl Default for FontInstanceOptions {
    fn default() -> FontInstanceOptions {
        FontInstanceOptions {
            render_mode: FontRenderMode::Subpixel,
            flags: 0,
            bg_color: ColorU::TRANSPARENT,
            synthetic_italics: SyntheticItalics::default(),
        }
    }
}


#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct SyntheticItalics {
    pub angle: i16,
}

impl Default for SyntheticItalics {
    fn default() -> Self {
        Self { angle: 0 }
    }
}

/// Represents the backing store of an arbitrary series of pixels for display by
/// WebRender. This storage can take several forms.
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C, u8)]
pub enum ImageData {
    /// A simple series of bytes, provided by the embedding and owned by WebRender.
    /// The format is stored out-of-band, currently in ImageDescriptor.
    Raw(U8Vec),
    /// An image owned by the embedding, and referenced by WebRender. This may
    /// take the form of a texture or a heap-allocated buffer.
    External(ExternalImageData),
}

/// Storage format identifier for externally-managed images.
#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum ExternalImageType {
    /// The image is texture-backed.
    TextureHandle(ImageBufferKind),
    /// The image is heap-allocated by the embedding.
    Buffer,
}

/// An arbitrary identifier for an external image provided by the
/// application. It must be a unique identifier for each external
/// image.
#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct ExternalImageId { pub inner: u64 }

static LAST_EXTERNAL_IMAGE_ID: AtomicUsize = AtomicUsize::new(0);

impl ExternalImageId {
    /// Creates a new, unique ExternalImageId
    pub fn new() -> Self {
        Self { inner: LAST_EXTERNAL_IMAGE_ID.fetch_add(1, AtomicOrdering::SeqCst) as u64 }
    }
}

/// Specifies the type of texture target in driver terms.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(C)]
pub enum ImageBufferKind {
    /// Standard texture. This maps to GL_TEXTURE_2D in OpenGL.
    Texture2D = 0,
    /// Rectangle texture. This maps to GL_TEXTURE_RECTANGLE in OpenGL. This
    /// is similar to a standard texture, with a few subtle differences
    /// (no mipmaps, non-power-of-two dimensions, different coordinate space)
    /// that make it useful for representing the kinds of textures we use
    /// in WebRender. See https://www.khronos.org/opengl/wiki/Rectangle_Texture
    /// for background on Rectangle textures.
    TextureRect = 1,
    /// External texture. This maps to GL_TEXTURE_EXTERNAL_OES in OpenGL, which
    /// is an extension. This is used for image formats that OpenGL doesn't
    /// understand, particularly YUV. See
    /// https://www.khronos.org/registry/OpenGL/extensions/OES/OES_EGL_image_external.txt
    TextureExternal = 2,
}

/// Descriptor for external image resources. See `ImageData`.
#[repr(C)]
#[derive(Debug, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct ExternalImageData {
    /// The identifier of this external image, provided by the embedding.
    pub id: ExternalImageId,
    /// For multi-plane images (i.e. YUV), indicates the plane of the
    /// original image that this struct represents. 0 for single-plane images.
    pub channel_index: u8,
    /// Storage format identifier.
    pub image_type: ExternalImageType,
}

pub type TileSize = u16;

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum ImageDirtyRect {
    All,
    Partial(LayoutRect)
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum ResourceUpdate {
    AddFont(AddFont),
    DeleteFont(FontKey),
    AddFontInstance(AddFontInstance),
    DeleteFontInstance(FontInstanceKey),
    AddImage(AddImage),
    UpdateImage(UpdateImage),
    DeleteImage(ImageKey),
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct AddImage {
    pub key: ImageKey,
    pub descriptor: ImageDescriptor,
    pub data: ImageData,
    pub tiling: Option<TileSize>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct UpdateImage {
    pub key: ImageKey,
    pub descriptor: ImageDescriptor,
    pub data: ImageData,
    pub dirty_rect: ImageDirtyRect,
}

#[derive(Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct AddFont {
    pub key: FontKey,
    pub font_bytes: U8Vec,
    pub font_index: u32,
}

impl fmt::Debug for AddFont {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AddFont {{ key: {:?}, font_bytes: [u8;{}], font_index: {} }}", self.key, self.font_bytes.len(), self.font_index)
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct AddFontInstance {
    pub key: FontInstanceKey,
    pub font_key: FontKey,
    pub glyph_size: Au,
    pub options: Option<FontInstanceOptions>,
    pub platform_options: Option<FontInstancePlatformOptions>,
    pub variations: Vec<FontVariation>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialOrd, PartialEq)]
pub struct FontVariation {
    pub tag: u32,
    pub value: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Epoch(pub u32);

impl Epoch {
    // We don't want the epoch to increase to u32::MAX, since
    // u32::MAX represents an invalid epoch, which could confuse webrender
    pub fn increment(&mut self) {
        use core::u32;
        const MAX_ID: u32 = u32::MAX - 1;
        *self = match self.0 {
            MAX_ID => Epoch(0),
            other => Epoch(other + 1),
        };
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, PartialOrd, Eq, Ord)]
pub struct Au(pub i32);

pub const AU_PER_PX: i32 = 60;
pub const MAX_AU: i32 = (1 << 30) - 1;
pub const MIN_AU: i32 = -(1 << 30) - 1;

impl Au {
    pub fn from_px(px: f32) -> Self {
        let target_app_units = (px * AU_PER_PX as f32) as i32;
        Au(target_app_units.min(MAX_AU).max(MIN_AU))
    }
    pub fn into_px(&self) -> f32 { self.0 as f32 / AU_PER_PX as f32 }
}

// Debug, PartialEq, Eq, PartialOrd, Ord
#[derive(Debug)]
pub enum AddFontMsg {
    // add font: font key, font bytes + font index
    Font(FontKey, StyleFontFamilyHash, FontRef),
    Instance(AddFontInstance, Au),
}

impl AddFontMsg {
    pub fn into_resource_update(&self) -> ResourceUpdate {
        use self::AddFontMsg::*;
        match self {
            Font(font_key, _, font_ref) => ResourceUpdate::AddFont(AddFont {
                key: *font_key,
                font_bytes: font_ref.get_data().bytes.clone(),
                font_index: font_ref.get_data().font_index,
            }),
            Instance(fi, _) => ResourceUpdate::AddFontInstance(fi.clone()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum DeleteFontMsg {
    Font(FontKey),
    Instance(FontInstanceKey, Au),
}

impl DeleteFontMsg {
    pub fn into_resource_update(&self) -> ResourceUpdate {
        use self::DeleteFontMsg::*;
        match self {
            Font(f) => ResourceUpdate::DeleteFont(*f),
            Instance(fi, _) => ResourceUpdate::DeleteFontInstance(*fi),
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct AddImageMsg(pub AddImage);

impl AddImageMsg {
    pub fn into_resource_update(&self) -> ResourceUpdate {
        ResourceUpdate::AddImage(self.0.clone())
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct DeleteImageMsg(ImageKey);

impl DeleteImageMsg {
    pub fn into_resource_update(&self) -> ResourceUpdate {
        ResourceUpdate::DeleteImage(self.0.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct LoadedFontSource {
    pub data: U8Vec,
    pub index: u32,
    pub load_outlines: bool,
}

// function to load the font source from a file
pub type LoadFontFn = fn(&StyleFontFamily, &FcFontCache) -> Option<LoadedFontSource>;

// function to parse the font given the loaded font source
pub type ParseFontFn = fn(LoadedFontSource) -> Option<FontRef>; // = Option<Box<azul_text_layout::Font>>

/// Given the fonts of the current frame, returns `AddFont` and `AddFontInstance`s of
/// which fonts / instances are currently not in the `current_registered_fonts` and
/// need to be added.
///
/// Deleting fonts can only be done after the entire frame has finished drawing,
/// otherwise (if removing fonts would happen after every DOM) we'd constantly
/// add-and-remove fonts after every IFrameCallback, which would cause a lot of
/// I/O waiting.
pub fn build_add_font_resource_updates(
    renderer_resources: &mut RendererResources,
    fc_cache: &FcFontCache,
    id_namespace: IdNamespace,
    fonts_in_dom: &FastHashMap<ImmediateFontId, FastBTreeSet<Au>>,
    font_source_load_fn: LoadFontFn,
    parse_font_fn: ParseFontFn,
) -> Vec<(StyleFontFamilyHash, AddFontMsg)> {

    let mut resource_updates = alloc::vec::Vec::new();
    let mut font_instances_added_this_frame = FastBTreeSet::new();

    'outer: for (im_font_id, font_sizes) in fonts_in_dom {

        macro_rules! insert_font_instances {($font_family_hash:expr, $font_key:expr, $font_size:expr) => ({

            let font_instance_key_exists = renderer_resources.currently_registered_fonts
                .get(&$font_key)
                .and_then(|(_, font_instances)| font_instances.get(&$font_size))
                .is_some() || font_instances_added_this_frame.contains(&($font_key, $font_size));

            if !font_instance_key_exists {

                let font_instance_key = FontInstanceKey::unique(id_namespace);

                // For some reason the gamma is way to low on Windows
                #[cfg(target_os = "windows")]
                let platform_options = FontInstancePlatformOptions {
                    gamma: 300,
                    contrast: 100,
                    cleartype_level: 100,
                };

                #[cfg(target_os = "linux")]
                let platform_options = FontInstancePlatformOptions {
                    lcd_filter: FontLCDFilter::Default,
                    hinting: FontHinting::Normal,
                };

                #[cfg(target_os = "macos")]
                let platform_options = FontInstancePlatformOptions::default();

                #[cfg(target_arch = "wasm32")]
                let platform_options = FontInstancePlatformOptions::default();

                let options = FontInstanceOptions {
                    render_mode: FontRenderMode::Subpixel,
                    flags: 0 | FONT_INSTANCE_FLAG_NO_AUTOHINT,
                    .. Default::default()
                };

                font_instances_added_this_frame.insert(($font_key, $font_size));
                resource_updates.push(($font_family_hash, AddFontMsg::Instance(AddFontInstance {
                    key: font_instance_key,
                    font_key: $font_key,
                    glyph_size: $font_size,
                    options: Some(options),
                    platform_options: Some(platform_options),
                    variations: alloc::vec::Vec::new(),
                }, $font_size)));
            }
        })}

        match im_font_id {
            ImmediateFontId::Resolved((font_family_hash, font_id)) => {
                // nothing to do, font is already added,
                // just insert the missing font instances
                for font_size in font_sizes.iter() {
                    insert_font_instances!(*font_family_hash, *font_id, *font_size);
                }
            },
            ImmediateFontId::Unresolved(style_font_families) => {

                // If the font is already loaded during the current frame,
                // do not attempt to load it again
                //
                // This prevents duplicated loading for fonts in different orders, i.e.
                // - vec!["Times New Roman", "serif"] and
                // - vec!["sans", "Times New Roman"]
                // ... will resolve to the same font instead of creating two fonts

                // If there is no font key, that means there's also no font instances
                let mut font_family_hash = None;
                let font_families_hash = StyleFontFamiliesHash::new(style_font_families.as_ref());

                // Find the first font that can be loaded and parsed
                'inner: for family in style_font_families.as_ref().iter() {

                    let current_family_hash = StyleFontFamilyHash::new(&family);

                    if let Some(font_id) = renderer_resources.font_id_map.get(&current_family_hash) {
                        // font key already exists
                        for font_size in font_sizes {
                            insert_font_instances!(current_family_hash, *font_id, *font_size);
                        }
                        continue 'outer;
                    }

                    let font_ref = match family {
                        StyleFontFamily::Ref(r) => r.clone(), // Clone the FontRef
                        other => {

                            // Load and parse the font
                            let font_data = match (font_source_load_fn)(&other, fc_cache) {
                                Some(s) => s,
                                None => continue 'inner,
                            };

                            let font_ref = match (parse_font_fn)(font_data) {
                                Some(s) => s,
                                None => continue 'inner,
                            };

                            font_ref
                        }
                    };

                    // font loaded properly
                    font_family_hash = Some((current_family_hash, font_ref));
                    break 'inner;
                }

                let (font_family_hash, font_ref) = match font_family_hash {
                    None => continue 'outer, // No font could be loaded, try again next frame
                    Some(s) => s,
                };

                // Generate a new font key, store the mapping between hash and font key
                let font_key = FontKey::unique(id_namespace);
                let add_font_msg = AddFontMsg::Font(font_key, font_family_hash, font_ref);

                renderer_resources.font_id_map.insert(font_family_hash, font_key);
                renderer_resources.font_families_map.insert(font_families_hash, font_family_hash);
                resource_updates.push((font_family_hash, add_font_msg));

                // Insert font sizes for the newly generated font key
                for font_size in font_sizes {
                    insert_font_instances!(font_family_hash, font_key, *font_size);
                }
            }
        }
    }

    resource_updates
}

/// Given the images of the current frame, returns `AddImage`s of
/// which image keys are currently not in the `current_registered_images` and
/// need to be added.
///
/// Deleting images can only be done after the entire frame has finished drawing,
/// otherwise (if removing images would happen after every DOM) we'd constantly
/// add-and-remove images after every IFrameCallback, which would cause a lot of
/// I/O waiting.
#[allow(unused_variables)]
#[cfg(feature = "multithreading")]
pub fn build_add_image_resource_updates(
    renderer_resources: &RendererResources,
    id_namespace: IdNamespace,
    epoch: Epoch,
    document_id: &DocumentId,
    images_in_dom: &FastBTreeSet<ImageRef>,
    insert_into_active_gl_textures: GlStoreImageFn,
) -> Vec<(ImageRefHash, AddImageMsg)> {

    use rayon::prelude::*;

    images_in_dom
    .par_iter()
    .filter_map(|image_ref| {
        let image_ref_hash = image_ref.get_hash();

        if renderer_resources.currently_registered_images.contains_key(&image_ref_hash) {
            return None;
        }

        // NOTE: The image_ref.clone() is a shallow clone,
        // does not actually clone the data
        match image_ref.get_data() {
            DecodedImage::Gl(texture) => {
                let descriptor = texture.get_descriptor();
                let key = ImageKey::unique(id_namespace);
                // NOTE: The texture is not really cloned here,
                let external_image_id = (insert_into_active_gl_textures)(*document_id, epoch, texture.library_internal_clone());
                Some((image_ref_hash, AddImageMsg(
                    AddImage {
                        key,
                        data: ImageData::External(ExternalImageData {
                            id: external_image_id,
                            channel_index: 0,
                            image_type: ExternalImageType::TextureHandle(ImageBufferKind::Texture2D),
                        }),
                        descriptor,
                        tiling: None,
                    }
                )))
            },
            DecodedImage::Raw((descriptor, data)) => {
                let key = ImageKey::unique(id_namespace);
                Some((image_ref_hash, AddImageMsg(AddImage {
                    key,
                    data: data.clone(), // deep-copy except in the &'static case
                    descriptor: descriptor.clone(), // deep-copy, but struct is not very large
                    tiling: None
                })))
            },
            DecodedImage::NullImage { width, height, format } => None,
            DecodedImage::Callback(_) => None, // Texture callbacks are handled after layout is done
        }
    }).collect()
}

/// Submits the `AddFont`, `AddFontInstance` and `AddImage` resources to the RenderApi.
/// Extends `currently_registered_images` and `currently_registered_fonts` by the
/// `last_frame_image_keys` and `last_frame_font_keys`, so that we don't lose track of
/// what font and image keys are currently in the API.
pub fn add_resources(
    renderer_resources: &mut RendererResources,
    all_resource_updates: &mut Vec<ResourceUpdate>,
    add_font_resources: Vec<(StyleFontFamilyHash, AddFontMsg)>,
    add_image_resources: Vec<(ImageRefHash, AddImageMsg)>,
) {
    all_resource_updates.extend(add_font_resources.iter().map(|(_, f)| f.into_resource_update()));
    all_resource_updates.extend(add_image_resources.iter().map(|(_, i)| i.into_resource_update()));

    for (image_ref_hash, add_image_msg) in add_image_resources.iter() {
        renderer_resources.currently_registered_images
        .insert(*image_ref_hash, (add_image_msg.0.key, add_image_msg.0.descriptor));
    }

    for (_, add_font_msg) in add_font_resources {
        use self::AddFontMsg::*;
        match add_font_msg {
            Font(fk, _hash, font_ref) => {
                renderer_resources.currently_registered_fonts
                .entry(fk)
                .or_insert_with(|| (font_ref, FastHashMap::default()));
            },
            Instance(fi, size) => {
                if let Some((_, instances)) = renderer_resources.currently_registered_fonts.get_mut(&fi.font_key) {
                    instances.insert(size, fi.key);
                }
            },
        }
    }
}
