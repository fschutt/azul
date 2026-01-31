//! File system operations module for C API
//!
//! Provides C-compatible wrappers around Rust's std::fs API.
//! This allows C code to use Rust's file operations without importing stdio.h.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use azul_css::{AzString, U8Vec, Void, impl_result, impl_result_inner, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_partialeq, impl_vec_mut, impl_option, impl_option_inner};

#[cfg(feature = "std")]
use std::path::Path;

// ============================================================================
// Error types
// ============================================================================

/// Error when performing file operations
#[derive(Debug, Clone, PartialEq)]
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
    pub fn from_io_error(e: std::io::Error) -> Self {
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
    Void,
    FileError,
    ResultVoidFileError,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl_result!(
    U8Vec,
    FileError,
    ResultU8VecFileError,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl_result!(
    AzString,
    FileError,
    ResultStringFileError,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl_result!(
    u64,
    FileError,
    Resultu64FileError,
    copy = false,
    [Debug, Clone, PartialEq]
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
#[derive(Debug, Clone, PartialEq)]
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
    [Debug, Clone, PartialEq]
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
/// 
/// # Arguments
/// * `path` - Path to the file
/// 
/// # Returns
/// * `Result<U8Vec, FileError>` - File contents or error
#[cfg(feature = "std")]
pub fn file_read(path: &str) -> Result<U8Vec, FileError> {
    let data = std::fs::read(path)
        .map_err(FileError::from_io_error)?;
    Ok(U8Vec::from(data))
}

/// Read a file to string (UTF-8)
/// 
/// # Arguments
/// * `path` - Path to the file
/// 
/// # Returns
/// * `Result<AzString, FileError>` - File contents or error
#[cfg(feature = "std")]
pub fn file_read_string(path: &str) -> Result<AzString, FileError> {
    let data = std::fs::read_to_string(path)
        .map_err(FileError::from_io_error)?;
    Ok(AzString::from(data))
}

/// Write bytes to a file (creates or overwrites)
/// 
/// # Arguments
/// * `path` - Path to the file
/// * `data` - Data to write
/// 
/// # Returns
/// * `Result<Void, FileError>` - Success or error
#[cfg(feature = "std")]
pub fn file_write(path: &str, data: &[u8]) -> Result<Void, FileError> {
    std::fs::write(path, data)
        .map(|_| Void::default()).map_err(FileError::from_io_error)
}

/// Write string to a file (creates or overwrites)
/// 
/// # Arguments
/// * `path` - Path to the file
/// * `data` - String to write
/// 
/// # Returns
/// * `Result<Void, FileError>` - Success or error
#[cfg(feature = "std")]
pub fn file_write_string(path: &str, data: &str) -> Result<Void, FileError> {
    std::fs::write(path, data.as_bytes())
        .map(|_| Void::default()).map_err(FileError::from_io_error)
}

/// Append bytes to a file
/// 
/// # Arguments
/// * `path` - Path to the file
/// * `data` - Data to append
/// 
/// # Returns
/// * `Result<Void, FileError>` - Success or error
#[cfg(feature = "std")]
pub fn file_append(path: &str, data: &[u8]) -> Result<Void, FileError> {
    use std::fs::OpenOptions;
    use std::io::Write;
    
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(FileError::from_io_error)?;
    
    file.write_all(data)
        .map(|_| Void::default())
        .map_err(FileError::from_io_error)
}

/// Copy a file
/// 
/// # Arguments
/// * `from` - Source path
/// * `to` - Destination path
/// 
/// # Returns
/// * `Result<u64, FileError>` - Bytes copied or error
#[cfg(feature = "std")]
pub fn file_copy(from: &str, to: &str) -> Result<u64, FileError> {
    std::fs::copy(from, to)
        .map_err(FileError::from_io_error)
}

/// Rename/move a file
/// 
/// # Arguments
/// * `from` - Source path
/// * `to` - Destination path
/// 
/// # Returns
/// * `Result<Void, FileError>` - Success or error
#[cfg(feature = "std")]
pub fn file_rename(from: &str, to: &str) -> Result<Void, FileError> {
    std::fs::rename(from, to)
        .map(|_| Void::default()).map_err(FileError::from_io_error)
}

/// Delete a file
/// 
/// # Arguments
/// * `path` - Path to the file
/// 
/// # Returns
/// * `Result<Void, FileError>` - Success or error
#[cfg(feature = "std")]
pub fn file_delete(path: &str) -> Result<Void, FileError> {
    std::fs::remove_file(path)
        .map(|_| Void::default()).map_err(FileError::from_io_error)
}

/// Check if a file or directory exists
/// 
/// # Arguments
/// * `path` - Path to check
/// 
/// # Returns
/// * `bool` - True if exists
#[cfg(feature = "std")]
pub fn path_exists(path: &str) -> bool {
    Path::new(path).exists()
}

/// Check if path is a file
/// 
/// # Arguments
/// * `path` - Path to check
/// 
/// # Returns
/// * `bool` - True if is a file
#[cfg(feature = "std")]
pub fn path_is_file(path: &str) -> bool {
    Path::new(path).is_file()
}

/// Check if path is a directory
/// 
/// # Arguments
/// * `path` - Path to check
/// 
/// # Returns
/// * `bool` - True if is a directory
#[cfg(feature = "std")]
pub fn path_is_dir(path: &str) -> bool {
    Path::new(path).is_dir()
}

/// Get file metadata
/// 
/// # Arguments
/// * `path` - Path to the file
/// 
/// # Returns
/// * `Result<FileMetadata, FileError>` - Metadata or error
#[cfg(feature = "std")]
pub fn file_metadata(path: &str) -> Result<FileMetadata, FileError> {
    let meta = std::fs::metadata(path)
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
        .map(|d| d.as_secs())
        .unwrap_or(0);
    
    let created_secs = meta.created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    
    Ok(FileMetadata {
        size: meta.len(),
        file_type,
        is_readonly: meta.permissions().readonly(),
        modified_secs,
        created_secs,
    })
}

// ============================================================================
// Directory operations
// ============================================================================

/// Create a directory
/// 
/// # Arguments
/// * `path` - Path to create
/// 
/// # Returns
/// * `Result<Void, FileError>` - Success or error
#[cfg(feature = "std")]
pub fn dir_create(path: &str) -> Result<Void, FileError> {
    std::fs::create_dir(path)
        .map(|_| Void::default()).map_err(FileError::from_io_error)
}

/// Create a directory and all parent directories
/// 
/// # Arguments
/// * `path` - Path to create
/// 
/// # Returns
/// * `Result<Void, FileError>` - Success or error
#[cfg(feature = "std")]
pub fn dir_create_all(path: &str) -> Result<Void, FileError> {
    std::fs::create_dir_all(path)
        .map(|_| Void::default()).map_err(FileError::from_io_error)
}

/// Delete an empty directory
/// 
/// # Arguments
/// * `path` - Path to delete
/// 
/// # Returns
/// * `Result<Void, FileError>` - Success or error
#[cfg(feature = "std")]
pub fn dir_delete(path: &str) -> Result<Void, FileError> {
    std::fs::remove_dir(path)
        .map(|_| Void::default()).map_err(FileError::from_io_error)
}

/// Delete a directory and all its contents
/// 
/// # Arguments
/// * `path` - Path to delete
/// 
/// # Returns
/// * `Result<Void, FileError>` - Success or error
#[cfg(feature = "std")]
pub fn dir_delete_all(path: &str) -> Result<Void, FileError> {
    std::fs::remove_dir_all(path)
        .map(|_| Void::default()).map_err(FileError::from_io_error)
}

/// List directory contents
/// 
/// # Arguments
/// * `path` - Path to the directory
/// 
/// # Returns
/// * `Result<DirEntryVec, FileError>` - Directory entries or error
#[cfg(feature = "std")]
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
            name: AzString::from(entry.file_name().to_string_lossy().to_string()),
            path: AzString::from(entry.path().to_string_lossy().to_string()),
            file_type,
        });
    }
    
    Ok(DirEntryVec::from_vec(result))
}

// ============================================================================
// Path operations
// ============================================================================

/// Join two paths
/// 
/// # Arguments
/// * `base` - Base path
/// * `path` - Path to join
/// 
/// # Returns
/// * `AzString` - Joined path
#[cfg(feature = "std")]
pub fn path_join(base: &str, path: &str) -> AzString {
    let joined = Path::new(base).join(path);
    AzString::from(joined.to_string_lossy().to_string())
}

/// Get the parent directory of a path
/// 
/// # Arguments
/// * `path` - Path to get parent of
/// 
/// # Returns
/// * `Option<AzString>` - Parent path or None
#[cfg(feature = "std")]
pub fn path_parent(path: &str) -> Option<AzString> {
    Path::new(path).parent()
        .map(|p| AzString::from(p.to_string_lossy().to_string()))
}

/// Get the file name from a path
/// 
/// # Arguments
/// * `path` - Path to get file name from
/// 
/// # Returns
/// * `Option<AzString>` - File name or None
#[cfg(feature = "std")]
pub fn path_file_name(path: &str) -> Option<AzString> {
    Path::new(path).file_name()
        .map(|n| AzString::from(n.to_string_lossy().to_string()))
}

/// Get the file extension from a path
/// 
/// # Arguments
/// * `path` - Path to get extension from
/// 
/// # Returns
/// * `Option<AzString>` - Extension or None
#[cfg(feature = "std")]
pub fn path_extension(path: &str) -> Option<AzString> {
    Path::new(path).extension()
        .map(|e| AzString::from(e.to_string_lossy().to_string()))
}

/// Canonicalize a path (resolve symlinks, make absolute)
/// 
/// # Arguments
/// * `path` - Path to canonicalize
/// 
/// # Returns
/// * `Result<AzString, FileError>` - Canonical path or error
#[cfg(feature = "std")]
pub fn path_canonicalize(path: &str) -> Result<AzString, FileError> {
    let canonical = std::fs::canonicalize(path)
        .map_err(FileError::from_io_error)?;
    Ok(AzString::from(canonical.to_string_lossy().to_string()))
}

// ============================================================================
// Temporary files
// ============================================================================

/// Get the system temporary directory
/// 
/// # Returns
/// * `AzString` - Path to temp directory
#[cfg(feature = "std")]
pub fn temp_dir() -> AzString {
    AzString::from(std::env::temp_dir().to_string_lossy().to_string())
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
    [Debug, Clone, PartialEq]
);

// Option type for FilePath
impl_option!(FilePath, OptionFilePath, copy = false, [Clone, Debug, PartialEq]);

impl Default for FilePath {
    fn default() -> Self {
        Self { inner: AzString::from_const_str("") }
    }
}

impl FilePath {
    /// Creates a new path from a string
    pub fn new(path: AzString) -> Self {
        Self { inner: path }
    }

    /// Creates an empty path
    pub fn empty() -> Self {
        Self::default()
    }

    /// Creates a path from a string slice
    pub fn from_str(s: &str) -> Self {
        Self { inner: AzString::from(String::from(s)) }
    }

    /// Returns the system temporary directory
    #[cfg(feature = "std")]
    pub fn get_temp_dir() -> Self {
        Self { inner: temp_dir() }
    }

    /// Returns the current working directory
    #[cfg(feature = "std")]
    pub fn get_current_dir() -> Result<FilePath, FileError> {
        match std::env::current_dir() {
            Ok(p) => Ok(Self { inner: AzString::from(p.to_string_lossy().into_owned()) }),
            Err(e) => Err(FileError::from_io_error(e)),
        }
    }

    /// Returns the user's home directory (e.g., /home/username on Linux, C:\Users\username on Windows)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_home_dir() -> Option<FilePath> {
        dirs::home_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's cache directory (e.g., ~/.cache on Linux, ~/Library/Caches on macOS)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_cache_dir() -> Option<FilePath> {
        dirs::cache_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's config directory (e.g., ~/.config on Linux, ~/Library/Application Support on macOS)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_config_dir() -> Option<FilePath> {
        dirs::config_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's local config directory (e.g., ~/.config on Linux, ~/Library/Application Support on macOS)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_config_local_dir() -> Option<FilePath> {
        dirs::config_local_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's data directory (e.g., ~/.local/share on Linux, ~/Library/Application Support on macOS)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_data_dir() -> Option<FilePath> {
        dirs::data_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's local data directory (e.g., ~/.local/share on Linux, ~/Library/Application Support on macOS)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_data_local_dir() -> Option<FilePath> {
        dirs::data_local_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's desktop directory (e.g., ~/Desktop)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_desktop_dir() -> Option<FilePath> {
        dirs::desktop_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's documents directory (e.g., ~/Documents)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_document_dir() -> Option<FilePath> {
        dirs::document_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's downloads directory (e.g., ~/Downloads)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_download_dir() -> Option<FilePath> {
        dirs::download_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's executable directory (e.g., ~/.local/bin on Linux)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_executable_dir() -> Option<FilePath> {
        dirs::executable_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's font directory (e.g., ~/.local/share/fonts on Linux, ~/Library/Fonts on macOS)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_font_dir() -> Option<FilePath> {
        dirs::font_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's pictures directory (e.g., ~/Pictures)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_picture_dir() -> Option<FilePath> {
        dirs::picture_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's preference directory (e.g., ~/.config on Linux, ~/Library/Preferences on macOS)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_preference_dir() -> Option<FilePath> {
        dirs::preference_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's public directory (e.g., ~/Public)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_public_dir() -> Option<FilePath> {
        dirs::public_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's runtime directory (e.g., /run/user/1000 on Linux)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_runtime_dir() -> Option<FilePath> {
        dirs::runtime_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's state directory (e.g., ~/.local/state on Linux)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_state_dir() -> Option<FilePath> {
        dirs::state_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's audio directory (e.g., ~/Music)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_audio_dir() -> Option<FilePath> {
        dirs::audio_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's video directory (e.g., ~/Videos)
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_video_dir() -> Option<FilePath> {
        dirs::video_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Returns the user's templates directory
    #[cfg(all(feature = "std", feature = "extra"))]
    pub fn get_template_dir() -> Option<FilePath> {
        dirs::template_dir().map(|p| Self { inner: AzString::from(p.to_string_lossy().into_owned()) })
    }

    /// Joins this path with another path component
    pub fn join(&self, other: &FilePath) -> FilePath {
        FilePath { inner: path_join(self.inner.as_str(), other.inner.as_str()) }
    }

    /// Joins this path with a string component  
    pub fn join_str(&self, component: &AzString) -> FilePath {
        FilePath { inner: path_join(self.inner.as_str(), component.as_str()) }
    }

    /// Returns the parent directory of this path
    #[cfg(feature = "std")]
    pub fn parent(&self) -> Option<FilePath> {
        path_parent(self.inner.as_str()).map(|p| FilePath { inner: p })
    }

    /// Returns the file name component of this path
    #[cfg(feature = "std")]
    pub fn file_name(&self) -> Option<AzString> {
        path_file_name(self.inner.as_str())
    }

    /// Returns the file extension of this path
    #[cfg(feature = "std")]
    pub fn extension(&self) -> Option<AzString> {
        path_extension(self.inner.as_str())
    }

    /// Checks if the path exists on the filesystem
    #[cfg(feature = "std")]
    pub fn exists(&self) -> bool {
        path_exists(self.inner.as_str())
    }

    /// Checks if the path is a file
    #[cfg(feature = "std")]
    pub fn is_file(&self) -> bool {
        path_is_file(self.inner.as_str())
    }

    /// Checks if the path is a directory
    #[cfg(feature = "std")]
    pub fn is_dir(&self) -> bool {
        path_is_dir(self.inner.as_str())
    }

    /// Checks if the path is absolute
    #[cfg(feature = "std")]
    pub fn is_absolute(&self) -> bool {
        Path::new(self.inner.as_str()).is_absolute()
    }

    /// Creates this directory and all parent directories
    #[cfg(feature = "std")]
    pub fn create_dir_all(&self) -> Result<Void, FileError> {
        dir_create_all(self.inner.as_str())
    }

    /// Creates this directory (parent must exist)
    #[cfg(feature = "std")]
    pub fn create_dir(&self) -> Result<Void, FileError> {
        dir_create(self.inner.as_str())
    }

    /// Removes this file
    #[cfg(feature = "std")]
    pub fn remove_file(&self) -> Result<Void, FileError> {
        file_delete(self.inner.as_str())
    }

    /// Removes this directory (must be empty)
    #[cfg(feature = "std")]
    pub fn remove_dir(&self) -> Result<Void, FileError> {
        dir_delete(self.inner.as_str())
    }

    /// Removes this directory and all contents
    #[cfg(feature = "std")]
    pub fn remove_dir_all(&self) -> Result<Void, FileError> {
        dir_delete_all(self.inner.as_str())
    }

    /// Reads the entire file at this path as bytes
    #[cfg(feature = "std")]
    pub fn read_bytes(&self) -> Result<U8Vec, FileError> {
        file_read(self.inner.as_str())
    }

    /// Reads the entire file at this path as a string
    #[cfg(feature = "std")]
    pub fn read_string(&self) -> Result<AzString, FileError> {
        file_read_string(self.inner.as_str())
    }

    /// Writes bytes to the file at this path
    #[cfg(feature = "std")]
    pub fn write_bytes(&self, data: &U8Vec) -> Result<Void, FileError> {
        file_write(self.inner.as_str(), data.as_ref())
    }

    /// Writes a string to the file at this path
    #[cfg(feature = "std")]
    pub fn write_string(&self, data: &AzString) -> Result<Void, FileError> {
        file_write_string(self.inner.as_str(), data.as_str())
    }

    /// Copies a file from this path to another path
    #[cfg(feature = "std")]
    pub fn copy_to(&self, dest: &FilePath) -> Result<u64, FileError> {
        file_copy(self.inner.as_str(), dest.inner.as_str())
    }

    /// Renames/moves a file from this path to another path
    #[cfg(feature = "std")]
    pub fn rename_to(&self, dest: &FilePath) -> Result<Void, FileError> {
        file_rename(self.inner.as_str(), dest.inner.as_str())
    }

    /// Returns the path as a string reference
    pub fn as_str(&self) -> &str {
        self.inner.as_str()
    }

    /// Returns the path as an AzString
    pub fn as_string(&self) -> AzString {
        self.inner.clone()
    }

    /// Lists directory contents
    #[cfg(feature = "std")]
    pub fn read_dir(&self) -> Result<DirEntryVec, FileError> {
        dir_list(self.inner.as_str())
    }

    /// Returns metadata about the file/directory
    #[cfg(feature = "std")]
    pub fn metadata(&self) -> Result<FileMetadata, FileError> {
        file_metadata(self.inner.as_str())
    }

    /// Makes the path canonical (absolute, with no `.` or `..` components)
    #[cfg(feature = "std")]
    pub fn canonicalize(&self) -> Result<FilePath, FileError> {
        path_canonicalize(self.inner.as_str()).map(|p| FilePath { inner: p })
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
