//! Best-effort SYSTEM information for slow-frame diagnostics.
//!
//! Gathered lazily, cached for the process lifetime, and attached to log
//! records ONLY when something is actually slow (the first slow frame of a
//! session) — a fast session sends none of this. That is the consent story:
//! hardware context is diagnostic payload for a problem report, not ambient
//! collection.
//!
//! Everything here is read from the OS without new dependencies:
//! `/proc/cpuinfo`, `/proc/meminfo`, `/etc/os-release`, `/sys/class/drm`
//! on Linux; `std::env::consts` everywhere. The GPU is the one field the
//! OS cannot answer better than the renderer itself — an app that owns a GL
//! context should call [`super::set_gpu_info`] with `GL_RENDERER`, which
//! overrides the sysfs guess.

use std::sync::OnceLock;

/// One flat snapshot of the machine, all fields best-effort ("unknown" when
/// the source is missing — a partial report beats no report).
#[derive(Debug, Clone)]
pub struct SystemInfo {
    /// CPU model name, e.g. `AMD Ryzen 7 5800X 8-Core Processor`.
    pub cpu_model: String,
    /// Logical CPU count.
    pub cpu_count: usize,
    /// Total physical RAM in bytes (0 = unknown).
    pub ram_total_bytes: u64,
    /// OS family + version, e.g. `linux (Ubuntu 24.04.2 LTS)`.
    pub os: String,
    /// Windowing system the process sees: `wayland`, `x11`, `win32`,
    /// `cocoa`, or `headless`.
    pub windowing: String,
    /// GPU description: the app-supplied renderer string if
    /// [`super::set_gpu_info`] was called, else a `/sys/class/drm` driver +
    /// PCI-id guess on Linux, else `unknown`.
    pub gpu: String,
}

impl SystemInfo {
    /// The snapshot as `(key, value)` pairs, ready to ride on a log record.
    #[must_use]
    pub fn as_attributes(&self) -> Vec<(String, String)> {
        vec![
            ("sys.cpu_model".to_owned(), self.cpu_model.clone()),
            ("sys.cpu_count".to_owned(), self.cpu_count.to_string()),
            (
                "sys.ram_total_bytes".to_owned(),
                self.ram_total_bytes.to_string(),
            ),
            ("sys.os".to_owned(), self.os.clone()),
            ("sys.windowing".to_owned(), self.windowing.clone()),
            ("sys.gpu".to_owned(), self.gpu.clone()),
        ]
    }
}

/// App-supplied GPU string (`GL_RENDERER`), set once via
/// [`super::set_gpu_info`].
pub(super) static GPU_INFO: OnceLock<String> = OnceLock::new();

/// The cached snapshot. First call gathers, later calls are free.
pub fn get() -> &'static SystemInfo {
    static INFO: OnceLock<SystemInfo> = OnceLock::new();
    INFO.get_or_init(gather)
}

fn gather() -> SystemInfo {
    SystemInfo {
        cpu_model: cpu_model(),
        cpu_count: std::thread::available_parallelism().map_or(0, std::num::NonZeroUsize::get),
        ram_total_bytes: ram_total_bytes(),
        os: os_description(),
        windowing: windowing_system(),
        gpu: GPU_INFO.get().cloned().unwrap_or_else(gpu_guess),
    }
}

fn cpu_model() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in cpuinfo.lines() {
                if let Some(rest) = line.strip_prefix("model name") {
                    if let Some((_, v)) = rest.split_once(':') {
                        return v.trim().to_owned();
                    }
                }
            }
        }
    }
    format!("unknown ({})", std::env::consts::ARCH)
}

fn ram_total_bytes() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if let Some(rest) = line.strip_prefix("MemTotal:") {
                    let kb: u64 = rest
                        .trim()
                        .trim_end_matches("kB")
                        .trim()
                        .parse()
                        .unwrap_or(0);
                    return kb.saturating_mul(1024);
                }
            }
        }
    }
    0
}

fn os_description() -> String {
    let family = std::env::consts::OS;
    #[cfg(target_os = "linux")]
    {
        if let Ok(rel) = std::fs::read_to_string("/etc/os-release") {
            for line in rel.lines() {
                if let Some(v) = line.strip_prefix("PRETTY_NAME=") {
                    return format!("{family} ({})", v.trim_matches('"'));
                }
            }
        }
    }
    family.to_owned()
}

fn windowing_system() -> String {
    #[cfg(target_os = "windows")]
    return "win32".to_owned();
    #[cfg(target_os = "macos")]
    return "cocoa".to_owned();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            "wayland".to_owned()
        } else if std::env::var_os("DISPLAY").is_some() {
            "x11".to_owned()
        } else {
            "headless".to_owned()
        }
    }
}

fn gpu_guess() -> String {
    #[cfg(target_os = "linux")]
    {
        // /sys/class/drm/card0/device/uevent has DRIVER=; vendor/device hold
        // the PCI ids. Enough to distinguish "nvidia 10de:2204" from "i915" —
        // the app's GL_RENDERER string (set_gpu_info) is always better.
        if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with("card") || name.contains('-') {
                    continue;
                }
                let dev = entry.path().join("device");
                let driver = std::fs::read_to_string(dev.join("uevent"))
                    .ok()
                    .and_then(|s| {
                        s.lines()
                            .find_map(|l| l.strip_prefix("DRIVER=").map(str::to_owned))
                    });
                let vendor = std::fs::read_to_string(dev.join("vendor"))
                    .map(|s| s.trim().trim_start_matches("0x").to_owned())
                    .unwrap_or_default();
                let device = std::fs::read_to_string(dev.join("device"))
                    .map(|s| s.trim().trim_start_matches("0x").to_owned())
                    .unwrap_or_default();
                if let Some(driver) = driver {
                    return format!("{driver} {vendor}:{device}");
                }
            }
        }
    }
    "unknown".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_has_no_empty_required_fields() {
        let info = get();
        assert!(!info.cpu_model.is_empty());
        assert!(!info.os.is_empty());
        assert!(!info.windowing.is_empty());
        assert!(!info.gpu.is_empty());
        // Linux CI and this dev box both expose /proc: RAM must be real there.
        #[cfg(target_os = "linux")]
        assert!(info.ram_total_bytes > 0, "MemTotal must parse on Linux");
    }

    #[test]
    fn attributes_carry_all_six_keys() {
        let attrs = get().as_attributes();
        let keys: Vec<&str> = attrs.iter().map(|(k, _)| k.as_str()).collect();
        for expected in [
            "sys.cpu_model",
            "sys.cpu_count",
            "sys.ram_total_bytes",
            "sys.os",
            "sys.windowing",
            "sys.gpu",
        ] {
            assert!(keys.contains(&expected), "missing {expected}");
        }
    }
}
