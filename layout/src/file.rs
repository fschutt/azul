//! File system operations module for C API
//!
//! Provides C-compatible wrappers around Rust's std::fs API.
//! This allows C code to use Rust's file operations without importing stdio.h.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use azul_css::{AzString, U8Vec, EmptyStruct, impl_result, impl_result_inner, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, impl_option, impl_option_inner};

#[cfg(feature = "std")]
use std::path::Path;

#[cfg(feature = "std")]
fn path_to_azstring(p: impl AsRef<Path>) -> AzString {
    AzString::from(p.as_ref().to_string_lossy().into_owned())
}

// ============================================================================
// Error types
// ============================================================================

/// Error when performing file operations
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct FileError {
    /// Error message
    pub message: AzString,
    /// Error kind
    pub kind: FileErrorKind,
}

/// Kind of file error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum FileErrorKind {
    /// File or directory not found
    NotFound,
    /// Permission denied
    PermissionDenied,
    /// File already exists
    AlreadyExists,
    /// Invalid path
    InvalidPath,
    /// I/O error
    IoError,
    /// Directory not empty
    DirectoryNotEmpty,
    /// Is a directory (expected file)
    IsDirectory,
    /// Is a file (expected directory)
    IsFile,
    /// Other error
    Other,
}

impl FileError {
    pub fn new(kind: FileErrorKind, message: impl Into<String>) -> Self {
        Self {
            message: AzString::from(message.into()),
            kind,
        }
    }
    
    #[cfg(feature = "std")]
    // taken by value so it can be used directly as `.map_err(FileError::from_io_error)`
    // (map_err yields an owned io::Error); a &-param would break every such call site.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use] pub fn from_io_error(e: std::io::Error) -> Self {
        use std::io::ErrorKind;
        
        let kind = match e.kind() {
            ErrorKind::NotFound => FileErrorKind::NotFound,
            ErrorKind::PermissionDenied => FileErrorKind::PermissionDenied,
            ErrorKind::AlreadyExists => FileErrorKind::AlreadyExists,
            ErrorKind::IsADirectory => FileErrorKind::IsDirectory,
            ErrorKind::DirectoryNotEmpty => FileErrorKind::DirectoryNotEmpty,
            _ => FileErrorKind::IoError,
        };
        
        Self {
            message: AzString::from(e.to_string()),
            kind,
        }
    }
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message.as_str())
    }
}

#[cfg(feature = "std")]
impl std::error::Error for FileError {}

// FFI-safe Result types for file operations
impl_result!(
    EmptyStruct,
    FileError,
    ResultEmptyStructFileError,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl_result!(
    U8Vec,
    FileError,
    ResultU8VecFileError,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl_result!(
    AzString,
    FileError,
    ResultStringFileError,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl_result!(
    u64,
    FileError,
    Resultu64FileError,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

// Forward declarations for result types that need later-defined types
// (FilePath, FileMetadata, DirEntryVec are defined below)

// ============================================================================
// File metadata
// ============================================================================

/// File type (file, directory, symlink)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum FileType {
    /// Regular file
    File,
    /// Directory
    Directory,
    /// Symbolic link
    Symlink,
    /// Other (device, socket, etc.)
    Other,
}

/// Metadata about a file
#[derive(Copy, Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct FileMetadata {
    /// File size in bytes
    pub size: u64,
    /// File type
    pub file_type: FileType,
    /// Is read-only
    pub is_readonly: bool,
    /// Last modification time (Unix timestamp in seconds)
    pub modified_secs: u64,
    /// Creation time (Unix timestamp in seconds, 0 if unavailable)
    pub created_secs: u64,
}

/// A directory entry
#[derive(Debug, Clone)]
#[repr(C)]
pub struct DirEntry {
    /// File name (not full path)
    pub name: AzString,
    /// Full path
    pub path: AzString,
    /// File type
    pub file_type: FileType,
}

/// Vec of DirEntry
impl_option!(DirEntry, OptionDirEntry, copy = false, [Debug, Clone]);
impl_vec!(DirEntry, DirEntryVec, DirEntryVecDestructor, DirEntryVecDestructorType, DirEntryVecSlice, OptionDirEntry);
impl_vec_clone!(DirEntry, DirEntryVec, DirEntryVecDestructor);
impl_vec_debug!(DirEntry, DirEntryVec);
impl_vec_mut!(DirEntry, DirEntryVec);

// Additional FFI-safe Result types for complex types
impl_result!(
    FileMetadata,
    FileError,
    ResultFileMetadataFileError,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl_result!(
    DirEntryVec,
    FileError,
    ResultDirEntryVecFileError,
    copy = false,
    clone = false,
    [Debug, Clone]
);

// ============================================================================
// File operations
// ============================================================================

/// Read a file to bytes
#[cfg(feature = "std")]
/// # Errors
///
/// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
pub fn file_read(path: &str) -> Result<U8Vec, FileError> {
    let data = std::fs::read(path)
        .map_err(FileError::from_io_error)?;
    Ok(U8Vec::from(data))
}

/// Read a file to string (UTF-8)
#[cfg(feature = "std")]
/// # Errors
///
/// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
pub fn file_read_string(path: &str) -> Result<AzString, FileError> {
    let data = std::fs::read_to_string(path)
        .map_err(FileError::from_io_error)?;
    Ok(AzString::from(data))
}

/// Write bytes to a file (creates or overwrites)
#[cfg(feature = "std")]
/// # Errors
///
/// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
pub fn file_write(path: &str, data: &[u8]) -> Result<EmptyStruct, FileError> {
    std::fs::write(path, data)
        .map(|()| EmptyStruct::default()).map_err(FileError::from_io_error)
}

/// Write string to a file (creates or overwrites)
#[cfg(feature = "std")]
/// # Errors
///
/// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
pub fn file_write_string(path: &str, data: &str) -> Result<EmptyStruct, FileError> {
    std::fs::write(path, data.as_bytes())
        .map(|()| EmptyStruct::default()).map_err(FileError::from_io_error)
}

/// Append bytes to a file
#[cfg(feature = "std")]
/// # Errors
///
/// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
pub fn file_append(path: &str, data: &[u8]) -> Result<EmptyStruct, FileError> {
    use std::fs::OpenOptions;
    use std::io::Write;
    
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(FileError::from_io_error)?;
    
    file.write_all(data)
        .map(|()| EmptyStruct::default())
        .map_err(FileError::from_io_error)
}

/// Copy a file
#[cfg(feature = "std")]
/// # Errors
///
/// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
pub fn file_copy(from: &str, to: &str) -> Result<u64, FileError> {
    std::fs::copy(from, to)
        .map_err(FileError::from_io_error)
}

/// Rename/move a file
#[cfg(feature = "std")]
/// # Errors
///
/// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
pub fn file_rename(from: &str, to: &str) -> Result<EmptyStruct, FileError> {
    std::fs::rename(from, to)
        .map(|()| EmptyStruct::default()).map_err(FileError::from_io_error)
}

/// Delete a file
#[cfg(feature = "std")]
/// # Errors
///
/// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
pub fn file_delete(path: &str) -> Result<EmptyStruct, FileError> {
    std::fs::remove_file(path)
        .map(|()| EmptyStruct::default()).map_err(FileError::from_io_error)
}

/// Check if a file or directory exists
#[cfg(feature = "std")]
#[must_use] pub fn path_exists(path: &str) -> bool {
    Path::new(path).exists()
}

/// Check if path is a file
#[cfg(feature = "std")]
#[must_use] pub fn path_is_file(path: &str) -> bool {
    Path::new(path).is_file()
}

/// Check if path is a directory
#[cfg(feature = "std")]
#[must_use] pub fn path_is_dir(path: &str) -> bool {
    Path::new(path).is_dir()
}

/// Get file metadata
#[cfg(feature = "std")]
/// # Errors
///
/// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
pub fn file_metadata(path: &str) -> Result<FileMetadata, FileError> {
    let meta = std::fs::symlink_metadata(path)
        .map_err(FileError::from_io_error)?;
    
    let file_type = if meta.is_file() {
        FileType::File
    } else if meta.is_dir() {
        FileType::Directory
    } else if meta.is_symlink() {
        FileType::Symlink
    } else {
        FileType::Other
    };
    
    let modified_secs = meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    
    let created_secs = meta.created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    
    Ok(FileMetadata {
        size: meta.len(),
        file_type,
        is_readonly: meta.permissions().readonly(),
        modified_secs,
        created_secs,
    })
}

// ----------------------------------------------------------------------------
// no_std stubs: keep the file API surface present without std::fs.
// ----------------------------------------------------------------------------

/// Error returned by file stubs when the `std` feature is disabled.
#[cfg(not(feature = "std"))]
fn no_std_file_error() -> FileError {
    FileError::new(FileErrorKind::Other, "file IO requires the `std` feature")
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn file_read(_path: &str) -> Result<U8Vec, FileError> {
    Err(no_std_file_error())
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn file_read_string(_path: &str) -> Result<AzString, FileError> {
    Err(no_std_file_error())
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn file_write(_path: &str, _data: &[u8]) -> Result<EmptyStruct, FileError> {
    Err(no_std_file_error())
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn file_write_string(_path: &str, _data: &str) -> Result<EmptyStruct, FileError> {
    Err(no_std_file_error())
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn file_append(_path: &str, _data: &[u8]) -> Result<EmptyStruct, FileError> {
    Err(no_std_file_error())
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn file_copy(_from: &str, _to: &str) -> Result<u64, FileError> {
    Err(no_std_file_error())
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn file_rename(_from: &str, _to: &str) -> Result<EmptyStruct, FileError> {
    Err(no_std_file_error())
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn file_delete(_path: &str) -> Result<EmptyStruct, FileError> {
    Err(no_std_file_error())
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn path_exists(_path: &str) -> bool {
    false
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn path_is_file(_path: &str) -> bool {
    false
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn path_is_dir(_path: &str) -> bool {
    false
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn file_metadata(_path: &str) -> Result<FileMetadata, FileError> {
    Err(no_std_file_error())
}

// ============================================================================
// Directory operations
// ============================================================================

/// Create a directory
#[cfg(feature = "std")]
/// # Errors
///
/// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
pub fn dir_create(path: &str) -> Result<EmptyStruct, FileError> {
    std::fs::create_dir(path)
        .map(|()| EmptyStruct::default()).map_err(FileError::from_io_error)
}

/// Create a directory and all parent directories
#[cfg(feature = "std")]
/// # Errors
///
/// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
pub fn dir_create_all(path: &str) -> Result<EmptyStruct, FileError> {
    std::fs::create_dir_all(path)
        .map(|()| EmptyStruct::default()).map_err(FileError::from_io_error)
}

/// Delete an empty directory
#[cfg(feature = "std")]
/// # Errors
///
/// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
pub fn dir_delete(path: &str) -> Result<EmptyStruct, FileError> {
    std::fs::remove_dir(path)
        .map(|()| EmptyStruct::default()).map_err(FileError::from_io_error)
}

/// Delete a directory and all its contents
#[cfg(feature = "std")]
/// # Errors
///
/// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
pub fn dir_delete_all(path: &str) -> Result<EmptyStruct, FileError> {
    std::fs::remove_dir_all(path)
        .map(|()| EmptyStruct::default()).map_err(FileError::from_io_error)
}

/// List directory contents
#[cfg(feature = "std")]
/// # Errors
///
/// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
pub fn dir_list(path: &str) -> Result<DirEntryVec, FileError> {
    let entries = std::fs::read_dir(path)
        .map_err(FileError::from_io_error)?;
    
    let mut result = Vec::new();
    
    for entry in entries {
        let entry = entry.map_err(FileError::from_io_error)?;
        let file_type = entry.file_type()
            .map(|ft| {
                if ft.is_file() {
                    FileType::File
                } else if ft.is_dir() {
                    FileType::Directory
                } else if ft.is_symlink() {
                    FileType::Symlink
                } else {
                    FileType::Other
                }
            })
            .unwrap_or(FileType::Other);
        
        result.push(DirEntry {
            name: path_to_azstring(entry.file_name()),
            path: path_to_azstring(entry.path()),
            file_type,
        });
    }
    
    Ok(DirEntryVec::from_vec(result))
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn dir_create(_path: &str) -> Result<EmptyStruct, FileError> {
    Err(no_std_file_error())
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn dir_create_all(_path: &str) -> Result<EmptyStruct, FileError> {
    Err(no_std_file_error())
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn dir_delete(_path: &str) -> Result<EmptyStruct, FileError> {
    Err(no_std_file_error())
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn dir_delete_all(_path: &str) -> Result<EmptyStruct, FileError> {
    Err(no_std_file_error())
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn dir_list(_path: &str) -> Result<DirEntryVec, FileError> {
    Err(no_std_file_error())
}

// ============================================================================
// Path operations
// ============================================================================

/// Join two paths
#[cfg(feature = "std")]
#[must_use] pub fn path_join(base: &str, path: &str) -> AzString {
    let joined = Path::new(base).join(path);
    path_to_azstring(joined)
}

/// Get the parent directory of a path
#[cfg(feature = "std")]
pub fn path_parent(path: &str) -> Option<AzString> {
    Path::new(path).parent()
        .map(path_to_azstring)
}

/// Get the file name from a path
#[cfg(feature = "std")]
pub fn path_file_name(path: &str) -> Option<AzString> {
    Path::new(path).file_name()
        .map(path_to_azstring)
}

/// Get the file extension from a path
#[cfg(feature = "std")]
pub fn path_extension(path: &str) -> Option<AzString> {
    Path::new(path).extension()
        .map(path_to_azstring)
}

/// Canonicalize a path (resolve symlinks, make absolute)
#[cfg(feature = "std")]
/// # Errors
///
/// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
pub fn path_canonicalize(path: &str) -> Result<AzString, FileError> {
    let canonical = std::fs::canonicalize(path)
        .map_err(FileError::from_io_error)?;
    Ok(path_to_azstring(canonical))
}

// ============================================================================
// Temporary files
// ============================================================================

/// Get the system temporary directory
#[cfg(feature = "std")]
#[must_use] pub fn temp_dir() -> AzString {
    path_to_azstring(std::env::temp_dir())
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn path_join(_base: &str, _path: &str) -> AzString {
    AzString::from_const_str("")
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn path_parent(_path: &str) -> Option<AzString> {
    None
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn path_file_name(_path: &str) -> Option<AzString> {
    None
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn path_extension(_path: &str) -> Option<AzString> {
    None
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn path_canonicalize(_path: &str) -> Result<AzString, FileError> {
    Err(no_std_file_error())
}

/// Stub: `std` feature disabled.
#[cfg(not(feature = "std"))]
pub fn temp_dir() -> AzString {
    AzString::from_const_str("")
}

// ============================================================================
// OOP-style Path wrapper
// ============================================================================

/// FFI-safe path type with OOP-style methods
/// 
/// This wraps a string path and provides method-based access to file operations.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct FilePath {
    pub inner: AzString,
}

// Result type for FilePath operations (must be after FilePath definition)
impl_result!(
    FilePath,
    FileError,
    ResultFilePathFileError,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

// Option type for FilePath
impl_option!(FilePath, OptionFilePath, copy = false, [Clone, Debug, PartialEq, Eq]);

impl Default for FilePath {
    fn default() -> Self {
        Self { inner: AzString::from_const_str("") }
    }
}

impl FilePath {
    /// Creates a new path from a string
    #[must_use] pub const fn new(path: AzString) -> Self {
        Self { inner: path }
    }

    /// Creates an empty path
    #[must_use] pub fn empty() -> Self {
        Self::default()
    }

    /// Creates a path from a string slice
    #[must_use] pub fn from_str(s: &str) -> Self {
        Self { inner: AzString::from(String::from(s)) }
    }

    /// Returns the system temporary directory
    #[cfg(feature = "std")]
    #[must_use] pub fn get_temp_dir() -> Self {
        Self { inner: temp_dir() }
    }

    /// Returns the current working directory
    #[cfg(feature = "std")]
    /// # Errors
    ///
    /// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
    pub fn get_current_dir() -> Result<Self, FileError> {
        match std::env::current_dir() {
            Ok(p) => Ok(Self { inner: path_to_azstring(p) }),
            Err(e) => Err(FileError::from_io_error(e)),
        }
    }

    /// Returns the user's home directory (e.g., /home/username on Linux, C:\Users\username on Windows)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_home_dir() -> Option<Self> {
        dirs::home_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's cache directory (e.g., ~/.cache on Linux, ~/Library/Caches on macOS)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_cache_dir() -> Option<Self> {
        dirs::cache_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's config directory (e.g., ~/.config on Linux, ~/Library/Application Support on macOS)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_config_dir() -> Option<Self> {
        dirs::config_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's local config directory (e.g., ~/.config on Linux, ~/Library/Application Support on macOS)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_config_local_dir() -> Option<Self> {
        dirs::config_local_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's data directory (e.g., ~/.local/share on Linux, ~/Library/Application Support on macOS)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_data_dir() -> Option<Self> {
        dirs::data_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's local data directory (e.g., ~/.local/share on Linux, ~/Library/Application Support on macOS)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_data_local_dir() -> Option<Self> {
        dirs::data_local_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's desktop directory (e.g., ~/Desktop)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_desktop_dir() -> Option<Self> {
        dirs::desktop_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's documents directory (e.g., ~/Documents)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_document_dir() -> Option<Self> {
        dirs::document_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's downloads directory (e.g., ~/Downloads)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_download_dir() -> Option<Self> {
        dirs::download_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's executable directory (e.g., ~/.local/bin on Linux)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_executable_dir() -> Option<Self> {
        dirs::executable_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's font directory (e.g., ~/.local/share/fonts on Linux, ~/Library/Fonts on macOS)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_font_dir() -> Option<Self> {
        dirs::font_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's pictures directory (e.g., ~/Pictures)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_picture_dir() -> Option<Self> {
        dirs::picture_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's preference directory (e.g., ~/.config on Linux, ~/Library/Preferences on macOS)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_preference_dir() -> Option<Self> {
        dirs::preference_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's public directory (e.g., ~/Public)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_public_dir() -> Option<Self> {
        dirs::public_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's runtime directory (e.g., /run/user/1000 on Linux)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_runtime_dir() -> Option<Self> {
        dirs::runtime_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's state directory (e.g., ~/.local/state on Linux)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_state_dir() -> Option<Self> {
        dirs::state_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's audio directory (e.g., ~/Music)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_audio_dir() -> Option<Self> {
        dirs::audio_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's video directory (e.g., ~/Videos)
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_video_dir() -> Option<Self> {
        dirs::video_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Returns the user's templates directory
    #[cfg(all(feature = "std", feature = "extra"))]
    #[must_use] pub fn get_template_dir() -> Option<Self> {
        dirs::template_dir().map(|p| Self { inner: path_to_azstring(p) })
    }

    /// Joins this path with another path component
    #[cfg(feature = "std")]
    #[must_use] pub fn join(&self, other: &Self) -> Self {
        Self { inner: path_join(self.inner.as_str(), other.inner.as_str()) }
    }

    /// Joins this path with a string component
    #[cfg(feature = "std")]
    #[must_use] pub fn join_str(&self, component: &AzString) -> Self {
        Self { inner: path_join(self.inner.as_str(), component.as_str()) }
    }

    /// Returns the parent directory of this path
    #[cfg(feature = "std")]
    #[must_use] pub fn parent(&self) -> Option<Self> {
        path_parent(self.inner.as_str()).map(|p| Self { inner: p })
    }

    /// Returns the file name component of this path
    #[cfg(feature = "std")]
    #[must_use] pub fn file_name(&self) -> Option<AzString> {
        path_file_name(self.inner.as_str())
    }

    /// Returns the file extension of this path
    #[cfg(feature = "std")]
    #[must_use] pub fn extension(&self) -> Option<AzString> {
        path_extension(self.inner.as_str())
    }

    /// Checks if the path exists on the filesystem
    #[cfg(feature = "std")]
    #[must_use] pub fn exists(&self) -> bool {
        path_exists(self.inner.as_str())
    }

    /// Checks if the path is a file
    #[cfg(feature = "std")]
    #[must_use] pub fn is_file(&self) -> bool {
        path_is_file(self.inner.as_str())
    }

    /// Checks if the path is a directory
    #[cfg(feature = "std")]
    #[must_use] pub fn is_dir(&self) -> bool {
        path_is_dir(self.inner.as_str())
    }

    /// Checks if the path is absolute
    #[cfg(feature = "std")]
    #[must_use] pub fn is_absolute(&self) -> bool {
        Path::new(self.inner.as_str()).is_absolute()
    }

    /// Creates this directory and all parent directories
    #[cfg(feature = "std")]
    /// # Errors
    ///
    /// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
    pub fn create_dir_all(&self) -> Result<EmptyStruct, FileError> {
        dir_create_all(self.inner.as_str())
    }

    /// Creates this directory (parent must exist)
    #[cfg(feature = "std")]
    /// # Errors
    ///
    /// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
    pub fn create_dir(&self) -> Result<EmptyStruct, FileError> {
        dir_create(self.inner.as_str())
    }

    /// Removes this file
    #[cfg(feature = "std")]
    /// # Errors
    ///
    /// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
    pub fn remove_file(&self) -> Result<EmptyStruct, FileError> {
        file_delete(self.inner.as_str())
    }

    /// Removes this directory (must be empty)
    #[cfg(feature = "std")]
    /// # Errors
    ///
    /// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
    pub fn remove_dir(&self) -> Result<EmptyStruct, FileError> {
        dir_delete(self.inner.as_str())
    }

    /// Removes this directory and all contents
    #[cfg(feature = "std")]
    /// # Errors
    ///
    /// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
    pub fn remove_dir_all(&self) -> Result<EmptyStruct, FileError> {
        dir_delete_all(self.inner.as_str())
    }

    /// Reads the entire file at this path as bytes
    #[cfg(feature = "std")]
    /// # Errors
    ///
    /// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
    pub fn read_bytes(&self) -> Result<U8Vec, FileError> {
        file_read(self.inner.as_str())
    }

    /// Reads the entire file at this path as a string
    #[cfg(feature = "std")]
    /// # Errors
    ///
    /// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
    pub fn read_string(&self) -> Result<AzString, FileError> {
        file_read_string(self.inner.as_str())
    }

    /// Writes bytes to the file at this path
    #[cfg(feature = "std")]
    /// # Errors
    ///
    /// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
    pub fn write_bytes(&self, data: &U8Vec) -> Result<EmptyStruct, FileError> {
        file_write(self.inner.as_str(), data.as_ref())
    }

    /// Writes a string to the file at this path
    #[cfg(feature = "std")]
    /// # Errors
    ///
    /// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
    pub fn write_string(&self, data: &AzString) -> Result<EmptyStruct, FileError> {
        file_write_string(self.inner.as_str(), data.as_str())
    }

    /// Copies a file from this path to another path
    #[cfg(feature = "std")]
    /// # Errors
    ///
    /// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
    pub fn copy_to(&self, dest: &Self) -> Result<u64, FileError> {
        file_copy(self.inner.as_str(), dest.inner.as_str())
    }

    /// Renames/moves a file from this path to another path
    #[cfg(feature = "std")]
    /// # Errors
    ///
    /// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
    pub fn rename_to(&self, dest: &Self) -> Result<EmptyStruct, FileError> {
        file_rename(self.inner.as_str(), dest.inner.as_str())
    }

    /// Returns the path as a string reference
    #[must_use] pub fn as_str(&self) -> &str {
        self.inner.as_str()
    }

    /// Returns the path as an `AzString`
    #[must_use] pub fn as_string(&self) -> AzString {
        self.inner.clone()
    }

    /// Lists directory contents
    #[cfg(feature = "std")]
    /// # Errors
    ///
    /// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
    pub fn read_dir(&self) -> Result<DirEntryVec, FileError> {
        dir_list(self.inner.as_str())
    }

    /// Returns metadata about the file/directory
    #[cfg(feature = "std")]
    /// # Errors
    ///
    /// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
    pub fn metadata(&self) -> Result<FileMetadata, FileError> {
        file_metadata(self.inner.as_str())
    }

    /// Makes the path canonical (absolute, with no `.` or `..` components)
    #[cfg(feature = "std")]
    /// # Errors
    ///
    /// Returns a `FileError` if the filesystem operation fails (e.g. path not found, permission denied, or an I/O error).
    pub fn canonicalize(&self) -> Result<Self, FileError> {
        path_canonicalize(self.inner.as_str()).map(|p| Self { inner: p })
    }
}

impl From<String> for FilePath {
    fn from(s: String) -> Self {
        Self { inner: AzString::from(s) }
    }
}

impl From<&str> for FilePath {
    fn from(s: &str) -> Self {
        Self { inner: AzString::from(String::from(s)) }
    }
}

impl From<AzString> for FilePath {
    fn from(s: AzString) -> Self {
        Self { inner: s }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    #[cfg(feature = "std")]
    fn test_temp_dir() {
        let temp = temp_dir();
        assert!(!temp.as_str().is_empty());
    }
    
    #[test]
    #[cfg(feature = "std")]
    fn test_path_join() {
        let joined = path_join("/home/user", "file.txt");
        assert!(joined.as_str().contains("file.txt"));
    }
}

#[cfg(test)]
#[allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::redundant_clone,
    clippy::similar_names,
    clippy::case_sensitive_file_extension_comparisons
)]
mod autotest_generated {
    use alloc::string::ToString;

    use super::*;

    // ------------------------------------------------------------------
    // Harness: every test gets its own directory under the system temp dir,
    // removed on drop. Tests run in parallel in one process, so names must
    // not collide.
    // ------------------------------------------------------------------

    #[cfg(feature = "std")]
    #[derive(Debug)]
    struct CaseDir(String);

    #[cfg(feature = "std")]
    impl CaseDir {
        fn new(tag: &str) -> Self {
            use core::sync::atomic::{AtomicUsize, Ordering};
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let name = format!("azul_autotest_file_{}_{}_{}", std::process::id(), tag, n);
            let dir = path_join(temp_dir().as_str(), &name).as_str().to_string();
            let _ = dir_delete_all(&dir);
            assert!(dir_create_all(&dir).is_ok(), "could not create case dir {dir}");
            Self(dir)
        }

        fn path(&self) -> &str {
            &self.0
        }

        fn child(&self, name: &str) -> String {
            path_join(&self.0, name).as_str().to_string()
        }
    }

    #[cfg(feature = "std")]
    impl Drop for CaseDir {
        fn drop(&mut self) {
            let _ = dir_delete_all(&self.0);
        }
    }

    /// Paths that must never panic and never resolve to anything on disk.
    ///
    /// Deliberately excludes `"../".repeat(n)` and `"/".repeat(n)`: on unix both
    /// resolve to the root directory, so they *do* exist. See
    /// `traversal_above_root_and_repeated_slashes_resolve_to_root`.
    #[cfg(feature = "std")]
    fn hostile_paths() -> Vec<String> {
        vec![
            String::new(),
            "   ".to_string(),
            "\t\n".to_string(),
            "\0".to_string(),
            "a\0b".to_string(),
            "valid;garbage".to_string(),
            "0".to_string(),
            "-0".to_string(),
            i64::MAX.to_string(),
            i64::MIN.to_string(),
            "NaN".to_string(),
            "inf".to_string(),
            "\u{1F600}".to_string(),
            "e\u{301}\u{327}".to_string(),
            "x".repeat(100_000),
            "[".repeat(10_000),
        ]
    }

    #[cfg(feature = "std")]
    fn as_opt_string(s: Option<AzString>) -> Option<String> {
        s.map(|x| x.as_str().to_string())
    }

    // ==================================================================
    // path_to_azstring (private)
    // ==================================================================

    #[test]
    #[cfg(feature = "std")]
    fn path_to_azstring_preserves_utf8_and_empty() {
        assert_eq!(path_to_azstring(Path::new("")).as_str(), "");
        assert_eq!(
            path_to_azstring(Path::new("/tmp/a b/\u{1F600}.txt")).as_str(),
            "/tmp/a b/\u{1F600}.txt"
        );
    }

    #[test]
    #[cfg(all(feature = "std", unix))]
    fn path_to_azstring_is_lossy_never_panics_on_invalid_utf8() {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

        let os = OsStr::from_bytes(&[b'/', b't', 0xFF, 0xFE, b'p']);
        let s = path_to_azstring(Path::new(os));
        assert!(s.as_str().contains('\u{FFFD}'));
        assert!(s.as_str().starts_with("/t"));
        assert!(s.as_str().ends_with('p'));
    }

    // ==================================================================
    // FileError: constructor / io mapping / Display
    // ==================================================================

    #[test]
    fn file_error_new_keeps_fields_verbatim() {
        let e = FileError::new(FileErrorKind::NotFound, "boom");
        assert_eq!(e.kind, FileErrorKind::NotFound);
        assert_eq!(e.message.as_str(), "boom");

        let empty = FileError::new(FileErrorKind::Other, "");
        assert_eq!(empty.message.as_str(), "");
        assert_eq!(empty.to_string(), "");
    }

    #[test]
    fn file_error_new_survives_huge_and_unicode_messages() {
        let huge = "x".repeat(1_000_000);
        let e = FileError::new(FileErrorKind::IoError, huge.clone());
        assert_eq!(e.message.as_str().len(), 1_000_000);
        assert_eq!(e.message.as_str(), huge);

        let uni = FileError::new(FileErrorKind::InvalidPath, "\u{1F600} e\u{301} \u{202E}");
        assert_eq!(uni.message.as_str(), "\u{1F600} e\u{301} \u{202E}");
        assert_eq!(uni.to_string(), "\u{1F600} e\u{301} \u{202E}");
    }

    #[test]
    fn file_error_display_does_not_interpret_format_specifiers() {
        // The message is user/OS-supplied; it must be printed, not treated as a
        // format string.
        let e = FileError::new(FileErrorKind::Other, "{} {0} {{}} %s %n");
        assert_eq!(e.to_string(), "{} {0} {{}} %s %n");
        assert!(format!("{e:?}").contains("Other"));
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_error_from_io_error_maps_every_known_kind() {
        use std::io::{Error, ErrorKind};

        let cases = [
            (ErrorKind::NotFound, FileErrorKind::NotFound),
            (ErrorKind::PermissionDenied, FileErrorKind::PermissionDenied),
            (ErrorKind::AlreadyExists, FileErrorKind::AlreadyExists),
            (ErrorKind::IsADirectory, FileErrorKind::IsDirectory),
            (ErrorKind::DirectoryNotEmpty, FileErrorKind::DirectoryNotEmpty),
            // everything else collapses to IoError
            (ErrorKind::InvalidData, FileErrorKind::IoError),
            (ErrorKind::InvalidInput, FileErrorKind::IoError),
            (ErrorKind::UnexpectedEof, FileErrorKind::IoError),
            (ErrorKind::WriteZero, FileErrorKind::IoError),
            (ErrorKind::Other, FileErrorKind::IoError),
        ];

        for (io_kind, expected) in cases {
            let e = FileError::from_io_error(Error::new(io_kind, "msg"));
            assert_eq!(e.kind, expected, "unexpected mapping for {io_kind:?}");
            assert_eq!(e.message.as_str(), "msg");
        }
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_error_from_io_error_tolerates_empty_and_raw_os_errors() {
        use std::io::{Error, ErrorKind};

        let empty = FileError::from_io_error(Error::new(ErrorKind::Other, ""));
        assert_eq!(empty.kind, FileErrorKind::IoError);
        assert_eq!(empty.message.as_str(), "");

        // ENOENT is 2 on every unix; the OS-error path must map like the
        // ErrorKind path.
        #[cfg(unix)]
        {
            let raw = FileError::from_io_error(Error::from_raw_os_error(2));
            assert_eq!(raw.kind, FileErrorKind::NotFound);
            assert!(!raw.message.as_str().is_empty());
        }

        // A nonsense errno must not panic and must not be silently classified.
        let bogus = FileError::from_io_error(Error::from_raw_os_error(i32::MAX));
        assert_eq!(bogus.kind, FileErrorKind::IoError);
    }

    // ==================================================================
    // file_read / file_write: round-trips and hostile paths
    // ==================================================================

    #[test]
    #[cfg(feature = "std")]
    fn file_write_read_roundtrip_bytes_including_invalid_utf8() {
        let case = CaseDir::new("rt_bytes");
        let p = case.child("blob.bin");

        let data: Vec<u8> = vec![0x00, 0xFF, 0xFE, 0x80, 0x7F, b'\n', 0x00];
        assert!(file_write(&p, &data).is_ok());

        let read = file_read(&p).expect("read back");
        assert_eq!(read.as_slice(), &data[..]);

        // The same bytes are not valid UTF-8: the string reader must reject
        // them instead of lossily decoding.
        let err = file_read_string(&p).expect_err("invalid utf-8 must not decode");
        assert_eq!(err.kind, FileErrorKind::IoError);
        assert!(!err.message.as_str().is_empty());
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_write_read_roundtrip_empty_file() {
        let case = CaseDir::new("rt_empty");
        let p = case.child("empty.bin");

        assert!(file_write(&p, b"").is_ok());
        assert!(file_read(&p).expect("read").is_empty());
        assert_eq!(file_read_string(&p).expect("read str").as_str(), "");
        assert_eq!(file_metadata(&p).expect("meta").size, 0);
        assert!(path_exists(&p));
        assert!(path_is_file(&p));
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_write_read_roundtrip_unicode_string() {
        let case = CaseDir::new("rt_unicode");
        let p = case.child("\u{1F600}_e\u{301}.txt");

        let content = "\u{1F600}\u{200D}\u{1F5A5}\u{FE0F} e\u{301}\u{327} \u{202E}rtl \u{0}nul";
        assert!(file_write_string(&p, content).is_ok());
        assert_eq!(file_read_string(&p).expect("read").as_str(), content);
        assert_eq!(file_read(&p).expect("read bytes").as_slice(), content.as_bytes());
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_write_read_roundtrip_one_megabyte() {
        let case = CaseDir::new("rt_big");
        let p = case.child("big.bin");

        let data: Vec<u8> = (0..1_000_000u32).map(|i| (i % 251) as u8).collect();
        assert!(file_write(&p, &data).is_ok());

        let read = file_read(&p).expect("read back");
        assert_eq!(read.len(), 1_000_000);
        assert_eq!(read.as_slice(), &data[..]);
        assert_eq!(file_metadata(&p).expect("meta").size, 1_000_000);
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_read_rejects_every_hostile_path_without_panicking() {
        // Read-only probes only: these are relative paths resolved against the
        // test CWD, so a destructive op here could touch a real file.
        for p in hostile_paths() {
            let r = file_read(&p);
            assert!(r.is_err(), "file_read unexpectedly succeeded for {p:?}");
            let e = r.unwrap_err();
            assert!(
                !e.message.as_str().is_empty(),
                "empty error message for {p:?}"
            );

            assert!(file_read_string(&p).is_err());
            assert!(file_metadata(&p).is_err());
            assert!(path_canonicalize(&p).is_err());
            assert!(dir_list(&p).is_err());
        }
    }

    #[test]
    #[cfg(all(feature = "std", unix))]
    fn traversal_above_root_and_repeated_slashes_resolve_to_root() {
        // Two path shapes that look like "obviously invalid garbage" but are in
        // fact valid names for `/`: `..` at the root is the root, and runs of
        // separators collapse. Anything treating these as rejected input is
        // wrong.
        let climb = "../".repeat(64);
        let slashes = "/".repeat(1024);

        for p in [climb.as_str(), slashes.as_str()] {
            assert!(path_exists(p), "{p:?} resolves to /, so it exists");
            assert!(path_is_dir(p));
            assert!(!path_is_file(p));
            assert_eq!(
                path_canonicalize(p).expect("resolves to root").as_str(),
                "/"
            );
        }
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_read_interior_nul_path_is_an_io_error_not_a_crash() {
        let err = file_read("some\0path").expect_err("NUL byte cannot reach the OS");
        assert_eq!(err.kind, FileErrorKind::IoError);
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_read_overlong_path_saturates_to_an_error() {
        let case = CaseDir::new("longpath");
        // A single component far beyond NAME_MAX.
        let p = case.child(&"n".repeat(5_000));
        let err = file_read(&p).expect_err("component exceeds NAME_MAX");
        assert_eq!(err.kind, FileErrorKind::IoError);
        assert!(!path_exists(&p));
    }

    #[test]
    #[cfg(all(feature = "std", unix))]
    fn file_read_on_a_directory_reports_is_directory() {
        let case = CaseDir::new("read_dir_as_file");
        let err = file_read(case.path()).expect_err("a directory is not readable as bytes");
        assert_eq!(err.kind, FileErrorKind::IsDirectory);
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_write_into_missing_parent_reports_not_found() {
        let case = CaseDir::new("write_missing_parent");
        let p = path_join(&case.child("nope"), "f.txt").as_str().to_string();

        let err = file_write(&p, b"x").expect_err("parent does not exist");
        assert_eq!(err.kind, FileErrorKind::NotFound);
        assert!(file_write_string(&p, "x").is_err());
        assert!(!path_exists(&p));
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_paths_are_never_trimmed() {
        let case = CaseDir::new("no_trim");
        let padded = case.child("  spaced  ");

        assert!(file_write_string(&padded, "payload").is_ok());
        assert_eq!(file_read_string(&padded).expect("exact name").as_str(), "payload");

        // The trimmed name is a *different* file and must not resolve.
        let trimmed = case.child("spaced");
        assert!(file_read_string(&trimmed).is_err());
        assert!(!path_exists(&trimmed));
    }

    // ==================================================================
    // file_append / file_copy / file_rename / file_delete
    // ==================================================================

    #[test]
    #[cfg(feature = "std")]
    fn file_append_creates_then_concatenates() {
        let case = CaseDir::new("append");
        let p = case.child("log.bin");

        assert!(!path_exists(&p));
        assert!(file_append(&p, b"aa").is_ok(), "append must create the file");
        assert!(file_append(&p, b"").is_ok(), "empty append is a no-op, not an error");
        assert!(file_append(&p, &[0xFF, 0x00]).is_ok());

        assert_eq!(file_read(&p).expect("read").as_slice(), &[b'a', b'a', 0xFF, 0x00]);
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_copy_returns_byte_count_and_duplicates_content() {
        let case = CaseDir::new("copy");
        let src = case.child("src.bin");
        let dst = case.child("dst.bin");
        let data: Vec<u8> = (0..=255u8).cycle().take(4096).collect();

        assert!(file_write(&src, &data).is_ok());
        assert_eq!(file_copy(&src, &dst).expect("copy"), 4096);
        assert_eq!(file_read(&dst).expect("read dst").as_slice(), &data[..]);
        assert!(path_exists(&src), "copy must not consume the source");

        // Copy overwrites an existing destination rather than failing.
        assert_eq!(file_copy(&src, &dst).expect("re-copy"), 4096);

        let missing = case.child("ghost.bin");
        assert_eq!(
            file_copy(&missing, &dst).expect_err("missing source").kind,
            FileErrorKind::NotFound
        );
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_rename_moves_and_missing_source_is_not_found() {
        let case = CaseDir::new("rename");
        let src = case.child("a.txt");
        let dst = case.child("b.txt");

        assert!(file_write_string(&src, "payload").is_ok());
        assert!(file_rename(&src, &dst).is_ok());
        assert!(!path_exists(&src), "source must be gone after rename");
        assert_eq!(file_read_string(&dst).expect("read").as_str(), "payload");

        assert_eq!(
            file_rename(&src, &dst).expect_err("source gone").kind,
            FileErrorKind::NotFound
        );
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_delete_is_not_idempotent_and_refuses_directories() {
        let case = CaseDir::new("delete");
        let p = case.child("victim.txt");

        assert!(file_write_string(&p, "x").is_ok());
        assert!(file_delete(&p).is_ok());
        assert!(!path_exists(&p));

        assert_eq!(
            file_delete(&p).expect_err("already deleted").kind,
            FileErrorKind::NotFound
        );

        // unlink() on a directory is EISDIR on Linux but EPERM on macOS, so only
        // assert that it fails rather than pinning the kind.
        assert!(
            file_delete(case.path()).is_err(),
            "file_delete must not remove a directory"
        );
        assert!(path_is_dir(case.path()));
    }

    // ==================================================================
    // path_exists / path_is_file / path_is_dir
    // ==================================================================

    #[test]
    #[cfg(feature = "std")]
    fn path_predicates_are_false_for_hostile_paths() {
        for p in hostile_paths() {
            assert!(!path_exists(&p), "{p:?} should not exist");
            assert!(!path_is_file(&p));
            assert!(!path_is_dir(&p));
        }
    }

    #[test]
    #[cfg(feature = "std")]
    fn path_is_file_and_is_dir_are_mutually_exclusive() {
        let case = CaseDir::new("predicates");
        let f = case.child("f.txt");
        assert!(file_write_string(&f, "x").is_ok());

        let candidates = [case.path().to_string(), f.clone(), case.child("ghost")];
        for p in &candidates {
            assert!(
                !(path_is_file(p) && path_is_dir(p)),
                "{p:?} claimed to be both a file and a directory"
            );
            assert_eq!(path_exists(p), path_is_file(p) || path_is_dir(p));
        }

        assert!(path_is_file(&f) && !path_is_dir(&f));
        assert!(path_is_dir(case.path()) && !path_is_file(case.path()));
    }

    // ==================================================================
    // file_metadata
    // ==================================================================

    #[test]
    #[cfg(feature = "std")]
    fn file_metadata_reports_size_type_and_mtime() {
        let case = CaseDir::new("meta");
        let f = case.child("f.bin");
        assert!(file_write(&f, &[1u8; 1234]).is_ok());

        let m = file_metadata(&f).expect("file metadata");
        assert_eq!(m.size, 1234);
        assert_eq!(m.file_type, FileType::File);
        assert!(!m.is_readonly);
        assert!(m.modified_secs > 0, "mtime must be a real unix timestamp");

        let d = file_metadata(case.path()).expect("dir metadata");
        assert_eq!(d.file_type, FileType::Directory);

        assert_eq!(
            file_metadata(&case.child("ghost")).expect_err("missing").kind,
            FileErrorKind::NotFound
        );
    }

    #[test]
    #[cfg(all(feature = "std", unix))]
    fn file_metadata_does_not_follow_symlinks_but_path_is_file_does() {
        let case = CaseDir::new("symlink_meta");
        let target = case.child("target.txt");
        let link = case.child("link.txt");
        assert!(file_write_string(&target, "hello").is_ok());
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        // symlink_metadata semantics: the link itself, not its target.
        let m = file_metadata(&link).expect("metadata");
        assert_eq!(m.file_type, FileType::Symlink);

        // ...while the predicates and readers *do* follow it.
        assert!(path_is_file(&link));
        assert_eq!(file_read_string(&link).expect("read through link").as_str(), "hello");
        assert_eq!(
            path_canonicalize(&link).expect("canonicalize").as_str(),
            path_canonicalize(&target).expect("canonicalize target").as_str()
        );
    }

    #[test]
    #[cfg(all(feature = "std", unix))]
    fn file_metadata_detects_dangling_symlink_without_error() {
        let case = CaseDir::new("dangling");
        let link = case.child("dangling.txt");
        std::os::unix::fs::symlink(case.child("nowhere"), &link).expect("symlink");

        let m = file_metadata(&link).expect("symlink_metadata must not follow");
        assert_eq!(m.file_type, FileType::Symlink);

        // Everything that follows the link must fail cleanly.
        assert!(!path_exists(&link));
        assert_eq!(
            file_read(&link).expect_err("dangling target").kind,
            FileErrorKind::NotFound
        );
    }

    // ==================================================================
    // Directory operations
    // ==================================================================

    #[test]
    #[cfg(feature = "std")]
    fn dir_create_rejects_existing_and_missing_parent() {
        let case = CaseDir::new("dir_create");

        assert_eq!(
            dir_create(case.path()).expect_err("already exists").kind,
            FileErrorKind::AlreadyExists
        );

        let orphan = path_join(&case.child("missing"), "child").as_str().to_string();
        assert_eq!(
            dir_create(&orphan).expect_err("missing parent").kind,
            FileErrorKind::NotFound
        );
        assert!(!path_exists(&orphan));

        // create_all fills in the parents and is idempotent.
        assert!(dir_create_all(&orphan).is_ok());
        assert!(dir_create_all(&orphan).is_ok(), "create_all must be idempotent");
        assert!(path_is_dir(&orphan));
    }

    #[test]
    #[cfg(feature = "std")]
    fn dir_create_all_handles_deep_nesting_without_stack_overflow() {
        let case = CaseDir::new("deep");
        let mut deep = case.child("d0");
        for i in 1..64 {
            deep = path_join(&deep, &format!("d{i}")).as_str().to_string();
        }

        assert!(dir_create_all(&deep).is_ok());
        assert!(path_is_dir(&deep));

        // Recursive removal of the same chain must also stay iterative.
        assert!(dir_delete_all(&case.child("d0")).is_ok());
        assert!(!path_exists(&deep));
    }

    #[test]
    #[cfg(feature = "std")]
    fn dir_delete_refuses_non_empty_but_delete_all_succeeds() {
        let case = CaseDir::new("dir_delete");
        let sub = case.child("sub");
        assert!(dir_create(&sub).is_ok());
        assert!(file_write_string(&path_join(&sub, "f.txt").as_str().to_string(), "x").is_ok());

        let err = dir_delete(&sub).expect_err("non-empty directory");
        #[cfg(unix)]
        assert_eq!(err.kind, FileErrorKind::DirectoryNotEmpty);
        assert!(!err.message.as_str().is_empty());
        assert!(path_is_dir(&sub), "failed delete must not have removed anything");

        assert!(dir_delete_all(&sub).is_ok());
        assert!(!path_exists(&sub));

        assert_eq!(
            dir_delete(&sub).expect_err("already gone").kind,
            FileErrorKind::NotFound
        );
    }

    #[test]
    #[cfg(feature = "std")]
    fn dir_list_enumerates_entries_with_matching_names_and_types() {
        let case = CaseDir::new("dir_list");
        let sub = case.child("subdir");
        let f = case.child("\u{1F600}.txt");
        assert!(dir_create(&sub).is_ok());
        assert!(file_write_string(&f, "x").is_ok());

        let entries = dir_list(case.path()).expect("list");
        assert_eq!(entries.len(), 2);

        for e in entries.iter() {
            assert!(!e.name.as_str().is_empty());
            assert!(
                e.path.as_str().ends_with(e.name.as_str()),
                "entry path {:?} must end with its name {:?}",
                e.path.as_str(),
                e.name.as_str()
            );
            assert!(path_exists(e.path.as_str()));

            match e.name.as_str() {
                "subdir" => assert_eq!(e.file_type, FileType::Directory),
                "\u{1F600}.txt" => assert_eq!(e.file_type, FileType::File),
                other => panic!("unexpected entry {other:?}"),
            }
        }

        assert!(dir_list(&sub).expect("empty dir").is_empty());
        assert!(dir_list(&f).is_err(), "listing a file must fail");
    }

    #[test]
    #[cfg(all(feature = "std", unix))]
    fn dir_list_reports_symlinks_as_symlinks() {
        let case = CaseDir::new("dir_list_symlink");
        let target = case.child("target.txt");
        assert!(file_write_string(&target, "x").is_ok());
        std::os::unix::fs::symlink(&target, case.child("link.txt")).expect("symlink");

        let entries = dir_list(case.path()).expect("list");
        let link = entries
            .iter()
            .find(|e| e.name.as_str() == "link.txt")
            .expect("link entry");
        assert_eq!(link.file_type, FileType::Symlink);
    }

    // ==================================================================
    // Path manipulation (pure, no filesystem)
    // ==================================================================

    #[test]
    #[cfg(all(feature = "std", unix))]
    fn path_join_absolute_component_replaces_the_base() {
        // std::path::Path::join semantics: an absolute second component wins.
        // This is the classic traversal footgun, so pin it down.
        assert_eq!(path_join("/home/user", "/etc/passwd").as_str(), "/etc/passwd");
        assert_eq!(path_join("/srv/data", "/").as_str(), "/");
    }

    #[test]
    #[cfg(feature = "std")]
    fn path_join_does_not_normalize_parent_traversal() {
        let joined = path_join("/srv/data", "../../etc/passwd");
        assert!(
            joined.as_str().contains(".."),
            "path_join is purely lexical and must not silently resolve `..`: {:?}",
            joined.as_str()
        );

        assert_eq!(path_join("", "x").as_str(), "x");
        assert!(path_join("base", "").as_str().starts_with("base"));
        assert!(path_join("base", "\u{1F600}").as_str().ends_with("\u{1F600}"));
    }

    #[test]
    #[cfg(feature = "std")]
    fn path_parent_edge_cases() {
        assert_eq!(as_opt_string(path_parent("")), None);
        assert_eq!(as_opt_string(path_parent("..")), Some(String::new()));
        assert_eq!(as_opt_string(path_parent("a")), Some(String::new()));

        #[cfg(unix)]
        {
            assert_eq!(as_opt_string(path_parent("/")), None);
            assert_eq!(as_opt_string(path_parent("/a")), Some("/".to_string()));
            assert_eq!(as_opt_string(path_parent("/a/b/c")), Some("/a/b".to_string()));
            assert_eq!(as_opt_string(path_parent("/a/b/")), Some("/a".to_string()));
        }
    }

    #[test]
    #[cfg(all(feature = "std", unix))]
    fn path_parent_of_ten_thousand_components_does_not_stack_overflow() {
        let deep = format!("/{}", vec!["a"; 10_000].join("/"));
        let parent = path_parent(&deep).expect("parent of a deep path");
        assert_eq!(parent.as_str().len(), deep.len() - 2);
        assert!(parent.as_str().starts_with("/a/a/"));
    }

    #[test]
    #[cfg(feature = "std")]
    fn path_file_name_edge_cases() {
        assert_eq!(as_opt_string(path_file_name("")), None);
        assert_eq!(as_opt_string(path_file_name("..")), None);
        assert_eq!(as_opt_string(path_file_name(".")), None);
        assert_eq!(as_opt_string(path_file_name("foo.txt")), Some("foo.txt".to_string()));
        assert_eq!(
            as_opt_string(path_file_name("\u{1F600}.txt")),
            Some("\u{1F600}.txt".to_string())
        );

        #[cfg(unix)]
        {
            assert_eq!(as_opt_string(path_file_name("/")), None);
            // A trailing separator is stripped, not treated as an empty name.
            assert_eq!(as_opt_string(path_file_name("/a/b/")), Some("b".to_string()));
        }
    }

    #[test]
    #[cfg(feature = "std")]
    fn path_extension_edge_cases() {
        assert_eq!(as_opt_string(path_extension("")), None);
        assert_eq!(as_opt_string(path_extension("noext")), None);
        // A dotfile has no extension...
        assert_eq!(as_opt_string(path_extension(".hidden")), None);
        // ...but a trailing dot yields an *empty* extension, not None.
        assert_eq!(as_opt_string(path_extension("a.")), Some(String::new()));
        // Only the last segment counts.
        assert_eq!(as_opt_string(path_extension("a.tar.gz")), Some("gz".to_string()));
        assert_eq!(as_opt_string(path_extension("a.\u{1F600}")), Some("\u{1F600}".to_string()));
        assert_eq!(as_opt_string(path_extension("..")), None);
    }

    #[test]
    #[cfg(feature = "std")]
    fn path_canonicalize_requires_existence_and_returns_absolute() {
        let case = CaseDir::new("canon");
        let f = case.child("f.txt");
        assert!(file_write_string(&f, "x").is_ok());

        let canon = path_canonicalize(&f).expect("canonicalize");
        assert!(FilePath::from_str(canon.as_str()).is_absolute());
        assert!(path_exists(canon.as_str()));
        assert!(!canon.as_str().contains(".."));

        assert_eq!(
            path_canonicalize("").expect_err("empty path").kind,
            FileErrorKind::NotFound
        );
        assert_eq!(
            path_canonicalize(&case.child("ghost")).expect_err("missing").kind,
            FileErrorKind::NotFound
        );

        // `..` is resolved away — but only through components that really are
        // directories (`f.txt/../f.txt` is ENOTDIR, not a lexical rewrite).
        let sub = case.child("sub");
        assert!(dir_create(&sub).is_ok());
        let via_dotdot = path_join(&path_join(&sub, "..").as_str().to_string(), "f.txt")
            .as_str()
            .to_string();
        assert_eq!(
            path_canonicalize(&via_dotdot).expect("resolve .. through a real dir").as_str(),
            canon.as_str()
        );

        let through_a_file = path_join(&path_join(&f, "..").as_str().to_string(), "f.txt")
            .as_str()
            .to_string();
        assert!(
            path_canonicalize(&through_a_file).is_err(),
            "a regular file must not act as a directory component"
        );
    }

    #[test]
    #[cfg(feature = "std")]
    fn temp_dir_is_a_real_directory() {
        let t = temp_dir();
        assert!(!t.as_str().is_empty());
        assert!(path_is_dir(t.as_str()));
        assert!(FilePath::from_str(t.as_str()).is_absolute());
    }

    // ==================================================================
    // FilePath: construction, invariants, delegation
    // ==================================================================

    #[test]
    fn file_path_constructors_agree_and_roundtrip_exactly() {
        let inputs = [
            "",
            "   ",
            "/usr/bin",
            "rel/path.txt",
            "\u{1F600}/e\u{301}",
            "with\0nul",
        ];

        for s in inputs {
            let from_str = FilePath::from_str(s);
            let new = FilePath::new(AzString::from(String::from(s)));
            let from_ref: FilePath = s.into();
            let from_string: FilePath = String::from(s).into();
            let from_az: FilePath = AzString::from(String::from(s)).into();

            assert_eq!(from_str.as_str(), s, "as_str must round-trip verbatim");
            assert_eq!(from_str, new);
            assert_eq!(from_str, from_ref);
            assert_eq!(from_str, from_string);
            assert_eq!(from_str, from_az);
            assert_eq!(from_str.as_string().as_str(), s);
        }
    }

    #[test]
    fn file_path_empty_and_default_are_the_neutral_value() {
        let e = FilePath::empty();
        assert_eq!(e, FilePath::default());
        assert_eq!(e, FilePath::from_str(""));
        assert!(e.as_str().is_empty());
        assert!(e.as_string().as_str().is_empty());
    }

    #[test]
    fn file_path_handles_a_100k_char_component() {
        let huge = "x".repeat(100_000);
        let p = FilePath::from_str(&huge);
        assert_eq!(p.as_str().len(), 100_000);
        assert_eq!(p.as_str(), huge);
        assert_eq!(p.clone(), p);
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_path_eq_implies_equal_hashes() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        assert!(set.insert(FilePath::from_str("/a/b")));
        assert!(
            !set.insert(FilePath::new(AzString::from(String::from("/a/b")))),
            "equal FilePaths must hash equally"
        );
        assert!(set.insert(FilePath::from_str("/a/b/")), "trailing slash is a distinct string");
        assert_eq!(set.len(), 2);
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_path_accessors_delegate_to_the_free_functions() {
        let inputs = [
            "",
            "   ",
            "/",
            "..",
            "/a/b/c.tar.gz",
            "a.",
            ".hidden",
            "\u{1F600}.txt",
            "with\0nul",
        ];

        for s in inputs {
            let p = FilePath::from_str(s);
            assert_eq!(
                p.parent().map(|q| q.as_str().to_string()),
                as_opt_string(path_parent(s)),
                "parent() diverged from path_parent for {s:?}"
            );
            assert_eq!(as_opt_string(p.file_name()), as_opt_string(path_file_name(s)));
            assert_eq!(as_opt_string(p.extension()), as_opt_string(path_extension(s)));
            assert_eq!(p.exists(), path_exists(s));
            assert_eq!(p.is_file(), path_is_file(s));
            assert_eq!(p.is_dir(), path_is_dir(s));

            let joined = p.join(&FilePath::from_str("child"));
            assert_eq!(joined.as_str(), path_join(s, "child").as_str());
            let joined_str = p.join_str(&AzString::from(String::from("child")));
            assert_eq!(joined_str.as_str(), joined.as_str());
        }
    }

    #[test]
    #[cfg(all(feature = "std", unix))]
    fn file_path_is_absolute_edge_cases() {
        assert!(FilePath::from_str("/").is_absolute());
        assert!(FilePath::from_str("/etc/passwd").is_absolute());
        assert!(!FilePath::from_str("").is_absolute());
        assert!(!FilePath::from_str("   ").is_absolute());
        assert!(!FilePath::from_str("relative").is_absolute());
        assert!(!FilePath::from_str("./relative").is_absolute());
        assert!(!FilePath::from_str("../escape").is_absolute());
    }

    #[test]
    #[cfg(feature = "std")]
    fn empty_file_path_fails_every_filesystem_operation_cleanly() {
        let e = FilePath::empty();

        assert!(!e.exists());
        assert!(!e.is_file());
        assert!(!e.is_dir());
        assert!(e.read_bytes().is_err());
        assert!(e.read_string().is_err());
        assert!(e.read_dir().is_err());
        assert!(e.metadata().is_err());
        assert!(e.canonicalize().is_err());
        assert!(e.remove_file().is_err());
        assert!(e.remove_dir().is_err());
        assert!(e.remove_dir_all().is_err());
        assert!(e.create_dir().is_err());
        assert!(e.write_bytes(&U8Vec::from_vec(vec![1, 2, 3])).is_err());
        assert!(e.write_string(&AzString::from(String::from("x"))).is_err());
        assert!(e.copy_to(&FilePath::from_str("/tmp/whatever")).is_err());
        assert!(e.rename_to(&FilePath::from_str("/tmp/whatever")).is_err());

        // Pure path ops still work on the empty path.
        assert!(e.parent().is_none());
        assert!(e.file_name().is_none());
        assert!(e.extension().is_none());
    }

    #[test]
    #[cfg(feature = "std")]
    fn create_dir_all_on_the_empty_path_reports_success_but_creates_nothing() {
        // Inherited from std: `fs::create_dir_all` short-circuits `""` to
        // `Ok(())`. So this is the one directory op where the empty path does
        // NOT fail — callers cannot treat Ok as "the directory now exists".
        let e = FilePath::empty();
        assert!(e.create_dir_all().is_ok());
        assert!(dir_create_all("").is_ok());
        assert!(!e.exists(), "Ok(()) must not be read as `the dir was created`");

        // The non-recursive variant does fail, so the two disagree on `""`.
        assert!(dir_create("").is_err());
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_path_oop_roundtrip_write_read_copy_rename_remove() {
        let case = CaseDir::new("oop");
        let root = FilePath::from_str(case.path());

        let nested = root.join(&FilePath::from_str("a")).join(&FilePath::from_str("b"));
        assert!(nested.create_dir_all().is_ok());
        assert!(nested.is_dir());

        let f = nested.join_str(&AzString::from(String::from("data.bin")));
        let payload = U8Vec::from_vec(vec![0u8, 0xFF, 0x41]);
        assert!(f.write_bytes(&payload).is_ok());
        assert_eq!(f.read_bytes().expect("read").as_slice(), payload.as_slice());
        assert_eq!(f.metadata().expect("meta").size, 3);
        assert_eq!(f.file_name().expect("name").as_str(), "data.bin");
        assert_eq!(f.extension().expect("ext").as_str(), "bin");
        assert_eq!(f.parent().expect("parent").as_str(), nested.as_str());

        let text = f.join(&FilePath::empty());
        assert!(text.as_str().starts_with(f.as_str()));

        let copy = nested.join(&FilePath::from_str("copy.bin"));
        assert_eq!(f.copy_to(&copy).expect("copy"), 3);
        assert!(copy.is_file());

        let moved = nested.join(&FilePath::from_str("moved.bin"));
        assert!(copy.rename_to(&moved).is_ok());
        assert!(!copy.exists());
        assert!(moved.is_file());

        assert_eq!(nested.read_dir().expect("list").len(), 2);

        assert!(moved.remove_file().is_ok());
        assert!(f.remove_file().is_ok());
        assert!(nested.remove_dir().is_ok(), "now-empty dir must be removable");
        assert!(root.join(&FilePath::from_str("a")).remove_dir_all().is_ok());
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_path_write_string_overwrites_rather_than_appends() {
        let case = CaseDir::new("overwrite");
        let f = FilePath::from_str(&case.child("f.txt"));

        assert!(f.write_string(&AzString::from(String::from("first"))).is_ok());
        assert!(f.write_string(&AzString::from(String::from("2"))).is_ok());
        assert_eq!(f.read_string().expect("read").as_str(), "2");
        assert_eq!(f.metadata().expect("meta").size, 1);
    }

    #[test]
    #[cfg(feature = "std")]
    fn file_path_env_dirs_are_sane() {
        let t = FilePath::get_temp_dir();
        assert_eq!(t.as_str(), temp_dir().as_str());
        assert!(t.is_dir());

        let cwd = FilePath::get_current_dir().expect("current dir");
        assert!(cwd.is_absolute());
        assert!(cwd.is_dir());
    }

    #[test]
    #[cfg(all(feature = "std", feature = "extra"))]
    fn file_path_user_dirs_never_panic_and_are_never_empty_strings() {
        let probes: [(&str, Option<FilePath>); 17] = [
            ("home", FilePath::get_home_dir()),
            ("cache", FilePath::get_cache_dir()),
            ("config", FilePath::get_config_dir()),
            ("config_local", FilePath::get_config_local_dir()),
            ("data", FilePath::get_data_dir()),
            ("data_local", FilePath::get_data_local_dir()),
            ("desktop", FilePath::get_desktop_dir()),
            ("document", FilePath::get_document_dir()),
            ("download", FilePath::get_download_dir()),
            ("executable", FilePath::get_executable_dir()),
            ("font", FilePath::get_font_dir()),
            ("picture", FilePath::get_picture_dir()),
            ("preference", FilePath::get_preference_dir()),
            ("public", FilePath::get_public_dir()),
            ("runtime", FilePath::get_runtime_dir()),
            ("state", FilePath::get_state_dir()),
            ("audio", FilePath::get_audio_dir()),
        ];

        for (name, dir) in probes {
            if let Some(d) = dir {
                assert!(!d.as_str().is_empty(), "{name} dir resolved to an empty path");
                assert!(d.is_absolute(), "{name} dir must be absolute: {:?}", d.as_str());
            }
        }

        // The remaining two go through the same `dirs` path; just prove they
        // do not panic.
        let _ = FilePath::get_video_dir();
        let _ = FilePath::get_template_dir();
    }

    // ==================================================================
    // Plain data types
    // ==================================================================

    #[test]
    fn file_metadata_holds_saturated_u64_fields() {
        let m = FileMetadata {
            size: u64::MAX,
            file_type: FileType::Other,
            is_readonly: true,
            modified_secs: u64::MAX,
            created_secs: 0,
        };
        let copy = m;

        assert_eq!(m, copy);
        assert_eq!(copy.size, u64::MAX);
        assert_eq!(copy.modified_secs, u64::MAX);
        assert!(format!("{m:?}").contains("18446744073709551615"));
    }

    #[test]
    fn dir_entry_vec_bounds_are_checked() {
        let empty = DirEntryVec::from_vec(Vec::new());
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert!(empty.get(0).is_none());
        assert!(empty.get(usize::MAX).is_none());

        let one = DirEntryVec::from_vec(vec![DirEntry {
            name: AzString::from(String::from("f.txt")),
            path: AzString::from(String::from("/tmp/f.txt")),
            file_type: FileType::File,
        }]);
        assert_eq!(one.len(), 1);
        assert_eq!(one.get(0).expect("first").name.as_str(), "f.txt");
        assert!(one.get(1).is_none());
        assert!(one.get(usize::MAX).is_none());
    }

    // ==================================================================
    // no_std stubs: every entry point must fail closed, never panic.
    // ==================================================================

    #[test]
    #[cfg(not(feature = "std"))]
    fn no_std_stubs_fail_closed() {
        for p in ["", "   ", "/etc/passwd", "\u{1F600}"] {
            assert_eq!(file_read(p).unwrap_err().kind, FileErrorKind::Other);
            assert!(file_read_string(p).is_err());
            assert!(file_write(p, &[0xFF, 0x00]).is_err());
            assert!(file_write_string(p, "x").is_err());
            assert!(file_append(p, b"").is_err());
            assert!(file_copy(p, p).is_err());
            assert!(file_rename(p, p).is_err());
            assert!(file_delete(p).is_err());
            assert!(file_metadata(p).is_err());
            assert!(dir_create(p).is_err());
            assert!(dir_create_all(p).is_err());
            assert!(dir_delete(p).is_err());
            assert!(dir_delete_all(p).is_err());
            assert!(dir_list(p).is_err());
            assert!(path_canonicalize(p).is_err());

            assert!(!path_exists(p));
            assert!(!path_is_file(p));
            assert!(!path_is_dir(p));
            assert!(path_parent(p).is_none());
            assert!(path_file_name(p).is_none());
            assert!(path_extension(p).is_none());
            assert_eq!(path_join(p, p).as_str(), "");
        }

        assert_eq!(temp_dir().as_str(), "");
        assert_eq!(no_std_file_error().kind, FileErrorKind::Other);
        assert!(!no_std_file_error().message.as_str().is_empty());
    }
}
