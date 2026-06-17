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

use azul_css::AzString;

/// The codec extension that gpu-video (and every H.264 Vulkan Video decoder)
/// requires on a physical device.
const VK_EXT_VIDEO_DECODE_H264: &[u8] = b"VK_KHR_video_decode_h264";

/// The extension a Vulkan Video **encoder** requires — absent on decode-only GPUs
/// (e.g. NVIDIA Maxwell / GTX 9xx), which is exactly what [`VideoEncodeCheck`]
/// warns about.
const VK_EXT_VIDEO_ENCODE_H264: &[u8] = b"VK_KHR_video_encode_h264";

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
        return match vulkan_has_ext(VK_EXT_VIDEO_DECODE_H264) {
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

/// Returns `Some(true)` if any physical device advertises the extension `want`
/// (e.g. `VK_KHR_video_decode_h264` / `VK_KHR_video_encode_h264`), `Some(false)`
/// if Vulkan works but none do, and `None` if Vulkan couldn't be
/// loaded/initialised at all. Never panics.
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn vulkan_has_ext(want: &[u8]) -> Option<bool> {
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
        let lib = crate::desktop::open_first_lib(&[LIBVULKAN])?;
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
                    if ext_name_matches(&p.extension_name, want) {
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
    /// How elevation is obtained, so the app can message correctly and decide
    /// whether to offer an in-app "Install" button:
    /// - `"pkexec"` — the **secure path**: polkit shows the OS's own trusted
    ///   password/biometric dialog; our process never sees the secret. (Polkit
    ///   itself can use a fingerprint via `pam_fprintd` — the same backend
    ///   azul-vault uses — if the admin configured it.) Safe to run from a GUI.
    /// - `"sudo"` — only works from a real terminal (sudo prompts on the tty).
    ///   A GUI should show the commands and ask the user to run them, NOT collect
    ///   the password itself.
    /// - `"none"` — no elevation needed, or no escalator available.
    pub elevation: String,
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
            elevation: String::from("none"),
            needs_reboot: false,
            commands: Vec::new(),
        }
    }

    fn from_commands(summary: String, needs_reboot: bool, commands: Vec<ProvisionCommand>) -> Self {
        let needs_elevation = commands.iter().any(|c| c.elevated);
        // Report the escalator the app would actually get. pkexec is preferred
        // (graphical, OS-owned prompt); we never collect the secret ourselves.
        let elevation = if needs_elevation {
            elevator().unwrap_or("none").to_string()
        } else {
            String::from("none")
        };
        ProvisionPlan {
            possible: !commands.is_empty(),
            summary,
            needs_elevation,
            elevation,
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
                reboot_required: false,
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
                            reboot_required: false,
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
                        reboot_required: false,
                        message: format!("`{}` exited with {}", c.display, s),
                    }
                }
                Err(e) => {
                    return ProvisionRunResult {
                        ok: false,
                        commands_run: run,
                        reboot_required: false,
                        message: format!("`{}` failed to start: {e}", c.display),
                    }
                }
            }
        }
        ProvisionRunResult {
            ok: true,
            commands_run: run,
            reboot_required: self.needs_reboot,
            message: if self.needs_reboot {
                String::from("driver installed — reboot now to load it")
            } else {
                String::from("all commands succeeded")
            },
        }
    }
}

/// Reboot the machine — the action an app fires when the user confirms the
/// "driver installed — reboot now?" prompt after a `reboot_required` install.
///
/// On Linux this goes through `systemctl reboot`, which logind normally lets an
/// **active local session** do *without* a password (no elevation needed — a
/// nicer flow than re-prompting); if polkit refuses, it retries via `pkexec`.
/// **Reboots immediately** — call only on explicit user confirmation. Returns
/// only if the reboot was *not* initiated (on success the process is torn down
/// with the system).
pub fn reboot_now() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        if try_spawn("systemctl", &["reboot"]) {
            return Ok(());
        }
        if let Some(e) = elevator() {
            if try_spawn(e, &["systemctl", "reboot"]) {
                return Ok(());
            }
        }
        Err(String::from(
            "could not initiate reboot (systemctl reboot refused; try `reboot` manually)",
        ))
    }
    #[cfg(target_os = "windows")]
    {
        if try_spawn("shutdown", &["/r", "/t", "0"]) {
            return Ok(());
        }
        Err(String::from("could not initiate reboot (shutdown /r failed)"))
    }
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
    {
        // These platforms decode via a built-in codec; a driver-install reboot
        // shouldn't arise. Provided for completeness on desktop macOS.
        #[cfg(target_os = "macos")]
        if let Some(e) = elevator() {
            if try_spawn(e, &["shutdown", "-r", "now"]) {
                return Ok(());
            }
        }
        Err(String::from("reboot not initiated on this platform"))
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    )))]
    {
        Err(String::from("reboot not supported on this platform"))
    }
}

/// Run `program args...`, returning whether it exited successfully. Used for the
/// reboot trigger (a success means the reboot was accepted / the process is
/// about to be torn down).
fn try_spawn(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ───────────────── Reboot-safety gate (the incident fix) ─────────────────

/// Whether booting a given kernel can actually reach the current root filesystem.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct RebootSafety {
    /// The target kernel's module set can reach the running root filesystem.
    pub safe: bool,
    /// What was checked / what is missing (for logs and the UI).
    pub detail: String,
}

/// Verify a kernel can reach `/` *before* anyone reboots into it.
///
/// The failure this prevents: a driver install pulled a brand-new kernel whose
/// initramfs lacked the boot disk's controller driver (`pata_atiixp`, which
/// ships in `linux-modules-extra`) -> the disk never appeared -> BusyBox. Every
/// package was `ii` and the GPU module built, so package-state checks all said
/// "safe to reboot". This gate asks the real question instead: does
/// `kernel_version`'s module set contain the driver for the disk that backs `/`,
/// plus `dm-crypt` when root is encrypted?
///
/// Reads only the world-readable `modules.dep`/`modules.builtin` (no root, no
/// `lsinitramfs`) — which alone would have caught the incident, since the driver
/// was simply absent from the new kernel's tree. Call before offering
/// [`reboot_now`] / declaring "safe to reboot". Non-Linux targets return
/// `safe = true` (no per-kernel initramfs concept).
pub fn reboot_safety_check(kernel_version: &str) -> RebootSafety {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = kernel_version;
        RebootSafety {
            safe: true,
            detail: String::from("not applicable on this platform"),
        }
    }
    #[cfg(target_os = "linux")]
    {
        let mut reasons = Vec::new();
        let mut safe = true;

        match root_disk_driver() {
            Some(driver) => {
                if kernel_has_module(kernel_version, &driver) {
                    reasons.push(format!("root-disk driver `{driver}` is in {kernel_version}"));
                } else {
                    safe = false;
                    reasons.push(format!(
                        "MISSING root-disk driver `{driver}` in {kernel_version} — the disk will \
                         not appear; install linux-modules-extra-{kernel_version}"
                    ));
                }
            }
            None => reasons.push(String::from(
                "could not resolve the root disk's driver (check skipped)",
            )),
        }

        if root_is_encrypted() {
            if kernel_has_module(kernel_version, "dm-crypt") {
                reasons.push(String::from("dm-crypt present (LUKS root)"));
            } else {
                safe = false;
                reasons.push(format!(
                    "MISSING dm-crypt in {kernel_version} — encrypted root unreachable"
                ));
            }
        }

        RebootSafety {
            safe,
            detail: reasons.join("; "),
        }
    }
}

/// stdout of `program args...` (trimmed), or None on failure/empty.
#[cfg(target_os = "linux")]
fn capture(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// The kernel driver for the **storage controller** the disk backing `/` hangs
/// off (e.g. `pata_atiixp`, `ahci`, `nvme`, `virtio_pci`) — the at-risk module,
/// not the generic disk driver (`sd`) which is always present.
///
/// `/sys/block/<disk>/device/driver` is the SCSI disk (`sd`); the controller is
/// further up the device tree. We canonicalise the block device's sysfs path and
/// return the driver of the deepest PCI function in its ancestry — i.e. the
/// host controller closest to the disk.
#[cfg(target_os = "linux")]
fn root_disk_driver() -> Option<String> {
    let src = capture("findmnt", &["-no", "SOURCE", "/"])?;
    // Inverse device tree (raw, no tree-drawing chars): find the TYPE=disk node.
    let tree = capture("lsblk", &["-rnso", "NAME,TYPE", &src])?;
    let disk = tree.lines().find_map(|line| {
        let mut it = line.split_whitespace();
        let name = it.next()?;
        if it.next() == Some("disk") {
            Some(name.to_string())
        } else {
            None
        }
    })?;
    // e.g. /sys/devices/pci0000:00/0000:00:14.1/ata1/host0/.../block/sda
    let real = std::fs::canonicalize(format!("/sys/block/{disk}")).ok()?;
    for anc in real.ancestors() {
        let name = match anc.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        // A PCI function dir is "dddd:bb:dd.f" — two ':' and a '.'.
        if name.matches(':').count() == 2 && name.contains('.') {
            if let Ok(link) = std::fs::read_link(anc.join("driver")) {
                if let Some(d) = link.file_name().and_then(|s| s.to_str()) {
                    return Some(d.to_string()); // deepest PCI device first -> the controller
                }
            }
        }
    }
    None
}

/// Is `/` backed by a dm-crypt (LUKS) device anywhere in its stack?
#[cfg(target_os = "linux")]
fn root_is_encrypted() -> bool {
    capture("findmnt", &["-no", "SOURCE", "/"])
        .and_then(|src| capture("lsblk", &["-rnso", "TYPE", &src]))
        .map(|types| types.lines().any(|t| t.trim() == "crypt"))
        .unwrap_or(false)
}

/// Is `module` available to `kernel_version` — as a loadable `.ko` or built-in?
/// Reads the world-readable `modules.dep`/`modules.builtin`; accepts both `-`
/// and `_` spellings (dm-crypt vs dm_crypt).
#[cfg(target_os = "linux")]
fn kernel_has_module(kernel_version: &str, module: &str) -> bool {
    let base = format!("/lib/modules/{kernel_version}");
    let names = [module.replace('_', "-"), module.replace('-', "_")];
    for index in ["modules.dep", "modules.builtin"] {
        if let Ok(contents) = std::fs::read_to_string(format!("{base}/{index}")) {
            for n in &names {
                if contents.contains(&format!("/{n}.ko")) {
                    return true;
                }
            }
        }
    }
    false
}

/// The newest installed kernel — the one GRUB version-sorts to as default. The
/// kernel a "safe to reboot?" check should target. (Best-effort approximation;
/// an explicit `GRUB_DEFAULT` can override the sort order.)
#[cfg(target_os = "linux")]
pub fn newest_installed_kernel() -> Option<String> {
    capture("sh", &["-c", "ls -1 /lib/modules | sort -V | tail -1"])
}

/// A repair plan for a kernel that [`reboot_safety_check`] flagged as unable to
/// reach root — the "detect a broken install and offer to fix it" path, for
/// users who hit the bug before the upstream/our-side fixes landed but still
/// want hardware video decode.
///
/// On Debian/Ubuntu the missing storage/crypt driver lives in
/// `linux-modules-extra-<kver>`; installing it and rebuilding that kernel's
/// initramfs is the whole fix (the exact recovery from the field incident). The
/// returned [`ProvisionPlan`] runs through the same consent + [`ProvisionPlan::run`]
/// path as everything else (shows the commands first, elevates via pkexec). An
/// already-bootable kernel yields an empty (`possible == false`) plan.
pub fn repair_kernel_plan(kernel_version: &str) -> ProvisionPlan {
    let safety = reboot_safety_check(kernel_version);
    if safety.safe {
        return ProvisionPlan::none(&format!(
            "{kernel_version} can already reach root ({}); nothing to repair",
            safety.detail
        ));
    }
    #[cfg(target_os = "linux")]
    {
        if which("apt-get") {
            let extra = format!("linux-modules-extra-{kernel_version}");
            return ProvisionPlan::from_commands(
                format!(
                    "{kernel_version} cannot reach root: {}. Install {extra} (it carries the \
                     missing driver) and rebuild that kernel's initramfs. Reboot afterwards to \
                     use the repaired kernel.",
                    safety.detail
                ),
                true, // a reboot is needed to actually run the repaired kernel
                vec![
                    ProvisionCommand::new("apt-get", &["install", "-y", extra.as_str()], true),
                    ProvisionCommand::new(
                        "update-initramfs",
                        &["-u", "-k", kernel_version],
                        true,
                    ),
                ],
            );
        }
    }
    ProvisionPlan::none(&format!(
        "no automatic repair available for {kernel_version}: {}",
        safety.detail
    ))
}

// ───────────────── One-call startup readiness check (DLL surface) ─────────────────

/// Outcome of applying a remediation ([`VideoStartupCheck::remediate`]).
#[repr(C)]
#[derive(Debug, Clone)]
pub struct VideoProvisionOutcome {
    /// Every step succeeded.
    pub ok: bool,
    /// A reboot is needed to finish (load the new driver / kernel).
    pub reboot_required: bool,
    /// Human-readable result / error.
    pub message: AzString,
}

impl From<ProvisionRunResult> for VideoProvisionOutcome {
    fn from(r: ProvisionRunResult) -> Self {
        VideoProvisionOutcome {
            ok: r.ok,
            reboot_required: r.reboot_required,
            message: r.message.into(),
        }
    }
}

/// A single startup readiness check for hardware video decode — the function an
/// app calls once at launch (before its main loop) to decide whether to warn the
/// user and offer a fix, instead of discovering a missing codec mid-session.
///
/// Pure inspection: [`run`](Self::run) changes nothing. If it reports work to do,
/// [`remediate`](Self::remediate) applies it (driver install and/or kernel
/// repair) through the consent + pkexec path.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct VideoStartupCheck {
    /// Hardware video decode is usable right now.
    pub hw_decode_ready: bool,
    /// The kernel GRUB will boot can actually reach root (won't drop to an
    /// initramfs shell) — the safeguard against the kernel-provisioning footgun.
    pub boot_safe: bool,
    /// An automatic remediation exists (driver install and/or kernel repair).
    pub can_remediate: bool,
    /// Applying the remediation will require a reboot.
    pub needs_reboot: bool,
    /// One-line status for a startup banner.
    pub summary: AzString,
    /// Full multi-line report (capability + boot-safety + the exact commands a
    /// remediation would run) for a details pane / consent dialog.
    pub detail: AzString,
}

/// (boot_safe, human detail, a kernel repair is available)
#[cfg(target_os = "linux")]
fn startup_boot_check() -> (bool, String, bool) {
    match newest_installed_kernel() {
        Some(k) => {
            let s = reboot_safety_check(&k);
            let repairable = !s.safe && repair_kernel_plan(&k).possible;
            (s.safe, format!("kernel {k}: {}", s.detail), repairable)
        }
        None => (true, String::from("no installed-kernel info available"), false),
    }
}
#[cfg(not(target_os = "linux"))]
fn startup_boot_check() -> (bool, String, bool) {
    (true, String::from("boot-safety check is Linux-only"), false)
}

impl VideoStartupCheck {
    /// Run all the readiness checks (call once at startup). Inspection only.
    pub fn run() -> VideoStartupCheck {
        let probe = probe_hw_decode();
        let (boot_safe, boot_detail, kernel_repairable) = startup_boot_check();
        let driver_plan = ProvisionPlan::detect();
        let driver_installable = !probe.available && driver_plan.possible;

        let can_remediate = kernel_repairable || driver_installable;
        let needs_reboot = !boot_safe || (driver_installable && driver_plan.needs_reboot);

        let summary = if probe.available && boot_safe {
            String::from("Hardware video decode is ready.")
        } else if !boot_safe {
            String::from(
                "A kernel update left the default kernel unbootable — a one-click repair is \
                 available.",
            )
        } else if driver_installable {
            String::from("Hardware video decode is off, but the drivers can be installed.")
        } else {
            format!("Hardware video decode is unavailable: {}", probe.detail)
        };

        let mut detail = format!(
            "hardware decode: available={} backend={} — {}\nboot path: {} {}",
            probe.available,
            probe.backend,
            probe.detail,
            if boot_safe { "OK —" } else { "UNBOOTABLE —" },
            boot_detail,
        );
        if driver_installable {
            detail.push_str(&format!("\ndriver install: {}", driver_plan.summary));
            for c in &driver_plan.commands {
                detail.push_str("\n  ");
                detail.push_str(&c.display);
            }
        }

        VideoStartupCheck {
            hw_decode_ready: probe.available,
            boot_safe,
            can_remediate,
            needs_reboot,
            summary: summary.into(),
            detail: detail.into(),
        }
    }

    /// Apply whatever [`run`](Self::run) found — repair an unbootable kernel
    /// first (the urgent one), then install the GPU driver — via the consent +
    /// pkexec path. **Side-effecting**; call only after the user has seen
    /// `detail` and consented.
    pub fn remediate() -> VideoProvisionOutcome {
        #[cfg(target_os = "linux")]
        if let Some(k) = newest_installed_kernel() {
            let rp = repair_kernel_plan(&k);
            if rp.possible {
                let r = rp.run();
                if !r.ok {
                    return VideoProvisionOutcome::from(r);
                }
            }
        }
        let dp = ProvisionPlan::detect();
        if dp.possible {
            return VideoProvisionOutcome::from(dp.run());
        }
        VideoProvisionOutcome {
            ok: true,
            reboot_required: false,
            message: AzString::from_const_str("nothing to remediate"),
        }
    }
}

/// Hardware **encode** capability + software fallback, so an app can warn the user
/// when only software (x264) encoding is available on this machine. The encode-side
/// mirror of [`VideoStartupCheck`]. Inspection only — changes nothing.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct VideoEncodeCheck {
    /// Hardware H.264 encode is usable right now.
    pub hw_encode_ready: bool,
    /// A software encoder (gstreamer `x264enc`) is available as a fallback.
    pub software_fallback: bool,
    /// Backend providing (or that would provide) hardware encode: "Vulkan Video",
    /// "VideoToolbox", "MediaCodec", or "none".
    pub backend: AzString,
    /// One-line capability summary for a banner / warning.
    pub summary: AzString,
    /// Full multi-line report (hardware + software fallback).
    pub detail: AzString,
}

impl VideoEncodeCheck {
    /// Probe hardware + software H.264 encode (call once at startup). Inspection only.
    pub fn run() -> VideoEncodeCheck {
        let (hw, backend, hw_detail) = probe_hw_encode();
        let software_fallback = software_x264_available();
        let summary = if hw {
            format!("Hardware H.264 encode is ready ({backend}).")
        } else if software_fallback {
            String::from(
                "Hardware H.264 encode is NOT available on this GPU — using software (x264) \
                 encoding (slower, CPU-bound).",
            )
        } else {
            String::from(
                "No H.264 encoder available: no hardware encode and no software x264 \
                 (install gstreamer1.0-plugins-ugly).",
            )
        };
        let detail = format!(
            "hardware encode: available={hw} backend={backend} — {hw_detail}\n\
             software fallback (gstreamer x264enc): {}",
            if software_fallback {
                "available"
            } else {
                "NOT found (install gstreamer1.0-plugins-ugly / x264)"
            },
        );
        VideoEncodeCheck {
            hw_encode_ready: hw,
            software_fallback,
            backend: AzString::from_const_str(backend),
            summary: summary.into(),
            detail: detail.into(),
        }
    }
}

/// Probe hardware H.264 **encode** capability: `(available, backend, detail)`.
fn probe_hw_encode() -> (bool, &'static str, String) {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        return (
            true,
            "VideoToolbox",
            String::from("Apple VideoToolbox (built into the OS)"),
        );
    }
    #[cfg(target_os = "android")]
    {
        return (
            true,
            "MediaCodec",
            String::from("Android MediaCodec (built into the OS)"),
        );
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        return match vulkan_has_ext(VK_EXT_VIDEO_ENCODE_H264) {
            Some(true) => (
                true,
                "Vulkan Video",
                String::from("VK_KHR_video_encode_h264 present"),
            ),
            Some(false) => (
                false,
                "Vulkan Video",
                String::from(
                    "Vulkan present but no VK_KHR_video_encode_h264 (GPU/driver lacks hardware \
                     H.264 encode — e.g. NVIDIA Maxwell / GTX 9xx are decode-only)",
                ),
            ),
            None => (
                false,
                "none",
                String::from("Vulkan loader not found / no usable GPU"),
            ),
        };
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "android",
        target_os = "linux",
        target_os = "windows"
    )))]
    {
        (
            false,
            "none",
            String::from("no hardware-encode backend for this platform"),
        )
    }
}

/// Whether gstreamer's software x264 encoder (`x264enc`) is installed + usable.
fn software_x264_available() -> bool {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        // `gst-inspect-1.0 x264enc` exits 0 iff the element is registered.
        return std::process::Command::new("gst-inspect-1.0")
            .arg("x264enc")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

/// Outcome of [`ProvisionPlan::run`].
#[derive(Debug, Clone, PartialEq)]
pub struct ProvisionRunResult {
    /// Every command succeeded.
    pub ok: bool,
    /// How many commands ran successfully before stopping.
    pub commands_run: usize,
    /// The install succeeded AND a reboot is required to load the new driver —
    /// the app's cue to show "driver installed, reboot now". Always false on
    /// failure. (The pre-install equivalent is `ProvisionPlan::needs_reboot`.)
    pub reboot_required: bool,
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

// ───────────────────────── Install progress ─────────────────────────
//
// So an app can draw a progress bar over the multi-minute polkit install
// instead of being blind, the runner streams each command's output and parses
// apt's machine-readable `APT::Status-Fd` lines into an overall percentage +
// the human-readable current step. (Threaded pollable handle: next step; this
// is the parser + percent math it builds on, all unit-testable.)

/// State of a (possibly background) provisioning install.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionState {
    /// Not started.
    Idle,
    /// A command is currently running.
    Running,
    /// All commands succeeded.
    Succeeded,
    /// A command failed (see `message`); the install stopped.
    Failed,
}

/// Which phase an apt `Status-Fd` line reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AptPhase {
    /// Downloading archives (`dlstatus:`).
    Download,
    /// Unpacking / configuring (`pmstatus:`).
    Install,
    /// An error line (`pmerror:`).
    Error,
    /// A conffile prompt (`pmconffile:`).
    ConfFile,
}

/// One parsed apt `Status-Fd` line.
#[derive(Debug, Clone, PartialEq)]
pub struct AptStatus {
    /// The phase this line reports.
    pub phase: AptPhase,
    /// apt's own 0..100 percent for that phase.
    pub percent: f32,
    /// Human-readable description (e.g. "Setting up nvidia-driver-535").
    pub description: String,
}

/// Parse one line apt writes to the fd named by `-o APT::Status-Fd=N`.
///
/// The format is `kind:item:percent:description`, where `description` may itself
/// contain colons (URLs), so only the first three colons are split on. Returns
/// `None` for any other line (plain apt chatter), so a caller can feed it every
/// stdout line indiscriminately.
pub fn parse_apt_status_line(line: &str) -> Option<AptStatus> {
    let mut parts = line.splitn(4, ':');
    let kind = parts.next()?;
    let phase = match kind {
        "dlstatus" => AptPhase::Download,
        "pmstatus" => AptPhase::Install,
        "pmerror" => AptPhase::Error,
        "pmconffile" => AptPhase::ConfFile,
        _ => return None,
    };
    let _item = parts.next()?;
    let percent = parts.next()?.trim().parse::<f32>().ok()?;
    let description = parts.next().unwrap_or("").trim().to_string();
    Some(AptStatus {
        phase,
        percent: percent.clamp(0.0, 100.0),
        description,
    })
}

/// Map one apt status update to an intra-command 0..100 percent: downloads fill
/// the lower half, unpack/configure the upper half, so a command that downloads
/// then installs sweeps 0→100 monotonically (an install with nothing to fetch
/// simply starts at 50).
pub fn command_percent(status: &AptStatus) -> f32 {
    match status.phase {
        AptPhase::Download => status.percent * 0.5,
        AptPhase::Install => 50.0 + status.percent * 0.5,
        // Error/conffile don't move the bar.
        AptPhase::Error | AptPhase::ConfFile => 0.0,
    }
}

/// Overall 0..100 across the whole plan: completed commands count full, the
/// in-flight command contributes its own fraction.
pub fn overall_percent(completed_steps: usize, total_steps: usize, cmd_percent: f32) -> u32 {
    if total_steps == 0 {
        return 100;
    }
    let cmd = cmd_percent.clamp(0.0, 100.0);
    let done = (completed_steps.min(total_steps)) as f32 * 100.0;
    let overall = (done + cmd) / total_steps as f32;
    overall.clamp(0.0, 100.0).round() as u32
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
            // ...and the escalator must be named (this box has pkexec — the
            // secure graphical path).
            assert!(plan.elevation == "pkexec" || plan.elevation == "sudo");
        } else {
            assert_eq!(plan.elevation, "none");
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

    /// apt Status-Fd lines parse into phase + percent + a description that keeps
    /// its embedded colons (URLs); non-status chatter yields None.
    #[test]
    fn apt_status_lines_parse() {
        let dl = parse_apt_status_line("dlstatus:1:13.3766:Hole http://archive.ubuntu.com/x")
            .expect("dlstatus parses");
        assert_eq!(dl.phase, AptPhase::Download);
        assert!((dl.percent - 13.3766).abs() < 0.001);
        assert_eq!(dl.description, "Hole http://archive.ubuntu.com/x");

        let pm = parse_apt_status_line("pmstatus:nvidia-driver-535:50.0:Setting up nvidia-driver-535")
            .expect("pmstatus parses");
        assert_eq!(pm.phase, AptPhase::Install);
        assert_eq!(pm.percent, 50.0);

        assert_eq!(
            parse_apt_status_line("pmerror:pkg:0:boom").unwrap().phase,
            AptPhase::Error
        );
        assert_eq!(
            parse_apt_status_line("pmconffile:/etc/x:0:prompt").unwrap().phase,
            AptPhase::ConfFile
        );
        // Plain apt output / arbitrary lines are not status lines.
        assert!(parse_apt_status_line("Unpacking nvidia-driver-535 ...").is_none());
        assert!(parse_apt_status_line("").is_none());
    }

    /// Download fills the lower half of a command, install the upper half.
    #[test]
    fn command_percent_splits_download_and_install() {
        let dl = AptStatus { phase: AptPhase::Download, percent: 100.0, description: String::new() };
        assert_eq!(command_percent(&dl), 50.0);
        let pm0 = AptStatus { phase: AptPhase::Install, percent: 0.0, description: String::new() };
        assert_eq!(command_percent(&pm0), 50.0);
        let pm100 = AptStatus { phase: AptPhase::Install, percent: 100.0, description: String::new() };
        assert_eq!(command_percent(&pm100), 100.0);
    }

    /// Overall percent counts finished commands as full and adds the in-flight
    /// command's fraction; degenerate inputs clamp.
    #[test]
    fn overall_percent_combines_steps_and_command() {
        // 2-command plan: command 0 fully done, command 1 at 50% -> (100+50)/2 = 75.
        assert_eq!(overall_percent(1, 2, 50.0), 75);
        // Nothing done, first command at 0% -> 0.
        assert_eq!(overall_percent(0, 2, 0.0), 0);
        // Everything done.
        assert_eq!(overall_percent(2, 2, 100.0), 100);
        // No steps -> trivially complete; out-of-range clamps.
        assert_eq!(overall_percent(0, 0, 0.0), 100);
        assert_eq!(overall_percent(5, 2, 999.0), 100);
    }

    /// The running kernel can, by definition, reach root (it did) -> safe; a
    /// kernel with no module tree at all -> unsafe. This is the check that would
    /// have stopped the incident: the new kernel lacked the root-disk driver.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "needs a real Linux desktop: inspects /lib/modules + /boot for the \
                running and a synthetic kernel; minimal CI runners lack that baseline \
                so reboot_safety_check can't distinguish a bare kernel. Run explicitly."]
    fn reboot_safety_passes_running_kernel_fails_a_bare_one() {
        if let Some(kver) = capture("uname", &["-r"]) {
            let r = reboot_safety_check(&kver);
            eprintln!("[reboot-safety] {kver}: safe={} — {}", r.safe, r.detail);
            assert!(r.safe, "running kernel must reach root: {}", r.detail);
        }
        let bad = reboot_safety_check("0.0.0-nonexistent-generic");
        assert!(
            !bad.safe,
            "a kernel with no module tree must be unsafe: {}",
            bad.detail
        );
    }

    /// The one-call startup check is self-consistent and produces a non-empty
    /// summary/detail; on this (now-repaired) box the boot path is safe.
    #[test]
    fn startup_check_is_self_consistent() {
        let c = VideoStartupCheck::run();
        eprintln!(
            "[startup] hw_ready={} boot_safe={} remediable={} reboot={} :: {}",
            c.hw_decode_ready, c.boot_safe, c.can_remediate, c.needs_reboot, c.summary.as_str()
        );
        assert!(!c.summary.as_str().is_empty());
        assert!(!c.detail.as_str().is_empty());
        // ready <=> nothing to remediate for decode (driver side).
        if c.hw_decode_ready && c.boot_safe {
            assert!(!c.can_remediate);
        }
    }

    /// A broken kernel yields a repair plan that installs the matching
    /// modules-extra and rebuilds the initramfs; a healthy one yields nothing.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "needs a real Linux desktop: depends on apt package metadata + kernel \
                module trees to build a repair plan; minimal CI runners lack them. \
                Run explicitly."]
    fn repair_plan_targets_modules_extra() {
        if let Some(kver) = capture("uname", &["-r"]) {
            assert!(
                !repair_kernel_plan(&kver).possible,
                "the running (bootable) kernel needs no repair"
            );
        }
        let plan = repair_kernel_plan("0.0.0-nonexistent-generic");
        if which("apt-get") {
            assert!(plan.possible, "a broken kernel on apt should be repairable");
            let cmds: Vec<&str> = plan.commands.iter().map(|c| c.display.as_str()).collect();
            let joined = cmds.join(" | ");
            assert!(
                joined.contains("linux-modules-extra-0.0.0-nonexistent-generic"),
                "got: {joined}"
            );
            assert!(joined.contains("update-initramfs"), "got: {joined}");
            assert!(plan.needs_elevation && plan.needs_reboot);
        }
    }
}
