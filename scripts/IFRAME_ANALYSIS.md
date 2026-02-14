# IFrame / Infinity Scrolling — Vollständige Analyse

## Zusammenfassung

**`infinity.c` zeigt nichts an**, weil `scan_for_iframes()` in `window.rs` (Z. 902)
auf `self.layout_results.get(&dom_id)` zugreift, aber `layout_results` für die aktuelle
DOM-ID erst **nach** dem IFrame-Scan eingefügt wird (Z. 887). Dadurch gibt `scan_for_iframes`
immer einen leeren Vec zurück — kein IFrame wird jemals erkannt, kein Callback aufgerufen.

Zusätzlich: Selbst wenn der InitialRender-Bug gefixt wäre, würde **Scrollen innerhalb eines
IFrames keine Neuinvokation des IFrame-Callbacks auslösen** — der `EdgeScrolled`-Pfad ist
zwar implementiert (`IFrameManager.check_reinvoke`), wird aber nirgends durch Scroll-Events
getriggert.

---

## Bug #1: `scan_for_iframes` findet keine IFrames (CRITICAL)

### Ort
`layout/src/window.rs` — `layout_and_generate_display_list()`

### Ablauf (kaputt)
```
layout_and_generate_display_list():
  1. layout_document()          → erzeugt display_list, tree, calculated_positions
  2. scan_for_iframes(dom_id, &tree, &positions)   ← HIER
        → self.layout_results.get(&dom_id)?         ← RETURNS None!
        → Vec ist leer, keine IFrames gefunden
  3. for (node_id, bounds) in iframes { ... }       ← Loop läuft nie
  4. self.layout_results.insert(dom_id, ...)        ← Erst HIER gespeichert
```

### Root Cause
`scan_for_iframes()` (Z. 906) braucht `styled_dom.node_data` um `NodeType::IFrame` zu erkennen.
Es versucht, diese aus `self.layout_results.get(&dom_id)` zu holen — aber `layout_results`
wird erst in Schritt 4 befüllt. Die `?`-Propagation in der `filter_map` bewirkt, dass jeder
Node übersprungen wird.

### Fix
`scan_for_iframes` muss `styled_dom` als Parameter bekommen statt es aus `layout_results`
zu lesen. `styled_dom_clone` ist als lokale Variable bereits verfügbar (Z. 833).

```rust
// VORHER:
fn scan_for_iframes(&self, dom_id: DomId, layout_tree: &LayoutTree, 
    calculated_positions: &BTreeMap<usize, LogicalPosition>) -> Vec<(NodeId, LogicalRect)>

// NACHHER:  
fn scan_for_iframes(&self, styled_dom: &StyledDom, layout_tree: &LayoutTree,
    calculated_positions: &BTreeMap<usize, LogicalPosition>) -> Vec<(NodeId, LogicalRect)>
```

### Doppel-Bug: `invoke_iframe_callback` hat das gleiche Problem
`invoke_iframe_callback()` (Z. 1042) macht ebenfalls `self.layout_results.get(&parent_dom_id)?`
um die `IFrameNode`-Daten zu extrahieren. Das muss ebenfalls die `styled_dom` direkt bekommen,
oder alternativ wird `layout_results` **vor** dem IFrame-Scan eingefügt und danach aktualisiert.

---

## Bug #2: Scroll-Events triggern keine IFrame-Re-Invokation (DESIGN GAP)

### Situation
`IFrameManager.check_reinvoke()` implementiert `EdgeScrolled`-Detection:
- Prüft ob Scroll-Position innerhalb `EDGE_THRESHOLD` (200px) von einem Rand ist
- Gibt `IFrameCallbackReason::EdgeScrolled(EdgeType::Bottom)` etc. zurück

**Aber**: `check_reinvoke()` wird nur von `invoke_iframe_callback_impl()` aufgerufen.
Und `invoke_iframe_callback_impl()` wird nur aufgerufen von:
1. `layout_dom_recursive` → `scan_for_iframes`-Loop (Initial-Layout — wegen Bug #1 kaputt)
2. `process_iframe_updates` → `trigger_iframe_rerender()` (nur manuell via Callback)

**Es gibt keinen Pfad, der bei einem Scroll-Event `check_reinvoke()` aufruft.**

### Konsequenz
- `infinity.c` kann nicht funktionieren, selbst wenn Bug #1 gefixt wird
- Die C-Version hat KEINEN `on_scroll`-Handler (verlässt sich rein auf IFrame-Re-Invokation)
- Die Rust-Version (`infinity.rs`) umgeht das Problem mit `on_scroll` + `Update::RefreshDom`
  — aber das ist ein Full-DOM-Rebuild, kein gezieltes IFrame-Update

### Was fehlt
Ein Scroll-Event (macOS: `scrollWheel`, Windows: `WM_MOUSEWHEEL`) müsste nach der Position-
Aktualisierung prüfen, ob der gescrollte Node ein IFrame-Parent ist, und dann
`invoke_iframe_callback` erneut aufrufen — oder mindestens `trigger_iframe_rerender` queuen.

---

## Bug #3: `invoke_iframe_callback` — StyledDom-Zugriff-Race

### Ort
`layout/src/window.rs` Z. 1042

### Problem
```rust
fn invoke_iframe_callback(&mut self, parent_dom_id, node_id, bounds, ...) -> Option<DomId> {
    let layout_result = self.layout_results.get(&parent_dom_id)?;  // ← None beim Initial-Render!
    let node_data = layout_result.styled_dom.node_data.as_container().get(node_id)?;
    let iframe_node = match node_data.get_node_type() {
        NodeType::IFrame(iframe) => iframe.clone(),
        _ => return None,
    };
    self.invoke_iframe_callback_impl(...)
}
```

Beim allerersten Aufruf aus `layout_dom_recursive` existiert `layout_results[parent_dom_id]`
noch nicht → Funktion returnt `None` → kein IFrame wird gerendert.

---

## Vergleich: infinity.c vs. infinity.rs

| Aspekt | infinity.c | infinity.rs |
|--------|-----------|-------------|
| IFrame-Callback | `render_iframe()` — berechnet `visible_start` aus `info.scroll_offset.y` | `render_iframe()` — liest `d.visible_start` aus State |
| Scroll-Handling | Keines — verlässt sich auf IFrame-Re-Invokation | `on_scroll` Handler → `Update::RefreshDom` |
| Funktioniert? | ❌ Nein (Bug #1 + #2) | ⚠️ Teilweise (RefreshDom = Full Rebuild) |
| Architektur-Ideal | Richtig: IFrame-Callback entscheidet was sichtbar ist | Workaround: manuelles RefreshDom |

### infinity.rs Workaround-Analyse
Die Rust-Version registriert einen `Scroll`-Event-Handler auf dem IFrame-Div:
```rust
.with_callback(EventFilter::Hover(HoverEventFilter::Scroll), data.clone(), on_scroll)
```
`on_scroll` liest die Scroll-Position, berechnet `visible_start`, und gibt `Update::RefreshDom` zurück.
Das **rebuildet den gesamten DOM** — alle Layouts, alle Display Lists. Funktioniert, ist aber O(n)
statt O(visible_items) wie bei richtigem IFrame-Re-Invoke.

---

## Renderer-Seite: Funktioniert korrekt

`compositor2.rs` (Z. 1309-1413) handelt `DisplayListItem::IFrame` korrekt:
1. Erstellt `PipelineId` aus `child_dom_id`
2. Schlägt `child_layout_result` in `layout_results` nach
3. Ruft `translate_displaylist_to_wr` rekursiv auf
4. Pushes `builder.push_iframe(...)` an WebRender
5. Sammelt nested Pipelines

`process_iframe_updates` in `wr_translate2.rs` (Z. 2792) ist ebenfalls korrekt implementiert
für den `trigger_iframe_rerender`-Pfad.

**Das Problem liegt ausschließlich auf der Layout-Seite**, nicht beim Renderer.

---

## IFrameManager: Edge-Detection funktioniert (in Isolation)

Die Tests in `layout/tests/iframe_manager.rs` bestätigen:
- `InitialRender` wird korrekt für nie-invoked IFrames zurückgegeben
- `BoundsExpanded` wird korrekt erkannt wenn Container wächst
- `EdgeScrolled(Bottom)` wird korrekt erkannt wenn `scroll_offset.y` nahe am unteren Rand ist
- `EdgeScrolled(Right)` analog für horizontales Scrollen
- Edge-Flags verhindern doppelte Callbacks für denselben Rand

**Alles korrekt implementiert — aber nie aufgerufen.**

---

## C API: Funktionen existieren

| C-Funktion | Deklariert in | Rust-Implementation |
|-----------|--------------|---------------------|
| `AzDom_createIframe(data, callback)` | `dll/azul.h:30122` | `core/src/dom.rs:2412` |
| `AzIFrameCallbackReturn_withDom(dom, scroll_size, scroll_offset, virtual_scroll_size, virtual_scroll_offset)` | `dll/azul.h:29898` | `core/src/callbacks.rs:355` |
| `AzIFrameCallbackInfo` struct | `dll/azul.h` | `core/src/callbacks.rs:186` |

Die C-API-Bindungen sind korrekt. Das Problem ist nicht im FFI-Layer.

---

## Fehlende Verbindungen (Zusammenfassung)

```
KAPUTT:
  scan_for_iframes() ──→ layout_results  ← NICHT BEFÜLLT (Bug #1)
  invoke_iframe_callback() ──→ layout_results  ← NICHT BEFÜLLT (Bug #3)

FEHLT KOMPLETT:
  Scroll-Event ──→ IFrameManager.check_reinvoke()  ← KEIN PFAD (Bug #2)
  
FUNKTIONIERT:
  IFrameManager.check_reinvoke() Logik  ← ✅ korrekt
  IFrameCallbackReturn.with_dom()       ← ✅ korrekt
  compositor2 IFrame rendering          ← ✅ korrekt
  process_iframe_updates()              ← ✅ korrekt
  trigger_iframe_rerender() Pfad        ← ✅ korrekt (aber niemand ruft es bei Scroll auf)
```
