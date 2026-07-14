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
//! let mut localizer = FluentLocalizerHandle::create("en-US");
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
//! let empty_args = FmtArgVec::new();
//! let msg = localizer.translate("en-US".into(), "hello".into(), empty_args);
//!
//! let args = FmtArgVec::from_vec(vec![FmtArg {
//!     key: AzString::from("name"),
//!     value: FmtValue::Str(AzString::from("Alice")),
//! }]);
//! let msg = localizer.translate("en-US".into(), "greeting".into(), args);
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

use azul_css::{AzString, U8Vec, StringVec, OptionStringVec, impl_option, impl_option_inner, impl_vec, impl_vec_clone, impl_vec_debug};

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

/// A single failure encountered while loading a Fluent language pack.
///
/// Each variant carries a human-readable detail message; the variant itself
/// classifies *what* went wrong so callers can match on the category instead
/// of parsing strings.
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum FluentLoadError {
    /// The ZIP archive could not be opened.
    OpenArchive(AzString),
    /// An entry inside the ZIP archive could not be read.
    ReadEntry(AzString),
    /// The locale could not be determined from a file path/name.
    UnknownLocale(AzString),
    /// A file's contents could not be read.
    ReadFile(AzString),
    /// A `.fluent`/`.ftl` resource failed to parse.
    Parse(AzString),
    /// A file was not valid UTF-8.
    InvalidUtf8(AzString),
    /// The file extension was not recognized.
    UnknownExtension(AzString),
}

impl_option!(FluentLoadError, OptionFluentLoadError, copy = false, [Debug, Clone]);
impl_vec!(FluentLoadError, FluentLoadErrorVec, FluentLoadErrorVecDestructor, FluentLoadErrorVecDestructorType, FluentLoadErrorVecSlice, OptionFluentLoadError);
impl_vec_clone!(FluentLoadError, FluentLoadErrorVec, FluentLoadErrorVecDestructor);
impl_vec_debug!(FluentLoadError, FluentLoadErrorVec);

/// Result of loading a ZIP language pack
#[derive(Debug, Clone)]
#[repr(C)]
pub struct FluentZipLoadResult {
    /// Number of files successfully loaded
    pub files_loaded: usize,
    /// Number of files that failed to load
    pub files_failed: usize,
    /// Typed errors for failed files
    pub errors: FluentLoadErrorVec,
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
        let mut ids = Vec::new();
        for source in &self.sources {
            if let Ok(resource) = parser::parse(source.as_str()) {
                for entry in resource.body {
                    if let fluent_syntax::ast::Entry::Message(msg) = entry {
                        ids.push(msg.id.name.to_string());
                    }
                }
            }
        }
        ids
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
                errors: FluentLoadErrorVec::from_vec(vec![FluentLoadError::OpenArchive(AzString::from(format!("Failed to open ZIP: {}", e)))]),
            },
        };

        let mut files_loaded = 0;
        let mut files_failed = 0;
        let mut errors: Vec<FluentLoadError> = Vec::new();

        for i in 0..archive.len() {
            let mut file = match archive.by_index(i) {
                Ok(f) => f,
                Err(e) => {
                    files_failed += 1;
                    errors.push(FluentLoadError::ReadEntry(AzString::from(format!("Failed to read file {}: {}", i, e))));
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
                            errors.push(FluentLoadError::UnknownLocale(AzString::from(format!("Could not determine locale from path: {}", name))));
                            continue;
                        }
                    }
                }
            };

            // Read file content
            let mut content = String::new();
            if let Err(e) = file.read_to_string(&mut content) {
                files_failed += 1;
                errors.push(FluentLoadError::ReadFile(AzString::from(format!("Failed to read {}: {}", name, e))));
                continue;
            }

            // Add resource
            if self.add_resource(&locale, &content) {
                files_loaded += 1;
            } else {
                files_failed += 1;
                errors.push(FluentLoadError::Parse(AzString::from(format!("Failed to parse {}", name))));
            }
        }

        FluentZipLoadResult {
            files_loaded,
            files_failed,
            errors: FluentLoadErrorVec::from_vec(errors),
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
                errors: FluentLoadErrorVec::from_vec(vec![FluentLoadError::ReadFile(AzString::from(format!("Failed to read file '{}': {}", path, e)))]),
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
                        errors: FluentLoadErrorVec::from_vec(vec![FluentLoadError::UnknownLocale(AzString::from(format!("Could not determine locale from filename: {}", path)))]),
                    },
                };

                match std::str::from_utf8(&data) {
                    Ok(content) => {
                        if self.add_resource(&locale, content) {
                            FluentZipLoadResult {
                                files_loaded: 1,
                                files_failed: 0,
                                errors: FluentLoadErrorVec::new(),
                            }
                        } else {
                            FluentZipLoadResult {
                                files_loaded: 0,
                                files_failed: 1,
                                errors: FluentLoadErrorVec::from_vec(vec![FluentLoadError::Parse(AzString::from(format!("Failed to parse {}", path)))]),
                            }
                        }
                    }
                    Err(e) => FluentZipLoadResult {
                        files_loaded: 0,
                        files_failed: 1,
                        errors: FluentLoadErrorVec::from_vec(vec![FluentLoadError::InvalidUtf8(AzString::from(format!("Invalid UTF-8 in {}: {}", path, e)))]),
                    },
                }
            }
            _ => FluentZipLoadResult {
                files_loaded: 0,
                files_failed: 1,
                errors: FluentLoadErrorVec::from_vec(vec![FluentLoadError::UnknownExtension(AzString::from(format!("Unknown file extension: {} (expected .fluent, .ftl, or .zip)", extension)))]),
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
                let ids: Vec<AzString> = bundle.get_message_ids().into_iter().map(AzString::from).collect();
                FluentLanguageInfo {
                    locale: AzString::from(locale.clone()),
                    message_count: ids.len(),
                    message_ids: ids,
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
        let result = localizer.translate(AzString::from("en-US"), AzString::from("hello"), empty_args);
        assert_eq!(result.as_str(), "Hello, world!");

        let args = FmtArgVec::from_vec(vec![FmtArg {
            key: AzString::from("name"),
            value: FmtValue::Str(AzString::from("Alice")),
        }]);
        let result = localizer.translate(AzString::from("en-US"), AzString::from("greeting"), args);
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

#[cfg(test)]
mod autotest_generated {
    use super::*;

    // ------------------------------------------------------------------
    // helpers
    // ------------------------------------------------------------------

    static TMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// A fresh temp directory, so that filename-derived locales ("en-US.ftl")
    /// can be used verbatim without colliding between parallel tests.
    fn tmp_dir() -> std::path::PathBuf {
        let n = TMP_COUNTER.fetch_add(1, AtomicOrdering::SeqCst);
        let mut p = std::env::temp_dir();
        p.push(format!("azul_fluent_autotest_{}_{}", std::process::id(), n));
        std::fs::create_dir_all(&p).expect("temp dir");
        p
    }

    fn args_of(pairs: Vec<(&str, FmtValue)>) -> FmtArgVec {
        FmtArgVec::from_vec(
            pairs
                .into_iter()
                .map(|(k, v)| FmtArg {
                    key: AzString::from(k),
                    value: v,
                })
                .collect::<Vec<_>>(),
        )
    }

    fn tr(h: &FluentLocalizerHandle, locale: &str, id: &str) -> String {
        h.translate(
            AzString::from(locale),
            AzString::from(id),
            FmtArgVec::new(),
        )
        .as_str()
        .to_string()
    }

    // ------------------------------------------------------------------
    // FluentError::new  (constructor)
    // ------------------------------------------------------------------

    #[test]
    fn autotest_fluent_error_new_preserves_payload_exactly() {
        assert_eq!(FluentError::new("").message.as_str(), "");
        assert_eq!(FluentError::new(String::new()).message.as_str(), "");

        // AzString is length-prefixed, so an interior NUL must survive intact
        let nul = FluentError::new("a\0b");
        assert_eq!(nul.message.as_str(), "a\0b");
        assert_eq!(nul.message.as_str().len(), 3);

        // emoji + combining mark + bidi override + BOM round-trip byte-for-byte
        let uni = "\u{1F600}e\u{0301}\u{202E}\u{FEFF}";
        assert_eq!(FluentError::new(uni).message.as_str(), uni);

        // 1 MiB payload: no truncation, no panic
        let huge = "x".repeat(1024 * 1024);
        let big = FluentError::new(huge.clone());
        assert_eq!(big.message.as_str().len(), huge.len());

        // Clone / PartialEq invariants
        assert_eq!(big.clone(), big);
        assert_ne!(FluentError::new("a"), FluentError::new("b"));
    }

    // ------------------------------------------------------------------
    // FluentSyntaxCheckResult::is_ok / get_errors  (predicate + getter)
    // ------------------------------------------------------------------

    #[test]
    fn autotest_syntax_check_result_predicate_and_getter_stay_dual() {
        let ok = FluentSyntaxCheckResult::Ok;
        assert!(ok.is_ok());
        assert!(matches!(ok.get_errors(), OptionStringVec::None));

        // an *empty* error list is still not "Ok" -- the variant decides, not the count
        let empty_errs = FluentSyntaxCheckResult::Errors(StringVec::from_vec(Vec::new()));
        assert!(!empty_errs.is_ok());
        match empty_errs.get_errors() {
            OptionStringVec::Some(v) => assert_eq!(v.len(), 0),
            OptionStringVec::None => panic!("Errors(_) must yield Some even when empty"),
        }

        let errs = FluentSyntaxCheckResult::Errors(StringVec::from_vec(vec![
            AzString::from("1:1: bad"),
            AzString::from("\u{1F600}"),
        ]));
        assert!(!errs.is_ok());

        // the getter is pure: repeated calls yield identical content and never drain
        match (errs.get_errors(), errs.get_errors()) {
            (OptionStringVec::Some(a), OptionStringVec::Some(b)) => {
                assert_eq!(a.as_slice(), b.as_slice());
                assert_eq!(a.len(), 2);
                assert_eq!(a.as_slice()[1].as_str(), "\u{1F600}");
            }
            _ => panic!("Errors(_) must yield Some"),
        }

        // invariant: is_ok() <=> get_errors() is None
        for r in [FluentSyntaxCheckResult::Ok, errs] {
            let is_none = matches!(r.get_errors(), OptionStringVec::None);
            assert_eq!(r.is_ok(), is_none);
        }
    }

    // ------------------------------------------------------------------
    // looks_like_locale  (private helper)
    // ------------------------------------------------------------------

    #[test]
    fn autotest_looks_like_locale_edge_and_garbage_inputs() {
        for good in ["en", "de", "eng", "en-US", "zh-Hans-CN", "EN-us"] {
            assert!(looks_like_locale(good), "{good:?} should look like a locale");
        }
        // first subtag must be 2-3 ASCII letters -- everything else is rejected
        for bad in [
            "", "e", "abcd", "main", "1234", "e2", "-", "-en", " en", "en ", "\u{1F600}", "0",
            "-0", "\t", "\u{0301}en",
        ] {
            assert!(!looks_like_locale(bad), "{bad:?} must be rejected");
        }

        // LAXNESS (pinned, not endorsed): only the *first* subtag is inspected, so any
        // 2-3 letter word passes -- including numeric-looking literals and source dirs...
        assert!(looks_like_locale("inf"));
        assert!(looks_like_locale("NaN"));
        assert!(looks_like_locale("src"));
        // ...and everything after the first '-' is never validated at all.
        assert!(looks_like_locale("en-!!!!-\u{1F600}"));

        // huge inputs terminate immediately (length check on the first subtag)
        assert!(!looks_like_locale(&"a".repeat(1_000_000)));
        assert!(!looks_like_locale(&"a-".repeat(100_000)));
        assert!(looks_like_locale(&format!("en-{}", "US-".repeat(100_000))));
    }

    // ------------------------------------------------------------------
    // extract_locale_from_path  (parser)
    // ------------------------------------------------------------------

    #[test]
    fn autotest_extract_locale_from_path_rejects_junk_without_panic() {
        // empty / whitespace / garbage / unicode -> None
        for none in [
            "",
            "   ",
            "\t\n",
            "/",
            "//////",
            "!!!.fluent",
            "1234.fluent",
            ".fluent",
            "\u{1F600}/\u{0301}.fluent",
            "valid;garbage.fluent",
        ] {
            assert_eq!(extract_locale_from_path(none), None, "{none:?}");
        }

        // positive controls (the documented cases)
        assert_eq!(
            extract_locale_from_path("en-US.fluent"),
            Some("en-US".to_string())
        );
        assert_eq!(
            extract_locale_from_path("/en-US/main.fluent"),
            Some("en-US".to_string())
        );
        assert_eq!(
            extract_locale_from_path("locales/de-DE/errors.fluent"),
            Some("de-DE".to_string())
        );

        // SHARP EDGE: the directory wins, the filename is ignored entirely
        assert_eq!(
            extract_locale_from_path("de-DE/en-US.fluent"),
            Some("de-DE".to_string())
        );
        // SHARP EDGE: any 2-3 letter directory is taken for a locale
        assert_eq!(
            extract_locale_from_path("src/hello.fluent"),
            Some("src".to_string())
        );
        // SHARP EDGE: trim_end_matches strips *every* trailing ".fluent", not just one
        assert_eq!(
            extract_locale_from_path("en.fluent.fluent.fluent"),
            Some("en".to_string())
        );
        // no extension is required at all
        assert_eq!(extract_locale_from_path("fr"), Some("fr".to_string()));
        assert_eq!(
            extract_locale_from_path("fr-FR/anything.txt"),
            Some("fr-FR".to_string())
        );

        // 1 MiB of path segments: linear, no hang
        let long = format!("{}x.fluent", "a/".repeat(200_000));
        assert_eq!(extract_locale_from_path(&long), None);
        // 10_000 traversal segments are silently ignored
        let deep = format!("{}en-US/x.fluent", "../".repeat(10_000));
        assert_eq!(extract_locale_from_path(&deep), Some("en-US".to_string()));

        // deterministic across calls
        assert_eq!(
            extract_locale_from_path("en-US.fluent"),
            extract_locale_from_path("en-US.fluent")
        );
    }

    // ------------------------------------------------------------------
    // FluentLocaleBundle::new  (parser, private)
    // ------------------------------------------------------------------

    #[test]
    fn autotest_locale_bundle_new_tracks_langid_parse_and_never_panics() {
        let long_lang = "a".repeat(1_000_000);
        let many_subtags = format!("en-{}", "US-".repeat(50_000));
        let cases: Vec<&str> = vec![
            "",
            " ",
            "   ",
            "\t\n",
            "!!!",
            "en US",
            "en-US;garbage",
            "  en-US  ",
            "0",
            "-0",
            "9223372036854775807",
            "NaN",
            "inf",
            "1e309",
            "\u{1F600}",
            "e\u{0301}n-US",
            "en\u{0000}US",
            "en-US\n",
            "en",
            "en-US",
            "zh-Hans-CN",
            "EN-us",
            &long_lang,
            &many_subtags,
        ];
        for c in cases {
            // invariant: a bundle exists exactly when unic-langid accepts the tag,
            // and the outcome is deterministic across calls
            let parses = c.parse::<LanguageIdentifier>().is_ok();
            assert_eq!(
                FluentLocaleBundle::new(c).is_some(),
                parses,
                "bundle/langid disagree for {c:?}"
            );
            assert_eq!(FluentLocaleBundle::new(c).is_some(), parses, "{c:?}");
        }

        // positive controls
        assert!(FluentLocaleBundle::new("en-US").is_some());
        assert!(FluentLocaleBundle::new("en").is_some());
        assert!(FluentLocaleBundle::new("zh-Hans-CN").is_some());
        // unambiguous garbage
        assert!(FluentLocaleBundle::new("!!!").is_none());
        assert!(FluentLocaleBundle::new("en US").is_none());
        assert!(FluentLocaleBundle::new("\u{1F600}").is_none());
        assert!(FluentLocaleBundle::new(&long_lang).is_none());
    }

    // ------------------------------------------------------------------
    // FluentLocaleBundle::add_resource  (parser, private)
    // ------------------------------------------------------------------

    #[test]
    fn autotest_locale_bundle_add_resource_edge_sources() {
        let mut b = FluentLocaleBundle::new("en-US").expect("en-US");

        // an empty source is a valid (empty) FTL resource
        assert!(b.add_resource("").is_ok());
        assert!(b.get_message_ids().is_empty());

        // blank *lines* are valid FTL...
        assert!(b.add_resource("   \n").is_ok());
        assert!(b.add_resource("\n\n\n").is_ok());
        // ...but FTL's blank_inline is SPACE-only and must be terminated by an EOL, so
        // trailing spaces with no newline -- and any tab at all -- are junk.
        assert!(b.add_resource("   ").is_err());
        assert!(b.add_resource("\t").is_err());
        assert!(b.add_resource("\t\n").is_err());

        // garbage / malformed / truncated: Err, never a panic
        for junk in [
            "!!!",
            "= no id",
            "\u{1F600} = x",
            "{",
            "}",
            "hello =",
            "hello = { $x",
            "hello = { $x ->",
            "\0",
            "\u{FEFF}hello = Hello",
        ] {
            assert!(b.add_resource(junk).is_err(), "{junk:?} must not parse");
        }

        // a rejected source is not recorded
        assert!(b.get_message_ids().is_empty());

        // 1 MiB of valid FTL parses without hanging
        let mut big = String::with_capacity(1_200_000);
        for i in 0..40_000 {
            big.push_str(&format!("m{i} = value {i}\n"));
        }
        assert!(b.add_resource(&big).is_ok());
        assert_eq!(b.get_message_ids().len(), 40_000);
        assert!(b.has_message("m39999"));
    }

    // ------------------------------------------------------------------
    // FluentLocaleBundle::format / has_message / get_message_ids
    // ------------------------------------------------------------------

    #[test]
    fn autotest_locale_bundle_format_and_ids_positive_control() {
        let mut b = FluentLocaleBundle::new("en-US").expect("en-US");
        assert!(b
            .add_resource("hello = Hello\ngreeting = Hi, { $name }!\n")
            .is_ok());

        assert!(b.has_message("hello"));
        assert!(!b.has_message("nope"));
        assert!(!b.has_message(""));
        assert!(!b.has_message("\u{1F600}"));
        assert!(!b.has_message(&"x".repeat(100_000)));

        assert_eq!(
            b.format("hello", &FmtArgVec::new()).as_deref(),
            Some("Hello")
        );
        assert_eq!(b.format("missing", &FmtArgVec::new()), None);
        assert_eq!(b.format("", &FmtArgVec::new()), None);
        assert_eq!(b.format(&"x".repeat(100_000), &FmtArgVec::new()), None);

        // a supplied arg the message never uses is ignored
        let unused = args_of(vec![("unused", FmtValue::Str(AzString::from("z")))]);
        assert_eq!(b.format("hello", &unused).as_deref(), Some("Hello"));

        // a *missing* arg is rendered as the literal placeholder, never a panic
        assert_eq!(
            b.format("greeting", &FmtArgVec::new()).as_deref(),
            Some("Hi, {$name}!")
        );
        // isolation marks are disabled, so the substitution is byte-exact
        let named = args_of(vec![("name", FmtValue::Str(AzString::from("Alice")))]);
        assert_eq!(
            b.format("greeting", &named).as_deref(),
            Some("Hi, Alice!")
        );

        assert_eq!(
            b.get_message_ids(),
            vec!["hello".to_string(), "greeting".to_string()]
        );
    }

    #[test]
    fn autotest_locale_bundle_format_numeric_extremes_do_not_panic() {
        let mut b = FluentLocaleBundle::new("en-US").expect("en-US");
        assert!(b.add_resource("v = ({ $v })\n").is_ok());

        let cases: Vec<FmtValue> = vec![
            FmtValue::Bool(true),
            FmtValue::Bool(false),
            FmtValue::Uchar(0),
            FmtValue::Uchar(u8::MAX),
            FmtValue::Schar(i8::MIN),
            FmtValue::Schar(i8::MAX),
            FmtValue::Ushort(u16::MAX),
            FmtValue::Sshort(i16::MIN),
            FmtValue::Uint(0),
            FmtValue::Uint(u32::MAX),
            FmtValue::Sint(i32::MIN),
            FmtValue::Sint(i32::MAX),
            FmtValue::Ulong(u64::MAX),
            FmtValue::Slong(i64::MIN),
            FmtValue::Slong(i64::MAX),
            FmtValue::Isize(isize::MIN),
            FmtValue::Isize(isize::MAX),
            FmtValue::Usize(0),
            FmtValue::Usize(usize::MAX),
            FmtValue::Float(f32::NAN),
            FmtValue::Float(f32::INFINITY),
            FmtValue::Float(f32::NEG_INFINITY),
            FmtValue::Float(f32::MIN_POSITIVE),
            FmtValue::Float(-0.0),
            FmtValue::Double(f64::NAN),
            FmtValue::Double(f64::INFINITY),
            FmtValue::Double(f64::NEG_INFINITY),
            FmtValue::Double(f64::MAX),
            FmtValue::Double(f64::MIN_POSITIVE),
            FmtValue::Double(-0.0),
            FmtValue::Str(AzString::from("")),
            FmtValue::Str(AzString::from("\u{1F600}e\u{0301}")),
            FmtValue::Str(AzString::from("{ $v }")),
            FmtValue::StrVec(StringVec::from_vec(Vec::new())),
            FmtValue::StrVec(StringVec::from_vec(vec![
                AzString::from("a"),
                AzString::from("b"),
            ])),
        ];
        for v in cases {
            let out = b
                .format("v", &args_of(vec![("v", v.clone())]))
                .expect("message `v` exists, so format() must return Some");
            assert!(
                out.starts_with('(') && out.ends_with(')'),
                "{v:?} formatted to {out:?}"
            );
        }

        // deterministic values
        assert_eq!(
            b.format("v", &args_of(vec![("v", FmtValue::Double(42.0))]))
                .as_deref(),
            Some("(42)")
        );
        assert_eq!(
            b.format("v", &args_of(vec![("v", FmtValue::Bool(true))]))
                .as_deref(),
            Some("(true)")
        );
        assert_eq!(
            b.format("v", &args_of(vec![("v", FmtValue::Slong(1_234_567))]))
                .as_deref(),
            Some("(1234567)")
        );
        // a string vec is joined with ", "
        assert_eq!(
            b.format(
                "v",
                &args_of(vec![(
                    "v",
                    FmtValue::StrVec(StringVec::from_vec(vec![
                        AzString::from("a"),
                        AzString::from("b"),
                    ]))
                )])
            )
            .as_deref(),
            Some("(a, b)")
        );

        // PRECISION LOSS (pinned): every integer arg is funnelled through `as f64`,
        // so values above 2^53 are silently rounded -- i64::MAX does NOT round-trip.
        let out = b
            .format("v", &args_of(vec![("v", FmtValue::Slong(i64::MAX))]))
            .expect("some");
        assert_ne!(out, format!("({})", i64::MAX));
        assert_eq!(out, "(9223372036854775808)");
    }

    #[test]
    fn autotest_locale_bundle_format_select_expression_nan_and_inf() {
        let mut b = FluentLocaleBundle::new("en-US").expect("en-US");
        assert!(b
            .add_resource("emails = { $count ->\n    [one] one email\n   *[other] many emails\n}\n")
            .is_ok());

        assert_eq!(
            b.format("emails", &args_of(vec![("count", FmtValue::Uint(1))]))
                .as_deref(),
            Some("one email")
        );
        assert_eq!(
            b.format("emails", &args_of(vec![("count", FmtValue::Uint(5))]))
                .as_deref(),
            Some("many emails")
        );
        // NaN / +-inf drive the CLDR plural-operand conversion; they must fall through
        // to the default variant rather than panicking inside intl_pluralrules.
        for weird in [
            FmtValue::Double(f64::NAN),
            FmtValue::Double(f64::INFINITY),
            FmtValue::Double(f64::NEG_INFINITY),
            FmtValue::Double(f64::MAX),
            FmtValue::Double(f64::MIN_POSITIVE),
            FmtValue::Float(f32::NAN),
            FmtValue::Slong(i64::MIN),
            FmtValue::Ulong(u64::MAX),
        ] {
            assert_eq!(
                b.format("emails", &args_of(vec![("count", weird.clone())]))
                    .as_deref(),
                Some("many emails"),
                "{weird:?}"
            );
        }
    }

    #[test]
    fn autotest_locale_bundle_format_duplicate_and_bulk_args() {
        let mut b = FluentLocaleBundle::new("en-US").expect("en-US");
        assert!(b.add_resource("v = ({ $v })\n").is_ok());

        // duplicate keys: last write wins (FluentArgs::set overwrites)
        let dup = FmtArgVec::from_vec(vec![
            FmtArg {
                key: AzString::from("v"),
                value: FmtValue::Sint(1),
            },
            FmtArg {
                key: AzString::from("v"),
                value: FmtValue::Sint(2),
            },
        ]);
        assert_eq!(b.format("v", &dup).as_deref(), Some("(2)"));

        // an empty key is legal on the args side but can never be referenced
        let empty_key = args_of(vec![("", FmtValue::Sint(7))]);
        assert_eq!(b.format("v", &empty_key).as_deref(), Some("({$v})"));

        // 10_000 unused args: no panic, no blow-up
        let many = FmtArgVec::from_vec(
            (0..10_000usize)
                .map(|i| FmtArg {
                    key: AzString::from(format!("k{i}")),
                    value: FmtValue::Usize(i),
                })
                .collect::<Vec<_>>(),
        );
        assert_eq!(b.format("v", &many).as_deref(), Some("({$v})"));
    }

    // ------------------------------------------------------------------
    // get_error_position  (numeric)
    // ------------------------------------------------------------------

    #[test]
    fn autotest_get_error_position_boundaries_and_saturation() {
        // zero offset / empty source
        assert_eq!(get_error_position("", 0), (1, 1));
        // an offset far past the end saturates at the final position, never panics
        assert_eq!(get_error_position("", usize::MAX), (1, 1));
        assert_eq!(get_error_position("abc", 0), (1, 1));
        assert_eq!(get_error_position("abc", 1), (1, 2));
        assert_eq!(get_error_position("abc", 3), (1, 4));
        assert_eq!(get_error_position("abc", usize::MAX), (1, 4));

        // '\n' terminates the line it sits on; the column resets afterwards
        assert_eq!(get_error_position("a\nb", 1), (1, 2));
        assert_eq!(get_error_position("a\nb", 2), (2, 1));
        assert_eq!(get_error_position("a\nb", 3), (2, 2));
        assert_eq!(get_error_position("\n\n\n", usize::MAX), (4, 1));
        // CRLF: '\r' is an ordinary column, only '\n' breaks the line
        assert_eq!(get_error_position("a\r\nb", 3), (2, 1));

        // offsets are BYTES but columns are CHARS: an offset landing inside a
        // multi-byte char resolves to the next boundary instead of slicing/panicking
        assert_eq!(get_error_position("\u{1F600}x", 0), (1, 1));
        assert_eq!(get_error_position("\u{1F600}x", 1), (1, 2)); // mid-emoji
        assert_eq!(get_error_position("\u{1F600}x", 3), (1, 2)); // mid-emoji
        assert_eq!(get_error_position("\u{1F600}x", 4), (1, 2));
        assert_eq!(get_error_position("\u{1F600}x", 5), (1, 3));
        // combining marks occupy their own column
        assert_eq!(get_error_position("e\u{0301}", usize::MAX), (1, 3));

        // large inputs: linear scan, no u32 overflow at these sizes
        assert_eq!(
            get_error_position(&"a".repeat(200_000), usize::MAX),
            (1, 200_001)
        );
        assert_eq!(
            get_error_position(&"\n".repeat(200_000), usize::MAX),
            (200_001, 1)
        );
    }

    // ------------------------------------------------------------------
    // check_fluent_syntax / check_fluent_syntax_bytes
    // ------------------------------------------------------------------

    #[test]
    fn autotest_check_fluent_syntax_edge_sources() {
        // an empty source is valid FTL
        assert!(check_fluent_syntax("").is_ok());
        assert!(check_fluent_syntax("hello = Hello, world!").is_ok());
        assert!(check_fluent_syntax("# just a comment\n").is_ok());
        // blank lines are valid...
        assert!(check_fluent_syntax("   \n").is_ok());
        assert!(check_fluent_syntax("\n\n\n").is_ok());
        // ...but unterminated trailing spaces and tabs are junk
        assert!(!check_fluent_syntax("   ").is_ok());
        assert!(!check_fluent_syntax("\t\n").is_ok());

        // garbage / unicode / control chars: deterministic, never a panic
        for src in [
            "\u{1F600}",
            "\u{0301}",
            "\0\0\0",
            "= = =",
            "{{{{",
            "]]]]",
            "-",
            "hello = { $x",
            "valid = 1;garbage",
        ] {
            let a = check_fluent_syntax(src).is_ok();
            let b = check_fluent_syntax(src).is_ok();
            assert_eq!(a, b, "{src:?} must be deterministic");
        }

        // errors are prefixed with the 1-based "line:column: " of the failure
        let r = check_fluent_syntax("ok = 1\nbad line here\n");
        match r.get_errors() {
            OptionStringVec::Some(e) => {
                assert!(!e.is_empty());
                assert!(
                    e.as_slice()[0].as_str().starts_with("2:"),
                    "{:?}",
                    e.as_slice()[0]
                );
            }
            OptionStringVec::None => panic!("expected a syntax error"),
        }

        // 400 KB of valid FTL: parses without hanging
        let mut big = String::with_capacity(600_000);
        for i in 0..40_000 {
            big.push_str(&format!("m{i} = v{i}\n"));
        }
        assert!(check_fluent_syntax(&big).is_ok());

        // many errors in a long source still terminate (note: each error re-scans the
        // whole source to compute its line/column, so this is O(source x errors))
        let mut junky = String::new();
        for i in 0..2_000 {
            junky.push_str(&format!("ok{i} = v\n!!!\n"));
        }
        assert!(!check_fluent_syntax(&junky).is_ok());
    }

    #[test]
    fn autotest_check_fluent_syntax_bytes_invalid_utf8_and_bom() {
        assert!(check_fluent_syntax_bytes(b"").is_ok());
        assert!(check_fluent_syntax_bytes(b"hello = Hello").is_ok());

        // invalid UTF-8 is reported as a single 0:0 error, not a panic
        let r = check_fluent_syntax_bytes(&[0xFF, 0xFE, 0x00]);
        match r.get_errors() {
            OptionStringVec::Some(e) => {
                assert_eq!(e.len(), 1);
                assert!(e.as_slice()[0].as_str().starts_with("0:0: Invalid UTF-8"));
            }
            OptionStringVec::None => panic!("invalid UTF-8 must be an error"),
        }
        // truncated multi-byte sequence
        assert!(!check_fluent_syntax_bytes(&[0xE2, 0x82]).is_ok());

        // SHARP EDGE: a UTF-8 BOM is *not* stripped, so a BOM-prefixed .ftl is junk
        assert!(!check_fluent_syntax_bytes("\u{FEFF}hello = Hello".as_bytes()).is_ok());
    }

    #[test]
    fn autotest_check_fluent_syntax_deep_placeable_nesting_does_not_overflow() {
        // The recursive-descent parser has no depth guard, so nesting depth maps 1:1 to
        // stack frames. Parse 1_000 nested placeables on a 32 MiB stack and require the
        // verdict to match the shallow, obviously-valid form.
        let shallow = check_fluent_syntax("m = {{{\"x\"}}}").is_ok();
        let depth = 1_000;
        let src = format!("m = {}\"x\"{}", "{".repeat(depth), "}".repeat(depth));
        let deep = std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(move || check_fluent_syntax(&src).is_ok())
            .expect("spawn")
            .join()
            .expect("parser must not crash on deeply nested placeables");
        assert_eq!(deep, shallow, "nesting depth must not change validity");
    }

    // ------------------------------------------------------------------
    // FluentLocalizerHandle: create / inner / default locale / Clone / Drop
    // ------------------------------------------------------------------

    #[test]
    fn autotest_localizer_default_locale_roundtrip_and_garbage() {
        let h = FluentLocalizerHandle::create("en-US");
        assert_eq!(h.get_default_locale().as_str(), "en-US");

        // create() validates nothing: any string is stored verbatim
        for junk in ["", "   ", "!!!", "\u{1F600}", "en\0US", "\u{0301}"] {
            let g = FluentLocalizerHandle::create(junk);
            assert_eq!(g.get_default_locale().as_str(), junk);
        }
        let long = "x".repeat(100_000);
        let g = FluentLocalizerHandle::create(&long);
        assert_eq!(g.get_default_locale().as_str(), long.as_str());

        // set_default_locale overwrites, and clones share one inner allocation
        h.set_default_locale("de-DE");
        assert_eq!(h.get_default_locale().as_str(), "de-DE");
        let c = h.clone();
        c.set_default_locale("fr-FR");
        assert_eq!(h.get_default_locale().as_str(), "fr-FR");
        assert!(core::ptr::eq(h.inner(), c.inner()));

        assert_eq!(
            FluentLocalizerHandle::default().get_default_locale().as_str(),
            "en-US"
        );
        // Debug takes the same mutex the getter does -- it must not deadlock
        assert!(format!("{h:?}").contains("fr-FR"));
    }

    #[test]
    fn autotest_localizer_clone_drop_refcount_is_sound() {
        let h = FluentLocalizerHandle::create("en-US");
        assert!(h.add_resource("en-US", "k = v\n"));

        // 1_000 clones, all dropped: the shared inner must survive
        let clones: Vec<FluentLocalizerHandle> = (0..1_000).map(|_| h.clone()).collect();
        for c in &clones {
            assert_eq!(tr(c, "en-US", "k"), "v");
        }
        drop(clones);
        assert_eq!(tr(&h, "en-US", "k"), "v");

        // the handle is Send + Sync: hammer it from 8 threads
        let shared = h.clone();
        let workers: Vec<_> = (0..8)
            .map(|i| {
                let s = shared.clone();
                std::thread::spawn(move || {
                    for _ in 0..50 {
                        s.add_resource(&format!("l{i}"), &format!("m{i} = v{i}\n"));
                        let _ = s.get_loaded_locales();
                        let _ = s.get_language_info();
                        let _ = tr(&s, "en-US", "k");
                        let _ = s.clone();
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().expect("worker thread panicked");
        }
        assert_eq!(tr(&h, "en-US", "k"), "v");
    }

    // ------------------------------------------------------------------
    // FluentLocalizerHandle::add_resource / add_resource_from_bytes
    // ------------------------------------------------------------------

    #[test]
    fn autotest_localizer_invalid_locale_silently_falls_back_to_an_en_us_bundle() {
        let h = FluentLocalizerHandle::create("en-US");

        // "!!!" is not a BCP-47 tag: FluentLocaleBundle::new() returns None and the code
        // substitutes an en-US bundle -- stored under the bogus key, reported as success.
        assert!(h.add_resource("!!!", "k = v\n"));
        assert!(h.has_message("!!!", "k"));
        assert_eq!(
            h.get_loaded_locales()
                .iter()
                .map(|s| s.as_str().to_string())
                .collect::<Vec<_>>(),
            vec!["!!!".to_string()]
        );
        assert_eq!(tr(&h, "!!!", "k"), "v");

        // same for the empty locale and for unicode junk
        assert!(h.add_resource("", "e = 1\n"));
        assert!(h.has_message("", "e"));
        assert!(h.add_resource("\u{1F600}", "u = 2\n"));
        assert!(h.has_message("\u{1F600}", "u"));

        // malformed FTL is still rejected
        assert!(!h.add_resource("en-US", "!!!"));
        assert!(!h.add_resource("en-US", "hello ="));
        // an empty source is accepted (it is an empty resource)
        assert!(h.add_resource("en-US", ""));
    }

    #[test]
    fn autotest_localizer_add_resource_from_bytes_rejects_invalid_utf8() {
        let h = FluentLocalizerHandle::create("en-US");
        assert!(h.add_resource_from_bytes("en-US", b"k = v\n"));
        assert!(h.has_message("en-US", "k"));
        assert!(h.add_resource_from_bytes("en-US", b""));

        // invalid UTF-8 -> false, nothing registered, no panic
        assert!(!h.add_resource_from_bytes("de-DE", &[0xFF, 0xFF]));
        assert!(!h.add_resource_from_bytes("de-DE", &[0xE2, 0x82]));
        assert!(!h.add_resource_from_bytes("de-DE", &[0x80]));
        // the failed locale was never even created
        assert!(!h.has_message("de-DE", "k"));
        assert_eq!(h.get_loaded_locales().len(), 1);
    }

    #[test]
    fn autotest_localizer_duplicate_ids_rejected_and_partial_adds_go_untracked() {
        let h = FluentLocalizerHandle::create("en-US");
        assert!(h.add_resource("en-US", "a = 1\n"));
        // re-defining an existing id is an override error -> false, first definition wins
        assert!(!h.add_resource("en-US", "a = 2\n"));
        assert_eq!(tr(&h, "en-US", "a"), "1");

        // BUG (pinned as current behaviour, NOT endorsed): a resource that redefines one
        // id but also introduces a NEW one is reported as a total failure and is never
        // pushed to `sources` -- yet fluent already registered the new id in the bundle.
        // So `b` is live for translate()/has_message() but invisible to
        // get_language_info(), and export_to_zip() silently drops it.
        assert!(!h.add_resource("en-US", "a = 3\nb = 9\n"));
        assert!(h.has_message("en-US", "b"));
        assert_eq!(tr(&h, "en-US", "b"), "9");

        let info = h.get_language_info();
        assert_eq!(info.len(), 1);
        let ids: Vec<&str> = info[0].message_ids.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            ids,
            vec!["a"],
            "`b` is live in the bundle but missing from the tracked sources"
        );
        assert_eq!(info[0].message_count, 1);

        // ...and it vanishes across an export/re-import round-trip
        let zip = export_to_zip(&h).expect("export");
        let h2 = FluentLocalizerHandle::create("en-US");
        let r = h2.load_from_zip(&zip);
        assert_eq!(r.files_failed, 0, "{:?}", r.errors);
        assert!(h2.has_message("en-US", "a"));
        assert!(
            !h2.has_message("en-US", "b"),
            "round-trip silently drops the partially-added message"
        );
    }

    // ------------------------------------------------------------------
    // translate / try_translate / fallback chain
    // ------------------------------------------------------------------

    #[test]
    fn autotest_translate_fallback_chain_terminates_on_cycles() {
        let h = FluentLocalizerHandle::create("en-US");
        assert!(h.add_resource("en-US", "only-en = EN\n"));
        assert!(h.add_resource("de-DE", "only-de = DE\n"));
        assert!(h.add_resource("de-CH", "only-ch = CH\n"));
        h.set_fallback_chain("de-CH", &["de-DE", "en-US"]);

        assert_eq!(tr(&h, "de-CH", "only-ch"), "CH"); // direct hit
        assert_eq!(tr(&h, "de-CH", "only-de"), "DE"); // first fallback
        assert_eq!(tr(&h, "de-CH", "only-en"), "EN"); // second fallback

        // nothing anywhere -> the message id itself is echoed back
        assert_eq!(tr(&h, "de-CH", "missing"), "missing");
        assert_eq!(tr(&h, "de-CH", ""), "");
        assert_eq!(tr(&h, "de-CH", "\u{1F600}"), "\u{1F600}");
        assert_eq!(tr(&h, "de-CH", "  spaced  "), "  spaced  "); // never trimmed

        // an entirely unknown locale still reaches the default locale
        assert_eq!(tr(&h, "xx-XX", "only-en"), "EN");
        assert_eq!(tr(&h, "", "only-en"), "EN");
        assert_eq!(tr(&h, &"z".repeat(100_000), "only-en"), "EN");

        // an empty chain is a no-op (the default locale still answers, or the id echoes)
        h.set_fallback_chain("de-CH", &[]);
        assert_eq!(tr(&h, "de-CH", "only-de"), "only-de");
        assert_eq!(tr(&h, "de-CH", "only-en"), "EN"); // via the default locale

        // a 10_000-entry chain is walked linearly, then the default locale answers
        let many: Vec<String> = (0..10_000).map(|i| format!("l{i}")).collect();
        let refs: Vec<&str> = many.iter().map(|s| s.as_str()).collect();
        h.set_fallback_chain("zz-ZZ", &refs);
        assert_eq!(tr(&h, "zz-ZZ", "only-en"), "EN");
    }

    #[test]
    fn autotest_translate_fallback_chain_is_one_hop_and_cycle_safe() {
        // default locale deliberately has no bundle, so only the chain can answer
        let g = FluentLocalizerHandle::create("qq-QQ");
        assert!(g.add_resource("en-US", "only-en = EN\n"));
        assert!(g.add_resource("de-DE", "only-de = DE\n"));
        assert!(g.add_resource("de-CH", "only-ch = CH\n"));
        g.set_fallback_chain("de-CH", &["de-DE"]);
        g.set_fallback_chain("de-DE", &["en-US"]);

        assert_eq!(tr(&g, "de-CH", "only-ch"), "CH"); // direct
        assert_eq!(tr(&g, "de-CH", "only-de"), "DE"); // one hop
        // the chain is NOT transitive: de-CH -> de-DE -> en-US never reaches en-US
        assert_eq!(tr(&g, "de-CH", "only-en"), "only-en");
        // an unreachable default locale simply means the id is echoed back
        assert_eq!(tr(&g, "qq-QQ", "only-en"), "only-en");

        // self-referential and mutually-recursive chains terminate instead of looping
        g.set_fallback_chain("de-DE", &["de-DE"]);
        assert_eq!(tr(&g, "de-DE", "missing"), "missing");
        g.set_fallback_chain("de-CH", &["de-DE"]);
        g.set_fallback_chain("de-DE", &["de-CH"]);
        assert_eq!(tr(&g, "de-CH", "missing"), "missing");
        assert_eq!(tr(&g, "de-DE", "missing"), "missing");
        // ...and the one legal hop across the cycle still resolves
        assert_eq!(tr(&g, "de-CH", "only-de"), "DE");
        assert_eq!(tr(&g, "de-DE", "only-ch"), "CH");
    }

    #[test]
    fn autotest_try_translate_is_strict_about_the_locale_key() {
        let h = FluentLocalizerHandle::create("en-US");
        assert!(h.add_resource("en-US", "k = v\n"));

        let empty = FmtArgVec::new();
        assert_eq!(
            h.try_translate("en-US", "k", &empty).map(|s| s.as_str().to_string()),
            Some("v".to_string())
        );
        // no fallback, no normalisation, no trimming: the locale key is a raw string
        assert_eq!(h.try_translate("en", "k", &empty), None);
        assert_eq!(h.try_translate("EN-US", "k", &empty), None);
        assert_eq!(h.try_translate(" en-US ", "k", &empty), None);
        assert_eq!(h.try_translate("", "k", &empty), None);
        assert_eq!(h.try_translate("\u{1F600}", "k", &empty), None);
        assert_eq!(h.try_translate(&"x".repeat(100_000), "k", &empty), None);
        // unknown message in a known locale
        assert_eq!(h.try_translate("en-US", "nope", &empty), None);
        assert_eq!(h.try_translate("en-US", "", &empty), None);
    }

    // ------------------------------------------------------------------
    // has_message / get_loaded_locales / get_language_info / clear_*
    // ------------------------------------------------------------------

    #[test]
    fn autotest_localizer_has_message_edge_inputs() {
        let h = FluentLocalizerHandle::create("en-US");
        assert!(h.add_resource("en-US", "k = v\n"));

        assert!(h.has_message("en-US", "k"));
        assert!(!h.has_message("en-US", "K")); // case-sensitive
        assert!(!h.has_message("en-US", "k ")); // never trimmed
        assert!(!h.has_message("en-US", " k"));
        assert!(!h.has_message("en-US", ""));
        assert!(!h.has_message("", "k")); // unknown locale
        assert!(!h.has_message("EN-US", "k")); // locale keys are NOT normalised
        assert!(!h.has_message("en-US", &"k".repeat(100_000)));
        assert!(!h.has_message(&"x".repeat(100_000), "k"));

        // has_message is a pure lookup and must never create a bundle
        assert_eq!(h.get_loaded_locales().len(), 1);
    }

    #[test]
    fn autotest_localizer_getters_on_an_empty_instance() {
        let h = FluentLocalizerHandle::create("en-US");
        assert!(h.get_loaded_locales().is_empty());
        assert!(h.get_language_info().is_empty());
        assert_eq!(h.get_default_locale().as_str(), "en-US");
        assert!(!h.has_message("en-US", "anything"));
        assert_eq!(tr(&h, "en-US", "anything"), "anything");
        // exporting nothing yields a valid (empty) archive
        let empty_zip = export_to_zip(&h).expect("empty export");
        let r = h.load_from_zip(&empty_zip);
        assert_eq!((r.files_loaded, r.files_failed), (0, 0));

        // clearing an empty instance is a no-op, not a panic
        h.clear_locale("en-US");
        h.clear_locale("");
        h.clear_locale("\u{1F600}");
        h.clear_all();
        h.clear_all();
        assert!(h.get_loaded_locales().is_empty());
    }

    #[test]
    fn autotest_localizer_clear_locale_and_clear_all() {
        let h = FluentLocalizerHandle::create("en-US");
        assert!(h.add_resource("en-US", "k = v\n"));
        assert!(h.add_resource("de-DE", "k = w\n"));
        assert_eq!(h.get_loaded_locales().len(), 2);

        h.clear_locale("fr-FR"); // unknown locale: no-op
        assert_eq!(h.get_loaded_locales().len(), 2);
        h.clear_locale("de-DE");
        assert_eq!(h.get_loaded_locales().len(), 1);
        assert!(!h.has_message("de-DE", "k"));
        assert!(h.has_message("en-US", "k"));
        // clearing resources must not touch the default locale
        assert_eq!(h.get_default_locale().as_str(), "en-US");

        h.clear_all();
        assert!(h.get_loaded_locales().is_empty());
        assert!(h.get_language_info().is_empty());
        // translate degrades to echoing the id...
        assert_eq!(tr(&h, "en-US", "k"), "k");
        // ...and the bundle can be repopulated with a previously-duplicate id
        assert!(h.add_resource("en-US", "k = v2\n"));
        assert_eq!(tr(&h, "en-US", "k"), "v2");
    }

    #[test]
    fn autotest_localizer_get_language_info_reports_ids_per_locale() {
        let h = FluentLocalizerHandle::create("en-US");
        assert!(h.add_resource("en-US", "a = A\nb = B\n"));
        assert!(h.add_resource("de-DE", "c = C\n"));

        let info = h.get_language_info();
        assert_eq!(info.len(), 2);
        // BTreeMap ordering: "de-DE" sorts before "en-US"
        assert_eq!(info[0].locale.as_str(), "de-DE");
        assert_eq!(info[0].message_count, 1);
        assert_eq!(info[1].locale.as_str(), "en-US");
        assert_eq!(info[1].message_count, 2);
        // message_count must always agree with the id list it is derived from
        for i in &info {
            assert_eq!(i.message_count, i.message_ids.len());
        }
        let en: Vec<&str> = info[1].message_ids.iter().map(|s| s.as_str()).collect();
        assert_eq!(en, vec!["a", "b"]);
    }

    // ------------------------------------------------------------------
    // ZIP: create / export / load  (round-trip)
    // ------------------------------------------------------------------

    #[test]
    fn autotest_zip_export_import_roundtrip_is_lossless() {
        let h = FluentLocalizerHandle::create("en-US");
        // two separate sources for one locale -> exported as part_0 / part_1
        assert!(h.add_resource("de-DE", "a = A\n"));
        assert!(h.add_resource("de-DE", "b = B\n"));
        assert!(h.add_resource("en-US", "c = C\n"));

        let bytes = export_to_zip(&h).expect("export");
        let h2 = FluentLocalizerHandle::create("en-US");
        let res = h2.load_from_zip(&bytes);
        assert_eq!(res.files_failed, 0, "{:?}", res.errors);
        assert_eq!(res.files_loaded, 3);

        for (loc, id, want) in [("de-DE", "a", "A"), ("de-DE", "b", "B"), ("en-US", "c", "C")] {
            assert!(h2.has_message(loc, id), "{loc}/{id}");
            assert_eq!(tr(&h2, loc, id), want);
        }
        let mut locales: Vec<String> = h2
            .get_loaded_locales()
            .iter()
            .map(|s| s.as_str().to_string())
            .collect();
        locales.sort();
        assert_eq!(locales, vec!["de-DE".to_string(), "en-US".to_string()]);

        // exporting the re-imported handle produces the same message set again
        let bytes2 = export_to_zip(&h2).expect("re-export");
        let h3 = FluentLocalizerHandle::create("en-US");
        assert_eq!(h3.load_from_zip(&bytes2).files_loaded, 3);
        assert_eq!(tr(&h3, "de-DE", "b"), "B");
    }

    #[test]
    fn autotest_create_fluent_zip_edge_entries() {
        let h = FluentLocalizerHandle::create("en-US");

        // an empty archive is valid and loads zero files (and zero failures)
        let empty = create_fluent_zip(Vec::new()).expect("empty zip");
        let r = h.load_from_zip(&empty);
        assert_eq!((r.files_loaded, r.files_failed), (0, 0));
        let r = h.load_from_zip_bytes(&U8Vec::from_vec(empty.clone()));
        assert_eq!((r.files_loaded, r.files_failed), (0, 0));

        // non-.fluent members are skipped silently, not counted as failures
        let zip = create_fluent_zip_from_strings(vec![
            ("README.md".to_string(), "not a translation".to_string()),
            ("en-US.fluent".to_string(), "k = v\n".to_string()),
        ])
        .expect("zip");
        let r = h.load_from_zip(&zip);
        assert_eq!((r.files_loaded, r.files_failed), (1, 0));
        assert_eq!(tr(&h, "en-US", "k"), "v");

        // a .fluent whose path carries no locale is a failure; siblings still load
        let zip = create_fluent_zip_from_strings(vec![
            ("whatever-long-name.fluent".to_string(), "x = 1\n".to_string()),
            ("fr-FR.fluent".to_string(), "y = 2\n".to_string()),
        ])
        .expect("zip");
        let r = h.load_from_zip(&zip);
        assert_eq!(r.files_loaded, 1);
        assert_eq!(r.files_failed, 1);
        assert_eq!(r.errors.len(), 1);
        assert!(matches!(
            r.errors.as_slice()[0],
            FluentLoadError::UnknownLocale(_)
        ));
        assert_eq!(tr(&h, "fr-FR", "y"), "2");

        // ...and an explicit locale override rescues it
        let r = h.load_from_zip_bytes_with_locale(&U8Vec::from_vec(zip), "es-ES");
        assert_eq!(r.files_failed, 0, "{:?}", r.errors);
        assert_eq!(r.files_loaded, 2);
        assert_eq!(tr(&h, "es-ES", "x"), "1");
        assert_eq!(tr(&h, "es-ES", "y"), "2");

        // a member with broken FTL is a Parse failure
        let zip = create_fluent_zip_from_strings(vec![(
            "it-IT.fluent".to_string(),
            "!!! broken\n".to_string(),
        )])
        .expect("zip");
        let r = h.load_from_zip(&zip);
        assert_eq!((r.files_loaded, r.files_failed), (0, 1));
        assert!(matches!(r.errors.as_slice()[0], FluentLoadError::Parse(_)));
    }

    #[test]
    fn autotest_load_from_zip_rejects_malformed_archives() {
        let h = FluentLocalizerHandle::create("en-US");
        for data in [
            &b""[..],
            &b"not a zip at all"[..],
            &[0x50, 0x4B, 0x03, 0x04][..], // truncated local file header
            &[0xFFu8; 64][..],
        ] {
            let r = h.load_from_zip(data);
            assert_eq!(r.files_loaded, 0);
            assert_eq!(r.files_failed, 1);
            assert_eq!(r.errors.len(), 1);
            assert!(matches!(
                r.errors.as_slice()[0],
                FluentLoadError::OpenArchive(_)
            ));
        }

        // a 4 MiB non-zip blob is rejected promptly, not scanned forever
        let big = vec![0u8; 4 * 1024 * 1024];
        let r = h.load_from_zip(&big);
        assert_eq!((r.files_loaded, r.files_failed), (0, 1));
        assert!(h.get_loaded_locales().is_empty());
    }

    #[test]
    fn autotest_load_from_path_edge_cases() {
        let h = FluentLocalizerHandle::create("en-US");
        let dir = tmp_dir();

        // missing file -> ReadFile, no panic
        let missing = dir.join("en-US.fluent");
        let r = h.load_from_path(missing.to_str().expect("utf8"), None);
        assert_eq!((r.files_loaded, r.files_failed), (0, 1));
        assert!(matches!(r.errors.as_slice()[0], FluentLoadError::ReadFile(_)));

        // empty path, directory path, NUL-in-path, over-long path: all ReadFile, no panic
        for bad in [
            String::new(),
            dir.to_str().expect("utf8").to_string(),
            "a\0b.fluent".to_string(),
            format!("/{}.fluent", "p".repeat(9_000)),
        ] {
            let r = h.load_from_path(&bad, None);
            assert_eq!(r.files_failed, 1, "{bad:?}");
            assert_eq!(r.files_loaded, 0, "{bad:?}");
        }

        // a real .fluent whose stem is not a locale -> UnknownLocale ...
        let p = dir.join("translations.fluent");
        std::fs::write(&p, b"k = v\n").expect("write");
        let r = h.load_from_path(p.to_str().expect("utf8"), None);
        assert_eq!((r.files_loaded, r.files_failed), (0, 1));
        assert!(matches!(
            r.errors.as_slice()[0],
            FluentLoadError::UnknownLocale(_)
        ));
        // ... and an override rescues it
        let r = h.load_from_path(p.to_str().expect("utf8"), Some("de-DE"));
        assert_eq!((r.files_loaded, r.files_failed), (1, 0));
        assert_eq!(tr(&h, "de-DE", "k"), "v");

        // unknown extension -> UnknownExtension (the file is read first, then rejected)
        let t = dir.join("en-US.txt");
        std::fs::write(&t, b"k = v\n").expect("write");
        let r = h.load_from_path(t.to_str().expect("utf8"), None);
        assert_eq!((r.files_loaded, r.files_failed), (0, 1));
        assert!(matches!(
            r.errors.as_slice()[0],
            FluentLoadError::UnknownExtension(_)
        ));

        // no extension at all -> UnknownExtension
        let n = dir.join("en-US");
        std::fs::write(&n, b"k = v\n").expect("write");
        let r = h.load_from_path(n.to_str().expect("utf8"), None);
        assert_eq!((r.files_loaded, r.files_failed), (0, 1));
        assert!(matches!(
            r.errors.as_slice()[0],
            FluentLoadError::UnknownExtension(_)
        ));

        // invalid UTF-8 in a .ftl -> InvalidUtf8
        let u = dir.join("es-ES.ftl");
        std::fs::write(&u, [0xFFu8, 0xFE, 0xFD]).expect("write");
        let r = h.load_from_path(u.to_str().expect("utf8"), None);
        assert_eq!((r.files_loaded, r.files_failed), (0, 1));
        assert!(matches!(
            r.errors.as_slice()[0],
            FluentLoadError::InvalidUtf8(_)
        ));

        // broken FTL in a .ftl -> Parse
        let b = dir.join("fr-FR.ftl");
        std::fs::write(&b, b"!!! broken\n").expect("write");
        let r = h.load_from_path(b.to_str().expect("utf8"), None);
        assert_eq!((r.files_loaded, r.files_failed), (0, 1));
        assert!(matches!(r.errors.as_slice()[0], FluentLoadError::Parse(_)));

        // a real .zip on disk loads through the same path
        let z = dir.join("pack.zip");
        let zip = create_fluent_zip_from_strings(vec![(
            "it-IT.fluent".to_string(),
            "k = v\n".to_string(),
        )])
        .expect("zip");
        std::fs::write(&z, &zip).expect("write");
        let r = h.load_from_path(z.to_str().expect("utf8"), None);
        assert_eq!((r.files_loaded, r.files_failed), (1, 0));
        assert_eq!(tr(&h, "it-IT", "k"), "v");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
