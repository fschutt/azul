    // impl LayoutCallbackEnumWrapper { ...

    #[new]
    pub fn __new__(py: Python, cb: PyObject) -> Result<Self, PyErr> {
        use pyo3::type_object::PyTypeInfo;

        {
            let cb_any = cb.as_ref(py);
            if !cb_any.is_callable() {
                let type_name = cb_any.get_type().name().unwrap_or("<unknown>");
                return Err(PyException::new_err(format!("ERROR in LayoutCallback.new: - argument \"cb\" is of type \"{}\", expected function", type_name)));
            }
        }

        Ok(Self {
            inner: AzLayoutCallback::Marshaled(AzMarshaledLayoutCallback {
                marshal_data: unsafe { mem::transmute(azul_impl::callbacks::RefAny::new(LayoutCallbackTy {
                    _py_layout_callback: Some(cb)
                })) },
                cb: AzMarshaledLayoutCallbackInner { cb: invoke_py_marshaled_layout_callback },
            }),
        })
    }