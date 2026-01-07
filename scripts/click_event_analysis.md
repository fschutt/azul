# Click Event Model Analysis

**Datum:** 7. Januar 2026

## Executive Summary

Die Analyse des Click-Event-Modells in Azul zeigt **zwei kritische Probleme**, die verhindern, dass Klicks auf Buttons funktionieren:

1. **Event Filter Mismatch**: Button registriert `MouseUp`, aber System dispatcht `LeftMouseUp`
2. **Fehlende Hit-Test-Daten bei Debug-API**: Die Debug-API aktualisiert nur den Window-State, aber nicht den Hit-Test

---

## Architektur-Übersicht

### JavaScript-Event-Modell (Referenz)

```
┌─────────────────────────────────────────────────────────────┐
│                    CAPTURING PHASE                          │
│  window → document → html → body → div → button             │
│                         ↓                                   │
├─────────────────────────────────────────────────────────────┤
│                     TARGET PHASE                            │
│                       button (event.target)                 │
│                         ↓                                   │
├─────────────────────────────────────────────────────────────┤
│                    BUBBLING PHASE                           │
│  button → div → body → html → document → window             │
└─────────────────────────────────────────────────────────────┘
```

**JavaScript Features:**
- `addEventListener(type, handler, useCapture)` - Capture vs Bubble phase
- `event.stopPropagation()` - Stop event from propagating
- `event.preventDefault()` - Prevent default browser action
- `event.target` - Original element that triggered event
- `event.currentTarget` - Element currently handling event

### Azul Event-Modell (Aktuell)

```
┌─────────────────────────────────────────────────────────────┐
│              1. Platform Event (NSEvent/WM_*)               │
│                         ↓                                   │
├─────────────────────────────────────────────────────────────┤
│              2. Window State Update                         │
│  - mouse_state.cursor_position                              │
│  - mouse_state.left_down = true/false                       │
│                         ↓                                   │
├─────────────────────────────────────────────────────────────┤
│              3. Hit Test (WebRender)                        │
│  - Determines which DOM nodes are under cursor              │
│  - Stores in hover_manager                                  │
│                         ↓                                   │
├─────────────────────────────────────────────────────────────┤
│              4. Event Determination                         │
│  determine_all_events() - Compares current vs previous      │
│  state to generate SyntheticEvents                          │
│                         ↓                                   │
├─────────────────────────────────────────────────────────────┤
│              5. Event Dispatch                              │
│  dispatch_synthetic_events() - Maps EventType to EventFilter│
│                         ↓                                   │
├─────────────────────────────────────────────────────────────┤
│              6. Callback Invocation                         │
│  invoke_callbacks_v2() - Finds matching callbacks on        │
│  hovered nodes and invokes them                             │
└─────────────────────────────────────────────────────────────┘
```

---

## Problem 1: Event Filter Mismatch (KRITISCH)

### Beschreibung

Der Button Widget registriert sich für `HoverEventFilter::MouseUp`:

```rust
// layout/src/widgets/button.rs:614
CoreCallbackData {
    event: EventFilter::Hover(HoverEventFilter::MouseUp),  // ← MouseUp
    callback: ...,
    refany: data,
}
```

Aber die Event-Dispatch-Logik konvertiert `EventType::MouseUp` zu `HoverEventFilter::LeftMouseUp`:

```rust
// core/src/events.rs:1886-1887
E::MouseUp => Some(EF::Hover(H::LeftMouseUp)),  // ← LeftMouseUp (UNTERSCHIEDLICH!)
```

### Warum passiert das?

`HoverEventFilter` hat **separate** Varianten für generische und button-spezifische Events:

```rust
// core/src/events.rs:1149-1170
pub enum HoverEventFilter {
    MouseDown,        // Generisch - irgendein Button
    LeftMouseDown,    // Spezifisch - linker Button
    RightMouseDown,   // Spezifisch - rechter Button
    MiddleMouseDown,  // Spezifisch - mittlerer Button
    MouseUp,          // Generisch - irgendein Button  ← Button registriert dieses
    LeftMouseUp,      // Spezifisch - linker Button    ← System dispatcht dieses
    RightMouseUp,     // Spezifisch - rechter Button
    MiddleMouseUp,    // Spezifisch - mittlerer Button
    ...
}
```

### Lösung

**Option A**: Button sollte `LeftMouseUp` registrieren (einfachste Lösung):
```rust
// layout/src/widgets/button.rs
event: EventFilter::Hover(HoverEventFilter::LeftMouseUp),
```

**Option B**: Dispatch-Logik sollte auch generisches `MouseUp` zusätzlich zu `LeftMouseUp` dispatchen

**Option C**: Callback-Matching sollte flexibler sein (z.B. `MouseUp` matcht auch `LeftMouseUp`)

---

## Problem 2: Hit-Test nicht aktualisiert bei Debug-API

### Beschreibung

Die Debug-API (`debug_server.rs`) ändert nur den Window-State:

```rust
// dll/src/desktop/shell2/common/debug_server.rs:1299-1330
DebugEvent::Click { x, y, ... } => {
    let mut new_state = callback_info.get_current_window_state().clone();
    new_state.mouse_state.cursor_position = CursorPosition::InWindow(...);
    new_state.mouse_state.left_down = true;
    callback_info.modify_window_state(new_state);  // ← Nur State-Änderung
    ...
}
```

**Aber**: Die Event-Verarbeitung benötigt Hit-Test-Daten in `hover_manager`:

```rust
// dll/src/desktop/shell2/common/event_v2.rs:764-777
if is_hover_event {
    // For hover events, search all nodes in the current hit test
    if let Some(hit_test) = layout_window.hover_manager.get_current(&InputPointId::Mouse) {
        for (dom_id, hit_test_data) in &hit_test.hovered_nodes {
            // ... search for matching callbacks
        }
    }
}
```

Wenn `hover_manager.get_current()` `None` zurückgibt (weil kein Hit-Test durchgeführt wurde), werden **keine Callbacks gefunden**!

### Vergleich: Native Events vs Debug-API

| Schritt | Native Events (macOS) | Debug-API |
|---------|----------------------|-----------|
| 1. Position setzen | ✅ `locationInWindow()` | ✅ `cursor_position = ...` |
| 2. Button-State | ✅ `left_down = true/false` | ✅ `left_down = true/false` |
| 3. Hit-Test | ✅ `self.update_hit_test(position)` | ❌ **FEHLT** |
| 4. Event-Dispatch | ✅ `process_window_events_recursive_v2()` | ❌ **FEHLT** |

### Lösung

Die Debug-API muss den vollständigen Event-Flow durchlaufen:

```rust
DebugEvent::Click { x, y, button, ... } => {
    // 1. Update window state
    let mut new_state = callback_info.get_current_window_state().clone();
    new_state.mouse_state.cursor_position = CursorPosition::InWindow(
        LogicalPosition { x: cx, y: cy }
    );
    
    // 2. WICHTIG: Hit-Test durchführen
    callback_info.update_hit_test(LogicalPosition { x: cx, y: cy });
    
    // 3. Mouse down
    new_state.mouse_state.left_down = true;
    callback_info.modify_window_state(new_state.clone());
    
    // 4. WICHTIG: Events verarbeiten (löst MouseDown callbacks aus)
    callback_info.process_events();
    
    // 5. Mouse up
    new_state.mouse_state.left_down = false;
    callback_info.modify_window_state(new_state);
    
    // 6. WICHTIG: Events erneut verarbeiten (löst MouseUp callbacks aus)
    callback_info.process_events();
}
```

---

## Problem 3: Kein Event Bubbling implementiert

### Beschreibung

Im aktuellen System werden Callbacks nur auf dem **direkt gehoverten Node** gesucht:

```rust
// event_v2.rs:771-777
for (node_id, _hit_item) in &hit_test_data.regular_hit_test_nodes {
    if let Some(node_data) = node_data_container.get(*node_id) {
        for callback in node_data.get_callbacks().iter() {
            if callback.event == event_filter {
                callbacks.push(callback.clone());  // Nur exakte Matches!
            }
        }
    }
}
```

**Es gibt kein Bubbling**: Wenn ein Text-Node geklickt wird, wird der Callback auf dem Parent-Button **nicht** ausgelöst.

### JavaScript-Verhalten (zum Vergleich)

```html
<button onclick="handleClick()">
    <span>Click me</span>  <!-- User klickt hier -->
</button>
```

In JavaScript würde der Click auf `<span>` zum `<button>` bublen und `handleClick()` auslösen.

### Azul-Verhalten

```xml
<div class="__azul-native-button-container" on:mouseup="handleClick">
    <text>Update counter</text>  <!-- User klickt hier -->
</div>
```

Der Klick auf `<text>` wird **nicht** zum Parent `<div>` weitergeleitet!

### Lösung: Event Bubbling implementieren

```rust
fn invoke_callbacks_v2(...) {
    // Sammle Callbacks, beginnend vom Ziel-Node
    let mut callbacks = Vec::new();
    let mut current_node_id = target_node_id;
    
    // Bubbling: Gehe durch Parent-Kette
    loop {
        if let Some(node_data) = get_node_data(current_node_id) {
            for callback in node_data.get_callbacks() {
                if callback.event == event_filter {
                    callbacks.push((current_node_id, callback.clone()));
                }
            }
        }
        
        // Gehe zum Parent
        match get_parent(current_node_id) {
            Some(parent) => current_node_id = parent,
            None => break,  // Kein Parent mehr
        }
    }
    
    // Invoke callbacks in Bubbling-Reihenfolge (Kind → Parent)
    for (node_id, callback) in callbacks {
        let result = invoke_single_callback(callback);
        if result.stop_propagation {
            break;  // Bubbling stoppen
        }
    }
}
```

---

## Empfohlene Fixes (Priorisiert)

### Fix 1: Event Filter korrigieren (5 Minuten)

```rust
// layout/src/widgets/button.rs:614
// ALT:
event: EventFilter::Hover(HoverEventFilter::MouseUp),
// NEU:
event: EventFilter::Hover(HoverEventFilter::LeftMouseUp),
```

### Fix 2: Debug-API Hit-Test hinzufügen (30 Minuten)

In `CallbackInfo` müssen folgende Methoden verfügbar sein:
- `update_hit_test(position: LogicalPosition)`
- `process_events() -> ProcessEventResult`

### Fix 3: Event Bubbling implementieren (2-4 Stunden)

1. `invoke_callbacks_v2()` ändern, um Parent-Kette zu durchlaufen
2. `CallCallbacksResult` um `stop_propagation: bool` erweitern
3. `CallbackInfo` um `stop_propagation()` Methode erweitern

---

## Anhang: Relevante Code-Locations

| Komponente | Datei | Zeilen |
|------------|-------|--------|
| Button Widget Callback | `layout/src/widgets/button.rs` | 612-620 |
| Event Type Mapping | `core/src/events.rs` | 1873-1930 |
| HoverEventFilter Enum | `core/src/events.rs` | 1145-1240 |
| Callback Invocation | `dll/src/desktop/shell2/common/event_v2.rs` | 707-820 |
| Event Determination | `layout/src/event_determination.rs` | 230-350 |
| Debug Server Click | `dll/src/desktop/shell2/common/debug_server.rs` | 1299-1460 |
| macOS Mouse Up Handler | `dll/src/desktop/shell2/macos/events.rs` | 145-210 |
