    // impl App {

    #[new]
    pub fn new(py: Python, data: PyObject, config: AzAppConfig) -> Result<Self, PyErr> {
        use pyo3::type_object::PyTypeInfo;

        if data.as_ref(py).is_callable() {
            return Err(PyException::new_err(format!("ERROR in App.new: - argument \"data\" is a function callback, expected class")));
        }

        let app_refany = azul_impl::callbacks::RefAny::new(AppDataTy { _py_app_data: Some(data) });
        Ok(unsafe { mem::transmute(crate::AzApp_new(app_refany, mem::transmute(config))) })
    }