    // impl WindowCreateOptions {

    #[new]
    pub fn __new__(py: Python, cb: PyObject) -> Result<Self, PyErr> {
        let window = azul_core::window::WindowCreateOptions {
            state: unsafe { mem::transmute(AzWindowState::__new__(py, cb)?) },
            .. Default::default()
        };
        Ok(unsafe { mem::transmute(window) })
    }