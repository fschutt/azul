    // impl Button {

    fn set_on_click(&mut self, py: Python, data: PyObject, callback: PyObject) -> Result<(), PyErr> { // RefAny<CallbackTy>
        use pyo3::type_object::PyTypeInfo;

        if data.as_ref(py.clone()).is_callable() {
            return Err(PyException::new_err(format!("ERROR in Button.set_on_click: - argument \"data\" is a function callback, expected class")));
        }

        let cb_any = callback.as_ref(py);
        if !cb_any.is_callable() {
            let type_name = cb_any.get_type().name().unwrap_or("<unknown>");
            return Err(PyException::new_err(format!("ERROR in Button.set_on_click: - argument \"callback\" is of type \"{}\", expected function", type_name)));
        }

        let callback_refany = azul_core::refany::RefAny::new(CallbackTy {
            _py_callback: Some(callback),
            _py_data: Some(data),
        });

        unsafe {
            crate::AzButton_setOnClick(
                mem::transmute(self),
                callback_refany,
                invoke_python_callback
            );
        }

        Ok(())
    }

    fn with_on_click(&mut self, py: Python, data: PyObject, callback: PyObject) -> Result<Self, PyErr> { // RefAny<CallbackTy>
        self.set_on_click(py, data, callback)?;
        let d: &mut crate::widgets::button::Button = unsafe { mem::transmute(self) };
        Ok(unsafe { mem::transmute(d.swap_with_default()) })
    }
