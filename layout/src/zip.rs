//! ZIP file manipulation module for C API exposure
//!
//! Provides a ZipFile struct for reading/writing ZIP archives.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
use core::fmt;

#[cfg(feature = "std")]
use std::path::Path;

// ============================================================================
// Configuration types
// ============================================================================

/// Configuration for reading ZIP archives
#[derive(Debug, Clone, Default)]
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
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_max_file_size(mut self, max_size: u64) -> Self {
        self.max_file_size = max_size;
        self
    }
    
    pub fn with_allow_path_traversal(mut self, allow: bool) -> Self {
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
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn store() -> Self {
        Self {
            compression_method: 0,
            ..Default::default()
        }
    }
    
    pub fn deflate(level: u8) -> Self {
        Self {
            compression_method: 1,
            compression_level: level.min(9),
            ..Default::default()
        }
    }
    
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

/// Vec of ZipPathEntry
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

/// Vec of ZipFileEntry  
pub type ZipFileEntryVec = Vec<ZipFileEntry>;

// ============================================================================
// Error types
// ============================================================================

/// Error when reading ZIP archives
#[derive(Debug, Clone, PartialEq)]
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
    FileTooLarge { path: String, size: u64, max_size: u64 },
}

impl fmt::Display for ZipReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZipReadError::InvalidFormat(msg) => write!(f, "Invalid ZIP format: {}", msg),
            ZipReadError::FileNotFound(path) => write!(f, "File not found: {}", path),
            ZipReadError::IoError(msg) => write!(f, "I/O error: {}", msg),
            ZipReadError::UnsafePath(path) => write!(f, "Unsafe path: {}", path),
            ZipReadError::EncryptedFile(path) => write!(f, "Encrypted file: {}", path),
            ZipReadError::FileTooLarge { path, size, max_size } => {
                write!(f, "File too large: {} ({} > {})", path, size, max_size)
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ZipReadError {}

/// Error when writing ZIP archives
#[derive(Debug, Clone, PartialEq)]
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
            ZipWriteError::IoError(msg) => write!(f, "I/O error: {}", msg),
            ZipWriteError::InvalidPath(path) => write!(f, "Invalid path: {}", path),
            ZipWriteError::CompressionError(msg) => write!(f, "Compression error: {}", msg),
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
    pub fn new() -> Self {
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
    #[cfg(feature = "zip_support")]
    pub fn list(data: &[u8], config: &ZipReadConfig) -> Result<ZipPathEntryVec, ZipReadError> {
        use std::io::Cursor;
        
        let cursor = Cursor::new(data);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| ZipReadError::InvalidFormat(e.to_string()))?;
        
        let mut entries = Vec::new();
        
        for i in 0..archive.len() {
            let file = archive.by_index(i)
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
    #[cfg(feature = "zip_support")]
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
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| ZipReadError::InvalidFormat(e.to_string()))?;
        
        let mut file = match archive.by_name(&entry.path) {
            Ok(f) => f,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(e) => return Err(ZipReadError::IoError(e.to_string())),
        };
        
        if file.is_dir() {
            return Ok(Some(Vec::new()));
        }
        
        let mut contents = Vec::with_capacity(entry.size as usize);
        file.read_to_end(&mut contents)
            .map_err(|e| ZipReadError::IoError(e.to_string()))?;
        
        Ok(Some(contents))
    }
    
    /// Load a ZIP archive from bytes
    /// 
    /// # Arguments
    /// * `data` - ZIP file bytes (consumed)
    /// * `config` - Read configuration
    #[cfg(feature = "zip_support")]
    pub fn from_bytes(data: Vec<u8>, config: &ZipReadConfig) -> Result<Self, ZipReadError> {
        use std::io::{Cursor, Read};
        
        let cursor = Cursor::new(&data);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| ZipReadError::InvalidFormat(e.to_string()))?;
        
        let mut entries = Vec::new();
        
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)
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
    #[cfg(all(feature = "zip_support", feature = "std"))]
    pub fn from_file(path: &Path, config: &ZipReadConfig) -> Result<Self, ZipReadError> {
        let data = std::fs::read(path)
            .map_err(|e| ZipReadError::IoError(e.to_string()))?;
        Self::from_bytes(data, config)
    }
    
    /// Write the ZIP archive to bytes
    /// 
    /// # Arguments
    /// * `config` - Write configuration
    #[cfg(feature = "zip_support")]
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
            .unix_permissions(config.unix_permissions);
        
        for entry in &self.entries {
            if entry.is_directory {
                writer.add_directory(&entry.path, options)
                    .map_err(|e| ZipWriteError::IoError(e.to_string()))?;
            } else {
                writer.start_file(&entry.path, options)
                    .map_err(|e| ZipWriteError::IoError(e.to_string()))?;
                writer.write_all(&entry.data)
                    .map_err(|e| ZipWriteError::IoError(e.to_string()))?;
            }
        }
        
        let result = writer.finish()
            .map_err(|e| ZipWriteError::IoError(e.to_string()))?;
        
        Ok(result.into_inner())
    }
    
    /// Write the ZIP archive to a file
    #[cfg(all(feature = "zip_support", feature = "std"))]
    pub fn to_file(&self, path: &Path, config: &ZipWriteConfig) -> Result<(), ZipWriteError> {
        let data = self.to_bytes(config)?;
        std::fs::write(path, data)
            .map_err(|e| ZipWriteError::IoError(e.to_string()))?;
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
    pub fn get(&self, path: &str) -> Option<&ZipFileEntry> {
        self.entries.iter().find(|e| e.path == path)
    }
    
    /// Check if archive contains a path
    pub fn contains(&self, path: &str) -> bool {
        self.entries.iter().any(|e| e.path == path)
    }
    
    /// Get list of all paths
    pub fn paths(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.path.as_str()).collect()
    }
    
    /// Filter entries by suffix (e.g., ".fluent", ".json")
    pub fn filter_by_suffix(&self, suffix: &str) -> Vec<&ZipFileEntry> {
        self.entries.iter()
            .filter(|e| !e.is_directory && e.path.ends_with(suffix))
            .collect()
    }
}

// ============================================================================
// Convenience functions (for simpler use cases)
// ============================================================================

/// Create a ZIP archive from file entries (consumes entries, no clone)
#[cfg(feature = "zip_support")]
pub fn zip_create(entries: Vec<ZipFileEntry>, config: &ZipWriteConfig) -> Result<Vec<u8>, ZipWriteError> {
    let zip = ZipFile { entries };
    zip.to_bytes(config)
}

/// Create a ZIP archive from path/data pairs (consumes entries, no clone)
#[cfg(feature = "zip_support")]
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
#[cfg(feature = "zip_support")]
pub fn zip_extract_all(data: &[u8], config: &ZipReadConfig) -> Result<Vec<ZipFileEntry>, ZipReadError> {
    let zip = ZipFile::from_bytes(data.to_vec(), config)?;
    Ok(zip.entries)
}

/// List contents of ZIP data without extracting
#[cfg(feature = "zip_support")]
pub fn zip_list_contents(data: &[u8], config: &ZipReadConfig) -> Result<Vec<ZipPathEntry>, ZipReadError> {
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
    
    #[cfg(feature = "zip_support")]
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
    
    #[cfg(feature = "zip_support")]
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
