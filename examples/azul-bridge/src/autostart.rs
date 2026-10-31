//! Starting the bridge at login, the way each system does it for a user's background program,
//! without an installer or administrator rights:
//!
//! - macOS: a LaunchAgent, `~/Library/LaunchAgents/io.azlin.bridge.plist` (`RunAtLoad`, restarted
//!   when it stops with an error, its errors into `<state>/bridge.log`).
//! - Linux: an XDG autostart entry, `<config>/autostart/azul-bridge.desktop` (GNOME, KDE, Xfce,
//!   ... start it with the desktop session).
//! - Windows: `AzlinBridge.vbs` in the user's Startup folder (`<roaming app data>\Microsoft\Windows\
//!   Start Menu\Programs\Startup`), which runs the bridge without a console window.
//!
//! Each runs `azul-bridge --state-dir <state> serve` from where the binary is now. `enable` writes
//! the file (it takes effect at the next login; `serve` starts it now), `disable` removes it.

use std::path::{Path, PathBuf};

/// The LaunchAgent's label.
pub const LABEL: &str = "io.azlin.bridge";

/// The systems the bridge knows how to start at login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    MacOs,
    Linux,
    Windows,
}

impl System {
    /// This computer's.
    #[must_use]
    pub fn current() -> Option<System> {
        if cfg!(target_os = "macos") {
            Some(System::MacOs)
        } else if cfg!(target_os = "windows") {
            Some(System::Windows)
        } else if cfg!(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd")) {
            Some(System::Linux)
        } else {
            None
        }
    }
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The LaunchAgent for `binary` serving the state folder `state`.
#[must_use]
pub fn launch_agent(binary: &Path, state: &Path) -> String {
    let b = xml(&binary.display().to_string());
    let s = xml(&state.display().to_string());
    let log = xml(&state.join("bridge.log").display().to_string());
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key>\n\t<string>{LABEL}</string>\n\
         \t<key>ProgramArguments</key>\n\t<array>\n\
         \t\t<string>{b}</string>\n\t\t<string>--state-dir</string>\n\t\t<string>{s}</string>\n\
         \t\t<string>serve</string>\n\t</array>\n\
         \t<key>RunAtLoad</key>\n\t<true/>\n\
         \t<key>KeepAlive</key>\n\t<dict>\n\t\t<key>SuccessfulExit</key>\n\t\t<false/>\n\t</dict>\n\
         \t<key>ProcessType</key>\n\t<string>Background</string>\n\
         \t<key>StandardErrorPath</key>\n\t<string>{log}</string>\n\
         </dict>\n\
         </plist>\n"
    )
}

/// One argument of a desktop entry's `Exec` (quoted, `" ` $ \` escaped inside the quotes).
fn exec_arg(arg: &str) -> String {
    let mut out = String::from("\"");
    for c in arg.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// The XDG autostart entry for `binary` serving `state`.
#[must_use]
pub fn desktop_entry(binary: &Path, state: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Azlin Bridge\n\
         Comment=The Azlin drive's mail and files for other programs, on 127.0.0.1\n\
         Exec={} --state-dir {} serve\n\
         Terminal=false\n\
         NoDisplay=true\n\
         X-GNOME-Autostart-enabled=true\n",
        exec_arg(&binary.display().to_string()),
        exec_arg(&state.display().to_string())
    )
}

/// The Startup script for `binary` serving `state`: the bridge without a console window.
#[must_use]
pub fn startup_script(binary: &Path, state: &Path) -> String {
    // In a VBScript string a quote is written twice.
    let quoted = |path: &Path| format!("\"\"{}\"\"", path.display().to_string().replace('"', ""));
    format!(
        "' Starts the Azlin Bridge at login without a window (azul-bridge autostart).\r\n\
         Set shell = CreateObject(\"WScript.Shell\")\r\n\
         shell.Run \"{} --state-dir {} serve\", 0, False\r\n",
        quoted(binary),
        quoted(state)
    )
}

/// Where the login item of `system` goes: `home` is the user's home folder, `config` the OS
/// config folder (`~/.config` on Linux, the roaming app data on Windows).
#[must_use]
pub fn item_path(system: System, home: &Path, config: &Path) -> PathBuf {
    match system {
        System::MacOs => home
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{LABEL}.plist")),
        System::Linux => config.join("autostart").join("azul-bridge.desktop"),
        System::Windows => config
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup")
            .join("AzlinBridge.vbs"),
    }
}

/// The login item's text for `system`.
#[must_use]
pub fn item_text(system: System, binary: &Path, state: &Path) -> String {
    match system {
        System::MacOs => launch_agent(binary, state),
        System::Linux => desktop_entry(binary, state),
        System::Windows => startup_script(binary, state),
    }
}

/// Writes the login item; returns where it is.
///
/// # Errors
///
/// When it cannot be written.
pub fn enable(system: System, home: &Path, config: &Path, binary: &Path, state: &Path) -> std::io::Result<PathBuf> {
    let path = item_path(system, home, config);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, item_text(system, binary, state))?;
    Ok(path)
}

/// Removes the login item (none there is no error); whether there was one.
///
/// # Errors
///
/// When it cannot be removed.
pub fn disable(system: System, home: &Path, config: &Path) -> std::io::Result<bool> {
    match std::fs::remove_file(item_path(system, home, config)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// Whether the login item is there.
#[must_use]
pub fn is_enabled(system: System, home: &Path, config: &Path) -> bool {
    item_path(system, home, config).is_file()
}

#[cfg(test)]
mod tests {
    use azul_storage::testing::TempDir;

    use super::*;

    #[test]
    fn the_launch_agent_runs_serve_at_login_and_escapes_its_paths() {
        let text = launch_agent(Path::new("/Apps/A & B/azul-bridge"), Path::new("/Users/x/.bridge"));
        assert!(text.contains("<string>io.azlin.bridge</string>"), "{text}");
        assert!(text.contains("<string>/Apps/A &amp; B/azul-bridge</string>"), "{text}");
        assert!(text.contains("<string>--state-dir</string>\n\t\t<string>/Users/x/.bridge</string>"), "{text}");
        assert!(text.contains("<key>RunAtLoad</key>\n\t<true/>"), "{text}");
        assert!(text.contains("<string>/Users/x/.bridge/bridge.log</string>"), "{text}");
    }

    #[test]
    fn the_desktop_entry_quotes_its_arguments() {
        let text = desktop_entry(Path::new("/opt/my apps/azul-bridge"), Path::new("/home/x/$state"));
        assert!(
            text.contains("Exec=\"/opt/my apps/azul-bridge\" --state-dir \"/home/x/\\$state\" serve\n"),
            "{text}"
        );
        assert!(text.starts_with("[Desktop Entry]\n") && text.contains("NoDisplay=true"));
    }

    #[test]
    fn the_startup_script_runs_the_bridge_without_a_window() {
        let text = startup_script(
            Path::new("C:\\Program Files\\Azlin\\azul-bridge.exe"),
            Path::new("C:\\Users\\x\\AppData\\Roaming\\azul-bridge"),
        );
        assert!(
            text.contains("shell.Run \"\"\"C:\\Program Files\\Azlin\\azul-bridge.exe\"\" --state-dir \"\"C:\\Users\\x\\AppData\\Roaming\\azul-bridge\"\" serve\", 0, False"),
            "{text}"
        );
    }

    #[test]
    fn enable_writes_the_item_where_the_system_looks_and_disable_removes_it() {
        let dir = TempDir::new("bridge-autostart");
        let home = dir.0.join("home");
        let config = dir.0.join("config");
        for system in [System::MacOs, System::Linux, System::Windows] {
            assert!(!is_enabled(system, &home, &config));
            let path = enable(system, &home, &config, Path::new("/bin/azul-bridge"), Path::new("/s")).unwrap();
            assert!(path.is_file() && is_enabled(system, &home, &config), "{path:?}");
            assert!(disable(system, &home, &config).unwrap());
            assert!(!disable(system, &home, &config).unwrap());
        }
        assert_eq!(
            item_path(System::Linux, &home, &config),
            config.join("autostart").join("azul-bridge.desktop")
        );
        assert!(item_path(System::MacOs, &home, &config).ends_with("Library/LaunchAgents/io.azlin.bridge.plist"));
    }
}
