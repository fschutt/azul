# Cursor & Hit-Test Architektur-Analyse Report

## Executive Summary

Das Cursor-System in Azul hat **drei fundamentale Architektur-Probleme**, die durch mehrere "Hacks" teilweise kompensiert wurden, aber nie korrekt gelöst wurden. Das Hauptproblem ist eine **invertierte Depth-Logik** in `CursorTypeHitTest::new()`.

---

## 1. Architektur-Übersicht

### 1.1 Hit-Test Pipeline

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           HIT-TEST PIPELINE                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. DISPLAY LIST BUILDING (layout/src/solver3/display_list.rs)              │
│     ┌─────────────────────────────────────────────────────────┐             │
│     │ paint_node_content()      → push_hit_test_area()        │             │
│     │ paint_inline_shape()      → push_hit_test_area() [HACK] │             │
│     │ generate_for_stacking_context() → push_hit_test_area()  │             │
│     └─────────────────────────────────────────────────────────┘             │
│                              ↓                                              │
│  2. TAG-ID ASSIGNMENT (core/src/prop_cache.rs)                              │
│     ┌─────────────────────────────────────────────────────────┐             │
│     │ CssPropertyCache::restyle() → creates TagIdToNodeIdMapping            │
│     │ Nodes get tags if they have:                            │             │
│     │   - Callbacks (onClick, onHover, etc.)                  │             │
│     │   - :hover/:active/:focus CSS pseudo-classes            │             │
│     │   - Non-default cursor: property                        │             │
│     │   - overflow: scroll/auto                               │             │
│     │   - Selectable text children [HACK]                     │             │
│     └─────────────────────────────────────────────────────────┘             │
│                              ↓                                              │
│  3. WEBRENDER HIT-TEST (dll/src/desktop/wr_translate2.rs)                   │
│     ┌─────────────────────────────────────────────────────────┐             │
│     │ fullhittest_new_webrender()                             │             │
│     │   - Calls WebRender's hit_test(physical_pos)            │             │
│     │   - Results are FRONT-TO-BACK (depth 0 = frontmost)     │             │
│     │   - Maps tags back to NodeIds via TagIdToNodeIdMapping  │             │
│     │   - Stores hit_depth from enumerate() index             │             │
│     └─────────────────────────────────────────────────────────┘             │
│                              ↓                                              │
│  4. CURSOR TYPE RESOLUTION (layout/src/hit_test.rs)                         │
│     ┌─────────────────────────────────────────────────────────┐             │
│     │ CursorTypeHitTest::new()                                │             │
│     │   - Iterates all hit nodes                              │             │
│     │   - Finds node with cursor: property                    │             │
│     │   - PROBLEM: Uses WRONG depth comparison                │             │
│     └─────────────────────────────────────────────────────────┘             │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Relevante Dateien

| Datei | Verantwortung |
|-------|---------------|
| `layout/src/solver3/display_list.rs` | Baut Display-Liste, pusht hit-test areas |
| `core/src/prop_cache.rs` | Tag-ID Assignment, bestimmt welche Nodes gehittet werden können |
| `core/src/hit_test_tag.rs` | Namespace-System für Tags (0x0100=DOM, 0x0200=Scrollbar) |
| `dll/src/desktop/wr_translate2.rs` | Übersetzt WebRender-Ergebnisse zu FullHitTest |
| `layout/src/hit_test.rs` | CursorTypeHitTest - bestimmt finalen Cursor |
| `core/src/ua_css.rs` | User-Agent CSS defaults (cursor:text für Text, cursor:pointer für Button) |

---

## 2. Die drei Hauptprobleme

### 2.1 PROBLEM 1: Invertierte Depth-Logik (KRITISCH)

**Ort:** `layout/src/hit_test.rs`, Zeilen 41-120

**Was passiert:**
```rust
// AKTUELLER BUGGY CODE:
let mut best_depth: u32 = 0;  // ← Initialisiert auf 0

// ...später...
if node_depth >= best_depth {  // ← Bevorzugt HÖHERE depths
    cursor_node = Some((*dom_id, *node_id));
    cursor_icon = translated;
    best_depth = node_depth;
}
```

**Das Problem:**
WebRender liefert Hit-Test-Ergebnisse **front-to-back**, d.h.:
- `depth = 0` = vorderster/oberster Node (z.B. Button)
- `depth = 2` = hinterster Node (z.B. Body)

Aber der Code bevorzugt **höhere** depths, also wählt er den **hintersten** Node!

**Debug-Output zeigt:**
```
DOM 0: 3 hit nodes
  NodeId=3 depth=0 type=Text      ← Text im Button (VORDERSTER)
  NodeId=2 depth=1 type=Button    ← Button
  NodeId=0 depth=2 type=Body      ← Body (HINTERSTER)
    -> text child NodeId=1 text_depth=3  ← HACK fügt depth hinzu!
  RESULT: cursor_icon=Text        ← FALSCH! Sollte Pointer sein
```

**Fix:**
```rust
let mut best_depth: u32 = u32::MAX;  // ← u32::MAX statt 0

if node_depth < best_depth {  // ← KLEINER statt größer
    cursor_node = Some((*dom_id, *node_id));
    cursor_icon = translated;
    best_depth = node_depth;
}
```

---

### 2.2 PROBLEM 2: Text-Child-Detection-Hack

**Ort:** `layout/src/hit_test.rs`, Zeilen 79-103

**Was der Hack macht:**
```rust
// Für jeden gehitteten Node, schaue ob er Text-Kinder hat
let hier = &node_hierarchy[*node_id];
if let Some(first_child) = hier.first_child_id(*node_id) {
    let mut child_id = Some(first_child);
    while let Some(cid) = child_id {
        let child_data = &node_data_container[cid];
        if matches!(child_data.get_node_type(), NodeType::Text(_)) {
            // Text-Kind gefunden → erhöhe Depth um 1
            let text_depth = node_depth + 1;  // ← KÜNSTLICHE DEPTH-ERHÖHUNG
            if text_depth > best_depth {
                // Setze Cursor auf Text-Child's cursor (I-beam)
                cursor_icon = translate_cursor(css_cursor);
            }
        }
    }
}
```

**Warum dieser Hack existiert:**
Text-Nodes sind **inline** und bekommen keine eigenen hit-test areas. Der Hack versucht, den I-beam-Cursor zu zeigen wenn die Maus über Text ist.

**Warum der Hack kaputt ist:**
1. Er prüft **alle** gehitteten Nodes auf Text-Kinder, nicht nur den vordersten
2. Kombiniert mit der invertierten Depth-Logik führt das dazu, dass:
   - Body (depth=2) wird geprüft
   - Body hat Text-Kind → text_depth = 3
   - 3 > 0, also wird I-beam gesetzt
   - Button (depth=0) "verliert" gegen den Body!

**Resultat:** I-beam-Cursor erscheint auf dem gesamten Body-Hintergrund, nicht nur über Text.

---

### 2.3 PROBLEM 3: Selectable-Text-Tag-Assignment

**Ort:** `core/src/prop_cache.rs`, Zeilen 906-946

**Was der Code macht:**
```rust
// Check for selectable text - nodes that contain text children and 
// user-select != none need hit-test tags for text selection support
let node_has_selectable_text = {
    // Check if this node has immediate text children
    let has_text_children = {
        let hier = node_hierarchy.as_container()[node_id];
        let mut has_text = false;
        if let Some(first_child) = hier.first_child_id(node_id) {
            let mut child_id = Some(first_child);
            while let Some(cid) = child_id {
                let child_data = &node_data_container[cid.index()];
                if matches!(child_data.get_node_type(), NodeType::Text(_)) {
                    has_text = true;
                    break;
                }
                child_id = node_hierarchy.as_container()[cid].next_sibling_id();
            }
        }
        has_text
    };
    
    if has_text_children {
        // Prüfe user-select property
        !matches!(user_select, StyleUserSelect::None)
    } else {
        false
    }
};

if node_has_selectable_text {
    node_should_have_tag = true;  // ← CONTAINER bekommt Tag, nicht Text-Node
}
```

**Das Problem:**
1. Der **Container** (Body, Div, etc.) bekommt den Tag, nicht der Text-Node selbst
2. Das bedeutet: Wenn du irgendwo auf den Body klickst, wird der Body gehittet
3. Dann kickt der Text-Child-Hack ein und zeigt I-beam

**Gewolltes Verhalten:**
I-beam sollte nur erscheinen wenn die Maus **direkt über dem Text** ist, nicht über dem gesamten Container.

---

## 3. Warum der Button nicht funktioniert

### 3.1 DOM-Struktur

```
Body (NodeId=0)
  ├── Text "Hello " (NodeId=1)
  ├── Button (NodeId=2)
  │     └── Text "Click me" (NodeId=3)
  └── ...
```

### 3.2 Hit-Test bei Maus über Button

WebRender liefert (front-to-back):
```
1. Text "Click me" (NodeId=3) - depth=0 - VORDERSTER
2. Button (NodeId=2)          - depth=1 - cursor:pointer
3. Body (NodeId=0)            - depth=2 - HINTERSTER
```

### 3.3 Was CursorTypeHitTest::new() macht

```
best_depth = 0  // Initial

NodeId=3 (Text, depth=0):
  - Hat cursor:text (aus UA CSS)
  - depth=0 >= best_depth=0 → NICHT AUSGEWÄHLT (weil Code >= und nicht > nutzt)

NodeId=2 (Button, depth=1):
  - Hat cursor:pointer (aus UA CSS)  
  - depth=1 >= best_depth=0 → AUSGEWÄHLT
  - best_depth = 1

NodeId=0 (Body, depth=2):
  - Kein cursor property auf Body selbst
  - ABER: Text-Child-Hack schaut nach Kindern
  - Findet Text (NodeId=1) → text_depth = 2+1 = 3
  - 3 > best_depth=1 → Text-Child "GEWINNT"!
  - cursor_icon = Text (I-beam)

ERGEBNIS: cursor=Text, node=Body
ERWARTET: cursor=Pointer, node=Button
```

---

## 4. Alle "Hacks" im Überblick

| # | Hack | Ort | Zweck | Problem |
|---|------|-----|-------|---------|
| 1 | Text-Child-Detection | `layout/src/hit_test.rs:79-103` | I-beam über Text zeigen | Wirkt auf alle Nodes, nicht nur vordersten |
| 2 | Selectable-Text-Tags | `core/src/prop_cache.rs:906-946` | Text-Selection ermöglichen | Container statt Text bekommt Tag |
| 3 | paint_inline_shape hit-test | `layout/src/solver3/display_list.rs:2660` | inline-block Elemente hittbar machen | Wurde hinzugefügt, löst aber nicht das Depth-Problem |
| 4 | Scrollable pre-clip hit-test | `layout/src/solver3/display_list.rs:1440` | Scroll-Events vor Clip-Region hittbar | Korrekt, aber kompliziert |

---

## 5. Empfohlene Fixes

### 5.1 FIX 1: Depth-Logik korrigieren (KRITISCH)

```rust
// layout/src/hit_test.rs

impl CursorTypeHitTest {
    pub fn new(hit_test: &FullHitTest, layout_window: &LayoutWindow) -> Self {
        let mut cursor_node = None;
        let mut cursor_icon = MouseCursorType::Default;
        let mut best_depth: u32 = u32::MAX;  // ← ÄNDERN

        for (dom_id, hit_nodes) in hit_test.hovered_nodes.iter() {
            // ...
            for (node_id, hit_item) in hit_nodes.regular_hit_test_nodes.iter() {
                let node_depth = hit_item.hit_depth;
                
                // FIX: Kleinere depth = weiter vorne = höhere Priorität
                if node_depth < best_depth {  // ← ÄNDERN
                    if let Some(cursor_prop) = cursor_prop_opt {
                        cursor_node = Some((*dom_id, *node_id));
                        cursor_icon = translate_cursor(css_cursor);
                        best_depth = node_depth;
                    }
                }
            }
        }
        // ...
    }
}
```

### 5.2 FIX 2: Text-Child-Hack entfernen

Der Text-Child-Hack sollte **komplett entfernt** werden. Er war ein Workaround für das invertierte Depth-Problem.

Nach dem Depth-Fix:
- Text-Node (depth=0) hat `cursor:text` aus UA CSS
- Button (depth=1) hat `cursor:pointer` aus UA CSS
- Der vorderste Node (Text=0 wenn über Text, Button=1 wenn über Button-Hintergrund) gewinnt

### 5.3 FIX 3: (Optional) Feinere Text-Hit-Detection

Für präzisen I-beam nur über Glyphen (nicht über Leerraum im Text-Container):
1. Hit-test areas für individuelle Text-Runs in `paint_inline_content()` pushen
2. Oder: Geometrische Prüfung in CursorTypeHitTest mit `point_relative_to_item`

---

## 6. Test-Szenario nach Fix

### 6.1 Maus über Button-Text

```
Hit nodes: Text(3,depth=0), Button(2,depth=1), Body(0,depth=2)
Text(3) hat cursor:text, depth=0 < u32::MAX → AUSGEWÄHLT
ERGEBNIS: cursor=Text (I-beam) über Text
```

### 6.2 Maus über Button-Hintergrund (neben Text)

```
Hit nodes: Button(2,depth=0), Body(0,depth=1)
Button(2) hat cursor:pointer, depth=0 < u32::MAX → AUSGEWÄHLT
ERGEBNIS: cursor=Pointer über Button
```

### 6.3 Maus über Body-Text

```
Hit nodes: Text(1,depth=0), Body(0,depth=1)
Text(1) hat cursor:text, depth=0 < u32::MAX → AUSGEWÄHLT
ERGEBNIS: cursor=Text (I-beam) über Text
```

### 6.4 Maus über Body-Hintergrund

```
Hit nodes: Body(0,depth=0)
Body(0) hat kein cursor property → cursor=Default
ERGEBNIS: cursor=Default über Body-Hintergrund
```

---

## 7. Zusammenfassung

| Problem | Root Cause | Fix | Priorität |
|---------|------------|-----|-----------|
| Button cursor:pointer funktioniert nicht | Invertierte Depth-Logik | `best_depth = u32::MAX`, `<` statt `>=` | KRITISCH |
| I-beam auf gesamtem Body | Text-Child-Hack + invertierte Depth | Text-Child-Hack entfernen | HOCH |
| Intermittierend 0 DOMs im Hit-Test | Unklar - möglicherweise Timing | Weitere Analyse nötig | MITTEL |

Der wichtigste Fix ist die **Depth-Logik-Korrektur**. Nach diesem Fix sollte der Text-Child-Hack nicht mehr nötig sein und kann entfernt werden.
