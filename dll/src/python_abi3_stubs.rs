//! Weak-ish Python C-API fallback stubs — Linux only, `python-extension` builds.
//!
//! WHY THIS EXISTS (bug B1)
//! -----------------------
//! pyo3 with `extension-module` deliberately does NOT link libpython (manylinux
//! images have no shared libpython; a real interpreter provides the symbols at
//! import time). So the built `libazul.so` would carry ~85 UNDEFINED `Py*` /
//! `_Py*` symbols, which broke every C/C++ app linking `-lazul`
//! ("undefined reference to PyTuple_SetItem" …). We ship ONE `libazul.so` for
//! BOTH Python import and C linking by baking fallback definitions of exactly
//! those symbols into the cdylib.
//!
//! WHY RUST, NOT THE OLD `python_abi3_weak_stubs.c`
//! ------------------------------------------------
//! The C stubs were compiled into a static archive and pulled in with
//! `--whole-archive`. But rustc's cdylib export control emits an anonymous
//! version script `{ global: <Az*/az_*/PyInit_*>; local: *; }`, and `local: *`
//! LOCALIZES every symbol it does not recognise as a crate export — including
//! the C archive's `Py*` symbols. A LOCAL symbol is resolved intra-DSO and is
//! NEVER interposed by the global scope, so libazul's own (via pyo3) reference
//! to e.g. `PyInterpreterState_Get` bound to the trap stub instead of the
//! interpreter's real symbol → `import azul` SIGILL'd during module init
//! (pyo3 `ModuleDef::make_module`). No linker flag overrides `local: *`
//! (`--export-dynamic-symbol` / `--dynamic-list` do not; a second
//! `--version-script` is rejected by GNU ld as "anonymous version tag cannot be
//! combined with other version tags").
//!
//! The one mechanism that puts a symbol in rustc's `global:` section on stable
//! is a real crate `#[no_mangle]` item. Defining the stubs here makes rustc
//! export them GLOBAL (default visibility). pyo3 lives in a SEPARATE crate, so
//! its calls go through the PLT/GOT and remain preemptible — the interpreter's
//! strong symbols (CPython exports `PyInterpreterState_Get` et al. from the
//! statically-linked python executable's `.dynsym`, or from `libpython3.x.so`,
//! either of which precedes libazul in the global scope) interpose these
//! fallbacks. A stub is only ever reached with NO interpreter present, where it
//! aborts loudly rather than corrupting state.
//!
//! PLATFORM SCOPE: Linux/ELF only. macOS uses `-undefined dynamic_lookup` + a
//! two-level namespace (defining these would self-bind and break import) and
//! Windows links pythonXX.lib — so this module is `cfg`-gated to Linux in
//! lib.rs, matching the old build.rs `target_os = "linux"` guard.
//!
//! MAINTENANCE: the lists below mirror the abi3 symbols pyo3 references. CI runs
//! `nm -D --undefined-only libazul.so | grep -E ' (Py|_Py)'` after the python
//! build and fails if any remain — a stale list is caught automatically; add the
//! reported symbol to `py_fn_stubs!` (functions) or `py_data_stubs!` (objects).

/// A function symbol. Arity/return are irrelevant: only the NAME must exist to
/// satisfy the linker, and the body is unreachable in a process without a Python
/// interpreter (which would interpose the real symbol).
macro_rules! py_fn_stubs {
    ($($name:ident),* $(,)?) => {
        $(
            #[no_mangle]
            pub extern "C" fn $name() -> ! {
                // Reached only if libazul is loaded without a CPython
                // interpreter in the global scope to interpose this symbol.
                ::std::process::abort()
            }
        )*
    };
}

/// A data symbol (PyTypeObject / PyObject* exception / interpreter singletons).
/// Generously sized and 8-byte aligned; never read in a non-Python process
/// (libpython interposes the real object when loaded).
macro_rules! py_data_stubs {
    ($($name:ident),* $(,)?) => {
        $(
            #[no_mangle]
            pub static $name: [u64; 64] = [0; 64];
        )*
    };
}

// ---- functions ----
//
// DELIBERATELY OMITTED: the refcount ABI functions `_Py_IncRef` / `_Py_DecRef`.
// Under the limited API (abi3) pyo3-ffi's `Py_INCREF`/`Py_DECREF` call these
// symbols directly on the hottest path (every Bound drop). A stub definition
// for them here is *used* by libazul's own pyo3 code instead of the
// interpreter's real function (self-bound under the RTLD_LOCAL dlopen CPython
// uses for extension modules), which corrupts refcounting — a strong
// `_Py_DecRef` stub made `import azul` allocate without bound and OOM. They MUST
// stay undefined so they resolve to the live interpreter's implementation.
// (Verified with a minimal standalone pyo3 repro: a strong `_Py_DecRef` stub
// alone reproduces the runaway; removing it fixes it.) The `nm -D` "no undefined
// Py*" CI gate must allow exactly `_Py_IncRef`/`_Py_DecRef` as undefined — they
// are always provided by any interpreter and never referenced by a pure-C
// `-lazul` consumer.
py_fn_stubs! {
    Py_GetVersion, Py_IsInitialized,
    PyBytes_AsString, PyBytes_Size, PyDict_Next, PyDict_Size,
    PyErr_Fetch, PyErr_GivenExceptionMatches, PyErr_NewExceptionWithDoc,
    PyErr_NormalizeException, PyErr_Print, PyErr_PrintEx, PyErr_Restore,
    PyErr_SetObject, PyErr_SetString, PyErr_WriteUnraisable,
    PyEval_RestoreThread, PyEval_SaveThread,
    PyException_GetCause, PyException_GetTraceback, PyException_SetCause,
    PyException_SetTraceback,
    PyFloat_AsDouble, PyFloat_FromDouble,
    PyGILState_Ensure, PyGILState_Release,
    PyImport_Import, PyInterpreterState_Get, PyInterpreterState_GetID,
    PyList_Append, PyList_New,
    PyLong_AsLong, PyLong_AsUnsignedLongLong, PyLong_FromLong,
    PyLong_FromSsize_t, PyLong_FromUnsignedLongLong,
    PyModule_Create2, PyNumber_Index,
    PyObject_Call, PyObject_CallNoArgs, PyObject_DelItem, PyObject_GC_UnTrack,
    PyObject_GenericGetDict, PyObject_GenericSetDict, PyObject_GetAttr,
    PyObject_GetItem, PyObject_Repr, PyObject_SetAttr, PyObject_SetAttrString,
    PyObject_SetItem, PyObject_Str,
    PyTraceBack_Print,
    PyTuple_GetItem, PyTuple_New, PyTuple_SetItem, PyTuple_Size,
    PyType_FromSpec, PyType_GenericAlloc, PyType_GetFlags, PyType_GetSlot,
    PyType_IsSubtype,
    PyUnicode_AsEncodedString, PyUnicode_AsUTF8AndSize,
    PyUnicode_FromStringAndSize, PyUnicode_InternInPlace,
}

// ---- data (type objects, exception objects, interpreter singletons) ----
py_data_stubs! {
    _Py_FalseStruct, _Py_NoneStruct, _Py_TrueStruct,
    PyBaseObject_Type, PyBool_Type, PyList_Type, PyLong_Type, PyTuple_Type,
    PyType_Type, PyUnicode_Type,
    PyExc_AttributeError, PyExc_BaseException, PyExc_ImportError,
    PyExc_OverflowError, PyExc_RuntimeError, PyExc_SystemError,
    PyExc_TypeError, PyExc_ValueError,
}
