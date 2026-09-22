---
slug: architecture/gui-builder
title: GUI Builder (AzBuilder)
language: en
canonical_slug: architecture/gui-builder
audience: external
maturity: experimental
guide_order: 43
topic_only: false
short_desc: Using the visual drag-and-drop builder
prerequisites: [architecture/components]
default-search-keys:
  - ComponentLibrary
  - ComponentDef
  - ComponentDataModel
---

# GUI Builder

Azul ships with a built-in GUI Builder—a visual drag-and-drop tool similar to GTK's Glade 
or QtBuilder. Instead of a standalone heavy desktop application, the builder is hosted 
natively by your application and accessed through your web browser. 

This is the ultimate payoff for building your app using [Components](./components.md) 
and their explicit `ComponentDataModel` structures. Because every component explicitly 
declares its properties (strings, colors, toggles), the builder can automatically 
generate a UI to tweak them in real-time.

## Starting the Builder

To launch the builder, you simply need to run the `AzBuilder` demo from the releases page.
Internally, this simply opens up an empty application with a debug server listening on 
`http://localhost:8080` - since the HTTP server (on a background thread) and the windows 
of the application (on the main thread) share the same process, they can internally pass
events around via a message queue.

When the native application starts, it will render a blank native window and automatically 
open `http://localhost:8080` in your default browser.

## The Interface

The browser interface acts as a "remote control" for your native application window.

![GUI Builder Interface](../../images/gui_builder_import.jpg)

The key here is that you can do all actions either via clicking in the GUI or also via `curl` commands:

```
curl 
```

### 1. Component Library (Left Sidebar)

The left sidebar lists all the components currently registered in your `AppConfig`'s component libraries (including the `builtin` HTML elements and any custom components you have registered). You can drag and drop these components directly into the center canvas structure.

### 2. Live Native Preview 

As you construct your component tree in the browser, the native window instantly updates to reflect the changes. Because the browser communicates with the native app via WebSockets, the native app handles all the actual rendering, layout, and styling. The browser simply manages the logical XML tree and data models.

### 3. Properties Panel (Right Sidebar)

![GUI Builder Properties](../../images/gui_builder_properties.jpg)

When you select a component, the right sidebar populates with all the fields defined in its `ComponentDataModel`. For example, if your custom component has a `ComponentFieldType::String` called "title", you will see a text input here. When you change the value, the new data model is pushed to the native app, which re-runs your `render_fn` and instantly updates the native window.

## Code Generation (Export)

Once you are satisfied with the visual layout of your UI, you do not need to manually 
write the code to recreate it. 

By clicking **Export** (or using the `compile_fn` pipeline), the builder takes the customized 
data models and generates the raw source code in your target language (Rust, Go, C, etc.). 
You can paste this code directly back into your project.

This completes the workflow: 

1. Define your component model.
2. visually assemble your layout using the browser interface.
3. Export the finished code back into your application.

