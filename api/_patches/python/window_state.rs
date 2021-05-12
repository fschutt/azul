    // impl AzWindowState {

    #[new]
    pub fn __new__(py: Python, cb: PyObject) -> Result<Self, PyErr> {
        let layout_callback = AzLayoutCallbackEnumWrapper::__new__(py, cb)?;
        let window = azul_impl::window::WindowState {
            layout_callback: unsafe { mem::transmute(layout_callback) },
            .. Default::default()
        };
        Ok(unsafe { mem::transmute(window) })
    }