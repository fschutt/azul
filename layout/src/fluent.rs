//! Project Fluent-based localization support for Azul
//!
//! This module provides a flexible localization system based on Project Fluent:
//! - Load translations from .fluent files
//! - Load language packs from ZIP archives
//! - Syntax validation for .fluent files
//! - Create ZIP archives of translations
//!
//! # Example
//!
//! ```rust,ignore
//! use azul_layout::fluent::FluentLocalizerHandle;
//!
//! // Create a localizer with default locale
//! let mut localizer = FluentLocalizerHandle::new("en-US");
//!
//! // Load translations from a string
//! localizer.add_resource("en-US", r#"
//! hello = Hello, world!
//! greeting = Hello, { $name }!
//! emails = You have { $count ->
//!     [one] one new email
//!    *[other] { $count } new emails
//! }.
//! "#);
//!
//! // Or load from a ZIP file containing .fluent files
//! let zip_data: Vec<u8> = std::fs::read("translations.zip").unwrap();
//! localizer.load_from_zip(&zip_data);
//!
//! // Translate messages
//! let msg = localizer.translate("en-US", "hello", None);
//! let msg = localizer.translate("en-US", "greeting", Some(&[("name", "Alice")]));
//! ```

use alloc::{
    borrow::ToOwned,
    boxed::Box,
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt::Write;
use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Mutex;
use std::io::{Read, Write as IoWrite, Cursor, Seek};

use azul_css::{AzString, U8Vec, StringVec, OptionStringVec};

use fluent::{FluentResource, FluentValue, FluentArgs};
use fluent::concurrent::FluentBundle;
use fluent_syntax::parser;
use unic_langid::LanguageIdentifier;
use zip::{ZipArchive, ZipWriter};
use zip::write::SimpleFileOptions;

/// Error type for Fluent operations
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct FluentError {
    pub message: AzString,
}

impl FluentError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: AzString::from(msg.into()),
        }
    }
}

/// A syntax error found in a .fluent file
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct FluentSyntaxError {
    /// The error message
    pub message: AzString,
    /// Line number (1-based)
    pub line: u32,
    /// Column number (1-based)
    pub column: u32,
}

/// Result of syntax checking a .fluent file
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum FluentSyntaxCheckResult {
    /// File is valid
    Ok,
    /// File has syntax errors (each string is "line:column: message")
    Errors(StringVec),
}

impl FluentSyntaxCheckResult {
    /// Returns true if the result is Ok (no syntax errors).
    pub fn is_ok(&self) -> bool {
        match self {
            FluentSyntaxCheckResult::Ok => true,
            FluentSyntaxCheckResult::Errors(_) => false,
        }
    }

    /// Get the error strings if this is an Errors result.
    /// Returns None if this is Ok.
    pub fn get_errors(&self) -> OptionStringVec {
        match self {
            FluentSyntaxCheckResult::Ok => OptionStringVec::None,
            FluentSyntaxCheckResult::Errors(e) => OptionStringVec::Some(e.clone()),
        }
    }
}

/// Result type for Fluent operations
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum FluentResult {
    Ok(AzString),
    Err(FluentError),
}

impl FluentResult {
    pub fn ok(s: impl Into<String>) -> Self {
        FluentResult::Ok(AzString::from(s.into()))
    }

    pub fn err(msg: impl Into<String>) -> Self {
        FluentResult::Err(FluentError::new(msg))
    }

    pub fn into_option(self) -> Option<AzString> {
        match self {
            FluentResult::Ok(s) => Some(s),
            FluentResult::Err(_) => None,
        }
    }

    pub fn unwrap_or(self, default: AzString) -> AzString {
        match self {
            FluentResult::Ok(s) => s,
            FluentResult::Err(_) => default,
        }
    }
}

// Import FmtArg types from fmt module
use crate::fmt::{FmtArg, FmtArgVec, FmtValue};

/// Information about a loaded language pack
#[derive(Debug, Clone)]
#[repr(C)]
pub struct FluentLanguageInfo {
    /// The locale identifier (e.g., "en-US", "de-DE")
    pub locale: AzString,
    /// Number of message IDs in this locale
    pub message_count: usize,
    /// List of all message IDs
    pub message_ids: Vec<AzString>,
}

/// Vec of FluentLanguageInfo
pub type FluentLanguageInfoVec = Vec<FluentLanguageInfo>;

/// Result of loading a ZIP language pack
#[derive(Debug, Clone)]
#[repr(C)]
pub struct FluentZipLoadResult {
    /// Number of files successfully loaded
    pub files_loaded: usize,
    /// Number of files that failed to load
    pub files_failed: usize,
    /// Error messages for failed files
    pub errors: StringVec,
}

/// A single Fluent bundle for one locale
struct FluentLocaleBundle {
    bundle: FluentBundle<FluentResource>,
    /// Source files that were loaded (for debugging)
    sources: Vec<String>,
}

impl FluentLocaleBundle {
    fn new(locale_str: &str) -> Option<Self> {
        let langid: LanguageIdentifier = locale_str.parse().ok()?;
        let mut bundle = FluentBundle::new_concurrent(vec![langid]);
        bundle.set_use_isolating(false); // Don't add Unicode isolation marks
        Some(Self {
            bundle,
            sources: Vec::new(),
        })
    }

    fn add_resource(&mut self, source: &str) -> Result<(), Vec<fluent::FluentError>> {
        let resource = FluentResource::try_new(source.to_owned())
            .map_err(|(_res, errors)| {
                errors.into_iter().map(|e| fluent::FluentError::ParserError(e)).collect::<Vec<_>>()
            })?;
        self.bundle.add_resource(resource)?;
        self.sources.push(source.to_owned());
        Ok(())
    }

    fn format(&self, message_id: &str, args: &FmtArgVec) -> Option<String> {
        let msg = self.bundle.get_message(message_id)?;
        let pattern = msg.value()?;

        let mut errors = vec![];
        let fluent_args = if args.is_empty() {
            None
        } else {
            let mut fa = FluentArgs::new();
            for arg in args.iter() {
                match &arg.value {
                    FmtValue::Str(s) => {
                        fa.set(arg.key.as_str().to_owned(), FluentValue::from(s.as_str().to_owned()));
                    }
                    FmtValue::Sint(n) => {
                        fa.set(arg.key.as_str().to_owned(), FluentValue::from(*n as f64));
                    }
                    FmtValue::Uint(n) => {
                        fa.set(arg.key.as_str().to_owned(), FluentValue::from(*n as f64));
                    }
                    FmtValue::Slong(n) => {
                        fa.set(arg.key.as_str().to_owned(), FluentValue::from(*n as f64));
                    }
                    FmtValue::Ulong(n) => {
                        fa.set(arg.key.as_str().to_owned(), FluentValue::from(*n as f64));
                    }
                    FmtValue::Float(n) => {
                        fa.set(arg.key.as_str().to_owned(), FluentValue::from(*n as f64));
                    }
                    FmtValue::Double(n) => {
                        fa.set(arg.key.as_str().to_owned(), FluentValue::from(*n));
                    }
                    FmtValue::Bool(b) => {
                        fa.set(arg.key.as_str().to_owned(), FluentValue::from(if *b { "true" } else { "false" }));
                    }
                    // Handle remaining numeric types
                    FmtValue::Uchar(n) => {
                        fa.set(arg.key.as_str().to_owned(), FluentValue::from(*n as f64));
                    }
                    FmtValue::Schar(n) => {
                        fa.set(arg.key.as_str().to_owned(), FluentValue::from(*n as f64));
                    }
                    FmtValue::Ushort(n) => {
                        fa.set(arg.key.as_str().to_owned(), FluentValue::from(*n as f64));
                    }
                    FmtValue::Sshort(n) => {
                        fa.set(arg.key.as_str().to_owned(), FluentValue::from(*n as f64));
                    }
                    FmtValue::Isize(n) => {
                        fa.set(arg.key.as_str().to_owned(), FluentValue::from(*n as f64));
                    }
                    FmtValue::Usize(n) => {
                        fa.set(arg.key.as_str().to_owned(), FluentValue::from(*n as f64));
                    }
                    FmtValue::StrVec(sv) => {
                        // Convert string vec to comma-separated string
                        let joined: String = sv.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
                        fa.set(arg.key.as_str().to_owned(), FluentValue::from(joined));
                    }
                }
            }
            Some(fa)
        };

        let result = self.bundle.format_pattern(pattern, fluent_args.as_ref(), &mut errors);
        Some(result.to_string())
    }

    fn has_message(&self, message_id: &str) -> bool {
        self.bundle.has_message(message_id)
    }

    fn get_message_ids(&self) -> Vec<String> {
        // FluentBundle doesn't provide iteration over message IDs directly
        // We'd need to parse the sources to get them
        // For now, return empty - this could be improved
        Vec::new()
    }
}

/// Inner data for FluentLocalizerHandle
pub struct FluentLocalizerInner {
    /// Bundles for each locale
    bundles: Mutex<BTreeMap<String, FluentLocaleBundle>>,
    /// Default locale
    default_locale: Mutex<String>,
    /// Fallback chain (locale -> list of fallback locales)
    fallback_chain: Mutex<BTreeMap<String, Vec<String>>>,
}

/// A thread-safe cache of Fluent localizers for multiple locales.
///
/// This is the main entry point for Fluent-based localization.
/// It can load translations from:
/// - Individual .fluent strings
/// - ZIP archives containing .fluent files
///
/// All methods are thread-safe and can be called from multiple threads.
#[repr(C)]
pub struct FluentLocalizerHandle {
    pub ptr: *const FluentLocalizerInner,
    pub copies: *const AtomicUsize,
    pub run_destructor: bool,
}

unsafe impl Send for FluentLocalizerHandle {}
unsafe impl Sync for FluentLocalizerHandle {}

impl Clone for FluentLocalizerHandle {
    fn clone(&self) -> Self {
        unsafe {
            self.copies
                .as_ref()
                .map(|m| m.fetch_add(1, AtomicOrdering::SeqCst));
        }
        Self {
            ptr: self.ptr,
            copies: self.copies,
            run_destructor: true,
        }
    }
}

impl Drop for FluentLocalizerHandle {
    fn drop(&mut self) {
        self.run_destructor = false;
        unsafe {
            let copies = (*self.copies).fetch_sub(1, AtomicOrdering::SeqCst);
            if copies == 1 {
                let _ = Box::from_raw(self.ptr as *mut FluentLocalizerInner);
                let _ = Box::from_raw(self.copies as *mut AtomicUsize);
            }
        }
    }
}

impl core::fmt::Debug for FluentLocalizerHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let inner = self.inner();
        let default_locale = inner.default_locale.lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| String::new());
        f.debug_struct("FluentLocalizerHandle")
            .field("default_locale", &default_locale)
            .finish()
    }
}

impl Default for FluentLocalizerHandle {
    fn default() -> Self {
        Self::create("en-US")
    }
}

impl FluentLocalizerHandle {
    /// Create a new Fluent localizer with the given default locale.
    pub fn create(default_locale: &str) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(FluentLocalizerInner {
                bundles: Mutex::new(BTreeMap::new()),
                default_locale: Mutex::new(default_locale.to_string()),
                fallback_chain: Mutex::new(BTreeMap::new()),
            })),
            copies: Box::into_raw(Box::new(AtomicUsize::new(1))),
            run_destructor: true,
        }
    }

    /// Get a reference to the inner data.
    #[inline]
    fn inner(&self) -> &FluentLocalizerInner {
        unsafe { &*self.ptr }
    }

    /// Get the default locale string.
    pub fn get_default_locale(&self) -> AzString {
        self.inner().default_locale.lock()
            .map(|g| AzString::from(g.clone()))
            .unwrap_or_else(|_| AzString::from("en-US"))
    }

    /// Set the default locale.
    pub fn set_default_locale(&self, locale: &str) {
        if let Ok(mut guard) = self.inner().default_locale.lock() {
            *guard = locale.to_string();
        }
    }

    /// Set the fallback chain for a locale.
    ///
    /// When a message is not found in the requested locale, the localizer
    /// will try each fallback locale in order.
    ///
    /// # Example
    /// ```rust,ignore
    /// // For Swiss German, fall back to German, then English
    /// localizer.set_fallback_chain("de-CH", &["de-DE", "en-US"]);
    /// ```
    pub fn set_fallback_chain(&self, locale: &str, fallbacks: &[&str]) {
        if let Ok(mut guard) = self.inner().fallback_chain.lock() {
            guard.insert(
                locale.to_string(),
                fallbacks.iter().map(|s| s.to_string()).collect(),
            );
        }
    }

    /// Add a Fluent resource (FTL content) for a specific locale.
    ///
    /// # Arguments
    /// * `locale` - BCP 47 locale string (e.g., "en-US", "de-DE")
    /// * `source` - FTL content (Fluent Translation List)
    ///
    /// # Returns
    /// `true` if the resource was successfully added, `false` if there were errors.
    pub fn add_resource(&self, locale: &str, source: &str) -> bool {
        if let Ok(mut bundles) = self.inner().bundles.lock() {
            let bundle = bundles
                .entry(locale.to_string())
                .or_insert_with(|| FluentLocaleBundle::new(locale).unwrap_or_else(|| {
                    FluentLocaleBundle::new("en-US").expect("en-US should always work")
                }));
            bundle.add_resource(source).is_ok()
        } else {
            false
        }
    }

    /// Add a Fluent resource from a U8Vec (for C API compatibility).
    pub fn add_resource_from_bytes(&self, locale: &str, data: &[u8]) -> bool {
        match std::str::from_utf8(data) {
            Ok(source) => self.add_resource(locale, source),
            Err(_) => false,
        }
    }

    /// Load translations from a ZIP archive.
    ///
    /// The ZIP file should contain .fluent files organized by locale.
    /// File naming convention: `{locale}.fluent` or `{locale}/{filename}.fluent`
    ///
    /// # Arguments
    /// * `data` - The ZIP file contents
    /// * `locale_override` - If Some, all files in the ZIP will be loaded for this locale.
    ///                       If None, the locale is detected from the file path.
    ///
    /// # Examples of valid ZIP structures:
    ///
    /// Flat structure:
    /// ```text
    /// translations.zip
    /// ├── en-US.fluent
    /// ├── de-DE.fluent
    /// └── fr-FR.fluent
    /// ```
    ///
    /// Nested structure:
    /// ```text
    /// translations.zip
    /// ├── en-US/
    /// │   ├── main.fluent
    /// │   └── errors.fluent
    /// ├── de-DE/
    /// │   ├── main.fluent
    /// │   └── errors.fluent
    /// ```
    pub fn load_from_zip_with_locale(&self, data: &[u8], locale_override: Option<&str>) -> FluentZipLoadResult {
        let cursor = Cursor::new(data);
        let mut archive = match ZipArchive::new(cursor) {
            Ok(a) => a,
            Err(e) => return FluentZipLoadResult {
                files_loaded: 0,
                files_failed: 1,
                errors: StringVec::from_vec(vec![AzString::from(format!("Failed to open ZIP: {}", e))]),
            },
        };

        let mut files_loaded = 0;
        let mut files_failed = 0;
        let mut errors = Vec::new();

        for i in 0..archive.len() {
            let mut file = match archive.by_index(i) {
                Ok(f) => f,
                Err(e) => {
                    files_failed += 1;
                    errors.push(AzString::from(format!("Failed to read file {}: {}", i, e)));
                    continue;
                }
            };

            let name = file.name().to_string();

            // Skip directories and non-.fluent files
            if file.is_dir() || !name.ends_with(".fluent") {
                continue;
            }

            // Use locale override or extract from filename
            let locale = match locale_override {
                Some(l) => l.to_string(),
                None => {
                    match extract_locale_from_path(&name) {
                        Some(l) => l,
                        None => {
                            files_failed += 1;
                            errors.push(AzString::from(format!("Could not determine locale from path: {}", name)));
                            continue;
                        }
                    }
                }
            };

            // Read file content
            let mut content = String::new();
            if let Err(e) = file.read_to_string(&mut content) {
                files_failed += 1;
                errors.push(AzString::from(format!("Failed to read {}: {}", name, e)));
                continue;
            }

            // Add resource
            if self.add_resource(&locale, &content) {
                files_loaded += 1;
            } else {
                files_failed += 1;
                errors.push(AzString::from(format!("Failed to parse {}", name)));
            }
        }

        FluentZipLoadResult {
            files_loaded,
            files_failed,
            errors: StringVec::from_vec(errors),
        }
    }

    /// Load translations from a ZIP archive (auto-detect locale from filename).
    pub fn load_from_zip(&self, data: &[u8]) -> FluentZipLoadResult {
        self.load_from_zip_with_locale(data, None)
    }

    /// Load translations from a ZIP archive (U8Vec for FFI).
    pub fn load_from_zip_bytes(&self, data: &U8Vec) -> FluentZipLoadResult {
        self.load_from_zip(data.as_slice())
    }

    /// Load translations from a ZIP archive with explicit locale (U8Vec for FFI).
    pub fn load_from_zip_bytes_with_locale(&self, data: &U8Vec, locale: &str) -> FluentZipLoadResult {
        self.load_from_zip_with_locale(data.as_slice(), Some(locale))
    }

    /// Load a single .fluent file with explicit locale.
    pub fn load_fluent_file(&self, locale: &str, content: &str) -> bool {
        self.add_resource(locale, content)
    }

    /// Load a single .fluent file from bytes with explicit locale.
    pub fn load_fluent_file_bytes(&self, locale: &str, data: &[u8]) -> bool {
        match std::str::from_utf8(data) {
            Ok(source) => self.add_resource(locale, source),
            Err(_) => false,
        }
    }

    /// Load translations from a local file path.
    ///
    /// # Arguments
    /// * `path` - Path to a .fluent file or a .zip file
    /// * `locale_override` - If Some, use this locale. If None, detect from filename.
    pub fn load_from_path(&self, path: &str, locale_override: Option<&str>) -> FluentZipLoadResult {
        let path_obj = std::path::Path::new(path);

        // Read file contents
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => return FluentZipLoadResult {
                files_loaded: 0,
                files_failed: 1,
                errors: StringVec::from_vec(vec![AzString::from(format!("Failed to read file '{}': {}", path, e))]),
            },
        };

        // Determine locale from filename if not overridden
        let locale = match locale_override {
            Some(l) => Some(l.to_string()),
            None => path_obj.file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| if looks_like_locale(s) { Some(s.to_string()) } else { None }),
        };

        // Handle based on file extension
        let extension = path_obj.extension().and_then(|s| s.to_str()).unwrap_or("");

        match extension {
            "zip" => self.load_from_zip_with_locale(&data, locale.as_deref()),
            "fluent" | "ftl" => {
                let locale = match locale {
                    Some(l) => l,
                    None => return FluentZipLoadResult {
                        files_loaded: 0,
                        files_failed: 1,
                        errors: StringVec::from_vec(vec![AzString::from(format!("Could not determine locale from filename: {}", path))]),
                    },
                };

                match std::str::from_utf8(&data) {
                    Ok(content) => {
                        if self.add_resource(&locale, content) {
                            FluentZipLoadResult {
                                files_loaded: 1,
                                files_failed: 0,
                                errors: StringVec::from_const_slice(&[]),
                            }
                        } else {
                            FluentZipLoadResult {
                                files_loaded: 0,
                                files_failed: 1,
                                errors: StringVec::from_vec(vec![AzString::from(format!("Failed to parse {}", path))]),
                            }
                        }
                    }
                    Err(e) => FluentZipLoadResult {
                        files_loaded: 0,
                        files_failed: 1,
                        errors: StringVec::from_vec(vec![AzString::from(format!("Invalid UTF-8 in {}: {}", path, e))]),
                    },
                }
            }
            _ => FluentZipLoadResult {
                files_loaded: 0,
                files_failed: 1,
                errors: StringVec::from_vec(vec![AzString::from(format!("Unknown file extension: {} (expected .fluent, .ftl, or .zip)", extension))]),
            },
        }
    }

    /// Translate a message ID to the target locale.
    ///
    /// # Arguments
    /// * `locale` - The target locale (e.g., "en-US")
    /// * `message_id` - The message ID to translate
    /// * `args` - Format arguments (pass empty vec for no arguments)
    ///
    /// # Returns
    /// The translated string, or the message ID if not found.
    pub fn translate(
        &self,
        locale: AzString,
        message_id: AzString,
        args: FmtArgVec,
    ) -> AzString {
        let locale = locale.as_str();
        let message_id = message_id.as_str();

        // Try the requested locale first
        if let Some(result) = self.try_translate(locale, message_id, &args) {
            return result;
        }

        // Try fallback chain
        if let Ok(fallbacks) = self.inner().fallback_chain.lock() {
            if let Some(chain) = fallbacks.get(locale) {
                for fallback in chain {
                    if let Some(result) = self.try_translate(fallback, message_id, &args) {
                        return result;
                    }
                }
            }
        }

        // Try default locale
        let default_locale = self.inner().default_locale.lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| "en-US".to_string());

        if locale != default_locale {
            if let Some(result) = self.try_translate(&default_locale, message_id, &args) {
                return result;
            }
        }

        // Return the message ID as fallback
        AzString::from(message_id.to_string())
    }

    /// Try to translate a message in a specific locale (no fallback).
    fn try_translate(
        &self,
        locale: &str,
        message_id: &str,
        args: &FmtArgVec,
    ) -> Option<AzString> {
        self.inner().bundles.lock().ok().and_then(|bundles| {
            bundles.get(locale).and_then(|bundle| {
                bundle.format(message_id, args).map(AzString::from)
            })
        })
    }

    /// Check if a message ID exists in the given locale.
    pub fn has_message(&self, locale: &str, message_id: &str) -> bool {
        self.inner().bundles.lock().ok().map(|bundles| {
            bundles.get(locale).map(|b| b.has_message(message_id)).unwrap_or(false)
        }).unwrap_or(false)
    }

    /// Get the list of all loaded locales.
    pub fn get_loaded_locales(&self) -> Vec<AzString> {
        self.inner().bundles.lock().ok().map(|bundles| {
            bundles.keys().map(|k| AzString::from(k.clone())).collect()
        }).unwrap_or_default()
    }

    /// Get information about all loaded languages.
    pub fn get_language_info(&self) -> FluentLanguageInfoVec {
        self.inner().bundles.lock().ok().map(|bundles| {
            bundles.iter().map(|(locale, bundle)| {
                FluentLanguageInfo {
                    locale: AzString::from(locale.clone()),
                    message_count: bundle.sources.len(), // Approximate
                    message_ids: bundle.get_message_ids().into_iter().map(AzString::from).collect(),
                }
            }).collect()
        }).unwrap_or_default()
    }

    /// Clear all loaded resources for a specific locale.
    pub fn clear_locale(&self, locale: &str) {
        if let Ok(mut bundles) = self.inner().bundles.lock() {
            bundles.remove(locale);
        }
    }

    /// Clear all loaded resources.
    pub fn clear_all(&self) {
        if let Ok(mut bundles) = self.inner().bundles.lock() {
            bundles.clear();
        }
    }
}

// ============================================================================
// Syntax Checking
// ============================================================================

/// Check the syntax of a Fluent (FTL) string.
///
/// Returns `Ok` if the syntax is valid, or a list of error strings.
/// Each error string has the format "line:column: message".
pub fn check_fluent_syntax(source: &str) -> FluentSyntaxCheckResult {
    match parser::parse(source) {
        Ok(_) => FluentSyntaxCheckResult::Ok,
        Err((_resource, errors)) => {
            let syntax_errors: Vec<AzString> = errors.iter().map(|e| {
                let message = format!("{:?}", e.kind);
                let (line, column) = get_error_position(source, e.pos.start);
                AzString::from(format!("{}:{}: {}", line, column, message))
            }).collect();
            FluentSyntaxCheckResult::Errors(syntax_errors.into())
        }
    }
}

/// Check the syntax of a Fluent file from bytes.
pub fn check_fluent_syntax_bytes(data: &[u8]) -> FluentSyntaxCheckResult {
    match std::str::from_utf8(data) {
        Ok(source) => check_fluent_syntax(source),
        Err(e) => FluentSyntaxCheckResult::Errors(vec![
            AzString::from(format!("0:0: Invalid UTF-8: {}", e))
        ].into()),
    }
}

/// Get line and column number from byte offset
fn get_error_position(source: &str, offset: usize) -> (u32, u32) {
    let mut line = 1u32;
    let mut column = 1u32;

    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    (line, column)
}

// ============================================================================
// ZIP Creation (uses ZipFileEntry from zip module)
// ============================================================================

use crate::zip::{ZipFile, ZipFileEntry, ZipWriteConfig};

/// Create a ZIP archive from Fluent file entries.
///
/// # Arguments
/// * `entries` - List of ZipFileEntry to include in the ZIP
///
/// # Returns
/// The ZIP file as a byte vector, or an error message.
pub fn create_fluent_zip(entries: Vec<ZipFileEntry>) -> Result<Vec<u8>, String> {
    let zip = ZipFile { entries };
    let config = ZipWriteConfig::default();
    zip.to_bytes(&config).map_err(|e| e.to_string())
}

/// Create a ZIP archive from locale/content pairs
pub fn create_fluent_zip_from_strings(files: Vec<(String, String)>) -> Result<Vec<u8>, String> {
    let entries: Vec<ZipFileEntry> = files
        .into_iter()
        .map(|(path, content)| ZipFileEntry::file(path, content.into_bytes()))
        .collect();
    create_fluent_zip(entries)
}

/// Export all translations from a FluentLocalizerHandle to a ZIP archive.
pub fn export_to_zip(localizer: &FluentLocalizerHandle) -> Result<Vec<u8>, String> {
    let bundles = localizer.inner().bundles.lock()
        .map_err(|e| format!("Lock error: {:?}", e))?;

    let entries: Vec<ZipFileEntry> = bundles.iter().flat_map(|(locale, bundle)| {
        bundle.sources.iter().enumerate().map(|(i, source)| {
            let path = if bundle.sources.len() == 1 {
                format!("{}.fluent", locale)
            } else {
                format!("{}/part_{}.fluent", locale, i)
            };
            ZipFileEntry::file(path, source.clone().into_bytes())
        }).collect::<Vec<_>>()
    }).collect();

    create_fluent_zip(entries)
}


// ============================================================================
// Helper Functions
// ============================================================================

/// Extract locale from a file path.
///
/// Supports:
/// - "en-US.fluent" -> "en-US"
/// - "en-US/main.fluent" -> "en-US"
/// - "locales/en-US/main.fluent" -> "en-US"
fn extract_locale_from_path(path: &str) -> Option<String> {
    let path = path.trim_start_matches('/');

    // Try to extract from directory structure first
    let parts: Vec<&str> = path.split('/').collect();

    if parts.len() >= 2 {
        // Check if second-to-last part looks like a locale (directory name)
        let potential_locale = parts[parts.len() - 2];
        if looks_like_locale(potential_locale) {
            return Some(potential_locale.to_string());
        }
    }

    // Try extracting from filename (strip .fluent suffix first)
    if let Some(filename) = parts.last() {
        let name = filename.trim_end_matches(".fluent");
        if looks_like_locale(name) {
            return Some(name.to_string());
        }
    }

    // Check if first part (after stripping suffix) looks like a locale
    if parts.len() == 1 {
        let name = parts[0].trim_end_matches(".fluent");
        if looks_like_locale(name) {
            return Some(name.to_string());
        }
    }

    None
}

/// Check if a string looks like a BCP 47 locale identifier.
fn looks_like_locale(s: &str) -> bool {
    // Simple check: 2-3 letters, optionally followed by '-' and more
    // e.g., "en", "en-US", "zh-Hans-CN"
    let parts: Vec<&str> = s.split('-').collect();
    if parts.is_empty() {
        return false;
    }

    let first = parts[0];
    if first.len() < 2 || first.len() > 3 {
        return false;
    }
    if !first.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }

    true
}

// ============================================================================
// Extension trait for LayoutCallbackInfo (like ICU)
// ============================================================================

/// Extension trait for accessing Fluent localizer from callbacks.
pub trait LayoutCallbackInfoFluentExt {
    /// Get the Fluent localizer handle for translations.
    fn get_fluent_localizer(&self) -> Option<FluentLocalizerHandle>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_translation() {
        let localizer = FluentLocalizerHandle::create("en-US");

        let ftl = r#"
hello = Hello, world!
greeting = Hello, { $name }!
"#;

        assert!(localizer.add_resource("en-US", ftl));

        let empty_args = FmtArgVec::new();
        let result = localizer.translate("en-US", "hello", &empty_args);
        assert_eq!(result.as_str(), "Hello, world!");

        let args = FmtArgVec::from_vec(vec![FmtArg {
            key: AzString::from("name"),
            value: FmtValue::Str(AzString::from("Alice")),
        }]);
        let result = localizer.translate("en-US", "greeting", &args);
        assert_eq!(result.as_str(), "Hello, Alice!");
    }

    #[test]
    fn test_syntax_check() {
        // Valid FTL
        let valid = "hello = Hello, world!";
        assert!(matches!(check_fluent_syntax(valid), FluentSyntaxCheckResult::Ok));

        // Invalid FTL (missing value)
        let invalid = "hello = ";
        let result = check_fluent_syntax(invalid);
        assert!(matches!(result, FluentSyntaxCheckResult::Errors(_)));
    }

    #[test]
    fn test_locale_extraction() {
        assert_eq!(extract_locale_from_path("en-US.fluent"), Some("en-US".to_string()));
        assert_eq!(extract_locale_from_path("en-US/main.fluent"), Some("en-US".to_string()));
        assert_eq!(extract_locale_from_path("locales/de-DE/errors.fluent"), Some("de-DE".to_string()));
    }

    #[test]
    fn test_looks_like_locale() {
        assert!(looks_like_locale("en"));
        assert!(looks_like_locale("en-US"));
        assert!(looks_like_locale("zh-Hans-CN"));
        assert!(!looks_like_locale("main"));
        assert!(!looks_like_locale("1234"));
    }
}
