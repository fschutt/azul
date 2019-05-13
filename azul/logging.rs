use dialogs::msg_box_ok;
use log::LevelFilter;
use std::sync::atomic::{Ordering, AtomicBool};

pub(crate) static SHOULD_ENABLE_PANIC_HOOK: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_up_logging(log_file_path: Option<&str>, log_level: LevelFilter) {

    use fern::InitError;
    use std::error::Error;

    /// Sets up the global logger
    fn set_up_logging_internal(log_file_path: Option<&str>, log_level: LevelFilter)
    -> Result<(), InitError>
    {
        use std::io::{Error as IoError, ErrorKind as IoErrorKind};
        use fern::{Dispatch, log_file};

        let log_location = {
            use std::env;

            let mut exe_location = env::current_exe()
            .map_err(|_| InitError::Io(IoError::new(IoErrorKind::Other,
                "Executable has no executable path (?), can't open log file")))?;

            exe_location.pop();
            exe_location.push(log_file_path.unwrap_or("error.log"));
            exe_location
        };

        Dispatch::new()
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
            .chain(log_file(log_location)?)
            .apply()?;
        Ok(())
    }

    match set_up_logging_internal(log_file_path, log_level) {
        Ok(_) => { },
        Err(e) => match e {
            InitError::Io(e) => {
                println!("[WARN] Logging IO init error: \r\nkind: {:?}\r\n\r\ndescription:\r\n{}\r\n\r\ncause:\r\n{:?}\r\n",
                           e.kind(), e.description(), e.source());
            },
            InitError::SetLoggerError(e) => {
                println!("[WARN] Logging initalization error: \r\ndescription:\r\n{}\r\n\r\ncause:\r\n{:?}\r\n",
                    e.description(), e.source());
            }
        }
    }
}

/// In the (rare) case of a panic, print it to the stdout, log it to the file and
/// prompt the user with a message box.
pub(crate) fn set_up_panic_hooks() {

    use std::panic::{self, PanicInfo};
    use backtrace::{Backtrace, BacktraceFrame};

    fn panic_fn(panic_info: &PanicInfo) {

        use std::thread;

        let payload = panic_info.payload();
        let location = panic_info.location();

        let payload_str = format!("{:?}", payload);
        let panic_str = payload.downcast_ref::<String>()
                .map(|s| s.as_ref())
                .or_else(||
                    payload.downcast_ref::<&str>()
                    .map(|s| *s)
                )
                .unwrap_or(payload_str.as_str());

        let location_str = location.map(|loc| format!("{} at line {}", loc.file(), loc.line()));
        let backtrace_str_old = format_backtrace(&Backtrace::new());
        let backtrace_str = backtrace_str_old
            .lines()
            .filter(|l| !l.is_empty())
            .collect::<Vec<&str>>()
            .join("\r\n");

        let thread = thread::current();
        let thread_name = thread.name().unwrap_or("<unnamed thread>");

        let error_str = format!(
            "An unexpected panic ocurred, the program has to exit.\r\n\
             Please report this error and attach the log file found in the directory of the executable.\r\n\
             \r\n\
             The error ocurred in: {} in thread {}\r\n\
             \r\n\
             Error information:\r\n\
             {}\r\n\
             \r\n\
             Backtrace:\r\n\
             \r\n\
             {}\r\n",
            location_str.unwrap_or(format!("<unknown location>")), thread_name, panic_str, backtrace_str);

        #[cfg(target_os = "linux")]
        let mut error_str_clone = error_str.clone();
        #[cfg(target_os = "linux")] {
            error_str_clone = error_str_clone.replace("<", "&lt;");
            error_str_clone = error_str_clone.replace(">", "&gt;");
        }

        // TODO: invoke external app crash handler with the location to the log file
        error!("{}", error_str);

        if SHOULD_ENABLE_PANIC_HOOK.load(Ordering::SeqCst) {
            #[cfg(not(target_os = "linux"))]
            msg_box_ok("Unexpected fatal error", &error_str, ::tinyfiledialogs::MessageBoxIcon::Error);
            #[cfg(target_os = "linux")]
            msg_box_ok("Unexpected fatal error", &error_str_clone, ::tinyfiledialogs::MessageBoxIcon::Error);
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

            // skip the first 10 symbols because they belong to the
            // backtrace library and aren't relevant for debugging
            symbols.iter().map(|symbol| {

                let mut nice_string = String::new();

                if let Some(name) = symbol.name() {
                    let name_demangled = format!("{}", name);
                    let name_demangled_new = name_demangled.rsplit("::").skip(1).map(|e| e.to_string()).collect::<Vec<String>>();
                    let name_demangled = name_demangled_new.into_iter().rev().collect::<Vec<String>>().join("::");
                    nice_string.push_str(&name_demangled);
                } else {
                    nice_string.push_str(UNRESOLVED_FN_STR);
                }

                let mut file_string = String::new();
                if let Some(file) = symbol.filename() {
                    let origin_file_name = file.file_name()
                        .unwrap_or(OsStr::new("unresolved file name"))
                        .to_string_lossy();
                    file_string.push_str(&format!("{}", origin_file_name));
                }

                if let Some(line) = symbol.lineno() {
                    file_string.push_str(&format!(":{}", line));
                }

                if !file_string.is_empty() {
                    nice_string.push_str(" @ ");
                    nice_string.push_str(&file_string);
                    if !nice_string.ends_with("\n") {
                        nice_string.push_str("\n");
                    }
                }

                nice_string

            }).collect::<Vec<String>>().join("")
        }

        backtrace
            .frames()
            .iter()
            .map(|frame| format_frame(frame))
            .collect::<Vec<String>>()
            .join("\r\n")
    }

    panic::set_hook(Box::new(panic_fn));
}