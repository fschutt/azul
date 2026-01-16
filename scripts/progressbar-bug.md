## Zusammenfassung: ProgressBar Width Bug

### Problem
Der ProgressBar-Container (Node 6) zeigte eine Breite von **8px** statt der erwarteten **~544px** (Elternbreite minus Padding). Die grüne Fortschrittsanzeige wurde dadurch falsch dargestellt.

### Ursache gefunden
Der Bug war in window.rs:

```rust
// VORHER (falsch):
pub fn get_node_size(&self, node_id: DomNodeId) -> Option<LogicalSize> {
    let layout_result = self.layout_results.get(&node_id.dom)?;
    let nid = node_id.node.into_crate_internal()?;
    let layout_node = layout_result.layout_tree.get(nid.index())?;  // ❌ DOM-Index als Layout-Index
    layout_node.used_size
}
```

Das Problem: **DOM-Indices ≠ Layout-Tree-Indices**. Der Layout-Tree hat ein eigenes Indexing-System (wegen anonymer Boxen, Pseudo-Elemente, etc.), aber `get_node_size()` und `get_node_position()` verwendeten den DOM-Index direkt als Layout-Tree-Index.

### Fix angewendet
Die Funktionen verwenden jetzt `dom_to_layout` Mapping:

```rust
// NACHHER (korrekt):
pub fn get_node_size(&self, node_id: DomNodeId) -> Option<LogicalSize> {
    let layout_result = self.layout_results.get(&node_id.dom)?;
    let nid = node_id.node.into_crate_internal()?;
    let layout_indices = layout_result.layout_tree.dom_to_layout.get(&nid)?;  // ✅ Mapping verwenden
    let layout_index = *layout_indices.first()?;
    let layout_node = layout_result.layout_tree.get(layout_index)?;
    layout_node.used_size
}
```

### Ergebnis
| Node | Vorher | Nachher |
|------|--------|---------|
| Node 6 (Container) | width=8.0 | **width=544.0** ✅ |
| Node 7 (Bar) | width=135.5 | width=135.5 |
| Node 8 (Placeholder) | width=0.0 | width=0.0 |

### Geänderte Dateien
1. **window.rs** - `get_node_size()` und `get_node_position()` korrigiert
2. **progressbar.rs** - `position: relative` zum Container hinzugefügt (für absolute Positionierung der Kinder)

### Status
- ✅ Build erfolgreich
- ✅ Layout-Werte korrekt (via Debug API verifiziert)
- ✅ Debug-Logging entfernt
- ⏳ Visueller Test ausstehend (Widget-Fenster konnte nicht gestartet werden wegen Port-Konflikten)
