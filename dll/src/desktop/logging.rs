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
            #[cfg(not(target_os = "linux"))]
            let dialog_str = &error_str;

            #[cfg(target_os = "linux")]
            let dialog_str = escape_dialog_html(&error_str);

            tfd::MessageBox::new("Unexpected fatal error", &dialog_str)
                .with_icon(tfd::MessageBoxIcon::Info)
                .run_modal();
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
