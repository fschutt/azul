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

After the table come two lines for a script: `[startup-json] {...}` (the headline numbers: `first_present_ms`, `before_library_ms`, `layout_passes`, `render_backend`; short, because logcat cuts an entry at 4 KB) and `[startup-marks] [...]` (every mark). `scripts/ci/startup_measure.py --exe ./my_app --runs 9` starts the app that many times with the switch on, reads these lines and prints the median of every mark, the cold first start apart from the warm ones, and the five costliest steps; `--expect-layouts 1` and `--max-first-present-ms` make it a gate.

## Reading the table

```text
[startup] timeline: process start from sysctl kern.proc.pid (resolution 1 us), origin = App::create entry 2.1 ms after it
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
| `app_run:*`, `run:*` | `App::run` and the platform's `run`: NSApplication, the menu, the launch hooks, `finishLaunching`, the event loop |
| `window:*` | the window: the native window, the content view, the GL context and WebRender (GPU only), the layout window, the menu bar, accessibility, the display link |
| `x11:*`, `wayland:*`, `win:*`, `android:*` | the same for the Linux, Windows and Android windows: the libraries, the display or the window class, the native window, the renderer (`render_backend` says `cpu` or `gpu` and what was asked for), the accessibility adapter, the map or the show |
| `android:egl_ready`, `android:gl_context_ready`, `android:webrender_ready`, `render_backend`, `android:first_gpu_frame_started`, `android:first_gpu_frame_done` | an Android window that draws with the GPU (`AZ_BACKEND=gpu`): EGL opened with its config and context, the OpenGL ES context current and probed (the detail says which driver), WebRender made, the choice of renderer (`render_backend` is also marked by a CPU window), the first frame begun (WebRender compiles its programs in it) and presented (`ms=` is the time it took: a phone's GPU a fraction of a second, a software GL a minute or two) |
| `gtk_im_loaded` | libgtk-3's input method context (X11 and Wayland): loaded at the first caret position a text field hands over, not at window creation; it shows as a late mark, and only in a run that has such a field |
| `layout_begin`, `layout_phases`, `layout_end` | one layout pass, its reason and its phases |
| `a11y_tree_first_built` | the layout pass that built the first accessibility tree |
| `first_present` | the first frame handed to the OS (macOS: the window is revealed) |
| `first_event` | the first input event (mouse, key, touch, scroll) the app handled, whoever sent it |

## `GET /startup`

With the debug server on (`AZ_DEBUG=<port>`) the same data is one JSON object: `enabled`, `process_start_known`, `process_start_source`, `before_library_ms`, `layout_passes`, `marks` (each with `i`, `name`, `at_ms`, `since_library_ms`, `thread`, `detail`) and, when they exist, `first_present_ms`, `first_event_ms`, `font_scan_complete_ms`, `a11y_tree_first_built_ms`. A harness reads a phone's numbers from it without parsing a log.

## Measuring well

- Say whether the run was COLD (the first start after minutes of nothing, or after an install: the loader, the font cache and the system's own caches are empty) or WARM, and how many runs the number is the median of. A cold start is what a user sees the first time.
- Say the load of the machine (`uptime`) and the build profile: a debug build is several times slower in `App::create` and in the layout than a release build.
- An emulator is not a phone, and a virtual machine is not a desktop: the shares of the phases matter more than the absolute numbers.
