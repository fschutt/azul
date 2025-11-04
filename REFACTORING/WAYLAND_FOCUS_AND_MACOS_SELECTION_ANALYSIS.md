# Wayland Focus Events und macOS Selection Range - Analyse

## Problem 1: Wayland Keyboard Focus Events

### Situation
- **X11**: Hat explizite `FocusIn` / `FocusOut` Events (Event-Codes 9/10)
- **Wayland**: Hat keine expliziten Window-Focus-Events wie X11
- **Wayland Focus Model**: Focus wird über `wl_keyboard::enter` / `wl_keyboard::leave` Callbacks signalisiert

### Wayland Keyboard Listener Struktur (bereits definiert)
```rust
pub struct wl_keyboard_listener {
    pub keymap: extern "C" fn(...),
    pub enter: extern "C" fn(         // <-- Focus gained
        data: *mut c_void,
        keyboard: *mut wl_keyboard,
        serial: u32,
        surface: *mut wl_surface,
        keys: *mut c_void,
    ),
    pub leave: extern "C" fn(         // <-- Focus lost
        data: *mut c_void,
        keyboard: *mut wl_keyboard,
        serial: u32,
        surface: *mut wl_surface,
    ),
    pub key: extern "C" fn(...),
    pub modifiers: extern "C" fn(...),
    pub repeat_info: extern "C" fn(...),
}
```

### Aktuelle Implementierung
- ✅ `wl_keyboard_listener` Struct bereits in `defines.rs` definiert
- ❌ Listener wird NICHT registriert - Keyboard-Events kommen anders
- ✅ `handle_key()` Methode funktioniert bereits (via XKB)
- ❌ `window_focused` Flag wird nie gesetzt (immer `false` bei Initialisierung)

### Vermutung: GTK macht das automatisch
Wayland-Backend nutzt GTK für IME (`gtk_im_context`). GTK könnte:
- Automatisch wl_keyboard Listener registrieren
- Focus-State intern verwalten
- IME-Position via GTK APIs setzen (bereits implementiert in `sync_ime_position_to_os()`)

### Wo der Focus-State fehlt
```rust
// dll/src/desktop/shell2/linux/wayland/mod.rs:862
window_focused: false,  // <-- Nie auf true gesetzt!
```

## Problem 2: macOS Selected Range

### NSTextInputClient Protokoll
macOS IME kann zwei Dinge abfragen:
1. **firstRectForCharacterRange** - Position des IME-Fensters (✅ implementiert)
2. **selectedRange** - Markierter Text im Eingabefeld (❓ relevant?)

### Aktuelle Implementierung
```rust
#[unsafe(method(selectedRange))]
fn selected_range(&self) -> NSRange {
    // Return NSNotFound to indicate no selection
    NSRange {
        location: usize::MAX,  // NSNotFound
        length: 0,
    }
}
```

### Bedeutung
- **NSNotFound** = "Keine Selektion vorhanden"
- **Normal für einfache Texteingabe**: Cursor-Position (keine Selection)
- **Nur relevant bei Text-Selection**: Markierter Text (z.B. durch Maus-Drag oder Shift+Arrow)

### Ist das ein Problem?
**NEIN, aktuell korrekt implementiert:**
- Browser zeigen auch nur Cursor (keine Selection) während IME-Komposition
- Safari/Chrome/Firefox geben ebenfalls NSNotFound zurück während IME
- Selection würde nur relevant bei:
  - IME möchte markierten Text ersetzen
  - User hat Text markiert VOR Start der IME-Komposition

### Wann wäre Selection-Support nötig?
Nur wenn:
1. User markiert Text mit Maus/Shift+Arrows
2. User startet IME-Eingabe (z.B. drückt japanische Tastatur-Taste)
3. IME soll markierten Text ersetzen

**Entscheidung:** Vorerst NICHT implementieren. Browsers machen das auch nicht.

## Lösungen

### Lösung 1: Wayland Focus Events (BENÖTIGT)

#### Option A: wl_keyboard Listener registrieren (Native)
```rust
// In WaylandWindow::new() nach wl_seat Initialisierung
extern "C" fn keyboard_enter_callback(
    data: *mut c_void,
    _keyboard: *mut wl_keyboard,
    _serial: u32,
    surface: *mut wl_surface,
    _keys: *mut c_void,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    if window.surface == surface {
        window.current_window_state.window_focused = true;
        window.sync_ime_position_to_os();  // Phase 2: OnFocus
    }
}

extern "C" fn keyboard_leave_callback(
    data: *mut c_void,
    _keyboard: *mut wl_keyboard,
    _serial: u32,
    surface: *mut wl_surface,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    if window.surface == surface {
        window.current_window_state.window_focused = false;
    }
}

let listener = wl_keyboard_listener {
    keymap: keyboard_keymap_callback,
    enter: keyboard_enter_callback,
    leave: keyboard_leave_callback,
    key: keyboard_key_callback,
    modifiers: keyboard_modifiers_callback,
    repeat_info: keyboard_repeat_info_callback,
};

// Get keyboard from seat
let keyboard = (wayland.wl_seat_get_keyboard)(seat);
(wayland.wl_keyboard_add_listener)(keyboard, &listener, window_ptr);
```

**Probleme:**
- Benötigt `wl_seat_get_keyboard` in dlopen
- Benötigt `wl_keyboard_add_listener` in dlopen
- Benötigt alle anderen Callbacks (keymap, key, modifiers, repeat_info)
- Komplex: Aktuelle Key-Events kommen via XKB, nicht via wl_keyboard::key

#### Option B: GTK Focus Listener (Einfacher)
```rust
// GTK hat eigene Focus-Events
extern "C" fn gtk_window_focus_in(
    widget: *mut GtkWidget,
    _event: *mut c_void,
    user_data: *mut c_void
) -> gboolean {
    let window = unsafe { &mut *(user_data as *mut WaylandWindow) };
    window.current_window_state.window_focused = true;
    window.sync_ime_position_to_os();
    0  // FALSE = continue propagation
}

extern "C" fn gtk_window_focus_out(
    widget: *mut GtkWidget,
    _event: *mut c_void,
    user_data: *mut c_void
) -> gboolean {
    let window = unsafe { &mut *(user_data as *mut WaylandWindow) };
    window.current_window_state.window_focused = false;
    0
}

// Connect signals (if GTK window exists)
if let (Some(gtk), Some(gtk_widget)) = (&self.gtk_im, &self.gtk_widget) {
    (gtk.g_signal_connect)(
        gtk_widget,
        c"focus-in-event".as_ptr(),
        gtk_window_focus_in as *mut c_void,
        window_ptr as *mut c_void
    );
    (gtk.g_signal_connect)(
        gtk_widget,
        c"focus-out-event".as_ptr(),
        gtk_window_focus_out as *mut c_void,
        window_ptr as *mut c_void
    );
}
```

**Aber:** Wayland-Backend hat kein GTK-Window, nur IM-Context!

#### Option C: Focus von xdg_toplevel ableiten (Eleganteste Lösung)
Wayland hat `xdg_toplevel` Listener mit Focus-Events:

```rust
// In defines.rs - BEREITS EXISTIERT?
pub struct xdg_toplevel_listener {
    pub configure: extern "C" fn(...),
    pub close: extern "C" fn(...),
    // Wayland-Protocols > 3.0 haben auch:
    // pub configure_bounds: extern "C" fn(...),
    // pub wm_capabilities: extern "C" fn(...),
}
```

**Problem:** xdg_toplevel hat KEINE Focus-Events in der Standard-Spec!

Focus in Wayland kommt NUR von:
- `wl_keyboard::enter` / `leave` (Keyboard-Focus)
- `wl_pointer::enter` / `leave` (Maus-Hover, nicht relevant)

#### Option D: Pragmatische Lösung (EMPFOHLEN)
Da Wayland bereits Key-Events empfängt (via `handle_key()`):
```rust
pub fn handle_key(&mut self, key: u32, state: u32) {
    // ... existing code ...
    
    // Pragmatic: If we receive keyboard events, we must have focus
    if !self.current_window_state.window_focused {
        self.current_window_state.window_focused = true;
        self.sync_ime_position_to_os();  // Phase 2: OnFocus (delayed)
    }
    
    // ... rest of key handling ...
}
```

**Vorteile:**
- Keine zusätzliche Wayland-API benötigt
- Funktioniert mit bestehendem Code
- Korrekte Semantik: "Keyboard-Events = Keyboard-Focus"

**Nachteil:**
- Focus wird erst bei ERSTEM Keypress erkannt (nicht sofort bei Focus-Wechsel)
- Aber: Das ist OK für IME! IME wird auch erst bei Keypress relevant

### Lösung 2: macOS Selected Range (NICHT BENÖTIGT)

**Entscheidung:** Aktuelle Implementierung (NSNotFound) ist korrekt.

**Begründung:**
1. Browsers implementieren das auch nicht für IME
2. IME-Komposition hat keine Text-Selection
3. Nur bei "Replace selected text with IME" relevant
4. Kann später hinzugefügt werden wenn nötig

**Wenn doch benötigt:**
```rust
// In MacOSWindow - neues Feld
pub text_selection: Option<(usize, usize)>,  // (start, length)

// In GLView/CPUView
#[unsafe(method(selectedRange))]
fn selected_range(&self) -> NSRange {
    if let Some(window_ptr) = *self.ivars().window_ptr.borrow() {
        unsafe {
            let macos_window = &*(window_ptr as *mut MacOSWindow);
            if let Some((start, length)) = macos_window.text_selection {
                return NSRange { location: start, length };
            }
        }
    }
    // No selection
    NSRange { location: usize::MAX, length: 0 }
}
```

Aber: Woher kommt `text_selection`?
- Müsste von Text-Layout-System kommen
- Müsste bei Maus-Selection / Shift+Arrow aktualisiert werden
- Komplex und vorerst nicht nötig

## Architektur-Anpassungen

### KEINE Anpassung nötig für macOS
- ✅ `selectedRange` bereits korrekt implementiert
- ✅ Gibt NSNotFound zurück (keine Selection)
- ✅ Entspricht Browser-Verhalten

### Minimale Anpassung für Wayland Focus
**Empfohlene Lösung:** Option D (Pragmatisch)

```rust
// dll/src/desktop/shell2/linux/wayland/mod.rs
pub fn handle_key(&mut self, key: u32, state: u32) {
    // ... existing code ...
    
    // Phase 2: OnFocus callback (delayed) - sync IME position on first keypress
    if !self.current_window_state.window_focused {
        self.current_window_state.window_focused = true;
        self.sync_ime_position_to_os();
    }
    
    // ... rest of existing key handling ...
}
```

**Alternative:** Option A (Native wl_keyboard Listener)
- Benötigt mehr Wayland-API-Integration
- Sauberer, aber komplexer
- Würde sofortigen Focus-Wechsel erkennen (nicht erst bei Keypress)

## Empfehlung

### Sofort implementieren (Wayland Focus - Option D)
✅ **Pragmatische Lösung in `handle_key()` hinzufügen**
- Minimale Code-Änderung
- Funktioniert mit bestehendem Code
- Ausreichend für IME-Use-Case

### Später evaluieren (Optional)
⚠️ **Native wl_keyboard Listener (Option A)**
- Wenn explizite Focus-Events benötigt werden
- Wenn IME-Position sofort bei Focus-Wechsel aktualisiert werden muss
- Benötigt mehr Wayland-dlopen-Integration

### NICHT implementieren (macOS Selection)
❌ **macOS `selectedRange` ist bereits korrekt**
- NSNotFound = keine Selection
- Entspricht Browser-Verhalten
- Nur bei speziellem Use-Case nötig (markierten Text mit IME ersetzen)

## Zusammenfassung

| Plattform | Feature | Status | Aktion |
|-----------|---------|--------|--------|
| Windows | OnFocus | ✅ Implementiert | WM_SETFOCUS + sync |
| Windows | OnCompositionStart | ✅ Implementiert | WM_IME_STARTCOMPOSITION + sync |
| Windows | Post-Layout | ✅ Implementiert | regenerate_layout() + sync |
| macOS | OnFocus | ✅ Implementiert | windowDidBecomeKey + sync |
| macOS | OnCompositionStart | ✅ Implementiert | setMarkedText + sync |
| macOS | Post-Layout | ✅ Implementiert | regenerate_layout() + sync |
| macOS | selectedRange | ✅ Korrekt | NSNotFound (keine Selection) |
| Linux X11 | OnFocus | ✅ Implementiert | FocusIn + sync |
| Linux X11 | OnCompositionStart | ✅ Native (XIM) | XIM handled automatisch |
| Linux X11 | Post-Layout | ❌ TODO | regenerate_layout() + sync |
| Linux Wayland | OnFocus | ⚠️ Fehlt | handle_key() + delayed focus |
| Linux Wayland | OnCompositionStart | ✅ GTK/text-input | GTK handled automatisch |
| Linux Wayland | Post-Layout | ❌ TODO | regenerate_layout() + sync |

**Nächste Schritte:**
1. ✅ Wayland Focus via `handle_key()` (pragmatisch)
2. ✅ X11 Post-Layout Callback
3. ✅ Wayland Post-Layout Callback
4. ✅ Kompilierung testen
5. ⚠️ Optional später: Native wl_keyboard Listener
