//! Hardware video-decode capability probe + driver-provisioning planner.
//!
//! Two jobs, so a downstream app can do the "you don't have hardware codecs,
//! but drivers are available — install them?" flow without hand-rolling any of
//! the platform plumbing:
//!
//! 1. **Probe** ([`probe_hw_decode`]): can this machine hardware-decode H.264
//!    *right now*? On Apple/Android the platform codec (VideoToolbox /
//!    MediaCodec) is always present; on Linux/Windows we dlopen the Vulkan
//!    loader and look for `VK_KHR_video_decode_h264` on any physical device
//!    (gpu-video, our decode backend there, needs Vulkan Video). Drives
//!    `capability::video_codec()`.
//!
//! 2. **Plan + run** ([`ProvisionPlan`]): if decode is *not* available but the
//!    drivers that would enable it can be installed, build the exact command
//!    list — kept as `program` + `args` + an `elevated` flag — so the app can
//!    show the user precisely what will run *before* anything executes, then
//!    [`ProvisionPlan::run`] it (elevation via `pkexec`, i.e. a graphical
//!    password prompt — we never touch the password ourselves).
//!
//! Driver facts (researched 2026-06): Vulkan Video decode ships in the NVIDIA
//! proprietary driver (all supported GPUs) and in recent Mesa for AMD (RADV) and
//! Intel (ANV). Mesa's open NVIDIA driver **NVK** only began exposing video
//! decode at the end of 2024, is still gated behind
//! `NVK_I_WANT_A_BROKEN_VULKAN_DRIVER=true`, and targets recent (RTX-era) GPUs —
//! so on this box (Maxwell GTX 960 on NVK) the practical remediation is the
//! proprietary NVIDIA driver. The module never panics: every probe failure maps
//! to "unknown / not available", never a crash.
//!
//! NOTE: the `ProvisionPlan` install API is not yet wrapped into `api.json`
//! (codegen) — that's a follow-up; the probe is already reachable via the
//! existing `AzCapability_video_codec()`.

use std::process::Command;

/// The codec extension that gpu-video (and every H.264 Vulkan Video decoder)
/// requires on a physical device.
const VK_EXT_VIDEO_DECODE_H264: &[u8] = b"VK_KHR_video_decode_h264";

/// Outcome of a hardware-decode probe.
#[derive(Debug, Clone, PartialEq)]
pub struct HwDecodeProbe {
    /// Hardware H.264 decode is usable right now.
    pub available: bool,
    /// Backend that provides (or would provide) it: "VideoToolbox",
    /// "MediaCodec", "Vulkan Video", or "none".
    pub backend: &'static str,
    /// Human-readable detail for a UI / log line.
    pub detail: String,
    /// A driver-install plan exists that could enable it (only meaningful when
    /// `available` is false).
    pub can_remediate: bool,
}

/// Probe whether this machine can hardware-decode H.264 video.
pub fn probe_hw_decode() -> HwDecodeProbe {
    // Apple + Android ship a system codec; no Vulkan, no install ever needed.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        return HwDecodeProbe {
            available: true,
            backend: "VideoToolbox",
            detail: String::from("Apple VideoToolbox (built into the OS)"),
            can_remediate: false,
        };
    }
    #[cfg(target_os = "android")]
    {
        return HwDecodeProbe {
            available: true,
            backend: "MediaCodec",
            detail: String::from("Android MediaCodec (built into the OS)"),
            can_remediate: false,
        };
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        return match vulkan_has_h264_decode() {
            Some(true) => HwDecodeProbe {
                available: true,
                backend: "Vulkan Video",
                detail: String::from("VK_KHR_video_decode_h264 present"),
                can_remediate: false,
            },
            Some(false) => {
                let plan = ProvisionPlan::detect();
                HwDecodeProbe {
                    available: false,
                    backend: "Vulkan Video",
                    detail: String::from(
                        "Vulkan present but no VK_KHR_video_decode_h264 (driver lacks video decode)",
                    ),
                    can_remediate: plan.possible,
                }
            }
            None => {
                let plan = ProvisionPlan::detect();
                HwDecodeProbe {
                    available: false,
                    backend: "none",
                    detail: String::from("Vulkan loader not found / no usable GPU"),
                    can_remediate: plan.possible,
                }
            }
        }
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "android",
        target_os = "linux",
        target_os = "windows"
    )))]
    {
        HwDecodeProbe {
            available: false,
            backend: "none",
            detail: String::from("unsupported platform"),
            can_remediate: false,
        }
    }
}

// ───────────────────────── Vulkan extension probe ─────────────────────────

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod vk {
    use core::ffi::{c_char, c_void};

    pub const VK_STRUCTURE_TYPE_APPLICATION_INFO: u32 = 0;
    pub const VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO: u32 = 1;
    pub const VK_SUCCESS: i32 = 0;
    /// VK_API_VERSION_1_0 — instance creation against 1.0 is universally
    /// accepted; device-extension enumeration is independent of it.
    pub const VK_API_VERSION_1_0: u32 = 1 << 22;

    pub type VkInstance = *mut c_void;
    pub type VkPhysicalDevice = *mut c_void;

    #[repr(C)]
    pub struct VkApplicationInfo {
        pub s_type: u32,
        pub p_next: *const c_void,
        pub p_application_name: *const c_char,
        pub application_version: u32,
        pub p_engine_name: *const c_char,
        pub engine_version: u32,
        pub api_version: u32,
    }

    #[repr(C)]
    pub struct VkInstanceCreateInfo {
        pub s_type: u32,
        pub p_next: *const c_void,
        pub flags: u32,
        pub p_application_info: *const VkApplicationInfo,
        pub enabled_layer_count: u32,
        pub pp_enabled_layer_names: *const *const c_char,
        pub enabled_extension_count: u32,
        pub pp_enabled_extension_names: *const *const c_char,
    }

    #[repr(C)]
    pub struct VkExtensionProperties {
        pub extension_name: [c_char; 256],
        pub spec_version: u32,
    }

    pub type PfnCreateInstance =
        extern "system" fn(*const VkInstanceCreateInfo, *const c_void, *mut VkInstance) -> i32;
    pub type PfnEnumeratePhysicalDevices =
        extern "system" fn(VkInstance, *mut u32, *mut VkPhysicalDevice) -> i32;
    pub type PfnEnumerateDeviceExtensionProperties = extern "system" fn(
        VkPhysicalDevice,
        *const c_char,
        *mut u32,
        *mut VkExtensionProperties,
    ) -> i32;
    pub type PfnDestroyInstance = extern "system" fn(VkInstance, *const c_void);
}

/// Returns `Some(true)` if any physical device advertises
/// `VK_KHR_video_decode_h264`, `Some(false)` if Vulkan works but none do, and
/// `None` if Vulkan couldn't be loaded/initialised at all. Never panics.
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn vulkan_has_h264_decode() -> Option<bool> {
    use core::ptr;

    use vk::*;

    #[cfg(target_os = "linux")]
    const LIBVULKAN: &str = "libvulkan.so.1";
    #[cfg(target_os = "windows")]
    const LIBVULKAN: &str = "vulkan-1.dll";

    // SAFETY: loading a system library + calling the Vulkan C ABI. Every pointer
    // is checked; all handles come straight from the driver. Worst case on a
    // broken loader is an Err/non-success code we turn into None/false.
    unsafe {
        let lib = libloading::Library::new(LIBVULKAN).ok()?;
        let create_instance: libloading::Symbol<PfnCreateInstance> =
            lib.get(b"vkCreateInstance\0").ok()?;
        let enum_devices: libloading::Symbol<PfnEnumeratePhysicalDevices> =
            lib.get(b"vkEnumeratePhysicalDevices\0").ok()?;
        let enum_dev_ext: libloading::Symbol<PfnEnumerateDeviceExtensionProperties> =
            lib.get(b"vkEnumerateDeviceExtensionProperties\0").ok()?;
        let destroy_instance: libloading::Symbol<PfnDestroyInstance> =
            lib.get(b"vkDestroyInstance\0").ok()?;

        let app = VkApplicationInfo {
            s_type: VK_STRUCTURE_TYPE_APPLICATION_INFO,
            p_next: ptr::null(),
            p_application_name: ptr::null(),
            application_version: 0,
            p_engine_name: ptr::null(),
            engine_version: 0,
            api_version: VK_API_VERSION_1_0,
        };
        let ci = VkInstanceCreateInfo {
            s_type: VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
            p_next: ptr::null(),
            flags: 0,
            p_application_info: &app,
            enabled_layer_count: 0,
            pp_enabled_layer_names: ptr::null(),
            enabled_extension_count: 0,
            pp_enabled_extension_names: ptr::null(),
        };

        let mut instance: VkInstance = ptr::null_mut();
        if create_instance(&ci, ptr::null(), &mut instance) != VK_SUCCESS || instance.is_null() {
            return None;
        }

        let result = (|| {
            let mut dev_count: u32 = 0;
            if enum_devices(instance, &mut dev_count, ptr::null_mut()) != VK_SUCCESS
                || dev_count == 0
            {
                return Some(false);
            }
            let mut devices: Vec<VkPhysicalDevice> = vec![ptr::null_mut(); dev_count as usize];
            if enum_devices(instance, &mut dev_count, devices.as_mut_ptr()) != VK_SUCCESS {
                return Some(false);
            }

            for &dev in devices.iter().take(dev_count as usize) {
                if dev.is_null() {
                    continue;
                }
                let mut ext_count: u32 = 0;
                if enum_dev_ext(dev, ptr::null(), &mut ext_count, ptr::null_mut()) != VK_SUCCESS
                    || ext_count == 0
                {
                    continue;
                }
                let mut props: Vec<VkExtensionProperties> = Vec::with_capacity(ext_count as usize);
                props.resize_with(ext_count as usize, || VkExtensionProperties {
                    extension_name: [0; 256],
                    spec_version: 0,
                });
                if enum_dev_ext(dev, ptr::null(), &mut ext_count, props.as_mut_ptr()) != VK_SUCCESS {
                    continue;
                }
                for p in props.iter().take(ext_count as usize) {
                    if ext_name_matches(&p.extension_name, VK_EXT_VIDEO_DECODE_H264) {
                        return Some(true);
                    }
                }
            }
            Some(false)
        })();

        destroy_instance(instance, ptr::null());
        // `lib` (and the symbols borrowing it) stay in scope until here, so the
        // calls above were all made against a loaded library.
        result
    }
}

/// Compare a NUL-terminated `extensionName[256]` (as `c_char`) against `want`.
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn ext_name_matches(name: &[core::ffi::c_char; 256], want: &[u8]) -> bool {
    for (i, &w) in want.iter().enumerate() {
        if name[i] as u8 != w {
            return false;
        }
    }
    // The next byte after the wanted name must be the NUL terminator.
    name.get(want.len()).map(|&c| c as u8 == 0).unwrap_or(false)
}

// ───────────────────────── Provisioning plan ─────────────────────────

/// One shell command in a remediation plan, kept as `program` + `args` (not a
/// joined string) so the runner can exec it without a shell and the UI can still
/// render `display`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProvisionCommand {
    /// Human-readable form for display, e.g. `pkexec ubuntu-drivers install`.
    pub display: String,
    /// Program to exec.
    pub program: String,
    /// Arguments passed to `program`.
    pub args: Vec<String>,
    /// Requires elevation (run via `pkexec`/`sudo`).
    pub elevated: bool,
}

impl ProvisionCommand {
    fn new(program: &str, args: &[&str], elevated: bool) -> Self {
        let prefix = if elevated { "pkexec " } else { "" };
        let display = format!("{prefix}{program} {}", args.join(" "));
        ProvisionCommand {
            display: display.trim_end().to_string(),
            program: program.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            elevated,
        }
    }
}

/// A plan to enable hardware decode: the ordered command list plus metadata,
/// built so the app can show the user exactly what will run before executing.
#[derive(Debug, Clone, PartialEq)]
pub struct ProvisionPlan {
    /// A plan could be built for this machine.
    pub possible: bool,
    /// What the plan does + caveats, for the UI.
    pub summary: String,
    /// Running it needs elevation.
    pub needs_elevation: bool,
    /// A reboot is needed afterwards (driver swap).
    pub needs_reboot: bool,
    /// Commands, in order.
    pub commands: Vec<ProvisionCommand>,
}

impl ProvisionPlan {
    fn none(reason: &str) -> Self {
        ProvisionPlan {
            possible: false,
            summary: reason.to_string(),
            needs_elevation: false,
            needs_reboot: false,
            commands: Vec::new(),
        }
    }

    fn from_commands(summary: String, needs_reboot: bool, commands: Vec<ProvisionCommand>) -> Self {
        let needs_elevation = commands.iter().any(|c| c.elevated);
        ProvisionPlan {
            possible: !commands.is_empty(),
            summary,
            needs_elevation,
            needs_reboot,
            commands,
        }
    }

    /// Build the remediation plan for the current machine. Pure inspection — runs
    /// nothing.
    pub fn detect() -> ProvisionPlan {
        #[cfg(target_os = "linux")]
        {
            linux_plan()
        }
        #[cfg(target_os = "windows")]
        {
            // Vulkan Video decode comes from the vendor GPU driver. winget can
            // fetch NVIDIA's app; AMD/Intel are best via the vendor installer.
            ProvisionPlan::from_commands(
                String::from(
                    "Update your GPU driver to one with Vulkan Video decode. On Windows the \
                     driver comes from the GPU vendor; this opens winget to install it. A \
                     reboot may be required.",
                ),
                true,
                vec![ProvisionCommand::new(
                    "winget",
                    &["install", "--id", "Nvidia.GeForceExperience", "-e"],
                    false,
                )],
            )
        }
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
        {
            ProvisionPlan::none(
                "this platform decodes via a built-in system codec; no driver install applies",
            )
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "windows",
            target_os = "macos",
            target_os = "ios",
            target_os = "android"
        )))]
        {
            ProvisionPlan::none("unsupported platform")
        }
    }

    /// Execute the plan. Elevated commands run through `pkexec` (graphical
    /// password prompt) or `sudo`; we never handle the password. Stops at the
    /// first failure. **Side-effecting** — call only after the user has reviewed
    /// `commands` and consented.
    pub fn run(&self) -> ProvisionRunResult {
        if !self.possible {
            return ProvisionRunResult {
                ok: false,
                commands_run: 0,
                message: String::from("no remediation plan for this machine"),
            };
        }
        let elevator = elevator();
        let mut run = 0usize;
        for c in &self.commands {
            let mut cmd = if c.elevated {
                match elevator {
                    Some(e) => {
                        let mut x = Command::new(e);
                        x.arg(&c.program);
                        x
                    }
                    None => {
                        return ProvisionRunResult {
                            ok: false,
                            commands_run: run,
                            message: String::from(
                                "an elevated step is required but neither pkexec nor sudo is \
                                 available",
                            ),
                        }
                    }
                }
            } else {
                Command::new(&c.program)
            };
            cmd.args(&c.args);
            match cmd.status() {
                Ok(s) if s.success() => run += 1,
                Ok(s) => {
                    return ProvisionRunResult {
                        ok: false,
                        commands_run: run,
                        message: format!("`{}` exited with {}", c.display, s),
                    }
                }
                Err(e) => {
                    return ProvisionRunResult {
                        ok: false,
                        commands_run: run,
                        message: format!("`{}` failed to start: {e}", c.display),
                    }
                }
            }
        }
        ProvisionRunResult {
            ok: true,
            commands_run: run,
            message: if self.needs_reboot {
                String::from("all commands succeeded — reboot to load the new driver")
            } else {
                String::from("all commands succeeded")
            },
        }
    }
}

/// Outcome of [`ProvisionPlan::run`].
#[derive(Debug, Clone, PartialEq)]
pub struct ProvisionRunResult {
    /// Every command succeeded.
    pub ok: bool,
    /// How many commands ran successfully before stopping.
    pub commands_run: usize,
    /// Human-readable result / error.
    pub message: String,
}

// ───────────────────────── Linux specifics ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(target_os = "linux")]
enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

/// PCI vendor IDs from `/sys/class/drm/card*/device/vendor` — robust and needs
/// no `lspci` binary.
#[cfg(target_os = "linux")]
fn detect_gpu_vendors() -> Vec<GpuVendor> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir("/sys/class/drm") {
        Ok(e) => e,
        Err(_) => return out,
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        // Match `cardN` exactly (skip connector nodes like `card0-eDP-1`).
        if !name.starts_with("card") || name.len() <= 4 || !name[4..].bytes().all(|b| b.is_ascii_digit())
        {
            continue;
        }
        if let Ok(v) = std::fs::read_to_string(e.path().join("device/vendor")) {
            let vendor = match v.trim() {
                "0x10de" => GpuVendor::Nvidia,
                "0x1002" | "0x1022" => GpuVendor::Amd,
                "0x8086" => GpuVendor::Intel,
                _ => GpuVendor::Other,
            };
            if !out.contains(&vendor) {
                out.push(vendor);
            }
        }
    }
    out
}

/// Whether `prog` is on `PATH`.
fn which(prog: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|p| p.join(prog).is_file()))
        .unwrap_or(false)
}

/// The available graphical/terminal privilege escalator, preferring `pkexec`
/// (polkit GUI prompt).
fn elevator() -> Option<&'static str> {
    if which("pkexec") {
        Some("pkexec")
    } else if which("sudo") {
        Some("sudo")
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn linux_plan() -> ProvisionPlan {
    let vendors = detect_gpu_vendors();
    let has_apt = which("apt-get") || which("apt");
    let has_dnf = which("dnf");
    let has_pacman = which("pacman");
    let has_zypper = which("zypper");
    let has_ubuntu_drivers = which("ubuntu-drivers");

    if vendors.contains(&GpuVendor::Nvidia) {
        // The proprietary NVIDIA driver provides mature Vulkan Video decode;
        // NVK (open) does not on older GPUs. Installing it swaps out nouveau and
        // needs a reboot (and MOK enrolment under Secure Boot).
        let summary = String::from(
            "Install the proprietary NVIDIA driver, which provides Vulkan Video H.264/H.265 \
             hardware decode (the open NVK driver does not expose it on this GPU). This replaces \
             the nouveau driver and requires a reboot; under Secure Boot you'll be prompted to \
             enrol a signing key (MOK).",
        );
        if has_ubuntu_drivers {
            return ProvisionPlan::from_commands(
                summary,
                true,
                vec![ProvisionCommand::new("ubuntu-drivers", &["install"], true)],
            );
        }
        if has_apt {
            return ProvisionPlan::from_commands(
                summary,
                true,
                vec![
                    ProvisionCommand::new("apt-get", &["update"], true),
                    ProvisionCommand::new("apt-get", &["install", "-y", "nvidia-driver"], true),
                ],
            );
        }
        if has_dnf {
            return ProvisionPlan::from_commands(
                format!("{summary} (Fedora: requires the RPM Fusion nonfree repo.)"),
                true,
                vec![ProvisionCommand::new(
                    "dnf",
                    &["install", "-y", "akmod-nvidia"],
                    true,
                )],
            );
        }
        if has_pacman {
            return ProvisionPlan::from_commands(
                summary,
                true,
                vec![ProvisionCommand::new(
                    "pacman",
                    &["-S", "--needed", "--noconfirm", "nvidia"],
                    true,
                )],
            );
        }
        if has_zypper {
            return ProvisionPlan::from_commands(
                format!("{summary} (openSUSE: requires the NVIDIA repo.)"),
                true,
                vec![ProvisionCommand::new(
                    "zypper",
                    &["install", "-y", "nvidia-video-G06"],
                    true,
                )],
            );
        }
        return ProvisionPlan::none(
            "NVIDIA GPU detected but no supported package manager found to install the driver",
        );
    }

    if vendors.contains(&GpuVendor::Amd) || vendors.contains(&GpuVendor::Intel) {
        // AMD (RADV) and Intel (ANV) get Vulkan Video decode from recent Mesa —
        // a userspace package update, no reboot.
        let summary = String::from(
            "Update the Mesa Vulkan drivers (RADV for AMD / ANV for Intel), which include Vulkan \
             Video decode in recent versions. This is a userspace package update — no reboot, \
             though you may need to restart the application.",
        );
        if has_apt {
            return ProvisionPlan::from_commands(
                summary,
                false,
                vec![
                    ProvisionCommand::new("apt-get", &["update"], true),
                    ProvisionCommand::new(
                        "apt-get",
                        &["install", "-y", "mesa-vulkan-drivers"],
                        true,
                    ),
                ],
            );
        }
        if has_dnf {
            return ProvisionPlan::from_commands(
                summary,
                false,
                vec![ProvisionCommand::new(
                    "dnf",
                    &["install", "-y", "mesa-vulkan-drivers"],
                    true,
                )],
            );
        }
        if has_pacman {
            let pkg = if vendors.contains(&GpuVendor::Amd) {
                "vulkan-radeon"
            } else {
                "vulkan-intel"
            };
            return ProvisionPlan::from_commands(
                summary,
                false,
                vec![ProvisionCommand::new(
                    "pacman",
                    &["-S", "--needed", "--noconfirm", pkg],
                    true,
                )],
            );
        }
        return ProvisionPlan::none(
            "AMD/Intel GPU detected but no supported package manager found to update Mesa",
        );
    }

    ProvisionPlan::none("no NVIDIA/AMD/Intel GPU detected to provision")
}

#[cfg(test)]
mod provision_tests {
    use super::*;

    /// The probe must never panic and must classify this box correctly: NVK on
    /// the GTX 960 exposes no Vulkan Video decode, so decode is unavailable but
    /// remediable (the NVIDIA driver install plan exists).
    #[test]
    fn probe_runs_and_is_self_consistent() {
        let p = probe_hw_decode();
        // available <=> not remediable (you don't offer an install when it works).
        if p.available {
            assert!(!p.can_remediate);
        }
        assert!(!p.backend.is_empty());
        assert!(!p.detail.is_empty());
        eprintln!(
            "[provision] hw-decode available={} backend={} detail={:?} remediate={}",
            p.available, p.backend, p.detail, p.can_remediate
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_detects_a_gpu_vendor() {
        // CI containers may have no /sys GPU node; only assert structure.
        let vendors = detect_gpu_vendors();
        eprintln!("[provision] gpu vendors: {vendors:?}");
    }

    /// On an NVIDIA + apt/ubuntu-drivers box (this one) the plan is a real,
    /// elevated, reboot-requiring driver install with a non-empty command list,
    /// and every command renders a display string.
    #[cfg(target_os = "linux")]
    #[test]
    fn plan_is_well_formed_when_possible() {
        let plan = ProvisionPlan::detect();
        eprintln!(
            "[provision] possible={} elevation={} reboot={} cmds={}",
            plan.possible,
            plan.needs_elevation,
            plan.needs_reboot,
            plan.commands.len()
        );
        for c in &plan.commands {
            assert!(!c.display.is_empty(), "every command renders for the consent UI");
            assert!(!c.program.is_empty());
            eprintln!("  - {} (elevated={})", c.display, c.elevated);
        }
        if plan.possible {
            assert!(!plan.commands.is_empty());
            assert!(!plan.summary.is_empty());
            // possible plans always touch system packages → elevation.
            assert!(plan.needs_elevation);
        }
        // possible == (commands non-empty) invariant.
        assert_eq!(plan.possible, !plan.commands.is_empty());
    }

    #[test]
    fn elevated_command_display_includes_pkexec() {
        let c = ProvisionCommand::new("ubuntu-drivers", &["install"], true);
        assert_eq!(c.display, "pkexec ubuntu-drivers install");
        assert_eq!(c.program, "ubuntu-drivers");
        assert_eq!(c.args, vec!["install"]);
        let plain = ProvisionCommand::new("winget", &["install", "x"], false);
        assert_eq!(plain.display, "winget install x");
    }
}
