## Updating window properties

You may also have noticed that the callback takes in a `AppState<DataModel>`,
not the `DataModel` directly. This is because you can change the window
settings, for example the title of the window:

```rust
fn callback(app_state: &mut AppState<DataModel>, _: WindowEvent) -> UpdateScreen {
    app_state.windows[window_id].window.title = "Hello";
    app_state.windows[window_id].window.menu += "&Application > &Quit\tAlt+F4";
}
```

Note how there isn't any `.get_title()` or `.set_title()`. Simply setting the
title is enough to invoke the (stateful) Win32 / X11 / Wayland / Cocoa functions
for setting the window title. You can query the active title / mouse or keyboard
state in the same way.