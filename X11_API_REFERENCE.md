# X11 / Xlib / EGL API Reference for the Native GUI Backend

Primary-source-grounded reference for fixing the azul X11 backend (raw Xlib via dlopen + EGL for GL).
Compiled 2026-06-03 (web research). Companion to `HANDOFF_LINUX_X11.md` (§10 lists which Wayland bugs
also exist on X11). Keep this next to the code while implementing the X11 fixes. Sources at the bottom.

---

## 1. EGL-on-X11 presentation & the default framebuffer (HIGH — "GPU renders garbage in undrawn regions")

### 1.1 `eglSwapBuffers` — is the back buffer defined after a swap?

```c
EGLBoolean eglSwapBuffers(EGLDisplay display, EGLSurface surface);
```

Decisive rule (Khronos EGL reference, verbatim):
- "The contents of the color buffer are left unchanged if the value of the `EGL_SWAP_BEHAVIOR`
  attribute of *surface* is `EGL_BUFFER_PRESERVED`, and are **undefined if the value is
  `EGL_BUFFER_DESTROYED`**."
- "The contents of **ancillary buffers are always undefined** after calling `eglSwapBuffers`."

So unless you explicitly opted into `EGL_BUFFER_PRESERVED` (and the config supports it — see 1.2), the
entire color buffer (FBO 0) holds **undefined contents** after a swap = recycled VRAM = garbage. Depth/
stencil/multisample are *always* undefined regardless. This is exactly the "garbage in undrawn regions"
symptom: any pixel not overwritten this frame shows recycled buffer content.

Other semantics: `eglSwapBuffers` implicitly flushes the bound context; it is a no-op (no error) on
pbuffer/pixmap surfaces; the default `EGL_SWAP_BEHAVIOR` is **implementation-chosen** (cannot assume
PRESERVED; on EGL < 1.2 it is effectively always DESTROYED).

### 1.2 `EGL_SWAP_BEHAVIOR` via `eglSurfaceAttrib`

```c
EGLBoolean eglSurfaceAttrib(EGLDisplay display, EGLSurface surface, EGLint attribute, EGLint value);
```
- `attribute = EGL_SWAP_BEHAVIOR`, `value ∈ { EGL_BUFFER_DESTROYED, EGL_BUFFER_PRESERVED }`.
- Initial value implementation-chosen; requires EGL ≥ 1.2.
- **Critical:** requesting `EGL_BUFFER_PRESERVED` raises `EGL_BAD_MATCH` if the surface's `EGLConfig`
  `EGL_SURFACE_TYPE` lacks `EGL_SWAP_BEHAVIOR_PRESERVED_BIT`. So you must pick such a config at
  `eglChooseConfig` time and check the `eglSurfaceAttrib` return. Many drivers don't expose the bit.

**Implication:** relying on preserved content is fragile. The robust fix is to treat FBO 0 as
**undefined every frame** and fully define it.

### 1.3 Must the app clear / fully overwrite FBO 0 every frame? — YES

Correct per-frame on-screen sequence:
```c
eglMakeCurrent(dpy, surface, surface, ctx);   // bind the right surface as draw+read
glBindFramebuffer(GL_FRAMEBUFFER, 0);          // target the default framebuffer (the window)
glViewport(0, 0, fb_width, fb_height);         // match current drawable size (px)
glClearColor(r, g, b, a);
glClear(GL_COLOR_BUFFER_BIT /* | GL_DEPTH_BUFFER_BIT | GL_STENCIL_BUFFER_BIT */);
// ... draw / composite onto FBO 0 ...
eglSwapBuffers(dpy, surface);
```
If you composite from an offscreen FBO/texture via a full-screen quad covering the whole viewport, that
also fully defines FBO 0 and the `glClear` is redundant — but clearing is cheap insurance and is
mandatory if your draw does not cover every pixel. "Pixels that are not owned will have undefined values."

### 1.4 Partial-present / damage extensions (and how they cause garbage if misused)

Two complementary extensions solving DIFFERENT problems:

**`EGL_KHR_swap_buffers_with_damage`** (surface damage = what changed between back and front; a hint to
the *compositor*):
```c
EGLBoolean eglSwapBuffersWithDamageKHR(EGLDisplay dpy, EGLSurface surface, EGLint *rects, EGLint n_rects);
```
- Each rect = 4 `EGLint` in `{x, y, width, height}` order, **relative to the bottom-left** of the
  surface (GL origin, NOT Xlib top-left).
- **Load-bearing gotcha (verbatim):** "the **entire contents of the back buffer will still be swapped to
  the front** so applications using this API must **still ensure that the entire back buffer is
  consistent**." Damage rects are only a compositor optimization; they do NOT excuse defining the whole
  buffer. Clearing only the damaged sub-rect → garbage elsewhere.
- Errors: `EGL_BAD_PARAMETER` if `n_rects < 0`, or `n_rects > 0` with `rects == NULL`.

**`EGL_KHR_partial_update`** (buffer damage = what changed in *this* back buffer since last used; lets the
driver skip untouched regions):
```c
EGLBoolean eglSetDamageRegionKHR(EGLDisplay dpy, EGLSurface surface, EGLint *rects, EGLint n_rects);
```
- Same encoding, lower-left origin.
- Must be called **before any draw commands this frame** (after the previous swap / surface creation).
  Calling it twice in a frame → `EGL_BAD_ACCESS`.
- Pixels OUTSIDE the damage region retain last-defined content, BUT "**any client API rendering which
  falls outside of the damage region results in undefined framebuffer contents for the entire
  framebuffer**." Wrong region / drawing outside it → whole-buffer garbage.

**Buffer age** (prerequisite for correct partial update) — desktop/Mesa variant `EGL_EXT_buffer_age`
(`EGL_BUFFER_AGE_EXT = 0x313D`): `eglQuerySurface(dpy, surface, EGL_BUFFER_AGE_EXT, &age)`. "An age of 0
means the buffer has only just been initialized and the contents are **undefined**"; age *n* = contents
are from *n* frames ago. Mesa returns `-1` on error, `0` when no back buffer. Correct use: `age == 0` →
redraw the WHOLE surface; `age == n` → may reuse and only repaint accumulated damage of last *n* frames.

### 1.5 `eglSwapInterval` (vsync) + binding the right surface
- `eglSwapInterval(dpy, interval)`: `0` = no vsync (tear, low latency), `1` = sync to vblank. Affects the
  surface bound to the calling thread — call AFTER `eglMakeCurrent`.
- Always `eglMakeCurrent` the intended window surface before `glClear`/draw/`eglSwapBuffers`. Swapping with
  the wrong/no surface current → `EGL_BAD_SURFACE`/`EGL_BAD_DISPLAY` or presents the wrong buffer.

---

## 2. Xlib event loop & window lifecycle (HIGH)

### 2.1 Non-blocking drain: `XNextEvent` / `XPending` / `XEventsQueued` / `XCheck*`
- `XNextEvent(dpy, &ev)` **blocks** until an event is available (flushes output first).
- `XPending(dpy)` = events received but not dequeued = `XEventsQueued(dpy, QueuedAfterFlush)`.
- `XEventsQueued(dpy, mode)`: `QueuedAlready` (no syscall), `QueuedAfterReading` (read without flush),
  `QueuedAfterFlush` (flush then read).
- `XCheck*` family is **non-blocking** (`XCheckIfEvent`, `XCheckWindowEvent`, `XCheckMaskEvent`,
  `XCheckTypedEvent`, `XCheckTypedWindowEvent`) — return `False` when nothing matches.

Recommended non-blocking drain:
```c
XFlush(dpy);
int n = XPending(dpy);             // = XEventsQueued(QueuedAfterFlush)
for (int i = 0; i < n; i++) {      // or: while (XPending(dpy)) { ... }
    XEvent ev;
    XNextEvent(dpy, &ev);          // won't block: n events are queued
    if (XFilterEvent(&ev, None))   // §3 — IME must see every event
        continue;
    dispatch(&ev);
}
```
To integrate with poll()/epoll, watch `ConnectionNumber(dpy)` for readability, then drain as above.

`XFlush` flushes the output buffer (no wait). `XSync(dpy, discard)` flushes AND waits for the server to
process all requests (`discard=True` empties the input queue) — round-trip cost; use sparingly.

### 2.2 Expose events — the `count` field & partial repaint
`XExposeEvent { … int x, y, width, height; int count; }` (rect origin **top-left**, Xlib convention).
- **Rule:** "If `count` is zero, no more `Expose` events follow for this window." Simple apps "ignore all
  `Expose` events with nonzero counts and perform full redisplays on events with zero counts." For partial
  repaint, accumulate the rects and flush when `count == 0`.
- Generated when window regions lack valid contents (mapped, exposed by overlap removal, resized larger
  w/o backing store). Never for `InputOnly` windows.

### 2.3 `ConfigureNotify` — resize, avoiding configure storms
`XConfigureEvent { … int x, y; int width, height; int border_width; … }` — `width`/`height` are the
client (inside) size in px (use for `glViewport` + EGL surface resize). Select via `StructureNotifyMask`.
- **Avoid storms:** cache last `(width,height)`; react only when it actually changes (ignore pure moves +
  duplicate sizes). Robust pattern: drain the whole queue, keep only the LAST ConfigureNotify size, resize
  once per drain.

### 2.4 Window close — `WM_DELETE_WINDOW`
Setup (after create, before map):
```c
Atom wm_delete = XInternAtom(dpy, "WM_DELETE_WINDOW", False);
XSetWMProtocols(dpy, win, &wm_delete, 1);
```
Detect — arrives as a `ClientMessage` whose `data.l[0]` is the `WM_DELETE_WINDOW` atom:
```c
if (ev.type == ClientMessage && (Atom)ev.xclient.data.l[0] == wm_delete) {
    /* close button -> begin shutdown */
}
```
The server does NOT destroy the window for you; you receive this and decide.

### 2.5 `MapNotify` / first Expose — when safe to render
`XMapWindow` → (with `StructureNotifyMask`) `MapNotify` when viewable → the first `Expose` is the
canonical "safe to draw" signal. Also ensure the EGL window surface was created and `eglMakeCurrent` is
bound before the first draw.

### 2.6 `XSelectInput` masks
```c
XSelectInput(dpy, win,
    ExposureMask | StructureNotifyMask
  | KeyPressMask | KeyReleaseMask
  | ButtonPressMask | ButtonReleaseMask | PointerMotionMask
  | EnterWindowMask | LeaveWindowMask
  | FocusChangeMask);   // FocusIn/FocusOut drive XSetICFocus/XUnsetICFocus
```
**Gotcha:** after `XCreateIC`, query `XGetICValues(ic, XNFilterEvents, &mask, NULL)` and OR that into your
`XSelectInput`, or the IM won't receive the events it needs.

---

## 3. Keyboard input & text / IME (HIGH — text-input bug + Japanese IME)

### 3.1 `XLookupString` vs `XmbLookupString` vs `Xutf8LookupString`
```c
int XLookupString  (XKeyEvent *event, char *buf, int n, KeySym *ks, XComposeStatus *cs);
int XmbLookupString (XIC ic, XKeyPressedEvent *ev, char *buf,    int bytes,  KeySym *ks, Status *st);
int Xutf8LookupString(XIC ic, XKeyPressedEvent *ev, char *buf,   int bytes,  KeySym *ks, Status *st);
int XwcLookupString (XIC ic, XKeyPressedEvent *ev, wchar_t *buf, int wchars, KeySym *ks, Status *st);
```
- `XLookupString` = Latin-1 / no IME — only for raw keysym lookup. **For UTF-8 text use
  `Xutf8LookupString` with an `XIC`.**
- The Mb/utf8/wc variants return committed-string length; the `KeySym` is returned separately (for
  non-text keys like arrows/Enter/F-keys).

**Status return (drives text-vs-command handling):**
| Status | buffer | keysym | int return |
|---|---|---|---|
| `XLookupNone` | — | — | 0 (no consistent input yet, e.g. mid-preedit) |
| `XLookupChars` | filled (committed text) | — | length |
| `XLookupKeySym` | — | filled | 0 |
| `XLookupBoth` | filled | filled | length |
| `XBufferOverflow` | — | — | required byte size — recall with bigger buffer |

**Critical rules:**
- Pass only `KeyPress` events (behavior on `KeyRelease` is undefined). Filter `ev.type == KeyPress` first.
- On `XBufferOverflow`, re-call with the returned size. 32–64 byte stack buffer covers most single commits.
- Insert into the text model only on `XLookupChars`/`XLookupBoth`; treat `XLookupKeySym`/`XLookupBoth` as a
  navigation/command key. **(This is the X11 analog of the Wayland backspace-tofu fix: a control keysym
  with no committed string must NOT be inserted as text.)**

### 3.2 XKB / keycode → keysym / modifiers
Without an IM: `XkbKeycodeToKeysym(dpy, keycode, group, level)` (preferred) or `XLookupKeysym`. Event
`state` holds modifier bits (`ShiftMask`, `ControlMask`, `Mod1Mask`=Alt, `Mod4Mask`=Super, `LockMask`,
`Mod2Mask`=NumLock). For text, prefer letting the XIC/`Xutf8LookupString` produce the committed string.

### 3.3 XIM / XIC input method (most likely broken)
**Locale + modifiers BEFORE opening the IM:**
```c
setlocale(LC_ALL, "");        /* or at least LC_CTYPE — honor user locale */
if (!XSupportsLocale()) { /* fall back */ }
XSetLocaleModifiers("");      /* "" honors XMODIFIERS env, e.g. @im=fcitx / @im=ibus */
```
If you never set the locale/modifiers you get the C locale and no real IME → **Japanese (fcitx5/ibus)
silently won't work.** `XSetLocaleModifiers("")` honoring `XMODIFIERS` is what bridges to fcitx5/ibus over
XIM.

**Open IM + create IC:**
```c
XIM xim = XOpenIM(dpy, NULL, NULL, NULL);
XIC xic = XCreateIC(xim,
    XNInputStyle,   XIMPreeditNothing | XIMStatusNothing,  /* root-window style: safe default */
    XNClientWindow, win,
    XNFocusWindow,  win,
    NULL);                                                 /* returns NULL on failure */
```
- Input styles: `XIMPreeditCallbacks` (on-the-spot, inline at caret — needs draw/caret/start/done
  callbacks), `XIMPreeditPosition` (over-the-spot, set `XNSpotLocation`), `XIMPreeditArea` (off-the-spot),
  `XIMPreeditNothing` (root-window). Status: `XIMStatusCallbacks`/`XIMStatusArea`/`XIMStatusNothing`.
  **Safe default: `XIMPreeditNothing | XIMStatusNothing`** (works with fcitx5/ibus; least code).
- Negotiate: `XGetIMValues(xim, XNQueryInputStyle, &styles, NULL)` returns supported `XIMStyles*`; pick one
  that intersects what you implement. Set `XNClientWindow` (toplevel) + `XNFocusWindow` (focused window).

**Per-event, two non-negotiable calls:**
1. **`XFilterEvent(&ev, None)` on EVERY event, before dispatch — `continue` if it returns `True`.** "some
   input method has filtered the event, and the client should discard the event." **Missing this is the
   #1 cause of broken text input** (dead keys / Compose / CJK preedit silently fail).
2. For unfiltered `KeyPress`, `Xutf8LookupString(xic, &ev.xkey, …)` to get committed UTF-8.

**Focus:** `XSetICFocus(xic)` on `FocusIn`, `XUnsetICFocus(xic)` on `FocusOut`.
**Reset:** `Xutf8ResetIC(xic)` clears pending input/preedit (returns current preedit string, free with
`XFree`). Call on programmatic clear / focus loss / abandon composition.

**To type Japanese:** UTF-8 `LC_CTYPE` + `XSetLocaleModifiers("")` + `XOpenIM`/`XCreateIC` (root-window
style OK) + `XFilterEvent` on every event + `XSetICFocus` on focus + `Xutf8LookupString` for committed
text + add the IC's `XNFilterEvents` mask to `XSelectInput`.

---

## 4. Software / CPU present on X11 (MEDIUM — CPU-render fallback / blit)

### 4.1 Plain path: `XCreateImage` + `XPutImage`
- `XCreateImage(dpy, visual, depth, ZPixmap, 0, data, w, h, bitmap_pad, bytes_per_line)` wraps a
  client-side buffer (use `ZPixmap` for 32-bit). `XPutImage(dpy, drawable, gc, image, sx,sy, dx,dy, w,h)`.
- **Pixel format gotcha:** match the visual's `red_mask`/`green_mask`/`blue_mask` and the XImage's
  `byte_order`. On little-endian x86 with the common BGRX visual, a `0x00RRGGBB` `uint32_t` per pixel
  (bytes B,G,R,X) is expected — **RGBA renderers must swizzle to BGRA/BGRX** or red/blue swap. For ZPixmap
  the depth **must equal the drawable's depth** or you get `BadMatch`.

### 4.2 Fast path: MIT-SHM (setup order matters — XImage first)
```c
XShmSegmentInfo shminfo;
XImage *img = XShmCreateImage(dpy, visual, depth, ZPixmap, NULL, &shminfo, w, h); /* data NULL */
shminfo.shmid   = shmget(IPC_PRIVATE, img->bytes_per_line * img->height, IPC_CREAT | 0600);
shminfo.shmaddr = img->data = shmat(shminfo.shmid, NULL, 0);   /* store in BOTH */
shminfo.readOnly = False;
XShmAttach(dpy, &shminfo);
XSync(dpy, False);
shmctl(shminfo.shmid, IPC_RMID, 0);          /* mark for deletion; freed after detach */
/* ... render into img->data ... */
XShmPutImage(dpy, win, gc, img, sx,sy, dx,dy, w,h, /*send_event=*/True);
```
- **send_event/completion:** with `send_event=True` the server posts an `XShmCompletionEvent` when the
  write is done. **If you reuse one segment you MUST wait for `ShmCompletion` before overwriting** (else
  you race the server → tearing/garbage). Simpler: double-buffer two SHM segments. Register the event type
  via `XShmGetEventBase`.
- Cleanup: `XShmDetach` → `XDestroyImage` → `shmdt` → `shmctl(IPC_RMID)`.

### 4.3 XShm vs plain `XPutImage`
- MIT-SHM for repeated full-window blits (per-frame present) — avoids the socket copy. Works only for a
  **local** server: probe `XShmQueryExtension(dpy)` + gate on a local connection.
- Plain `XPutImage` for one-off/small/infrequent blits, or when SHM is unavailable (fallback).

---

## 5. GLX vs EGL on X11 (LOW — context note)
- **GLX**: historical X11↔OpenGL binding (`glXChooseFBConfig`, `glXCreateContextAttribsARB`,
  `glXMakeCurrent`, `glXSwapBuffers`). Desktop-GL only, Xlib-coupled.
- **EGL**: cross-platform (GLES, Wayland, X11, headless), modern context creation, access to
  `EGL_KHR_partial_update` / buffer-age / swap-with-damage. azul uses EGL on X11.
- **Binding EGL to an Xlib Window — pitfalls:**
  - Prefer `eglGetPlatformDisplay(EGL_PLATFORM_X11_KHR=0x31D5, x11_display_ptr, attribs)` over legacy
    `eglGetDisplay`. Optional `EGL_PLATFORM_X11_SCREEN_KHR=0x31D6`.
  - **Biggest footgun:** `eglCreatePlatformWindowSurface` takes a **pointer to** the X11 `Window`
    (`&window`), whereas legacy `eglCreateWindowSurface` takes the `Window` **by value** (cast through
    `EGLNativeWindowType`). Mixing them up → crash/garbage.
  - **Visual match:** create the X11 `Window` with a visual matching the chosen `EGLConfig`'s
    `EGL_NATIVE_VISUAL_ID`, or you get `EGL_BAD_MATCH` / wrong colors.

---

## Most important takeaways for the 3 X11 bugs

**FBO-0 garbage:** after `eglSwapBuffers` the default-FB color buffer is **undefined** unless
`EGL_SWAP_BEHAVIOR == EGL_BUFFER_PRESERVED` (needs a config with `EGL_SWAP_BEHAVIOR_PRESERVED_BIT` — don't
assume). Ancillary buffers always undefined. **Fix:** every frame `eglMakeCurrent` → `glBindFramebuffer(0)`
→ `glViewport` → `glClearColor` → `glClear` before compositing. Damage/partial-update do NOT preserve
untouched pixels (swap-with-damage still swaps the whole buffer; partial-update makes the whole FB
undefined if you draw outside the region or mishandle `age==0`).

**Text input + repaint:** (a) IME — `setlocale(LC_CTYPE,"")` + `XSetLocaleModifiers("")` BEFORE `XOpenIM`;
create an `XIC` (`XIMPreeditNothing|XIMStatusNothing`); call `XFilterEvent(&ev,None)` on every event and
`continue` on `True`; `XSetICFocus`/`XUnsetICFocus` on focus; read committed text via `Xutf8LookupString`
on `KeyPress` only, inserting only on `XLookupChars`/`XLookupBoth` (control keysyms → command, not text).
(b) Repaint after typing — mark dirty + post a repaint; full redraw on `Expose.count==0`; coalesce
`ConfigureNotify`.

**CPU hit-test/blit:** `ZPixmap`, depth must equal the drawable's (else `BadMatch`); honor
`red/green/blue_mask` + `byte_order` (BGRX on x86 — swizzle from RGBA). Prefer MIT-SHM (gate on
`XShmQueryExtension` + local server) with `XPutImage` fallback; wait on `ShmCompletion` before reusing a
segment, or double-buffer.

---

## Sources
- eglSwapBuffers (color buffer undefined unless PRESERVED; ancillary always undefined): https://registry.khronos.org/EGL/sdk/docs/man/html/eglSwapBuffers.xhtml (mirror: https://katastrophos.net/harmattan-dev/html/egl/eglSwapBuffers.html)
- eglSurfaceAttrib (EGL_SWAP_BEHAVIOR, EGL_BAD_MATCH / EGL_SWAP_BEHAVIOR_PRESERVED_BIT): https://registry.khronos.org/EGL/sdk/docs/man/html/eglSurfaceAttrib.xhtml
- EGL_KHR_swap_buffers_with_damage (rect {x,y,w,h}, bottom-left, "entire back buffer must be consistent"): https://registry.khronos.org/EGL/extensions/KHR/EGL_KHR_swap_buffers_with_damage.txt
- EGL_KHR_partial_update (eglSetDamageRegionKHR timing, EGL_BAD_ACCESS, outside-region undefined): https://registry.khronos.org/EGL/extensions/KHR/EGL_KHR_partial_update.txt
- EGL_EXT_buffer_age (EGL_BUFFER_AGE_EXT=0x313D, age 0 ⇒ undefined): https://registry.khronos.org/EGL/extensions/EXT/EGL_EXT_buffer_age.txt
- EGL_KHR_platform_x11 (EGL_PLATFORM_X11_KHR=0x31D5, native_window = pointer to Window): https://registry.khronos.org/EGL/extensions/KHR/EGL_KHR_platform_x11.txt
- eglGetPlatformDisplay: https://registry.khronos.org/EGL/sdk/docs/man/html/eglGetPlatformDisplay.xhtml
- OpenGL Wiki — Default Framebuffer / Framebuffer (glClear bits, undefined/unowned pixels): https://www.khronos.org/opengl/wiki/Default_Framebuffer , https://www.khronos.org/opengl/wiki/Framebuffer
- Xlib Expose (count, repaint on count==0): https://tronche.com/gui/x/xlib/events/exposure/expose.html
- Xlib ConfigureNotify: https://tronche.com/gui/x/xlib/events/window-state-change/configure.html
- XEventsQueued / XPending: https://tronche.com/gui/x/xlib/event-handling/XEventsQueued.html
- XCheckIfEvent (non-blocking): https://tronche.com/gui/x/xlib/event-handling/manipulating/XCheckIfEvent.html
- XFlush / XSync: https://www.x.org/releases/current/doc/man/man3/XFlush.3.xhtml
- XSelectInput / masks: https://tronche.com/gui/x/xlib/event-handling/XSelectInput.html , https://tronche.com/gui/x/xlib/events/mask.html
- WM_DELETE_WINDOW / XSetWMProtocols / ClientMessage: https://www.lemoda.net/c/xlib-wmclose/ , https://sidvind.com/wiki/Xlib_and_GLX:_Part_2
- XmbLookupString / Xutf8LookupString (encodings, Status table, KeyPress-only): https://www.x.org/releases/current/doc/man/man3/XmbLookupString.3.xhtml , https://manpages.debian.org/testing/libx11-doc/XmbLookupString.3.en.html
- XFilterEvent (call on every event; discard if True): https://www.x.org/releases/current/doc/man/man3/XFilterEvent.3.xhtml
- XOpenIM / locale + XSetLocaleModifiers: https://x.org/releases/X11R7.7-RC1/doc/man/man3/XOpenIM.3.xhtml
- Xlib Input Method Overview (styles, focus, negotiation): https://docs.oracle.com/cd/E19620-01/805-3916/xtxlib-7/index.html
- XSetICFocus / XUnsetICFocus: https://manpages.debian.org/bullseye/libx11-doc/XSetICFocus.3.en.html
- XmbResetIC / Xutf8ResetIC: https://manpages.debian.org/bullseye/libx11-doc/XmbResetIC.3.en.html
- X Input Method Protocol (XIM): https://www.x.org/releases/X11R7.6/doc/libX11/specs/XIM/xim.html
- MIT-SHM (XShmCreateImage/Attach/PutImage, lifecycle, completion): https://www.x.org/releases/X11R7.7/doc/xextproto/shm.html , https://www.x.org/releases/X11R7.6/doc/man/man3/XShmPutImage.3.xhtml

_Note: registry.khronos.org HTML refpages + the Khronos OpenGL wiki 403 the fetcher; normative text was
sourced from official mirrors (katastrophos.net, KhronosGroup/EGL-Registry, nigels-com/glfixes) and
cross-checked._
