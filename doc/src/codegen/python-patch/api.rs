// Python API patches for types that require manual implementation
// These are types with callbacks, Python GC integration, or complex logic
//
// NOTE: This file is included via include!() in the generated python_capi.rs
// All types referenced here must be either:
// - Defined in this file
// - Defined in the generated python_capi.rs (AzFoo types)
// - Available via crate::ffi (C-API functions from dll_api.rs)
// - Available via azul_core:: (internal types)

use pyo3::gc::{PyVisit, PyTraverseError};
use pyo3::conversion::IntoPyObject;

// Import types from C-API that are used by helper functions and manual implementations
// NOTE: Vec types like AzU8Vec, AzMonitorVec are now generated, so don't import them
use crate::ffi::dll::{
    // String and basic Vec types - these are skipped in generator due to conflicting trait impls
    // They have manual FromPyObject/IntoPyObject implementations below
    AzString,
    AzU8Vec,
    AzStringVec,
    // Ref/RefMut types for slices (not generated, used in helper functions)
    AzRefstr,
    AzU8VecRef,
    AzU8VecRefMut,
    AzF32VecRef,
    AzGLuintVecRef,
    AzGLintVecRefMut,
    AzGLint64VecRefMut,
    AzGLbooleanVecRefMut,
    AzGLfloatVecRefMut,
    AzRefstrVecRef,
    AzTessellatedSvgNodeVecRef,
    // Destructor types for Vec ownership (enum with function pointer variants)
    AzU8VecDestructor,
    AzStringVecDestructor,
    // Layout callback type for WindowCreateOptions
    AzLayoutCallbackType,
    AzLayoutCallback,
    AzLayoutCallbackInner,
};

// helper functions for type conversion
fn pystring_to_azstring(input: &String) -> AzString {
    input.clone().into()
}

fn az_string_to_py_string(input: AzString) -> String {
    let bytes = unsafe {
        core::slice::from_raw_parts(input.vec.ptr, input.vec.len)
    };
    String::from_utf8_lossy(bytes).into_owned()
}

fn pystring_to_refstr(input: &str) -> AzRefstr {
    AzRefstr {
        ptr: input.as_ptr() as *const core::ffi::c_void,
        len: input.len(),
    }
}

fn az_vecu8_to_py_vecu8(input: AzU8Vec) -> Vec<u8> {
    let slice = unsafe {
        core::slice::from_raw_parts(input.ptr, input.len)
    };
    slice.to_vec()
}

fn vec_string_to_vec_refstr(input: &Vec<&str>) -> Vec<AzRefstr> {
    input.iter().map(|i| pystring_to_refstr(i)).collect()
}

fn pybytesrefmut_to_vecu8refmut(input: &mut Vec<u8>) -> AzU8VecRefMut {
    AzU8VecRefMut { ptr: input.as_mut_ptr() as *mut core::ffi::c_void, len: input.len() }
}

fn pybytesref_to_vecu8_ref(input: &Vec<u8>) -> AzU8VecRef {
    AzU8VecRef { ptr: input.as_ptr() as *const core::ffi::c_void, len: input.len() }
}

fn pylist_f32_to_rust(input: &Vec<f32>) -> AzF32VecRef {
    AzF32VecRef { ptr: input.as_ptr() as *const core::ffi::c_void, len: input.len() }
}

fn pylist_u32_to_rust(input: &Vec<u32>) -> AzGLuintVecRef {
    AzGLuintVecRef { ptr: input.as_ptr() as *const core::ffi::c_void, len: input.len() }
}

fn pylist_i32_to_rust(input: &mut Vec<i32>) -> AzGLintVecRefMut {
    AzGLintVecRefMut { ptr: input.as_mut_ptr() as *mut core::ffi::c_void, len: input.len() }
}

fn pylist_i64_to_rust(input: &mut Vec<i64>) -> AzGLint64VecRefMut {
    AzGLint64VecRefMut { ptr: input.as_mut_ptr() as *mut core::ffi::c_void, len: input.len() }
}

fn pylist_bool_to_rust(input: &mut Vec<u8>) -> AzGLbooleanVecRefMut {
    AzGLbooleanVecRefMut { ptr: input.as_mut_ptr() as *mut core::ffi::c_void, len: input.len() }
}

fn pylist_glfloat_to_rust(input: &mut Vec<f32>) -> AzGLfloatVecRefMut {
    AzGLfloatVecRefMut { ptr: input.as_mut_ptr() as *mut core::ffi::c_void, len: input.len() }
}

fn pylist_str_to_rust(input: &Vec<AzRefstr>) -> AzRefstrVecRef {
    AzRefstrVecRef { ptr: input.as_ptr() as *const core::ffi::c_void, len: input.len() }
}

fn pylist_tessellated_svg_node(input: &Vec<AzTessellatedSvgNode>) -> AzTessellatedSvgNodeVecRef {
    AzTessellatedSvgNodeVecRef { ptr: input.as_ptr() as *const core::ffi::c_void, len: input.len() }
}

// from implementations for type conversion
impl From<String> for AzString {
    fn from(s: String) -> AzString {
        let bytes = s.into_bytes();
        let ptr = bytes.as_ptr();
        let len = bytes.len();
        let cap = bytes.capacity();
        core::mem::forget(bytes);
        
        AzString {
            vec: AzU8Vec {
                ptr,
                len,
                cap,
                destructor: AzU8VecDestructor::DefaultRust,
            }
        }
    }
}

impl From<AzString> for String {
    fn from(s: AzString) -> String {
        az_string_to_py_string(s)
    }
}

impl From<AzU8Vec> for Vec<u8> {
    fn from(input: AzU8Vec) -> Vec<u8> {
        az_vecu8_to_py_vecu8(input)
    }
}

impl From<Vec<u8>> for AzU8Vec {
    fn from(input: Vec<u8>) -> AzU8Vec {
        let ptr = input.as_ptr();
        let len = input.len();
        let cap = input.capacity();
        core::mem::forget(input);
        
        AzU8Vec {
            ptr,
            len,
            cap,
            destructor: AzU8VecDestructor::DefaultRust,
        }
    }
}

// // pyo3 conversion traits for azul types (pyo3 0.27 api)
// these allow pyo3 to automatically convert between python and rust types
//
use pyo3::Borrowed;

// --- AzString <-> Python str ---

impl FromPyObject<'_, '_> for AzString {
    type Error = PyErr;
    
    fn extract(ob: Borrowed<'_, '_, PyAny>) -> Result<Self, Self::Error> {
        let s: String = ob.extract()?;
        Ok(s.into())
    }
}

impl<'py> IntoPyObject<'py> for AzString {
    type Target = PyString;
    type Output = Bound<'py, PyString>;
    type Error = std::convert::Infallible;
    
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let s: String = self.into();
        Ok(PyString::new(py, &s))
    }
}

// --- AzU8Vec <-> Python bytes ---

impl FromPyObject<'_, '_> for AzU8Vec {
    type Error = PyErr;
    
    fn extract(ob: Borrowed<'_, '_, PyAny>) -> Result<Self, Self::Error> {
        let v: Vec<u8> = ob.extract()?;
        Ok(v.into())
    }
}

impl<'py> IntoPyObject<'py> for AzU8Vec {
    type Target = PyBytes;
    type Output = Bound<'py, PyBytes>;
    type Error = std::convert::Infallible;
    
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let slice = unsafe { core::slice::from_raw_parts(self.ptr, self.len) };
        Ok(PyBytes::new(py, slice))
    }
}

// --- AzStringVec <-> Python list[str] ---

impl FromPyObject<'_, '_> for AzStringVec {
    type Error = PyErr;
    
    fn extract(ob: Borrowed<'_, '_, PyAny>) -> Result<Self, Self::Error> {
        let v: Vec<String> = ob.extract()?;
        let az_strings: Vec<AzString> = v.into_iter().map(|s| s.into()).collect();
        Ok(AzStringVec::from_vec(az_strings))
    }
}

impl<'py> IntoPyObject<'py> for AzStringVec {
    type Target = PyList;
    type Output = Bound<'py, PyList>;
    type Error = PyErr;
    
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let strings: Vec<String> = self.into_rust_vec();
        PyList::new(py, strings)
    }
}

// Helper methods for AzStringVec
impl AzStringVec {
    fn from_vec(v: Vec<AzString>) -> Self {
        let ptr = v.as_ptr();
        let len = v.len();
        let cap = v.capacity();
        core::mem::forget(v);
        
        AzStringVec {
            ptr,
            len,
            cap,
            destructor: AzStringVecDestructor::DefaultRust,
        }
    }
    
    fn into_rust_vec(self) -> Vec<String> {
        let slice = unsafe { core::slice::from_raw_parts(self.ptr, self.len) };
        slice.iter().map(|s| {
            let bytes = unsafe { core::slice::from_raw_parts(s.vec.ptr, s.vec.len) };
            String::from_utf8_lossy(bytes).into_owned()
        }).collect()
    }
}

// // callback wrapper types (stored inside refany)
// these hold python objects and are the key to the callback system
//
/// Holds Python objects for the main App (data + layout callback)
#[repr(C)]
pub struct AppDataTy {
    pub _py_app_data: Option<Py<PyAny>>,
    pub _py_layout_callback: Option<Py<PyAny>>,
}

/// Holds Python objects for layout callbacks (used in MarshaledLayoutCallback)
#[repr(C)]
pub struct LayoutCallbackTy {
    pub _py_layout_callback: Option<Py<PyAny>>,
}

/// Holds Python objects for event callbacks (On.Click, On.Hover, etc.)
#[repr(C)]
pub struct CallbackTy {
    pub _py_callback: Option<Py<PyAny>>,
    pub _py_data: Option<Py<PyAny>>,
}

/// Holds Python objects for iframe callbacks
#[repr(C)]
pub struct IFrameCallbackTy {
    pub _py_iframe_callback: Option<Py<PyAny>>,
    pub _py_iframe_data: Option<Py<PyAny>>,
}

/// Holds Python objects for timer callbacks
/// 
/// NOTE: Timer callbacks require `TimerCallbackInfo` and `TimerCallbackReturn` types
/// which are defined in `azul_layout::timer` but NOT yet exported in api.json / C-API.
/// To implement timer callbacks:
/// 1. Add TimerCallbackInfo, TimerCallbackReturn, TerminateTimer to api.json
/// 2. Regenerate the C-API with `cargo run -p azul-doc -- codegen`
/// 3. Add `invoke_py_timer` trampoline here (similar to `invoke_py_iframe`)
/// 4. Timer callbacks run on main thread, so no threading concerns
#[repr(C)]
pub struct TimerCallbackTy {
    pub _py_timer_callback: Option<Py<PyAny>>,
    pub _py_timer_data: Option<Py<PyAny>>,
}

/// Holds Python objects for thread writeback callbacks
#[repr(C)]
pub struct ThreadWriteBackCallbackTy {
    pub _py_thread_callback: Option<Py<PyAny>>,
    pub _py_thread_data: Option<Py<PyAny>>,
}

/// Holds Python objects for image render callbacks
#[repr(C)]
pub struct ImageCallbackTy {
    pub _py_image_callback: Option<Py<PyAny>>,
    pub _py_image_data: Option<Py<PyAny>>,
}

/// Holds arbitrary Python data (for datasets)
#[repr(C)]
pub struct DatasetTy {
    pub _py_data: Option<Py<PyAny>>,
}

// // extern "c" trampolines
// these are called by the c-api and invoke python callbacks
//
/// Trampoline for layout callbacks
/// 
/// The RefAny contains an AppDataTy which holds both:
/// - The Python layout callback function
/// - The Python user data
extern "C" fn invoke_py_layout_callback(
    app_data: &mut azul_core::refany::RefAny,
    info: azul_core::callbacks::LayoutCallbackInfo
) -> azul_core::styled_dom::StyledDom {
    
    let default = azul_core::styled_dom::StyledDom::default();
    
    // Get the app data (which contains the Python callback AND user data)
    let app = match app_data.downcast_ref::<AppDataTy>() {
        Some(s) => s,
        None => {
            #[cfg(feature = "logging")]
            log::error!("Failed to downcast app_data to AppDataTy");
            return default;
        }
    };

    let py_callback = match app._py_layout_callback.as_ref() {
        Some(s) => s,
        None => {
            #[cfg(feature = "logging")]
            log::error!("No layout callback found in app_data");
            return default;
        }
    };

    let py_data = match app._py_app_data.as_ref() {
        Some(s) => s,
        None => {
            #[cfg(feature = "logging")]
            log::error!("No app data found in app_data");
            return default;
        }
    };

    // Call the Python layout callback
    Python::with_gil(|py| {
        let info_py: AzLayoutCallbackInfo = unsafe { mem::transmute(info) };
        
        match py_callback.call1(py, (py_data.clone_ref(py), info_py)) {
            Ok(result) => {
                match result.extract::<AzStyledDom>(py) {
                    Ok(styled_dom) => unsafe { mem::transmute(styled_dom) },
                    Err(e) => {
                        #[cfg(feature = "logging")]
                        log::error!("Layout callback must return StyledDom: {:?}", e);
                        default
                    }
                }
            }
            Err(e) => {
                #[cfg(feature = "logging")]
                log::error!("Exception in layout callback: {:?}", e);
                default
            }
        }
    })
}

/// Trampoline for regular event callbacks
extern "C" fn invoke_py_callback(
    data: &mut azul_core::refany::RefAny,
    info: azul_layout::callbacks::CallbackInfo
) -> azul_core::callbacks::Update {
    use azul_core::callbacks::Update;
    
    let default = Update::DoNothing;
    
    let cb = match data.downcast_mut::<CallbackTy>() {
        Some(s) => s,
        None => return default,
    };

    let py_callback = match cb._py_callback.as_ref() {
        Some(s) => s,
        None => return default,
    };

    let py_data = match cb._py_data.as_ref() {
        Some(s) => s,
        None => return default,
    };

    Python::with_gil(|py| {
        let info_py: AzCallbackInfo = unsafe { mem::transmute(info) };
        
        match py_callback.call1(py, (py_data.clone_ref(py), info_py)) {
            Ok(result) => {
                match result.extract::<AzUpdate>(py) {
                    Ok(update) => unsafe { mem::transmute(update) },
                    Err(_) => default,
                }
            }
            Err(e) => {
                #[cfg(feature = "logging")]
                log::error!("Exception in callback: {:?}", e);
                default
            }
        }
    })
}

/// Trampoline for iframe callbacks
extern "C" fn invoke_py_iframe(
    data: &mut azul_core::refany::RefAny,
    info: azul_core::callbacks::IFrameCallbackInfo
) -> azul_core::callbacks::IFrameCallbackReturn {
    
    // Use Default trait which properly initializes all fields
    let default = azul_core::callbacks::IFrameCallbackReturn::default();
    
    let cb = match data.downcast_mut::<IFrameCallbackTy>() {
        Some(s) => s,
        None => return default,
    };

    let py_callback = match cb._py_iframe_callback.as_ref() {
        Some(s) => s,
        None => return default,
    };

    let py_data = match cb._py_iframe_data.as_ref() {
        Some(s) => s,
        None => return default,
    };

    Python::with_gil(|py| {
        let info_py: AzIFrameCallbackInfo = unsafe { mem::transmute(info) };
        
        match py_callback.call1(py, (py_data.clone_ref(py), info_py)) {
            Ok(result) => {
                match result.extract::<AzIFrameCallbackReturn>(py) {
                    Ok(ret) => unsafe { mem::transmute(ret) },
                    Err(_) => default,
                }
            }
            Err(e) => {
                #[cfg(feature = "logging")]
                log::error!("Exception in iframe callback: {:?}", e);
                default
            }
        }
    })
}

// NOTE: Timer callbacks are not implemented yet because they require
// complex types (TimerCallbackInfo, TimerCallbackReturn) that are not
// easily exposed to Python. This is a TODO for future work.

// app implementation
/// The main application - runs the event loop
#[pyclass(name = "App", module = "azul", unsendable)]
pub struct AzApp {
    pub ptr: *const c_void,
    pub run_destructor: bool,
}

#[pymethods]
impl AzApp {
    /// Create a new App with user data and a layout callback
    /// 
    /// Python usage:
    ///     def layout(data, info):
    ///         return StyledDom.from_dom(Dom.body())
    ///     
    ///     app = App(my_data, layout)
    ///     app.run(WindowCreateOptions.new())
    #[new]
    fn new(data: Py<PyAny>, layout_callback: Py<PyAny>) -> PyResult<Self> {
        // Verify callback is callable
        Python::with_gil(|py| {
            if !layout_callback.bind(py).is_callable() {
                return Err(PyException::new_err("layout_callback must be callable"));
            }
            Ok(())
        })?;

        // Create app data wrapper with the Python objects
        let app_data = AppDataTy {
            _py_app_data: Some(data),
            _py_layout_callback: Some(layout_callback),
        };

        // Store in RefAny - this is an internal detail, user never sees it
        let refany = azul_core::refany::RefAny::new(app_data);

        // Get default app config
        let app_config = unsafe { crate::ffi::dll::AzAppConfig_new() };

        // Call C-API to create the app
        let app = unsafe {
            crate::ffi::dll::AzApp_new(
                mem::transmute(refany),
                app_config,
            )
        };

        Ok(AzApp {
            ptr: app.ptr,
            run_destructor: true,
        })
    }

    /// Run the application event loop with an initial window
    fn run(&mut self, window: AzWindowCreateOptions) {
        // Note: We need to set the layout callback in the window
        // For now, just run with the window as-is
        unsafe {
            crate::ffi::dll::AzApp_run(
                mem::transmute(self),
                window.inner.clone(),
            );
        }
    }

    /// Add another window to the application
    fn add_window(&mut self, window: AzWindowCreateOptions) {
        unsafe {
            crate::ffi::dll::AzApp_addWindow(
                mem::transmute(self),
                window.inner.clone(),
            );
        }
    }

    /// Get the list of available monitors
    fn get_monitors(&self) -> AzMonitorVec {
        unsafe {
            mem::transmute(crate::ffi::dll::AzApp_getMonitors(
                mem::transmute(self),
            ))
        }
    }

    fn __traverse__(&self, _visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        // GC traversal is intentionally empty. Here's why:
        //
        // The App contains a RefAny which holds our AppDataTy (with Py<PyAny> fields).
        // Ideally we'd traverse those Python objects, but:
        //
        // 1. RefAny is an opaque C struct - we'd need to transmute through multiple
        //    layers (AzApp -> App -> Box<AppInternal> -> RefAny -> AppDataTy)
        // 2. This is fragile and could break if internal layouts change
        // 3. The Python GC only needs __traverse__ to detect reference cycles
        // 4. Cycles between Python and Azul objects are rare in practice
        // 5. When App is dropped, RefAny's destructor properly decrements Py<PyAny> refcounts
        //
        // If cycles become a problem, we could add a C-API function:
        //   AzApp_getRefAnyPtr(&self) -> *mut RefAny
        // Then downcast to AppDataTy and traverse the Py<PyAny> fields.
        Ok(())
    }

    fn __clear__(&mut self) {
        // GC clearing is intentionally empty - see __traverse__ comments.
        // Python objects in RefAny are properly cleaned up when App is dropped.
    }

    fn __str__(&self) -> String {
        "App { ... }".to_string()
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

impl Drop for AzApp {
    fn drop(&mut self) {
        if self.run_destructor {
            unsafe {
                crate::ffi::dll::AzApp_delete(mem::transmute(self));
            }
        }
    }
}

// // layout callback info (needs unsendable due to pointers)
// must match c-api layout exactly for mem::transmute to work
//
/// Information passed to layout callbacks
#[pyclass(name = "LayoutCallbackInfo", module = "azul", unsendable)]
#[repr(C)]
pub struct AzLayoutCallbackInfo {
    pub ref_data: *const c_void,
    pub window_size: AzWindowSize,
    pub theme: AzWindowTheme,
    pub _abi_ref: *const c_void,
    pub _abi_mut: *mut c_void,
}

#[pymethods]
impl AzLayoutCallbackInfo {
    fn __str__(&self) -> String {
        "LayoutCallbackInfo { ... }".to_string()
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

// // window create options
// we implement this manually because the c-api version contains callbacks
// the struct layout must match the c-api exactly for mem::transmute to work
//
/// Options for creating a new window
/// NOTE: This is a simplified version for Python - some fields are not exposed
#[pyclass(name = "WindowCreateOptions", module = "azul", unsendable)]
pub struct AzWindowCreateOptions {
    inner: crate::ffi::dll::AzWindowCreateOptions,
}

#[pymethods]
impl AzWindowCreateOptions {
    /// Create default window options
    #[new]
    fn new() -> Self {
        Self { inner: Default::default() }
    }

    /// Set the window title
    fn with_title(&self, title: String) -> Self {
        let mut inner = self.inner.clone();
        inner.state.title = title.into();
        Self { inner }
    }

    fn __str__(&self) -> String {
        "WindowCreateOptions { ... }".to_string()
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

impl Clone for AzWindowCreateOptions {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

// // note: refany is not exposed to python users!
// it's used internally to store python objects, but users never interact with it.
//