---
slug: windowing/macos
title: Windowing — macOS
language: en
canonical_slug: windowing/macos
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: macOS shell - Cocoa, AppKit, IME, a11y
prerequisites: [windowing/common]
tracked_files:
  - dll/src/desktop/shell2/macos/accessibility.rs
  - dll/src/desktop/shell2/macos/clipboard.rs
  - dll/src/desktop/shell2/macos/coregraphics.rs
  - dll/src/desktop/shell2/macos/corevideo.rs
  - dll/src/desktop/shell2/macos/events.rs
  - dll/src/desktop/shell2/macos/gl.rs
  - dll/src/desktop/shell2/macos/menu.rs
  - dll/src/desktop/shell2/macos/mod.rs
  - dll/src/desktop/shell2/macos/registry.rs
  - dll/src/desktop/shell2/macos/system_style.rs
  - dll/src/desktop/shell2/macos/tooltip.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
default-search-keys:
  - WindowCreateOptions
  - MonitorId
  - AppTerminationBehavior
  - CoreMenuCallback
---

# Windowing — macOS

## Overview

*WIP — main lifecycle and rendering are stable; the iOS branch in `shell2/ios/` shares much of this code but is not yet complete.* The macOS backend is `MacOSWindow`. It uses the `objc2` family of crates (`objc2`, `objc2_app_kit`, `objc2_foundation`) for static linking against `Cocoa.framework` / `AppKit.framework`. Unlike the Linux and Windows backends, **system frameworks are not dlopen'd** — they are linked at build time. CoreGraphics, CoreVideo, and the OpenGL framework are loaded via `dlopen` for narrowly-scoped reasons (older-OS forward compatibility, runtime-only display detection, deprecated API isolation).

The struct embeds the `event::CommonWindowState` like every other backend. macOS-specific fields include the `NSWindow` / `GLView` / `WindowDelegate` retained pointers, an `IOPMAssertionID` for keep-screen-awake, the `CoreVideoFunctions` table for the `CVDisplayLink` VSYNC callback, the `CoreGraphicsFunctions` table for display ID enumeration, and the optional `MacOSAccessibilityAdapter`.

## Window lifecycle and the GLView pattern

`MacOSWindow::new_with_fc_cache` runs on the main thread (enforced by `MainThreadMarker`). The setup is:

1. `NSApplication::sharedApplication(mtm)` — get the singleton.
2. `setActivationPolicy(NSApplicationActivationPolicy::Regular)` — makes the app a Dock-visible regular app (not a background helper).
3. `setup_main_menu(&app, mtm)` — installs the standard *Application > Quit* menu item (Cmd+Q sends `terminate:` to NSApp).
4. `app.finishLaunching()` — required so the Window Server fully registers the app. Without this, accessibility queries return `kAXErrorCannotComplete` (-25204) because macOS considers the app "not yet launched".
5. `app.activateIgnoringOtherApps(true)` — bring to front (deprecated API, but still the only way to handle this on older macOS).
6. Create `NSWindow` with the requested `NSWindowStyleMask`.
7. Create the `GLView` (defined inline via `objc2::define_class!`) — a custom `NSOpenGLView` subclass that owns the GL context, tracking area, IME state, and a back-pointer to `MacOSWindow`.
8. `setup_gl_view_back_pointer()` and `finalize_delegate_pointer()` — wire the *raw* `*mut c_void` window pointer into the view and delegate ivars so callbacks can recover `&mut MacOSWindow` from inside the AppKit callback.

The window is registered in the macOS thread-local registry, keyed by `*mut AnyObject`.

## Run loop — two strategies

The macOS `pub fn run` chooses one of two event-loop strategies based on `AppTerminationBehavior`:

### RunForever — NSApplication.run()

Standard macOS behaviour: the application runs forever, even when all its windows are closed. `app.run()` blocks until `terminate:` is sent (Cmd+Q, *Quit* menu, or a programmatic call). This is the right choice for menu-bar apps and document-based apps that want to stay in the Dock.

### ReturnToMain / EndProcess — manual loop

Suitable for one-shot applications. The pattern:

```rust,ignore
loop {
    autoreleasepool(|_| {
        // 1. Drain pending NSEvents (non-blocking)
        while let Some(event) = app.nextEventMatchingMask_untilDate_inMode_dequeue(
            NSEventMask::Any, None, NSDefaultRunLoopMode, true,
        ) {
            // dispatch to our handlers
            for wptr in registry::get_all_window_ptrs() {
                let macos_event = MacOSEvent::from_nsevent(&event);
                (*wptr).process_event(&event, &macos_event);
            }
            // forward to system
            app.sendEvent(&event);
        }

        // 2. If all windows closed, return / exit
        if registry::is_empty() { /* return or exit */ }

        // 3. Process per-window state diff, scroll wheel, gestures, a11y, popup creates
        // 4. Block on the run loop with a "distantFuture" date — wakes on
        //    *any* run loop source (Mach ports, timers, NSEvents)
        let run_loop = NSRunLoop::currentRunLoop();
        run_loop.runMode_beforeDate(NSDefaultRunLoopMode,
                                    &NSDate::distantFuture());

        // 5. After waking, drain any newly-arrived NSEvents and forward them
    });
}
```

The choice between `nextEventMatchingMask` and `runMode:beforeDate:` matters: `nextEventMatchingMask` only dequeues `NSEvent`s and **ignores other run loop sources**, including the Mach ports macOS accessibility uses. Without `runMode:beforeDate:` waking on those ports, VoiceOver and System Events queries time out with `kAXErrorCannotComplete` (-25204). Both APIs are needed — the run loop wakes for any source, and then we drain `nextEventMatchingMask` to process any new `NSEvent`s.

## GLView — receiving events

`GLView`, defined via `objc2::define_class!`, overrides the relevant `NSResponder` methods:

- `mouseDown:`, `mouseUp:`, `mouseDragged:`, `mouseMoved:`, `rightMouseDown:`, `otherMouseDown:`, etc.
- `scrollWheel:` (continuous trackpad scroll, treated as `ScrollSource::Touch`; mouse wheel is `Discrete`).
- `magnifyWithEvent:` (pinch), `rotateWithEvent:`, `swipeWithEvent:` (mapped to `GestureEvent::Pinch` / `Rotate` / `Swipe`).
- `keyDown:`, `keyUp:`, `flagsChanged:` for raw key events.
- `insertText:`, `setMarkedText:replacementRange:`, `unmarkText` for IME composition (the `NSTextInputClient` protocol).

Each override:

1. Recovers `&mut MacOSWindow` from `window_ptr_ivar`.
2. Updates the relevant fields on `current_window_state`.
3. Calls `process_window_events()` from `common::event::PlatformWindow`.

The IME methods set `ime_key_handled.set(true)` so that `handle_key_down` doesn't double-process the same key event during composition.

## Tracking areas and mouseExited:

macOS doesn't deliver `mouseMoved:` to a view by default — you must either enable `acceptsMouseMovedEvents` on the window (no per-view filtering) or install an `NSTrackingArea`. `GLView`'s `updateTrackingAreas` override creates a tracking area that covers its bounds with `NSTrackingMouseEnteredAndExited | NSTrackingActiveInActiveApp`. The area must be re-created on every `viewDidChangeFrame` because tracking areas don't follow geometry changes.

## Render path — OpenGL via NSOpenGLContext

`RenderBackend` selects `OpenGL` or `CPU`. GPU mode wires:

1. `NSOpenGLPixelFormatAttribute` array with depth, stencil, double-buffer, accelerated, GL 3.2 core profile.
2. `NSOpenGLPixelFormat::initWithAttributes` → `NSOpenGLContext`.
3. `setView:` to bind the context to the GLView.
4. `gl::GlFunctions::initialize()` — opens `/System/Library/Frameworks/OpenGL.framework/OpenGL` via dlopen and resolves every entry point with `dlsym`. The handle is kept on the struct so the framework stays loaded for the window's lifetime.

The `NSOpenGLContext` lives on the GLView; `flushBuffer` on `present`. `drawRect:` triggers paint; the first `drawRect:` call is what moves the window from invisible to on-screen.

OpenGL on macOS is deprecated as of 10.14 but still works on every shipping macOS (and through Rosetta on Apple Silicon). Migration to Metal is an open task — see the `RenderContext::Metal` variant in `common/compositor.rs` which already includes the necessary `MTLDevice` / `MTLCommandQueue` slots.

CPU mode goes through the same `cpurender` path as the other backends, with the framebuffer drawn into an `NSBitmapImageRep` and composited via `NSImage::drawInRect:`.

## VSYNC via CVDisplayLink

`CoreVideoFunctions` is dlopen'd from `/System/Library/Frameworks/CoreVideo.framework/CoreVideo`. `CVDisplayLinkCreateWithCGDisplay(displayID, &mut link)` creates a display link tied to the screen's refresh rate; `CVDisplayLinkSetOutputCallback(link, vsync_callback, window_ptr as *mut c_void)` installs a callback that fires once per VBlank on a dedicated thread. The callback signals `new_frame_ready` via the shared `Arc<(Mutex<bool>, Condvar)>` so the next frame can be presented in sync with refresh.

CoreVideo is dlopen'd because the framework moved between OS versions and `extern { ... }` linkage broke older macOS. Loading it dynamically lets the same binary run on 10.14 through 14.x.

## Display IDs — CGDirectDisplayID

`CoreGraphicsFunctions` loads `ApplicationServices.framework` (which transitively contains CoreGraphics) and resolves `CGMainDisplayID`, `CGDisplayBounds`. This is enough to identify the primary display and read its bounds for monitor enumeration. The full multi-monitor enumeration uses `NSScreen::screens(mtm)` — the CoreGraphics module is mostly used to build stable `MonitorId`s that survive screen reconfiguration.

## IME — NSTextInputClient

The GLView conforms to `NSTextInputClient`. AppKit calls `insertText:replacementRange:` to commit, and `setMarkedText:selectedRange:replacementRange:` for the live preedit. The marked text is buffered into `LayoutWindow.cursor_manager.preedit` and rendered inline; `firstRectForCharacterRange:` returns the caret rect so the IME candidate window appears in the right place.

The constant `MIN_IME_CURSOR_HEIGHT = 16.0` caps the candidate-window anchor height — without a minimum, single-line inputs end up with no visible IME panel.

`ime_key_handled: Cell<bool>` on `GLViewIvars` is the double-dispatch lock: when `setMarkedText:` or `insertText:` is called, it sets the flag so `handle_key_down` (which AppKit also calls for the same key event) skips sending a key event to the layout layer.

## Menus — NSMenu

`AzulMenuTarget`, defined via `define_class!`, is an NSObject that receives menu actions. Its `menuItemAction:` selector fires when any menu item with this target is clicked; the item's `tag` (which encodes a `command_id`) is pushed to the global `PENDING_MENU_ACTIONS: Mutex<Vec<isize>>`.

The main loop drains this queue and looks up the command ID in the window's `menu_command_callbacks` map to invoke the user `CoreMenuCallback`.

`AzulMenuTarget::shared_instance(mtm)` is a thread-local singleton — all NSMenuItems point at the same target, which keeps the per-item ivars at zero size.

The macOS app menu (the "{appname}" menu with About / Quit) is installed by `setup_main_menu` independently from any window menu; it survives the closure of all windows.

## Tooltips — NSPanel

The macOS tooltip wraps an `NSPanel` (a borderless utility window) with an `NSTextField` for the body. Width is computed by character count heuristics (`POINTS_PER_CHAR = 7.0`, capped at `TOOLTIP_MAX_WIDTH = 400`). Position uses `setFrameTopLeftPoint` to align the top-left of the panel with the hover anchor (Cocoa's coordinate system has Y increasing upward, so the geometry needs `screen_height - y` flipping — handled in the position setter).

This is the legacy direct-AppKit tooltip path. Future tooltips will flow through the same `pending_window_creates` queue used for popup menus, rendered via the standard layout pipeline so they support arbitrary styled DOM.

## Keep-screen-awake — IOPMAssertion

The IOKit FFI for `IOPMAssertionCreateWithName` + `IOPMAssertionRelease` lives in the macOS module. When `WindowFlags::keep_screen_awake` flips on, the window calls `IOPMAssertionCreateWithName("PreventUserIdleDisplaySleep", kIOPMAssertionLevelOn, "Azul")` and stores the resulting `IOPMAssertionID`. Release on flip-off or window close.

## Clipboard — NSPasteboard

The clipboard module wraps the deprecated `objc` (not `objc2`) crate to talk to `NSPasteboard` via dynamic message dispatch. The flow:

1. `[NSPasteboard generalPasteboard]`.
2. `[pasteboard clearContents]`.
3. `[pasteboard setString:text forType:NSPasteboardTypeString]`.

Read uses `[pasteboard stringForType:NSPasteboardTypeString]`. Both operations are synchronous — `NSPasteboard` is famously slow but there is no async API.

`#[link(name = "AppKit", kind = "framework")]` on the `extern "C" {}` block forces `NSPasteboard` to be in the class resolver's path even though no symbols are imported.

## Accessibility — accesskit_macos

The accessibility module uses `accesskit_macos::SubclassingAdapter` when the `a11y` feature is on. The adapter swizzles `NSAccessibility` methods on the GLView so VoiceOver queries get the AzulRoot accessibility tree.

`request_initial_tree` returns `None` to force the Placeholder → Active transition, which generates `AXFocusedUIElementChanged` notifications that VoiceOver needs for correct navigation. Returning `Some(tree)` here would skip Placeholder and go straight Inactive → Active, suppressing the focus event and breaking screen-reader navigation.

Action requests arrive on a `Sender<ActionRequest>` channel (`ChannelActionHandler`); the main loop drains them via `process_accessibility_actions()` outside of the NSEvent dispatch critical section.

## Multi-window registry

Same pattern as Linux and Windows. Thread-local `BTreeMap<*mut AnyObject, *mut MacOSWindow>`. The pointer is leaked via `Box::into_raw` from the run loop; recovered by `Box::from_raw` when the window is unregistered.

`pending_window_creates` is the dispatch target for popup menus and dialogs — each entry is a fresh `WindowCreateOptions` that produces a new `MacOSWindow` registered into the same map.

## system_style

`SystemStyle::detect_macos` queries:

- `NSApp.effectiveAppearance` for dark/light/auto.
- `NSColor::controlAccentColor` (10.14+) for accent color.
- `NSFont::systemFontOfSize:` for the UI font name.
- `[NSScreen mainScreen].backingScaleFactor` for the base scale.
- `NSWindow::titlebarAppearsTransparent` heuristics for window background material defaults.

Result populates `azul_css::system::SystemStyle`, identical to the Linux/Windows implementations. Theme changes arrive via `NSAppearanceDidChangeNotification` on the AppDelegate; the notification triggers a full `regenerate_layout`.

## Known issues / TODOs

- OpenGL is deprecated; a Metal compositor through `RenderContext::Metal` is sketched but not implemented.
- The CPU rendering path uses `NSImage` blit which is not the fastest — Core Graphics direct framebuffer paint via `CGContextRef` would be faster.
- `flagsChanged:` modifier diffing is heuristic — sometimes modifier state lags pressed-key state by one event. Real fix needs explicit modifier-state tracking on each `keyDown:`.
- Multi-touch (`NSTouch` from a Magic Trackpad) is not yet routed — only `mouseDown:`/`scrollWheel:` are processed.

## Coming Up Next

- [Common](common.md) — Shared shell infrastructure across platforms
- [Windows](windows.md) — Windows shell - Win32 messages, DirectComposition, IME
- [Menus and CSD](menus-and-csd.md) — Menus and client-side decorations across platforms
- [Windowing Overview](../windowing.md) — Per-window aggregate, headless variant, and the platform shell layer
