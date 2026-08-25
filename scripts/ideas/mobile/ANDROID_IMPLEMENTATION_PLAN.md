# Android Implementation Plan

Reference: macOS backend (`dll/src/desktop/shell2/macos/mod.rs`) as golden standard.

## Current State

Android has **no platform backend**. What exists:

- `core/src/window.rs` — `AndroidHandle { a_native_window: *mut c_void }` and `RawWindowHandle::Android` variant
- `css/src/system.rs` — `Platform::Android`, `android_material_light()`, `android_holo_dark()`, `android_fallback_chain()`, `ScrollPhysics::android()`
- `css/src/dynamic_selector.rs` — `parse_android_version()` for CSS media queries
- `webrender/` — Android-specific shader macros, GL driver workarounds for Android emulators
- `dll/src/desktop/shell2/mod.rs` — No `android` module, no cfg block

There is no `shell2/android/` directory, no Cargo.toml dependencies, no build.rs NDK setup.

## Architecture Overview

```
Android App Process (NativeActivity)
├── ANativeActivity_onCreate() → android_main()  [via android-activity crate]
├── android_main(app: AndroidApp)
│   ├── AndroidWindow::new()
│   │   ├── ANativeWindow from AndroidApp
│   │   ├── CommonWindowState (layout, hit test, resources)
│   │   └── CpuBackend (cpurender → AzulPixmap)
│   └── Event loop: app.poll_events()
│       ├── MainEvent::InitWindow → get ANativeWindow, start rendering
│       ├── MainEvent::InputAvailable → process touch/key events
│       ├── MainEvent::TerminateWindow → release ANativeWindow
│       ├── MainEvent::Resume/Pause → lifecycle
│       └── MainEvent::Destroy → cleanup
└── Rendering: ANativeWindow_lock() → memcpy pixmap → ANativeWindow_unlockAndPost()
```

**Key design decision: Use `android-activity` crate with `native-activity` feature.**

This gives us:
- Zero Java code (NativeActivity is built into Android)
- Cross-compilation from macOS
- `android_main()` entry point
- Event loop with `poll_events()`
- `ANativeWindow` access for CPU rendering
- Touch input via `MotionEvent`
- Keyboard input via `KeyEvent` (hardware keys)

The one limitation is text input (soft keyboard / IME) — NativeActivity's support is primitive.
We handle this with a small JNI bridge (Phase 5).

## Cross-Compilation Toolchain

**Targets (all Tier 2, available via rustup):**
```
aarch64-linux-android        — ARM64 (95%+ of modern phones)
armv7-linux-androideabi      — ARM32 (legacy)
x86_64-linux-android         — x86-64 (emulators, Chromebooks)
i686-linux-android           — x86 (old emulators only)
```

**Prerequisites:**
- Any host OS (macOS, Linux, Windows)
- Android NDK (r27+) — provides Clang cross-compilers
- Android SDK Command-Line Tools — provides `aapt2`, `zipalign`, `apksigner`
- JRE — for `keytool` and `apksigner` (no JDK build system needed)
- `adb` — for device/emulator deployment (included in SDK Platform-Tools)

**NO Android Studio, NO Gradle, NO Java source code (for Phase 1-4).**

**Install NDK from CLI (no Android Studio):**
```bash
# Download command-line tools only from developer.android.com
mkdir -p ~/android-sdk/cmdline-tools
cd ~/android-sdk/cmdline-tools
unzip commandlinetools-mac-*.zip
mv cmdline-tools latest

export ANDROID_HOME=~/android-sdk
export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$PATH"

# Install build tools, platform, and NDK
sdkmanager "build-tools;34.0.0" "platforms;android-34" "ndk;27.0.12077973"
```

**Rust targets:**
```bash
rustup target add aarch64-linux-android x86_64-linux-android
```

**Linker configuration** (`.cargo/config.toml` in project root):
```toml
[target.aarch64-linux-android]
linker = "~/android-sdk/ndk/27.0.12077973/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android21-clang"

[target.x86_64-linux-android]
linker = "~/android-sdk/ndk/27.0.12077973/toolchains/llvm/prebuilt/darwin-x86_64/bin/x86_64-linux-android21-clang"
```

Or use `cargo-ndk` to auto-configure:
```bash
cargo install cargo-ndk
cargo ndk -t arm64-v8a build --release
```

**Build command:**
```bash
cargo build --target aarch64-linux-android --release
# Output: target/aarch64-linux-android/release/libazul_example.so
```

Note: The crate type must be `cdylib` for Android to load it as a shared library.

## APK Structure (Minimal, No Java)

```
myazulapp.apk (ZIP file)
├── AndroidManifest.xml              (binary XML, compiled by aapt2)
├── lib/
│   ├── arm64-v8a/
│   │   └── libmyazulapp.so         (Rust code, aarch64)
│   └── x86_64/
│       └── libmyazulapp.so         (Rust code, x86_64 for emulator)
├── resources.arsc                    (resource table, from aapt2)
└── META-INF/                         (signing info)
```

**AndroidManifest.xml:**
```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="com.example.myazulapp"
    android:versionCode="1"
    android:versionName="1.0">

    <uses-sdk android:minSdkVersion="21" android:targetSdkVersion="34" />

    <application
        android:label="My Azul App"
        android:hasCode="false">

        <activity
            android:name="android.app.NativeActivity"
            android:configChanges="orientation|screenSize|screenLayout|keyboardHidden|keyboard"
            android:exported="true">

            <meta-data
                android:name="android.app.lib_name"
                android:value="myazulapp" />

            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
```

Key: `android:hasCode="false"` means no DEX/Java code. `android:configChanges` prevents 
Activity restart on rotation/keyboard (we handle it natively).

**Build APK from CLI:**
```bash
#!/bin/bash
set -e
APP_NAME="myazulapp"
BUNDLE_ID="com.example.myazulapp"
BUILD_TOOLS="$ANDROID_HOME/build-tools/34.0.0"
PLATFORM="$ANDROID_HOME/platforms/android-34"

# 1. Compile manifest
$BUILD_TOOLS/aapt2 link \
    --manifest AndroidManifest.xml \
    -I $PLATFORM/android.jar \
    -o base.apk

# 2. Add native library
mkdir -p lib/arm64-v8a
cp target/aarch64-linux-android/release/lib${APP_NAME}.so lib/arm64-v8a/
cd lib && zip -r ../base.apk arm64-v8a/ && cd ..

# 3. Align
$BUILD_TOOLS/zipalign -f 4 base.apk aligned.apk

# 4. Sign
$BUILD_TOOLS/apksigner sign \
    --ks debug.keystore \
    --ks-key-alias androiddebugkey \
    --ks-pass pass:android \
    aligned.apk

# 5. Install
adb install -r aligned.apk
adb shell am start -n $BUNDLE_ID/android.app.NativeActivity
```

## Implementation Phases

### Phase 1: Module Structure & Compilation

**Goal:** `cargo build --target aarch64-linux-android` compiles (even if the app does nothing useful).

**New files to create:**

1. **`dll/src/desktop/shell2/android/mod.rs`** — Platform backend (~400 lines initial)

2. **Modify `dll/src/desktop/shell2/mod.rs`:**
   ```rust
   // Add after the iOS line:
   #[cfg(target_os = "android")]
   pub mod android;
   
   // In cfg_if! block, add:
   } else if #[cfg(target_os = "android")] {
       pub use android::AndroidWindow as Window;
       pub use android::AndroidEvent as WindowEvent;
   }
   
   // In get_backend_name():
   #[cfg(target_os = "android")]
   return "android-nativeactivity";
   ```

3. **Modify `dll/Cargo.toml`:**
   ```toml
   [target.'cfg(target_os = "android")'.dependencies]
   android-activity = { version = "0.6", features = ["native-activity"] }
   ndk = "0.9"
   jni = "0.21"                    # For text input JNI bridge (Phase 5)
   ```

4. **Modify `dll/build.rs`:**
   ```rust
   let target = env::var("TARGET").unwrap_or_default();
   if target.contains("android") {
       // Check for NDK
       if env::var("ANDROID_NDK_HOME").is_err() && env::var("ANDROID_HOME").is_err() {
           println!("cargo:warning=ANDROID_NDK_HOME or ANDROID_HOME not set.");
           println!("cargo:warning=Install Android NDK: sdkmanager 'ndk;27.0.12077973'");
       }
       // Link Android system libraries
       println!("cargo:rustc-link-lib=android");
       println!("cargo:rustc-link-lib=log");
       return;
   }
   ```

5. **Modify `dll/src/desktop/shell2/run.rs`:**
   ```rust
   #[cfg(target_os = "android")]
   pub fn run(
       app_data: RefAny,
       config: AppConfig,
       fc_cache: Arc<FcFontCache>,
       font_registry: Option<Arc<FcFontRegistry>>,
       root_window: WindowCreateOptions,
   ) -> Result<(), WindowError> {
       // On Android, the event loop is driven by android-activity
       // Store initial options for android_main to retrieve
       unsafe {
           ANDROID_INITIAL_OPTIONS = Some((app_data, config, fc_cache, font_registry, root_window));
       }
       // The actual event loop runs in android_main(), called by the NativeActivity glue
       Ok(())
   }
   
   #[cfg(target_os = "android")]
   pub(super) static mut ANDROID_INITIAL_OPTIONS: Option<(
       RefAny, AppConfig, Arc<FcFontCache>, Option<Arc<FcFontRegistry>>, WindowCreateOptions
   )> = None;
   ```

**Minimal `android/mod.rs` skeleton:**

```rust
//! Android backend using NativeActivity + android-activity crate.

use std::ffi::c_void;
use std::sync::Arc;
use std::cell::RefCell;

use android_activity::{AndroidApp, MainEvent, InputStatus};
use ndk::native_window::NativeWindow;

use crate::impl_platform_window_getters;
use crate::desktop::shell2::common::{
    event::{self, PlatformWindow, CommonWindowState},
    debug_server::LogCategory,
    WindowError,
};
use azul_core::{
    resources::{AppConfig, ImageCache, RendererResources, DpiScaleFactor},
    window::{RawWindowHandle, AndroidHandle},
    refany::RefAny,
    gl::OptionGlContextPtr,
};
use azul_layout::{
    window::LayoutWindow,
    window_state::{FullWindowState, WindowCreateOptions},
    ScrollbarDragState,
    cpurender::{self, AzulPixmap},
    headless::CpuHitTester,
};
use rust_fontconfig::FcFontCache;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderBackend { Cpu }

#[derive(Debug, Clone)]
pub enum AndroidEvent {
    Close,
}

pub struct AndroidWindow {
    native_window: Option<NativeWindow>,
    backend: RenderBackend,
    is_open: bool,
    // CPU rendering
    cpu_pixmap: Option<AzulPixmap>,
    cpu_glyph_cache: cpurender::GlyphCache,
    // Common
    pub common: CommonWindowState,
}

impl AndroidWindow {
    pub fn new(
        options: WindowCreateOptions,
        fc_cache: Arc<FcFontCache>,
        config: AppConfig,
    ) -> Result<Self, WindowError> {
        let full_window_state = FullWindowState::new(options.state);
        let mut layout_window = LayoutWindow::new(fc_cache.as_ref().clone()).unwrap();
        layout_window.current_window_state = full_window_state.clone();
        layout_window.routes = config.routes.clone();

        Ok(Self {
            native_window: None,
            backend: RenderBackend::Cpu,
            is_open: true,
            cpu_pixmap: None,
            cpu_glyph_cache: cpurender::GlyphCache::new(),
            common: CommonWindowState {
                layout_window: Some(layout_window),
                current_window_state: full_window_state,
                previous_window_state: None,
                image_cache: ImageCache::default(),
                renderer_resources: RendererResources::default(),
                fc_cache: fc_cache.clone(),
                gl_context_ptr: None.into(),
                system_style: Arc::new(azul_css::system::SystemStyle::default()),
                app_data: Arc::new(RefCell::new(RefAny::default())),
                scrollbar_drag_state: None,
                hit_tester: None,
                cpu_hit_tester: Some(CpuHitTester::new()),
                last_hovered_node: None,
                document_id: None,
                id_namespace: None,
                render_api: None,
                renderer: None,
                frame_needs_regeneration: true,
                display_list_initialized: false,
                display_list_dirty: false,
                a11y_dirty: true,
            },
        })
    }

    pub fn set_native_window(&mut self, window: NativeWindow) {
        self.native_window = Some(window);
        self.common.frame_needs_regeneration = true;
    }

    pub fn clear_native_window(&mut self) {
        self.native_window = None;
    }

    pub fn poll_event(&mut self) -> Option<AndroidEvent> { None }
    pub fn is_open(&self) -> bool { self.is_open }
    pub fn close(&mut self) { self.is_open = false; }
}

impl PlatformWindow for AndroidWindow {
    impl_platform_window_getters!(common);

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Android(AndroidHandle {
            a_native_window: self.native_window.as_ref()
                .map(|w| w.ptr().as_ptr() as *mut c_void)
                .unwrap_or(std::ptr::null_mut()),
        })
    }

    fn sync_window_state(&mut self) {}
}
```

### Phase 2: CPU Rendering Pipeline

**Goal:** Render an Azul UI to the screen via `ANativeWindow_lock` → memcpy → `unlockAndPost`.

This is the simplest possible rendering path on Android — no EGL, no shaders, no GPU setup.

**How `ANativeWindow_lock` works:**

```rust
use ndk::native_window::{NativeWindow, NativeWindowBufferFormat};

fn render_frame(window: &mut AndroidWindow) {
    let native_window = match &window.native_window {
        Some(w) => w,
        None => return,  // No surface yet
    };
    
    let (width, height) = (native_window.width() as u32, native_window.height() as u32);
    if width == 0 || height == 0 { return; }
    
    // 1. Run layout if needed
    if window.common.frame_needs_regeneration {
        // ... regenerate layout via PlatformWindow default impl ...
    }
    
    // 2. Build display list from layout results
    let display_list = build_display_list(&window.common);
    
    // 3. Render to CPU pixmap
    let pixmap = window.cpu_pixmap.get_or_insert_with(|| AzulPixmap::new(width, height));
    pixmap.resize_grow_only(width, height);
    cpurender::render(&display_list, pixmap, dpi, 
        &window.common.renderer_resources,
        &window.common.fc_cache,
        &window.cpu_glyph_cache);
    
    // 4. Lock the native window buffer and copy pixels
    native_window.set_buffers_geometry(
        width as i32, height as i32,
        NativeWindowBufferFormat::RGBX_8888,
    );
    
    if let Ok(mut buffer) = native_window.lock(None) {
        let src = pixmap.data();
        let dst = buffer.bits();
        let src_stride = (width * 4) as usize;
        let dst_stride = (buffer.stride() * 4) as usize;
        
        // Copy row-by-row (strides may differ)
        for y in 0..height as usize {
            let src_row = &src[y * src_stride..(y + 1) * src_stride];
            let dst_row = &mut dst[y * dst_stride..y * dst_stride + src_stride];
            dst_row.copy_from_slice(src_row);
        }
        // buffer.drop() calls ANativeWindow_unlockAndPost()
    }
}
```

**Pixel format note:** `ANativeWindow_lock` returns a buffer in the window's native format. 
We request `RGBX_8888` which is compatible with our RGBA8 pixmap. The alpha channel is ignored
(the window surface is always opaque). If the device doesn't support RGBX_8888, we may need
to convert — but all modern Android devices support it.

**DPI handling:**
```rust
// Get display density from Android configuration
let config = app.config();
let density_dpi = config.density_dpi();  // e.g., 440
let scale_factor = density_dpi as f32 / 160.0;  // Android baseline is 160 DPI
```

### Phase 3: Event Loop (android_main)

**Goal:** Wire up the android-activity event loop to drive the Azul window.

The `android_main` function is the entry point called by the NativeActivity glue:

```rust
#[no_mangle]
fn android_main(app: AndroidApp) {
    // Retrieve initial options stored by run()
    let (app_data, config, fc_cache, font_registry, root_window) = unsafe {
        super::run::ANDROID_INITIAL_OPTIONS.take()
            .expect("run() must be called before android_main()")
    };
    
    let mut window = AndroidWindow::new(root_window, fc_cache, config).unwrap();
    
    loop {
        // Poll with 16ms timeout (~60fps when active, blocks when paused)
        let timeout = if window.native_window.is_some() {
            Some(std::time::Duration::from_millis(16))
        } else {
            None  // Block indefinitely until window is available
        };
        
        app.poll_events(timeout, |event| {
            match event {
                // --- Window lifecycle ---
                MainEvent::InitWindow { .. } => {
                    if let Some(nw) = app.native_window() {
                        window.set_native_window(nw);
                        render_frame(&mut window);
                    }
                }
                MainEvent::TerminateWindow { .. } => {
                    window.clear_native_window();
                }
                MainEvent::WindowResized { .. } => {
                    window.common.frame_needs_regeneration = true;
                    render_frame(&mut window);
                }
                
                // --- App lifecycle ---
                MainEvent::Resume { .. } => {
                    // App came to foreground
                }
                MainEvent::Pause => {
                    // App going to background — release resources if needed
                }
                MainEvent::Destroy => {
                    window.close();
                }
                
                // --- Input ---
                MainEvent::InputAvailable => {
                    app.input_events(|input_event| {
                        match input_event {
                            android_activity::input::InputEvent::MotionEvent(motion) => {
                                handle_motion_event(&mut window, &motion);
                            }
                            android_activity::input::InputEvent::KeyEvent(key) => {
                                handle_key_event(&mut window, &key);
                            }
                            _ => {}
                        }
                        InputStatus::Unhandled
                    });
                    
                    // After processing input, run event system and redraw
                    window.process_window_events(0);
                    render_frame(&mut window);
                }
                
                _ => {}
            }
        });
        
        if !window.is_open() {
            break;
        }
        
        // Tick timers, process thread callbacks
        // (same as headless backend timer loop)
        let needs_redraw = window.tick_timers_and_threads();
        if needs_redraw {
            render_frame(&mut window);
        }
    }
}
```

### Phase 4: Touch Input

**Goal:** Touch events drive hover/click/scroll in the Azul event system.

```rust
use android_activity::input::{MotionAction, MotionEvent};

fn handle_motion_event(window: &mut AndroidWindow, motion: &MotionEvent) {
    let action = motion.action();
    
    // Get primary pointer position
    let pointer = motion.pointer_at_index(0);
    let x = pointer.x();
    let y = pointer.y();
    
    // Convert physical pixels to logical pixels
    let scale = window.get_dpi_scale_factor();
    let logical_x = x / scale;
    let logical_y = y / scale;
    
    match action {
        MotionAction::Down => {
            // First finger down — map to mouse left button
            window.common.current_window_state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(
                    LogicalPosition::new(logical_x, logical_y)
                );
            window.common.current_window_state.mouse_state.left_down = true;
            
            // Update hit test
            if let Some(ref mut ht) = window.common.cpu_hit_tester {
                ht.hit_test(LogicalPosition::new(logical_x, logical_y));
            }
        }
        MotionAction::Move => {
            window.common.current_window_state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(
                    LogicalPosition::new(logical_x, logical_y)
                );
            
            if let Some(ref mut ht) = window.common.cpu_hit_tester {
                ht.hit_test(LogicalPosition::new(logical_x, logical_y));
            }
            
            // Multi-touch scroll detection (two fingers)
            if motion.pointer_count() >= 2 {
                let p0 = motion.pointer_at_index(0);
                let p1 = motion.pointer_at_index(1);
                // Compute scroll delta from finger movement
                // ... update scroll state ...
            }
        }
        MotionAction::Up => {
            window.common.current_window_state.mouse_state.left_down = false;
        }
        MotionAction::Cancel => {
            window.common.current_window_state.mouse_state.left_down = false;
        }
        MotionAction::PointerDown => {
            // Additional finger — generate TouchStart event
            let idx = motion.pointer_index();
            let p = motion.pointer_at_index(idx);
            let id = p.pointer_id();
            // Store in TouchEventData for EventFilter::Touch dispatch
        }
        MotionAction::PointerUp => {
            // Finger lifted — generate TouchEnd event
        }
        _ => {}
    }
}
```

**Multi-touch → scroll mapping:**
The core framework has `ScrollManager` with momentum physics. Two-finger pan should translate
to scroll deltas, similar to how macOS trackpad scroll events work.

### Phase 5: Text Input (Soft Keyboard)

**The problem:** `NativeActivity` only delivers hardware `KeyEvent`s. The soft keyboard on Android
goes through `InputConnection` (a Java interface). There is NO NDK equivalent.

**Solution: Minimal JNI bridge (~50 lines of Java)**

This is the one place where we need a small amount of Java code. The Java class implements
`InputConnection` and calls back into Rust via JNI.

**Java code** (`scripts/android/NativeInputConnection.java`, ~50 lines):
```java
package com.azul.input;

import android.view.inputmethod.BaseInputConnection;
import android.view.View;

public class NativeInputConnection extends BaseInputConnection {
    private long nativePtr;

    public NativeInputConnection(View view, boolean fullEditor, long nativePtr) {
        super(view, fullEditor);
        this.nativePtr = nativePtr;
    }

    @Override
    public boolean commitText(CharSequence text, int newCursorPosition) {
        nativeCommitText(nativePtr, text.toString(), newCursorPosition);
        return true;
    }

    @Override
    public boolean setComposingText(CharSequence text, int newCursorPosition) {
        nativeSetComposingText(nativePtr, text.toString(), newCursorPosition);
        return true;
    }

    @Override
    public boolean deleteSurroundingText(int beforeLength, int afterLength) {
        nativeDeleteSurrounding(nativePtr, beforeLength, afterLength);
        return true;
    }

    private native void nativeCommitText(long ptr, String text, int cursor);
    private native void nativeSetComposingText(long ptr, String text, int cursor);
    private native void nativeDeleteSurrounding(long ptr, int before, int after);
}
```

**Compile without Gradle:**
```bash
javac -source 11 -target 11 \
    -classpath $ANDROID_HOME/platforms/android-34/android.jar \
    -d classes/ \
    scripts/android/NativeInputConnection.java

$ANDROID_HOME/build-tools/34.0.0/d8 classes/com/azul/input/NativeInputConnection.class \
    --output dex/

# Add classes.dex to APK
cd dex && zip -r ../base.apk classes.dex && cd ..
```

**Rust JNI side** (in `android/mod.rs`):
```rust
use jni::JNIEnv;

// Called from Java NativeInputConnection.commitText()
#[no_mangle]
extern "C" fn Java_com_azul_input_NativeInputConnection_nativeCommitText(
    env: JNIEnv,
    _obj: jni::objects::JObject,
    native_ptr: jni::sys::jlong,
    text: jni::objects::JString,
    _cursor: jni::sys::jint,
) {
    let window = unsafe { &mut *(native_ptr as *mut AndroidWindow) };
    let text_str: String = env.get_string(&text).unwrap().into();
    window.handle_text_input(&text_str);
    window.process_window_events(0);
}

// Show soft keyboard
fn show_soft_keyboard(app: &AndroidApp) {
    app.show_soft_input(true);  // android-activity method
}

// Hide soft keyboard
fn hide_soft_keyboard(app: &AndroidApp) {
    app.hide_soft_input(true);
}
```

**For Phase 1-4 (without Java), basic keyboard works:**
```rust
fn handle_key_event(window: &mut AndroidWindow, key: &KeyEvent) {
    // Hardware key events and basic soft keyboard characters
    // are delivered as KeyEvent with key_code and unicode char
    if let Some(ch) = key.unicode_char() {
        if ch != 0 {
            let text = char::from_u32(ch as u32).map(|c| c.to_string());
            if let Some(t) = text {
                window.handle_text_input(&t);
            }
        }
    }
}
```
This gives basic ASCII input without the Java bridge. IME/CJK requires Phase 5.

### Phase 6: Lifecycle & Configuration Changes

**Android lifecycle events (mapped to Azul):**

| Android Event | Azul Action |
|---|---|
| `MainEvent::InitWindow` | Create rendering surface, trigger first layout |
| `MainEvent::TerminateWindow` | Release rendering surface (pixmap retained) |
| `MainEvent::Resume` | Re-enable timers, resume animations |
| `MainEvent::Pause` | Pause timers, reduce CPU usage |
| `MainEvent::Destroy` | Clean shutdown |
| `MainEvent::ConfigChanged` | DPI or locale changed — re-query, re-layout |
| `MainEvent::WindowResized` | New dimensions — re-layout, resize pixmap |

**Orientation handling:**
`android:configChanges="orientation|screenSize"` in AndroidManifest.xml prevents Activity 
recreation on rotation. Instead, `MainEvent::WindowResized` is received and we re-layout.

```rust
MainEvent::WindowResized { .. } => {
    if let Some(ref nw) = window.native_window {
        let new_width = nw.width();
        let new_height = nw.height();
        window.common.current_window_state.size.width = new_width as f32;
        window.common.current_window_state.size.height = new_height as f32;
        window.common.frame_needs_regeneration = true;
    }
}
```

**Save/restore state:**
Android can kill the process when backgrounded. For a UI framework, the simplest approach is
to re-create the UI from scratch on `InitWindow`. The user's `RefAny` app state is lost unless
they serialize it — this matches how NativeActivity games work.

### Phase 7: Build Scripts & CI

1. **`scripts/build-android.sh`** — Complete build + package + deploy:
   ```bash
   #!/bin/bash
   set -e
   
   APP_NAME="azul_example"
   BUNDLE_ID="com.azul.example"
   TARGET=${1:-aarch64-linux-android}
   BUILD_TOOLS="$ANDROID_HOME/build-tools/34.0.0"
   PLATFORM="$ANDROID_HOME/platforms/android-34"
   
   # Map Rust target to APK ABI directory
   case "$TARGET" in
       aarch64-linux-android)     ABI="arm64-v8a" ;;
       armv7-linux-androideabi)   ABI="armeabi-v7a" ;;
       x86_64-linux-android)     ABI="x86_64" ;;
       i686-linux-android)       ABI="x86" ;;
   esac
   
   # Build
   cargo build --target $TARGET --release --lib
   
   # Package
   rm -rf build_android && mkdir -p build_android/lib/$ABI
   cp target/$TARGET/release/lib${APP_NAME}.so build_android/lib/$ABI/
   cp scripts/android/AndroidManifest.xml build_android/
   
   cd build_android
   $BUILD_TOOLS/aapt2 link \
       --manifest AndroidManifest.xml \
       -I $PLATFORM/android.jar \
       -o base.apk
   
   cd lib && zip -r ../base.apk $ABI/ && cd ..
   $BUILD_TOOLS/zipalign -f 4 base.apk aligned.apk
   
   # Create debug keystore if it doesn't exist
   if [ ! -f debug.keystore ]; then
       keytool -genkeypair \
           -keystore debug.keystore -alias androiddebugkey \
           -keyalg RSA -keysize 2048 -validity 10000 \
           -storepass android -keypass android \
           -dname "CN=Android Debug,O=Android,C=US"
   fi
   
   $BUILD_TOOLS/apksigner sign \
       --ks debug.keystore --ks-key-alias androiddebugkey \
       --ks-pass pass:android aligned.apk
   
   # Deploy
   adb install -r aligned.apk
   adb shell am start -n $BUNDLE_ID/android.app.NativeActivity
   ```

2. **`scripts/android/AndroidManifest.xml`** — Template (shown above)

3. **Debug keystore generation** — one-time, included in build script

**Emulator testing from macOS:**
```bash
# Install emulator (one-time)
sdkmanager "system-images;android-34;google_apis;arm64-v8a" "emulator"

# Create AVD
avdmanager create avd -n azul_test -k "system-images;android-34;google_apis;arm64-v8a"

# Launch emulator
emulator -avd azul_test &

# Build and deploy
./scripts/build-android.sh x86_64-linux-android  # for x86 emulator
# or
./scripts/build-android.sh aarch64-linux-android  # for ARM emulator (Apple Silicon)
```

**CI (GitHub Actions):**
```yaml
- name: Build Android
  run: |
    # Install NDK
    sdkmanager "ndk;27.0.12077973" "build-tools;34.0.0" "platforms;android-34"
    rustup target add aarch64-linux-android
    cargo ndk -t arm64-v8a build --release --lib
```

### Phase 8: Play Store Submission

**AAB format (required for Play Store since Aug 2021):**

APK is fine for development/sideloading. Play Store requires AAB (Android App Bundle).
Use `bundletool` (Google's tool, a Java JAR):

```bash
# Convert APK structure to AAB module format
$BUILD_TOOLS/aapt2 link \
    --proto-format \
    --manifest AndroidManifest.xml \
    -I $PLATFORM/android.jar \
    -o base.zip

# Add lib/ to the module zip
cd lib && zip -r ../base.zip $ABI/ && cd ..

# Build AAB
java -jar bundletool.jar build-bundle \
    --modules=base.zip \
    --output=myapp.aab

# Sign AAB
jarsigner -keystore release.keystore myapp.aab myapp

# Upload to Play Store
# (via Play Console web UI or Play Developer API)
```

**Play Store requirements:**
- `targetSdkVersion` 34+ (Android 14)
- 64-bit native libraries (arm64-v8a)
- App signing by Google Play (you upload with your upload key, Google re-signs)
- Privacy policy
- Content rating questionnaire

## File Change Summary

| File | Change |
|---|---|
| `dll/src/desktop/shell2/android/mod.rs` | **New:** Full platform backend |
| `dll/src/desktop/shell2/mod.rs` | Add `android` module, cfg_if, backend name |
| `dll/Cargo.toml` | Add Android dependencies |
| `dll/build.rs` | Add NDK check, Android link libs |
| `dll/src/desktop/shell2/run.rs` | Add `#[cfg(target_os = "android")]` run function |
| `core/src/window.rs` | No changes (AndroidHandle already exists) |
| `scripts/build-android.sh` | **New:** Build + package + deploy script |
| `scripts/android/AndroidManifest.xml` | **New:** Template manifest |
| `scripts/android/NativeInputConnection.java` | **New:** Text input bridge (Phase 5) |

## Dependencies (dll/Cargo.toml)

```toml
[target.'cfg(target_os = "android")'.dependencies]
android-activity = { version = "0.6", features = ["native-activity"] }
ndk = "0.9"
jni = { version = "0.21", optional = true }  # Only needed for Phase 5 text input
```

The `android-activity` crate provides:
- `android_main` entry point
- `AndroidApp` handle for event polling
- `NativeWindow` access
- Input event types
- Lifecycle events
- `show_soft_input` / `hide_soft_input`

The `ndk` crate provides Rust bindings to:
- `ANativeWindow_lock` / `ANativeWindow_unlockAndPost`
- `ANativeWindow_setBuffersGeometry`
- `AConfiguration` (density, orientation)

## Priority Order

1. **Phase 1** (module structure + compilation) — Foundation, ~2 hours
2. **Phase 2** (CPU rendering) — Core value, ~1 day
3. **Phase 3** (event loop) — Makes it run, ~4 hours
4. **Phase 4** (touch input) — Makes it interactive, ~4 hours
5. **Phase 5** (text input with JNI) — Usable for text, ~1 day
6. **Phase 6** (lifecycle) — Required for production, ~4 hours
7. **Phase 7** (build scripts) — Distribution, ~2 hours
8. **Phase 8** (Play Store) — Release, ~2 hours

## Key Design Decisions

1. **NativeActivity over GameActivity**: Zero Java code for Phase 1-4. Text input is the only
   thing that needs Java, and it's ~50 lines compiled with javac+d8 (no Gradle).

2. **ANativeWindow_lock over EGL/Vulkan**: Simplest rendering path. No GPU setup, no shader
   compilation, no context management. For a UI framework doing CPU rendering, this is ideal.
   If profiling shows the blit is a bottleneck, upgrade to EGL texture blit later.

3. **android-activity crate over raw NDK**: Provides a clean Rust API, handles the complex 
   NativeActivity lifecycle dance (main thread vs native thread), and is maintained by the 
   Rust Android community. Used by winit, Bevy, wgpu.

4. **CPU-only initially**: Matches the headless backend pattern. GPU rendering (EGL/Vulkan)
   can be added later as an optimization, following the macOS dual-backend model.

5. **No Gradle**: The entire build uses `cargo build` + `aapt2` + `zipalign` + `apksigner`.
   These are standalone binaries from the Android SDK. The only Java needed is for text input
   (Phase 5), compiled with plain `javac` + `d8`.
