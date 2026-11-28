//! The system UI font of an iPhone: San Francisco, the family the `system-ui` chain starts with ("System Font").
//!
//! The font scan of an iOS app is CoreText's `CTFontCollectionCreateFromAvailableFonts` (the sandbox denies `read_dir` on
//! the system font directories, see rust-fontconfig's `mobile_ios.rs`), and that collection does not hold the system UI
//! font: its faces are the hidden `.SFUI-*` names. No scan ever knew the family, every launch warned that the system UI
//! font "was NOT found by the font scan" (the Simulator consoles of 2026-11-26), and every text set in `system-ui` fell
//! back to the next family of the chain.
//!
//! What this does, once per window and only when the registry does not know the family already (a manifest that an
//! earlier launch persisted holds it: then nothing is read): ask CoreText for the system UI font and for the file it
//! was made from (`kCTFontURLAttribute`; the sandbox lets an app read that exact file), parse the file's faces like a
//! scan does (`FcParseFontPatterns`: the coverage and the style come from the file) and insert them under the name
//! "System Font" with the file's path. The bytes are read to parse them and dropped: the registry keeps the path.

use core::ffi::c_void;

use rust_fontconfig::{registry::FcFontRegistry, FcFontPath, FcPattern};

use crate::{
    desktop::shell2::common::{
        debug_server::LogCategory,
        ios_logic::{first_readable, path_from_c_buffer, SYSTEM_FONT_FAMILY, SYSTEM_UI_FONT_FILES},
    },
    log_info, log_warn,
};

#[link(name = "CoreText", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    /// The file URL of a font (descriptor): a CFURL, `NULL` when the font has no file.
    static kCTFontURLAttribute: *const c_void;
    /// `CTFontUIFontType` is a `uint32_t`; `language` NULL = the user's.
    fn CTFontCreateUIFontForLanguage(ui_type: u32, size: f64, language: *const c_void) -> *const c_void;
    fn CTFontCopyAttribute(font: *const c_void, attribute: *const c_void) -> *const c_void;
    /// Returns a `Boolean` (an unsigned char): non-zero when the buffer holds the path.
    fn CFURLGetFileSystemRepresentation(
        url: *const c_void,
        resolve_against_base: u8,
        buffer: *mut u8,
        max_buf_len: isize,
    ) -> u8;
    fn CFRelease(cf: *const c_void);
}

/// `kCTFontUIFontSystem`: the font the system draws its UI in.
const CT_UI_FONT_SYSTEM: u32 = 2;

/// The file CoreText made the system UI font from.
fn core_text_font_file() -> Option<String> {
    // SAFETY: plain CoreText / CoreFoundation calls on objects this function creates and releases; a NULL answer is
    // checked before every use.
    unsafe {
        let font = CTFontCreateUIFontForLanguage(CT_UI_FONT_SYSTEM, 17.0, core::ptr::null());
        if font.is_null() {
            return None;
        }
        let url = CTFontCopyAttribute(font, kCTFontURLAttribute);
        CFRelease(font);
        if url.is_null() {
            return None;
        }
        let mut buffer = [0_u8; 1024];
        let filled = CFURLGetFileSystemRepresentation(url, 1, buffer.as_mut_ptr(), buffer.len() as isize);
        CFRelease(url);
        if filled == 0 {
            return None;
        }
        path_from_c_buffer(&buffer)
    }
}

fn is_readable_file(path: &str) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| meta.is_file())
}

/// Make sure the registry knows the family "System Font": see the module's docs. Never panics: a missing file or an
/// unparsable one is logged and the text falls back along the chain, as it did before.
pub(super) fn ensure_system_ui_font(registry: &FcFontRegistry) {
    let wanted = FcPattern {
        name: Some(String::from(SYSTEM_FONT_FAMILY)),
        ..FcPattern::default()
    };
    if registry.query(&wanted).is_some() {
        return;
    }
    let path = core_text_font_file()
        .filter(|path| is_readable_file(path))
        .or_else(|| first_readable(SYSTEM_UI_FONT_FILES, is_readable_file).map(String::from));
    let Some(path) = path else {
        log_warn!(
            LogCategory::Layout,
            "[fonts] ios system font: CoreText names no file for the system UI font and none of {:?} is readable: \
             system-ui text falls back along the chain",
            SYSTEM_UI_FONT_FILES
        );
        return;
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            log_warn!(LogCategory::Layout, "[fonts] ios system font: {} cannot be read: {}", path, error);
            return;
        }
    };
    let Some(patterns) = rust_fontconfig::FcParseFontPatterns(&bytes) else {
        log_warn!(LogCategory::Layout, "[fonts] ios system font: {} holds no font face the parser reads", path);
        return;
    };
    let faces = patterns.len();
    for (index, mut pattern) in patterns.into_iter().enumerate() {
        pattern.name = Some(String::from(SYSTEM_FONT_FAMILY));
        pattern.family = Some(String::from(SYSTEM_FONT_FAMILY));
        registry.insert_font(
            pattern,
            FcFontPath {
                path: path.clone(),
                font_index: index,
                bytes_hash: 0,
            },
        );
    }
    log_info!(
        LogCategory::Layout,
        "[fonts] ios system font: {} ({} face(s)) registered as {:?}",
        path,
        faces,
        SYSTEM_FONT_FAMILY
    );
}
