# Examples

## `async`

- Shows the use of `async::Task` to create a background thread to run a long-running task.
  In this case the background thread just sleeps / blocks for 10 seconds.
- While the background thread is running, the main thread has a timer that updates the window,
  which could be used for showing
- Note that the window can be resized while the background thread is running
- After the background thread is finished, the UI is updated again to show that the
  thread has finished running. The thread is joined without blocking the UI.

## `calculator`

- Shows how function composition can be used to reorganize code for the calculator rows,
  by compositing `numpad_btn` (to render a single button of a calculator) and `render_row`
  (to render a row of buttons)
- Also shows how to handle window-global events (to listen for key input without
  requiring the user to hover or focus over any element).

## `game_of_life`

- Shows how to use timers in order to update the game board every 200ms.
- Performance demo, performs the layout for 5600 rectangles (TODO: Should be replaced by an image).

## `hot_reload`

- Shows a window where the CSS can be hot-reloaded (you can run the demo, then edit the )
- In release mode, the CSS isn't hot-reloadable, but only parsed once at startup.

## `list`

- Shows how to use iterators to build a DOM by using an iterator over an array of strings

## `opengl`

- Shows how to render an OpenGL texture as an image via a `GlTextureCallback`

## `slider`

- Shows how to use CSS variables that can be changed at runtime by user input
- Shows how to center an absolute-positioned element

## `svg`

- Shows how to spawn a file dialog (to ask for an input SVG file) and load an SVG file
  by using the SVG renderer (which, internally is based on drawing to an OpenGL texture,
  so it's similar to the `opengl` demo).
- Requires the features `svg` and `svg_parsing`.

## `table`

- Shows the use of iframes to render infinite data structures
- Note that cells that are not visible are not rendered in the DOM
- The table is scrollable, the `IFrameCallback` is called again after a certain scroll threshold
- Performance demo, performs the layout for about 6000 rectangles

## `text_editor`

- TODO: Should show a text editor

## `text_input`

- Shows a simple `TextInput` widget that demonstrates two-way data binding

## `text_shaping`

- Shows the use of harfbuzz to show kerning,
- Has an XML file attached that can be edited at runtime. Good for testing fonts and

## `transparent_window`

- TODO: Should show a window without standard window decorations with a half-transparent background

## `xml`

- Shows the XML hot-reload system and the XML-to-Rust compiler
- XML can be live edited and gets compiled to Rust code in release mode