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

## Wayland MARSHALLING FIX SPEC (researched 2026-05-21) — the backend is non-functional

Two research agents (libwayland API + full binding audit) found the wayland shell's request marshalling is broken: it's dead at registry-bind. Fix spec below; apply in ONE pass (partial leaves it dead).

### Marshalling API (real exported symbols, dlsym-able)
- `wl_proxy_marshal_flags(proxy, opcode:u32, interface:*const wl_interface, version:u32, flags:u32, ...) -> *mut wl_proxy` (libwayland >=1.20; PREFER). flags=0 normally; `WL_MARSHAL_FLAG_DESTROY=1` only on destroy-requests.
- `wl_proxy_marshal_constructor(proxy, opcode, interface, ...)` + `_constructor_versioned(proxy, opcode, interface, version, ...)` (older; fallback if marshal_flags is NULL).
- `wl_proxy_marshal(proxy, opcode, ...)` (non-constructor requests). `wl_proxy_get_version(proxy)->u32`. `wl_proxy_add_listener` (already loaded, OK). `wl_proxy_destroy` (OK).
- INLINE-ONLY (NOT exported; must reimplement via the above): wl_display_get_registry, wl_registry_bind, wl_seat_get_pointer/keyboard/touch, every wl_<iface>_<request>, all xdg_*.
- Rust can't varargs-via-fn-ptr: transmute the raw marshaller to a CONCRETE per-request signature `(proxy,u32,*const wl_interface,u32,u32, ...args, *mut c_void newid)`. Constructor requests ALWAYS pass a trailing NULL new_id.
- **APPROACH:** the broken `transmute(...)` wrapper FIELDS in dlopen.rs must become real fns/methods that inject the hardcoded opcode + interface (the fields drop them). Add dlsym of `wl_proxy_marshal_flags`, `wl_proxy_marshal_constructor_versioned`, `wl_proxy_get_version`. Add the missing `wl_interface` globals: wl_surface/pointer/keyboard/touch/callback/region/shm_pool/buffer (+ wl_subsurface), and xdg_surface/toplevel/popup/positioner (xdg ones exported only if the xdg-shell .o is linked — confirm, the shell already loads xdg_wm_base_interface).

### Constructor requests (opcode, returned-interface) — were dropping both:
wl_registry_bind: op 0, SPECIAL: `marshal_constructor_versioned(reg, 0, iface, version, name:u32, iface->name:string, version:u32, NULL)`. | wl_compositor.create_surface op0 ->wl_surface | .create_region op1 ->wl_region | wl_subcompositor.get_subsurface op1 ->wl_subsurface(NULL,surface,parent) | wl_surface.frame op3 ->wl_callback | xdg_wm_base.get_xdg_surface op2 ->xdg_surface(NULL,surface) | .create_positioner op1 ->xdg_positioner | xdg_surface.get_toplevel op1 ->xdg_toplevel | .get_popup op3 ->xdg_popup(NULL,parent,positioner) | wl_seat.get_pointer op0 ->wl_pointer | .get_keyboard op1 ->wl_keyboard | .get_touch op2 ->wl_touch | wl_shm.create_pool op0 ->wl_shm_pool(NULL,fd,size) | wl_shm_pool.create_buffer op0 ->wl_buffer(NULL,offset,w,h,stride,format).

### Plain requests (wl_proxy_marshal, opcode) — were dropping the opcode:
wl_surface: destroy0 attach1(buf,x,y) damage2(x,y,w,h) frame3 set_opaque_region4 commit6 | wl_subsurface: destroy0 set_position1(x,y) set_desync5 | xdg_wm_base.pong op3(serial) | xdg_surface.ack_configure op4(serial) | xdg_toplevel: set_title2(str) move5(seat,serial) set_max_size7(w,h) set_min_size8(w,h) set_maximized10 unset_maximized11 set_minimized12 set_fullscreen13(output) unset_fullscreen14 | xdg_positioner: destroy0 set_size1(w,h) set_anchor_rect2(x,y,w,h) set_anchor4 set_gravity5 set_constraint_adjustment6 | xdg_popup: destroy0 grab1(seat,serial) | wl_buffer.destroy0 | wl_shm_pool.destroy1 | wl_region: destroy0(NB audit said 1—verify) add1(x,y,w,h) | wl_pointer.set_cursor op0(serial,surface,hx,hy).

### f64 wl_fixed ABI bug (defines.rs wl_pointer_listener + events.rs handlers): CONFIRMED
`wl_fixed_t = int32_t` (24.8); listeners receive i32, not f64. Fix wl_pointer_listener.enter/motion `surface_x/y` + axis `value` from f64 -> i32 (wl_fixed_t); in pointer_enter/motion/axis handlers convert `v as f64 / 256.0`. Same rule for wl_touch (down/motion x/y = i32) + tablet tool (motion/tilt/rotation = i32 wl_fixed; pressure/distance = u32 0..65535; slider = i32).

### Then: wl_touch (op via get_touch + wl_touch_interface + listener) + zwp_tablet_v2 (manager get_tablet_seat op0 ->seat(NULL,wl_seat); hand-roll zwp_tablet_*_interface descriptors — NOT in libwayland). Feed -> touch_state / update_pen_state_full, per the protocol section above.
