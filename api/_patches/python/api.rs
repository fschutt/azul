
use pyo3::{PyVisit, PyTraverseError, PyGCProtocol};

fn pystring_to_azstring(input: &String) -> AzString {
    input.clone().into()
}
fn az_string_to_py_string(input: AzString) -> String {
    input.into()
}
fn pystring_to_refstr(input: &str) -> AzRefstr {
    AzRefstr {
        ptr: input.as_ptr(),
        len: input.len(),
    }
}
fn az_vecu8_to_py_vecu8(input: AzU8Vec) -> Vec<u8> {
    let input: azul_impl::css::U8Vec = unsafe { mem::transmute(input) };
    input.into_library_owned_vec()
}
fn vec_string_to_vec_refstr(input: &Vec<&str>) -> Vec<AzRefstr> {
    input.iter().map(|i| pystring_to_refstr(i)).collect()
}
fn pybytesrefmut_to_vecu8refmut(input: &mut Vec<u8>) -> AzU8VecRefMut {
    AzU8VecRefMut { ptr: input.as_mut_ptr(), len: input.len() }
}
fn pybytesref_to_vecu8_ref(input: &Vec<u8>) -> AzU8VecRef {
    AzU8VecRef { ptr: input.as_ptr(), len: input.len() }
}
fn pylist_f32_to_rust(input: &Vec<f32>) -> AzF32VecRef {
    AzF32VecRef { ptr: input.as_ptr(), len: input.len() }
}
fn pylist_u32_to_rust(input: &Vec<u32>) -> AzGLuintVecRef {
    AzGLuintVecRef { ptr: input.as_ptr(), len: input.len() }
}
fn pylist_i32_to_rust(input: &mut Vec<i32>) -> AzGLintVecRefMut {
    AzGLintVecRefMut { ptr: input.as_mut_ptr(), len: input.len() }
}
fn pylist_i64_to_rust(input: &mut Vec<i64>) -> AzGLint64VecRefMut {
    AzGLint64VecRefMut { ptr: input.as_mut_ptr(), len: input.len() }
}
fn pylist_bool_to_rust(input: &mut Vec<u8>) -> AzGLbooleanVecRefMut {
    AzGLbooleanVecRefMut { ptr: input.as_mut_ptr(), len: input.len() }
}
fn pylist_glfoat_to_rust(input: &mut Vec<f32>) -> AzGLfloatVecRefMut {
    AzGLfloatVecRefMut { ptr: input.as_mut_ptr(), len: input.len() }
}
fn pylist_str_to_rust(input: &Vec<AzRefstr>) -> AzRefstrVecRef {
    AzRefstrVecRef { ptr: input.as_ptr(), len: input.len() }
}
fn pylist_tesselated_svg_node(input: &Vec<AzTesselatedSvgNode>) -> AzTesselatedSvgNodeVecRef {
    AzTesselatedSvgNodeVecRef { ptr: input.as_ptr(), len: input.len() }
}

impl From<String> for AzString {
    fn from(s: String) -> AzString {
        Self { vec: s.into_bytes().into() }
    }
}

impl From<AzString> for String {
    fn from(s: AzString) -> String {
        let s: azul_impl::css::AzString = unsafe { mem::transmute(s) };
        s.into_library_owned_string()
    }
}

// AzU8Vec
impl From<AzU8Vec> for Vec<u8> {
    fn from(input: AzU8Vec) -> Vec<u8> {
        let input: azul_impl::css::U8Vec = unsafe { mem::transmute(input) };
        input.into_library_owned_vec()
    }
}

impl From<Vec<u8>> for AzU8Vec {
    fn from(input: Vec<u8>) -> AzU8Vec {

        let ptr = input.as_ptr();
        let len = input.len();
        let cap = input.capacity();

        let _ = ::core::mem::ManuallyDrop::new(input);

        Self {
            ptr,
            len,
            cap,
            destructor: AzU8VecDestructorEnumWrapper::DefaultRust(),
        }

    }
}

// manually implement App::new, WindowState::new,
// WindowCreateOptions::new and LayoutCallback::new

#[pyproto]
impl PyGCProtocol for AzApp {
    fn __traverse__(&self, visit: PyVisit) -> Result<(), PyTraverseError> {

        let data: &azul_impl::app::AzAppPtr = unsafe { mem::transmute(self) };

        // NOTE: should not block - this should only succeed
        // AFTER the App has finished executing
        let mut app_lock = match data.ptr.try_lock().ok() {
            Some(s) => s,
            None => return Ok(()),
        };

        let data_ref = match app_lock.data.downcast_ref::<AppDataTy>() {
            Some(s) => s,
            None => return Ok(()),
        };

        if let Some(obj) = data_ref._py_app_data.as_ref() {
            visit.call(obj)?;
        }

        Ok(())
    }

    fn __clear__(&mut self) {

        let mut data: &mut azul_impl::app::AzAppPtr = unsafe { mem::transmute(self) };

        // NOTE: should not block - this should only succeed
        // AFTER the App has finished executing
        let mut app_lock = match data.ptr.try_lock().ok() {
            Some(s) => s,
            None => return,
        };

        let mut data = match app_lock.data.downcast_mut::<AppDataTy>() {
            Some(s) => s,
            None => return,
        };

        // Clear reference, this decrements Python ref counter.
        data._py_app_data = None;
    }
}

#[repr(C)]
pub struct AppDataTy {
    _py_app_data: Option<PyObject>,
}

#[repr(C)]
pub struct LayoutCallbackTy {
    // acual callable object from python
    _py_layout_callback: Option<PyObject>,
}

extern "C" fn invoke_py_marshaled_layout_callback(
    marshal_data: &mut AzRefAny,
    app_data: &mut AzRefAny,
    info: AzLayoutCallbackInfo
) -> AzStyledDom {

    let mut marshal_data: &mut azul_impl::callbacks::RefAny = unsafe { mem::transmute(marshal_data) };
    let mut app_data: &mut azul_impl::callbacks::RefAny = unsafe { mem::transmute(app_data) };

    let mut app_data_downcast = match app_data.downcast_mut::<AppDataTy>() {
        Some(s) => s,
        None => return AzStyledDom::default(),
    };

    let mut app_data_downcast = match app_data_downcast._py_app_data.as_mut() {
        Some(s) => s,
        None => return AzStyledDom::default(),
    };

    let mut pyfunction = match marshal_data.downcast_mut::<LayoutCallbackTy>() {
        Some(s) => s,
        None => return AzStyledDom::default(),
    };

    let mut pyfunction = match pyfunction._py_layout_callback.as_mut() {
        Some(s) => s,
        None => return AzStyledDom::default(),
    };

    // call layout callback into python
    let s: AzStyledDom = Python::with_gil(|py| {

        match pyfunction.call1(py.clone(), (app_data_downcast.clone_ref(py.clone()), info)) {
            Ok(o) => match o.as_ref(py).extract::<AzStyledDom>() {
                Ok(o) => o.clone(),
                Err(e) => {
                    #[cfg(feature = "logging")] {
                        let cb_any = o.as_ref(py);
                        let type_name = cb_any.get_type().name().unwrap_or("<unknown>");
                        log::error!("ERROR: LayoutCallback returned object of type {}, expected azul.dom.StyledDom", type_name);
                    }
                    AzStyledDom::default()
                }
            },
            Err(e) => {
                #[cfg(feature = "logging")] {
                    log::error!("Exception caught when invoking LayoutCallback: {}", e);
                }
                AzStyledDom::default()
            }
        }
    });

    s
}

#[pyproto]
impl PyGCProtocol for AzMarshaledLayoutCallback {
    fn __traverse__(&self, visit: PyVisit) -> Result<(), PyTraverseError> {

        let data: &azul_impl::callbacks::MarshaledLayoutCallback = unsafe { mem::transmute(self) };

        // temporary clone since we can't borrow mutable here
        let mut refany = data.marshal_data.clone();

        let data = match refany.downcast_ref::<LayoutCallbackTy>() {
            Some(s) => s,
            None => return Ok(()),
        };

        if let Some(obj) = data._py_layout_callback.as_ref() {
            visit.call(obj)?;
        }

        Ok(())
    }

    fn __clear__(&mut self) {

        let mut data: &mut azul_impl::callbacks::MarshaledLayoutCallback = unsafe { mem::transmute(self) };

        let mut data = match data.marshal_data.downcast_mut::<LayoutCallbackTy>() {
            Some(s) => s,
            None => return,
        };

        if data._py_layout_callback.as_mut().is_some() {
            // Clear reference, this decrements Python ref counter.
            data._py_layout_callback = None;
        }
    }
}

#[repr(C)]
pub struct IFrameCallbackTy {
    _py_iframe_data: Option<PyObject>,
    _py_iframe_callback: Option<PyObject>,
}

#[pyproto]
impl PyGCProtocol for AzIFrameNode {
    fn __traverse__(&self, visit: PyVisit) -> Result<(), PyTraverseError> {

        let data: &azul_impl::dom::IFrameNode = unsafe { mem::transmute(self) };

        // temporary clone since we can't borrow mutable here
        let mut refany = data.data.clone();

        let data = match refany.downcast_ref::<IFrameCallbackTy>() {
            Some(s) => s,
            None => return Ok(()),
        };

        if let Some(obj) = data._py_iframe_data.as_ref() {
            visit.call(obj)?;
        }

        if let Some(obj) = data._py_iframe_callback.as_ref() {
            visit.call(obj)?;
        }

        Ok(())
    }

    fn __clear__(&mut self) {

        let mut data: &mut azul_impl::dom::IFrameNode = unsafe { mem::transmute(self) };

        let mut data = match data.data.downcast_mut::<IFrameCallbackTy>() {
            Some(s) => s,
            None => return,
        };

        if data._py_iframe_data.as_mut().is_some() {
            // Clear reference, this decrements Python ref counter.
            data._py_iframe_data = None;
        }

        if data._py_iframe_callback.as_mut().is_some() {
            // Clear reference, this decrements Python ref counter.
            data._py_iframe_callback = None;
        }
    }
}

extern "C" fn invoke_python_iframe(data: &mut AzRefAny, info: AzIFrameCallbackInfo) -> AzIFrameCallbackReturn {

    let default: AzIFrameCallbackReturn = unsafe { mem::transmute(azul_impl::callbacks::IFrameCallbackReturn {
         dom: azul_impl::styled_dom::StyledDom::default(),
         scroll_size: azul_impl::window::LogicalSize::new(0.0, 0.0),
         scroll_offset: azul_impl::window::LogicalPosition::new(0.0, 0.0),
         virtual_scroll_size: azul_impl::window::LogicalSize::new(0.0, 0.0),
         virtual_scroll_offset: azul_impl::window::LogicalPosition::new(0.0, 0.0),
    }) };

    let data: &mut azul_impl::callbacks::RefAny = unsafe { mem::transmute(data) };

    let mut iframe_cb = match data.downcast_mut::<IFrameCallbackTy>() {
        Some(s) => s,
        None => return default,
    };

    let mut iframe_cb = &mut *iframe_cb;

    let mut py_data = match iframe_cb._py_iframe_data.as_mut() {
        Some(s) => s,
        None => return default,
    };

    let mut py_function = match iframe_cb._py_iframe_callback.as_mut() {
        Some(s) => s,
        None => return default,
    };

    // call iframe callback into python
    let s: AzIFrameCallbackReturn = Python::with_gil(|py| {
        match py_function.call1(py.clone(), (py_data.clone_ref(py.clone()), info)) {
            Ok(o) => match o.as_ref(py).extract::<AzIFrameCallbackReturn>() {
                Ok(o) => o.clone(),
                Err(e) => {
                    #[cfg(feature = "logging")] {
                        let cb_any = o.as_ref(py);
                        let type_name = cb_any.get_type().name().unwrap_or("<unknown>");
                        log::error!("ERROR: LayoutCallback returned object of type {}, expected azul.callbacks.AzIFrameCallbackReturn", type_name);
                    }
                    default
                }
            },
            Err(e) => {
                #[cfg(feature = "logging")] {
                    log::error!("Exception caught when invoking IFrameCallback: {}", e);
                }
                default
            }
        }
    });

    s
}

#[repr(C)]
pub struct CallbackTy {
    _py_callback: Option<PyObject>,
    _py_data: Option<PyObject>,
}

#[pyproto]
impl PyGCProtocol for AzCallbackData {
    fn __traverse__(&self, visit: PyVisit) -> Result<(), PyTraverseError> {

        let data: &azul_impl::dom::CallbackData = unsafe { mem::transmute(self) };

        // temporary clone since we can't borrow mutable here
        let mut refany = data.data.clone();

        let data = match refany.downcast_ref::<CallbackTy>() {
            Some(s) => s,
            None => return Ok(()),
        };

        if let Some(obj) = data._py_callback.as_ref() {
            visit.call(obj)?;
        }

        if let Some(obj) = data._py_data.as_ref() {
            visit.call(obj)?;
        }

        Ok(())
    }

    fn __clear__(&mut self) {

        let mut data: &mut azul_impl::dom::CallbackData = unsafe { mem::transmute(self) };

        let mut data = match data.data.downcast_mut::<CallbackTy>() {
            Some(s) => s,
            None => return,
        };

        if data._py_callback.as_mut().is_some() {
            // Clear reference, this decrements Python ref counter.
            data._py_callback = None;
        }

        if data._py_data.as_mut().is_some() {
            // Clear reference, this decrements Python ref counter.
            data._py_data = None;
        }
    }
}

extern "C" fn invoke_python_callback(data: &mut AzRefAny, info: AzCallbackInfo) -> AzUpdate {

    let mut default: AzUpdate = AzUpdate::DoNothing;

    let data: &mut azul_impl::callbacks::RefAny = unsafe { mem::transmute(data) };

    let mut cb = match data.downcast_mut::<CallbackTy>() {
        Some(s) => s,
        None => return default,
    };

    let mut cb = &mut *cb;

    let mut py_data = match cb._py_data.as_mut() {
        Some(s) => s,
        None => return default,
    };

    let mut py_function = match cb._py_callback.as_mut() {
        Some(s) => s,
        None => return default,
    };

    // call callback into python
    let s: AzUpdate = Python::with_gil(|py| {
        match py_function.call1(py.clone(), (py_data.clone_ref(py.clone()), info)) {
            Ok(o) => match o.as_ref(py).extract::<AzUpdateEnumWrapper>() {
                Ok(o) => unsafe{ mem::transmute(o.clone()) },
                Err(e) => {
                    #[cfg(feature = "logging")] {
                        let cb_any = o.as_ref(py);
                        let type_name = cb_any.get_type().name().unwrap_or("<unknown>");
                        log::error!("ERROR: Callback returned object of type {}, expected azul.callbacks.Update", type_name);
                    }
                    default
                }
            },
            Err(e) => {
                #[cfg(feature = "logging")] {
                    log::error!("Exception caught when invoking Callback: {}", e);
                }
                default
            }
        }
    });

    s
}

#[repr(C)]
pub struct TimerCallbackTy {
    _py_timer_callback: Option<PyObject>,
    _py_timer_data: Option<PyObject>,
}

#[pyproto]
impl PyGCProtocol for AzTimer {
    fn __traverse__(&self, visit: PyVisit) -> Result<(), PyTraverseError> {

        let data: &azul_impl::task::Timer = unsafe { mem::transmute(self) };

        // temporary clone since we can't borrow mutable here
        let mut refany = data.data.clone();

        let data = match refany.downcast_ref::<TimerCallbackTy>() {
            Some(s) => s,
            None => return Ok(()),
        };

        if let Some(obj) = data._py_timer_callback.as_ref() {
            visit.call(obj)?;
        }

        if let Some(obj) = data._py_timer_data.as_ref() {
            visit.call(obj)?;
        }

        Ok(())
    }

    fn __clear__(&mut self) {

        let mut data: &mut azul_impl::task::Timer = unsafe { mem::transmute(self) };

        let mut data = match data.data.downcast_mut::<TimerCallbackTy>() {
            Some(s) => s,
            None => return,
        };

        if data._py_timer_callback.as_mut().is_some() {
            // Clear reference, this decrements Python ref counter.
            data._py_timer_callback = None;
        }

        if data._py_timer_data.as_mut().is_some() {
            // Clear reference, this decrements Python ref counter.
            data._py_timer_data = None;
        }
    }
}

#[repr(C)]
pub struct ImageCallbackTy {
    _py_image_callback: Option<PyObject>,
    _py_image_data: Option<PyObject>,
}

#[pyproto]
impl PyGCProtocol for AzImageRef {
    fn __traverse__(&self, visit: PyVisit) -> Result<(), PyTraverseError> {

        let data: &azul_impl::resources::ImageRef = unsafe { mem::transmute(self) };

        let image_callback = match data.get_image_callback() {
            Some(s) => s,
            None => return Ok(()),
        };

        // temporary clone since we can't borrow mutable here
        let mut refany = image_callback.data.clone();

        let data = match refany.downcast_ref::<ImageCallbackTy>() {
            Some(s) => s,
            None => return Ok(()),
        };

        if let Some(obj) = data._py_image_callback.as_ref() {
            visit.call(obj)?;
        }

        if let Some(obj) = data._py_image_data.as_ref() {
            visit.call(obj)?;
        }

        Ok(())
    }

    fn __clear__(&mut self) {

        let mut data: &mut azul_impl::resources::ImageRef = unsafe { mem::transmute(self) };

        let image_callback = match data.get_image_callback_mut() {
            Some(s) => s,
            None => return,
        };

        let mut data = match image_callback.data.downcast_mut::<ImageCallbackTy>() {
            Some(s) => s,
            None => return,
        };

        if data._py_image_callback.as_mut().is_some() {
            // Clear reference, this decrements Python ref counter.
            data._py_image_callback = None;
        }

        if data._py_image_data.as_mut().is_some() {
            // Clear reference, this decrements Python ref counter.
            data._py_image_data = None;
        }
    }
}

#[repr(C)]
pub struct ThreadWriteBackCallbackTy {
    _py_thread_callback: Option<PyObject>,
    _py_thread_data: Option<PyObject>,
}

#[pyproto]
impl PyGCProtocol for AzThread {
    fn __traverse__(&self, visit: PyVisit) -> Result<(), PyTraverseError> {

        let data: &azul_impl::task::Thread = unsafe { mem::transmute(self) };

        let mut thread_inner = match data.ptr.try_lock().ok() {
            Some(o) => o,
            None => return Ok(()),
        };

        let mut data = match thread_inner.writeback_data.downcast_mut::<ThreadWriteBackCallbackTy>() {
            Some(s) => s,
            None => return Ok(()),
        };

        if let Some(obj) = data._py_thread_callback.as_ref() {
            visit.call(obj)?;
        }

        if let Some(obj) = data._py_thread_data.as_ref() {
            visit.call(obj)?;
        }

        Ok(())
    }

    fn __clear__(&mut self) {

        let mut data: &mut azul_impl::task::Thread = unsafe { mem::transmute(self) };

        let mut thread_inner = match data.ptr.try_lock().ok() {
            Some(o) => o,
            None => return,
        };

        let mut data = match thread_inner.writeback_data.downcast_mut::<ThreadWriteBackCallbackTy>() {
            Some(s) => s,
            None => return,
        };

        if data._py_thread_callback.as_mut().is_some() {
            // Clear reference, this decrements Python ref counter.
            data._py_thread_callback = None;
        }

        if data._py_thread_data.as_mut().is_some() {
            // Clear reference, this decrements Python ref counter.
            data._py_thread_data = None;
        }
    }
}

#[repr(C)]
pub struct DatasetTy {
    _py_data: Option<PyObject>,
}

#[pyproto]
impl PyGCProtocol for AzNodeData {
    fn __traverse__(&self, visit: PyVisit) -> Result<(), PyTraverseError> {

        let data: &azul_impl::dom::NodeData = unsafe { mem::transmute(self) };

        // temporary clone since we can't borrow mutable here
        let dataset = match data.get_dataset().as_ref() {
            Some(s) => s,
            None => return Ok(()),
        };

        let mut refany = dataset.clone();

        let data = match refany.downcast_ref::<DatasetTy>() {
            Some(s) => s,
            None => return Ok(()),
        };

        if let Some(obj) = data._py_data.as_ref() {
            visit.call(obj)?;
        }

        Ok(())
    }

    fn __clear__(&mut self) {

        let mut data: &mut azul_impl::dom::NodeData = unsafe { mem::transmute(self) };

        let dataset = match data.get_dataset_mut().as_mut() {
            Some(s) => s,
            None => return,
        };

        let mut data = match dataset.downcast_mut::<DatasetTy>() {
            Some(s) => s,
            None => return,
        };

        if data._py_data.as_mut().is_some() {
            // Clear reference, this decrements Python ref counter.
            data._py_data = None;
        }
    }
}