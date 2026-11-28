---
slug: debugging/startup
title: Startup Performance
language: en
canonical_slug: debugging/startup
audience: external
maturity: wip
guide_order: 203
topic_only: false
short_desc: Where the first milliseconds of an app go, on every platform
prerequisites: [debugging]
tracked_files:
  - dll/src/desktop/startup_timeline.rs
  - dll/src/desktop/shell2/common/log_gate.rs
  - dll/src/desktop/shell2/common/event.rs
last_generated_rev: 3a542c07c0066abbaac8f1ccad53e1465e6fc8b9
generated_at: 2026-11-26T00:00:00Z
default-search-keys:
  - App
---

# Startup Performance

## Introduction

*WIP.* The marks and their names may grow; the switch (`AZ_STARTUP`) and the output format are stable.

An app starts in a few steps that look like one: the OS creates the process and loads the libraries, your `main` runs, `App::create` reads the system style and sets up fonts, the window is made (a native window, and with the GPU renderer a GL context and WebRender), the first layout runs your `layout()` callback and the solver, a display list is built, the first frame is drawn and handed to the OS. The startup timeline puts a time on each of them, from the moment the OS created the process to the first frame on screen and the first input event the app handled. It is OFF by default and costs one test of a bool per mark when off.

```bash
AZ_STARTUP=1 ./my_app                                     # the table is printed to stderr at the first present
AZ_STARTUP=1 AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER=1 ./my_app  # print it and exit: a clean measure
AZ_STARTUP=1 AZ_DEBUG=8765 ./my_app                       # ... and  curl http://127.0.0.1:8765/startup  answers JSON
adb shell setprop debug.az.startup 1                      # Android: the table goes to logcat (tag RustStdoutStderr)
```

On iOS the table goes to the console of the process (`xcrun simctl launch --console`). `AZ_LOG_STDERR=0` keeps stderr clean and `AZ_LOG_FILE=<path>` writes the lines to a file; neither depends on the log level.

After the table come two lines for a script: `[startup-json] {...}` (the headline numbers: `first_present_ms`, `before_library_ms`, `layout_passes`, `render_backend`; short, because logcat cuts an entry at 4 KB) and `[startup-marks] [...]` (every mark). `scripts/ci/startup_measure.py --exe ./my_app --runs 9` starts the app that many times with the switch on, reads these lines and prints the median of every mark, the cold first start apart from the warm ones, and the five costliest steps; `--expect-layouts 1` and `--max-first-present-ms` make it a gate. `--from-log <file>` reads the timelines of a text instead (the logcat of ten cold starts of a phone, a console log) and prints the same medians and phases.

## Reading the table

```text
[startup] timeline: process start from sysctl KERN_PROC_PID (resolution 1 us), origin = App::create entry 2.1 ms after it
[startup]   #      at_ms       +ms  thread                 mark
[startup]   0      0.000         -  -                      process_start
[startup]   1      2.104     2.104  main                   library_entered
[startup]   2      2.455     0.351  main                   app_create:env_and_settings
...
[startup]  41    418.220    21.004  main                   layout_end #1 passes=1
[startup]  57    455.911     0.832  main                   first_present shows_document=true blank_draws_before=0
[startup] first_present at 455.911 ms (453.807 ms after the library was entered)
```

- `at_ms` counts from the process creation the OS recorded: macOS and iOS `sysctl kern.proc.pid` (1 us), Linux and Android `/proc/self/stat` against `/proc/uptime` (10 ms ticks: the first row is only that precise), Windows `GetProcessTimes` (1 us). Where the platform says nothing the first line says so and the times count from `library_entered`.
- `+ms` is the time since the previous mark OF THE SAME THREAD: a mark of the font scan does not shorten a main-thread step. A mark is the END of the step it names: `app_create:system_style` is what `App::create` spent on the system style.
- `library_entered` is the first line of `App::create`: the row before it is the platform's own floor (the loader, the runtime, your `main` up to `App::create`).
- Each layout pass has a `layout_begin #N reason=...` and a `layout_end #N passes=...` mark and, between them, a `layout_phases` line with the pass's biggest phases. `layout passes begun: N` counts them: an app that did not change anything between the window and its first frame should show ONE layout before `first_present`.
- Marks after the table (the font scan finishing, the first input) print as single `[startup] late: ...` lines.

## The marks

| mark | what it ends |
|---|---|
| `library_entered` | everything before the first line of `App::create` |
| `app_create:*` | the steps of `App::create`: `env_and_settings` (on Android this includes the properties), `settings_and_rice`, `e2e_services_logger`, `identity_and_trust`, `system_style` (with `style:*` marks inside on macOS), `theme_light_rice_watcher`, `icons`, `app_internal`, `done` |
| `fonts:*` | the font registry: created, the disk cache read (`loaded=true` is a warm start), the scan threads spawned |
| `font_dirs_scanned`, `font_scan_complete` | the scan on its worker threads: what the first layout can wait for on a machine without the font cache |
| `fonts:cache`, `fonts:cache_written` | what the font cache did for this start (`hit`, `miss` or `rebuilt (why)`, the faces it loaded, what the check and the load cost) and, from the thread that saves it after the scan, what was written (`Unchanged` for a hit that found nothing new) |
| `app_run:*`, `run:*` | `App::run` and the platform's `run`: NSApplication, the menu, the launch hooks, `finishLaunching`, the event loop |
| `window:*` | the window: the native window, the content view, the GL context and WebRender (GPU only), the layout window, the menu bar, accessibility, the display link |
| `x11:*`, `wayland:*`, `win:*`, `android:*` | the same for the Linux, Windows and Android windows: the libraries, the display or the window class, the native window, the renderer (`render_backend` says `cpu` or `gpu` and what was asked for), the accessibility adapter, the map or the show |
| `android:egl_ready`, `android:gl_context_ready`, `android:webrender_ready`, `render_backend`, `android:first_gpu_frame_started`, `android:first_gpu_frame_done` | an Android window that draws with the GPU (`AZ_BACKEND=gpu`): EGL opened with its config and context, the OpenGL ES context current and probed (the detail says which driver), WebRender made, the choice of renderer (`render_backend` is also marked by a CPU window), the first frame begun (WebRender compiles its programs in it) and presented (`ms=` is the time it took: a phone's GPU a fraction of a second, a software GL a minute or two) |
| `gtk_im_loaded` | libgtk-3's input method context (X11 and Wayland): loaded at the first caret position a text field hands over, not at window creation; it shows as a late mark, and only in a run that has such a field |
| `layout_begin`, `layout_phases`, `layout_end` | one layout pass, its reason and its phases |
| `layout_extra_pass` | why a layout ran a SECOND pass inside one call (`#1 lifecycle_refresh=true docks_changed=false`): a lifecycle callback asked for a refresh, or the docking of an inline `<transient-window>` changed; a window whose first DOM is simply laid out has none |
| `frame:first_render_*` | the first frame's render on macOS, split: `begin`, `prepared` (threads polled, size synced), `content_prepared` (the images the frame draws: the procedural materials, the desktop wallpaper they reflect), `rasterized`, `done` (the pixels are handed to the view) |
| `a11y_tree_first_built` | the layout pass that built the first accessibility tree |
| `first_present` | the first frame handed to the OS (macOS: the window is revealed) |
| `first_event` | the first input event (mouse, key, touch, scroll) the app handled, whoever sent it |

## `GET /startup`

With the debug server on (`AZ_DEBUG=<port>`) the same data is one JSON object: `enabled`, `process_start_known`, `process_start_source`, `before_library_ms`, `layout_passes`, `marks` (each with `i`, `name`, `at_ms`, `since_library_ms`, `thread`, `detail`) and, when they exist, `first_present_ms`, `first_event_ms`, `font_scan_complete_ms`, `a11y_tree_first_built_ms`. A harness reads a phone's numbers from it without parsing a log.

## Measuring well

- Say whether the run was COLD (the first start after minutes of nothing, or after an install: the loader, the font cache and the system's own caches are empty) or WARM, and how many runs the number is the median of. A cold start is what a user sees the first time.
- Say the load of the machine (`uptime`) and the build profile: a debug build is several times slower in `App::create` and in the layout than a release build.
- An emulator is not a phone, and a virtual machine is not a desktop: the shares of the phases matter more than the absolute numbers.

## The font cache

The first start of an app on a machine scans every font file of the system (about 200 ms for 370 fonts on macOS, more on a Linux desktop with thousands) and keeps what it found in a manifest, so that the next start loads a few hundred faces in 5 ms and scans nothing again. The manifest is `rfc/fonts/manifest.bin` in the platform's cache directory (`~/Library/Caches` on macOS and in an iOS app's container, `$XDG_CACHE_HOME` or `~/.cache` on Linux, `%LOCALAPPDATA%` on Windows, the app's own cache directory on Android), and `manifest.key` beside it vouches for it.

A start believes the manifest only when the key file describes exactly that manifest (its length and a hash of it), was made by this build of azul on this OS build, for the same scanned directories, and every font file it lists still has the size and modification time it had. Anything else (no manifest, a cut or damaged one, an OS update, a font that changed or went away, a manifest another build wrote) is a rebuild: the scan runs as if there were no cache and the pair is replaced, atomically, when it is complete. The first frame never waits for the write, and a start that finds nothing new writes nothing. The `fonts:cache` mark of the timeline says which of the three happened and why:

```text
[startup]  14   31.2   4.6  main  fonts:cache hit, 812 faces, check 1.1 ms, load 3.4 ms
[startup]  14   33.0  96.0  main  fonts:cache rebuilt (os build changed: 24F74 -> 24G90), 0 faces, check 0.9 ms, load 0.0 ms
```

To force a rebuild, remove the two files; nothing else keeps font data between runs.

## What a person sees before the first picture (Android)

The table above says when the engine posts its first picture; what the person looks at until then is the system's STARTING WINDOW (the app icon on the window colour: `windowSplashScreenBackground` on Android 12 and later, the theme's `windowBackground` before). An Azul window is a surface the engine draws on itself (`Window.takeSurface`): the view system never draws it, so left alone the system takes the starting window away when the window "has drawn", which for such a window is at once, about 0.4 s after the start, before the surface has a single buffer. The screen then shows the empty window, black, until the first layout is done: 0.4 s for a small app and 1.1 s for the widget gallery on the emulator, and the longer the first layout the longer the black.

`AzulActivity` therefore holds the window's FIRST DRAW (a pre-draw listener that cancels it) until the engine says its first picture is posted, with a limit of 4 s so that an engine that never posts cannot trap the app behind its starting window: the starting window stays, and then it goes in ONE frame: on Android 12 and later an exit listener removes the copy of the window the system hands over at once (`SplashScreen.setOnExitAnimationListener`), so the system's own exit animation does not run (in the night mode it fades a black window over a page that is nearly black and passes through a window with no icon and no page: 3 frames median and 4 to 6 at the worst on the lab's controls for Android 14 and 15; the cut has none in 20 of 20 night starts). The hold belongs to the activity, not to the process: a WARM start (the phone kept the process of an app the user left and makes its activity again) has a starting window of its own and is held until the engine posts its first picture again. (The exit listener is no hold: the system hands the app a copy of the starting window that sits in the window's view hierarchy, which nobody draws, so holding THAT held nothing on the screen.) Two things the engine knows are known before that picture and not after it, because a picture that moves or changes colour a moment later is a flicker too: the window's first insets (the status bar, the navigation bar, the cutout: kept by an early listener, `NativeTextBridge.insetsSnapshot`, applied when the surface is created; not for an app that asks the system to hide a bar or to draw into the cutout area, such as AzWriter on a phone: its request changes the insets a few frames later, so its first layout is made as before and the insets come after the request), and the phone's night mode (the activity's configuration). `AZ_STARTUP=1` shows it: `layout_begin #1` and no `reason=Resize` pass before `first_present`; the log line `[Android] the window's first insets are known at the surface` says the first layout had them. After the first picture the system reports `WindowResized` at every relayout of the window that touches its surface (the first draw going ahead after the hold is one), most of them with the size the window already has: the engine lays out again only for another size, and says which it was (`[Android] WindowResized: the surface is 1080x2400 px, the size the window has: nothing to lay out`).

The lab that measures it frame by frame is `scripts/ci/android_startup_lab.sh` (a control app, the real Java side over a stand-in native library, no engine) and `scripts/ci/android_startup_real.sh` (the real apps, light and night, ten cold starts each): the screen is recorded at 30 fps and every frame is classified (`S` the starting window, `B` black, `1` / `2` the pictures, `W` a light picture in the night mode).
