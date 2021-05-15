// impl Dom {
    // data: RefAny, callback: IFrameCallbackType) -> Self {

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

        Ok(unsafe { mem::transmute(crate::AzDom_iframe(iframe_refany, invoke_python_iframe)) })
    }

    #[staticmethod]
    fn set_dataset(&mut self, dataset: PyObject) { // RefAny<DataSetTy>

    }

    #[staticmethod]
    fn with_dataset(&mut self, dataset: PyObject) -> Dom { // RefAny<DataSetTy>

    }

    #[staticmethod]
    fn add_callback(&mut self, dataset: PyObject) { // RefAny<CallbackTy>

    }

    #[staticmethod]
    fn with_callback(&mut self, dataset: PyObject) -> Dom { // RefAny<CallbackTy>

    }