//! Gate: the process environment is read ONCE, at the start, in one place; everything else gets
//! typed settings handed down (the env pass of 2026-11-23, ledger `env-ledger.md`).
//!
//! The library crates (azul-css, azul-core, azul-layout, webrender) never call `std::env`: each
//! defines typed settings with a pure `from_lookup(&dyn azul_css::env::EnvSource)` and names its
//! variables in ONE place (its settings module). The one process reader is `dll/src/env.rs`
//! (`ProcessEnv`), and a tool's `main` for the doc crate. This gate ratchets that: every
//! production file that reads, writes or names the environment is listed with an exact count that
//! only goes down - a new site is a failure, and so is a count that is higher than the code's.
//! A cluster that wires a file lowers its count (to zero: the line is deleted); the end state is
//! the short list of the reader, the platform facades and the process-level proofs.
//!
//! Six rules, found in shipped code (comments and `#[cfg(test)]` items are not read):
//!
//! - `env-read`: `env::var`, `env::var_os`, `env::vars`, `env::vars_os` as a call or as a
//!   function value (`.map(std::env::var)`), and `use std::env::{.., var_os, ..}` imports. A
//!   parameter or closure named `var` is no read, so the pure constructors
//!   (`from_vars(var: impl Fn(&str) -> Option<String>)`) pass.
//! - `env-write`: `env::set_var` and `env::remove_var`.
//! - `env-flag`: a use of the `env_flag!` macro (one hidden `static OnceLock` each) and its
//!   definition.
//! - `env-dir`: `env::temp_dir`, `env::home_dir` and `dirs::*`: platform lookups that read
//!   `HOME`, `XDG_*` or `TMPDIR` inside std or the `dirs` crate. They stay behind facades
//!   (`layout/src/file.rs`), each with a pure twin that takes the values.
//! - `process-env`: `ProcessEnv::new()` and `ProcessEnv::default()`, a reader of the live process
//!   environment made where a question is asked. The app makes ONE at its start
//!   (`App::create`); every other construction is a place that reads the process again, past the
//!   `env-read` rule (which sees only the one reader's two calls), so each is listed.
//! - `az-name`: a string literal that IS a variable's name (`AZ_*`, `AZUL_*`, `TEXTDBG`, the
//!   platform names in [`PLATFORM_NAMES`]) outside a settings file. A settings file is
//!   `settings.rs`, `*_settings.rs` or a file under a `settings/` directory: the one place of a
//!   crate that knows which variable means what.
//!
//! The allowances are grouped by the work cluster of the ledger (section 4) that owns the files,
//! so two clusters never edit the same lines: `W0` css, core and webrender; `W1` solver3 and text3;
//! `W2` window, render and widgets; `W3` e2e, telemetry and services; `W4` the dll's app start and
//! shells; `W5` the dll's Linux shell and extras; `W6` the tools (doc, the web-lift transpiler);
//! `W7` the facades and process-level proofs that stay. To read what a file holds today:
//!
//! ```text
//! cargo test -p azul-srcgate --test env_gate -- --ignored --nocapture print_reads
//! ```
//!
//! CI runs the whole package (`cargo test -p azul-srcgate --no-fail-fast`, see `ci_gate.rs`).

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use azul_srcgate::{
    files, hits, locate, self_test, strings, workspace, Allow, Budget, Hit, Located, Orphans, Root,
    Rule,
};

static ROOTS: &[Root] = &[
    Root {
        dir: "core/src",
        min_files: 20,
    },
    Root {
        dir: "css/src",
        min_files: 5,
    },
    Root {
        dir: "layout/src",
        min_files: 50,
    },
    Root {
        dir: "dll/src",
        min_files: 100,
    },
    Root {
        dir: "doc/src",
        min_files: 5,
    },
    Root {
        dir: "webrender/core/src",
        min_files: 40,
    },
];

const READ: &str = "env-read";
const WRITE: &str = "env-write";
const FLAG: &str = "env-flag";
const DIR: &str = "env-dir";
const NAME: &str = "az-name";
const MAKER: &str = "process-env";

/// The names of the variables that are not `AZ_*` / `AZUL_*` but are read by the engine, the
/// tools or the platform layers (ledger section 2.4).
static PLATFORM_NAMES: &[&str] = &[
    "ANDROID_AVD_HOME",
    "ANDROID_NDK_HOME",
    "ANTHROPIC_API_KEY",
    "APPDATA",
    "CARGO_TARGET_DIR",
    "CHROME",
    "CLAUDECODE",
    "DBUS_SESSION_BUS_ADDRESS",
    "DESKTOP_SESSION",
    "DISPLAY",
    "FLATPAK_ID",
    "FONTCONFIG_FILE",
    "FONTCONFIG_PATH",
    "GNOME_DESKTOP_SESSION_ID",
    "GTK_CONFIG_HOME",
    "HOME",
    "HYPRLAND_INSTANCE_SIGNATURE",
    "I3SOCK",
    "JAVA_HOME",
    "KDE_FULL_SESSION",
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    "LC_CTYPE",
    "LC_MESSAGES",
    "LLC",
    "LLVM_LINK",
    "LLVM_OPT",
    "LOCALAPPDATA",
    "NO_COLOR",
    "PATH",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "REMILL_LIFT_BIN",
    "RUST_BACKTRACE",
    "SNAP",
    "SWAYSOCK",
    "TELEGRAM_BOT_TOKEN",
    "TELEGRAM_CHAT_ID",
    "TEMP",
    "TEXTDBG",
    "TMPDIR",
    "USERPROFILE",
    "WASM_LD",
    "WASM_OPT",
    "WAYLAND_DISPLAY",
    "WAYLAND_DISPLAY_HEIGHT",
    "WAYLAND_DISPLAY_WIDTH",
    "WR_TRACY_PATH",
    "XCURSOR_SIZE",
    "XCURSOR_THEME",
    "XDG_CACHE_HOME",
    "XDG_CONFIG_DIRS",
    "XDG_CONFIG_HOME",
    "XDG_CURRENT_DESKTOP",
    "XDG_DATA_DIRS",
    "XDG_DATA_HOME",
    "XDG_RUNTIME_DIR",
    "XDG_SESSION_TYPE",
];

// ---------------------------------------------------------------------------------------------
// The finders
// ---------------------------------------------------------------------------------------------

/// Whether the tokens before index `i` are `env ::`.
fn after_env(t: &[Located], i: usize) -> bool {
    i >= 3
        && t[i - 1].tok.is_punct(':')
        && t[i - 2].tok.is_punct(':')
        && t[i - 3].tok.is_ident("env")
}

/// Every use of a member of the module `env` that is one of `names`: `env::name` (a call, a
/// function value or an import) and the names inside `env::{.., name, ..}`.
fn env_members(t: &[Located], names: &[&str]) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        if x.tok.ident().is_some_and(|n| names.contains(&n)) && after_env(t, i) {
            found.push(x.line);
        }
        // `env::{a, b as c, d}`: the names at the top level of the group
        if x.tok.is_punct('{') && after_env(t, i) {
            let mut depth = 1;
            let mut j = i + 1;
            while j < t.len() && depth > 0 {
                if t[j].tok.is_punct('{') {
                    depth += 1;
                } else if t[j].tok.is_punct('}') {
                    depth -= 1;
                } else if depth == 1 && t[j].tok.ident().is_some_and(|n| names.contains(&n)) {
                    // not the alias after `as`
                    let aliased = j >= 1 && t[j - 1].tok.is_ident("as");
                    if !aliased {
                        found.push(t[j].line);
                    }
                }
                j += 1;
            }
        }
    }
    found
}

fn env_reads(t: &[Located]) -> Vec<usize> {
    env_members(t, &["var", "var_os", "vars", "vars_os"])
}

fn env_writes(t: &[Located]) -> Vec<usize> {
    env_members(t, &["set_var", "remove_var"])
}

/// A use of `env_flag!` or its definition `macro_rules! env_flag`.
fn env_flags(t: &[Located]) -> Vec<usize> {
    let mut found = azul_srcgate::macro_calls(t, "env_flag");
    for (i, x) in t.iter().enumerate() {
        if x.tok.is_ident("macro_rules")
            && t.get(i + 1).is_some_and(|n| n.tok.is_punct('!'))
            && t.get(i + 2).is_some_and(|n| n.tok.is_ident("env_flag"))
        {
            found.push(x.line);
        }
    }
    found
}

/// `env::temp_dir`, `env::home_dir` and every member of the `dirs` crate: `dirs::config_dir` and
/// the names inside `dirs::{..}`.
fn env_dirs(t: &[Located]) -> Vec<usize> {
    let mut found = env_members(t, &["temp_dir", "home_dir"]);
    for (i, x) in t.iter().enumerate() {
        let after_dirs = i >= 3
            && t[i - 1].tok.is_punct(':')
            && t[i - 2].tok.is_punct(':')
            && t[i - 3].tok.is_ident("dirs");
        if !after_dirs {
            continue;
        }
        if x.tok.ident().is_some() {
            found.push(x.line);
        } else if x.tok.is_punct('{') {
            let mut depth = 1;
            let mut j = i + 1;
            while j < t.len() && depth > 0 {
                if t[j].tok.is_punct('{') {
                    depth += 1;
                } else if t[j].tok.is_punct('}') {
                    depth -= 1;
                } else if depth == 1
                    && t[j].tok.ident().is_some()
                    && !t[j].tok.is_ident("as")
                    && !(j >= 1 && t[j - 1].tok.is_ident("as"))
                {
                    found.push(t[j].line);
                }
                j += 1;
            }
        }
    }
    found
}

/// Whether `s` is the name of an environment variable: `AZ_*` / `AZUL_*` in capitals, or one of
/// the platform names.
fn is_variable_name(s: &str) -> bool {
    let shouty = |s: &str| {
        s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    };
    ((s.starts_with("AZ_") || s.starts_with("AZUL_")) && s.len() > 3 && shouty(s))
        || PLATFORM_NAMES.contains(&s)
}

fn env_names(t: &[Located]) -> Vec<usize> {
    strings(t, is_variable_name)
}

/// `ProcessEnv::new()` and `ProcessEnv::default()`, behind any path (`crate::env::ProcessEnv::new`):
/// a reader of the live process environment made where it is asked. `Self::new()`, `MapEnv::new()`
/// and the definition of the type are no construction of the process reader.
fn process_env_makers(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        if x.tok.is_ident("ProcessEnv")
            && t.get(i + 1).is_some_and(|n| n.tok.is_punct(':'))
            && t.get(i + 2).is_some_and(|n| n.tok.is_punct(':'))
            && t
                .get(i + 3)
                .is_some_and(|n| n.tok.is_ident("new") || n.tok.is_ident("default"))
        {
            found.push(x.line);
        }
    }
    found
}

/// A settings file: the one place of a crate that names its variables.
fn is_settings_file(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name == "settings.rs" || name.ends_with("_settings.rs") || rel.contains("/settings/")
}

fn rules() -> Vec<Rule> {
    vec![
        Rule::new(
            READ,
            env_reads,
            &[
                "let a = std::env::var(\"X\");",
                "let b = std::env::var_os(\"X\");",
                "let c = std::env::vars();",
                "let d = std::env::vars_os();",
                "let e = [\"a\"].iter().map(std::env::var);",
                "use std::env::var;",
                "use std::env::{args, var_os};",
                "use std::env::{args, var_os as get};",
            ],
            &[
                "let v = env.var(\"X\");",
                "let var = |n: &str| None::<String>; var(\"X\");",
                "let a = std::env::args();",
                "let v = map.var_os(\"X\");",
                "let e = std::env::VarError::NotPresent;",
                "use std::env::{args, current_dir};",
                "use std::env::{args as var};",
                "let n = env!(\"CARGO_PKG_NAME\");",
                "let v = azul_css::env::NoEnv.var(\"X\");",
            ],
        ),
        Rule::new(
            WRITE,
            env_writes,
            &[
                "unsafe { std::env::set_var(\"X\", \"1\"); }",
                "unsafe { std::env::remove_var(\"X\"); }",
                "let f = std::env::set_var;",
                "use std::env::{remove_var, args};",
            ],
            &[
                "let mut m = Map::new(); m.set_var(\"X\", \"1\");",
                "custom::set_var(\"X\");",
                "fn set_var(name: &str) {}",
            ],
        ),
        Rule::new(
            FLAG,
            env_flags,
            &[
                "let on = env_flag!(\"AZ_X\");",
                "macro_rules! env_flag { () => {}; }",
            ],
            &["let env_flag = 1;", "let on = other_flag!(\"AZ_X\");"],
        ),
        Rule::new(
            DIR,
            env_dirs,
            &[
                "let t = std::env::temp_dir();",
                "let h = std::env::home_dir();",
                "let c = dirs::config_dir();",
                "let c = Some(1).or_else(dirs::cache_dir);",
                "use dirs::{config_dir, home_dir};",
                "use std::env::{temp_dir};",
            ],
            &[
                "let t = scratch.temp_dir();",
                "let d = std::env::current_dir();",
                "let c = std::env::current_exe();",
                "let dirs = vec![1]; dirs.len();",
                "use dirs;",
            ],
        ),
        Rule::new(
            NAME,
            env_names,
            &[
                "let a = std::env::var(\"AZ_BACKEND\");",
                "const NAME: &str = \"AZUL_FILE_PICKER\";",
                "let h = \"HOME\";",
                "let t = \"TEXTDBG\";",
                "let w = \"WAYLAND_DISPLAY\";",
            ],
            &[
                "let a = \"AZ_\";",
                "let m = \"AZ_BACKEND=headless\";",
                "let n = \"AzString\";",
                "let l = \"AZ_lowercase\";",
                "let h = \"HOMES\";",
                "let a = AZ_BACKEND;",
            ],
        ),
        Rule::new(
            MAKER,
            process_env_makers,
            &[
                "let e = ProcessEnv::new();",
                "let e = crate::env::ProcessEnv::new();",
                "let e = &azul_dll::env::ProcessEnv::default();",
                "Self::from_lookup(&crate::env::ProcessEnv::new())",
            ],
            &[
                "let e = Self::new();",
                "let e = MapEnv::new();",
                "let e = ProcessEnvironment::new();",
                "impl ProcessEnv { pub fn new() -> Self { Self {} } }",
                "let e = ProcessEnv;",
                "let n = ProcessEnv::NAME;",
            ],
        ),
    ]
}

// ---------------------------------------------------------------------------------------------
// The allowances: today's counts, by the cluster that wires the file. Lower a count when a file
// is wired; delete the line at zero. `Allow::exactly`: a new site, a stale count and an unused
// allowance all fail.
// ---------------------------------------------------------------------------------------------

/// W0: css, core, webrender (the shared seam, `AppConfig::create`, the log gate, the test hooks).
static W0: &[Allow] = &[
    Allow::exactly(
        "core/src/run_mode.rs",
        NAME,
        11,
        "TestHooks::from_env: the one place that knows which variable of the test run means what \
         (keyring_gate, RF38); a settings file of its own would be the same list",
    ),
    Allow::exactly(
        "css/src/codegen/lang/cobol.rs",
        NAME,
        1,
        "not an environment variable: DISPLAY is a reserved word of COBOL in the keyword table \
         of the code generator",
    ),
    Allow::exactly(
        "css/src/rice.rs",
        NAME,
        4,
        "RiceEnv::from_lookup: the home directories the rice root and the legacy stylesheet are \
         looked for under (HOME, USERPROFILE, APPDATA, XDG_CONFIG_HOME)",
    ),
];
/// W1: layout solver3, text3, glyph_cache, lib.rs: nothing left.
///
/// Every site of solver3, text3 and glyph_cache is wired (the settings are in
/// `layout/src/settings/{solver,text}.rs`), and the `env_flag!` macro of lib.rs went with its
/// last use (cluster W2: window.rs x3, widgets/node_graph.rs x1).
const W1: &[Allow] = &[];
/// W2: layout window, cpu_backend, cpurender, managers, widgets, scroll_timer, callbacks.
static W2: &[Allow] = &[
];
/// W3: layout e2e, telemetry, request, probe, updater, icon_remap, file_picker, file, dialogs.
static W3: &[Allow] = &[
    Allow::exactly(
        "layout/src/desktop/file_picker.rs",
        DIR,
        1,
        "registered_file: where AzDrive registers its picker is where `dirs` says the config folder is, a platform lookup of the OS (no variable of ours)",
    ),
    Allow::exactly(
        "layout/src/dialogs/report_problem.rs",
        DIR,
        1,
        "report_dir: the disk fallback of a problem report in a build without the updater is the system's temp folder (`env::temp_dir`)",
    ),
    Allow::exactly(
        "layout/src/file.rs",
        DIR,
        20,
        "FilePath::get_*_dir, get_temp_dir and get_current_dir are the C-ABI functions that ask the OS (`dirs`, `std::env`): no context, the platform facade of the crate",
    ),
    Allow::exactly(
        "layout/src/telemetry/mod.rs",
        DIR,
        1,
        "the panic hook writes the crash dump to the system's temp folder (`env::temp_dir`) for the reporter process it respawns",
    ),
    Allow::exactly(
        "layout/src/tray_icon.rs",
        NAME,
        1,
        "AZ_TRAY_ICON_DUMP: render_icon_to_rgba is called from the dll's platform tray code (tray/*, app_icon.rs), which has no settings carrier yet; W4 follows",
    ),
    Allow::exactly(
        "layout/src/tray_icon.rs",
        READ,
        1,
        "AZ_TRAY_ICON_DUMP: render_icon_to_rgba is called from the dll's platform tray code (tray/*, app_icon.rs), which has no settings carrier yet; W4 follows",
    ),
    Allow::exactly(
        "layout/src/updater.rs",
        NAME,
        2,
        "the update facade: InstallKind::detect and detect_for (SNAP) and draw_rollout_bucket (AZ_UPDATE_BUCKET) are the context-free functions of the C API (api.json InstallKind and ReleaseInfo); the app's own update flows take both from AppEnv",
    ),
    Allow::exactly(
        "layout/src/updater.rs",
        DIR,
        1,
        "default_state_dir: the update state falls back to the system's temp folder (`env::temp_dir`) where the user's data folder is not set",
    ),
    Allow::exactly(
        "layout/src/updater.rs",
        READ,
        2,
        "the update facade: InstallKind::detect and detect_for (SNAP) and draw_rollout_bucket (AZ_UPDATE_BUCKET) are the context-free functions of the C API (api.json InstallKind and ReleaseInfo); the app's own update flows take both from AppEnv",
    ),
];
/// W4: dll app start, compositor, wr_translate, shell2 common, macos, windows, android, headless.
static W4: &[Allow] = &[
    Allow::exactly(
        "dll/src/desktop/app_identity.rs",
        NAME,
        1,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/app_identity.rs",
        READ,
        1,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/compositor2.rs",
        NAME,
        3,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/compositor2.rs",
        READ,
        3,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/display.rs",
        NAME,
        5,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/display.rs",
        READ,
        5,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/global_hotkey/mod.rs",
        NAME,
        1,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/global_hotkey/mod.rs",
        READ,
        1,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/global_hotkey/x11.rs",
        NAME,
        1,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/global_hotkey/x11.rs",
        READ,
        1,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/native_screenshot.rs",
        NAME,
        1,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/native_screenshot.rs",
        READ,
        1,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/notifications/linux.rs",
        NAME,
        1,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/notifications/linux.rs",
        READ,
        1,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/shader_cache.rs",
        NAME,
        4,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/shader_cache.rs",
        READ,
        4,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/common/clipboard.rs",
        NAME,
        2,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/common/clipboard.rs",
        READ,
        2,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/common/debug_server/stub.rs",
        NAME,
        1,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/common/debug_server/stub.rs",
        READ,
        1,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/headless/mod.rs",
        NAME,
        1,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/headless/mod.rs",
        READ,
        1,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/run.rs",
        NAME,
        1,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/windows/system_style.rs",
        NAME,
        2,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/windows/system_style.rs",
        READ,
        2,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/windows/webview.rs",
        NAME,
        2,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/shell2/windows/webview.rs",
        READ,
        2,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/wallpaper.rs",
        NAME,
        2,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/wallpaper.rs",
        READ,
        2,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/wr_translate2.rs",
        NAME,
        5,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/wr_translate2.rs",
        READ,
        3,
        "dll app start and shells: wired by cluster W4",
    ),
    Allow::exactly(
        "dll/src/desktop/compositor2.rs",
        MAKER,
        1,
        "dll app start and shells: wired by cluster W4 (show_hit_test_overlay builds a reader of \
         its own, cached in a static)",
    ),
    Allow::exactly(
        "dll/src/desktop/wr_translate2.rs",
        MAKER,
        2,
        "dll app start and shells: wired by cluster W4 (the tracy path of the renderer options and \
         merge_overlay_env, cached in a static, each build a reader of their own)",
    ),
];
/// W5: dll linux shell and extras. Everything is wired through `PlatformEnv`, `LinuxSettings`
/// and `ExtraSettings` (settings files, one place that names the variables) except the OS
/// facade below.
static W5: &[Allow] = &[Allow::exactly(
    "dll/src/desktop/extra/sqlite/mod.rs",
    DIR,
    1,
    "platform facade: the OS temp directory (`std::env::temp_dir`: TMPDIR on Unix, \
     GetTempPath on Windows) is the last fallback of the local database's folder; it has no \
     snapshot twin because the OS decides it",
)];
/// W6: tools: doc and the web-lift transpiler (`dll/src/web`).
static W6: &[Allow] = &[
    Allow::exactly(
        "dll/src/web/classify.rs",
        NAME,
        1,
        "not a read of the process: AZ_CODEGEN_DIR_RESOLVED is the compile-time env! (set by \
         dll/build.rs) that names the directory of the generated bindings",
    ),
    Allow::exactly(
        "dll/src/web/lift_settings.rs",
        DIR,
        1,
        "the web-lift reader of the platform: the temp dir, asked once by run_web into \
         WebLiftSettings::temp_dir (the variables go through azul_dll::env::ProcessEnv)",
    ),
    Allow::exactly(
        "doc/src/codegen/v2/abi_guard.rs",
        NAME,
        1,
        "not an environment variable: AZ_ABI_HASH is a constant of the Rust the generator writes",
    ),
    Allow::exactly(
        "doc/src/codegen/v2/lang_cobol/mod.rs",
        NAME,
        1,
        "not an environment variable: DISPLAY is a reserved word in the COBOL keyword list",
    ),
    Allow::exactly(
        "doc/src/dllgen/deploy.rs",
        NAME,
        1,
        "not an environment variable: AZ_REFLECT_JSON is a C++ macro the generated examples must \
         not carry",
    ),
    Allow::exactly(
        "doc/src/process_env.rs",
        DIR,
        1,
        "the doc tool's reader of the process: the platform's temp dir, asked once by `main` into \
         ToolSettings::temp_dir",
    ),
    Allow::exactly(
        "doc/src/process_env.rs",
        READ,
        2,
        "the doc tool's reader of the process (var and var_os), read once by `main`; azul-doc \
         does not link azul-dll, so it has its own beside azul_dll::env",
    ),
    Allow::exactly(
        "doc/src/reftest/pipeline.rs",
        WRITE,
        1,
        "FONTCONFIG_FILE feeds fontconfig and rust-fontconfig, third-party readers of that \
         variable (Chrome reads it at spawn, the engine's alias parser on its first font stack)",
    ),
];
/// W7: what stays for good: the reader, the facades, the process-level proofs.
static W7: &[Allow] = &[
    Allow::exactly(
        "dll/src/env.rs",
        READ,
        2,
        "ProcessEnv: the one reader of the process environment (var and var_os); the rest of the \
         workspace asks an EnvSource",
    ),
    Allow::exactly(
        "dll/src/desktop/app.rs",
        MAKER,
        1,
        "App::create: the ONE reader of an app, made at its start; every setting of the dll and of \
         the library crates is built from it",
    ),
    Allow::exactly(
        "dll/src/env.rs",
        MAKER,
        1,
        "app_config_create: the C API's AppConfig::create runs before App::create and makes a \
         reader of its own, for the default text anti-aliasing",
    ),
    Allow::exactly(
        "dll/src/desktop/extra/extra_settings.rs",
        MAKER,
        1,
        "ExtraSettings::of_process: the entry point of a service behind a public C-ABI function \
         that a host calls with no app in reach (a stand-in device, the screen capture)",
    ),
    Allow::exactly(
        "dll/src/desktop/platform_settings.rs",
        MAKER,
        1,
        "PlatformEnv::of_process: the same, for the platform lookups of such a service entry point",
    ),
    Allow::exactly(
        "dll/src/desktop/notifications/mod.rs",
        MAKER,
        1,
        "headless_asked_of_the_process: PlatformCapability::notifications() probed BEFORE \
         App::create has read the environment (AzClock builds its state first)",
    ),
    Allow::exactly(
        "dll/src/web/lift_settings.rs",
        MAKER,
        1,
        "WebLiftSettings::from_process: the one reader of a web-lift run (tool code, made once at \
         the entry of run_web)",
    ),
    // The codegen directory is a COMPILE-TIME value (`env!`, set by dll/build.rs): not a read of
    // the process environment, so it is no setting. The literal stays where the `include!` is.
    Allow::exactly(
        "dll/src/lib.rs",
        NAME,
        4,
        "compile-time env!(\"AZ_CODEGEN_DIR_RESOLVED\") of the generated bindings (build.rs), not a \
         run-time read",
    ),
    Allow::exactly(
        "dll/src/php_extension.rs",
        NAME,
        1,
        "compile-time env!(\"AZ_CODEGEN_DIR_RESOLVED\") of the generated bindings (build.rs), not a \
         run-time read",
    ),
    Allow::exactly(
        "dll/src/desktop/material_icons.rs",
        NAME,
        1,
        "compile-time env!(\"AZ_CODEGEN_DIR_RESOLVED\") of the generated bindings (build.rs), not a \
         run-time read",
    ),
    // The platform's temporary directory is a lookup the OS owns (TMPDIR, GetTempPath, the
    // per-user directory of macOS): a facade, as `layout/src/file.rs` is for the user folders.
    Allow::exactly(
        "dll/src/e2e_web_runner.rs",
        DIR,
        1,
        "std::env::temp_dir: where the embedded web harness is extracted (the platform's temporary \
         directory, which the OS owns)",
    ),
    Allow::exactly(
        "dll/src/desktop/notifications/apple.rs",
        DIR,
        1,
        "std::env::temp_dir: the platform's temporary directory, which the OS owns (the copy of \
         the notification attachment)",
    ),
];

fn allowed() -> Vec<Allow> {
    [W0, W1, W2, W3, W4, W5, W6, W7].concat()
}

// ---------------------------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------------------------

/// Everything wrong with the tree below `base` under `allowed`: the rules' own cases, the files
/// (every compiled file under the roots), the hits (a name in a settings file is no hit) and the
/// judgement of the budget.
fn check(base: &Path, roots: &[Root], allowed: &[Allow]) -> Result<Vec<Hit>, Vec<String>> {
    let rules = rules();
    self_test(&rules);
    let read = files(base, roots, Orphans::Ignore)?;
    let mut found = hits(&read, &rules);
    found.retain(|h| !(h.rule == NAME && is_settings_file(&h.rel)));
    let wrong = Budget(allowed).judge(&found);
    if wrong.is_empty() {
        Ok(found)
    } else {
        Err(wrong)
    }
}

#[test]
fn the_environment_is_read_only_where_the_list_says() {
    if let Err(wrong) = check(&workspace(), ROOTS, &allowed()) {
        panic!(
            "the environment is read ONCE in dll/src/env.rs and the values are handed down \
             (env-ledger.md): {} finding(s):\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }
}

/// The count of a rule in a file only goes down: no allowance may say more than the code holds
/// (the budget already fails on a different count; this names the direction for a reader).
#[test]
fn every_allowance_names_its_reason() {
    for a in allowed() {
        assert!(a.count.is_some(), "{}: the count is exact", a.file);
        assert!(a.why.len() > 10, "{}: say why", a.file);
        assert!(
            workspace().join(a.file).is_file(),
            "{}: the file is gone, delete the allowance",
            a.file
        );
    }
}

/// The cluster of the ledger that owns a file.
fn cluster_of(rel: &str) -> &'static str {
    let starts = |p: &str| rel.starts_with(p);
    if starts("css/src/") || starts("core/src/") || starts("webrender/") {
        "W0"
    } else if starts("layout/src/solver3/")
        || starts("layout/src/text3/")
        || rel == "layout/src/glyph_cache.rs"
        || rel == "layout/src/lib.rs"
    {
        "W1"
    } else if starts("layout/src/e2e/")
        || starts("layout/src/telemetry/")
        || starts("layout/src/dialogs/")
        || [
            "request.rs",
            "probe.rs",
            "updater.rs",
            "icon_remap.rs",
            "tray_icon.rs",
            "dom_lint.rs",
            "file.rs",
        ]
        .iter()
        .any(|f| rel == format!("layout/src/{f}"))
        || rel == "layout/src/desktop/file_picker.rs"
    {
        "W3"
    } else if starts("layout/src/") {
        "W2"
    } else if starts("dll/src/web/") || starts("doc/src/") {
        "W6"
    } else if starts("dll/src/desktop/shell2/linux/") || starts("dll/src/desktop/extra/") {
        "W5"
    } else {
        "W4"
    }
}

/// Prints every site as an allowance line, by cluster: `cargo test -p azul-srcgate --test
/// env_gate -- --ignored --nocapture print_reads`, to write the lists above.
#[test]
#[ignore = "ENV-1: prints the reads; the lists above are written from it"]
fn print_reads() {
    let read = files(&workspace(), ROOTS, Orphans::Ignore).unwrap_or_else(|w| panic!("{w:#?}"));
    let mut found = hits(&read, &rules());
    found.retain(|h| !(h.rule == NAME && is_settings_file(&h.rel)));
    let mut per: BTreeMap<(&str, &str, &str), usize> = BTreeMap::new();
    for h in &found {
        *per.entry((cluster_of(&h.rel), h.rel.as_str(), h.rule.as_str()))
            .or_default() += 1;
    }
    for ((cluster, file, rule), n) in per {
        println!("ALLOW {cluster} {file} {rule} {n}");
    }
}

// ---------------------------------------------------------------------------------------------
// The gate's own tests: a planted tree
// ---------------------------------------------------------------------------------------------

/// A tree on disk below a fresh base: (path below the base, text).
fn base_with(name: &str, tree: &[(&str, &str)]) -> PathBuf {
    let base = std::env::temp_dir().join(format!("azul-srcgate-env-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    for (path, text) in tree {
        let full = base.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
    base
}

static SMALL: &[Root] = &[Root {
    dir: "core/src",
    min_files: 1,
}];

fn judged(base: &Path, allowed: &[Allow]) -> Result<usize, Vec<String>> {
    check(base, SMALL, allowed).map(|h| h.len())
}

#[test]
fn a_planted_read_is_caught_in_an_unlisted_file_counted_in_a_listed_one_and_goes_stale() {
    let planted = "pub fn f() -> bool {\n    std::env::var(\"X\").is_ok() || std::env::var_os(\"Y\").is_some()\n}\n";
    let base = base_with(
        "planted",
        &[("core/src/lib.rs", "mod a;\n"), ("core/src/a.rs", planted)],
    );
    let wrong = judged(&base, &[]).expect_err("an unlisted read is a finding");
    assert_eq!(wrong.len(), 2, "{wrong:?}");
    assert!(
        wrong.iter().any(|w| w.starts_with("core/src/a.rs:2")),
        "{wrong:?}"
    );
    let listed = [Allow::exactly("core/src/a.rs", READ, 2, "the planted copy")];
    assert_eq!(judged(&base, &listed), Ok(2));
    std::fs::write(
        base.join("core/src/a.rs"),
        format!("{planted}pub fn g() {{\n    unsafe {{ std::env::set_var(\"A\", \"1\"); }}\n}}\n"),
    )
    .unwrap();
    let wrong = judged(&base, &listed).expect_err("a write is a new site");
    assert!(wrong.iter().any(|w| w.contains("(env-write)")), "{wrong:?}");
    std::fs::write(base.join("core/src/a.rs"), "pub fn clean() {}\n").unwrap();
    let wrong = judged(&base, &listed).expect_err("a read that went away leaves a stale count");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 0 x env-read where its allowance says 2")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_name_in_a_settings_file_a_comment_or_a_test_is_no_finding() {
    let base = base_with(
        "names",
        &[
            ("core/src/lib.rs", "mod settings;\nmod b;\n"),
            (
                "core/src/settings.rs",
                "pub fn names(env: &dyn Fn(&str) -> Option<String>) {\n    env(\"AZ_BACKEND\");\n    env(\"HOME\");\n}\n",
            ),
            (
                "core/src/b.rs",
                "// AZ_BACKEND, HOME\n/// `AZ_LOG`\npub fn a() {\n    println!(\"AZ_BACKEND={}\", 1);\n}\n#[cfg(test)]\nmod tests {\n    fn t() {\n        std::env::set_var(\"AZ_KEYRING\", \"memory\");\n    }\n}\n",
            ),
        ],
    );
    assert_eq!(judged(&base, &[]), Ok(0));
    // a settings file reads no variable: the rule env-read still applies to it
    std::fs::write(
        base.join("core/src/settings.rs"),
        "pub fn f() { let _ = std::env::var(\"AZ_X\"); }\n",
    )
    .unwrap();
    let wrong = judged(&base, &[]).expect_err("a settings file does not read the process");
    assert_eq!(wrong.len(), 1, "{wrong:?}");
    assert!(wrong[0].contains("(env-read)"), "{wrong:?}");
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn the_rules_tell_the_five_kinds_apart() {
    let t = |src: &str| locate(&azul_srcgate::code(src));
    let src = "fn f() {\n    std::env::var(\"AZ_X\");\n    std::env::set_var(\"AZ_Y\", \"1\");\n    let _ = env_flag!(\"AZ_Z\");\n    let _ = dirs::home_dir();\n}\n";
    let toks = t(src);
    assert_eq!(env_reads(&toks), vec![2]);
    assert_eq!(env_writes(&toks), vec![3]);
    assert_eq!(env_flags(&toks), vec![4]);
    assert_eq!(env_dirs(&toks), vec![5]);
    assert_eq!(env_names(&toks), vec![2, 3, 4]);
}
