# Azul Callback System Architecture

## Overview

Azul uses a two-tier callback system to support both native Rust callbacks and 
language bindings (Python, C++, etc.). This document explains why this architecture
is necessary and how it works.

## The Problem

In Rust, we can pass function pointers directly:

```rust
extern "C" fn my_layout(data: RefAny, info: LayoutCallbackInfo) -> StyledDom {
    // ...
}

let window = WindowCreateOptions::create(LayoutCallback::create(my_layout));
```

But in Python, we have `Py<PyAny>` callable objects that:
1. Are managed by Python's GC
2. Need the GIL to call
3. Have a different calling convention than `extern "C"` functions

## The Solution: Callback Wrapper Structs

For each callback type, Azul defines:

1. **CallbackType** - The raw function pointer type
   ```rust
   pub type LayoutCallbackType = extern "C" fn(RefAny, LayoutCallbackInfo) -> StyledDom;
   ```

2. **Callback** - A wrapper struct pairing the function pointer with optional context
   ```rust
   pub struct LayoutCallback {
       pub cb: LayoutCallbackType,      // The actual function pointer
       pub ctx: OptionRefAny,           // Optional context (used for Python callables)
   }
   ```

## How Python Uses This

When Python code does:
```python
def my_layout(data, info):
    return Dom.body().style(Css.empty())

window = WindowCreateOptions(my_layout)
```

The Python codegen:

1. **Stores the callable**: Wraps `my_layout` in a `PyCallableWrapper` and stores it
   in the callback's `ctx` field as a `RefAny`

2. **Uses a trampoline**: Instead of `my_layout`, it passes a Rust function that:
   - Extracts the `PyCallableWrapper` from `ctx`
   - Acquires the GIL
   - Calls the Python callable
   - Converts the result back to Rust types

```rust
extern "C" fn invoke_py_layout_callback(
    app_data: AzRefAny,
    info: AzLayoutCallbackInfo
) -> AzStyledDom {
    // Get the Python callable from the ctx field
    let callable = info.get_ctx().downcast::<PyCallableWrapper>();
    
    Python::attach(|py| {
        // Call the Python function
        let result = callable.call1(py, (data_wrapper, info_wrapper));
        // Convert result back to Rust
        result.extract::<StyledDom>()
    })
}
```

## IR Metadata

The IR tracks callback information at multiple levels:

### 1. CallbackTypedefDef (for raw function pointer types)
```rust
pub struct CallbackTypedefDef {
    pub name: String,           // e.g., "LayoutCallbackType"
    pub args: Vec<CallbackArg>, // Function arguments
    pub return_type: Option<String>,
    pub external_path: Option<String>,
}
```

### 2. CallbackWrapperInfo (on structs that wrap callbacks)
```rust
pub struct CallbackWrapperInfo {
    pub callback_typedef_name: String,  // e.g., "LayoutCallbackType"
    pub callback_field_name: String,    // Usually "cb"
}
```

### 3. CallbackArgInfo (on function arguments)
```rust
pub struct CallbackArgInfo {
    pub callback_typedef_name: String,   // e.g., "CallbackType"
    pub callback_wrapper_name: String,   // e.g., "Callback" or "CoreCallback"
    pub trampoline_name: String,         // e.g., "invoke_py_callback"
}
```

## Implications for Rust API

For the **Rust** API, we want ergonomic usage:

```rust
// User writes a simple function
extern "C" fn my_callback(data: RefAny, info: CallbackInfo) -> Update { ... }

// And passes it directly (not wrapped in a struct)
dom.with_callback(EventFilter::Hover(...), data, my_callback);
```

The C-API function signature takes the **CallbackType** (function pointer) directly,
not the full **Callback** wrapper struct:

```rust
// C-API signature
pub fn AzDom_withCallback(
    dom: AzDom,
    filter: AzEventFilter,
    data: AzRefAny,
    callback: AzCallbackType,  // Just the fn pointer!
) -> AzDom;
```

The internal implementation constructs the full Callback struct:

```rust
// Internal implementation
pub fn withCallback(self, filter: EventFilter, data: RefAny, callback: CallbackType) -> Dom {
    let full_callback = Callback {
        cb: callback,
        ctx: OptionRefAny::None,  // No context needed for Rust
    };
    // ...
}
```

## Language-Specific Handling

| Language | Parameter Type | What User Passes | Internal Handling |
|----------|---------------|------------------|-------------------|
| Rust     | `CallbackType` (fn ptr) | `my_callback` | Used directly |
| Python   | `Py<PyAny>` | `my_callable` | Wrapped + trampoline |
| C/C++    | `CallbackType` (fn ptr) | Function pointer | Used directly |

## Codegen Rules

When generating method signatures:

1. **Check `arg.callback_info`** - If present, this arg is a callback type
2. **For Rust**: Use `CallbackType` (the fn pointer type) directly
3. **For Python**: Accept `Py<PyAny>`, generate trampoline wrapping code
4. **For C/C++**: Use the function pointer type

The IR provides `callback_info.callback_typedef_name` which is the fn pointer type
to use in the Rust API signature instead of the wrapper struct type.
