use std::{
    fmt,
    path::PathBuf,
    io::Error as IoError,
    sync::atomic::{AtomicUsize, Ordering},
    collections::hash_map::Entry::*,
};
use webrender::api::{
    FontKey, ImageData, ImageDescriptor, FontInstanceKey,
    RenderApi, ResourceUpdate, AddImage, ImageKey, AddFont,
};
pub use webrender::api::ImageFormat as RawImageFormat;
#[cfg(feature = "image_loading")]
use image::ImageError;
use FastHashMap;
use app_units::Au;
use clipboard2::{Clipboard, ClipboardError, SystemClipboard};
use azul_css::{PixelValue, StyleLetterSpacing};
use {
    FastHashSet,
    images::ImageInfo,
    ui_description::UiDescription,
    text_layout::{split_text_into_words, TextSizePx},
    text_cache::{TextId, TextCache},
    window::{FakeDisplay, WindowCreateError},
    app::AppConfig,
    traits::Layout,
};

pub type CssImageId = String;
pub type CssFontId = String;

static IMAGE_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageId {
    id: usize,
}

impl ImageId {
    pub(crate) fn new() -> Self {
        let unique_id = IMAGE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self {
            id: unique_id,
        }
    }
}

static FONT_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontId {
    id: usize,
}

impl FontId {
    pub(crate) fn new() -> Self {
        let unique_id = FONT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self {
            id: unique_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImageSource {
    /// The image is embedded inside the binary file
    Embedded(&'static [u8]),
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

#[derive(Debug)]
pub enum ImageReloadError {
    Io(IoError, PathBuf),
}

impl Clone for ImageReloadError {
    fn clone(&self) -> Self {
        use self::ImageReloadError::*;
        match self {
            Io(err, path) => Io(IoError::new(err.kind(), "Io Error"), path.clone()),
        }
    }
}

impl fmt::Display for ImageReloadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::ImageReloadError::*;
        match self {
            Io(err, path_buf) => write!(f, "Could not load \"{}\" - IO error: {}", path_buf.as_path().to_string_lossy(), err),
        }
    }
}

#[derive(Debug)]
pub enum FontReloadError {
    Io(IoError, PathBuf),
    FontNotFound(String),
}

impl Clone for ImageReloadError {
    fn clone(&self) -> Self {
        use self::FontReloadError::*;
        match self {
            Io(err, path) => Io(IoError::new(err.kind(), "Io Error"), path.clone()),
            FontNotFound(id) => FontNotFound(id.clone()),
        }
    }
}

impl fmt::Display for FontReloadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::FontReloadError::*;
        match self {
            Io(err, path_buf) => write!(f, "Could not load \"{}\" - IO error: {}", path_buf.as_path().to_string_lossy(), err),
            FontNotFound(id) => write!(f, "Could not locate system font: \"{}\" found", id),
        }
    }
}

impl ImageSource {
    /// Returns the bytes of the font
    pub(crate) fn get_bytes(&self) -> Result<Vec<u8>, ImageReloadError> {
        use std::fs;
        use self::ImageSource::*;
        match self {
            Embedded(bytes) => Ok(bytes.to_vec()),
            File(file_path) => fs::read(file_path).map_err(|e| ImageReloadError::Io(e, file_path.clone())),
        }
    }
}

impl FontSource {

    /// Returns the bytes of the font (loads the font from the system in case it is a `FontSource::System` font)
    pub(crate) fn get_bytes(&self) -> Result<Vec<u8>, FontReloadError> {
        use std::fs;
        use self::FontSource::*;
        match self {
            Embedded(bytes) => Ok(bytes.to_vec()),
            File(file_path) => fs::read(file_path).map_err(|e| FontReloadError::Io(e, file_path.clone())),
            System(id) => load_system_font(id).ok_or(FontReloadError::FontNotFound(id.clone())),
        }
    }
}

/// Raw image made up of raw pixels (either BRGA8 or A8)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawImage {
    pixels: Vec<u8>,
    image_dimensions: (u32, u32),
    data_format: RawImageFormat,
}

/// Stores the resources for the application, souch as fonts, images and cached
/// texts, also clipboard strings
///
/// Images and fonts can be references across window contexts (not yet tested,
/// but should work).
pub struct AppResources {
    /// The CssImageId is the string used in the CSS, i.e. "my_image" -> ImageId(4)
    pub(crate) css_ids_to_image_ids: FastHashMap<CssImageId, ImageId>,
    /// Stores where the images were loaded from
    pub(crate) images: FastHashMap<ImageId, ImageSource>,
    /// Raw images are the same as
    pub(crate) raw_images: FastHashMap<ImageId, RawImage>,
    /// All image keys currently active in the RenderApi
    pub(crate) currently_registered_images: FastHashMap<ImageId, ImageInfo>,
    /// Same as CssImageId -> ImageId, but for fonts, i.e. "Roboto" -> FontId(9)
    pub(crate) css_ids_to_font_ids: FastHashMap<CssFontId, FontId>,
    /// Stores where the fonts were loaded from
    pub(crate) fonts: FastHashMap<FontId, FontSource>,
    /// All font keys currently active in the RenderApi
    pub(crate) currently_registered_fonts: FastHashMap<FontId, (FontKey, FastHashMap<Au, FontInstanceKey>)>,
    /// If a font does not get used for one frame, the corresponding instance key gets
    /// deleted. If a FontId has no FontInstanceKeys anymore, the font key gets deleted.
    ///
    /// The only thing remaining in memory permanently is the FontSource (which is only
    /// the string of the file path where the font was loaded from, so no huge memory pressure).
    /// The reason for this agressive strategy is that the
    pub(crate) last_frame_font_keys: FastHashMap<FontId, (FontKey, FastHashMap<Au, FontInstanceKey>)>,
    /// Same thing for images: If the image isn't displayed, it is deleted from memory, only
    /// the `ImageSource` (i.e. the path / source where the image was loaded from) remains.
    ///
    /// This way the image can be re-loaded if necessary but doesn't have to reside in memory at all times.
    pub(crate) last_frame_image_keys: FastHashSet<ImageId>,
    /// Stores long texts across frames
    pub(crate) text_cache: TextCache,
    /// In order to properly load / unload fonts and images as well as share resources
    /// between windows, this field stores the (application-global) Renderer.
    pub(crate) fake_display: FakeDisplay,
    /// Keyboard clipboard storage and retrieval functionality
    clipboard: SystemClipboard,
}

impl AppResources {
    /// Creates a new renderer (the renderer manages the resources and is therfore tied to the resources).
    fn new(app_config: &AppConfig) -> Result<Self, WindowCreateError> {
        Ok(Self {
            css_ids_to_image_ids: FastHashMap::default(),
            images: FastHashMap::default(),
            raw_images: FastHashMap::default(),
            css_ids_to_font_ids: FastHashMap::default(),
            fonts: FastHashMap::default(),
            last_frame_font_keys: FastHashMap::default(),
            last_frame_image_keys: FastHashSet::default(),
            text_cache: TextCache::default(),
            fake_display: FakeDisplay::new(app_config.renderer_type, &app_config.debug_state, app_config.background_color)?,
            clipboard: SystemClipboard::new().unwrap(),
        })
    }
}

impl AppResources {

    /// Returns the IDs of all currently loaded fonts in `self.font_data`
    pub fn get_loaded_font_ids(&self) -> Vec<FontId> {
        self.font_data.borrow().keys().cloned().collect()
    }

    pub fn get_loaded_image_ids(&self) -> Vec<ImageId> {
        self.images.keys().cloned().collect()
    }

    pub fn get_loaded_css_image_ids(&self) -> Vec<CssImageId> {
        self.css_ids_to_image_ids.keys().cloned().collect()
    }

    pub fn get_loaded_css_font_ids(&self) -> Vec<CssFontId> {
        self.css_ids_to_font_ids.keys().cloned().collect()
    }

    pub fn get_loaded_text_ids(&self) -> Vec<TextId> {
        let mut text_ids = Vec::new();
        text_ids.extend(self.text_cache.string_cache.keys().cloned());
        text_ids.extend(self.text_cache.layouted_strings_cache.keys().cloned());
        text_ids
    }

    // -- ImageId cache

    /// Add an image from a PNG, JPEG or other - note that for specialized image formats,
    /// you have to enable them as features in the Cargo.toml file.
    ///
    /// ### Returns
    ///
    /// - `Some(())` if the image was inserted correctly
    /// - `None` if the ImageId already exists (you have to delete the image first using `.delete_image()`)
    #[cfg(feature = "image_loading")]
    pub fn add_image(&mut self, image_id: ImageId, image_source: ImageSource) -> Option<()> {
        match self.images.entry(image_id) {
            Occupied(_) => None,
            Vacant(v) => {
                v.insert(image_source);
                Some(())
            }
        }
    }

    /// Add raw image data (directly from a Vec<u8>) in BRGA8 or A8 format
    ///
    /// ### Returns
    ///
    /// - `Some(())` if the image was inserted correctly
    /// - `None` if the ImageId already exists (you have to delete the image first using `.delete_image()`)
    pub fn add_image_raw(&mut self, image_id: ImageId, image: RawImage) -> Option<()> {
        match self.raw_images.entry(image_id) {
            Occupied(_) => None,
            Vacant(v) => {
                v.insert(image);
                Some(())
            }
        }
    }

    /// See [`AppState::has_image()`](../app_state/struct.AppState.html#method.has_image)
    pub fn has_image(&self, image_id: &ImageId) -> bool {
        let has_image = self.images.get(image_id).is_some();
        let has_raw_image = self.raw_images.get(image_id).is_some();
        has_image || has_raw_image
    }

    pub fn delete_image(&mut self, image_id: ImageId) {
        self.images.delete(image_id);
        self.raw_images.delete(image_id);
    }

    pub fn add_css_image_id<S: Into<String>>(&mut self, css_id: S) -> ImageId {
        *self.css_ids_to_image_ids.entry(css_id.into()).or_insert_with(|| ImageId::new())
    }

    pub(crate) fn get_image_info(key: &ImageId) -> Option<ImageInfo> {
        self.currently_registered_images.get(key)
    }

    pub fn has_css_image_id<S: AsRef<str>>(&self, css_id: S) -> bool {
        self.get_css_image_id(css_id).is_some()
    }

    /// Returns the ImageId for a given CSS ID - the CSS ID is what you added your image as:
    ///
    /// ```no_run,ignore
    /// let image_id = app_resources.add_image("test", ImageSource::Embedded(include_bytes!("./my_image.ttf")));
    /// ```
    pub fn get_css_image_id<S: AsRef<str>>(&self, css_id: S) -> Option<ImageId> {
        self.css_ids_to_image_ids.get(css_id.as_ref()).cloned()
    }

    pub fn delete_css_image_id<S: AsRef<str>>(&mut self, css_id: S) -> Option<ImageId> {
        self.css_ids_to_image_ids.remove(css_id.as_ref())
    }

    // -- FontId cache

    pub fn add_font<I: Into<Vec<u8>>>(&mut self, font_id: FontId, font_source: FontSource) -> Option<()> {
        match self.fonts.entry(font_id) {
            Occupied(_) => None,
            Vacant(v) => {
                v.insert(font_source);
                Some(())
            }
        }
    }

    /// Given a `FontId`, returns the bytes for that font or `None`, if the `FontId` is invalid.
    pub fn get_font_bytes(&self, font_id: &FontId) -> Option<Result<Vec<u8>, FontReloadError>> {
        let font_source = self.fonts.get(font_id)?;
        Some(font_source.get_bytes())
    }

    /// Checks if a `FontId` is valid, i.e. if a font is currently ready-to-use
    pub fn has_font(&self, id: &FontId) -> bool {
        self.fonts.borrow().get(id).is_some()
    }

    pub fn delete_font(&mut self, id: &FontId) {
        self.fonts.delete(id);
    }

    /// Returns the `(FontKey, FontInstance)` - convenience function for the display list, to
    /// query fonts and font keys from the display list
    pub(crate) fn get_font_instance<I: Into<Au>>(&self, font_id: &FontId, font_size: I) -> Option<(FontKey, FontInstanceKey)> {
        let au = font_size.into();
        self.currently_registered_fonts.get(font_id).and_then(|(font_key, font_instances)| {
            font_instances.get(&au).map(|font_instance_key| (font_key, font_instance_key))
        })
    }

    // -- TextId cache

    /// Adds a string to the internal text cache, but only store it as a string,
    /// without caching the layout of the string.
    pub fn add_text(&mut self, text: &str) -> TextId {
        self.text_cache.add_text(text)
    }

    /// Removes a string from both the string cache and the layouted text cache
    pub fn delete_text(&mut self, id: TextId) {
        self.text_cache.delete_text(id);
    }

    /// Empties the entire internal text cache, invalidating all `TextId`s. Use with care.
    pub fn clear_all_texts(&mut self) {
        self.text_cache.clear_all_texts();
    }

    // -- Clipboard

    /// Returns the contents of the system clipboard
    pub fn get_clipboard_string(&self) -> Result<String, ClipboardError> {
        self.clipboard.get_string_contents()
    }

    /// Sets the contents of the system clipboard - currently only strings are supported
    pub fn set_clipboard_string<S: Into<String>>(&mut self, contents: S) -> Result<(), ClipboardError> {
        self.clipboard.set_string_contents(contents.into())
    }
}

#[cfg(feature = "image_loading")]
fn decode_image_data<I: Into<Vec<u8>>>(image_data: I)
-> Result<(ImageData, ImageDescriptor), ImageError>
{
    use image; // the crate
    use images; // the module

    let image_data = image_data.into();
    let image_format = image::guess_format(&image_data)?;
    let decoded = image::load_from_memory_with_format(&image_data, image_format)?;
    Ok(images::prepare_image(decoded)?)
}

/// Scans the styled UI for all font IDs + their font size
fn scan_ui_description_for_font_keys<T: Layout>(input: &UiDescription<T>) -> FastHashMap<FontId, FastHashSet<Au>> {

}

/// Scans the styled UI for all font IDs + their font size
fn scan_ui_description_for_image_keys<T: Layout>(input: &UiDescription<T>) -> FastHashMap<FontId, FastHashSet<Au>> {

}

/// Looks if any new images need to be uploaded and stores the in the image resources
fn update_resources(
    api: &RenderApi,
    app_resources: &mut AppResources,
    resource_updates: &mut Vec<ResourceUpdate>)
{
    update_image_resources(api, app_resources, resource_updates);
    update_font_resources(api, app_resources, resource_updates);
}

fn update_image_resources(
    api: &RenderApi,
    app_resources: &mut AppResources,
    resource_updates: &mut Vec<ResourceUpdate>)
{
    use images::{ImageState, ImageInfo};

    let mut updated_images = Vec::<(ImageId, (ImageData, ImageDescriptor))>::new();
    let mut to_delete_images = Vec::<(ImageId, Option<ImageKey>)>::new();

    // possible performance bottleneck (duplicated cloning) !!
    for (key, value) in app_resources.images.iter() {
        match *value {
            ImageState::ReadyForUpload(ref d) => {
                updated_images.push((key.clone(), d.clone()));
            },
            ImageState::Uploaded(_) => { },
            ImageState::AboutToBeDeleted((ref k, _)) => {
                to_delete_images.push((key.clone(), k.clone()));
            }
        }
    }

    // Remove any images that should be deleted
    for (resource_key, image_key) in to_delete_images.into_iter() {
        if let Some(image_key) = image_key {
            resource_updates.push(ResourceUpdate::DeleteImage(image_key));
        }
        app_resources.images.remove(&resource_key);
    }

    // Upload all remaining images to the GPU only if the haven't been
    // uploaded yet
    for (resource_key, (data, descriptor)) in updated_images.into_iter() {

        let key = api.generate_image_key();
        resource_updates.push(ResourceUpdate::AddImage(
            AddImage { key, descriptor, data, tiling: None }
        ));

        *app_resources.images.get_mut(&resource_key).unwrap() =
            ImageState::Uploaded(ImageInfo {
                key: key,
                descriptor: descriptor
        });
    }
}

// almost the same as update_image_resources, but fonts
// have two HashMaps that need to be updated
fn update_font_resources(
    api: &RenderApi,
    app_resources: &mut AppResources,
    resource_updates: &mut Vec<ResourceUpdate>
) {
    use font::FontState;
    use azul_css::FontId;

    let mut updated_fonts = Vec::<(FontId, Vec<u8>)>::new();
    let mut to_delete_fonts = Vec::<(FontId, Option<(FontKey, Vec<FontInstanceKey>)>)>::new();

    for (key, value) in app_resources.font_data.borrow().iter() {
        match &*(*value.2).borrow() {
            FontState::ReadyForUpload(ref bytes) => {
                updated_fonts.push((key.clone(), bytes.clone()));
            },
            FontState::Uploaded(_) => { },
            FontState::AboutToBeDeleted(ref font_key) => {
                let to_delete_font_instances = font_key.and_then(|f_key| {
                    let to_delete_font_instances = app_resources.fonts[&f_key].values().cloned().collect();
                    Some((f_key.clone(), to_delete_font_instances))
                });
                to_delete_fonts.push((key.clone(), to_delete_font_instances));
            }
        }
    }

    // Delete the complete font. Maybe a more granular option to
    // keep the font data in memory should be added later
    for (resource_key, to_delete_instances) in to_delete_fonts.into_iter() {
        if let Some((font_key, font_instance_keys)) = to_delete_instances {
            for instance in font_instance_keys {
                resource_updates.push(ResourceUpdate::DeleteFontInstance(instance));
            }
            resource_updates.push(ResourceUpdate::DeleteFont(font_key));
            app_resources.fonts.remove(&font_key);
        }
        app_resources.font_data.borrow_mut().remove(&resource_key);
    }

    // Upload all remaining fonts to the GPU only if the haven't been uploaded yet
    for (resource_key, data) in updated_fonts.into_iter() {
        let key = api.generate_font_key();
        resource_updates.push(ResourceUpdate::AddFont(AddFont::Raw(key, data, 0))); // TODO: use the index better?
        let mut borrow_mut = app_resources.font_data.borrow_mut();
        *borrow_mut.get_mut(&resource_key).unwrap().2.borrow_mut() = FontState::Uploaded(key);
    }
}

fn load_system_font(id: &str) -> Option<Vec<u8>> {
    use font_loader::system_fonts::{self, FontPropertyBuilder};

    let font_builder = match &id {
        "monospace" => {
            #[cfg(target_os = "linux")] {
                let native_monospace_font = linux_get_native_font(LinuxNativeFontType::Monospace);
                FontPropertyBuilder::new().family(&native_monospace_font)
            }
            #[cfg(not(target_os = "linux"))] {
                FontPropertyBuilder::new().monospace()
            }
        },
        "fantasy" => FontPropertyBuilder::new().oblique(),
        "sans-serif" => {
            #[cfg(target_os = "mac_os")] {
                // For some reason, this selects Helvetica
                FontPropertyBuilder::new().family("Arial")
            }
            #[cfg(target_os = "linux")] {
                let native_sans_serif_font = linux_get_native_font(LinuxNativeFontType::SansSerif);
                FontPropertyBuilder::new().family(&native_sans_serif_font)
            }
            #[cfg(all(not(target_os = "linux"), not(target_os = "mac_os")))] {
                FontPropertyBuilder::new().family("sans-serif")
            }
        },
        "serif" => {
            FontPropertyBuilder::new().family("Times New Roman")
        },
        other => FontPropertyBuilder::new().family(other)
    };

    system_fonts::get(&font_builder)
}

/// Return the native fonts
#[cfg(target_os = "linux")]
enum LinuxNativeFontType { SansSerif, Monospace }

#[cfg(target_os = "linux")]
fn linux_get_native_font(font_type: LinuxNativeFontType) -> String {

    use std::env;
    use std::process::Command;
    use self::LinuxNativeFontType::*;

    let font_name = match font_type {
        SansSerif => "font-name",
        Monospace => "monospace-font-name",
    };

    let fallback_font_name = match font_type {
        SansSerif => "Ubuntu",
        Monospace => "Ubuntu Mono",
    };

    // Execute "gsettings get org.gnome.desktop.interface font-name" and parse the output
    let gsetting_cmd_result =
        Command::new("gsettings")
            .arg("get")
            .arg("org.gnome.desktop.interface")
            .arg(font_name)
            .output()
            .ok().map(|output| output.stdout)
            .and_then(|stdout_bytes| String::from_utf8(stdout_bytes).ok())
            .map(|stdout_string| stdout_string.lines().collect::<String>());

    match gsetting_cmd_result {
        Some(s) => parse_gsettings_font(s).to_string(),
        None => fallback_font_name.to_string(),
    }
}

// 'Ubuntu Mono 13' => Ubuntu Mono
#[cfg(target_os = "linux")]
fn parse_gsettings_font(input: &str) -> &str {
    use std::char;
    let input = input.trim();
    let input = input.trim_matches('\'');
    let input = input.trim_right_matches(char::is_numeric);
    let input = input.trim();
    input
}

#[test]
#[cfg(target_os = "linux")]
fn test_parse_gsettings_font() {
    assert_eq!(parse_gsettings_font("'Ubuntu 11'"), "Ubuntu");
    assert_eq!(parse_gsettings_font("'Ubuntu Mono 13'"), "Ubuntu Mono");
}