# Physical testing — Linux / X11 — 2026-09-04

**Device.** Linux Mint 22.2 "Zara", XFCE 4 on **Xorg 21.1.11**, `DISPLAY=:0.0`,
`XDG_SESSION_TYPE=x11`, `XDG_CURRENT_DESKTOP=XFCE`. Single 1920x1080 @ 60 Hz.
NOT Cinnamon — the session is XFCE, which matters because it decides which
settings store is authoritative.

Desktop's own settings, for reference (`xfconf-query`):

    xsettings /Net/ThemeName     Mint-Y-Aqua      (light)
    xsettings /Net/IconThemeName Mint-Y-Sand
    xsettings /Gtk/FontName      Ubuntu 10
    xfwm4     /general/theme     Mint-Y-Aqua      (light titlebars)
    xfwm4     /general/button_layout  O|HMC

The machine also still holds a **KDE session's leftovers**: gsettings
`gtk-theme=Breeze`, `icon-theme=breeze-dark`, `color-scheme=prefer-dark`, and
`~/.config/gtk-3.0/settings.ini` with `gtk-application-prefer-dark-theme=true`.
That contradiction is what made the bugs below visible, and it is not exotic —
any machine that has ever run another desktop is in this state.

Binary under test: `feat/rust-fontconfig-5` + the commits in this PR,
`cargo build -p azul-dll --release --features build-dll`, app = AzWriter
(`link-dynamic`).

## Results

| id | promise (one line) | verdict | evidence |
|----|--------------------|---------|----------|
| system style | the app matches the desktop's fonts/colours/theme | **BROKEN -> FIXED** | dump said `platform Linux(Other("XFCE"))` and `settings_source gsettings:gnome` on the next line; theme Dark on a light desktop, `Noto Sans 10` for `Ubuntu 10`. After: `settings_source xfconf`, theme Light, ui `Ubuntu 10`, gtk theme `Mint-Y-Aqua` |
| icon theme | the app draws the desktop's icons | **BROKEN -> FIXED** | `icon_theme breeze-dark` (a theme this desktop does not use). After the source fix it read `Mint-Y-Sand` but registered **0** icons; three lookup defects later, **18/17** |
| titlebar drag | dragging a client-drawn titlebar moves the window | **BROKEN -> FIXED** | dead code in the wrong `impl` block + a gesture session never closed. Sweep at fixed x: y=4 DRAG, 10 dead, 14 DRAG, 18 dead, 21 DRAG, 24 dead, 27 DRAG. After: all 7 DRAG |
| double-click maximize | double-clicking the titlebar toggles maximize/restore | **BROKEN -> FIXED** | `1920x1047@0,0` + `_NET_WM_STATE_MAXIMIZED_{VERT,HORZ}` -> `1280x800@320,140`, atoms cleared; and back. Debug builds PANICKED here (`update_unsynced_state` assert) |
| CSD resize band | the 8 px edge band resizes | **PARTIAL -> FIXED** | it also ran while maximized, where there is no resizable edge, and swallowed the press before the gesture manager saw it |
| gallery contrast | ribbon style previews are readable | **BROKEN -> FIXED** | dark blue `#2e74b5` / `#262626` on charcoal; Title sample invisible. Now lifted to WCAG AA against the chrome, hue kept, light theme byte-identical |
| `headless_window_features` | the target builds | **BROKEN -> FIXED** | had not compiled since `TouchPoint` gained `seat_id`; 7/7 now |
| resize smoothness | interactive resize repaints at refresh rate | **BROKEN — not root-caused** | **44.2 ms median between frames (min 40.3, max 70.3) = ~23 fps on a 60 Hz display**, measured from the app's own ConfigureNotify timestamps over 28 consecutive resize steps. The FAST PATH IS TAKEN (no "boundary crossed" lines, so no full DOM regeneration), and `detect_frame_interval` reads the CRTC's real 60 Hz — so the cost is inside render/present, not relayout or pacing. Next step: instrument `render_and_present` |
| library size | — | **NOT A DEFECT** | The 402 MB `libazul.so` is a LOCAL `release` build: that profile deliberately keeps `debug = 1, strip = false` "so samply can resolve them" (`strip --strip-debug` -> **70 MB**). **CI does not ship this.** Dist artifacts build with `--profile prod-release` (`rust.yml:926-930`), which sets `strip = "symbols"`, `debug = 0`, thin LTO and `codegen-units = 1`; the `.a` gets `scripts/strip_staticlib.sh` on top, because cargo's `strip` only touches final linked outputs, never archives |

## Not tested (no hardware / not reached)

Multi-seat (needs a second master pointer pair), pen/tablet, gamepad rumble,
touch, IME/XIM composition, RandR hotplug, pointer lock. Every ledger item
under "Device verification owed" remains owed.

## Traps found (each cost >10 min)

1. **The app links the STALE system library.** `ldd target/release/AzWriter`
   resolves `libazul.so` to `/lib/libazul.so` — a 41 MB copy from June 1. A
   plain run tests three-month-old code. Run with `LD_LIBRARY_PATH` pointing at
   the freshly built one.
2. **Two feature sets fight over one output path.** `cargo build -p azul-dll
   --features build-dll` and `cargo build -p AzWriter` (which wants
   `link-dynamic`) both write `target/release/libazul.so`; building the app
   after the library replaces the 402 MB library with a 5.6 MB one, and the app
   then fails to start with "file too short". Build the library LAST, or keep a
   copy outside `target/release`.
3. **Setting `RUSTFLAGS` to help the linker forces a full azul-dll rebuild**
   (it changes every fingerprint). Combined with `earlyoom`, configured on this
   box to *prefer* killing `rustc|cargo|lld|ld`, that produced a **0-byte
   `libazul.so` from a build that reported success**. Use `LIBRARY_PATH`.
4. **`cargo` reports success for a truncated link.** Always check
   `ls -la target/release/libazul.so` after a build here.
5. **A lint that lies costs more than no lint.** The bare-text lint claimed a
   text node's callbacks were "INERT"; it pointed at the test's DOM while the
   real fault was a missing filter family. (Fixed earlier in this branch.)
6. **`| head` on a running GUI kills it.** Closing the stderr pipe makes the
   diagnostics sink panic — "failed printing to stderr: Broken pipe" — and the
   app aborts. A library must not die because stderr went away; still open.

## Worth a follow-up (not a defect)

CI **reports** artifact sizes (`after local-symbol strip:`, `$before -> $after
bytes`, and into the release HTML) but nothing **fails** on a size regression.
Given how much of this repo's history is size work, a threshold gate on the
shipped `.so`/`.a` would turn that reporting into a ratchet.

## Still open

- Resize repaint at ~23 fps (above) — measured, not root-caused.
- Broken-pipe stderr panic in `azul_core::diagnostics::default_sink`.
- The X11 audit's remaining P1/P2/P3: no `PropertyChangeMask`/`PropertyNotify`
  handler, so a WM-initiated maximize (Alt+F10, edge-snap) is never observed;
  no `_NET_FRAME_EXTENTS`/`_GTK_FRAME_EXTENTS`; `XI_KeyPress` is selected on
  `XIAllMasterDevices` while primary-seat keys are dropped; pointer lock uses
  `XGrabPointer` under XI2.
