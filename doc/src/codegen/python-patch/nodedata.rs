// impl NodeData {

    #[staticmethod]
    fn iframe(py: Python, data: PyObject, callback: PyObject) -> Result<Self, PyErr> { // RefAny<IFrameCallbackTy>

        use pyo3::type_object::PyTypeInfo;

        if data.as_ref(py.clone()).is_callable() {
            return Err(PyException::new_err(format!("ERROR in Dom.iframe: - argument \"data\" is a function callback, expected class")));
        }

        let cb_any = callback.as_ref(py);
        if !cb_any.is_callable() {
            let type_name = cb_any.get_type().name().unwrap_or("<unknown>");
            return Err(PyException::new_err(format!("ERROR in Dom.iframe: - argument \"callback\" is of type \"{}\", expected function", type_name)));
        }

        let iframe_refany = azul_impl::callbacks::RefAny::new(IFrameCallbackTy {
            _py_iframe_data: Some(data),
            _py_iframe_callback: Some(callback),
        });

        Ok(unsafe { mem::transmute(crate::AzNodeData_iframe(iframe_refany, invoke_python_iframe)) })
    }

    fn set_dataset(&mut self, py: Python, dataset: PyObject) -> Result<(), PyErr> { // RefAny<DatasetTy>
        use pyo3::type_object::PyTypeInfo;

        if dataset.as_ref(py.clone()).is_callable() {
            return Err(PyException::new_err(format!("ERROR in Dom.set_dataset: - argument \"dataset\" is a function callback, expected class")));
        }

        let dataset_refany = azul_impl::callbacks::RefAny::new(DatasetTy {
            _py_data: Some(dataset),
        });

        crate::AzNodeData_setDataset(unsafe { mem::transmute(self) }, dataset_refany);

        Ok(())
    }

    fn with_dataset(&mut self, py: Python, dataset: PyObject) -> Result<Self, PyErr> { // RefAny<DatasetTy>
        self.set_dataset(py, dataset)?;
        let d: &mut azul_impl::dom::NodeData = unsafe { mem::transmute(self) };
        Ok(unsafe { mem::transmute(d.swap_with_default()) })
    }

    fn add_callback(&mut self, py: Python, event: AzEventFilterEnumWrapper, data: PyObject, callback: PyObject) -> Result<(), PyErr> { // RefAny<CallbackTy>
        use pyo3::type_object::PyTypeInfo;

        if data.as_ref(py.clone()).is_callable() {
            return Err(PyException::new_err(format!("ERROR in Dom.add_callback: - argument \"data\" is a function callback, expected class")));
        }

        let cb_any = callback.as_ref(py);
        if !cb_any.is_callable() {
            let type_name = cb_any.get_type().name().unwrap_or("<unknown>");
            return Err(PyException::new_err(format!("ERROR in Dom.add_callback: - argument \"callback\" is of type \"{}\", expected function", type_name)));
        }

        let callback_refany = azul_impl::callbacks::RefAny::new(CallbackTy {
            _py_callback: Some(callback),
            _py_data: Some(data),
        });

        unsafe {
            crate::AzNodeData_addCallback(
                mem::transmute(self),
                mem::transmute(event),
                callback_refany,
                invoke_python_callback
            );
        }

        Ok(())
    }

    fn with_callback(&mut self, py: Python, event: AzEventFilterEnumWrapper, data: PyObject, callback: PyObject) -> Result<Self, PyErr> { // RefAny<CallbackTy>
        self.add_callback(py, event, data, callback)?;
        let d: &mut azul_impl::dom::NodeData = unsafe { mem::transmute(self) };
        Ok(unsafe { mem::transmute(d.swap_with_default()) })
    }