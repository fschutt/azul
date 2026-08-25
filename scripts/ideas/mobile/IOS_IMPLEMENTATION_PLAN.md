# iOS Implementation Plan

Reference: macOS backend (`dll/src/desktop/shell2/macos/mod.rs`) as golden standard.

## Current State

The iOS backend lives in `dll/src/desktop/shell2/ios/mod.rs` (~305 lines). It has:

- Basic `IOSWindow` struct with `CommonWindowState`
- UIWindow / UIViewController / AzulView creation via raw `objc` FFI
- AppDelegate with `application:didFinishLaunchingWithOptions:` 
- Skeleton `touchesBegan:withEvent:` handler (logs only)
- `PlatformWindow` trait impl with `impl_platform_window_getters!(common)`
- Always falls back to CPU rendering (GPU stub returns Err)

**Known bugs in current code:**
1. `dll/src/desktop/shell2/mod.rs:39` — `pub mod linux;` should be `pub mod ios;`
2. `core_graphics_sys` is imported but not declared in `dll/Cargo.toml`
3. `IOSHandle.ui_view` and `ui_view_controller` are always null
4. `run.rs:595` — `INITIAL_OPTIONS` signature includes `Option<Arc<FcFontRegistry>>` but `IOSWindow::new()` doesn't accept it
5. Uses old `objc` crate (0.2) with raw `msg_send!` — fine for now, migration to `objc2` is optional

## Architecture Overview

```
iOS App Process
├── main() → UIApplication::main() [never returns]
├── AppDelegate (ObjC class, registered dynamically from Rust)
│   └── application:didFinishLaunchingWithOptions:
│       ├── IOSWindow::new()
│       │   ├── UIWindow + UIViewController + AzulView
│       │   ├── CADisplayLink (frame timing)
│       │   ├── CommonWindowState (layout, hit test, resources)
│       │   └── CpuBackend (cpurender → AzulPixmap)
│       └── Store in global static AZUL_IOS_WINDOW
├── AzulView (UIView subclass, registered dynamically from Rust)
│   ├── drawRect: → render frame → blit AzulPixmap to CGImage → CALayer.contents
│   ├── touchesBegan:withEvent: → update state → process_window_events()
│   ├── touchesMoved:withEvent: → update state → process_window_events()
│   ├── touchesEnded:withEvent: → update state → process_window_events()
│   └── touchesCancelled:withEvent: → update state → process_window_events()
└── CADisplayLink → triggers redraw at display refresh rate
```

The key insight from macOS: all event processing goes through `PlatformWindow::process_window_events()` 
which is a default trait method. Platform code only needs to:
1. Translate native events into `FullWindowState` updates
2. Call `update_hit_test()` for pointer/touch events
3. Call `process_window_events(0)` 
4. Apply the returned `ProcessEventResult` (redraw, regenerate layout, etc.)

## Cross-Compilation Toolchain

**Targets (all Tier 2, available via rustup):**
```
aarch64-apple-ios          — physical devices
aarch64-apple-ios-sim      — Apple Silicon simulator
x86_64-apple-ios           — Intel simulator
```

**Prerequisites:**
- macOS host (mandatory — no Linux cross-compilation for iOS)
- Xcode installed (for SDK, linker, codesign)
- `xcode-select --install` for command-line tools

**Build commands:**
```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cargo build --target aarch64-apple-ios --release        # device
cargo build --target aarch64-apple-ios-sim --release    # simulator
```

**No Xcode project needed.** The entire build + bundle + sign + deploy flow uses CLI tools only:
- `xcrun` — tool dispatcher
- `codesign` — code signing
- `xcrun simctl` — simulator control
- `xcrun altool` — App Store upload

## .app Bundle Structure (Minimal)

```
MyAzulApp.app/
├── MyAzulApp                      # Mach-O executable
├── Info.plist                     # Required metadata
└── embedded.mobileprovision       # Only for physical device
```

**Minimal Info.plist:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>      <string>MyAzulApp</string>
    <key>CFBundleIdentifier</key>      <string>com.example.myazulapp</string>
    <key>CFBundleName</key>            <string>MyAzulApp</string>
    <key>CFBundleVersion</key>         <string>1</string>
    <key>CFBundleShortVersionString</key> <string>1.0</string>
    <key>MinimumOSVersion</key>        <string>16.0</string>
    <key>UILaunchStoryboardName</key>  <string></string>
    <key>UIRequiredDeviceCapabilities</key>
    <array><string>arm64</string></array>
    <key>UISupportedInterfaceOrientations</key>
    <array>
        <string>UIInterfaceOrientationPortrait</string>
        <string>UIInterfaceOrientationLandscapeLeft</string>
        <string>UIInterfaceOrientationLandscapeRight</string>
    </array>
</dict>
</plist>
```

Note: Empty `UILaunchStoryboardName` gives a full-screen app without a storyboard file.

**Simulator (no signing needed):**
```bash
mkdir -p MyAzulApp.app
cp target/aarch64-apple-ios-sim/release/myazulapp MyAzulApp.app/MyAzulApp
cp Info.plist MyAzulApp.app/
xcrun simctl install booted MyAzulApp.app
xcrun simctl launch --console booted com.example.myazulapp
```

**Physical device (requires signing):**
```bash
codesign --force --timestamp=none \
  --sign "Apple Development: dev@example.com (TEAMID)" \
  --entitlements entitlements.xcent MyAzulApp.app
ios-deploy --bundle MyAzulApp.app --justlaunch
```

## Implementation Phases

### Phase 1: Fix Compilation & Build Infrastructure

**Goal:** `cargo build --target aarch64-apple-ios-sim` compiles without errors.

**Files to modify:**

1. **`dll/src/desktop/shell2/mod.rs:39`** — Fix module declaration:
   ```rust
   // BEFORE (bug):
   #[cfg(target_os = "ios")]
   pub mod linux;
   
   // AFTER:
   #[cfg(target_os = "ios")]
   pub mod ios;
   ```

2. **`dll/Cargo.toml`** — Fix iOS dependencies. Remove `core_graphics_sys` usage from ios/mod.rs 
   (use raw FFI for CGRect instead, or add the dep). The `CGRect` type is just:
   ```rust
   #[repr(C)]
   struct CGPoint { x: f64, y: f64 }
   #[repr(C)]
   struct CGSize { width: f64, height: f64 }
   #[repr(C)]
   struct CGRect { origin: CGPoint, size: CGSize }
   ```
   Define these locally in `ios/mod.rs` to avoid the extra dependency.

3. **`dll/build.rs`** — Make `ios-deploy` check a warning instead of a panic (not everyone 
   has ios-deploy installed, and it's not needed for simulator testing):
   ```rust
   fn check_ios_deploy() {
       let status = Command::new("ios-deploy").arg("--version").status();
       match status {
           Ok(s) if s.success() => { /* All good */ }
           _ => {
               println!("cargo:warning=ios-deploy not found. Install with: brew install ios-deploy");
               println!("cargo:warning=Without it, you cannot deploy to physical iOS devices.");
           }
       }
   }
   ```

4. **`dll/build.rs`** — Add simulator runner alongside device runner:
   ```bash
   # scripts/ios-sim-runner.sh
   xcrun simctl install booted "$APP_BUNDLE_PATH"
   xcrun simctl launch --console booted "$BUNDLE_ID"
   ```

5. **`run.rs:595`** — Fix `INITIAL_OPTIONS` to pass `font_registry` to `IOSWindow::new()` 
   or update `IOSWindow::new()` signature to accept it.

### Phase 2: CPU Rendering Pipeline

**Goal:** Render an actual Azul UI to the screen using CPU compositing.

The macOS CPU backend flow is:
```
Layout pass → DisplayList → cpurender::render() → AzulPixmap (RGBA8) → NSBitmapImageRep → CPUView
```

For iOS, the equivalent is:
```
Layout pass → DisplayList → cpurender::render() → AzulPixmap (RGBA8) → CGImage → CALayer.contents
```

**Implementation in `ios/mod.rs`:**

1. **Add CpuBackend to IOSWindow** (mirror headless/mod.rs CpuBackend):
   ```rust
   use azul_layout::cpurender::{self, AzulPixmap, CompositorState};
   use azul_layout::headless::CpuHitTester;
   
   pub struct IOSWindow {
       ui_window: Id<Object>,
       custom_view: Id<Object>,  // Store reference to AzulView
       backend: RenderBackend,
       is_open: bool,
       // CPU rendering state
       cpu_pixmap: Option<AzulPixmap>,          // Retained pixel buffer
       cpu_compositor: Option<CompositorState>,  // Layer compositor
       cpu_glyph_cache: cpurender::GlyphCache,   // Text cache
       last_display_list: Option<DisplayList>,    // For damage computation
       // Common fields
       pub common: event::CommonWindowState,
   }
   ```

2. **Implement `drawRect:` to blit AzulPixmap → CGImage → CALayer:**
   ```rust
   extern "C" fn draw_rect(_self: &Object, _cmd: Sel, _rect: CGRect) {
       let window = unsafe { &mut *AZUL_IOS_WINDOW };
       
       // 1. Run layout if needed (first frame or after callback)
       if window.common.frame_needs_regeneration {
           window.regenerate_layout();  // Uses PlatformWindow default impl
       }
       
       // 2. Get display list from layout results
       let display_list = window.build_display_list();
       
       // 3. Compute damage rects (incremental rendering)
       let damage = match &window.last_display_list {
           Some(old) => cpurender::compute_display_list_damage(old, &display_list),
           None => None, // Full repaint
       };
       
       // 4. Render to retained pixmap
       let (w, h) = window.get_window_size_pixels();
       let pixmap = window.cpu_pixmap.get_or_insert_with(|| AzulPixmap::new(w, h));
       pixmap.resize_grow_only(w, h);
       
       match damage {
           Some(rects) => cpurender::render_display_list_damaged(
               &display_list, pixmap, dpi,
               &window.common.renderer_resources,
               &window.common.fc_cache,
               &window.cpu_glyph_cache, &rects,
           ),
           None => cpurender::render(
               &display_list, pixmap, dpi,
               &window.common.renderer_resources,
               &window.common.fc_cache,
               &window.cpu_glyph_cache,
           ),
       };
       
       window.last_display_list = Some(display_list);
       
       // 5. Blit pixmap to CGImage → CALayer.contents
       unsafe {
           let data = pixmap.data();
           let cgimage = create_cgimage_from_rgba(data, w, h);
           let layer: *mut Object = msg_send![_self, layer];
           let _: () = msg_send![layer, setContents: cgimage];
       }
   }
   ```

3. **CGImage creation from RGBA buffer** (Core Graphics FFI):
   ```rust
   #[link(name = "CoreGraphics", kind = "framework")]
   extern "C" {
       fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
       fn CGColorSpaceRelease(cs: *mut c_void);
       fn CGDataProviderCreateWithData(
           info: *mut c_void,
           data: *const u8,
           size: usize,
           release: Option<extern "C" fn(*mut c_void, *const u8, usize)>,
       ) -> *mut c_void;
       fn CGDataProviderRelease(provider: *mut c_void);
       fn CGImageCreate(
           width: usize, height: usize,
           bitsPerComponent: usize, bitsPerPixel: usize,
           bytesPerRow: usize,
           space: *mut c_void,
           bitmapInfo: u32,
           provider: *mut c_void,
           decode: *const f64,
           shouldInterpolate: bool,
           intent: u32,
       ) -> *mut c_void;  // CGImageRef
   }
   
   const K_CG_IMAGE_ALPHA_PREMULTIPLIED_LAST: u32 = 1;
   const K_CG_BITMAP_BYTE_ORDER_DEFAULT: u32 = 0;
   const K_CG_RENDERING_INTENT_DEFAULT: u32 = 0;
   
   unsafe fn create_cgimage_from_rgba(pixels: &[u8], width: u32, height: u32) -> *mut c_void {
       let cs = CGColorSpaceCreateDeviceRGB();
       let provider = CGDataProviderCreateWithData(
           ptr::null_mut(),
           pixels.as_ptr(),
           pixels.len(),
           None,
       );
       let image = CGImageCreate(
           width as usize, height as usize,
           8, 32,
           (width as usize) * 4,
           cs,
           K_CG_IMAGE_ALPHA_PREMULTIPLIED_LAST | K_CG_BITMAP_BYTE_ORDER_DEFAULT,
           provider,
           ptr::null(),
           false,
           K_CG_RENDERING_INTENT_DEFAULT,
       );
       CGDataProviderRelease(provider);
       CGColorSpaceRelease(cs);
       image
   }
   ```

4. **CADisplayLink for frame timing** (replaces macOS CVDisplayLink):
   ```rust
   // Register a display link callback on AzulView
   extern "C" fn display_link_fired(_self: &Object, _cmd: Sel, _link: *mut Object) {
       let window = unsafe { &mut *AZUL_IOS_WINDOW };
       if window.needs_redraw() {
           let view = &window.custom_view;
           let _: () = unsafe { msg_send![view, setNeedsDisplay] };
       }
   }
   
   // In IOSWindow::new(), after creating the view:
   fn setup_display_link(view: &Id<Object>) {
       unsafe {
           let display_link: Id<Object> = msg_send![
               class!(CADisplayLink),
               displayLinkWithTarget: view.as_ptr()
               selector: sel!(displayLinkFired:)
           ];
           let run_loop: Id<Object> = msg_send![class!(NSRunLoop), mainRunLoop];
           let mode: Id<Object> = msg_send![class!(NSString), 
               stringWithUTF8String: "kCFRunLoopDefaultMode\0".as_ptr()];
           let _: () = msg_send![display_link, addToRunLoop: run_loop forMode: mode];
       }
   }
   ```

### Phase 3: Touch Event Handling

**Goal:** Touch events drive the Azul event system (hover, click, scroll).

The macOS backend translates `NSEvent` → `FullWindowState` updates → `process_window_events()`.
iOS must translate `UITouch` → `FullWindowState` updates → `process_window_events()`.

**Key mapping:**

| UITouch phase | Azul state update |
|---|---|
| `touchesBegan` | `mouse_state.left_down = true`, `cursor_position = touch.location` |
| `touchesMoved` | `cursor_position = touch.location` |
| `touchesEnded` | `mouse_state.left_down = false` |
| `touchesCancelled` | `mouse_state.left_down = false` |

Note: Map first touch to mouse left button for compatibility with the existing click/hover system.
Multi-touch generates `TouchStart`/`TouchMove`/`TouchEnd` events via `EventFilter::Touch`.

**Implementation:**

```rust
extern "C" fn touches_began(_self: &Object, _cmd: Sel, touches: *mut Object, _event: *mut Object) {
    let window = unsafe { &mut *AZUL_IOS_WINDOW };
    
    // Get the set of touches
    let all_objects: *mut Object = unsafe { msg_send![touches, allObjects] };
    let count: usize = unsafe { msg_send![all_objects, count] };
    
    for i in 0..count {
        let touch: *mut Object = unsafe { msg_send![all_objects, objectAtIndex: i] };
        let location: CGPoint = unsafe { 
            msg_send![touch, locationInView: window.custom_view.as_ptr()] 
        };
        let scale: f64 = unsafe {
            let screen: *mut Object = msg_send![class!(UIScreen), mainScreen];
            msg_send![screen, scale]
        };
        
        let logical_x = location.x as f32;
        let logical_y = location.y as f32;
        
        // Update window state (first touch = mouse position)
        if i == 0 {
            window.common.current_window_state.mouse_state.cursor_position = 
                azul_core::window::CursorPosition::InWindow(
                    LogicalPosition::new(logical_x, logical_y)
                );
            window.common.current_window_state.mouse_state.left_down = true;
        }
        
        // Update hit test for the new position
        if let Some(ref mut cpu_ht) = window.common.cpu_hit_tester {
            cpu_ht.hit_test(LogicalPosition::new(logical_x, logical_y));
        }
    }
    
    // Run unified event processing
    window.process_window_events(0);
    
    // Check if we need to redraw
    let _: () = unsafe { msg_send![window.custom_view, setNeedsDisplay] };
}

// touches_moved, touches_ended, touches_cancelled follow the same pattern
```

**Multi-touch support** (for scroll gestures):
- Two-finger pan → translate to scroll events
- Pinch → could map to zoom (optional)
- The core framework already has `TouchEventData { id, position, force }` and 
  `EventFilter::Touch(TouchStart/TouchMove/TouchEnd/TouchCancel)`

### Phase 4: Text Input

**Two levels, implement in order:**

#### Level 1: UIKeyInput (basic keyboard, ~50 lines)

Add to AzulView class registration:
```rust
fn get_or_create_view_class() -> &'static Class {
    // ... existing code ...
    // Add UIKeyInput methods:
    decl.add_method(sel!(canBecomeFirstResponder), 
        can_become_first_responder as extern "C" fn(&Object, Sel) -> bool);
    decl.add_method(sel!(hasText), 
        has_text as extern "C" fn(&Object, Sel) -> bool);
    decl.add_method(sel!(insertText:), 
        insert_text as extern "C" fn(&Object, Sel, *mut Object));
    decl.add_method(sel!(deleteBackward), 
        delete_backward as extern "C" fn(&Object, Sel));
    
    // Conform to UIKeyInput protocol
    let protocol = Protocol::get("UIKeyInput").unwrap();
    decl.add_protocol(protocol);
}

extern "C" fn can_become_first_responder(_self: &Object, _cmd: Sel) -> bool { true }

extern "C" fn has_text(_self: &Object, _cmd: Sel) -> bool {
    let window = unsafe { &*AZUL_IOS_WINDOW };
    // Check if focused text field has content
    window.common.layout_window.as_ref()
        .map(|lw| lw.text_edit_manager.has_text())
        .unwrap_or(false)
}

extern "C" fn insert_text(_self: &Object, _cmd: Sel, text: *mut Object) {
    let window = unsafe { &mut *AZUL_IOS_WINDOW };
    let utf8: *const u8 = unsafe { msg_send![text, UTF8String] };
    let c_str = unsafe { std::ffi::CStr::from_ptr(utf8 as *const i8) };
    let text_str = c_str.to_str().unwrap_or("");
    
    // Feed text through the input interpreter
    // (same path as macOS insertText: from NSTextInputClient)
    window.handle_text_input(text_str);
    window.process_window_events(0);
}

extern "C" fn delete_backward(_self: &Object, _cmd: Sel) {
    let window = unsafe { &mut *AZUL_IOS_WINDOW };
    window.handle_key_down(VirtualKeyCode::Back);
    window.process_window_events(0);
}
```

Show/hide keyboard:
```rust
// Show: when a text input node gets focus
let _: () = unsafe { msg_send![window.custom_view, becomeFirstResponder] };

// Hide: when focus leaves text input
let _: () = unsafe { msg_send![window.custom_view, resignFirstResponder] };
```

#### Level 2: UITextInput (full IME, CJK) — future work

This requires implementing the `UITextInput` protocol with:
- Custom `UITextPosition` / `UITextRange` subclasses
- `markedTextRange` / `selectedTextRange` properties
- `setMarkedText:selectedRange:` for IME preedit
- `unmarkText` for committing composed text
- Integration with Azul's `CursorManager` preedit system

This is complex (~300-500 lines) and mirrors the macOS `NSTextInputClient` implementation.
Defer to a later phase — UIKeyInput covers ASCII/Latin text input.

### Phase 5: Lifecycle & Orientation

**AppDelegate lifecycle methods to add:**

```rust
// In get_or_create_app_delegate_class():
decl.add_method(sel!(applicationDidBecomeActive:),
    did_become_active as extern "C" fn(&Object, Sel, *mut Object));
decl.add_method(sel!(applicationWillResignActive:),
    will_resign_active as extern "C" fn(&Object, Sel, *mut Object));
decl.add_method(sel!(applicationDidEnterBackground:),
    did_enter_background as extern "C" fn(&Object, Sel, *mut Object));
decl.add_method(sel!(applicationWillEnterForeground:),
    will_enter_foreground as extern "C" fn(&Object, Sel, *mut Object));
```

**Orientation changes** — handled by `android:configChanges` equivalent:
In Info.plist, declare supported orientations. When rotation happens:
1. iOS calls `viewWillTransitionToSize:withTransitionCoordinator:` on the view controller
2. The view's `layoutSubviews` is called with new bounds
3. We detect the size change in `drawRect:` and trigger a full re-layout

Add to AzulView:
```rust
decl.add_method(sel!(layoutSubviews), 
    layout_subviews as extern "C" fn(&Object, Sel));

extern "C" fn layout_subviews(_self: &Object, _cmd: Sel) {
    let window = unsafe { &mut *AZUL_IOS_WINDOW };
    let bounds: CGRect = unsafe { msg_send![_self, bounds] };
    let new_width = bounds.size.width as f32;
    let new_height = bounds.size.height as f32;
    
    // Update window state
    window.common.current_window_state.size.width = new_width;
    window.common.current_window_state.size.height = new_height;
    window.common.frame_needs_regeneration = true;
    
    // Resize CPU pixmap
    if let Some(ref mut pixmap) = window.cpu_pixmap {
        pixmap.resize_grow_only(new_width as u32, new_height as u32);
    }
}
```

**DPI/Scale factor:**
```rust
let screen: *mut Object = unsafe { msg_send![class!(UIScreen), mainScreen] };
let scale: f64 = unsafe { msg_send![screen, scale] };  // 2.0 or 3.0
// Store as DpiScaleFactor in FullWindowState
```

**Safe area insets** (notch/Dynamic Island):
```rust
// iOS 11+
let insets: UIEdgeInsets = unsafe { msg_send![window.ui_window, safeAreaInsets] };
// insets.top, .bottom, .left, .right — account for in layout
```

### Phase 6: Clipboard & Accessibility

**UIPasteboard** (clipboard):
```rust
// Copy
let pasteboard: *mut Object = unsafe { msg_send![class!(UIPasteboard), generalPasteboard] };
let ns_string: *mut Object = unsafe { msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()] };
let _: () = unsafe { msg_send![pasteboard, setString: ns_string] };

// Paste  
let string: *mut Object = unsafe { msg_send![pasteboard, string] };
if !string.is_null() {
    let utf8: *const u8 = unsafe { msg_send![string, UTF8String] };
    // ...
}
```

**Accessibility** (VoiceOver):
- Wire `a11y_dirty` flag to rebuild accessibility tree
- Implement `isAccessibilityElement`, `accessibilityLabel`, `accessibilityTraits` on AzulView
- Or use `accesskit_macos` equivalent for iOS (check accesskit crate for iOS support)
- The `CommonWindowState.a11y_dirty` flag already exists

### Phase 7: Build Scripts & CI

**Add to `scripts/`:**

1. **`scripts/build-ios.sh`** — Complete build + bundle script:
   ```bash
   #!/bin/bash
   set -e
   TARGET=${1:-aarch64-apple-ios-sim}
   APP_NAME="AzulExample"
   BUNDLE_ID="com.azul.example"
   
   cargo build --target $TARGET --release -p azul-example
   
   rm -rf ${APP_NAME}.app
   mkdir -p ${APP_NAME}.app
   cp target/${TARGET}/release/azul-example ${APP_NAME}.app/${APP_NAME}
   cp scripts/ios/Info.plist ${APP_NAME}.app/
   
   if [[ "$TARGET" == *"-sim"* ]] || [[ "$TARGET" == "x86_64-apple-ios" ]]; then
       xcrun simctl install booted ${APP_NAME}.app
       xcrun simctl launch --console booted $BUNDLE_ID
   else
       codesign --force --timestamp=none \
           --sign "${IOS_SIGNING_IDENTITY}" \
           --entitlements scripts/ios/entitlements.xcent \
           ${APP_NAME}.app
       ios-deploy --bundle ${APP_NAME}.app --justlaunch
   fi
   ```

2. **`scripts/ios/Info.plist`** — Template Info.plist (shown above)

3. **`scripts/ios/entitlements.xcent`** — Template entitlements

**App Store submission** requires:
- IPA: `mkdir Payload && cp -r MyApp.app Payload/ && zip -r MyApp.ipa Payload/`
- Asset catalog with 1024x1024 icon: `xcrun actool Assets.xcassets --compile MyApp.app --platform iphoneos --minimum-deployment-target 16.0`
- Upload: `xcrun altool --upload-app -f MyApp.ipa --apiKey KEY --apiIssuer ISSUER --type ios`

## File Change Summary

| File | Change |
|---|---|
| `dll/src/desktop/shell2/mod.rs:39` | Fix `pub mod linux` → `pub mod ios` |
| `dll/src/desktop/shell2/ios/mod.rs` | Rewrite: CPU rendering, touch events, text input, lifecycle |
| `dll/Cargo.toml` | Remove `core_graphics_sys` from iOS deps (define CGRect locally) |
| `dll/build.rs` | Make `ios-deploy` check non-fatal; add simulator runner |
| `dll/src/desktop/shell2/run.rs:595` | Fix INITIAL_OPTIONS signature |
| `core/src/window.rs` | No changes needed (IOSHandle already defined) |
| `scripts/build-ios.sh` | New: build + bundle + deploy script |
| `scripts/ios/Info.plist` | New: template |
| `scripts/ios/entitlements.xcent` | New: template |

## Dependencies (dll/Cargo.toml)

Current iOS dependencies are fine for Phase 1-5:
```toml
[target.'cfg(target_os = "ios")'.dependencies]
objc = "0.2"
objc-foundation = "0.1"
objc_id = "0.1"
```

No additional crates needed. All UIKit/CoreGraphics/QuartzCore APIs are called via raw FFI 
(matching the existing pattern). The `objc` 0.2 crate provides `msg_send!`, `class!`, `sel!`,
and `ClassDecl` which is sufficient.

**Optional future migration to objc2:**
```toml
[target.'cfg(target_os = "ios")'.dependencies]
objc2 = "0.6.4"
objc2-foundation = { version = "0.3.2", features = ["NSObject", "NSString", "NSRunLoop"] }
objc2-ui-kit = { version = "0.3.2", features = ["UIApplication", "UIView", "UIWindow", ...] }
objc2-quartz-core = { version = "0.3.2", features = ["CALayer", "CADisplayLink"] }
objc2-core-graphics = "0.3.2"
```
This migration is optional — the raw `objc` + FFI approach works and matches the existing code.

## Testing Strategy

1. **Headless E2E tests** — Already work via `AZ_BACKEND=headless AZ_E2E=tests.json`. 
   These test the cross-platform event/layout/callback logic. No iOS-specific work needed.

2. **Simulator testing** — `cargo build --target aarch64-apple-ios-sim` + deploy via `xcrun simctl`.
   Manual testing of touch, rendering, orientation.

3. **CI** — GitHub Actions macOS runners have Xcode pre-installed. Add a job:
   ```yaml
   - name: Build iOS (simulator)
     run: |
       rustup target add aarch64-apple-ios-sim
       cargo build --target aarch64-apple-ios-sim -p azul-dll
   ```

## Priority Order

1. **Phase 1** (compilation) — Immediate, ~1 hour
2. **Phase 2** (CPU rendering) — Core value, ~1 day  
3. **Phase 3** (touch events) — Makes it interactive, ~4 hours
4. **Phase 4 Level 1** (basic text input) — Usable for text, ~2 hours
5. **Phase 5** (lifecycle/orientation) — Required for production, ~4 hours
6. **Phase 6** (clipboard/a11y) — Polish, ~1 day
7. **Phase 7** (build scripts/CI) — Distribution, ~2 hours
8. **Phase 4 Level 2** (full IME) — CJK support, ~2 days
