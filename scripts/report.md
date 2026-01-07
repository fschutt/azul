# E2E Test Report: test_hello_world_click.sh

**Datum:** 3. Januar 2026  
**Analysierte Commits:**
- `f4af2c89` - Fix BFC re-layout for inline and inline-block sizing
- `b44d1ef1` - Fix display:inline sizing according to CSS 2.2 § 10.3.1
- `e7c1c95c` - Refactor background and border painting for inline / inline-block items

---

## Zusammenfassung der Probleme

Es wurden **4 Hauptprobleme** identifiziert, die den E2E-Test und die Funktionalität des hello-world Beispiels beeinträchtigen:

| # | Problem | Schweregrad | Status |
|---|---------|-------------|--------|
| 1 | CSS Specificity: Inline-Styles haben niedrigere Priorität als Stylesheet-Styles | **KRITISCH** | Offen |
| 2 | Button-Styling wird nicht angewendet | **HOCH** | Folge von #1 |
| 3 | Cursor:pointer wird bei Hover nicht aktualisiert | **MITTEL** | Offen |
| 4 | Mausklicks werden nicht registriert / Callbacks nicht ausgelöst | **KRITISCH** | Offen |

---

## Problem 1: CSS Specificity ist invertiert (KRITISCH)

### Beschreibung
Die CSS-Kaskade in Azul priorisiert **Stylesheet-Styles über Inline-Styles**, was dem CSS-Standard widerspricht.

### Technische Details

In [core/src/prop_cache.rs](../core/src/prop_cache.rs#L1342-L1500) wird in `get_property()` folgende Reihenfolge verwendet:

```rust
// AKTUELLER (FALSCHER) CODE:
if node_state.normal {
    // 1. ZUERST: CSS Stylesheet Properties
    if let Some(p) = self.css_normal_props.get(node_id)... { return Some(p); }
    
    // 2. DANACH: Inline CSS Properties  
    if let Some(p) = node_data.inline_css_props... { return Some(p); }
    
    // 3. ZULETZT: Cascaded/Inherited Properties
    if let Some(p) = self.cascaded_normal_props... { return Some(p); }
}
```

### Erwartetes Verhalten (CSS Spec)
Die CSS-Spezifikation definiert folgende Prioritätsreihenfolge (höchste zuerst):
1. **Inline-Styles** (höchste Priorität, da `style=""` Attribut)
2. ID-Selektoren
3. Klassen-/Attribut-Selektoren
4. Element-Selektoren

### Korrektur
Die Reihenfolge muss umgekehrt werden:

```rust
// KORREKTER CODE:
if node_state.normal {
    // 1. ZUERST: Inline CSS Properties (höchste Priorität!)
    if let Some(p) = node_data.inline_css_props... { return Some(p); }
    
    // 2. DANACH: CSS Stylesheet Properties
    if let Some(p) = self.css_normal_props.get(node_id)... { return Some(p); }
    
    // 3. ZULETZT: Cascaded/Inherited Properties
    if let Some(p) = self.cascaded_normal_props... { return Some(p); }
}
```

### Betroffene Stellen
- [core/src/prop_cache.rs#L1342-L1500](../core/src/prop_cache.rs#L1342-L1500) - `get_property()` Funktion
- Alle State-Varianten: `normal`, `hover`, `active`, `focus`

---

## Problem 2: Button-Styling wird nicht angewendet (HOCH)

### Beschreibung
Der "Increase Counter" Button im hello-world Beispiel erscheint vollständig ungestylt (kein Hintergrund, kein Border, kein Padding).

### Ursache
Dies ist eine **direkte Folge von Problem 1**. Das Button-Widget in [layout/src/widgets/button.rs](../layout/src/widgets/button.rs#L543-L648) setzt seine Styles über `inline_css_props`:

```rust
// Button::dom() in button.rs
Dom::create_div()
    .with_ids_and_classes(IdOrClassVec::from_const_slice(CONTAINER_CLASS))
    .with_inline_css_props(self.container_style)  // <-- Diese werden ignoriert!
    .with_callbacks(callbacks.into())
```

Die `container_style` enthält:
- `display: inline`
- `background-content: [...]`
- `cursor: pointer`
- `border: 1px solid rgb(172, 172, 172)`
- `padding: 5px 3px`

Da aber `Css::empty()` beim Styling verwendet wird:
```rust
// hello-world.rs
Dom::create_body()
    .with_child(label)
    .with_child(button)
    .style(Css::empty())  // Leeres Stylesheet, aber Inline-Props werden überschrieben
```

### Lösung
Durch Behebung von Problem 1 (CSS Specificity) wird dieses Problem automatisch gelöst.

---

## Problem 3: Cursor wird bei Hover nicht aktualisiert (MITTEL)

### Beschreibung
Wenn der Mauszeiger über den Button bewegt wird, ändert sich der Cursor nicht auf `pointer` (Hand-Symbol).

### Technische Details

Der Button definiert `cursor: pointer` in seinen Inline-Styles:
```rust
// layout/src/widgets/button.rs
Normal(CssProperty::const_cursor(StyleCursor::Pointer)),
```

Die Cursor-Aktualisierung erfolgt in den Platform-spezifischen Event-Handlern:

**macOS** ([dll/src/desktop/shell2/macos/events.rs#L257-L262](../dll/src/desktop/shell2/macos/events.rs#L257-L262)):
```rust
if let Some(hit_test) = layout_window.hover_manager.get_current(&InputPointId::Mouse) {
    let cursor_test = layout_window.compute_cursor_type_hit_test(hit_test);
    self.set_cursor(cursor_name);
}
```

### Mögliche Ursachen
1. **Hit-Test liefert keine Ergebnisse** - Der Button wird nicht als "unter dem Cursor" erkannt
2. **CSS-Eigenschaften werden nicht geladen** - Wegen Problem 1 wird `cursor: pointer` nicht gelesen
3. **`compute_cursor_type_hit_test()` scheitert** - Die Cursor-Berechnung gibt falschen Wert zurück

### Debugging-Schritte
1. Über Debug-API `hit_test` an Button-Position senden
2. Prüfen ob `node_id` zurückgegeben wird
3. CSS-Properties des Nodes über `get_node_css_properties` abfragen

---

## Problem 4: Mausklicks werden nicht registriert (KRITISCH)

### Beschreibung
Wenn per Debug-API Mausklicks gesendet werden (`mouse_down`, `mouse_up`), wird der Button-Callback nicht ausgelöst und der Counter erhöht sich nicht.

### Analyse der Debug-API

Die Debug-Server Implementierung in [dll/src/desktop/shell2/common/debug_server.rs](../dll/src/desktop/shell2/common/debug_server.rs#L1200-L1280) zeigt:

```rust
DebugEvent::MouseDown { x, y, button } => {
    let mut new_state = callback_info.get_current_window_state().clone();
    new_state.mouse_state.cursor_position = CursorPosition::InWindow(...);
    new_state.mouse_state.left_down = true;
    callback_info.modify_window_state(new_state);
    needs_update = true;
}
```

### Potenzielle Probleme

1. **Kein Hit-Test-Update**
   - Die Debug-API aktualisiert nur `window_state`, aber führt keinen Hit-Test durch
   - Die Event-Verarbeitung erwartet aber gültige Hit-Test-Daten

2. **Fehlende Event-Synthese**
   - `modify_window_state()` ändert nur den State
   - Es werden keine synthetischen Events generiert (`MouseUp`, `MouseDown`)
   - Der Window State Diff in der Event-Schleife erkennt die Änderung möglicherweise nicht korrekt

3. **Timer-basierte Verarbeitung**
   - Die Debug-API arbeitet über einen Timer (`debug_timer_callback`)
   - Zwischen `mouse_down` und `mouse_up` muss ein Frame-Update erfolgen
   - Das aktuelle Script sendet beide Events möglicherweise zu schnell

### Korrektur-Vorschlag

Die Debug-API sollte explizit den Event-Processing-Pfad auslösen:

```rust
DebugEvent::MouseDown { x, y, button } => {
    // 1. Update window state
    let mut new_state = callback_info.get_current_window_state().clone();
    new_state.mouse_state.cursor_position = CursorPosition::InWindow(...);
    new_state.mouse_state.left_down = true;
    
    // 2. Update hit test at position
    callback_info.update_hit_test(LogicalPosition { x, y });  // FEHLT!
    
    // 3. Modify state (triggers state diffing)
    callback_info.modify_window_state(new_state);
    
    // 4. Process events synchronously
    callback_info.process_pending_events();  // FEHLT!
    
    needs_update = true;
}
```

---

## Problem mit dem E2E-Test-Script

### Aktuelle Probleme im Script

1. **Falscher initialer Counter-Wert**
   ```bash
   # Script erwartet:
   if echo "$initial_html" | grep -q ">5<"; then
   
   # Aber hello-world.rs initialisiert mit:
   let data = DataModel { counter: 0 };  # Counter startet bei 0, nicht 5!
   ```

2. **Button-Position wird geschätzt**
   ```bash
   button_x=$(echo "$window_width / 2" | bc)
   button_y=$(echo "$window_height * 3 / 4" | bc)
   ```
   - Ohne funktionierendes Button-Styling ist die tatsächliche Position unbekannt
   - Sollte `get_all_nodes_layout` verwenden um Button-Rect zu finden

3. **Keine Verzögerung zwischen Events**
   ```bash
   send_request "{\"type\":\"mouse_down\",...}"
   send_request "{\"type\":\"mouse_up\",...}"  # Sofort danach!
   ```
   - Möglicherweise zu schnell für State-Diffing

---

## Empfohlene Reihenfolge der Fixes

### Priorität 1: CSS Specificity (Problem 1)
**Datei:** `core/src/prop_cache.rs`

Inline-Styles müssen vor Stylesheet-Styles geprüft werden in:
- `get_property()` 
- Alle State-Varianten (normal, hover, active, focus)

### Priorität 2: Event-Synthese in Debug-API (Problem 4)
**Datei:** `dll/src/desktop/shell2/common/debug_server.rs`

Erweitern der Maus-Event-Handler um:
- Hit-Test Update
- Event-Processing Auslösung

### Priorität 3: Test-Script korrigieren
**Datei:** `scripts/test_hello_world_click.sh`

- Initial Counter von 5 auf 0 ändern
- Button-Position aus Layout-Daten lesen
- Delay zwischen mouse_down und mouse_up einfügen

---

## Anhang: Relevante Code-Locations

| Komponente | Datei | Zeilen |
|------------|-------|--------|
| CSS Property Cache | `core/src/prop_cache.rs` | 1342-1500 |
| Button Widget | `layout/src/widgets/button.rs` | 543-648 |
| Debug Server | `dll/src/desktop/shell2/common/debug_server.rs` | 1000-1400 |
| macOS Events | `dll/src/desktop/shell2/macos/events.rs` | 240-350 |
| Cursor Hit Test | `layout/src/window.rs` | 1836-1900 |
| StyledDom Creation | `core/src/styled_dom.rs` | 637-800 |
