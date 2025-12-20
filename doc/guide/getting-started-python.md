# Getting Started with Python

This guide shows how to create your first Azul application in Python.

## Quick Start

Install Azul from PyPI:

```bash
pip install azul
```

Or download from [releases](https://azul.rs/releases).

## Hello World

```python
from azul import *

# Your application state
class MyApp:
    def __init__(self):
        self.counter = 0

# Layout function - converts state to UI
def layout(data, info):
    return Dom.body() \
        .with_child(Dom.text(f"Counter: {data.counter}")) \
        .style(Css.empty())

# Run the app
app = App(MyApp(), AppConfig(layout))
app.run(WindowCreateOptions.default())
```

## Adding Callbacks

```python
from azul import *

class MyApp:
    def __init__(self):
        self.counter = 0

# Callback function
def on_click(data, info):
    data.counter += 1
    return Update.RefreshDom

def layout(data, info):
    button = Dom.div() \
        .with_inline_style("padding: 10px 20px; background: #4a90e2; color: white; cursor: pointer;") \
        .with_child(Dom.text("Click me!"))
    
    # Attach callback
    event = EventFilter.Hover(HoverEventFilter.MouseUp)
    button.add_callback(event, data, on_click)
    
    return Dom.body() \
        .with_child(Dom.text(f"Counter: {data.counter}")) \
        .with_child(button) \
        .style(Css.empty())

app = App(MyApp(), AppConfig(layout))
app.run(WindowCreateOptions.default())
```

## RefAny and Data Types

Any Python object can be used where `RefAny` is required:

```python
class MyData:
    def __init__(self):
        self.users = ["Alice", "Bob", "Charlie"]
        self.selected = 0

# App::new() takes a RefAny - pass your Python object directly
app = App(MyData(), AppConfig(layout))
```

## Union Enums

Union enums have static constructors and a `match()` function:

```python
# Create union enum
size = OptionLogicalSize.Some(LogicalSize(600, 800))

# Pattern match
tag, payload = size.match()
if tag == "Some":
    print(f"Size: {payload.width} x {payload.height}")
elif tag == "None":
    print("No size")
```

## Constructors

Default constructors take all arguments in field order:

```python
# API: struct ColorU { r: u8, g: u8, b: u8, a: u8 }
color = ColorU(255, 255, 255, 255)  # r, g, b, a

# Named explicit constructor
dom = Dom(NodeType.Div)  # Not Dom.new(...)
```

## Vectors and Options

`*Vec` types accept Python lists, `*Option` types handle `None`:

```python
# Get monitors - returns list
monitors = app.get_monitors()
print(monitors[0])

# Optional return value
cursor_pos = callbackinfo.get_cursor_relative_to_viewport()
if cursor_pos is not None:
    print(f"Cursor at {cursor_pos.x}, {cursor_pos.y}")
```

## Debug Printing

All types implement `__str__()` and `__repr__()`:

```python
color = ColorU(255, 0, 0, 255)
print(color)  # ColorU { r: 255, g: 0, b: 0, a: 255 }
```

## Nested Struct Modification

Due to a PyO3 bug, nested struct modification requires copying:

```python
# This doesn't work:
window.state.flags.frame = WindowFrame.Maximized

# Workaround:
state = window.state.copy()
flags = state.flags.copy()
flags.frame = WindowFrame.Maximized
state.flags = flags
window.state = state
```

## Enum Comparison

Simple enums support comparison:

```python
align = LayoutAlignItems.Stretch
if align == LayoutAlignItems.Stretch:
    print("Aligned to stretch!")
```

## Complete Example

```python
from azul import *

class TodoApp:
    def __init__(self):
        self.items = ["Buy groceries", "Walk the dog", "Learn Azul"]
        self.new_item = ""

def on_add_item(data, info):
    if data.new_item.strip():
        data.items.append(data.new_item)
        data.new_item = ""
        return Update.RefreshDom
    return Update.DoNothing

def layout(data, info):
    items = [
        Dom.div()
            .with_inline_style("padding: 8px; border-bottom: 1px solid #ccc;")
            .with_child(Dom.text(item))
        for item in data.items
    ]
    
    return Dom.body() \
        .with_inline_style("padding: 20px; font-family: sans-serif;") \
        .with_child(Dom.div()
            .with_inline_style("font-size: 24px; margin-bottom: 20px;")
            .with_child(Dom.text("Todo List"))) \
        .with_children(items) \
        .style(Css.empty())

app = App(TodoApp(), AppConfig(layout))
app.run(WindowCreateOptions.default())
```

## Next Steps

- [CSS Styling](css-styling.html) - Supported CSS properties
- [Widgets](widgets.html) - Interactive components
- [Architecture](architecture.html) - Framework design

[Back to overview](https://azul.rs/guide)
