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

## Browser Interface

The browser interface acts as a "remote control" for your native application window. If 
you click on any node (such as "body"), this then activates the node in the debugger and
shows the properties. If there is a text field, then you can edit the property.

![AzBuilder GUI Builder Interface](../../images/debugger-inital.jpg)

If your applications `RefAny` has the JSON serialization and deserialization set up 
(e.g. in C via the `AZ_REFLECT_JSON` macro), then the top-left panel will also show you
the current state of the application data. With the click on the camera symbol you can create
a "named snapshot" of the application state, which you can then use in the E2E tests - so that
you can quickly set and restore a certain application state (remember that your UI is an 
`f(State) -> UI` function).

The "Components" panel allows you to drag and drop components into the DOM tree above it,
while you can edit the attributes of the selected component. Note that as you edit, the native
window will update itself, to show you the native preview of your UI. It is recommended to use
a second monitor here so you can put the browser tab with the debugger UI and the actual preview
side-to-side.

At the top, you have the menu bar with "Import" and "Export" - to save your current project, 
use "Export > Project" (same with "Import > Project" to load one).

## Slash Commands

Since the `AzBuilder` is merely an empty window listening on an HTTP port, 
you can control the window remotely via `curl`. The "browser UI" is merely 
a visual interface for firing `curl` commands:

```sh
# resize the AzBuilder window
curl -s -X POST http://localhost:8080/ -d '{"op": "resize", "width": 100, "height": 200}'
```

The various "op" operations are documented in the [Debugging Guide](../debugging.md), but
in the Browser UI you can fire off the commands from your browser by pressing "/" in the 
"Terminal" command line - which also shows you examples and arguments:

![CURL commands in the browser](../../images/slash-commands.jpg)

Important are the `"op": "take_screenshot"` and `"op": "take_native_screenshot"`, which return
Base64 PNG data - the browser UI can natively show this. The difference is that the `take_screenshot`
op only renders the content of the UI, without the system window chrome - so that you can save 
the image and use it in reference tests against regressions. The other op is meant more for taking 
"nice" screenshots for showcases or documentation.

The API is also extremely useful for AI agents (Claude / Codex / whatever), since agents can both 
run curl commands and visually inspect images.

## Designing E2E Tests

While his is better documented in the [E2E Testing](../debugging/e2e-testing.md) guide, 
the step from firing off single slash-commands to writing full end-to-end tests is so trivial, 
that the HTML UI comes with an entire UI to design your tests. End to end tests are nothing but
a series of `op` steps with an `op: assert_eq` or similar built in the middle. 

Since the `cpurender` backend can run headlessly, this enables us to run many end-to-end tests
in parallel and headlessly and, using the screenshot API mentioned above, create regression tests.
The HTML UI in the second panel allows you to simply add your steps together and run them in 
succession, while "guiding" you through the arguments of each "op" call:

![End to End Test Builder](../../images/debugger-e2e.jpg)

You can click the green triangle button to run your test, which merely runs the various `op` 
commands against the window in succession. If you click the "cloud" button, this runs all 
end-to-end tests headlessly and shows a checkmark / error for all failed tests.

You can also import the JSON definitions of E2E tests over "Import" - which appends the new E2E tests to
your current ones. Once you're done designing your end-to-end tests, you can export them into a
JSON file (Export > E2E Tests) and run them on your application with `AZ_BACKEND=headless AZ_E2E=my-tests.json`. This environment variable also works on an entire directory. 

As a result, you'll then get then a "cargo-like" output of the test results and the app will quit 
after the tests have been run:

```sh
git clone https://github.com/fschutt/azul
cargo run --release -p azul-doc -- codegen all
CARGO_TARGET_DIR=target/demo cargo build --release -p azul-dll --features build-dll
CARGO_TARGET_DIR=target/demo cargo build --release -p AzBuilder
AZ_BACKEND=headless AZ_E2E=e2e ./target/release/AzBuilder
```

shows:

```

```

So, instead of running end-to-end tests against `AzBuilder` (here we are running the "self-tests",
which assert various layout behaviours and APIs against regressions), you can obviously run this
on your own GUI application instead. 

### 1. Component Library (Left Sidebar)

The left sidebar lists all the components currently registered in your `AppConfig`'s component libraries (including the `builtin` HTML elements and any custom components you have registered). You can drag and drop these components directly into the center canvas structure.

### 2. Live Native Preview 

As you construct your component tree in the browser, the native window instantly updates to reflect the changes. Because the browser communicates with the native app via WebSockets, the native app handles all the actual rendering, layout, and styling. The browser simply manages the logical XML tree and data models.

### 3. Properties Panel (Right Sidebar)

![GUI Builder Properties](../../images/component-library.jpg)

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

