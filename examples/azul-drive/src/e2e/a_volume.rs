//! Volumes of their own for a device: its folder at the top of one (`drive_volume`, or
//! `drive_pair` / `drive_open` with `fs`), ejected and plugged in again (`drive_detach`,
//! `drive_attach`: the same folder, a new device id), and another disk mounted at a folder
//! inside it (`drive_mount_inside`, `drive_unmount_inside`). Each is a disk image in the
//! sandbox: `hdiutil` on macOS; on Linux a loop device, which needs `sudo -n` (mounting is
//! root's there). The images go with `drive_cleanup` (detached first).

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use super::{
    a_state::{with_a, Volume},
    need, str_arg, with_harness, Answer, Args,
};

/// A file system as `fs` names it (`apfs:16m`, `fat32:40m`, `exfat:16m`, `ext4:64m`,
/// `apfs-cs:16m` - case-sensitive -, `hfs:16m`): its kind and size in MiB.
pub(super) fn parse_fs(spec: &str) -> Result<(String, u64), String> {
    let (kind, size) = spec.split_once(':').unwrap_or((spec, "32m"));
    let mib = size
        .trim_end_matches(['m', 'M'])
        .parse::<u64>()
        .map_err(|_| format!("'{spec}': the size is no number of MiB"))?;
    match kind {
        "apfs" | "apfs-cs" | "hfs" | "fat32" | "exfat" | "ext4" => Ok((kind.to_string(), mib)),
        other => Err(format!("no file system '{other}' (apfs, apfs-cs, hfs, fat32, exfat, ext4)")),
    }
}

#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
fn run(cmd: &mut Command) -> Result<String, String> {
    let shown = format!("{cmd:?}");
    let out = cmd.output().map_err(|e| format!("{shown}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{shown} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The image of a new volume of `kind` and `mib` MiB at `image` (its name `label`).
#[cfg(target_os = "macos")]
fn make_image(image: &Path, kind: &str, mib: u64, label: &str) -> Result<(), String> {
    let fs = match kind {
        "apfs" => "APFS",
        "apfs-cs" => "Case-sensitive APFS",
        "hfs" => "HFS+",
        "fat32" => "MS-DOS FAT32",
        "exfat" => "ExFAT",
        other => return Err(format!("macOS makes no '{other}' volume here (ext4 is Linux's)")),
    };
    run(Command::new("hdiutil")
        .args(["create", "-quiet", "-size"])
        .arg(format!("{mib}m"))
        .args(["-fs", fs, "-volname", label])
        .arg(image))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn attach(image: &Path, at: &Path) -> Result<(), String> {
    run(Command::new("hdiutil")
        .args(["attach", "-quiet", "-nobrowse", "-owners", "on", "-mountpoint"])
        .arg(at)
        .arg(image))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn detach(at: &Path) -> Result<(), String> {
    run(Command::new("hdiutil").args(["detach", "-quiet"]).arg(at))
        .or_else(|_| run(Command::new("hdiutil").args(["detach", "-quiet", "-force"]).arg(at)))?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn make_image(image: &Path, kind: &str, mib: u64, label: &str) -> Result<(), String> {
    fs::File::create(image)
        .and_then(|f| f.set_len(mib * 1024 * 1024))
        .map_err(|e| format!("{}: {e}", image.display()))?;
    let mut mkfs = match kind {
        "ext4" => {
            let mut c = Command::new("mkfs.ext4");
            c.args(["-q", "-F", "-L", label]);
            c
        }
        "fat32" => {
            let mut c = Command::new("mkfs.vfat");
            c.args(["-F", "32", "-n", &label.to_uppercase()]);
            c
        }
        "exfat" => {
            let mut c = Command::new("mkfs.exfat");
            c.args(["-n", label]);
            c
        }
        other => return Err(format!("Linux makes no '{other}' volume here (apfs and hfs are macOS's)")),
    };
    run(mkfs.arg(image))?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn attach(image: &Path, at: &Path) -> Result<(), String> {
    let id = |flag: &str| run(Command::new("id").arg(flag)).map(|s| s.trim().to_string());
    let (uid, gid) = (id("-u")?, id("-g")?);
    // FAT and exFAT keep no owners: the mount gives the files to this user.
    let owned = format!("loop,uid={uid},gid={gid}");
    let mounted = run(Command::new("sudo").args(["-n", "mount", "-o", &owned]).arg(image).arg(at))
        .or_else(|_| run(Command::new("sudo").args(["-n", "mount", "-o", "loop"]).arg(image).arg(at)));
    mounted.map_err(|e| format!("mounting a volume needs root on Linux (sudo -n): {e}"))?;
    let _ = run(Command::new("sudo").args(["-n", "chown", &format!("{uid}:{gid}")]).arg(at));
    Ok(())
}

#[cfg(target_os = "linux")]
fn detach(at: &Path) -> Result<(), String> {
    run(Command::new("sudo").args(["-n", "umount"]).arg(at))
        .or_else(|_| run(Command::new("sudo").args(["-n", "umount", "-l"]).arg(at)))?;
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn make_image(_image: &Path, _kind: &str, _mib: u64, _label: &str) -> Result<(), String> {
    Err(String::from("volumes of their own are made on macOS and Linux only"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn attach(_image: &Path, _at: &Path) -> Result<(), String> {
    Err(String::from("volumes of their own are made on macOS and Linux only"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn detach(_at: &Path) -> Result<(), String> {
    Ok(())
}

/// A new volume of `spec` mounted at `at` (an empty folder, made when missing), its image in
/// the sandbox's `volumes/`.
fn make(run_dir: &Path, name: &str, spec: &str, at: &Path) -> Result<Volume, String> {
    let (kind, mib) = parse_fs(spec)?;
    fs::create_dir_all(at).map_err(|e| format!("{}: {e}", at.display()))?;
    if fs::read_dir(at).map(|mut d| d.next().is_some()).unwrap_or(false) {
        return Err(format!("{} is not empty: a volume is mounted on an empty folder", at.display()));
    }
    let images = run_dir.join("volumes");
    fs::create_dir_all(&images).map_err(|e| format!("{}: {e}", images.display()))?;
    let ext = if cfg!(target_os = "macos") { "dmg" } else { "img" };
    let image = images.join(format!("{name}.{ext}"));
    let label: String = name.chars().filter(char::is_ascii_alphanumeric).take(11).collect();
    make_image(&image, &kind, mib, if label.is_empty() { "E2E" } else { &label })?;
    attach(&image, at)?;
    Ok(Volume {
        image,
        mount: at.to_path_buf(),
        attached: true,
    })
}

/// The device's folder at the top of a volume of `spec` of its own (`drive_volume`, and the
/// `fs` of `drive_pair` / `drive_open`): the folder must still be empty.
pub(super) fn root_volume(args: &Args, spec: &str) -> Result<PathBuf, String> {
    let (name, root, run_dir) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.name.clone(), d.root.clone(), h.run.clone()))
    })?;
    let volume = make(&run_dir, &format!("{name}-root"), spec, &root)?;
    with_a(|a| a.volumes.insert((name, String::new()), volume))?;
    Ok(root)
}

/// `drive_volume`.
pub(super) fn volume(args: &Args) -> Answer {
    let spec = need(args, "fs")?.to_string();
    let root = root_volume(args, &spec)?;
    Ok(serde_json::json!({ "mount": root, "fs": spec }))
}

/// The volume of the device's folder (`path` absent) or of a folder in it.
fn volume_key(args: &Args) -> Result<(String, String), String> {
    let name = with_harness(|h| Ok(h.device(args)?.name.clone()))?;
    Ok((name, str_arg(args, "path").unwrap_or("").trim_matches('/').to_string()))
}

/// `drive_detach` (`drive_unmount_inside` with a `path`): the volume ejected; its mount
/// point stays as an empty folder.
pub(super) fn detach_step(args: &Args) -> Answer {
    let key = volume_key(args)?;
    let mount = with_a(|a| a.volumes.get(&key).filter(|v| v.attached).map(|v| v.mount.clone()))?
        .ok_or("no volume of the device is attached there")?;
    detach(&mount)?;
    with_a(|a| {
        if let Some(v) = a.volumes.get_mut(&key) {
            v.attached = false;
        }
    })?;
    Ok(serde_json::json!({ "detached": mount }))
}

/// `drive_attach`: the volume plugged in again at the same folder (the system gives it a new
/// device id).
pub(super) fn attach_step(args: &Args) -> Answer {
    let key = volume_key(args)?;
    let (image, mount) = with_a(|a| {
        a.volumes
            .get(&key)
            .filter(|v| !v.attached)
            .map(|v| (v.image.clone(), v.mount.clone()))
    })?
    .ok_or("no detached volume of the device there")?;
    attach(&image, &mount)?;
    with_a(|a| {
        if let Some(v) = a.volumes.get_mut(&key) {
            v.attached = true;
        }
    })?;
    Ok(serde_json::json!({ "attached": mount }))
}

/// `drive_mount_inside`: another disk (a volume of `fs`) mounted at `path` inside the device's
/// folder, holding `files` (`{name: text}`).
pub(super) fn mount_inside(args: &Args) -> Answer {
    let (name, root, run_dir) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.name.clone(), d.root.clone(), h.run.clone()))
    })?;
    let path = need(args, "path")?.trim_matches('/').to_string();
    let spec = str_arg(args, "fs").unwrap_or("apfs:16m").to_string();
    let at = root.join(&path);
    let label = format!("{name}-{}", path.replace('/', "-"));
    let volume = make(&run_dir, &label, &spec, &at)?;
    if let Some(files) = args.get("files").and_then(serde_json::Value::as_object) {
        for (file, text) in files {
            let target = at.join(file);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            fs::write(&target, text.as_str().unwrap_or_default())
                .map_err(|e| format!("{}: {e}", target.display()))?;
        }
    }
    with_a(|a| a.volumes.insert((name, path.clone()), volume))?;
    Ok(serde_json::json!({ "mount": at }))
}

/// Every attached volume detached (before the sandbox goes).
pub(super) fn detach_all(volumes: Vec<Volume>) {
    // The inner mounts first: a volume mounted in another's folder.
    let mut attached: Vec<Volume> = volumes.into_iter().filter(|v| v.attached).collect();
    attached.sort_by_key(|v| std::cmp::Reverse(v.mount.components().count()));
    for v in attached {
        let _ = detach(&v.mount);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_system_is_named_by_its_kind_and_size() {
        assert_eq!(parse_fs("apfs:16m").unwrap(), (String::from("apfs"), 16));
        assert_eq!(parse_fs("fat32:40M").unwrap(), (String::from("fat32"), 40));
        assert_eq!(parse_fs("exfat").unwrap(), (String::from("exfat"), 32));
        assert!(parse_fs("ntfs:16m").is_err());
        assert!(parse_fs("ext4:big").is_err());
    }
}
