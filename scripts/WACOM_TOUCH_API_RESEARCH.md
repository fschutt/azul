# wacom/touch — X11 XInput2 + Wayland API reference (researched 2026-05-21)

Authoritative ABI/protocol reference for the Linux desktop touch+pen feeds, researched
online from upstream headers/specs (the X11 shell hand-rolls X bindings; Wayland uses
the shell's existing protocol plumbing). Mirrors the iOS/Android/macOS/Windows feed:
pen -> `gesture_drag_manager.update_pen_state_full`, touch -> `current_window_state.touch_state`.

## X11 XInput2 (XI2) — for hand-rolled `#[repr(C)]` FFI in x11/defines.rs

**Base type widths (client side, never `_XSERVER64`):** `Window`/`Atom`/`Time`/`Cursor`/`XID`/`Mask`/`serial` = `c_ulong` (pointer-width). `Bool`/`Status`/all `int` fields = `c_int` (32-bit). `double` = `c_double`. Use `#[repr(C)]`, let the compiler pad (e.g. `int deviceid` 4B then `Window root` 8B on x86-64).

**Structs (exact field order + type):**
- `XIEventMask { deviceid: c_int, mask_len: c_int, mask: *mut c_uchar }`
- `XIValuatorState { mask_len: c_int, mask: *mut c_uchar, values: *mut c_double }`
- `XIButtonState { mask_len: c_int, mask: *mut c_uchar }`
- `XIModifierState { base: c_int, latched: c_int, locked: c_int, effective: c_int }` (`XIGroupState` = identical)
- `XIAnyClassInfo { type_: c_int, sourceid: c_int }`
- `XIValuatorClassInfo { type_: c_int, sourceid: c_int, number: c_int, label: Atom(c_ulong), min: c_double, max: c_double, value: c_double, resolution: c_int, mode: c_int }`
- `XIDeviceInfo { deviceid: c_int, name: *mut c_char, use_: c_int, attachment: c_int, enabled: c_int(Bool), num_classes: c_int, classes: *mut *mut XIAnyClassInfo }`
- `XIDeviceEvent { type_: c_int, serial: c_ulong, send_event: c_int, display: *mut Display, extension: c_int, evtype: c_int, time: c_ulong, deviceid: c_int, sourceid: c_int, detail: c_int, root: c_ulong, event: c_ulong, child: c_ulong, root_x: c_double, root_y: c_double, event_x: c_double, event_y: c_double, flags: c_int, buttons: XIButtonState, valuators: XIValuatorState, mods: XIModifierState, group: XIGroupState }` — `detail` = button# (button events) or touch tracking-id (touch events). Pressure/tilt are valuator entries inside `valuators`, NOT named fields.
- `XGenericEventCookie { type_: c_int, serial: c_ulong, send_event: c_int, display: *mut Display, extension: c_int, evtype: c_int, cookie: c_uint, data: *mut c_void }` — add as a variant of the shell's `XEvent` union; union must be >= `[c_long; 24]` (pad[24]).

**Constants:** `GenericEvent = 35`. XI evtypes: `XI_ButtonPress=4, XI_ButtonRelease=5, XI_Motion=6, XI_TouchBegin=18, XI_TouchUpdate=19, XI_TouchEnd=20` (NB: 15/16/17 are the *Raw* variants — do not use). `XIAllDevices=0, XIAllMasterDevices=1`. Class types: `XIValuatorClass=2, XITouchClass=8`. `XIModeAbsolute=1, XIModeRelative=0`. Mask macros: `XISetMask: mask[ev>>3] |= 1<<(ev&7)`; `XIMaskIsSet: mask[ev>>3] & (1<<(ev&7))`; `mask_len = (XI_TouchEnd>>3)+1 = 3` bytes.

**Functions** (XI* from libXi; XGetEventData/XFreeEventData/XQueryExtension/XInternAtom from libX11):
- `XIQueryVersion(dpy, *mut c_int major, *mut c_int minor) -> c_int(Status)` (pass 2,2 for touch)
- `XISelectEvents(dpy, win: c_ulong, *mut XIEventMask, num: c_int) -> c_int`
- `XIQueryDevice(dpy, deviceid: c_int, *mut c_int ndev) -> *mut XIDeviceInfo`; `XIFreeDeviceInfo(*mut XIDeviceInfo)`
- `XGetEventData(dpy, *mut XGenericEventCookie) -> c_int(Bool)`; `XFreeEventData(dpy, *mut XGenericEventCookie)`
- `XQueryExtension(dpy, name: *const c_char, *mut c_int opcode, *mut c_int ev, *mut c_int err) -> c_int(Bool)`
- `XInternAtom(dpy, name: *const c_char, only_if_exists: c_int) -> Atom(c_ulong)`

**Valuator decode (values is PACKED, not indexed by valuator#):** for valuator N, present iff `XIMaskIsSet(mask,N)`; its slot = count of set bits in mask below N. Values are already `c_double`. Idiom: walk a pointer into `values`, advancing only when the mask bit is set.

**Label atoms (intern + compare to `XIValuatorClassInfo.label`; `.number` = valuator index):** `"Abs Pressure"`, `"Abs Tilt X"`, `"Abs Tilt Y"`, `"Abs Distance"` (+ MT: `"Abs MT Position X/Y"`, `"Abs MT Pressure"`, `"Abs MT Tracking ID"`). Atoms are session-dynamic — intern at runtime, never hardcode.

**Setup/flow:** `XQueryExtension(dpy,"XInputExtension",&opcode,..)` (cache opcode) -> `XIQueryVersion(2,2)` -> build `XIEventMask{deviceid:XIAllMasterDevices, mask_len:3, mask}` with bits ButtonPress|Release|Motion|TouchBegin|Update|End -> `XISelectEvents(root_or_win,&mask,1)` -> `XIQueryDevice(XIAllDevices)` once, walk `classes[]`, for `XIValuatorClass` entries map `label`->`number`. Loop: `if xev.type==GenericEvent && xev.xcookie.extension==opcode { XGetEventData -> cast xcookie.data to *XIDeviceEvent (pick by evtype) -> decode valuators -> feed -> XFreeEventData }`.

## Wayland — wl_touch + zwp_tablet_v2 (stable tablet-v2.xml, interfaces still `zwp_*_v2`)

wl_fixed = signed 24.8 -> f64 = raw/256.0. `frame` is the atomic commit boundary for both protocols.

**wl_seat** (registry `"wl_seat"`): capability bitfield `pointer=1, keyboard=2, touch=4`. Event `capabilities(uint)` (test `&4`). Request `get_touch(new_id wl_touch)` (errors if no touch cap).

**wl_touch** events: `down(serial:u, time:u, surface:obj, id:i, x:fixed, y:fixed)`, `up(serial:u, time:u, id:i)`, `motion(time:u, id:i, x:fixed, y:fixed)`, `frame()`, `cancel()` (drop all points, no up follows), `shape(id:i, major:fixed, minor:fixed)`[v6], `orientation(id:i, orientation:fixed)`[v6]. x/y surface-local.

**Tablet** (registry `"zwp_tablet_manager_v2"`, bind v<=2):
- `zwp_tablet_manager_v2`: req `get_tablet_seat(new_id zwp_tablet_seat_v2, wl_seat)`, `destroy()`.
- `zwp_tablet_seat_v2`: events `tablet_added(new_id zwp_tablet_v2)`, `tool_added(new_id zwp_tablet_tool_v2)`, `pad_added(...)` (server-created new_ids — just add listeners).
- `zwp_tablet_v2`: events `name(string)`, `id(vid:u, pid:u)`, `path(string)`, `bustype(u)`[v2], `done()`, `removed()`.
- `zwp_tablet_tool_v2`:
  - enum `type`: pen=0x140, eraser=0x141, brush=0x142, pencil=0x143, airbrush=0x144, finger=0x145, mouse=0x146, lens=0x147.
  - enum `capability`: tilt=1, pressure=2, distance=3, rotation=4, slider=5, wheel=6. enum `button_state`: released=0, pressed=1.
  - descriptive burst (after tool_added, ends at `done`): `type(uint)`, `hardware_serial(hi:u, lo:u)`, `hardware_id_wacom(hi:u, lo:u)`, `capability(uint)` (one each), `done()`, `removed()`.
  - interaction (ends at `frame`): `proximity_in(serial:u, tablet:obj, surface:obj)`, `proximity_out()`, `down(serial:u)`, `up()`, `motion(x:fixed, y:fixed)` surface-local, `pressure(uint 0..65535)`, `distance(uint 0..65535)`, `tilt(tilt_x:fixed, tilt_y:fixed)` degrees, `rotation(degrees:fixed)`, `slider(int -65535..65535)`, `wheel(degrees:fixed, clicks:int)`, `button(serial:u, button:u BTN_*, state:u)`, `frame(time:u)`.
  - Map: pressure/65535 -> 0..1; tilt_x/y degrees; rotation degrees; `type==eraser` -> eraser; down/up = tip contact. Accumulate per `frame`.

**Flow:** proximity_in -> [down] -> motion/pressure/tilt/... -> frame(commit) -> ... -> [up] -> proximity_out.
