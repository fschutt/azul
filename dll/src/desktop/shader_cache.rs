//! Shader disk cache for WebRender program binaries.
//!
//! After a shader is lazily compiled + linked on first use, we extract the
//! binary via `glGetProgramBinary()` and store it on disk. On the next app
//! launch, the binary is loaded from disk and fed to `glProgramBinary()`,
//! skipping the expensive compile + link step (~10-50ms per shader).
//!
//! Cache layout:
//!   ~/Library/Caches/azul/shaders/<renderer_hash>/    (macOS)
//!   ~/.cache/azul/shaders/<renderer_hash>/             (Linux)
//!   %LOCALAPPDATA%\azul\shaders\<renderer_hash>\       (Windows)
//!
//! Each shader binary is stored as:
//!   <digest_hex>.bin   — raw program binary bytes
//!   <digest_hex>.meta  — 12 bytes: format (u32 LE) + digest (u64 LE)
//!
//! The `<renderer_hash>` subdirectory is a hash of the GL renderer string +
//! GL version, ensuring that cache entries are invalidated when the GPU driver
//! changes.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
};

use webrender::{ProgramBinary, ProgramCache, ProgramCacheObserver, ProgramSourceDigest};

/// The most program binaries one renderer's cache directory keeps. WebRender has a few hundred shader variants and an app
/// uses a few dozen of them; a directory with more files than this is not ours any more (or a driver that writes a new
/// binary per run), and it is emptied.
pub const MAX_CACHED_PROGRAMS: usize = 512;

/// The most bytes one renderer's cache directory keeps (program binaries are 20 to 300 KB).
pub const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;

/// Whether a cache directory of `files` files and `bytes` bytes is over the caps: it is emptied, the programs compile
/// again, and the cache fills up once more.
#[must_use]
pub const fn over_cap(files: usize, bytes: u64) -> bool {
    files > MAX_CACHED_PROGRAMS || bytes > MAX_CACHE_BYTES
}

/// Checks a directory the platform hands the cache (an app's private cache directory): absolute, no `..`, a sane
/// length. A relative path would put the cache wherever the process happens to run.
///
/// # Errors
///
/// What is wrong with it, in a word the diagnostic line shows.
pub fn validate_base_dir(dir: &Path) -> Result<(), String> {
    if !dir.is_absolute() {
        return Err(format!("{} is not an absolute path", dir.display()));
    }
    if dir
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!("{} contains ..", dir.display()));
    }
    if dir.as_os_str().len() > 1024 {
        return Err("the path is longer than 1024 bytes".to_owned());
    }
    Ok(())
}

/// Where an Android app keeps the compiled programs, best first, from the app's private files directory
/// (`AndroidApp::internal_data_path`, `/data/user/0/<package>/files`): the app's `cache` directory beside it (the
/// system empties it when storage runs low, which is what a cache wants), else a directory of its own in `files`.
#[must_use]
pub fn android_cache_candidates(internal_data_path: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::with_capacity(2);
    if let Some(app_dir) = internal_data_path.parent() {
        candidates.push(app_dir.join("cache").join("azul").join("shaders"));
    }
    candidates.push(internal_data_path.join("azul-shaders"));
    candidates
}

/// The directory an Android app keeps the compiled programs in: the first of [`android_cache_candidates`] whose app directory
/// (two levels above the candidate: `cache` for the first, the app's own for the second) `is_dir` says exists, or `None` when none
/// does. The caller passes the file system's answer; the choice itself is pure.
#[must_use]
pub fn choose_android_cache_dir(
    internal_data_path: &Path,
    is_dir: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    android_cache_candidates(internal_data_path)
        .into_iter()
        .find(|candidate| candidate.ancestors().nth(2).is_some_and(&is_dir))
}

/// What opening a program cache did: the cache WebRender takes, how many program binaries were read from disk
/// (0 is a cold start, more a warm one), where, and why there is none.
#[derive(Debug, Clone, Default)]
pub struct ProgramCacheStatus {
    /// The directory in use.
    pub dir: Option<PathBuf>,
    /// The program binaries read from disk.
    pub loaded: usize,
    /// Why no cache is kept (a directory that cannot be made, a platform without one), when it is not.
    pub problem: Option<String>,
}

/// Disk-backed shader cache that implements WebRender's ProgramCacheObserver trait.
pub struct ShaderDiskCache {
    /// Directory where this cache stores shader binaries.
    /// Includes the renderer hash subdirectory.
    cache_dir: PathBuf,
    /// A failed write was reported once: a read-only or full disk is one diagnostic, not one per program.
    write_failure_reported: std::sync::atomic::AtomicBool,
}

impl ShaderDiskCache {
    /// Create a new shader disk cache for the given GL renderer.
    ///
    /// The `gl_renderer` and `gl_version` strings are hashed to create a
    /// subdirectory, ensuring cache invalidation when drivers change.
    ///
    /// Returns `None` if the cache directory cannot be determined.
    pub fn new(gl_renderer: &str, gl_version: &str) -> Option<Self> {
        Self::open(None, gl_renderer, gl_version).ok()
    }

    /// Like [`Self::new`] under `base_dir` (an app's private cache directory) when one is given, and with the reason
    /// when there is no cache. A directory over the caps ([`over_cap`]) is emptied.
    ///
    /// # Errors
    ///
    /// The directory is not valid, cannot be made, or the platform has none.
    pub fn open(
        base_dir: Option<&Path>,
        gl_renderer: &str,
        gl_version: &str,
    ) -> Result<Self, String> {
        let base = match base_dir {
            Some(dir) => {
                validate_base_dir(dir)?;
                dir.to_path_buf()
            }
            None => get_shader_cache_base_dir()
                .ok_or_else(|| "this platform keeps no shader cache".to_owned())?,
        };

        // Hash renderer + version to create a unique subdirectory
        let mut hasher = DefaultHasher::new();
        gl_renderer.hash(&mut hasher);
        gl_version.hash(&mut hasher);
        let renderer_hash = hasher.finish();

        let cache_dir = base.join(format!("{:016x}", renderer_hash));

        // Ensure directory exists
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("cannot make {}: {e}", cache_dir.display()))?;

        let cache = ShaderDiskCache {
            cache_dir,
            write_failure_reported: std::sync::atomic::AtomicBool::new(false),
        };
        // The caps are for the directory an app was handed (a phone's cache directory); the desktop's own directory is
        // as it was (it grows with every WebRender build: a ledger row, not a change of this commit).
        if base_dir.is_some() {
            cache.empty_when_over_cap();
        }
        Ok(cache)
    }

    /// The directory of this renderer's binaries.
    pub fn dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Empties the directory when it is over the caps.
    fn empty_when_over_cap(&self) {
        let Ok(entries) = std::fs::read_dir(&self.cache_dir) else {
            return;
        };
        let (mut files, mut bytes) = (0_usize, 0_u64);
        let mut paths = Vec::new();
        for entry in entries.flatten() {
            files += 1;
            bytes += entry.metadata().map_or(0, |m| m.len());
            paths.push(entry.path());
        }
        if over_cap(files, bytes) {
            crate::plog_warn!(
                "[Shader Cache] {} holds {} files, {} bytes: over the caps, emptied",
                self.cache_dir.display(),
                files,
                bytes
            );
            for path in paths {
                let _ = std::fs::remove_file(path);
            }
        }
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
        if let Err(e) = std::fs::write(&bin_path, &binary.bytes) {
            self.report_write_failure(&e);
            return;
        }

        // Write metadata: format (u32 LE) + digest (u64 LE)
        let mut meta = Vec::with_capacity(12);
        meta.extend_from_slice(&binary.format.to_le_bytes());
        meta.extend_from_slice(&binary.source_digest.0.to_le_bytes());
        if let Err(e) = std::fs::write(&meta_path, &meta) {
            self.report_write_failure(&e);
            // Remove orphaned .bin file to keep cache consistent
            let _ = std::fs::remove_file(&bin_path);
        }
    }

    /// A write that failed (a full or read-only disk) is a diagnostic, once, never a crash: the programs compile
    /// again next start.
    fn report_write_failure(&self, error: &std::io::Error) {
        if !self
            .write_failure_reported
            .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            crate::plog_warn!(
                "[Shader Cache] cannot write to {}: {error}; the programs compile again at the next start",
                self.cache_dir.display()
            );
        }
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
        Some(PathBuf::from(local_app_data).join("azul").join("shaders"))
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

#[cfg(test)]
#[path = "shader_cache_test.rs"]
mod tests;
