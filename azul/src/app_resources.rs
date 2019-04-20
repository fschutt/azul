use std::{
    fmt,
    path::PathBuf,
    io::Error as IoError,
};
use webrender::api::{
    AddImage, ResourceUpdate, AddFont, AddFontInstance, RenderApi,
};
use {
    FastHashMap, FastHashSet,
    display_list::DisplayList,
};
pub use webrender::api::{
    ImageFormat as WrImageFormat,
    ImageData as WrImageData,
    ImageDescriptor as WrImageDescriptor
};
#[cfg(feature = "image_loading")]
pub use image::{ImageError, DynamicImage, GenericImageView};
pub use azul_core::app_resources::{
    AppResources, Au, ImmediateFontId, LoadedFont, RawImageFormat,
    FontKey, FontInstanceKey, ImageKey, ImageSource, FontSource,
    RawImage, CssFontId, CssImageId, TextCache, TextId, ImageId, FontId,
    ImageInfo,
};

#[derive(Debug)]
pub enum ImageReloadError {
    Io(IoError, PathBuf),
    #[cfg(feature = "image_loading")]
    DecodingError(ImageError),
    #[cfg(not(feature = "image_loading"))]
    DecodingModuleNotActive,
}

impl Clone for ImageReloadError {
    fn clone(&self) -> Self {
        use self::ImageReloadError::*;
        match self {
            Io(err, path) => Io(IoError::new(err.kind(), "Io Error"), path.clone()),
            #[cfg(feature = "image_loading")]
            DecodingError(e) => DecodingError(e.clone()),
            #[cfg(not(feature = "image_loading"))]
            DecodingModuleNotActive => DecodingModuleNotActive,
        }
    }
}

impl fmt::Display for ImageReloadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::ImageReloadError::*;
        match &self {
            Io(err, path_buf) => write!(f, "Could not load \"{}\" - IO error: {}", path_buf.as_path().to_string_lossy(), err),
            #[cfg(feature = "image_loading")]
            DecodingError(err) => write!(f, "Image decoding error: \"{}\"", err),
            #[cfg(not(feature = "image_loading"))]
            DecodingModuleNotActive => write!(f, "Found decoded image, but crate was not compiled with --features=\"image_loading\""),
        }
    }
}

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

/// Used only for debugging, so that the AppResource garbage
/// collection tests can run without a real RenderApi
#[cfg(test)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct FakeRenderApi { }

#[cfg(test)]
impl FakeRenderApi { fn new() -> Self { Self { } } }

pub(crate) trait FontImageApi {
    fn new_image_key(&self) -> ImageKey;
    fn new_font_key(&self) -> FontKey;
    fn new_font_instance_key(&self) -> FontInstanceKey;
    fn update_resources(&self, Vec<ResourceUpdate>);
    fn flush_scene_builder(&self);
}

impl FontImageApi for RenderApi {
    fn new_image_key(&self) -> ImageKey { self.generate_image_key() }
    fn new_font_key(&self) -> FontKey { self.generate_font_key() }
    fn new_font_instance_key(&self) -> FontInstanceKey { self.generate_font_instance_key() }
    fn update_resources(&self, updates: Vec<ResourceUpdate>) { self.update_resources(updates); }
    fn flush_scene_builder(&self) { self.flush_scene_builder(); }
}

#[cfg(test)]
use webrender::api::IdNamespace;

// Fake RenderApi for unit testing
#[cfg(test)]
impl FontImageApi for FakeRenderApi {
    fn new_image_key(&self) -> ImageKey { ImageKey::DUMMY }
    fn new_font_key(&self) -> FontKey { FontKey::new(IdNamespace(0), 0) }
    fn new_font_instance_key(&self) -> FontInstanceKey { FontInstanceKey::new(IdNamespace(0), 0) }
    fn update_resources(&self, _: Vec<ResourceUpdate>) { }
    fn flush_scene_builder(&self) { }
}

/// Scans the DisplayList for new images and fonts. After this call, the RenderApi is
/// guaranteed to know about all FontKeys and FontInstanceKey
pub(crate) fn add_fonts_and_images<T, U: FontImageApi>(
    app_resources: &mut AppResources,
    render_api: &mut U,
    display_list: &DisplayList<T>
) {
    let font_keys = scan_ui_description_for_font_keys(&app_resources, display_list);
    let image_keys = scan_ui_description_for_image_keys(&app_resources, display_list);

    app_resources.last_frame_font_keys.extend(font_keys.clone().into_iter());
    app_resources.last_frame_image_keys.extend(image_keys.clone().into_iter());

    let add_font_resource_updates = build_add_font_resource_updates(app_resources, render_api, &font_keys);
    let add_image_resource_updates = build_add_image_resource_updates(app_resources, render_api, &image_keys);

    add_resources(app_resources, add_font_resource_updates, render_api, add_image_resource_updates);
}

/// To be called at the end of a frame (after the UI has rendered):
/// Deletes all FontKeys and FontImageKeys that weren't used in
/// the last frame, to save on memory. If the font needs to be recreated, it
/// needs to be reloaded from the `FontSource`.
pub(crate) fn garbage_collect_fonts_and_images<U: FontImageApi>(
    app_resources: &mut AppResources,
    render_api: &mut U,
) {
    let delete_font_resource_updates = build_delete_font_resource_updates(app_resources);
    let delete_image_resource_updates = build_delete_image_resource_updates(app_resources);

    delete_resources(app_resources, render_api, delete_font_resource_updates, delete_image_resource_updates);

    app_resources.last_frame_font_keys.clear();
    app_resources.last_frame_image_keys.clear();
}

/// Returns the **decoded** bytes of the image + the descriptor (contains width / height).
/// Returns an error if the data is encoded, but the crate wasn't built with `--features="image_loading"`
#[allow(unused_variables)]
fn image_source_get_bytes(image_source: &ImageSource)
-> Result<(WrImageData, WrImageDescriptor), ImageReloadError>
{
    match image_source {
        ImageSource::Embedded(bytes) => {
            #[cfg(feature = "image_loading")] {
                decode_image_data(bytes.to_vec()).map_err(|e| ImageReloadError::DecodingError(e))
            }
            #[cfg(not(feature = "image_loading"))] {
                Err(ImageReloadError::DecodingModuleNotActive)
            }
        },
        ImageSource::Raw(raw_image) => {
            let opaque = is_image_opaque(raw_image.data_format, &raw_image.pixels[..]);
            let allow_mipmaps = true;
            let descriptor = WrImageDescriptor::new(
                raw_image.image_dimensions.0 as i32,
                raw_image.image_dimensions.1 as i32,
                raw_image.data_format,
                opaque,
                allow_mipmaps
            );
            let data = WrImageData::new(raw_image.pixels.clone());
            Ok((data, descriptor))
        },
        ImageSource::File(file_path) => {
            #[cfg(feature = "image_loading")] {
                use std::fs;
                let bytes = fs::read(file_path).map_err(|e| ImageReloadError::Io(e, file_path.clone()))?;
                decode_image_data(bytes).map_err(|e| ImageReloadError::DecodingError(e))
            }
            #[cfg(not(feature = "image_loading"))] {
                Err(ImageReloadError::DecodingModuleNotActive)
            }
        },
    }
}

/// Returns the bytes of the font (loads the font from the system in case it is a `FontSource::System` font).
/// Also returns the index into the font (in case the font is a font collection).
fn font_source_get_bytes(font_source: &FontSource) -> Result<(Vec<u8>, i32), FontReloadError> {
    use std::fs;
    match font_source {
        FontSource::Embedded(bytes) => Ok((bytes.to_vec(), 0)),
        FontSource::File(file_path) => {
            fs::read(file_path)
            .map_err(|e| FontReloadError::Io(e, file_path.clone()))
            .map(|f| (f, 0))
        },
        FontSource::System(id) => load_system_font(id).ok_or(FontReloadError::FontNotFound(id.clone())),
    }
}

/// Scans the display list for all font IDs + their font size
fn scan_ui_description_for_font_keys<'a, T>(
    app_resources: &AppResources,
    display_list: &DisplayList<'a, T>
) -> FastHashMap<ImmediateFontId, FastHashSet<Au>> {

    use dom::NodeType::*;
    use ui_solver;

    let mut font_keys = FastHashMap::default();

    for node_id in display_list.rectangles.linear_iter() {

        let node_data = &display_list.ui_descr.ui_descr_arena.node_data[node_id];
        let display_rect = &display_list.rectangles[node_id];

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
fn scan_ui_description_for_image_keys<'a, T>(
    app_resources: &AppResources,
    display_list: &DisplayList<'a, T>
) -> FastHashSet<ImageId> {

    use dom::NodeType::*;

    display_list.rectangles
    .iter()
    .zip(display_list.ui_descr.ui_descr_arena.node_data.iter())
    .filter_map(|(display_rect, node_data)| {
        match node_data.get_node_type() {
            Image(id) => Some(id),
            _ => {
                let background = display_rect.style.background.as_ref()?;
                let css_image_id = background.get_css_image_id()?;
                let image_id = app_resources.get_css_image_id(&css_image_id.0)?;
                Some(*image_id)
            }
        }
    }).collect()
}

// Debug, PartialEq, Eq, PartialOrd, Ord
#[derive(Clone)]
enum AddFontMsg {
    Font(LoadedFont),
    Instance(AddFontInstance, Au),
}

// Debug, PartialEq, Eq, PartialOrd, Ord
#[derive(Clone)]
enum DeleteFontMsg {
    Font(FontKey),
    Instance(FontInstanceKey, Au),
}
// Debug, PartialEq, Eq, PartialOrd, Ord
#[derive(Clone)]
struct AddImageMsg(AddImage, ImageInfo);

// Debug, PartialEq, Eq, PartialOrd, Ord
#[derive(Clone)]
struct DeleteImageMsg(ImageKey, ImageInfo);

impl AddFontMsg {
    fn into_resource_update(&self) -> ResourceUpdate {
        use self::AddFontMsg::*;
        match self {
            Font(f) => ResourceUpdate::AddFont(AddFont::Raw(f.font_key, f.font_bytes.clone(), f.font_index as u32)),
            Instance(fi, _) => ResourceUpdate::AddFontInstance(fi.clone()),
        }
    }
}

impl DeleteFontMsg {
    fn into_resource_update(&self) -> ResourceUpdate {
        use self::DeleteFontMsg::*;
        use wr_translate::{translate_font_key, translate_font_instance_key};
        match self {
            Font(f) => ResourceUpdate::DeleteFont(translate_font_key(*f)),
            Instance(fi, _) => ResourceUpdate::DeleteFontInstance(translate_font_instance_key(*fi)),
        }
    }
}

impl AddImageMsg {
    fn into_resource_update(&self) -> ResourceUpdate {
        ResourceUpdate::AddImage(self.0.clone())
    }
}

impl DeleteImageMsg {
    fn into_resource_update(&self) -> ResourceUpdate {
        ResourceUpdate::DeleteImage(self.0.clone())

    }
}

/// Given the fonts of the current frame, returns `AddFont` and `AddFontInstance`s of
/// which fonts / instances are currently not in the `current_registered_fonts` and
/// need to be added.
///
/// Deleting fonts can only be done after the entire frame has finished drawing,
/// otherwise (if removing fonts would happen after every DOM) we'd constantly
/// add-and-remove fonts after every IFrameCallback, which would cause a lot of
/// I/O waiting.
fn build_add_font_resource_updates<T: FontImageApi>(
    app_resources: &AppResources,
    render_api: &mut T,
    fonts_in_dom: &FastHashMap<ImmediateFontId, FastHashSet<Au>>,
) -> Vec<(ImmediateFontId, AddFontMsg)> {

    use webrender::api::{FontInstancePlatformOptions, FontInstanceOptions, FontRenderMode, FontInstanceFlags};

    let mut resource_updates = Vec::new();

    for (im_font_id, font_sizes) in fonts_in_dom {

        macro_rules! insert_font_instances {($font_id:expr, $font_key:expr, $font_index:expr, $font_size:expr) => ({

            let font_instance_key_exists = app_resources.currently_registered_fonts
                .get(&$font_id)
                .and_then(|loaded_font| loaded_font.font_instances.get(&$font_size))
                .is_some();

            if !font_instance_key_exists {

                let font_instance_key = render_api.new_font_instance_key();

                // For some reason the gamma is way to low on Windows
                #[cfg(target_os = "windows")]
                let platform_options = FontInstancePlatformOptions {
                    gamma: 300,
                    contrast: 100,
                };

                #[cfg(target_os = "linux")]
                use webrender::api::{FontLCDFilter, FontHinting};

                #[cfg(target_os = "linux")]
                let platform_options = FontInstancePlatformOptions {
                    lcd_filter: FontLCDFilter::Default,
                    hinting: FontHinting::LCD,
                };

                #[cfg(target_os = "macos")]
                let platform_options = FontInstancePlatformOptions::default();

                let mut font_instance_flags = FontInstanceFlags::empty();

                font_instance_flags.set(FontInstanceFlags::SUBPIXEL_BGR, false);
                font_instance_flags.set(FontInstanceFlags::NO_AUTOHINT, true);
                font_instance_flags.set(FontInstanceFlags::LCD_VERTICAL, false);

                let options = FontInstanceOptions {
                    render_mode: FontRenderMode::Subpixel,
                    flags: font_instance_flags,
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

        match app_resources.currently_registered_fonts.get(im_font_id) {
            Some(loaded_font) => {
                for font_size in font_sizes.iter() {
                    insert_font_instances!(im_font_id.clone(), loaded_font.font_key, loaded_font.font_index, *font_size);
                }
            },
            None => {
                use azul_core::app_resources::ImmediateFontId::*;

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
fn build_add_image_resource_updates<T: FontImageApi>(
    app_resources: &AppResources,
    render_api: &mut T,
    images_in_dom: &FastHashSet<ImageId>,
) -> Vec<(ImageId, AddImageMsg)> {

    images_in_dom.iter()
    .filter(|image_id| !app_resources.currently_registered_images.contains_key(*image_id))
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
fn add_resources<T: FontImageApi>(
    app_resources: &mut AppResources,
    render_api: &mut T,
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
        app_resources.currently_registered_images.insert(*image_id, add_image_msg.1);
    }

    for (font_id, add_font_msg) in add_font_resources {
        use self::AddFontMsg::*;
        match add_font_msg {
            Font(f) => { app_resources.currently_registered_fonts.insert(font_id, LoadedFont::new(f.font_key, f.font_bytes, f.font_index)); },
            Instance(fi, size) => { app_resources.currently_registered_fonts.get_mut(&font_id).unwrap().font_instances.insert(size, fi.key); },
        }
    }
}

fn build_delete_font_resource_updates(
    app_resources: &AppResources
) -> Vec<(ImmediateFontId, DeleteFontMsg)> {

    let mut resource_updates = Vec::new();

    // Delete fonts that were not used in the last frame or have zero font instances
    for (font_id, loaded_font) in app_resources.currently_registered_fonts.iter() {
        resource_updates.extend(
            loaded_font.font_instances.iter()
            .filter(|(au, _)| app_resources.last_frame_font_keys[font_id].contains(au))
            .map(|(au, font_instance_key)| (font_id.clone(), DeleteFontMsg::Instance(*font_instance_key, *au)))
        );
        if !app_resources.last_frame_font_keys.contains_key(font_id) || loaded_font.font_instances.is_empty() {
            // Delete the font and all instances if there are no more instances of the font
            resource_updates.push((font_id.clone(), DeleteFontMsg::Font(loaded_font.font_key)));
        }
    }

    resource_updates
}

/// At the end of the frame, all images that are registered, but weren't used in the last frame
fn build_delete_image_resource_updates(
    app_resources: &AppResources
) -> Vec<(ImageId, DeleteImageMsg)> {
    app_resources.currently_registered_images.iter()
    .filter(|(id, _info)| !app_resources.last_frame_image_keys.contains(id))
    .map(|(id, info)| (*id, DeleteImageMsg(info.key, *info)))
    .collect()
}

fn delete_resources<T: FontImageApi>(
    app_resources: &mut AppResources,
    render_api: &mut T,
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
        app_resources.currently_registered_images.remove(&removed_id);
    }

    for (font_id, delete_font_msg) in delete_font_resources {
        use self::DeleteFontMsg::*;
        match delete_font_msg {
            Font(_) => { app_resources.currently_registered_fonts.remove(&font_id); },
            Instance(_, size) => { app_resources.currently_registered_fonts.get_mut(&font_id).unwrap().delete_font_instance(&size); },
        }
    }
}

#[cfg(feature = "image_loading")]
fn decode_image_data(image_data: Vec<u8>) -> Result<(WrImageData, WrImageDescriptor), ImageError> {
    use image; // the crate

    let image_format = image::guess_format(&image_data)?;
    let decoded = image::load_from_memory_with_format(&image_data, image_format)?;
    Ok(prepare_image(decoded)?)
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
                FontPropertyBuilder::new().family("Helvetica")
            }
            #[cfg(target_os = "linux")] {
                let native_sans_serif_font = linux_get_native_font(LinuxNativeFontType::SansSerif);
                FontPropertyBuilder::new().family(&native_sans_serif_font)
            }
            #[cfg(all(not(target_os = "linux"), not(target_os = "mac_os")))] {
                FontPropertyBuilder::new().family("Segoe UI")
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

    match &gsetting_cmd_result {
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
    let input = input.trim_end_matches(char::is_numeric);
    let input = input.trim();
    input
}

#[test]
#[cfg(target_os = "linux")]
fn test_parse_gsettings_font() {
    assert_eq!(parse_gsettings_font("'Ubuntu 11'"), "Ubuntu");
    assert_eq!(parse_gsettings_font("'Ubuntu Mono 13'"), "Ubuntu Mono");
}

// The next three functions are taken from:
// https://github.com/christolliday/limn/blob/master/core/src/resources/image.rs

#[cfg(feature = "image_loading")]
fn prepare_image(image_decoded: DynamicImage)
    -> Result<(WrImageData, WrImageDescriptor), ImageError>
{
    use image;
    let image_dims = image_decoded.dimensions();

    // see: https://github.com/servo/webrender/blob/80c614ab660bf6cca52594d0e33a0be262a7ac12/wrench/src/yaml_frame_reader.rs#L401-L427
    let (format, bytes) = match image_decoded {
        image::ImageLuma8(bytes) => {
            let pixels = bytes.into_raw();
            (RawImageFormat::R8, pixels)
        },
        image::ImageLumaA8(bytes) => {
            let mut pixels = Vec::with_capacity(image_dims.0 as usize * image_dims.1 as usize * 4);
            for greyscale_alpha in bytes.chunks(2) {
                let grey = greyscale_alpha[0];
                let alpha = greyscale_alpha[1];
                pixels.extend_from_slice(&[
                    grey,
                    grey,
                    grey,
                    alpha,
                ]);
            }
            // TODO: necessary for greyscale?
            premultiply(pixels.as_mut_slice());
            (RawImageFormat::BGRA8, pixels)
        },
        image::ImageRgba8(mut bytes) => {
            let mut pixels = bytes.into_raw();
            // no extra allocation necessary, but swizzling
            for rgba in pixels.chunks_mut(4) {
                let r = rgba[0];
                let g = rgba[1];
                let b = rgba[2];
                let a = rgba[3];
                rgba[0] = b;
                rgba[1] = r;
                rgba[2] = g;
                rgba[3] = a;
            }
            premultiply(pixels.as_mut_slice());
            (RawImageFormat::BGRA8, pixels)
        },
        image::ImageRgb8(bytes) => {
            let mut pixels = Vec::with_capacity(image_dims.0 as usize * image_dims.1 as usize * 4);
            for rgb in bytes.chunks(3) {
                pixels.extend_from_slice(&[
                    rgb[2], // b
                    rgb[1], // g
                    rgb[0], // r
                    0xff    // a
                ]);
            }
            (RawImageFormat::BGRA8, pixels)
        },
        image::ImageBgr8(bytes) => {
            let mut pixels = Vec::with_capacity(image_dims.0 as usize * image_dims.1 as usize * 4);
            for bgr in bytes.chunks(3) {
                pixels.extend_from_slice(&[
                    bgr[0], // b
                    bgr[1], // g
                    bgr[2], // r
                    0xff    // a
                ]);
            }
            (RawImageFormat::BGRA8, pixels)
        },
        image::ImageBgra8(bytes) => {
            // Already in the correct format
            let mut pixels = bytes.into_raw();
            premultiply(pixels.as_mut_slice());
            (RawImageFormat::BGRA8, pixels)
        },
    };

    let opaque = is_image_opaque(format, &bytes[..]);
    let allow_mipmaps = true;
    let descriptor = ImageDescriptor::new(image_dims.0 as i32, image_dims.1 as i32, format, opaque, allow_mipmaps);
    let data = ImageData::new(bytes);

    Ok((data, descriptor))
}

fn is_image_opaque(format: RawImageFormat, bytes: &[u8]) -> bool {
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
fn premultiply(data: &mut [u8]) {
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

#[test]
fn test_font_gc() {

    use std::collections::BTreeMap;
    use prelude::*;
    use ui_description::UiDescription;
    use ui_state::UiState;
    use ui_solver::px_to_au;
    use {FastHashMap, FastHashSet};
    use std::hash::Hash;

    struct Mock { }

    let mut app_resources = AppResources::new(&AppConfig::default()).unwrap();
    let mut focused_node = None;
    let mut pending_focus_target = None;
    let is_mouse_down = false;
    let hovered_nodes = BTreeMap::new();
    let css = css::from_str(r#"
        #one { font-family: Helvetica; }
        #two { font-family: Arial; }
        #three { font-family: Times New Roman; }
    "#).unwrap();

    let mut ui_state_frame_1: UiState<Mock> = Dom::mock_from_xml(r#"
        <p id="one">Hello</p>
        <p id="two">Hello</p>
        <p id="three">Hello</p>
    "#).into_ui_state();
    let ui_description_frame_1 = UiDescription::match_css_to_dom(&mut ui_state_frame_1, &css, &mut focused_node, &mut pending_focus_target, &hovered_nodes, is_mouse_down);
    let display_list_frame_1 = DisplayList::new_from_ui_description(&ui_description_frame_1, &ui_state_frame_1);


    let mut ui_state_frame_2: UiState<Mock> = Dom::mock_from_xml(r#"
        <p>Hello</p>
    "#).into_ui_state();
    let ui_description_frame_2 = UiDescription::match_css_to_dom(&mut ui_state_frame_2, &css, &mut focused_node, &mut pending_focus_target, &hovered_nodes, is_mouse_down);
    let display_list_frame_2 = DisplayList::new_from_ui_description(&ui_description_frame_2, &ui_state_frame_2);


    let mut ui_state_frame_3: UiState<Mock> = Dom::mock_from_xml(r#"
        <p id="one">Hello</p>
        <p id="two">Hello</p>
        <p id="three">Hello</p>
    "#).into_ui_state();
    let ui_description_frame_3 = UiDescription::match_css_to_dom(&mut ui_state_frame_3, &css, &mut focused_node, &mut pending_focus_target, &hovered_nodes, is_mouse_down);
    let display_list_frame_3 = DisplayList::new_from_ui_description(&ui_description_frame_3, &ui_state_frame_3);


    // Assert that all fonts got added and detected correctly
    let mut expected_fonts = FastHashMap::new();
    expected_fonts.insert(FontId::new(), FontSource::System(String::from("Helvetica")));
    expected_fonts.insert(FontId::new(), FontSource::System(String::from("Arial")));
    expected_fonts.insert(FontId::new(), FontSource::System(String::from("Times New Roman")));

    fn build_map<T: Hash + Eq, U>(i: Vec<(T, U)>) -> FastHashMap<T, U> {
        let mut map = FastHashMap::default();
        for (k, v) in i { map.insert(k, v); }
        map
    }

    fn build_set<T: Hash + Eq>(i: Vec<T>) -> FastHashSet<T> {
        let mut set = FastHashSet::default();
        for x in i { set.insert(x); }
        set
    }

    assert_eq!(scan_ui_description_for_image_keys(&app_resources, &display_list_frame_1), FastHashSet::default());
    assert_eq!(scan_ui_description_for_image_keys(&app_resources, &display_list_frame_2), FastHashSet::default());
    assert_eq!(scan_ui_description_for_image_keys(&app_resources, &display_list_frame_3), FastHashSet::default());

    assert_eq!(scan_ui_description_for_font_keys(&app_resources, &display_list_frame_1), build_map(vec![
        (ImmediateFontId::Unresolved("Arial".to_string()), build_set(vec![px_to_au(10.0)])),
        (ImmediateFontId::Unresolved("Helvetica".to_string()), build_set(vec![px_to_au(10.0)])),
        (ImmediateFontId::Unresolved("Times New Roman".to_string()), build_set(vec![px_to_au(10.0)])),
    ]));
    assert_eq!(scan_ui_description_for_font_keys(&app_resources, &display_list_frame_2), build_map(vec![
        (ImmediateFontId::Unresolved("sans-serif".to_string()), build_set(vec![px_to_au(10.0)])),
    ]));
    assert_eq!(scan_ui_description_for_font_keys(&app_resources, &display_list_frame_3), build_map(vec![
        (ImmediateFontId::Unresolved("Arial".to_string()), build_set(vec![px_to_au(10.0)])),
        (ImmediateFontId::Unresolved("Helvetica".to_string()), build_set(vec![px_to_au(10.0)])),
        (ImmediateFontId::Unresolved("Times New Roman".to_string()), build_set(vec![px_to_au(10.0)])),
    ]));



    app_resources.add_fonts_and_images(&display_list_frame_1);
    assert_eq!(app_resources.currently_registered_fonts.len(), 3);
    assert_eq!(app_resources.last_frame_font_keys.len(), 3);

    // Assert that the first frame doesn't delete the fonts again
    app_resources.garbage_collect_fonts_and_images();
    assert_eq!(app_resources.currently_registered_fonts.len(), 3); // fails

    // Assert that fonts don't get double-inserted, still the same font sources as previously
    app_resources.add_fonts_and_images(&display_list_frame_3);
    app_resources.garbage_collect_fonts_and_images();
    assert_eq!(app_resources.currently_registered_fonts.len(), 3);

    // Assert that no new fonts get added on subsequent frames
    app_resources.add_fonts_and_images(&display_list_frame_3);
    app_resources.add_fonts_and_images(&display_list_frame_3);
    app_resources.add_fonts_and_images(&display_list_frame_3);
    app_resources.add_fonts_and_images(&display_list_frame_3);
    app_resources.add_fonts_and_images(&display_list_frame_3);
    app_resources.garbage_collect_fonts_and_images();
    assert_eq!(app_resources.currently_registered_fonts.len(), 3);

    // If the DOM changes, the fonts should get deleted, the only font still present is "sans-serif"
    app_resources.add_fonts_and_images(&display_list_frame_2);
    app_resources.garbage_collect_fonts_and_images();
    assert_eq!(app_resources.currently_registered_fonts.len(), 1);

    app_resources.add_fonts_and_images(&display_list_frame_1);
    app_resources.garbage_collect_fonts_and_images();
    assert_eq!(app_resources.currently_registered_fonts.len(), 3);
}