//! Logging initialization (via `fern`) and panic hook setup with backtrace formatting.

use core::sync::atomic::{AtomicBool, Ordering};

use log::LevelFilter;

/// Whether to show a message box to the user when a panic occurs.
pub static SHOULD_ENABLE_PANIC_HOOK: AtomicBool = AtomicBool::new(false);

/// Escape `<` and `>` so the panic dialog text renders as literal characters
/// in `tinyfiledialogs` on Linux, which interprets the body as Pango markup.
/// `&` is intentionally not escaped: doing so would double-escape any entity
/// references that are already present in the input.
#[cfg(any(target_os = "linux", test))]
fn escape_dialog_html(s: &str) -> String {
    s.replace('<', "&lt;").replace('>', "&gt;")
}

/// Configures the global logger using `fern` to write to stdout at the given level.
#[cfg(all(feature = "use_fern_logger", not(feature = "use_pyo3_logger")))]
pub fn set_up_logging(log_level: LevelFilter) {
    use std::error::Error;

    use fern::InitError;

    /// Sets up the global logger
    fn set_up_logging_internal(log_level: LevelFilter) -> Result<(), InitError> {
        fern::Dispatch::new()
            .format(|out, message, record| {
                out.finish(format_args!(
                    "[{}][{}] {}",
                    record.level(),
                    record.target(),
                    message
                ))
            })
            .level(log_level)
            .chain(::std::io::stdout())
            .apply()?;
        Ok(())
    }

    match set_up_logging_internal(log_level) {
        Ok(_) => {}
        Err(e) => match e {
            InitError::Io(e) => {
                println!(
                    "[WARN] Logging IO init error: \r\nkind: \
                     {:?}\r\n\r\ndescription:\r\n{}\r\n\r\ncause:\r\n{:?}\r\n",
                    e.kind(),
                    e,
                    e.source()
                );
            }
            InitError::SetLoggerError(_) => {}
        },
    }
}

/// In the (rare) case of a panic, print it to the stdout, log it to the file and
/// prompt the user with a message box.
pub fn set_up_panic_hooks() {
    use std::panic::{self, PanicInfo};

    use backtrace::{Backtrace, BacktraceFrame};

    fn panic_fn(panic_info: &PanicInfo) {
        use std::thread;

        let payload = panic_info.payload();
        let location = panic_info.location();

        let payload_str = format!("{:?}", payload);
        let panic_str = payload
            .downcast_ref::<String>()
            .map(|s| s.as_ref())
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or(payload_str.as_str());

        let location_str = location.map(|loc| format!("{} at line {}", loc.file(), loc.line()));
        let backtrace_str_old = format_backtrace(&Backtrace::new());
        let backtrace_str = backtrace_str_old
            .lines()
            .filter(|l| !l.is_empty())
            .collect::<Vec<&str>>()
            .join("\r\n");
        // let backtrace_str = "";
        let thread = thread::current();
        let thread_name = thread.name().unwrap_or("<unnamed thread>");

        let error_str = format!(
            "An unexpected panic occurred, the program has to exit.\r\nPlease report this error \
             and attach the log file found in the directory of the executable.\r\n\r\nThe error \
             occurred in: {} in thread {}\r\n\r\nError \
             information:\r\n{}\r\n\r\nBacktrace:\r\n\r\n{}\r\n",
            location_str.unwrap_or("<unknown location>".to_string()),
            thread_name,
            panic_str,
            backtrace_str
        );

        // TODO: invoke external app crash handler with the location to the log file
        log::error!("{}", error_str);

        if SHOULD_ENABLE_PANIC_HOOK.load(Ordering::SeqCst) {
            // tfd (tinyfiledialogs) is desktop-only; Android/iOS have no
            // equivalent modal API from a Rust dep. Log to stderr instead;
            // logcat/console will pick it up.
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                #[cfg(not(target_os = "linux"))]
                let dialog_str = &error_str;

                #[cfg(target_os = "linux")]
                let dialog_str = escape_dialog_html(&error_str);

                tfd::MessageBox::new("Unexpected fatal error", &dialog_str)
                    .with_icon(tfd::MessageBoxIcon::Info)
                    .run_modal();
            }
            #[cfg(any(target_os = "android", target_os = "ios"))]
            {
                eprintln!("[FATAL] {}", error_str);
            }
        }
    }

    fn format_backtrace(backtrace: &Backtrace) -> String {
        fn format_frame(frame: &BacktraceFrame) -> String {
            use std::ffi::OsStr;

            let ip = frame.ip();
            let symbols = frame.symbols();

            const UNRESOLVED_FN_STR: &str = "unresolved function";

            if symbols.is_empty() {
                return format!("{} @ {:?}", UNRESOLVED_FN_STR, ip);
            }

            symbols
                .iter()
                .map(|symbol| {
                    let mut nice_string = String::new();

                    if let Some(name) = symbol.name() {
                        let name_demangled = format!("{}", name);
                        let name_demangled_new = name_demangled
                            .rsplit("::")
                            .skip(1)
                            .map(|e| e.to_string())
                            .collect::<Vec<String>>();
                        let name_demangled = name_demangled_new
                            .into_iter()
                            .rev()
                            .collect::<Vec<String>>()
                            .join("::");
                        nice_string.push_str(&name_demangled);
                    } else {
                        nice_string.push_str(UNRESOLVED_FN_STR);
                    }

                    let mut file_string = String::new();
                    if let Some(file) = symbol.filename() {
                        let origin_file_name = file
                            .file_name()
                            .unwrap_or(OsStr::new("unresolved file name"))
                            .to_string_lossy();
                        file_string.push_str(&origin_file_name);
                    }

                    if let Some(line) = symbol.lineno() {
                        file_string.push_str(&format!(":{}", line));
                    }

                    if !file_string.is_empty() {
                        nice_string.push_str(" @ ");
                        nice_string.push_str(&file_string);
                        if !nice_string.ends_with("\n") {
                            nice_string.push('\n');
                        }
                    }

                    nice_string
                })
                .collect::<Vec<String>>()
                .join("")
        }

        backtrace
            .frames()
            .iter()
            .map(format_frame)
            .collect::<Vec<String>>()
            .join("\r\n")
    }

    panic::set_hook(Box::new(panic_fn));
}

// ============================================================================
// Built-in default logger (dependency-free; works in the lean `build-dll` DLL).
//
// The fern logger above is gated behind `use_fern_logger`, which the shipped
// `build-dll` library does NOT enable — so the released `.deb`/`.dylib`/`.dll`
// installs no logger and every `plog_*!` / `log_*!` / `log::*!` call is silently
// discarded. That is why a misbehaving app "just exits with no error".
//
// This installs a minimal `log::Log` that writes to stderr, controlled by the
// `AZ_LOG` environment variable and ON BY DEFAULT (we are in a testing phase —
// the goal is that NOTHING ever quits silently):
//
//   AZ_LOG unset / 1 / true / on / yes   -> ON at Debug   (the default)
//   AZ_LOG = trace|debug|info|warn|error -> ON at that level
//   AZ_LOG = 0 / off / false / none / no -> OFF (no logger installed)
//
// It never clobbers a logger the host already installed (pyo3-log under Python,
// android_logger on Android, env_logger in azul-self-test, a user's own logger).
// ============================================================================

/// Parse `AZ_LOG` into a max level filter. `None` means "logging disabled".
/// Unset (or any unrecognized truthy value) defaults to `Debug` — verbose but
/// not the per-frame `Trace` firehose; pass `AZ_LOG=trace` for everything.
pub fn az_log_level() -> Option<LevelFilter> {
    let raw = std::env::var("AZ_LOG").unwrap_or_default();
    match raw.trim().to_ascii_lowercase().as_str() {
        "0" | "off" | "false" | "none" | "no" | "disable" | "disabled" => None,
        "error" => Some(LevelFilter::Error),
        "warn" | "warning" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "trace" | "all" => Some(LevelFilter::Trace),
        // "", "1", "true", "on", "yes", "debug", or anything unknown -> Debug.
        _ => Some(LevelFilter::Debug),
    }
}

// Zero-field unit logger: `log` is built with default-features off, so the
// `std`-only `set_boxed_logger` is unavailable — we must hand `set_logger` a
// `&'static dyn Log`. State that the install computes (color, start time) lives
// in module statics; the level filter is `log::max_level()` (set via
// `set_max_level`), which the `log` macros also consult to skip work early.
static LOG_COLOR: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static LOG_START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

struct StderrLogger;
static STDERR_LOGGER: StderrLogger = StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        use std::io::Write;
        let us = LOG_START
            .get()
            .map(|s| s.elapsed().as_micros())
            .unwrap_or(0);
        let color = LOG_COLOR.load(core::sync::atomic::Ordering::Relaxed);
        let lvl = record.level();
        let (col, reset) = if color {
            let c = match lvl {
                log::Level::Error => "\x1b[31m", // red
                log::Level::Warn => "\x1b[33m",  // yellow
                log::Level::Info => "\x1b[32m",  // green
                log::Level::Debug => "\x1b[36m", // cyan
                log::Level::Trace => "\x1b[90m", // bright black
            };
            (c, "\x1b[0m")
        } else {
            ("", "")
        };
        let loc = match (record.file(), record.line()) {
            (Some(f), Some(l)) => {
                // Trim workspace-absolute paths to the file name for readability.
                let short = f.rsplit('/').next().unwrap_or(f);
                format!("  ({}:{})", short, l)
            }
            _ => String::new(),
        };
        // Lock stderr once per line so concurrent threads don't interleave.
        let mut err = std::io::stderr().lock();
        let _ = writeln!(
            err,
            "{col}[{us:>10}us] [{lvl:<5}] [{target}] {args}{loc}{reset}",
            col = col,
            us = us,
            lvl = lvl,
            target = record.target(),
            args = record.args(),
            loc = loc,
            reset = reset,
        );
    }

    fn flush(&self) {
        use std::io::Write;
        let _ = std::io::stderr().flush();
    }
}

/// Install azul's built-in stderr logger unless `AZ_LOG` disables it (default
/// ON). Idempotent and safe to call from every `App::create`: if a logger is
/// already installed (by the host or a previous call) this is a no-op.
pub fn init_default_logger() {
    use std::sync::atomic::{AtomicBool, Ordering};
    // Only attempt the install once per process.
    static TRIED: AtomicBool = AtomicBool::new(false);
    if TRIED.swap(true, Ordering::SeqCst) {
        return;
    }
    let level = match az_log_level() {
        Some(l) => l,
        None => return, // AZ_LOG=off — stay silent.
    };
    let color = {
        use std::io::IsTerminal;
        // Honor NO_COLOR (https://no-color.org/) and only colorize a real TTY.
        std::env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal()
    };
    // Stash install-time state for the zero-field static logger.
    let _ = LOG_START.set(std::time::Instant::now());
    LOG_COLOR.store(color, core::sync::atomic::Ordering::Relaxed);
    // `set_logger` (not the std-only `set_boxed_logger`) takes a &'static dyn Log
    // and works with `log`'s default-features-off build. It fails if the host
    // already installed a logger — that's fine, theirs wins.
    if log::set_logger(&STDERR_LOGGER).is_ok() {
        log::set_max_level(level);
        log::info!(
            target: "azul",
            "logging enabled at {:?} (AZ_LOG=off to silence, AZ_LOG=trace for everything)",
            level
        );
    }
}

#[cfg(test)]
mod tests {
    use super::escape_dialog_html;

    #[test]
    fn escapes_lt() {
        assert_eq!(escape_dialog_html("<"), "&lt;");
    }

    #[test]
    fn escapes_gt() {
        assert_eq!(escape_dialog_html(">"), "&gt;");
    }

    #[test]
    fn escapes_mixed_tag() {
        assert_eq!(escape_dialog_html("<a>"), "&lt;a&gt;");
    }

    #[test]
    fn leaves_plain_text_untouched() {
        let plain = "panic at file.rs line 42 in thread main";
        assert_eq!(escape_dialog_html(plain), plain);
    }

    #[test]
    fn does_not_escape_ampersand() {
        // Documents the intentional behavior: `&` is not escaped, so an input
        // that already contains `&lt;` is double-escaped to `&lt;` only on the
        // angle bracket, leaving the existing entity intact.
        assert_eq!(escape_dialog_html("&lt;"), "&lt;");
        assert_eq!(escape_dialog_html("a & b"), "a & b");
    }
}
