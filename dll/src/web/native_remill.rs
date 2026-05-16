//! Safe Rust wrapper around `dll/src/web/cpp/azul_remill.cpp`'s
//! C ABI. Only compiled with the `web-transpiler-static` feature;
//! when off, the lift pipeline falls back to subprocess invocations
//! of `remill-lift-17` + `opt` + `llc` + `wasm-ld`.
//!
//! All FFI calls are wrapped in a process-wide Mutex because LLVM's
//! TargetRegistry and LLD's static state are NOT thread-safe.
//! Workloads that need parallelism should batch lifts via the
//! upcoming `lift_batch` API, not call lift_single from multiple
//! threads.

#![cfg(feature = "web-transpiler-static")]

use std::ffi::{c_char, c_int, CStr, CString};
use std::sync::Mutex;

extern "C" {
    fn az_remill_lift(
        arch_name: *const c_char,
        os_name: *const c_char,
        address: u64,
        bytes: *const u8,
        bytes_len: usize,
        ir_out: *mut *mut c_char,
        ir_len_out: *mut usize,
        err_out: *mut *mut c_char,
    ) -> c_int;

    fn az_remill_compile_to_wasm32_obj(
        ir_str: *const c_char,
        ir_len: usize,
        obj_out: *mut *mut u8,
        obj_len_out: *mut usize,
        err_out: *mut *mut c_char,
    ) -> c_int;

    fn az_remill_wasm_link(
        objs: *const *const u8,
        obj_lens: *const usize,
        obj_count: usize,
        exports: *const *const c_char,
        export_count: usize,
        import_memory: c_int,
        import_table: c_int,
        initial_memory_bytes: u32,
        wasm_out: *mut *mut u8,
        wasm_len_out: *mut usize,
        err_out: *mut *mut c_char,
    ) -> c_int;

    fn az_remill_free(ptr: *mut c_char);
    fn az_remill_free_buf(ptr: *mut u8);
}

/// Anchor static references so the linker's `-Wl,-dead_strip` pass
/// keeps the FFI symbols even before any Rust callsite invokes them.
/// Without this, the linker sees the extern decls as unused and
/// strips libazul_remill_wrapper.a's bodies from the final dylib.
#[used]
static ANCHOR_LIFT: unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    u64,
    *const u8,
    usize,
    *mut *mut c_char,
    *mut usize,
    *mut *mut c_char,
) -> c_int = az_remill_lift;
#[used]
static ANCHOR_COMPILE: unsafe extern "C" fn(
    *const c_char,
    usize,
    *mut *mut u8,
    *mut usize,
    *mut *mut c_char,
) -> c_int = az_remill_compile_to_wasm32_obj;
#[used]
static ANCHOR_LINK: unsafe extern "C" fn(
    *const *const u8,
    *const usize,
    usize,
    *const *const c_char,
    usize,
    c_int,
    c_int,
    u32,
    *mut *mut u8,
    *mut usize,
    *mut *mut c_char,
) -> c_int = az_remill_wasm_link;
#[used]
static ANCHOR_FREE: unsafe extern "C" fn(*mut c_char) = az_remill_free;
#[used]
static ANCHOR_FREE_BUF: unsafe extern "C" fn(*mut u8) = az_remill_free_buf;

/// Serializes every FFI call. LLVM's TargetRegistry has shared state;
/// LLD's wasm driver uses CommandLine globals. Concurrent calls would
/// race. Per-fn parallelism happens at the higher level (multiple
/// callbacks, batched lift, dep-cache reuse).
static FFI_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug)]
pub struct NativeRemillError {
    pub stage: &'static str,
    pub code: i32,
    pub message: String,
}

impl std::fmt::Display for NativeRemillError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] rc={}: {}", self.stage, self.code, self.message)
    }
}
impl std::error::Error for NativeRemillError {}

/// Lift native bytes to LLVM IR text via the in-process wrapper.
///
/// `arch_name`, `os_name`: same strings as remill-lift-17's `--arch`
/// and `--os` (e.g., "aarch64", "macos").
pub fn lift(
    arch_name: &str,
    os_name: &str,
    address: u64,
    bytes: &[u8],
) -> Result<String, NativeRemillError> {
    let _guard = FFI_LOCK.lock().unwrap();
    let arch_c = CString::new(arch_name).map_err(|_| NativeRemillError {
        stage: "lift",
        code: -1,
        message: "arch_name contains NUL byte".into(),
    })?;
    let os_c = CString::new(os_name).map_err(|_| NativeRemillError {
        stage: "lift",
        code: -1,
        message: "os_name contains NUL byte".into(),
    })?;
    let mut ir_ptr: *mut c_char = std::ptr::null_mut();
    let mut ir_len: usize = 0;
    let mut err_ptr: *mut c_char = std::ptr::null_mut();
    let rc = unsafe {
        az_remill_lift(
            arch_c.as_ptr(),
            os_c.as_ptr(),
            address,
            bytes.as_ptr(),
            bytes.len(),
            &mut ir_ptr,
            &mut ir_len,
            &mut err_ptr,
        )
    };
    if rc != 0 {
        let message = unsafe { take_c_string(err_ptr) };
        return Err(NativeRemillError {
            stage: "lift",
            code: rc as i32,
            message,
        });
    }
    let ir = unsafe { take_c_string_with_len(ir_ptr, ir_len) };
    Ok(ir)
}

/// Compile LLVM IR text to a wasm32 .o object via in-process opt + llc.
pub fn compile_to_wasm32_obj(ir_text: &str) -> Result<Vec<u8>, NativeRemillError> {
    let _guard = FFI_LOCK.lock().unwrap();
    let ir_c = CString::new(ir_text).map_err(|_| NativeRemillError {
        stage: "compile",
        code: -1,
        message: "IR text contains NUL byte".into(),
    })?;
    let mut obj_ptr: *mut u8 = std::ptr::null_mut();
    let mut obj_len: usize = 0;
    let mut err_ptr: *mut c_char = std::ptr::null_mut();
    let rc = unsafe {
        az_remill_compile_to_wasm32_obj(
            ir_c.as_ptr(),
            ir_text.len(),
            &mut obj_ptr,
            &mut obj_len,
            &mut err_ptr,
        )
    };
    if rc != 0 {
        let message = unsafe { take_c_string(err_ptr) };
        return Err(NativeRemillError {
            stage: "compile",
            code: rc as i32,
            message,
        });
    }
    Ok(unsafe { take_byte_buf(obj_ptr, obj_len) })
}

/// Link wasm32 .o objects into a final .wasm via in-process lld::wasm.
pub fn wasm_link(
    objs: &[Vec<u8>],
    exports: &[String],
    import_memory: bool,
    import_table: bool,
    initial_memory_bytes: u32,
) -> Result<Vec<u8>, NativeRemillError> {
    let _guard = FFI_LOCK.lock().unwrap();
    let obj_ptrs: Vec<*const u8> = objs.iter().map(|o| o.as_ptr()).collect();
    let obj_lens: Vec<usize> = objs.iter().map(|o| o.len()).collect();
    // CString needs to outlive the *const c_char pointers it produces.
    let export_cs: Vec<CString> = exports
        .iter()
        .map(|s| CString::new(s.as_str()).expect("export name has NUL"))
        .collect();
    let export_ptrs: Vec<*const c_char> = export_cs.iter().map(|s| s.as_ptr()).collect();
    let mut wasm_ptr: *mut u8 = std::ptr::null_mut();
    let mut wasm_len: usize = 0;
    let mut err_ptr: *mut c_char = std::ptr::null_mut();
    let rc = unsafe {
        az_remill_wasm_link(
            obj_ptrs.as_ptr(),
            obj_lens.as_ptr(),
            objs.len(),
            export_ptrs.as_ptr(),
            exports.len(),
            import_memory as c_int,
            import_table as c_int,
            initial_memory_bytes,
            &mut wasm_ptr,
            &mut wasm_len,
            &mut err_ptr,
        )
    };
    if rc != 0 {
        let message = unsafe { take_c_string(err_ptr) };
        return Err(NativeRemillError {
            stage: "wasm_link",
            code: rc as i32,
            message,
        });
    }
    Ok(unsafe { take_byte_buf(wasm_ptr, wasm_len) })
}

unsafe fn take_c_string(ptr: *mut c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
    az_remill_free(ptr);
    s
}

unsafe fn take_c_string_with_len(ptr: *mut c_char, len: usize) -> String {
    if ptr.is_null() || len == 0 {
        if !ptr.is_null() {
            az_remill_free(ptr);
        }
        return String::new();
    }
    let slice = std::slice::from_raw_parts(ptr as *const u8, len);
    let s = String::from_utf8_lossy(slice).into_owned();
    az_remill_free(ptr);
    s
}

unsafe fn take_byte_buf(ptr: *mut u8, len: usize) -> Vec<u8> {
    if ptr.is_null() || len == 0 {
        if !ptr.is_null() {
            az_remill_free_buf(ptr);
        }
        return Vec::new();
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    let v = slice.to_vec();
    az_remill_free_buf(ptr);
    v
}
// touched Sa 16 Mai 2026 23:45:36 CEST
