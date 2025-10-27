# Azul - Desktop GUI framework

<!-- [START badges] -->
[![CI](https://github.com/fschutt/azul/actions/workflows/rust.yml/badge.svg)](https://github.com/fschutt/azul/actions/workflows/rust.yml)
[![Coverage Status](https://coveralls.io/repos/github/fschutt/azul/badge.svg?branch=master)](https://coveralls.io/github/fschutt/azul?branch=master)
[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust Compiler Version](https://img.shields.io/badge/rustc-1.58%20stable-blue.svg)]()
[![dependency status](https://deps.rs/repo/github/fschutt/azul/status.svg)](https://deps.rs/repo/github/fschutt/azul)
<!-- [END badges] -->

> Azul is a free, functional, reactive GUI framework for Rust, C and C++,
built using the WebRender rendering engine and a CSS / HTML-like document
object model for rapid development of beautiful, native desktop applications

###### [Website](https://azul.rs/) | [Releases](https://azul.rs/releases) | [User guide](https://azul.rs/guide) | [API documentation](https://azul.rs/api) | [Video demo](https://www.youtube.com/watch?v=kWL0ehf4wwI) | [Matrix Chat](https://discord.gg/nxUmsCG)

## Features

Azul uses [webrender](https://github.com/servo/webrender) (the rendering engine behind 
Firefox) to render your UI, so it supports lots of common CSS features like:

- gradients (linear, radial, conic)
- box shadows
- SVG filters
- composition operators (multiply, darken, etc.)
- border styling
- border-radii
- scrolling / automatic overflow
- CSS transforms

See the [list of supported CSS keys / values](https://azul.rs/guide/1.0.0-alpha1/CSSstyling) for more info.

On top of that, Azul features...

- lots of built-in widgets ([Button](https://azul.rs/api/1.0.0-alpha1#st.Button), [TextInput](https://azul.rs/api/1.0.0-alpha1#st.TextInput), [CheckBox](https://azul.rs/api/1.0.0-alpha1#st.CheckBox), [ColorInput](https://azul.rs/api/1.0.0-alpha1#st.ColorInput), [TextInput](https://azul.rs/api/1.0.0-alpha1#st.TextInput), [NumberInput](https://azul.rs/api/1.0.0-alpha1#st.NumberInput))
- embedding OpenGL textures
- simplified HTML-like relative/absolute layout system based on CSS flexbox
- 60+ FPS animations via [Animation](https://azul.rs/api/1.0.0-alpha1#st.Animation) API
- cross-platform native dialogs
- cross-platform text shaping and rendering
- SVG parsing and rendering
- shape tesselation for rendering large numbers of 2D lines, circles, rects, shapes, etc. in a single draw call
- managing off-main-thread tasks for I/O
- dynamic linking via shared library\*
- usable from Rust, C, C++ and Python via auto-generated API bindings\**
- HTML-to-Rust compilation for fast prototyping / hot reload

\* static linking not yet available

\** C++ bindings and Python are not yet stabilized and might not work depending
on the branch you're using. They will be stabilized before the release.

## Screenshots 

![image](https://user-images.githubusercontent.com/12084016/129535820-ca2b56a6-fdb5-4d0d-b043-a7f5394339e9.png)
![image](https://user-images.githubusercontent.com/12084016/129535780-69b9365b-ad87-439f-9d10-d416991de8fc.png)
![image](https://user-images.githubusercontent.com/12084016/128639991-e98c0b92-66df-4ad8-973b-c9d45c68d5b3.png)
![image](https://user-images.githubusercontent.com/12084016/126752996-1ec1f221-2b01-4f01-99c6-794640228d59.png)

## Hello World

### Python

```py
from azul import *

class DataModel:
    def __init__(self, counter):
        self.counter = counter

def render_dom(data, info):
    
    label = Dom.text("{}".format(data.counter))
    label.set_inline_style("font-size: 50px;")
    
    button = Button("Increment counter")
    button.set_on_click(data, increment_counter)

    dom = Dom.body()
    dom.add_child(label)
    dom.add_child(button.dom())

    return dom.style(Css.empty())

def increment_counter(data, info):
    data.counter += 1;
    return Update.RefreshDom

app = App(DataModel(5), AppConfig(LayoutSolver.Default))
app.run(WindowCreateOptions(render_dom))
```

### Rust

```rust
use azul::prelude::*;
use azul::widgets::{button::Button, label::Label};

struct DataModel {
    counter: usize,
}

extern "C" 
fn render_dom(data: &mut RefAny, _: &mut LayoutInfo) -> StyledDom {

    let data = data.downcast_ref::<DataModel>()?;

    let label = Dom::text(format!("{}", data.counter))
        .with_inline_style("font-size: 50px;");
        
    let button = Button::new("Increment counter")
        .onmouseup(increment_counter, data.clone());

    Dom::body()
    .with_child(label)
    .with_child(button.dom())
    .style(Css::empty())
}

extern "C" 
fn increment_counter(data: &mut RefAny, _: &mut CallbackInfo) -> Update {
    let mut data = data.downcast_mut::<DataModel>()?;
    data.counter += 1;
    Update::RefreshDom // call render_dom() again
}

fn main() {
    let initial_data = RefAny::new(DataModel { counter: 0 });
    let app = App::new(initial_data, AppConfig::default());
    app.run(WindowCreateOptions::new(render_dom));
}
```

### C

```c
#include "azul.h"

typedef struct {
    uint32_t counter;
} DataModel;

void DataModel_delete(DataModel* restrict A) { }
AZ_REFLECT(DataModel, DataModel_delete);

AzStyledDom render_dom(AzRefAny* data, AzLayoutInfo* info) {

    DataModelRef d = DataModelRef_create(data);
    if !(DataModel_downcastRef(data, &d)) {
        return AzStyledDom_empty();
    }
    
    char buffer [20];
    int written = snprintf(buffer, 20, "%d", d->counter);
    AzString const labelstring = AzString_copyFromBytes(&buffer, 0, written);
    AzDom label = AzDom_text(labelstring);
    AzString const inline_css = AzString_fromConstStr("font-size: 50px;");
    AzDom_setInlineStyle(&label, inline_css);
    
    AzString const buttontext = AzString_fromConstStr("Increment counter");
    AzButton button = AzButton_new(buttontext, AzRefAny_clone(data));
    AzButton_setOnClick(&button, incrementCounter);

    AzDom body = Dom_body();
    AzDom_addChild(body, AzButton_dom(&button));
    AzDom_addChild(body, label);
    
    AzCss global_css = AzCss_empty();
    return AzDom_style(body, global_css);
}

Update incrementCounter(RefAny* data, CallbackInfo* event) {
    DataModelRefMut d = DataModelRefMut_create(data);
    if !(DataModel_downcastRefMut(data, &d)) {
        return Update_DoNothing;
    }
    d->ptr.counter += 1;
    DataModelRefMut_delete(&d);
    return Update_RefreshDom;
}

int main() {
    DataModel model = { .counter = 5 };
    AzApp app = AzApp_new(DataModel_upcast(model), AzAppConfig_default());
    AzApp_run(app, AzWindowCreateOptions_new(render_dom));
    return 0;
}
```

## License

Azul is licensed under the MPL-2.0. Which means that yes, you can build 
proprietary applications using azul without having to publish your code: 
you only have to publish changes made to *the library itself*.

Copyright 2017 - current Felix Schütt
