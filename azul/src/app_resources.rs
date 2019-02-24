use std::{
    path::PathBuf,
    io::Error as IoError,
    sync::atomic::{AtomicUsize, Ordering},
    collections::hash_map::Entry::*,
};
use webrender::api::{
    FontKey, ImageData, ImageDescriptor, FontInstanceKey,
    ResourceUpdate, AddImage, AddFont, AddFontInstance,
};
pub use webrender::api::ImageFormat as RawImageFormat;
#[cfg(feature = "image_loading")]
use image::ImageError;
use FastHashMap;
use app_units::Au;
use clipboard2::{Clipboard, ClipboardError, SystemClipboard};
use {
    FastHashSet,
    images::ImageInfo,
    text_cache::{TextId, TextCache},
    window::{FakeDisplay, WindowCreateError},
    app::AppConfig,
    traits::Layout,
    display_list::DisplayList,
    text_layout::Words,
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

impl_display!(ImageReloadError, {
    Io(err, path_buf) => format!("Could not load \"{}\" - IO error: {}", path_buf.as_path().to_string_lossy(), err),
});

#[derive(Debug)]
pub enum FontReloadError {
    Io(IoError, PathBuf),
    FontNotFound(String),
}

impl Clone for FontReloadError {
    fn clone(&self) -> Self {
        use self::FontReloadError::*;
        match self {
            Io(err, path) => Io(IoError::new(err.kind(), "Io Error"), path.clone()),
            FontNotFound(id) => FontNotFound(id.clone()),
        }
    }
}

impl_display!(FontReloadError, {
    Io(err, path_buf) => format!("Could not load \"{}\" - IO error: {}", path_buf.as_path().to_string_lossy(), err),
    FontNotFound(id) => format!("Could not locate system font: \"{}\" found", id),
});

impl ImageSource {
    /// Returns the bytes of the image - note that the descriptor might be missing
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

    /// Returns the bytes of the font (loads the font from the system in case it is a `FontSource::System` font).
    /// Also returns the index into the font (in case the font is a font collection).
    pub fn get_bytes(&self) -> Result<(Vec<u8>, i32), FontReloadError> {
        use std::fs;
        use self::FontSource::*;
        match self {
            Embedded(bytes) => Ok((bytes.to_vec(), 0)),
            File(file_path) => {
                fs::read(file_path)
                .map_err(|e| FontReloadError::Io(e, file_path.clone()))
                .map(|f| (f, 0))
            },
            System(id) => load_system_font(id).ok_or(FontReloadError::FontNotFound(id.clone())),
        }
    }
}

/// Raw image made up of raw pixels (either BRGA8 or A8)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawImage {
    pub pixels: Vec<u8>,
    pub image_dimensions: (u32, u32),
    pub data_format: RawImageFormat,
}

/// Stores the resources for the application, souch as fonts, images and cached
/// texts, also clipboard strings
///
/// Images and fonts can be references across window contexts (not yet tested,
/// but should work).
pub struct AppResources {
    /// The CssImageId is the string used in the CSS, i.e. "my_image" -> ImageId(4)
    pub(crate) css_ids_to_image_ids: FastHashMap<CssImageId, ImageId>,
    /// Same as CssImageId -> ImageId, but for fonts, i.e. "Roboto" -> FontId(9)
    pub(crate) css_ids_to_font_ids: FastHashMap<CssFontId, FontId>,
    /// Stores where the images were loaded from
    pub(crate) images: FastHashMap<ImageId, ImageSource>,
    /// Raw images are the same as regular images, but not in PNG or JPEG format, but rather as raw bytes
    pub(crate) raw_images: FastHashMap<ImageId, RawImage>,
    /// Stores where the fonts were loaded from
    pub(crate) fonts: FastHashMap<FontId, FontSource>,
    /// All image keys currently active in the RenderApi
    pub(crate) currently_registered_images: FastHashMap<ImageId, ImageInfo>,
    /// All font keys currently active in the RenderApi
    pub(crate) currently_registered_fonts: FastHashMap<FontId, (FontKey, FastHashMap<Au, FontInstanceKey>)>,
    /// If an image isn't displayed, it is deleted from memory, only
    /// the `ImageSource` (i.e. the path / source where the image was loaded from) remains.
    ///
    /// This way the image can be re-loaded if necessary but doesn't have to reside in memory at all times.
    pub(crate) last_frame_image_keys: FastHashMap<ImageId, ImageInfo>,
    /// If a font does not get used for one frame, the corresponding instance key gets
    /// deleted. If a FontId has no FontInstanceKeys anymore, the font key gets deleted.
    ///
    /// The only thing remaining in memory permanently is the FontSource (which is only
    /// the string of the file path where the font was loaded from, so no huge memory pressure).
    /// The reason for this agressive strategy is that the
    pub(crate) last_frame_font_keys: FastHashMap<FontId, (FontKey, FastHashMap<Au, FontInstanceKey>)>,
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
    pub(crate) fn new(app_config: &AppConfig) -> Result<Self, WindowCreateError> {
        Ok(Self {
            css_ids_to_image_ids: FastHashMap::default(),
            css_ids_to_font_ids: FastHashMap::default(),
            images: FastHashMap::default(),
            raw_images: FastHashMap::default(),
            fonts: FastHashMap::default(),
            currently_registered_images: FastHashMap::default(),
            currently_registered_fonts: FastHashMap::default(),
            last_frame_image_keys: FastHashMap::default(),
            last_frame_font_keys: FastHashMap::default(),
            text_cache: TextCache::default(),
            fake_display: FakeDisplay::new(app_config.renderer_type, &app_config.debug_state, app_config.background_color)?,
            clipboard: SystemClipboard::new().unwrap(),
        })
    }
}

impl AppResources {

    /// Returns the IDs of all currently loaded fonts in `self.font_data`
    pub fn get_loaded_font_ids(&self) -> Vec<FontId> {
        self.fonts.keys().cloned().collect()
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
        self.text_cache.string_cache.keys().cloned().collect()
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

    /// Returns whether the AppResources has currently a certain image ID registered
    pub fn has_image(&self, image_id: &ImageId) -> bool {
        let has_image = self.images.get(image_id).is_some();
        let has_raw_image = self.raw_images.get(image_id).is_some();
        has_image || has_raw_image
    }

    /// Given an `ImageId`, returns the bytes for that image or `None`, if the `ImageId` is invalid.
    pub fn get_image_bytes(&self, image_id: &ImageId) -> Option<Result<Vec<u8>, ImageReloadError>> {
        match self.images.get(image_id) {
            Some(image_source) => Some(image_source.get_bytes()),
            None => self.raw_images.get(image_id).map(|raw_img| Ok(raw_img.pixels.clone()))
        }
    }

    pub fn delete_image(&mut self, image_id: &ImageId) {
        self.images.remove(image_id);
        self.raw_images.remove(image_id);
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

    pub fn add_font(&mut self, font_id: FontId, font_source: FontSource) -> Option<()> {
        match self.fonts.entry(font_id) {
            Occupied(_) => None,
            Vacant(v) => {
                v.insert(font_source);
                Some(())
            }
        }
    }

    /// Given a `FontId`, returns the bytes for that font or `None`, if the `FontId` is invalid.
    pub fn get_font_bytes(&self, font_id: &FontId) -> Option<Result<(Vec<u8>, i32), FontReloadError>> {
        let font_source = self.fonts.get(font_id)?;
        Some(font_source.get_bytes())
    }

    /// Checks if a `FontId` is valid, i.e. if a font is currently ready-to-use
    pub fn has_font(&self, id: &FontId) -> bool {
        self.fonts.get(id).is_some()
    }

    pub fn delete_font(&mut self, id: &FontId) {
        self.fonts.remove(id);
    }

    /// Returns the `(FontKey, FontInstance)` - convenience function for the display list, to
    /// query fonts and font keys from the display list
    pub(crate) fn get_font_instance<I: Into<Au>>(&self, font_id: &FontId, font_size: I) -> Option<(FontKey, FontInstanceKey)> {
        let au = font_size.into();
        self.currently_registered_fonts.get(font_id).and_then(|(font_key, font_instances)| {
            font_instances.get(&au).map(|font_instance_key| (*font_key, *font_instance_key))
        })
    }

    // -- TextId cache

    /// Adds a string to the internal text cache, but only store it as a string,
    /// without caching the layout of the string.
    pub fn add_text(&mut self, text: &str) -> TextId {
        self.text_cache.add_text(text)
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

/// Scans the display list for all font IDs + their font size
fn scan_ui_description_for_font_keys<'a, T: Layout>(
    app_resources: &AppResources,
    display_list: &DisplayList<'a, T>
) -> FastHashMap<FontId, FastHashSet<Au>>
{
    use dom::NodeType::*;
    use ui_solver;

    let mut font_keys = FastHashMap::default();

    for node_id in display_list.rectangles.linear_iter() {

        let node_data = &display_list.ui_descr.ui_descr_arena.node_data[node_id];
        let display_rect = &display_list.rectangles[node_id];

        match node_data.node_type {
            Text(_) | Label(_) => {
                let css_font_id = ui_solver::get_font_id(&display_rect.style);
                let font_id = match app_resources.css_ids_to_font_ids.get(css_font_id) {
                    Some(s) => s,
                    None => continue,
                };
                let font_size = ui_solver::get_font_size(&display_rect.style);
                font_keys
                    .entry(*font_id)
                    .or_insert_with(|| FastHashSet::default())
                    .insert(ui_solver::font_size_to_au(font_size));
            },
            _ => { }
        }
    }

    font_keys
}

/// Scans the display list for all image keys
fn scan_ui_description_for_image_keys<'a, T: Layout>(
    app_resources: &AppResources,
    display_list: &DisplayList<'a, T>
) -> FastHashSet<ImageId>
{
    use dom::NodeType::*;

    display_list.rectangles
    .iter()
    .zip(display_list.ui_descr.ui_descr_arena.node_data.iter())
    .filter_map(|(display_rect, node_data)| {
        match node_data.node_type {
            Image(id) => Some(id),
            _ => {
                let css_image_id = display_rect.style.background?.get_css_image_id()?;
                let image_id = app_resources.get_css_image_id(&css_image_id.0)?;
                Some(*image_id)
            }
        }
    }).collect()
}

/// Given the fonts of the current frame, returns `AddFont` and `AddFontInstance`s of
/// which fonts / instances are currently not in the `current_registered_fonts` and
/// need to be added. Modifies `last_frame_font_keys` to contain the added font keys.
///
/// Deleting fonts can only be done after the entire frame has finished drawing,
/// otherwise (if removing fonts would happen after every DOM) we'd constantly
/// add-and-remove fonts after every IFrameCallback, which would cause a lot of
/// I/O waiting.
fn build_add_font_resource_updates(
    app_resources: &mut AppResources,
    current_used_fonts: &FastHashMap<FontId, FastHashSet<Au>>,
) -> Vec<ResourceUpdate> {

    let mut resource_updates = Vec::new();

    for (font_id, font_sizes) in current_used_fonts {

        macro_rules! insert_font_instances {($font_id:expr, $font_key:expr, $font_size:expr) => ({
            let font_instance_key = app_resources.fake_display.render_api.generate_font_instance_key();

            app_resources.last_frame_font_keys
                .entry($font_id)
                .or_insert_with(|| ($font_key, FastHashMap::new())).1
                .insert($font_size, font_instance_key);

            resource_updates.push(ResourceUpdate::AddFontInstance(AddFontInstance {
                key: font_instance_key,
                font_key: $font_key,
                glyph_size: $font_size,
                options: None, // TODO: LCD options
                platform_options: None,
                variations: Vec::new(),
            }));
        })}

        match app_resources.currently_registered_fonts.get(font_id) {
            Some((font_key, existing_font_instances)) => {
                for font_size in font_sizes.iter().filter(|s| !existing_font_instances.contains_key(s)) {
                    insert_font_instances!(*font_id, *font_key, *font_size);
                }
            },
            None => {
                // If there is no font key, that means there's also no font instances
                let font_source = match app_resources.fonts.get(font_id) {
                    Some(s) => s,
                    None => continue,
                };

                let font_bytes = match font_source.get_bytes() {
                    Ok(o) => o,
                    Err(e) => {
                        #[cfg(feature = "logging")] {
                            warn!("Could not load font with ID: {:?} - error: {}", font_id, e);
                        }
                        continue;
                    }
                };

                let font_key = app_resources.fake_display.render_api.generate_font_key();
                app_resources.last_frame_font_keys.entry(*font_id).or_insert_with(|| (font_key, FastHashMap::new()));
                resource_updates.push(ResourceUpdate::AddFont(AddFont::Raw(font_key, font_bytes.0, font_bytes.1 as u32)));

                for font_size in font_sizes {
                    insert_font_instances!(*font_id, font_key, *font_size);
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
fn build_add_image_resource_update(
    app_resources: &mut AppResources,
    current_used_images: &FastHashSet<ImageId>,
) -> Vec<ResourceUpdate> {

    use images::is_image_opaque;

    let mut resource_updates = Vec::new();

    for image_id in current_used_images.iter().filter(|id| {
        !app_resources.currently_registered_images.contains_key(id)
    }) {
        match app_resources.images.get(image_id) {
            Some(source) => {
                #[cfg(feature = "image_loading")] {
                    let image_bytes = match source.get_bytes() {
                        Ok(o) => o,
                        Err(e) => {
                            #[cfg(feature = "logging")] {
                                warn!("Could not load image with ID: {} - error: {}", font_id, e);
                            }
                            continue;
                        }
                    };

                    let (decoded_image_data, image_descriptor) = match decode_image_data() {
                        Ok(o) => o,
                        Err(e) => {
                            #[cfg(feature = "logging")] {
                                warn!("Could not decode image with ID: {} - error: {}", font_id, e);
                            }
                            continue;
                        }
                    };

                    let image_key = api.generate_image_key();
                    app_resources.last_frame_image_keys.insert(image_id, ImageInfo {
                        key: image_key,
                        descriptor: image_descriptor,
                    });

                    resource_updates.push(ResourceUpdate::AddImage(
                        AddImage { key: image_key, descriptor, data: decoded_image_data, tiling: None }
                    ));
                }
            },
            None => {
                // Image is not a normal image, but may be a raw image
                match app_resources.raw_images.remove(image_id) {
                    Some(RawImage { pixels, image_dimensions, data_format }) => {
                        let opaque = is_image_opaque(data_format, &pixels[..]);
                        let allow_mipmaps = true;
                        let descriptor = ImageDescriptor::new(
                            image_dimensions.0 as i32,
                            image_dimensions.1 as i32,
                            data_format,
                            opaque,
                            allow_mipmaps
                        );
                        let data = ImageData::new(pixels);
                        let render_api = &app_resources.fake_display.render_api;
                        let image_key = render_api.generate_image_key();

                        app_resources.last_frame_image_keys.insert(*image_id, ImageInfo {
                            key: image_key,
                            descriptor: descriptor
                        });

                        resource_updates.push(ResourceUpdate::AddImage(
                            AddImage { key: image_key, descriptor, data, tiling: None }
                        ));
                    },
                    None => { }, // invalid image ID
                }
            }
        }
    }

    resource_updates
}

/// Submits the `AddFont`, `AddFontInstance` and `AddImage` resources to the RenderApi.
/// Extends `currently_registered_images` and `currently_registered_fonts` by the
/// `last_frame_image_keys` and `last_frame_font_keys`, so that we don't lose track of
/// what font and image keys are currently in the API.
fn add_resources(
    app_resources: &mut AppResources,
    add_font_resources: Vec<ResourceUpdate>,
    add_image_resources: Vec<ResourceUpdate>,
) {
    let mut merged_resource_updates = add_font_resources;
    merged_resource_updates.extend(add_image_resources.into_iter());
    app_resources.fake_display.render_api.update_resources(merged_resource_updates);

    for (image_id, image_info) in app_resources.last_frame_image_keys {
        app_resources.currently_registered_images.insert(image_id, image_info);
    }

    for (font_id, (font_key, font_instances)) in app_resources.last_frame_font_keys {
        app_resources.currently_registered_fonts
            .entry(font_id)
            .or_insert_with(|| (font_key, FastHashMap::default())).1
            .extend(font_instances.clone().into_iter());
    }
}

fn build_delete_font_resource_updates(
    app_resources: &mut AppResources
) -> Vec<ResourceUpdate> {

    let mut to_remove_fonts = Vec::new();
    let mut to_remove_font_instance_keys = Vec::new();

    // Delete fonts that were not used in the last frame or have zero font instances
    for (font_id, (font_key, font_instances)) in &app_resources.currently_registered_fonts {
        if !app_resources.last_frame_font_keys.contains_key(&font_id) || font_instances.is_empty() {
            to_remove_fonts.push((*font_id, *font_key));
            for (au, font_instance_key) in font_instances.iter() {
                to_remove_font_instance_keys.push((*font_id, *au, *font_instance_key));
            }
        } else {
            for (au, font_instance_key) in font_instances.iter() {
                if !app_resources.last_frame_font_keys[font_id].1.contains_key(au) {
                    to_remove_font_instance_keys.push((*font_id, *au, *font_instance_key));
                }
            }
        }
    }

    let mut resource_updates = Vec::new();

    for (font_id, font_key) in to_remove_fonts {
        resource_updates.push(ResourceUpdate::DeleteFont(font_key));
        app_resources.currently_registered_fonts.remove(&font_id);
    }

    for (font_id, au, font_instance_key) in to_remove_font_instance_keys {
        resource_updates.push(ResourceUpdate::DeleteFontInstance(font_instance_key));
        app_resources.currently_registered_fonts[&font_id].1.remove(&au);
    }

    resource_updates
}

/// At the end of the frame, all images that are registered, but weren't used in the last frame
fn build_delete_image_resource_updates(
    app_resources: &mut AppResources
) -> Vec<ResourceUpdate> {

    let to_remove_image_keys = app_resources.currently_registered_images.iter().filter(|(id, info)| {
        !app_resources.last_frame_image_keys.contains_key(id)
    }).map(|(id, info)| (*id, *info)).collect::<Vec<(ImageId, ImageInfo)>>();

    let resource_updates = to_remove_image_keys.iter().map(|(_removed_id, removed_info)| {
        ResourceUpdate::DeleteImage(removed_info.key)
    }).collect();

    for (removed_id, _removed_info) in to_remove_image_keys {
        app_resources.currently_registered_images.remove(&removed_id);
        app_resources.raw_images.remove(&removed_id);
    }

    resource_updates
}

/// Clears the `last_frame_image_keys` and `last_frame_font_keys` fields
/// (usually invoked at the start of a new frame).
fn clear_last_frame_images_and_fonts(app_resources: &mut AppResources) {
    app_resources.last_frame_image_keys = FastHashMap::default();
    app_resources.last_frame_font_keys = FastHashMap::default();
}

fn delete_resources(
    app_resources: &mut AppResources,
    mut delete_font_resources: Vec<ResourceUpdate>,
    mut delete_image_resources: Vec<ResourceUpdate>,
) {
    let render_api = &app_resources.fake_display.render_api;
    delete_font_resources.append(&mut delete_image_resources);
    render_api.update_resources(delete_font_resources);
    // Assure that the add / remove fonts get processed immediately
    render_api.flush_scene_builder();
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

/// Returns the font + the index of the font (in case the font is a collection)
fn load_system_font(id: &str) -> Option<(Vec<u8>, i32)> {
    use font_loader::system_fonts::{self, FontPropertyBuilder};

    let font_builder = match id {
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

    system_fonts::get(&font_builder.build())
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