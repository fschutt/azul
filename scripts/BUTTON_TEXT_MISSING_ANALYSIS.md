# Button-Text nicht sichtbar - Analyse-Report

## ✅ PROBLEM GELÖST

Das Problem wurde behoben! Der Button-Text "Increase counter" wird jetzt korrekt angezeigt.

## Zusammenfassung

Das Problem: Im Hello-World C-Beispiel wurde der Button angezeigt (grauer Kasten), aber der Button-Text "Increase counter" fehlte komplett.

## Root Causes (behoben)

Es gab **zwei zusammenhängende Probleme**:

### Problem 1: inline_layout_result auf falschem Node gespeichert

**Datei:** [fc.rs#L5315-5332](../layout/src/solver3/fc.rs#L5315-5332)

Die `inline_layout_result` (die den gelayouteten Text enthält) wurde auf dem Text-Kind des inline-blocks gespeichert, nicht auf dem inline-block selbst. Wenn `paint_inline_object` versuchte, den Inhalt zu rendern, suchte es auf dem falschen Node.

**Fix:** Nach dem Layout eines inline-blocks wird die `inline_layout_result` vom IFC-Kind auf den inline-block-Parent propagiert:

```rust
// FIX: Propagate inline_layout_result from IFC child to inline-block parent
{
    let inline_block_node = tree.get(child_index).unwrap();
    let first_child_with_ifc = inline_block_node.children.iter()
        .filter_map(|&c| tree.get(c))
        .find(|n| n.inline_layout_result.is_some());
    if let Some(ifc_child) = first_child_with_ifc {
        let cached_layout = ifc_child.inline_layout_result.clone();
        if cached_layout.is_some() {
            tree.get_mut(child_index).unwrap().inline_layout_result = cached_layout;
        }
    }
}
```

### Problem 2: paint_inline_object renderte Inline-Block-Inhalt nicht

**Datei:** [display_list.rs#L2568-2592](../layout/src/solver3/display_list.rs#L2568-2592)

Die Funktion `paint_inline_object` rief nur `paint_inline_shape` auf (für Hintergrund/Border), aber renderte nie den eigentlichen Inhalt des inline-blocks.

**Fix:** Nach dem Rendern von Background/Border wird jetzt auch der Inhalt via `paint_inline_content` gerendert:

```rust
InlineContent::Shape(shape) => {
    self.paint_inline_shape(builder, object_bounds, shape, bounds)?;

    // FIX: Render the content of inline-blocks
    if let Some(node_id) = shape.source_node_id {
        if let Some(indices) = self.positioned_tree.tree.dom_to_layout.get(&node_id) {
            if let Some(&layout_idx) = indices.first() {
                if let Some(node) = self.positioned_tree.tree.get(layout_idx) {
                    if let Some(cached) = &node.inline_layout_result {
                        let border_box = BorderBoxRect(object_bounds);
                        let content_box = border_box.to_content_box(
                            &node.box_props.padding,
                            &node.box_props.border,
                        );
                        self.paint_inline_content(builder, content_box.rect(), &cached.layout)?;
                    }
                }
            }
        }
    }
}
```

## Rendering-Flow (korrigiert)

```
1. Body (BFC) → paint_in_flow_descendants()
   ├── Anonymous IFC Wrapper → paint_node_content()
   │   └── IFC hat inline_layout_result → paint_inline_content()
   │       ├── Text "5" → push_text_run() ✓ Text wird gerendert
   │       └── Button (Shape) → paint_inline_object()
   │           ├── paint_inline_shape() → push_backgrounds_and_border() ✓ Hintergrund wird gerendert
   │           └── paint_inline_content(button.inline_layout_result) ✓ Button-Text wird gerendert!
```

## Dateien geändert

- [layout/src/solver3/fc.rs](../layout/src/solver3/fc.rs) - +17 Zeilen
- [layout/src/solver3/display_list.rs](../layout/src/solver3/display_list.rs) - +25 Zeilen

## Tests

Alle 3 Tests in `inline_block_text.rs` bestehen:
- `test_inline_block_text_generates_text_items` ✓
- `test_inline_block_css_width_is_applied` ✓
- `test_text_wraps_at_constrained_width` ✓

### 2. **Hinweis: Doppelte Button-Display-Definition (NICHT der Hauptbug)**

**Datei:** [core/src/ua_css.rs](../core/src/ua_css.rs)

```rust
// Zeile 655 - ERSTER Match, wird verwendet
(NT::Button, PT::Display) => Some(&DISPLAY_INLINE_BLOCK),

// Zeile 683 - Zweiter Match, wird NIE erreicht (Rust wählt ersten Match)
(NT::Button, PT::Display) => Some(&DISPLAY_INLINE),
```

In Rust wird der ERSTE passende Match-Arm gewählt. Da Zeile 655 vor Zeile 683 kommt, bekommt Button korrekt `display: inline-block`. Die doppelte Definition sollte trotzdem entfernt werden (ist aber nicht die Ursache des Bugs).

### 3. **Koordinatensystem-Änderungen in neueren Commits (sekundär)**

Die letzten Commits (349f3d9f und 4fbf76de) haben Änderungen an der Koordinatenberechnung vorgenommen, aber diese sind nicht die Hauptursache. Jedoch könnten sie den Button-Offset erklären.

## Empfohlene Fixes

### Fix 1: paint_inline_object muss Inhalt rendern (PRIORITÄT 1)

In [display_list.rs](../layout/src/solver3/display_list.rs) muss `paint_inline_object` oder `paint_inline_shape` den Inhalt des inline-blocks rendern:

```rust
fn paint_inline_object(
    &self,
    builder: &mut DisplayListBuilder,
    base_pos: LogicalPosition,
    positioned_item: &PositionedItem,
) -> Result<()> {
    // ... existing code ...
    
    match content {
        InlineContent::Image(image) => { /* ... */ }
        InlineContent::Shape(shape) => {
            self.paint_inline_shape(builder, object_bounds, shape, bounds)?;
            
            // ✅ FIX: Render inline-block content!
            if let Some(node_id) = shape.source_node_id {
                // Find the layout node for this inline-block
                if let Some(&layout_idx) = self.positioned_tree.tree.dom_to_layout
                    .get(&node_id)
                    .and_then(|v| v.first()) 
                {
                    if let Some(node) = self.positioned_tree.tree.get(layout_idx) {
                        // If this inline-block has its own inline content, render it
                        if let Some(cached_layout) = &node.inline_layout_result {
                            let inline_layout = &cached_layout.layout;
                            
                            // Calculate content-box rect for the inline-block
                            let border_box = object_bounds; // already adjusted
                            let content_box_origin = LogicalPosition {
                                x: border_box.origin.x + node.box_props.padding.left + node.box_props.border.left,
                                y: border_box.origin.y + node.box_props.padding.top + node.box_props.border.top,
                            };
                            let content_rect = LogicalRect::new(
                                content_box_origin,
                                LogicalSize::new(
                                    (border_box.size.width - node.box_props.padding.left - node.box_props.padding.right
                                        - node.box_props.border.left - node.box_props.border.right).max(0.0),
                                    (border_box.size.height - node.box_props.padding.top - node.box_props.padding.bottom
                                        - node.box_props.border.top - node.box_props.border.bottom).max(0.0),
                                ),
                            );
                            
                            self.paint_inline_content(builder, content_rect, inline_layout)?;
                        }
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}
```

### Fix 2: Entferne doppelte Button-Definition (PRIORITÄT 2)

In [ua_css.rs](../core/src/ua_css.rs#L683), entferne die doppelte Definition:

```rust
// ENTFERNE Zeile 683:
// (NT::Button, PT::Display) => Some(&DISPLAY_INLINE),
```

### Fix 3: Überprüfe Koordinaten-Offset (PRIORITÄT 3)

Falls der Button-Offset nach links weiterhin besteht, überprüfe die margin-box → border-box Konvertierung in `paint_inline_shape`.

## Detaillierte Analyse des Rendering-Flows

### Layout-Phase (fc.rs)

1. **Body** wird als BFC layoutet → `layout_bfc()`
2. Body hat inline-Kinder (Text "5" + Button) → Anonymous IFC Wrapper wird erstellt
3. **Anonymous IFC Wrapper** wird layoutet → `layout_ifc()`
4. `collect_and_measure_inline_content()` sammelt:
   - Text "5" als `InlineContent::Text`
   - Button als `InlineContent::Shape` (via `layout_formatting_context()` rekursiv)
5. **Button** wird rekursiv layoutet → fällt in `_ =>` Arm von `layout_formatting_context()` → `layout_bfc()`
6. Button's Kinder werden gesammelt → Text "Increase counter"
7. Button bekommt `inline_layout_result` mit dem Text ✓

### Rendering-Phase (display_list.rs)

1. `generate_for_stacking_context()` startet bei Body
2. `paint_in_flow_descendants()` für Body's Kinder
3. Anonymous IFC Wrapper → `paint_node_content()` 
4. IFC hat `inline_layout_result` → `paint_inline_content()`
5. Text "5" → `push_text_run()` ✓
6. Button (Shape) → `paint_inline_object()` → `paint_inline_shape()`
7. **❌ BUG**: `paint_inline_shape()` rendert nur Hintergrund/Border
8. Button's `inline_layout_result` wird NIE gerendert!

### Warum `paint_node_content()` den Button nicht erreicht

Der Button wird nicht über den normalen `paint_in_flow_descendants()` Pfad erreicht, weil:
- Er ist ein Kind des Anonymous IFC Wrappers im Layout-Tree
- Er wird als `InlineContent::Shape` im IFC behandelt
- `paint_inline_content()` → `paint_inline_object()` rendert ihn als "Objekt" im IFC
- Der normale rekursive Pfad (`paint_in_flow_descendants` → `paint_node_content`) wird nie aufgerufen

## Koordinatensystem-Übersicht

```
┌─────────────────────────────────────────┐
│ Window (Viewport)                       │
│                                         │
│  ┌───────────────────────────────────┐  │
│  │ Body (BFC)                        │  │
│  │ Position: (8, 8) (8px margin)     │  │
│  │                                   │  │
│  │  ┌─────────────────────────────┐  │  │
│  │  │ Anonymous IFC Wrapper       │  │  │
│  │  │ (inline_layout_result)      │  │  │
│  │  │                             │  │  │
│  │  │  "5"  [Button box]          │  │  │
│  │  │   ✓     ✓ bg/border         │  │  │
│  │  │         ❌ text fehlt        │  │  │
│  │  │                             │  │  │
│  │  └─────────────────────────────┘  │  │
│  │                                   │  │
│  └───────────────────────────────────┘  │
│                                         │
└─────────────────────────────────────────┘
```

### Button-Struktur im Detail

```
Button (Layout-Node, FC: InlineBlock)
├── dom_node_id: NodeId für <button>
├── used_size: (width, height) nach Layout
├── inline_layout_result: ✓ VORHANDEN
│   └── layout: UnifiedLayout
│       └── items: [PositionedItem für "Increase counter"]
└── children: [text_node_index]
```

## Wichtige Dateien

| Datei | Zweck |
|-------|-------|
| [layout/src/solver3/display_list.rs](../layout/src/solver3/display_list.rs) | Display-Liste generieren - **BUG hier** |
| [layout/src/solver3/fc.rs](../layout/src/solver3/fc.rs) | BFC/IFC Layout-Logik |
| [core/src/ua_css.rs](../core/src/ua_css.rs) | User-Agent CSS Defaults |
| [layout/src/widgets/button.rs](../layout/src/widgets/button.rs) | Button-Widget mit Styles |

## Nächste Schritte

1. **Fix display_list.rs** - `paint_inline_object` muss Inhalt rendern
2. **Test** - Rebuild und verifizieren, dass Button-Text erscheint
3. **Optional**: Entferne doppelte Button-Definition in ua_css.rs
4. **Falls Offset-Problem weiterhin besteht**: Prüfe Koordinaten-Berechnung
