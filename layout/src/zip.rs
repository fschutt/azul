//! ZIP file manipulation module for C API exposure
//!
//! Provides a ZipFile struct for reading/writing ZIP archives.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

#[cfg(feature = "std")]
use std::path::Path;

// ============================================================================
// Configuration types
// ============================================================================

/// Configuration for reading ZIP archives
#[derive(Copy, Debug, Clone, Default)]
#[repr(C)]
pub struct ZipReadConfig {
    /// Maximum file size to extract (0 = unlimited)
    pub max_file_size: u64,
    /// Whether to allow paths with ".." (path traversal) - default: false
    pub allow_path_traversal: bool,
    /// Whether to skip encrypted files instead of erroring - default: false  
    pub skip_encrypted: bool,
}

impl ZipReadConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub const fn with_max_file_size(mut self, max_size: u64) -> Self {
        self.max_file_size = max_size;
        self
    }

    #[must_use]
    pub const fn with_allow_path_traversal(mut self, allow: bool) -> Self {
        self.allow_path_traversal = allow;
        self
    }
}

/// Configuration for writing ZIP archives
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ZipWriteConfig {
    /// Compression method: 0 = Store (no compression), 1 = Deflate
    pub compression_method: u8,
    /// Compression level (0-9, only for Deflate)
    pub compression_level: u8,
    /// Unix permissions for files (default: 0o644)
    pub unix_permissions: u32,
    /// Archive comment
    pub comment: String,
}

impl Default for ZipWriteConfig {
    fn default() -> Self {
        Self {
            compression_method: 1, // Deflate
            compression_level: 6,  // Default compression
            unix_permissions: 0o644,
            comment: String::new(),
        }
    }
}

impl ZipWriteConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn store() -> Self {
        Self {
            compression_method: 0,
            compression_level: 0,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn deflate(level: u8) -> Self {
        Self {
            compression_method: 1,
            compression_level: level.min(9),
            ..Default::default()
        }
    }

    #[must_use]
    pub fn with_comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = comment.into();
        self
    }
}

// ============================================================================
// Entry types
// ============================================================================

/// Path entry in a ZIP archive (metadata only, no data)
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ZipPathEntry {
    /// File path within the archive
    pub path: String,
    /// Whether this is a directory
    pub is_directory: bool,
    /// Uncompressed size in bytes
    pub size: u64,
    /// Compressed size in bytes
    pub compressed_size: u64,
    /// CRC32 checksum
    pub crc32: u32,
}

/// Vec of `ZipPathEntry`
pub type ZipPathEntryVec = Vec<ZipPathEntry>;

/// File entry in a ZIP archive (with data, for writing)
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ZipFileEntry {
    /// File path within the archive
    pub path: String,
    /// File contents (empty for directories)
    pub data: Vec<u8>,
    /// Whether this is a directory
    pub is_directory: bool,
}

impl ZipFileEntry {
    /// Create a new file entry
    pub fn file(path: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            path: path.into(),
            data,
            is_directory: false,
        }
    }

    /// Create a new directory entry
    pub fn directory(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            data: Vec::new(),
            is_directory: true,
        }
    }
}

/// Vec of `ZipFileEntry`  
pub type ZipFileEntryVec = Vec<ZipFileEntry>;

// ============================================================================
// Error types
// ============================================================================

/// Error when reading ZIP archives
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum ZipReadError {
    /// Invalid ZIP format
    InvalidFormat(String),
    /// File not found in archive
    FileNotFound(String),
    /// I/O error
    IoError(String),
    /// Path traversal attack detected
    UnsafePath(String),
    /// File is encrypted (unsupported)
    EncryptedFile(String),
    /// File too large
    FileTooLarge {
        path: String,
        size: u64,
        max_size: u64,
    },
}

impl fmt::Display for ZipReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat(msg) => write!(f, "Invalid ZIP format: {msg}"),
            Self::FileNotFound(path) => write!(f, "File not found: {path}"),
            Self::IoError(msg) => write!(f, "I/O error: {msg}"),
            Self::UnsafePath(path) => write!(f, "Unsafe path: {path}"),
            Self::EncryptedFile(path) => write!(f, "Encrypted file: {path}"),
            Self::FileTooLarge {
                path,
                size,
                max_size,
            } => {
                write!(f, "File too large: {path} ({size} > {max_size})")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ZipReadError {}

/// Error when writing ZIP archives
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum ZipWriteError {
    /// I/O error
    IoError(String),
    /// Invalid path
    InvalidPath(String),
    /// Compression error
    CompressionError(String),
}

impl fmt::Display for ZipWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError(msg) => write!(f, "I/O error: {msg}"),
            Self::InvalidPath(path) => write!(f, "Invalid path: {path}"),
            Self::CompressionError(msg) => write!(f, "Compression error: {msg}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ZipWriteError {}

// ============================================================================
// ZipFile struct
// ============================================================================

/// A ZIP archive that can be read from or written to
#[derive(Debug, Clone, Default)]
#[repr(C)]
pub struct ZipFile {
    /// The entries in the archive
    pub entries: ZipFileEntryVec,
}

impl ZipFile {
    /// Create a new empty ZIP archive
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// List contents of a ZIP archive without loading file data
    ///
    /// # Arguments
    /// * `data` - ZIP file bytes
    /// * `config` - Read configuration
    ///
    /// # Returns
    /// List of path entries (metadata only)
    #[cfg(feature = "zip")]
    /// # Errors
    ///
    /// Returns a `ZipReadError` if the archive is malformed or cannot be read.
    pub fn list(data: &[u8], config: &ZipReadConfig) -> Result<ZipPathEntryVec, ZipReadError> {
        use std::io::Cursor;

        let cursor = Cursor::new(data);
        let mut archive =
            zip::ZipArchive::new(cursor).map_err(|e| ZipReadError::InvalidFormat(e.to_string()))?;

        let mut entries = Vec::new();

        for i in 0..archive.len() {
            let file = archive
                .by_index(i)
                .map_err(|e| ZipReadError::IoError(e.to_string()))?;

            let path = file.name().to_string();

            // Security check
            if !config.allow_path_traversal && path.contains("..") {
                return Err(ZipReadError::UnsafePath(path));
            }

            entries.push(ZipPathEntry {
                path,
                is_directory: file.is_dir(),
                size: file.size(),
                compressed_size: file.compressed_size(),
                crc32: file.crc32(),
            });
        }

        Ok(entries)
    }

    /// Extract a single file from ZIP data
    ///
    /// # Arguments
    /// * `data` - ZIP file bytes
    /// * `entry` - The path entry to extract
    /// * `config` - Read configuration
    ///
    /// # Returns
    /// The file contents, or None if not found
    #[cfg(feature = "zip")]
    /// # Errors
    ///
    /// Returns a `ZipReadError` if the archive is malformed or cannot be read.
    pub fn get_single_file(
        data: &[u8],
        entry: &ZipPathEntry,
        config: &ZipReadConfig,
    ) -> Result<Option<Vec<u8>>, ZipReadError> {
        use std::io::{Cursor, Read};

        // Size check
        if config.max_file_size > 0 && entry.size > config.max_file_size {
            return Err(ZipReadError::FileTooLarge {
                path: entry.path.clone(),
                size: entry.size,
                max_size: config.max_file_size,
            });
        }

        let cursor = Cursor::new(data);
        let mut archive =
            zip::ZipArchive::new(cursor).map_err(|e| ZipReadError::InvalidFormat(e.to_string()))?;

        let mut file = match archive.by_name(&entry.path) {
            Ok(f) => f,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(e) => return Err(ZipReadError::IoError(e.to_string())),
        };

        if file.is_dir() {
            return Ok(Some(Vec::new()));
        }

        let mut contents = Vec::with_capacity(usize::try_from(entry.size).unwrap_or(0));
        file.read_to_end(&mut contents)
            .map_err(|e| ZipReadError::IoError(e.to_string()))?;

        Ok(Some(contents))
    }

    /// Load a ZIP archive from bytes
    ///
    /// # Arguments
    /// * `data` - ZIP file bytes (borrowed)
    /// * `config` - Read configuration
    #[cfg(feature = "zip")]
    /// # Errors
    ///
    /// Returns a `ZipReadError` if the archive is malformed or cannot be read.
    pub fn from_bytes(data: &[u8], config: &ZipReadConfig) -> Result<Self, ZipReadError> {
        use std::io::{Cursor, Read};

        let cursor = Cursor::new(data);
        let mut archive =
            zip::ZipArchive::new(cursor).map_err(|e| ZipReadError::InvalidFormat(e.to_string()))?;

        let mut entries = Vec::new();

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| ZipReadError::IoError(e.to_string()))?;

            let path = file.name().to_string();

            // Security check
            if !config.allow_path_traversal && path.contains("..") {
                return Err(ZipReadError::UnsafePath(path));
            }

            // Size check
            if config.max_file_size > 0 && file.size() > config.max_file_size {
                return Err(ZipReadError::FileTooLarge {
                    path,
                    size: file.size(),
                    max_size: config.max_file_size,
                });
            }

            let is_directory = file.is_dir();
            let mut file_data = Vec::new();

            if !is_directory {
                file.read_to_end(&mut file_data)
                    .map_err(|e| ZipReadError::IoError(e.to_string()))?;
            }

            entries.push(ZipFileEntry {
                path,
                data: file_data,
                is_directory,
            });
        }

        Ok(Self { entries })
    }

    /// Load a ZIP archive from a file path
    #[cfg(all(feature = "zip", feature = "std"))]
    /// # Errors
    ///
    /// Returns a `ZipReadError` if the archive is malformed or cannot be read.
    pub fn from_file(path: &Path, config: &ZipReadConfig) -> Result<Self, ZipReadError> {
        let data = std::fs::read(path).map_err(|e| ZipReadError::IoError(e.to_string()))?;
        Self::from_bytes(&data, config)
    }

    /// Write the ZIP archive to bytes
    ///
    /// # Arguments
    /// * `config` - Write configuration
    #[cfg(feature = "zip")]
    /// # Errors
    ///
    /// Returns a `ZipWriteError` if the archive cannot be built or written.
    pub fn to_bytes(&self, config: &ZipWriteConfig) -> Result<Vec<u8>, ZipWriteError> {
        use std::io::{Cursor, Write};
        use zip::write::SimpleFileOptions;

        let buffer = Vec::new();
        let cursor = Cursor::new(buffer);
        let mut writer = zip::ZipWriter::new(cursor);

        // Set archive comment
        if !config.comment.is_empty() {
            writer.set_comment(config.comment.clone());
        }

        let compression = match config.compression_method {
            0 => zip::CompressionMethod::Stored,
            _ => zip::CompressionMethod::Deflated,
        };

        let options = SimpleFileOptions::default()
            .compression_method(compression)
            .compression_level(Some(i64::from(config.compression_level)))
            .unix_permissions(config.unix_permissions);

        for entry in &self.entries {
            if entry.is_directory {
                writer
                    .add_directory(&entry.path, options)
                    .map_err(|e| ZipWriteError::IoError(e.to_string()))?;
            } else {
                writer
                    .start_file(&entry.path, options)
                    .map_err(|e| ZipWriteError::IoError(e.to_string()))?;
                writer
                    .write_all(&entry.data)
                    .map_err(|e| ZipWriteError::IoError(e.to_string()))?;
            }
        }

        let result = writer
            .finish()
            .map_err(|e| ZipWriteError::IoError(e.to_string()))?;

        Ok(result.into_inner())
    }

    /// Write the ZIP archive to a file
    #[cfg(all(feature = "zip", feature = "std"))]
    /// # Errors
    ///
    /// Returns a `ZipWriteError` if the archive cannot be built or written.
    pub fn to_file(&self, path: &Path, config: &ZipWriteConfig) -> Result<(), ZipWriteError> {
        let data = self.to_bytes(config)?;
        std::fs::write(path, data).map_err(|e| ZipWriteError::IoError(e.to_string()))?;
        Ok(())
    }

    // ========================================================================
    // Convenience methods for modifying the archive
    // ========================================================================

    /// Add a file entry (consumes the data, no clone)
    pub fn add_file(&mut self, path: impl Into<String>, data: Vec<u8>) {
        let path = path.into();
        // Remove existing entry with same path
        self.entries.retain(|e| e.path != path);
        self.entries.push(ZipFileEntry::file(path, data));
    }

    /// Add a directory entry
    pub fn add_directory(&mut self, path: impl Into<String>) {
        let path = path.into();
        self.entries.retain(|e| e.path != path);
        self.entries.push(ZipFileEntry::directory(path));
    }

    /// Remove an entry by path
    pub fn remove(&mut self, path: &str) {
        self.entries.retain(|e| e.path != path);
    }

    /// Get an entry by path
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&ZipFileEntry> {
        self.entries.iter().find(|e| e.path == path)
    }

    /// Check if archive contains a path
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.entries.iter().any(|e| e.path == path)
    }

    /// Get list of all paths
    #[must_use]
    pub fn paths(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.path.as_str()).collect()
    }

    /// Filter entries by suffix (e.g., ".fluent", ".json")
    #[must_use]
    pub fn filter_by_suffix(&self, suffix: &str) -> Vec<&ZipFileEntry> {
        self.entries
            .iter()
            .filter(|e| !e.is_directory && e.path.ends_with(suffix))
            .collect()
    }
}

// ============================================================================
// Convenience functions (for simpler use cases)
// ============================================================================

/// Create a ZIP archive from file entries (consumes entries, no clone)
#[cfg(feature = "zip")]
/// # Errors
///
/// Returns a `ZipWriteError` if the archive cannot be built or written.
pub fn zip_create(
    entries: Vec<ZipFileEntry>,
    config: &ZipWriteConfig,
) -> Result<Vec<u8>, ZipWriteError> {
    let zip = ZipFile { entries };
    zip.to_bytes(config)
}

/// Create a ZIP archive from path/data pairs (consumes entries, no clone)
#[cfg(feature = "zip")]
/// # Errors
///
/// Returns a `ZipWriteError` if the archive cannot be built or written.
pub fn zip_create_from_files(
    files: Vec<(String, Vec<u8>)>,
    config: &ZipWriteConfig,
) -> Result<Vec<u8>, ZipWriteError> {
    let entries: Vec<ZipFileEntry> = files
        .into_iter()
        .map(|(path, data)| ZipFileEntry::file(path, data))
        .collect();
    zip_create(entries, config)
}

/// Extract all files from ZIP data
#[cfg(feature = "zip")]
/// # Errors
///
/// Returns a `ZipReadError` if the archive is malformed or cannot be read.
pub fn zip_extract_all(
    data: &[u8],
    config: &ZipReadConfig,
) -> Result<Vec<ZipFileEntry>, ZipReadError> {
    let zip = ZipFile::from_bytes(data, config)?;
    Ok(zip.entries)
}

/// List contents of ZIP data without extracting
#[cfg(feature = "zip")]
/// # Errors
///
/// Returns a `ZipReadError` if the archive is malformed or cannot be read.
pub fn zip_list_contents(
    data: &[u8],
    config: &ZipReadConfig,
) -> Result<Vec<ZipPathEntry>, ZipReadError> {
    ZipFile::list(data, config)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zip_config_defaults() {
        let read_config = ZipReadConfig::default();
        assert_eq!(read_config.max_file_size, 0);
        assert!(!read_config.allow_path_traversal);

        let write_config = ZipWriteConfig::default();
        assert_eq!(write_config.compression_method, 1);
        assert_eq!(write_config.compression_level, 6);
    }

    #[test]
    fn test_zip_file_entry_creation() {
        let file = ZipFileEntry::file("test.txt", b"Hello".to_vec());
        assert_eq!(file.path, "test.txt");
        assert!(!file.is_directory);
        assert_eq!(file.data, b"Hello");

        let dir = ZipFileEntry::directory("subdir/");
        assert!(dir.is_directory);
        assert!(dir.data.is_empty());
    }

    #[cfg(feature = "zip")]
    #[test]
    fn test_zip_roundtrip() {
        let files = vec![
            ("hello.txt".to_string(), b"Hello, World!".to_vec()),
            ("sub/nested.txt".to_string(), b"Nested file".to_vec()),
        ];

        let write_config = ZipWriteConfig::default();
        let zip_data = zip_create_from_files(files, &write_config).expect("Failed to create ZIP");

        let read_config = ZipReadConfig::default();
        let entries = zip_extract_all(&zip_data, &read_config).expect("Failed to extract");

        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.path == "hello.txt"));
        assert!(entries.iter().any(|e| e.path == "sub/nested.txt"));
    }

    #[cfg(feature = "zip")]
    #[test]
    fn test_zip_file_manipulation() {
        let mut zip = ZipFile::new();

        zip.add_file("a.txt", b"AAA".to_vec());
        zip.add_file("b.txt", b"BBB".to_vec());

        assert_eq!(zip.entries.len(), 2);
        assert!(zip.contains("a.txt"));
        assert!(zip.contains("b.txt"));

        zip.remove("a.txt");
        assert_eq!(zip.entries.len(), 1);
        assert!(!zip.contains("a.txt"));

        // Overwrite existing
        zip.add_file("b.txt", b"NEW".to_vec());
        assert_eq!(zip.entries.len(), 1);
        assert_eq!(zip.get("b.txt").unwrap().data, b"NEW");
    }
}

// ============================================================================
// Autotest: adversarial tests
// ============================================================================

#[cfg(test)]
mod autotest_generated {
    use super::*;

    // ------------------------------------------------------------------
    // helpers
    // ------------------------------------------------------------------

    /// A ZIP that this module can actually produce: default (Deflate/6) config.
    #[cfg(feature = "zip")]
    fn build(entries: Vec<ZipFileEntry>) -> Vec<u8> {
        zip_create(entries, &ZipWriteConfig::default()).expect("default write config must work")
    }

    /// Hand-rolled 22-byte "end of central directory" record = an empty archive.
    #[cfg(feature = "zip")]
    fn eocd_only() -> Vec<u8> {
        let mut v = vec![0x50, 0x4B, 0x05, 0x06];
        v.extend_from_slice(&[0u8; 18]);
        v
    }

    /// Adversarial path strings reused across the lookup tests.
    fn nasty_paths() -> Vec<String> {
        vec![
            String::new(),
            "   ".to_string(),
            "\t\n".to_string(),
            "\0".to_string(),
            "a\0b".to_string(),
            "..".to_string(),
            "../../etc/passwd".to_string(),
            "./a.txt".to_string(),
            "a.txt ".to_string(),
            " a.txt".to_string(),
            "a.txt;garbage".to_string(),
            "0".to_string(),
            "-0".to_string(),
            "NaN".to_string(),
            "inf".to_string(),
            "-inf".to_string(),
            "9223372036854775807".to_string(),
            "-9223372036854775808".to_string(),
            "18446744073709551615".to_string(),
            "1e309".to_string(),
            "\u{1F600}".to_string(),
            "e\u{0301}\u{0301}\u{0301}.txt".to_string(),
            "\u{202E}txt.exe".to_string(),
            "\u{FEFF}a.txt".to_string(),
            "A/".repeat(2000),
            "x".repeat(100_000),
        ]
    }

    // ==================================================================
    // constructors / config (feature-independent)
    // ==================================================================

    #[test]
    fn autotest_read_config_builders_at_numeric_extremes() {
        let base = ZipReadConfig::new();
        let def = ZipReadConfig::default();
        assert_eq!(base.max_file_size, def.max_file_size);
        assert_eq!(base.allow_path_traversal, def.allow_path_traversal);
        assert_eq!(base.skip_encrypted, def.skip_encrypted);
        assert_eq!(base.max_file_size, 0);
        assert!(!base.allow_path_traversal);
        assert!(!base.skip_encrypted);

        for size in [
            0u64,
            1,
            u64::from(u32::MAX),
            u64::MAX / 2,
            u64::MAX - 1,
            u64::MAX,
        ] {
            let c = ZipReadConfig::new().with_max_file_size(size);
            assert_eq!(c.max_file_size, size);
            // the other fields must not be perturbed by the builder
            assert!(!c.allow_path_traversal);
            assert!(!c.skip_encrypted);
        }

        for allow in [false, true] {
            let c = ZipReadConfig::new()
                .with_max_file_size(u64::MAX)
                .with_allow_path_traversal(allow);
            assert_eq!(c.allow_path_traversal, allow);
            assert_eq!(c.max_file_size, u64::MAX);
        }

        // builders are order-independent and idempotent
        let a = ZipReadConfig::new()
            .with_max_file_size(7)
            .with_allow_path_traversal(true);
        let b = ZipReadConfig::new()
            .with_allow_path_traversal(true)
            .with_max_file_size(7);
        assert_eq!(a.max_file_size, b.max_file_size);
        assert_eq!(a.allow_path_traversal, b.allow_path_traversal);
        let c = a.with_max_file_size(7);
        assert_eq!(c.max_file_size, 7);
        assert!(c.allow_path_traversal);

        // ZipReadConfig is Copy: the "consumed" value is still usable
        let orig = ZipReadConfig::new();
        let _moved = orig.with_max_file_size(99);
        assert_eq!(orig.max_file_size, 0);
    }

    #[test]
    fn autotest_write_config_new_store_and_defaults() {
        let new = ZipWriteConfig::new();
        let def = ZipWriteConfig::default();
        assert_eq!(new.compression_method, def.compression_method);
        assert_eq!(new.compression_level, def.compression_level);
        assert_eq!(new.unix_permissions, def.unix_permissions);
        assert_eq!(new.comment, def.comment);
        assert_eq!(new.compression_method, 1);
        assert_eq!(new.compression_level, 6);
        assert_eq!(new.unix_permissions, 0o644);
        assert!(new.comment.is_empty());

        let store = ZipWriteConfig::store();
        assert_eq!(store.compression_method, 0);
        assert_eq!(store.compression_level, 0);
        // store() only overrides the two compression fields
        assert_eq!(store.unix_permissions, 0o644);
        assert!(store.comment.is_empty());
    }

    #[test]
    fn autotest_write_config_deflate_saturates_level() {
        // documented clamp is `level.min(9)`; verify across the whole u8 domain
        for level in 0u16..=255 {
            let level = u8::try_from(level).unwrap();
            let cfg = ZipWriteConfig::deflate(level);
            assert_eq!(
                cfg.compression_method, 1,
                "deflate() must always select Deflate"
            );
            assert_eq!(
                cfg.compression_level,
                level.min(9),
                "deflate({level}) did not saturate at 9"
            );
            assert!(cfg.compression_level <= 9);
        }
        // explicit boundary spot checks
        assert_eq!(ZipWriteConfig::deflate(0).compression_level, 0);
        assert_eq!(ZipWriteConfig::deflate(9).compression_level, 9);
        assert_eq!(ZipWriteConfig::deflate(10).compression_level, 9);
        assert_eq!(ZipWriteConfig::deflate(u8::MIN).compression_level, 0);
        assert_eq!(ZipWriteConfig::deflate(u8::MAX).compression_level, 9);
    }

    #[test]
    fn autotest_write_config_with_comment_extremes() {
        // empty
        let c = ZipWriteConfig::new().with_comment("");
        assert!(c.comment.is_empty());

        // unicode + control chars + NUL are stored verbatim (no sanitising)
        for s in [
            "\u{1F600}\u{1F9F0}",
            "e\u{0301}combining",
            "line1\nline2\r\n",
            "nul\0inside",
            "\u{202E}rtl",
        ] {
            let c = ZipWriteConfig::new().with_comment(s);
            assert_eq!(c.comment, s);
            assert_eq!(c.comment.chars().count(), s.chars().count());
        }

        // very long comment (well past the u16 EOCD comment-length field)
        let huge = "z".repeat(200_000);
        let c = ZipWriteConfig::new().with_comment(huge.clone());
        assert_eq!(c.comment.len(), 200_000);
        assert_eq!(c.comment, huge);
        // other fields untouched
        assert_eq!(c.compression_method, 1);
        assert_eq!(c.compression_level, 6);

        // with_comment accepts both &str and String, and last write wins
        let c = ZipWriteConfig::store()
            .with_comment("a")
            .with_comment(String::from("b"));
        assert_eq!(c.comment, "b");
        assert_eq!(c.compression_method, 0);
    }

    #[test]
    fn autotest_zip_file_entry_constructors_no_panic() {
        // empty path
        let e = ZipFileEntry::file("", Vec::new());
        assert!(e.path.is_empty());
        assert!(e.data.is_empty());
        assert!(!e.is_directory);

        // path/data extremes
        let long_path = "p".repeat(200_000);
        let e = ZipFileEntry::file(long_path.clone(), vec![0xFFu8; 4096]);
        assert_eq!(e.path, long_path);
        assert_eq!(e.data.len(), 4096);
        assert!(!e.is_directory);

        // non-UTF8-looking bytes as *data* are fine (data is Vec<u8>)
        let e = ZipFileEntry::file("bin", vec![0xFFu8, 0xFE, 0x00, 0x80]);
        assert_eq!(e.data, vec![0xFFu8, 0xFE, 0x00, 0x80]);

        // directory() always discards data and flags is_directory
        for p in nasty_paths() {
            let d = ZipFileEntry::directory(p.clone());
            assert_eq!(d.path, p);
            assert!(d.is_directory);
            assert!(d.data.is_empty());
        }

        // constructors never rewrite the path (no trailing-slash normalisation)
        assert_eq!(ZipFileEntry::directory("sub").path, "sub");
        assert_eq!(ZipFileEntry::directory("sub/").path, "sub/");
    }

    // ==================================================================
    // Display / error serialisation
    // ==================================================================

    #[test]
    fn autotest_read_error_display_all_variants_non_empty() {
        let cases = vec![
            (ZipReadError::InvalidFormat("bad magic".into()), "bad magic"),
            (ZipReadError::FileNotFound("a.txt".into()), "a.txt"),
            (ZipReadError::IoError("eof".into()), "eof"),
            (ZipReadError::UnsafePath("../x".into()), "../x"),
            (ZipReadError::EncryptedFile("s.bin".into()), "s.bin"),
            (
                ZipReadError::FileTooLarge {
                    path: "big".into(),
                    size: 10,
                    max_size: 5,
                },
                "big",
            ),
        ];
        for (err, needle) in cases {
            let s = err.to_string();
            assert!(!s.is_empty(), "empty Display for {err:?}");
            assert!(s.contains(needle), "Display {s:?} lost payload {needle:?}");
            // Debug must also be non-empty and must not equal Display
            assert!(!format!("{err:?}").is_empty());
        }
    }

    #[test]
    fn autotest_read_error_display_edge_payloads() {
        // empty payloads still produce a non-empty, prefixed message
        for err in [
            ZipReadError::InvalidFormat(String::new()),
            ZipReadError::FileNotFound(String::new()),
            ZipReadError::IoError(String::new()),
            ZipReadError::UnsafePath(String::new()),
            ZipReadError::EncryptedFile(String::new()),
        ] {
            let s = err.to_string();
            assert!(!s.is_empty(), "empty payload produced empty Display");
            assert!(s.contains(':'), "expected a prefixed message, got {s:?}");
        }

        // u64 boundaries in FileTooLarge
        for (size, max_size) in [
            (0u64, 0u64),
            (0, u64::MAX),
            (u64::MAX, 0),
            (u64::MAX, u64::MAX),
            (u64::MAX - 1, u64::MAX),
        ] {
            let err = ZipReadError::FileTooLarge {
                path: "\u{1F600}/p".into(),
                size,
                max_size,
            };
            let s = err.to_string();
            assert!(s.contains(&format!("{size}")));
            assert!(s.contains(&format!("{max_size}")));
            assert!(s.contains("\u{1F600}"));
        }

        // unicode / control / NUL payloads round-trip through Display unchanged
        for payload in [
            "\u{1F600}",
            "e\u{0301}",
            "a\0b",
            "line\nbreak",
            &"L".repeat(50_000),
        ] {
            let err = ZipReadError::UnsafePath(payload.to_string());
            assert!(err.to_string().contains(payload));
        }
    }

    #[test]
    fn autotest_write_error_display_all_variants_non_empty() {
        let cases = vec![
            (ZipWriteError::IoError("disk full".into()), "disk full"),
            (ZipWriteError::InvalidPath("\u{1F600}".into()), "\u{1F600}"),
            (ZipWriteError::CompressionError("level".into()), "level"),
        ];
        for (err, needle) in cases {
            let s = err.to_string();
            assert!(!s.is_empty());
            assert!(s.contains(needle));
            assert!(s.contains(':'));
        }

        for err in [
            ZipWriteError::IoError(String::new()),
            ZipWriteError::InvalidPath(String::new()),
            ZipWriteError::CompressionError(String::new()),
        ] {
            assert!(!err.to_string().is_empty());
        }

        // huge + NUL payloads do not panic
        let big = ZipWriteError::CompressionError("\0".to_string() + &"q".repeat(100_000));
        assert!(big.to_string().len() >= 100_000);
    }

    #[test]
    fn autotest_error_equality_and_std_error_impls() {
        assert_eq!(
            ZipReadError::UnsafePath("a".into()),
            ZipReadError::UnsafePath("a".into())
        );
        assert_ne!(
            ZipReadError::UnsafePath("a".into()),
            ZipReadError::FileNotFound("a".into())
        );
        assert_ne!(
            ZipReadError::FileTooLarge {
                path: "p".into(),
                size: 1,
                max_size: 2
            },
            ZipReadError::FileTooLarge {
                path: "p".into(),
                size: 1,
                max_size: 3
            }
        );
        assert_eq!(
            ZipWriteError::IoError("x".into()),
            ZipWriteError::IoError("x".into())
        );
        assert_ne!(
            ZipWriteError::IoError("x".into()),
            ZipWriteError::InvalidPath("x".into())
        );

        // Clone must preserve equality
        let e = ZipReadError::FileTooLarge {
            path: "p".into(),
            size: u64::MAX,
            max_size: 0,
        };
        assert_eq!(e.clone(), e);

        #[cfg(feature = "std")]
        {
            let r: &dyn std::error::Error = &e;
            assert!(!r.to_string().is_empty());
            let w = ZipWriteError::IoError("x".into());
            let r: &dyn std::error::Error = &w;
            assert!(!r.to_string().is_empty());
        }
    }

    // ==================================================================
    // in-memory ZipFile invariants (feature-independent)
    // ==================================================================

    #[test]
    fn autotest_zipfile_new_and_default_are_empty() {
        let a = ZipFile::new();
        let b = ZipFile::default();
        assert!(a.entries.is_empty());
        assert!(b.entries.is_empty());
        assert!(a.paths().is_empty());
        assert!(a.filter_by_suffix("").is_empty());
        assert!(a.filter_by_suffix(".txt").is_empty());
        assert!(a.get("").is_none());
        assert!(!a.contains(""));

        // every adversarial lookup on an empty archive is None/false, never a panic
        for p in nasty_paths() {
            assert!(a.get(&p).is_none());
            assert!(!a.contains(&p));
        }

        // remove on an empty archive is a no-op
        let mut c = ZipFile::new();
        c.remove("nope");
        c.remove("");
        assert!(c.entries.is_empty());
    }

    #[test]
    fn autotest_add_file_dedup_keeps_last_write() {
        let mut zip = ZipFile::new();
        zip.add_file("a", b"1".to_vec());
        zip.add_file("b", b"2".to_vec());
        zip.add_file("a", b"3".to_vec());
        assert_eq!(zip.entries.len(), 2);
        assert_eq!(zip.get("a").unwrap().data, b"3");
        // the replaced entry is re-appended at the end, so order changes
        assert_eq!(zip.paths(), vec!["b", "a"]);

        // repeated writes to the same path never grow the archive
        for i in 0..100u32 {
            zip.add_file("a", format!("{i}").into_bytes());
        }
        assert_eq!(zip.entries.len(), 2);
        assert_eq!(zip.get("a").unwrap().data, b"99");
    }

    #[test]
    fn autotest_add_directory_and_add_file_share_the_path_namespace() {
        let mut zip = ZipFile::new();
        zip.add_file("x", b"data".to_vec());
        assert!(!zip.get("x").unwrap().is_directory);

        // add_directory replaces a file at the same path
        zip.add_directory("x");
        assert_eq!(zip.entries.len(), 1);
        assert!(zip.get("x").unwrap().is_directory);
        assert!(zip.get("x").unwrap().data.is_empty());

        // ...and vice versa
        zip.add_file("x", b"back".to_vec());
        assert_eq!(zip.entries.len(), 1);
        assert!(!zip.get("x").unwrap().is_directory);
        assert_eq!(zip.get("x").unwrap().data, b"back");

        // "x" and "x/" are *different* paths at this layer
        zip.add_directory("x/");
        assert_eq!(zip.entries.len(), 2);
        assert!(zip.contains("x"));
        assert!(zip.contains("x/"));
    }

    #[test]
    fn autotest_add_and_remove_adversarial_paths_no_panic() {
        let mut zip = ZipFile::new();
        let paths = nasty_paths();
        for (i, p) in paths.iter().enumerate() {
            zip.add_file(p.clone(), vec![u8::try_from(i % 256).unwrap()]);
        }
        // nasty_paths() has no duplicates, so every path survived
        assert_eq!(zip.entries.len(), paths.len());
        for p in &paths {
            assert!(zip.contains(p), "lost path {p:?}");
            assert!(zip.get(p).is_some());
        }
        for p in &paths {
            zip.remove(p);
            assert!(!zip.contains(p));
        }
        assert!(zip.entries.is_empty());

        // removing a path that is a *prefix*/*suffix* of a stored path must not match
        let mut zip = ZipFile::new();
        zip.add_file("dir/file.txt", b"d".to_vec());
        zip.remove("dir/");
        zip.remove("file.txt");
        zip.remove("dir/file.tx");
        zip.remove("dir/file.txt ");
        assert_eq!(
            zip.entries.len(),
            1,
            "remove() must match the whole path only"
        );
        zip.remove("dir/file.txt");
        assert!(zip.entries.is_empty());
    }

    #[test]
    fn autotest_get_and_contains_agree_and_reject_junk() {
        let mut zip = ZipFile::new();
        zip.add_file("a.txt", b"A".to_vec());
        zip.add_file("\u{1F600}.txt", b"E".to_vec());
        zip.add_directory("sub/");

        // exact matches only
        assert!(zip.contains("a.txt"));
        assert!(zip.contains("\u{1F600}.txt"));
        assert!(zip.contains("sub/"));

        // leading/trailing junk, case changes and near-misses are all rejected
        for p in [
            " a.txt",
            "a.txt ",
            "A.TXT",
            "a.txt\0",
            "./a.txt",
            "/a.txt",
            "a.txt;x",
            "sub",
            "sub//",
            "\u{1F600}",
            "\u{1F600}.TXT",
        ] {
            assert!(!zip.contains(p), "unexpected match for {p:?}");
            assert!(zip.get(p).is_none());
        }

        // get()/contains() must never disagree, for any input
        for p in nasty_paths() {
            assert_eq!(zip.get(&p).is_some(), zip.contains(&p), "disagree on {p:?}");
        }

        // a 1M-char probe neither panics nor hangs
        let huge = "y".repeat(1_000_000);
        assert!(zip.get(&huge).is_none());
        assert!(!zip.contains(&huge));
    }

    #[test]
    fn autotest_paths_mirrors_entries_in_order() {
        let mut zip = ZipFile::new();
        assert!(zip.paths().is_empty());

        for i in 0..50u32 {
            zip.add_file(format!("f{i}"), vec![u8::try_from(i).unwrap()]);
        }
        zip.add_directory("d/");

        let paths = zip.paths();
        assert_eq!(paths.len(), zip.entries.len());
        for (p, e) in paths.iter().zip(zip.entries.iter()) {
            assert_eq!(*p, e.path.as_str());
        }
        // directories are included in paths()
        assert!(paths.contains(&"d/"));

        // duplicates constructed directly are all reported
        let dup = ZipFile {
            entries: vec![
                ZipFileEntry::file("same", b"1".to_vec()),
                ZipFileEntry::file("same", b"2".to_vec()),
            ],
        };
        assert_eq!(dup.paths(), vec!["same", "same"]);
        // get() returns the *first* match
        assert_eq!(dup.get("same").unwrap().data, b"1");
        assert!(dup.contains("same"));
        // ...and remove() drops every duplicate
        let mut dup = dup;
        dup.remove("same");
        assert!(dup.entries.is_empty());
    }

    #[test]
    fn autotest_filter_by_suffix_edge_cases() {
        let zip = ZipFile {
            entries: vec![
                ZipFileEntry::file("a.txt", b"1".to_vec()),
                ZipFileEntry::file("b.TXT", b"2".to_vec()),
                ZipFileEntry::file("README", b"3".to_vec()),
                ZipFileEntry::file("", b"4".to_vec()),
                ZipFileEntry::file("\u{1F600}.json", b"5".to_vec()),
                ZipFileEntry::directory("dir.txt"),
                ZipFileEntry::directory("sub/"),
            ],
        };

        // empty suffix matches every *non-directory* entry
        assert_eq!(zip.filter_by_suffix("").len(), 5);
        assert!(zip.filter_by_suffix("").iter().all(|e| !e.is_directory));

        // directories are excluded even when their path ends with the suffix
        let txt = zip.filter_by_suffix(".txt");
        assert_eq!(txt.len(), 1);
        assert_eq!(txt[0].path, "a.txt");

        // matching is case-sensitive
        assert_eq!(zip.filter_by_suffix(".TXT").len(), 1);
        assert_eq!(zip.filter_by_suffix(".Txt").len(), 0);

        // whole-path suffix matches
        assert_eq!(zip.filter_by_suffix("README").len(), 1);

        // multibyte suffix must not split a char boundary or panic
        assert_eq!(zip.filter_by_suffix("\u{1F600}.json").len(), 1);
        assert_eq!(zip.filter_by_suffix("json").len(), 1);

        // suffix longer than any path -> empty, no panic
        assert!(zip.filter_by_suffix(&"n".repeat(100_000)).is_empty());
        // junk suffixes
        assert!(zip.filter_by_suffix("\0").is_empty());
        assert!(zip.filter_by_suffix("  ").is_empty());
    }

    // ==================================================================
    // parsers: malformed / hostile input
    // ==================================================================

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_readers_reject_empty_and_garbage_without_panicking() {
        let cfg = ZipReadConfig::default();

        let inputs: Vec<Vec<u8>> = vec![
            Vec::new(),
            b"   ".to_vec(),
            b"\t\n\r ".to_vec(),
            b"not a zip file at all".to_vec(),
            vec![0u8; 22],
            vec![0xFF, 0xFE, 0x00],
            vec![0xC3, 0x28, 0xA0, 0xA1], // invalid UTF-8
            b"PK".to_vec(),               // truncated signature
            b"PK\x03\x04".to_vec(),       // local header signature only
            b"PK\x05\x06".to_vec(),       // truncated EOCD
            b"0 -0 NaN inf 9223372036854775807".to_vec(),
            "\u{1F600}\u{0301}".as_bytes().to_vec(), // multibyte unicode
            b"[".repeat(10_000),                     // "deeply nested" junk
            b"PK\x05\x06".repeat(5_000),             // many EOCD-ish signatures
        ];

        for data in inputs {
            let listed = ZipFile::list(&data, &cfg);
            let loaded = ZipFile::from_bytes(&data, &cfg);
            let extracted = zip_extract_all(&data, &cfg);
            let contents = zip_list_contents(&data, &cfg);

            // the free functions must agree with the inherent methods
            assert_eq!(loaded.is_err(), extracted.is_err());
            assert_eq!(listed.is_err(), contents.is_err());

            match loaded {
                Err(e) => {
                    // garbage must surface as a *parse* failure, never as a
                    // security/limit verdict (UnsafePath / FileTooLarge / ...)
                    assert!(
                        matches!(e, ZipReadError::InvalidFormat(_) | ZipReadError::IoError(_)),
                        "unexpected error kind for {:?}: {e:?}",
                        &data[..data.len().min(8)]
                    );
                    assert!(!e.to_string().is_empty());
                }
                // if it *did* parse, it must be a degenerate empty archive
                Ok(z) => assert!(z.entries.is_empty()),
            }
        }

        // empty input specifically is a format error, not an I/O error
        assert!(matches!(
            ZipFile::from_bytes(b"", &cfg),
            Err(ZipReadError::InvalidFormat(_))
        ));
        assert!(matches!(
            ZipFile::list(b"", &cfg),
            Err(ZipReadError::InvalidFormat(_))
        ));
    }

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_readers_handle_one_megabyte_of_junk() {
        let cfg = ZipReadConfig::default();
        // 1 MiB with no valid central directory: must fail fast, not hang or OOM
        let junk = vec![b'A'; 1_000_000];
        assert!(ZipFile::from_bytes(&junk, &cfg).is_err());
        assert!(ZipFile::list(&junk, &cfg).is_err());

        // 1 MiB of zeros (a plausible sparse/zeroed file)
        let zeros = vec![0u8; 1_000_000];
        assert!(ZipFile::from_bytes(&zeros, &cfg).is_err());

        // 1 MiB ending in something that looks like an EOCD but isn't consistent
        let mut fake = vec![b'B'; 1_000_000];
        fake.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]);
        fake.extend_from_slice(&[0xFFu8; 18]);
        let res = ZipFile::from_bytes(&fake, &cfg);
        assert!(
            res.map_or(true, |z| z.entries.is_empty()),
            "a bogus EOCD must not yield phantom entries"
        );
    }

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_minimal_valid_archives_parse_as_empty() {
        let cfg = ZipReadConfig::default();

        // positive control #1: what this module itself writes for an empty archive
        let own = ZipFile::new()
            .to_bytes(&ZipWriteConfig::default())
            .expect("empty archive must be writable");
        let round = ZipFile::from_bytes(&own, &cfg).expect("own empty archive must re-read");
        assert!(round.entries.is_empty());
        assert!(ZipFile::list(&own, &cfg).unwrap().is_empty());

        // an empty archive is also writable with the store() config (no file entries)
        assert!(ZipFile::new().to_bytes(&ZipWriteConfig::store()).is_ok());

        // positive control #2: the canonical 22-byte EOCD-only archive
        let eocd = eocd_only();
        assert_eq!(eocd.len(), 22);
        if let Ok(z) = ZipFile::from_bytes(&eocd, &cfg) {
            assert!(z.entries.is_empty());
        }
    }

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_truncated_and_bitflipped_archives_never_panic() {
        let cfg = ZipReadConfig::default();
        let good = build(vec![
            ZipFileEntry::file("a.txt", b"hello hello hello hello".to_vec()),
            ZipFileEntry::file("b.bin", vec![7u8; 512]),
        ]);
        assert!(ZipFile::from_bytes(&good, &cfg).is_ok());

        // every truncation prefix must be handled (Err or degenerate Ok), never a panic
        for cut in [
            0,
            1,
            3,
            4,
            10,
            good.len() / 4,
            good.len() / 2,
            good.len() - 1,
        ] {
            let _ = ZipFile::from_bytes(&good[..cut], &cfg);
            let _ = ZipFile::list(&good[..cut], &cfg);
        }

        // single-byte corruption anywhere in the stream
        for i in (0..good.len()).step_by(7) {
            let mut bad = good.clone();
            bad[i] ^= 0xFF;
            let _ = ZipFile::from_bytes(&bad, &cfg);
            let _ = ZipFile::list(&bad, &cfg);
        }

        // trailing junk appended after the EOCD
        let mut trailing = good.clone();
        trailing.extend_from_slice(b"garbage;garbage");
        let _ = ZipFile::from_bytes(&trailing, &cfg);

        // leading junk prepended before the local headers
        let mut leading = b"JUNK".to_vec();
        leading.extend_from_slice(&good);
        let _ = ZipFile::from_bytes(&leading, &cfg);
    }

    // ==================================================================
    // round-trip: encode == decode
    // ==================================================================

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_roundtrip_all_byte_values_and_empty_files() {
        let all_bytes: Vec<u8> = (0..=255u8).collect();
        let entries = vec![
            ZipFileEntry::file("bytes.bin", all_bytes.clone()),
            ZipFileEntry::file("empty.bin", Vec::new()),
            ZipFileEntry::file("one.bin", vec![0u8]),
        ];
        let bytes = build(entries);
        let cfg = ZipReadConfig::default();
        let round = ZipFile::from_bytes(&bytes, &cfg).unwrap();

        assert_eq!(round.entries.len(), 3);
        assert_eq!(round.paths(), vec!["bytes.bin", "empty.bin", "one.bin"]);
        assert_eq!(round.get("bytes.bin").unwrap().data, all_bytes);
        assert!(round.get("empty.bin").unwrap().data.is_empty());
        assert_eq!(round.get("one.bin").unwrap().data, vec![0u8]);
        assert!(round.entries.iter().all(|e| !e.is_directory));

        // re-encoding the decoded archive yields the same decoded content
        let again = round.to_bytes(&ZipWriteConfig::default()).unwrap();
        let round2 = ZipFile::from_bytes(&again, &cfg).unwrap();
        assert_eq!(round2.paths(), round.paths());
        for e in &round.entries {
            assert_eq!(round2.get(&e.path).unwrap().data, e.data);
        }
    }

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_roundtrip_unicode_paths_and_content() {
        let paths = [
            "\u{1F600}.txt",
            "e\u{0301}\u{0301}combining.txt",
            "\u{4F60}\u{597D}/\u{4E16}\u{754C}.txt",
            "\u{FEFF}bom.txt",
            "spaces   and\ttabs.txt",
        ];
        let entries: Vec<ZipFileEntry> = paths
            .iter()
            .enumerate()
            .map(|(i, p)| ZipFileEntry::file(*p, format!("payload \u{1F9F0} {i}").into_bytes()))
            .collect();

        let bytes = build(entries);
        let round = ZipFile::from_bytes(&bytes, &ZipReadConfig::default()).unwrap();
        assert_eq!(round.entries.len(), paths.len());
        for (i, p) in paths.iter().enumerate() {
            let e = round
                .get(p)
                .unwrap_or_else(|| panic!("unicode path {p:?} was not preserved"));
            assert_eq!(e.data, format!("payload \u{1F9F0} {i}").into_bytes());
        }
    }

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_roundtrip_deep_paths_and_large_payload() {
        // ~4 KiB deeply nested path (2000 components) - must not stack-overflow
        let deep = "a/".repeat(2000) + "leaf.txt";
        assert!(!deep.contains(".."));
        // 100 KiB payload with a non-degenerate byte distribution
        let big: Vec<u8> = (0..100_000u32)
            .map(|i| u8::try_from(i % 251).unwrap())
            .collect();

        let bytes = build(vec![
            ZipFileEntry::file(deep.clone(), b"leaf".to_vec()),
            ZipFileEntry::file("big.bin", big.clone()),
        ]);
        let round = ZipFile::from_bytes(&bytes, &ZipReadConfig::default()).unwrap();
        assert_eq!(round.get(&deep).unwrap().data, b"leaf");
        assert_eq!(round.get("big.bin").unwrap().data, big);

        // list() reports the true uncompressed size for the large entry
        let listed = ZipFile::list(&bytes, &ZipReadConfig::default()).unwrap();
        let big_meta = listed.iter().find(|e| e.path == "big.bin").unwrap();
        assert_eq!(big_meta.size, 100_000);
        assert!(!big_meta.is_directory);
    }

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_roundtrip_directory_entries_get_a_trailing_slash() {
        let bytes = build(vec![
            ZipFileEntry::directory("with_slash/"),
            ZipFileEntry::directory("no_slash"),
            ZipFileEntry::file("f.txt", b"x".to_vec()),
        ]);
        let round = ZipFile::from_bytes(&bytes, &ZipReadConfig::default()).unwrap();
        assert_eq!(round.entries.len(), 3);

        let with = round.get("with_slash/").expect("dir with slash preserved");
        assert!(with.is_directory);
        assert!(with.data.is_empty());

        // NOTE: the underlying writer rewrites "no_slash" -> "no_slash/", so the
        // path that comes back is NOT the path that went in. Asserted, not fixed.
        assert!(round.get("no_slash").is_none());
        let without = round
            .get("no_slash/")
            .expect("dir without slash was rewritten");
        assert!(without.is_directory);

        assert!(!round.get("f.txt").unwrap().is_directory);

        // list() agrees about directory-ness
        let listed = ZipFile::list(&bytes, &ZipReadConfig::default()).unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(listed.iter().filter(|e| e.is_directory).count(), 2);
        for d in listed.iter().filter(|e| e.is_directory) {
            assert_eq!(d.size, 0);
            assert!(d.path.ends_with('/'));
        }
    }

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_roundtrip_survives_config_extremes() {
        let entries = || vec![ZipFileEntry::file("f.bin", vec![9u8; 3000])];

        // every deflate level that the writer accepts
        for level in 1..=9u8 {
            let cfg = ZipWriteConfig::deflate(level);
            let bytes = zip_create(entries(), &cfg)
                .unwrap_or_else(|e| panic!("deflate({level}) failed: {e}"));
            let round = ZipFile::from_bytes(&bytes, &ZipReadConfig::default()).unwrap();
            assert_eq!(round.get("f.bin").unwrap().data, vec![9u8; 3000]);
        }
        // deflate() saturates, so 10..=255 all behave like 9
        for level in [10u8, 100, u8::MAX] {
            let bytes = zip_create(entries(), &ZipWriteConfig::deflate(level)).unwrap();
            let round = ZipFile::from_bytes(&bytes, &ZipReadConfig::default()).unwrap();
            assert_eq!(round.get("f.bin").unwrap().data.len(), 3000);
        }

        // u32::MAX permissions must not overflow or corrupt the archive
        let mut cfg = ZipWriteConfig {
            unix_permissions: u32::MAX,
            ..Default::default()
        };
        let bytes = zip_create(entries(), &cfg).unwrap();
        assert_eq!(
            ZipFile::from_bytes(&bytes, &ZipReadConfig::default())
                .unwrap()
                .get("f.bin")
                .unwrap()
                .data
                .len(),
            3000
        );
        cfg.unix_permissions = 0;
        assert!(zip_create(entries(), &cfg).is_ok());

        // a unicode archive comment keeps the EOCD findable
        let cfg = ZipWriteConfig::default().with_comment("\u{1F5DC}\u{FE0F} t\u{E9}st comment");
        let bytes = zip_create(entries(), &cfg).unwrap();
        let round = ZipFile::from_bytes(&bytes, &ZipReadConfig::default()).unwrap();
        assert_eq!(round.entries.len(), 1);

        // an over-long comment (past the u16 EOCD length field) must not panic
        let cfg = ZipWriteConfig::default().with_comment("c".repeat(70_000));
        let _ = zip_create(entries(), &cfg);
    }

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_convenience_functions_agree_with_methods() {
        let files = vec![
            ("a.txt".to_string(), b"AAA".to_vec()),
            ("dir/b.bin".to_string(), vec![0u8, 255, 128]),
        ];
        let cfg = ZipWriteConfig::default();
        let via_files = zip_create_from_files(files.clone(), &cfg).unwrap();
        let via_entries = zip_create(
            files
                .iter()
                .map(|(p, d)| ZipFileEntry::file(p.clone(), d.clone()))
                .collect(),
            &cfg,
        )
        .unwrap();
        let via_method = ZipFile {
            entries: files
                .iter()
                .map(|(p, d)| ZipFileEntry::file(p.clone(), d.clone()))
                .collect(),
        }
        .to_bytes(&cfg)
        .unwrap();

        let rcfg = ZipReadConfig::default();
        for bytes in [&via_files, &via_entries, &via_method] {
            let extracted = zip_extract_all(bytes, &rcfg).unwrap();
            let loaded = ZipFile::from_bytes(bytes, &rcfg).unwrap();
            assert_eq!(extracted.len(), 2);
            assert_eq!(loaded.entries.len(), 2);
            for (i, (p, d)) in files.iter().enumerate() {
                assert_eq!(&extracted[i].path, p);
                assert_eq!(&extracted[i].data, d);
                assert_eq!(&loaded.entries[i].path, p);
            }

            // zip_list_contents == ZipFile::list, and paths/sizes match the data
            let listed = zip_list_contents(bytes, &rcfg).unwrap();
            let listed2 = ZipFile::list(bytes, &rcfg).unwrap();
            assert_eq!(listed.len(), listed2.len());
            for (a, b) in listed.iter().zip(listed2.iter()) {
                assert_eq!(a.path, b.path);
                assert_eq!(a.size, b.size);
                assert_eq!(a.compressed_size, b.compressed_size);
                assert_eq!(a.crc32, b.crc32);
                assert_eq!(a.is_directory, b.is_directory);
            }
            for (meta, (p, d)) in listed.iter().zip(files.iter()) {
                assert_eq!(&meta.path, p);
                assert_eq!(meta.size, d.len() as u64);
                assert!(!meta.is_directory);
            }
        }

        // empty input list -> valid empty archive
        let empty = zip_create_from_files(Vec::new(), &cfg).unwrap();
        assert!(zip_extract_all(&empty, &rcfg).unwrap().is_empty());
        assert!(zip_list_contents(&empty, &rcfg).unwrap().is_empty());
    }

    // ==================================================================
    // security checks: path traversal + size limits
    // ==================================================================

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_path_traversal_check_is_a_plain_substring_test() {
        // "a..b.txt" is NOT a traversal, but the check is `path.contains("..")`,
        // so it is rejected anyway. Asserting the real (over-strict) behaviour.
        let bytes = build(vec![
            ZipFileEntry::file("a..b.txt", b"harmless".to_vec()),
            ZipFileEntry::file("ok.txt", b"ok".to_vec()),
        ]);

        let strict = ZipReadConfig::default();
        match ZipFile::from_bytes(&bytes, &strict) {
            Err(ZipReadError::UnsafePath(p)) => assert_eq!(p, "a..b.txt"),
            other => panic!("expected UnsafePath, got {other:?}"),
        }
        match ZipFile::list(&bytes, &strict) {
            Err(ZipReadError::UnsafePath(p)) => assert_eq!(p, "a..b.txt"),
            other => panic!("expected UnsafePath from list(), got {other:?}"),
        }

        // ...and the whole archive is rejected, not just the offending entry
        let loose = ZipReadConfig::new().with_allow_path_traversal(true);
        let round = ZipFile::from_bytes(&bytes, &loose).unwrap();
        assert_eq!(round.entries.len(), 2);
        assert_eq!(round.get("a..b.txt").unwrap().data, b"harmless");
        assert_eq!(ZipFile::list(&bytes, &loose).unwrap().len(), 2);

        // a real traversal path is rejected under the strict config too
        let evil = build(vec![ZipFileEntry::file("../../etc/passwd", b"x".to_vec())]);
        assert!(matches!(
            ZipFile::from_bytes(&evil, &strict),
            Err(ZipReadError::UnsafePath(_))
        ));
        assert!(ZipFile::from_bytes(&evil, &loose).is_ok());

        // a path with a single dot is fine
        let dotted = build(vec![ZipFileEntry::file("./a.txt", b"x".to_vec())]);
        assert!(ZipFile::from_bytes(&dotted, &strict).is_ok());
    }

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_max_file_size_is_enforced_by_from_bytes_only() {
        let payload = vec![b'q'; 1000];
        let bytes = build(vec![ZipFileEntry::file("big.bin", payload.clone())]);

        // 0 means unlimited
        let unlimited = ZipReadConfig::new().with_max_file_size(0);
        assert_eq!(
            ZipFile::from_bytes(&bytes, &unlimited).unwrap().entries[0].data,
            payload
        );

        // exactly at the limit is allowed; one below is not
        let at = ZipReadConfig::new().with_max_file_size(1000);
        assert!(ZipFile::from_bytes(&bytes, &at).is_ok());
        let under = ZipReadConfig::new().with_max_file_size(999);
        match ZipFile::from_bytes(&bytes, &under) {
            Err(ZipReadError::FileTooLarge {
                path,
                size,
                max_size,
            }) => {
                assert_eq!(path, "big.bin");
                assert_eq!(size, 1000);
                assert_eq!(max_size, 999);
            }
            other => panic!("expected FileTooLarge, got {other:?}"),
        }
        assert!(ZipFile::from_bytes(&bytes, &ZipReadConfig::new().with_max_file_size(1)).is_err());
        assert!(zip_extract_all(&bytes, &under).is_err());

        // NOTE: list() deliberately ignores max_file_size (metadata only), so a
        // 1-byte limit still lists a 1000-byte entry. Documented, not enforced.
        let listed = ZipFile::list(&bytes, &under).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].size, 1000);
        assert!(listed[0].compressed_size > 0);
        assert_eq!(zip_list_contents(&bytes, &under).unwrap().len(), 1);
    }

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_get_single_file_lookup_semantics() {
        let bytes = build(vec![
            ZipFileEntry::file("a.txt", b"AAA".to_vec()),
            ZipFileEntry::directory("sub/"),
        ]);
        let cfg = ZipReadConfig::default();
        let meta = ZipFile::list(&bytes, &cfg).unwrap();

        // positive control: every listed entry is retrievable and matches from_bytes
        let loaded = ZipFile::from_bytes(&bytes, &cfg).unwrap();
        for m in &meta {
            let got = ZipFile::get_single_file(&bytes, m, &cfg).unwrap();
            assert_eq!(
                got.as_deref(),
                Some(loaded.get(&m.path).unwrap().data.as_slice())
            );
        }

        // a directory yields an empty payload, not an error
        let dir = meta.iter().find(|m| m.is_directory).unwrap();
        assert_eq!(
            ZipFile::get_single_file(&bytes, dir, &cfg).unwrap(),
            Some(Vec::new())
        );

        // missing / junk paths return Ok(None), never Err and never a panic
        for p in nasty_paths() {
            let entry = ZipPathEntry {
                path: p.clone(),
                is_directory: false,
                size: 0,
                compressed_size: 0,
                crc32: 0,
            };
            assert_eq!(
                ZipFile::get_single_file(&bytes, &entry, &cfg).unwrap(),
                None,
                "expected None for {p:?}"
            );
        }

        // malformed archive data surfaces as InvalidFormat
        let entry = ZipPathEntry {
            path: "a.txt".into(),
            is_directory: false,
            size: 3,
            compressed_size: 3,
            crc32: 0,
        };
        for junk in [b"".as_slice(), b"   ", b"nope", &[0xFF, 0xFE, 0x00]] {
            assert!(matches!(
                ZipFile::get_single_file(junk, &entry, &cfg),
                Err(ZipReadError::InvalidFormat(_))
            ));
        }
    }

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_get_single_file_size_check_runs_before_parsing() {
        // The limit check is done on the caller-supplied entry, before the archive
        // is even opened - so garbage bytes still yield FileTooLarge.
        let cfg = ZipReadConfig::new().with_max_file_size(10);
        let entry = ZipPathEntry {
            path: "x".into(),
            is_directory: false,
            size: 11,
            compressed_size: 0,
            crc32: 0,
        };
        match ZipFile::get_single_file(b"total garbage", &entry, &cfg) {
            Err(ZipReadError::FileTooLarge {
                path,
                size,
                max_size,
            }) => {
                assert_eq!(path, "x");
                assert_eq!(size, 11);
                assert_eq!(max_size, 10);
            }
            other => panic!("expected FileTooLarge before parsing, got {other:?}"),
        }

        // boundary: size == max is allowed through to the parser
        let at_limit = ZipPathEntry {
            size: 10,
            ..entry.clone()
        };
        assert!(matches!(
            ZipFile::get_single_file(b"total garbage", &at_limit, &cfg),
            Err(ZipReadError::InvalidFormat(_))
        ));

        // max_file_size == 0 disables the check entirely, even for u64::MAX sizes
        let unlimited = ZipReadConfig::default();
        let huge = ZipPathEntry {
            size: u64::MAX,
            ..entry
        };
        assert!(matches!(
            ZipFile::get_single_file(b"total garbage", &huge, &unlimited),
            Err(ZipReadError::InvalidFormat(_))
        ));
    }

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_get_single_file_trusts_the_callers_metadata() {
        // BUG (documented, not fixed): get_single_file checks `entry.size` -- which
        // the caller (or a hostile archive header) supplies -- instead of the real
        // entry size, so a lying entry walks straight past max_file_size.
        let payload = vec![b'z'; 5000];
        let bytes = build(vec![ZipFileEntry::file("big.bin", payload.clone())]);

        let capped = ZipReadConfig::new().with_max_file_size(10);
        let liar = ZipPathEntry {
            path: "big.bin".into(),
            is_directory: false,
            size: 0, // lie: real size is 5000
            compressed_size: 0,
            crc32: 0,
        };
        let got = ZipFile::get_single_file(&bytes, &liar, &capped).unwrap();
        assert_eq!(
            got,
            Some(payload),
            "the 10-byte cap was bypassed by a lying entry.size"
        );
        // ...while from_bytes with the same config correctly refuses:
        assert!(matches!(
            ZipFile::from_bytes(&bytes, &capped),
            Err(ZipReadError::FileTooLarge { .. })
        ));

        // BUG (documented, not fixed): get_single_file also performs no path
        // traversal check at all, unlike list()/from_bytes().
        let bytes = build(vec![ZipFileEntry::file("../evil.txt", b"pwned".to_vec())]);
        let strict = ZipReadConfig::default();
        assert!(matches!(
            ZipFile::from_bytes(&bytes, &strict),
            Err(ZipReadError::UnsafePath(_))
        ));
        let entry = ZipPathEntry {
            path: "../evil.txt".into(),
            is_directory: false,
            size: 5,
            compressed_size: 5,
            crc32: 0,
        };
        assert_eq!(
            ZipFile::get_single_file(&bytes, &entry, &strict).unwrap(),
            Some(b"pwned".to_vec()),
            "get_single_file has no UnsafePath guard"
        );
    }

    /// BUG (documented, not fixed): `get_single_file` does
    /// `Vec::with_capacity(usize::try_from(entry.size).unwrap_or(0))` on the
    /// *declared* size. A hostile archive header (surfaced verbatim by `list()`)
    /// declaring `u64::MAX` therefore aborts the process with "capacity overflow"
    /// before a single byte is read. Should be a bounded/incremental read.
    #[cfg(all(feature = "zip", target_pointer_width = "64"))]
    #[test]
    #[should_panic]
    fn autotest_bug_get_single_file_capacity_overflow_on_declared_size() {
        let bytes = build(vec![ZipFileEntry::file("a.txt", b"AAA".to_vec())]);
        let entry = ZipPathEntry {
            path: "a.txt".into(),
            is_directory: false,
            size: u64::MAX, // max_file_size == 0 means "unlimited", so this passes the check
            compressed_size: 3,
            crc32: 0,
        };
        let _ = ZipFile::get_single_file(&bytes, &entry, &ZipReadConfig::default());
    }

    // ==================================================================
    // writer: configurations that cannot produce an archive
    // ==================================================================

    /// BUG (documented, not fixed): `to_bytes` always passes
    /// `compression_level(Some(..))`, but the backend rejects *any* explicit level
    /// for `Stored`. `ZipWriteConfig::store()` therefore cannot write a single
    /// file entry -- uncompressed archives are unreachable through this API.
    #[cfg(feature = "zip")]
    #[test]
    fn autotest_bug_store_config_cannot_write_file_entries() {
        let cfg = ZipWriteConfig::store();
        let err = zip_create(vec![ZipFileEntry::file("a.txt", b"A".to_vec())], &cfg)
            .expect_err("store() unexpectedly produced an archive");
        assert!(
            err.to_string().contains("compression level"),
            "unexpected error for store(): {err}"
        );
        assert!(matches!(err, ZipWriteError::IoError(_)));

        // ...but an archive with no file entries still succeeds, which makes the
        // failure look intermittent to callers.
        assert!(ZipFile::new().to_bytes(&cfg).is_ok());

        // any compression_method != 0 maps to Deflate and works
        let mut deflate_ish = ZipWriteConfig::store();
        deflate_ish.compression_method = 2;
        deflate_ish.compression_level = 6;
        assert!(zip_create(
            vec![ZipFileEntry::file("a.txt", b"A".to_vec())],
            &deflate_ish
        )
        .is_ok());
    }

    /// BUG (documented, not fixed): `ZipWriteConfig::deflate(0)` is accepted by the
    /// builder (`0.min(9) == 0`) but the deflate backend's valid level range starts
    /// at 1, so the resulting config can never write a file.
    #[cfg(feature = "zip")]
    #[test]
    fn autotest_bug_deflate_level_zero_is_unwritable() {
        let cfg = ZipWriteConfig::deflate(0);
        assert_eq!(cfg.compression_level, 0, "builder accepted level 0");
        let err = zip_create(vec![ZipFileEntry::file("a.txt", b"A".to_vec())], &cfg)
            .expect_err("deflate(0) unexpectedly produced an archive");
        assert!(
            err.to_string().contains("compression level"),
            "unexpected error for deflate(0): {err}"
        );
        // level 1 is the first level that actually works
        assert!(zip_create(
            vec![ZipFileEntry::file("a.txt", b"A".to_vec())],
            &ZipWriteConfig::deflate(1)
        )
        .is_ok());
    }

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_duplicate_paths_make_the_archive_unwritable() {
        // add_file() de-duplicates, but ZipFile.entries is a public field and
        // zip_create() takes an arbitrary Vec, so duplicates reach the writer.
        let cfg = ZipWriteConfig::default();
        let err = zip_create(
            vec![
                ZipFileEntry::file("dup.txt", b"1".to_vec()),
                ZipFileEntry::file("dup.txt", b"2".to_vec()),
            ],
            &cfg,
        )
        .expect_err("duplicate paths unexpectedly accepted");
        assert!(matches!(err, ZipWriteError::IoError(_)));
        assert!(!err.to_string().is_empty());

        // zip_create_from_files has the same hazard
        assert!(zip_create_from_files(
            vec![
                ("d".to_string(), b"1".to_vec()),
                ("d".to_string(), b"2".to_vec()),
            ],
            &cfg
        )
        .is_err());

        // going through add_file() is safe because it de-duplicates first
        let mut zip = ZipFile::new();
        zip.add_file("dup.txt", b"1".to_vec());
        zip.add_file("dup.txt", b"2".to_vec());
        let bytes = zip.to_bytes(&cfg).unwrap();
        assert_eq!(
            ZipFile::from_bytes(&bytes, &ZipReadConfig::default())
                .unwrap()
                .get("dup.txt")
                .unwrap()
                .data,
            b"2"
        );
    }

    #[cfg(feature = "zip")]
    #[test]
    fn autotest_to_bytes_with_hostile_paths_never_panics() {
        let cfg = ZipWriteConfig::default();
        let loose = ZipReadConfig::new().with_allow_path_traversal(true);
        // BUG (documented, NOT exercised here): nothing validates path length, and
        // the ZIP file-name field is a u16. A path of 65_536+ bytes panics inside
        // the writer (`file_name_raw.len().try_into().unwrap()`) instead of
        // returning `ZipWriteError::InvalidPath`. It cannot be asserted with
        // #[should_panic] because ZipWriter::drop re-panics on the same unwrap
        // while unwinding, which aborts the process. Hence the <60_000 filter.
        // Each path is written into its own archive so one rejection does not mask
        // the others; the contract under test is "Ok or Err, never a panic".
        for p in nasty_paths().into_iter().filter(|p| p.len() < 60_000) {
            if let Ok(bytes) = zip_create(vec![ZipFileEntry::file(p.clone(), b"x".to_vec())], &cfg)
            {
                // if it encoded, it must decode back without panicking
                let _ = ZipFile::from_bytes(&bytes, &loose);
            }
            if let Ok(bytes) = zip_create(vec![ZipFileEntry::directory(p)], &cfg) {
                let _ = ZipFile::from_bytes(&bytes, &loose);
            }
        }

        // a 60_000-byte path is under the u16 field limit and must round-trip
        let long = "L".repeat(60_000);
        let bytes = zip_create(vec![ZipFileEntry::file(long.clone(), b"x".to_vec())], &cfg)
            .expect("60_000-byte path must be writable");
        assert_eq!(
            ZipFile::from_bytes(&bytes, &loose)
                .unwrap()
                .get(&long)
                .unwrap()
                .data,
            b"x"
        );
    }

    // ==================================================================
    // file-system entry points
    // ==================================================================

    #[cfg(all(feature = "zip", feature = "std"))]
    #[test]
    fn autotest_from_file_missing_path_is_io_error() {
        let cfg = ZipReadConfig::default();
        for p in [
            "/nonexistent_dir_azul_autotest_zip/sub/archive.zip",
            "",
            "/nonexistent_dir_azul_autotest_zip/\u{1F600}.zip",
        ] {
            match ZipFile::from_file(Path::new(p), &cfg) {
                Err(ZipReadError::IoError(msg)) => assert!(!msg.is_empty()),
                other => panic!("expected IoError for {p:?}, got {other:?}"),
            }
        }

        // a directory is not a readable archive either
        let tmp = std::env::temp_dir();
        assert!(ZipFile::from_file(&tmp, &cfg).is_err());
    }

    #[cfg(all(feature = "zip", feature = "std"))]
    #[test]
    fn autotest_to_file_unwritable_path_is_io_error() {
        let mut zip = ZipFile::new();
        zip.add_file("a.txt", b"A".to_vec());
        let cfg = ZipWriteConfig::default();
        match zip.to_file(
            Path::new("/nonexistent_dir_azul_autotest_zip/sub/out.zip"),
            &cfg,
        ) {
            Err(ZipWriteError::IoError(msg)) => assert!(!msg.is_empty()),
            other => panic!("expected IoError, got {other:?}"),
        }

        // a write-config failure is reported before the filesystem is touched
        let store = ZipWriteConfig::store();
        assert!(zip
            .to_file(
                Path::new("/nonexistent_dir_azul_autotest_zip/x.zip"),
                &store
            )
            .is_err());
    }

    #[cfg(all(feature = "zip", feature = "std"))]
    #[test]
    fn autotest_file_roundtrip_via_temp_dir() {
        let mut zip = ZipFile::new();
        zip.add_file("a.txt", b"AAA".to_vec());
        zip.add_file("\u{1F600}/b.bin", vec![0u8, 255, 128]);
        zip.add_directory("d/");

        let path = std::env::temp_dir().join(format!(
            "azul_autotest_zip_roundtrip_{}.zip",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        match zip.to_file(&path, &ZipWriteConfig::default()) {
            Ok(()) => {
                let round = ZipFile::from_file(&path, &ZipReadConfig::default())
                    .expect("archive written by to_file must be readable");
                assert_eq!(round.entries.len(), 3);
                assert_eq!(round.get("a.txt").unwrap().data, b"AAA");
                assert_eq!(
                    round.get("\u{1F600}/b.bin").unwrap().data,
                    vec![0u8, 255, 128]
                );
                assert!(round.get("d/").unwrap().is_directory);
                // to_file and to_bytes must produce identical content
                let in_memory = zip.to_bytes(&ZipWriteConfig::default()).unwrap();
                let on_disk = std::fs::read(&path).unwrap();
                assert_eq!(in_memory.len(), on_disk.len());
                let _ = std::fs::remove_file(&path);
            }
            Err(ZipWriteError::IoError(_)) => {
                // temp dir not writable in this environment - nothing to assert
            }
            Err(other) => panic!("unexpected write error: {other:?}"),
        }
    }
}
