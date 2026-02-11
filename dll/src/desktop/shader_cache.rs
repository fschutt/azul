///! Shader disk cache for WebRender program binaries.
///!
///! After a shader is lazily compiled + linked on first use, we extract the
///! binary via `glGetProgramBinary()` and store it on disk. On the next app
///! launch, the binary is loaded from disk and fed to `glProgramBinary()`,
///! skipping the expensive compile + link step (~10-50ms per shader).
///!
///! Cache layout:
///!   ~/Library/Caches/azul/shaders/<renderer_hash>/    (macOS)
///!   ~/.cache/azul/shaders/<renderer_hash>/             (Linux)
///!   %LOCALAPPDATA%\azul\shaders\<renderer_hash>\       (Windows)
///!
///! Each shader binary is stored as:
///!   <digest_hex>.bin   — raw program binary bytes
///!   <digest_hex>.meta  — 12 bytes: format (u32 LE) + digest (u64 LE)
///!
///! The `<renderer_hash>` subdirectory is a hash of the GL renderer string +
///! GL version, ensuring that cache entries are invalidated when the GPU driver
///! changes.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use webrender::{ProgramBinary, ProgramCache, ProgramCacheObserver, ProgramSourceDigest};

/// Disk-backed shader cache that implements WebRender's ProgramCacheObserver trait.
pub struct ShaderDiskCache {
    /// Directory where this cache stores shader binaries.
    /// Includes the renderer hash subdirectory.
    cache_dir: PathBuf,
}

impl ShaderDiskCache {
    /// Create a new shader disk cache for the given GL renderer.
    ///
    /// The `gl_renderer` and `gl_version` strings are hashed to create a
    /// subdirectory, ensuring cache invalidation when drivers change.
    ///
    /// Returns `None` if the cache directory cannot be determined.
    pub fn new(gl_renderer: &str, gl_version: &str) -> Option<Self> {
        let base = get_shader_cache_base_dir()?;

        // Hash renderer + version to create a unique subdirectory
        let mut hasher = DefaultHasher::new();
        gl_renderer.hash(&mut hasher);
        gl_version.hash(&mut hasher);
        let renderer_hash = hasher.finish();

        let cache_dir = base.join(format!("{:016x}", renderer_hash));

        // Ensure directory exists
        if let Err(_) = std::fs::create_dir_all(&cache_dir) {
            return None;
        }

        Some(ShaderDiskCache { cache_dir })
    }

    /// Try to load all cached shader binaries from disk into the ProgramCache.
    /// Returns the number of shaders loaded.
    pub fn load_all_from_disk(&self, program_cache: &ProgramCache) -> usize {
        let mut count = 0;
        let entries = match std::fs::read_dir(&self.cache_dir) {
            Ok(e) => e,
            Err(_) => return 0,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("bin") {
                continue;
            }

            if let Some(binary) = self.load_one_shader(&path) {
                program_cache.load_program_binary(Arc::new(binary));
                count += 1;
            }
        }

        count
    }

    /// Load a single shader binary from its .bin + .meta files.
    fn load_one_shader(&self, bin_path: &Path) -> Option<ProgramBinary> {
        let meta_path = bin_path.with_extension("meta");

        let bytes = std::fs::read(bin_path).ok()?;
        let meta = std::fs::read(&meta_path).ok()?;

        // Meta format: format (4 bytes LE u32) + digest (8 bytes LE u64)
        if meta.len() != 12 {
            return None;
        }

        let format = u32::from_le_bytes([meta[0], meta[1], meta[2], meta[3]]);
        let digest_val = u64::from_le_bytes([
            meta[4], meta[5], meta[6], meta[7], meta[8], meta[9], meta[10], meta[11],
        ]);

        let digest = ProgramSourceDigest(digest_val);
        Some(ProgramBinary::new(bytes, format, digest))
    }

    /// Save a single shader binary to disk.
    fn save_one_shader(&self, binary: &ProgramBinary) {
        let digest_hex = format!("{:016x}", binary.source_digest.0);
        let bin_path = self.cache_dir.join(format!("{}.bin", digest_hex));
        let meta_path = self.cache_dir.join(format!("{}.meta", digest_hex));

        // Don't overwrite existing entries
        if bin_path.exists() {
            return;
        }

        // Write binary data
        if let Err(_) = std::fs::write(&bin_path, &binary.bytes) {
            return;
        }

        // Write metadata: format (u32 LE) + digest (u64 LE)
        let mut meta = Vec::with_capacity(12);
        meta.extend_from_slice(&binary.format.to_le_bytes());
        meta.extend_from_slice(&binary.source_digest.0.to_le_bytes());
        let _ = std::fs::write(&meta_path, &meta);
    }
}

impl ProgramCacheObserver for ShaderDiskCache {
    fn save_shaders_to_disk(&self, entries: Vec<Arc<ProgramBinary>>) {
        for binary in &entries {
            self.save_one_shader(binary);
        }
    }

    fn set_startup_shaders(&self, _entries: Vec<Arc<ProgramBinary>>) {
        // We don't maintain a separate startup shader list — we load all
        // cached binaries on startup. This callback is a no-op.
    }

    fn try_load_shader_from_disk(
        &self,
        digest: &ProgramSourceDigest,
        program_cache: &std::rc::Rc<ProgramCache>,
    ) {
        let digest_hex = format!("{:016x}", digest.0);
        let bin_path = self.cache_dir.join(format!("{}.bin", digest_hex));

        if let Some(binary) = self.load_one_shader(&bin_path) {
            program_cache.load_program_binary(Arc::new(binary));
        }
    }

    fn notify_program_binary_failed(&self, program_binary: &Arc<ProgramBinary>) {
        // A cached binary failed to link — remove it from disk so we don't
        // keep trying to load a broken binary.
        let digest_hex = format!("{:016x}", program_binary.source_digest.0);
        let bin_path = self.cache_dir.join(format!("{}.bin", digest_hex));
        let meta_path = self.cache_dir.join(format!("{}.meta", digest_hex));
        let _ = std::fs::remove_file(&bin_path);
        let _ = std::fs::remove_file(&meta_path);
    }
}

/// Get the base directory for shader cache.
fn get_shader_cache_base_dir() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let xdg = std::env::var("XDG_CACHE_HOME").ok();
        let base = match xdg {
            Some(dir) => PathBuf::from(dir),
            None => {
                let home = std::env::var("HOME").ok()?;
                PathBuf::from(home).join(".cache")
            }
        };
        Some(base.join("azul").join("shaders"))
    }

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").ok()?;
        Some(
            PathBuf::from(home)
                .join("Library")
                .join("Caches")
                .join("azul")
                .join("shaders"),
        )
    }

    #[cfg(target_os = "windows")]
    {
        let local_app_data = std::env::var("LOCALAPPDATA").ok()?;
        Some(
            PathBuf::from(local_app_data)
                .join("azul")
                .join("shaders"),
        )
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}
