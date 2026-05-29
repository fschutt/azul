/* Weak Python C-API stubs — Linux only, `python-extension` builds only.
 *
 * WHY THIS EXISTS (bug B1)
 * ------------------------
 * pyo3 with the `extension-module` feature deliberately does NOT link
 * libpython: the manylinux / cibuildwheel images have no shared libpython, and
 * a real CPython interpreter provides those symbols at import time. As a result
 * the built `libazul.so` carries ~80 UNDEFINED `Py*` / `_Py*` symbols.
 *
 * That is fine for `import azul`, but it broke every C/C++ app that links
 * `-lazul`: their linker reported "undefined reference to PyTuple_SetItem" etc.
 * We want ONE shipped `libazul.so` that works for BOTH Python import and C
 * linking (rather than a separate "azul-with-python" library).
 *
 * HOW IT WORKS
 * ------------
 * We compile WEAK, default-visibility definitions of exactly those symbols into
 * libazul.so. Then:
 *
 *   * `import azul`: the interpreter has already loaded libpython into the
 *     RTLD_GLOBAL global scope. When libazul.so is dlopen'd, the dynamic linker
 *     resolves libazul's *internal* references to `Py*` against the global scope
 *     FIRST, so libpython's STRONG definitions win and our weak ones are shadowed
 *     — the real CPython functions are called. (Verified contract; if a stub is
 *     ever reached it `__builtin_trap()`s loudly rather than corrupting state.)
 *
 *   * C/C++ app linking `-lazul` with NO libpython: the symbols are now DEFINED
 *     (weak) in libazul's dynamic symbol table, so the link succeeds. The stubs
 *     are never *called* because no code path reaches the Python module without
 *     a Python interpreter present.
 *
 * PLATFORM SCOPE: Linux/ELF only. macOS uses `-undefined dynamic_lookup` (set in
 * build.rs) and a two-level namespace where defining these here would bind
 * libazul's own references to the stubs and break `import azul`; Windows links
 * pythonXX.lib. So build.rs compiles this file only for `target_os = "linux"`.
 *
 * MAINTENANCE: the list below mirrors the abi3 symbols pyo3 references. CI runs
 * `nm -D --undefined-only libazul.so | grep -E ' (Py|_Py)'` after the python
 * build and fails if any remain — so a stale list is caught automatically; add
 * the reported symbol here (PYFUNC for functions, PYDATA for objects/types).
 */

#if defined(__linux__)

#define WEAK __attribute__((weak, visibility("default")))

/* A function symbol. Wrong arity/return is irrelevant: only the NAME is needed
 * to satisfy the linker, and the body is unreachable in a non-Python process. */
#define PYFUNC(name) WEAK void name(void) { __builtin_trap(); }

/* A data symbol (PyTypeObject / PyObject* exception / interpreter singletons).
 * Generously sized; never read in a non-Python process (libpython interposes
 * the real object when loaded as a module). */
#define PYDATA(name) WEAK char name[512] = {0};

/* ---- functions ---- */
PYFUNC(_Py_DecRef)
PYFUNC(_Py_IncRef)
PYFUNC(Py_GetVersion)
PYFUNC(Py_IsInitialized)
PYFUNC(PyBytes_AsString)
PYFUNC(PyBytes_Size)
PYFUNC(PyDict_Next)
PYFUNC(PyDict_Size)
PYFUNC(PyErr_Fetch)
PYFUNC(PyErr_GivenExceptionMatches)
PYFUNC(PyErr_NewExceptionWithDoc)
PYFUNC(PyErr_NormalizeException)
PYFUNC(PyErr_Print)
PYFUNC(PyErr_PrintEx)
PYFUNC(PyErr_Restore)
PYFUNC(PyErr_SetObject)
PYFUNC(PyErr_SetString)
PYFUNC(PyErr_WriteUnraisable)
PYFUNC(PyEval_RestoreThread)
PYFUNC(PyEval_SaveThread)
PYFUNC(PyException_GetCause)
PYFUNC(PyException_GetTraceback)
PYFUNC(PyException_SetCause)
PYFUNC(PyException_SetTraceback)
PYFUNC(PyFloat_AsDouble)
PYFUNC(PyFloat_FromDouble)
PYFUNC(PyGILState_Ensure)
PYFUNC(PyGILState_Release)
PYFUNC(PyImport_Import)
PYFUNC(PyInterpreterState_Get)
PYFUNC(PyInterpreterState_GetID)
PYFUNC(PyList_Append)
PYFUNC(PyList_New)
PYFUNC(PyLong_AsLong)
PYFUNC(PyLong_AsUnsignedLongLong)
PYFUNC(PyLong_FromLong)
PYFUNC(PyLong_FromSsize_t)
PYFUNC(PyLong_FromUnsignedLongLong)
PYFUNC(PyModule_Create2)
PYFUNC(PyNumber_Index)
PYFUNC(PyObject_Call)
PYFUNC(PyObject_CallNoArgs)
PYFUNC(PyObject_DelItem)
PYFUNC(PyObject_GC_UnTrack)
PYFUNC(PyObject_GenericGetDict)
PYFUNC(PyObject_GenericSetDict)
PYFUNC(PyObject_GetAttr)
PYFUNC(PyObject_GetItem)
PYFUNC(PyObject_Repr)
PYFUNC(PyObject_SetAttr)
PYFUNC(PyObject_SetAttrString)
PYFUNC(PyObject_SetItem)
PYFUNC(PyObject_Str)
PYFUNC(PyTraceBack_Print)
PYFUNC(PyTuple_GetItem)
PYFUNC(PyTuple_New)
PYFUNC(PyTuple_SetItem)
PYFUNC(PyTuple_Size)
PYFUNC(PyType_FromSpec)
PYFUNC(PyType_GenericAlloc)
PYFUNC(PyType_GetFlags)
PYFUNC(PyType_GetSlot)
PYFUNC(PyType_IsSubtype)
PYFUNC(PyUnicode_AsEncodedString)
PYFUNC(PyUnicode_AsUTF8AndSize)
PYFUNC(PyUnicode_FromStringAndSize)
PYFUNC(PyUnicode_InternInPlace)

/* ---- data (type objects, exception objects, interpreter singletons) ---- */
PYDATA(_Py_FalseStruct)
PYDATA(_Py_NoneStruct)
PYDATA(_Py_TrueStruct)
PYDATA(PyBaseObject_Type)
PYDATA(PyBool_Type)
PYDATA(PyList_Type)
PYDATA(PyLong_Type)
PYDATA(PyTuple_Type)
PYDATA(PyType_Type)
PYDATA(PyUnicode_Type)
PYDATA(PyExc_AttributeError)
PYDATA(PyExc_BaseException)
PYDATA(PyExc_ImportError)
PYDATA(PyExc_OverflowError)
PYDATA(PyExc_RuntimeError)
PYDATA(PyExc_SystemError)
PYDATA(PyExc_TypeError)
PYDATA(PyExc_ValueError)

#endif /* __linux__ */
