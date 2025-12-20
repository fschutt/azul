# Getting Started with C++

This guide shows how to create your first Azul application in C++.

## Quick Start

Download the pre-compiled library from [releases](https://azul.rs/releases) and include `azul.hpp` in your project.

## Hello World

```cpp
#include "azul.hpp"
#include <iostream>

// Your application state
class MyApp {
public:
    int counter = 0;
    MyApp() = default;
};

// Layout function - converts state to UI
StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto app = data.downcastRef<MyApp>();
    if (!app) {
        return StyledDom::default_();
    }
    
    auto text = "Counter: " + std::to_string(app->ptr.counter);
    
    return Dom::body()
        .withChild(Dom::text(String::fromStdString(text)))
        .style(Css::empty());
}

int main() {
    auto data = RefAny::new_(std::move(MyApp()));
    auto app = App::new_(std::move(data), AppConfig::new_((LayoutCallbackType)layout));
    app.run(WindowCreateOptions::default_());
    return 0;
}
```

## Enums

Enums use C++11 `enum class` for type safety:

```cpp
// Explicit constructors are static functions
auto window = WindowCreateOptions::default_(LayoutSolver::Default);
```

## Move Semantics

All by-value arguments require `std::move`:

```cpp
auto window = WindowCreateOptions::default_(LayoutSolver::Default);
app.run(std::move(window));
```

## Adding Callbacks

```cpp
// Callback function
Update onClick(RefAny& data, CallbackInfo& info) {
    auto app = data.downcastRefMut<MyApp>();
    if (app) {
        app->ptr.counter++;
        return Update::RefreshDom;
    }
    return Update::DoNothing;
}

// In layout function
auto button = Dom::div()
    .withInlineStyle(String::fromConstStr(
        "padding: 10px 20px; background: #4a90e2; color: white; cursor: pointer;"
    ))
    .withChild(Dom::text(String::fromConstStr("Click me!")));

// Attach callback
auto event = EventFilter::Hover(HoverEventFilter::MouseUp);
button.root.addCallback(event, data.clone(), (CallbackType)onClick);
```

## Constructors

Default constructors take arguments in field order:

```cpp
// API: struct ColorU { r: u8, g: u8, b: u8, a: u8 }
auto color = ColorU(/*r*/ 255, /*g*/ 255, /*b*/ 255, /*a*/ 255);
```

Explicit constructors are static functions:

```cpp
auto window = WindowCreateOptions::default_(LayoutSolver::Default);
```

## RefAny and Generics

No macros needed - use C++ templates:

```cpp
class MyStruct {
public:
    int field;
    MyStruct(int f) noexcept : field(f) {}
};

auto object = RefAny::new_(std::move(MyStruct(5)));

// Read-only access
if (auto ref = object.downcastRef<MyStruct>()) {
    std::cout << ref->ptr.field << std::endl;
}

// Mutable access
if (auto refMut = object.downcastRefMut<MyStruct>()) {
    refMut->ptr.field = 10;
}
```

## Vector Conversions

Convert `std::vector` without copying:

```cpp
auto scancodes = std::vector<int>{0, 1, 2, 3};
auto converted = ScanCodeVec::fromStdVector(std::move(scancodes));
auto backToStd = converted.intoStdVector();
```

Convert strings:

```cpp
auto stdString = std::string("Hello, Azul!");
auto azulString = String::fromStdString(std::move(stdString));
```

## Pattern Matching

Union enums have `constexpr` constructors and match functions for pattern matching:

```cpp
// Create a union enum - Exact() is a constexpr function
auto cursor = StyleCursorValue::Exact(StyleCursor::Grab);

// Const reference match (returns std::optional<const StyleCursor*>)
if (auto result = cursor.matchRefExact()) {
    std::cout << "Cursor is Grab" << std::endl;
}

// Mutable match (returns std::optional<StyleCursor* restrict>)
if (auto result = cursor.matchRefMutExact()) {
    *result = StyleCursor::Default;
}
```

## Compilation

```bash
# Linux
g++ -std=c++17 -o myapp myapp.cpp -L. -lazul -Wl,-rpath,'$ORIGIN'

# macOS
clang++ -std=c++17 -o myapp myapp.cpp -L. -lazul -Wl,-rpath,@loader_path

# Windows (MSVC)
cl /std:c++17 myapp.cpp azul.lib
```

## Next Steps

- [CSS Styling](css-styling.html) - Supported CSS properties
- [Widgets](widgets.html) - Interactive components
- [Architecture](architecture.html) - Framework design

[Back to overview](https://azul.rs/guide)
