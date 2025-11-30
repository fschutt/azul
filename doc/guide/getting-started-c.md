# Getting Started with C

This guide shows how to create your first Azul application in C.

## Quick Start

Download the pre-compiled library from [releases](https://azul.rs/releases) and include `azul.h` in your project.

## Hello World

```c
#include "azul.h"

// Your application state
typedef struct {
    int counter;
} MyApp;

// Destructor (required by AZ_REFLECT)
void MyApp_destructor(MyApp* instance) { }
AZ_REFLECT(MyApp, MyApp_destructor);

// Layout function - converts state to UI
AzStyledDom layout(AzRefAny* data, AzLayoutCallbackInfo info) {
    MyAppRef app_ref = MyAppRef_create(data);
    MyApp* app;
    if (!MyApp_downcastRef(data, &app_ref)) {
        return AzStyledDom_default();
    }
    
    char text[64];
    snprintf(text, sizeof(text), "Counter: %d", app_ref.ptr->counter);
    
    AzDom body = AzDom_body();
    AzDom_addChild(&body, AzDom_text(AzString_fromConstStr(text)));
    
    MyAppRef_delete(&app_ref);
    return AzDom_style(body, AzCssApiWrapper_empty());
}

int main() {
    MyApp initial_state = { .counter = 0 };
    AzRefAny data = MyApp_upcast(initial_state);
    
    AzApp app = AzApp_new(data, AzAppConfig_new((AzLayoutCallbackType)layout));
    AzApp_run(app, AzWindowCreateOptions_default());
    
    return 0;
}
```

## Naming Conventions

**Functions** are named `Az` + class name + `_` + function name:

```c
// Rust: app::App::new()
AzApp_new()

// Rust: dom::Dom::body()
AzDom_body()
```

**Enums** are named `Az` + enum name + `_` + variant name:

```c
// Rust: LayoutAlignItems::Stretch
AzLayoutAlignItems_Stretch

// Rust: Update::RefreshDom
AzUpdate_RefreshDom
```

**Union enums** use compile-time macros:

```c
AzStyleCursorValue cursor = AzStyleCursorValue_Exact(AzStyleCursor_Grab);
```

## Adding Callbacks

```c
// Callback function
AzUpdate on_click(AzRefAny* data, AzCallbackInfo* info) {
    MyAppRefMut app_ref = MyAppRefMut_create(data);
    if (MyApp_downcastRefMut(data, &app_ref)) {
        app_ref.ptr->counter++;
        MyAppRefMut_delete(&app_ref);
        return AzUpdate_RefreshDom;
    }
    return AzUpdate_DoNothing;
}

// In layout function
AzDom button = AzDom_div();
AzDom_setInlineStyle(&button, AzString_fromConstStr(
    "padding: 10px 20px; background: #4a90e2; color: white; cursor: pointer;"
));
AzDom_addChild(&button, AzDom_text(AzString_fromConstStr("Click me!")));

// Attach callback
AzEventFilter event = AzEventFilter_Hover(AzHoverEventFilter_MouseUp);
AzDom_addCallback(&button, event, *data, (AzCallbackType)on_click);
```

## Memory Management

Classes marked with "has destructor" have `_delete()` functions. Destructors 
automatically call sub-destructors for all fields (no need to recurse manually):

```c
AzApp app = AzApp_new(/* ... */);
// ... use app ...
AzApp_delete(&app);  // Cleans up app and all its contents
```

All classes can be deep-copied via `_deepCopy()` - note this can be expensive for large objects:

```c
AzWindowCreateOptions w = AzWindowCreateOptions_new();
AzWindowCreateOptions copy = AzWindowCreateOptions_deepCopy(&w);
```

## Pattern Matching

Use `_matchRef` and `_matchMut` functions to emulate Rust pattern matching:

```c
// Create a union enum
AzStyleCursorValue cursor = AzStyleCursorValue_Exact(AzStyleCursor_Grab);

// Destructure with const reference
AzStyleCursor* result;
if (AzStyleCursorValue_matchRefExact(&cursor, &result)) {
    printf("Cursor style: Grab\n");
    // result is initialized here and points to cursor's payload
}

// Destructure with mutable reference
AzStyleCursor* resultMut;
if (AzStyleCursorValue_matchRefMutExact(&cursor, &resultMut)) {
    *resultMut = AzStyleCursor_Default;  // Modify the value
}
```

The difference between `_matchRef()` and `_matchMut()` is that `_matchRef` 
takes a `const*` and `_matchMut()` takes a `restrict*` to the result. 
The lifetime of `result` is equal to the lifetime of `cursor` since `result` 
simply points to the payload of the tagged union.

## The AZ_REFLECT Macro

The `AZ_REFLECT` macro enables runtime type reflection for `RefAny`. It generates:
- `YourType_upcast()` - Convert your struct to RefAny
- `YourType_downcastRef()` - Checked const borrow from RefAny
- `YourType_downcastRefMut()` - Checked mutable borrow from RefAny

```c
typedef struct { int field; } MyStruct;

void MyStruct_delete(MyStruct* restrict A) { }  // Destructor
AZ_REFLECT(MyStruct, MyStruct_delete);

// Usage:
// Create a RefAny from your struct
AzRefAny object = MyStruct_upcast((MyStruct){ .field = 5 });

// Read-only access (const borrow)
MyStructRef structref = MyStructRef_create(&object);
if (MyStruct_downcastRef(&object, &structref)) {
    printf("field = %d\n", structref.ptr->field);  // prints "5"
}
MyStructRef_delete(&structref);  // Release the borrow

// Mutable access (restrict borrow)
// Cannot borrow ref and refmut simultaneously - checked at runtime!
MyStructRefMut structrefmut = MyStructRefMut_create(&object);
if (MyStruct_downcastRefMut(&object, &structrefmut)) {
    structrefmut.ptr->field = 6;  // Modify the value
}
MyStructRefMut_delete(&structrefmut);  // Release the borrow

// Clean up - decreases refcount, calls destructor when 0
MyStruct_delete(&object);
```

## Vectors and Strings

Create vectors at compile time:

```c
AzScanCodeVec v = AzScanCodeVec_empty();
AzString s = AzString_fromConstStr("hello");  // Compile-time string
```

## Compilation

```bash
# Linux
gcc -o myapp myapp.c -L. -lazul -Wl,-rpath,'$ORIGIN'

# macOS
clang -o myapp myapp.c -L. -lazul -Wl,-rpath,@loader_path

# Windows
cl myapp.c azul.lib
```

## Next Steps

- [CSS Styling](css-styling.html) - Supported CSS properties
- [Widgets](widgets.html) - Interactive components
- [Architecture](architecture.html) - Framework design

[Back to overview](https://azul.rs/guide)
