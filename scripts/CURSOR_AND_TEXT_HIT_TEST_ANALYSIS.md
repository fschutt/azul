# Cursor und Text Hit-Test Architektur-Analyse

**Datum**: 19. Januar 2026  
**Status**: Verifizierung der Gemini-Analyse vor Implementation

## 1. Zusammenfassung des Problems

### Aktuelle Symptome
1. `cursor:pointer` auf Button funktioniert nicht korrekt
2. I-Beam-Cursor erscheint auf gesamtem Body statt nur über Text
3. Text-Drag-Selection funktioniert nicht zuverlässig

### Grundursache
Text-Nodes (`NodeType::Text`) erzeugen **keine eigenen Hit-Test-Bereiche** im WebRender-Display-List. 
Wenn der Mauszeiger über Text ist, gibt der Hit-Test den **Container** (Body, Button, Div) zurück, nicht den Text selbst.

---

## 2. Aktuelle Architektur

### 2.1 Tag-Namespace-System

```
+----------+------------------------------------+---------------------------+
| Marker   | Zweck                              | Trigger Re-Render?        |
+----------+------------------------------------+---------------------------+
| 0x0100   | DOM Node (Callbacks, Focus, Hover) | Ja                        |
| 0x0200   | Scrollbar-Komponenten              | Nein (nur Scroll-Update)  |
| 0x0300   | Selection (Text-Auswahl)           | Ja                        |
| 0x0400   | Cursor (Cursor-Icon)               | Nein                      |
| 0x0500   | Reserviert                         | -                         |
+----------+------------------------------------+---------------------------+
```

**Definiert in**: [core/src/hit_test_tag.rs](../core/src/hit_test_tag.rs#L46-L60)

### 2.2 Text-Layout-Pipeline

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           TEXT LAYOUT PIPELINE                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  [1] DOM Traversal (fc.rs)                                                  │
│      ├── NodeType::Text("Hello")                                            │
│      └── Erstellt: StyledRun { text, style, logical_start_byte }            │
│                    ❌ FEHLT: source_node_id                                  │
│                                                                              │
│  [2] InlineContent Collection (fc.rs:4775-4805)                             │
│      └── InlineContent::Text(StyledRun)                                     │
│          ❌ NodeId geht hier verloren!                                       │
│                                                                              │
│  [3] Text Shaping (cache.rs)                                                │
│      ├── StyledRun → VisualRun                                              │
│      ├── VisualRun → ShapedCluster                                          │
│      └── ShapedCluster enthält Glyphen-Positionen                           │
│          ❌ Keine Rückverfolgbarkeit zum ursprünglichen NodeId               │
│                                                                              │
│  [4] Glyph Runs (glyphs.rs)                                                 │
│      └── SimpleGlyphRun { glyphs, font_hash, color... }                     │
│          ❌ Keine NodeId-Information                                         │
│                                                                              │
│  [5] Display List (display_list.rs)                                         │
│      └── DisplayListItem::Text { glyphs, font_hash, color, clip_rect }      │
│          ❌ Kein HitTestArea für Text-Runs                                   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.3 Betroffene Dateien und Strukturen

| Datei | Struktur | Problem |
|-------|----------|---------|
| [layout/src/text3/cache.rs#L811](../layout/src/text3/cache.rs#L811) | `StyledRun` | Fehlt `source_node_id: NodeId` |
| [layout/src/solver3/fc.rs#L4775](../layout/src/solver3/fc.rs#L4775) | `collect_inline_content()` | NodeId wird nicht propagiert |
| [layout/src/text3/glyphs.rs#L36](../layout/src/text3/glyphs.rs#L36) | `SimpleGlyphRun` | Fehlt NodeId |
| [layout/src/solver3/display_list.rs](../layout/src/solver3/display_list.rs) | `build_display_list()` | Kein HitTestArea für Text |
| [layout/src/hit_test.rs#L70](../layout/src/hit_test.rs#L70) | `CursorTypeHitTest::new()` | Hat Text-Child-Hack |
| [core/src/prop_cache.rs#L909](../core/src/prop_cache.rs#L909) | `restyle()` | Gibt Tags an Container statt Text |

---

## 3. Aktuelle Hacks und Workarounds

### 3.1 Text-Child-Detection Hack (hit_test.rs)

**Ort**: [layout/src/hit_test.rs#L87-L118](../layout/src/hit_test.rs#L87-L118)

```rust
// Wenn Container keinen expliziten Cursor hat, prüfe Text-Kinder
let hier = &node_hierarchy[*node_id];
if let Some(first_child) = hier.first_child_id(*node_id) {
    let mut child_id = Some(first_child);
    while let Some(cid) = child_id {
        let child_data = &node_data_container[cid];
        if matches!(child_data.get_node_type(), NodeType::Text(_)) {
            // Found a text child - check its cursor property
            let child_cursor = styled_dom.get_css_property_cache().get_cursor(...);
            if let Some(child_cursor_prop) = child_cursor {
                cursor_icon = translate_cursor(css_cursor);
                break;
            }
        }
        child_id = node_hierarchy[cid].next_sibling_id();
    }
}
```

**Problem**: Dieser Hack prüft **alle Kinder** des Containers, nicht nur den Bereich unter dem Mauszeiger. Wenn Body einen Text-Child hat, zeigt der gesamte Body den I-Beam.

### 3.2 Selectable-Text Tag Assignment (prop_cache.rs)

**Ort**: [core/src/prop_cache.rs#L909-L947](../core/src/prop_cache.rs#L909-L947)

```rust
let node_has_selectable_text = {
    // Check if this node has immediate text children
    let has_text_children = { /* ... prüft ob Container Text-Kinder hat */ };
    
    if has_text_children {
        // Check user-select property on this container
        let user_select = self.get_user_select(&node_data, &node_id, &default_node_state)...;
        !matches!(user_select, StyleUserSelect::None)
    } else {
        false
    }
};

if node_has_selectable_text {
    node_should_have_tag = true;  // Container bekommt Tag, nicht der Text
}
```

**Problem**: Der **Container** bekommt den Hit-Test-Tag, nicht die einzelnen Text-Nodes. Das verhindert präzises Hit-Testing auf Text-Ebene.

### 3.3 UA CSS für Text Nodes (ua_css.rs)

**Ort**: [core/src/ua_css.rs](../core/src/ua_css.rs)

```css
/* Impliziter Cursor für Text */
Text { cursor: text; }
```

**Problem**: Diese Regel ist korrekt, aber nutzlos weil Text-Nodes keinen Hit-Test-Bereich haben.

---

## 4. Gemini-Analyse Verifizierung

### 4.1 Behauptung: StyledRun fehlt source_node_id

**Status**: ✅ BESTÄTIGT

Aus [layout/src/text3/cache.rs#L811-L817](../layout/src/text3/cache.rs#L811-L817):
```rust
#[derive(Debug, Clone, Hash)]
pub struct StyledRun {
    pub text: String,
    pub style: Arc<StyleProperties>,
    pub logical_start_byte: usize,
    // ❌ KEIN source_node_id Feld!
}
```

### 4.2 Behauptung: InlineContent::Text verliert NodeId

**Status**: ✅ BESTÄTIGT

Aus [layout/src/solver3/fc.rs#L4800](../layout/src/solver3/fc.rs#L4800):
```rust
content.push(InlineContent::Text(StyledRun {
    text: text_content.to_string(),
    style: Arc::new(get_style_properties(ctx.styled_dom, dom_id)),
    logical_start_byte: 0,
    // ❌ dom_id wird NICHT in StyledRun gespeichert!
}));
```

### 4.3 Behauptung: SimpleGlyphRun fehlt NodeId

**Status**: ✅ BESTÄTIGT

Aus [layout/src/text3/glyphs.rs#L32-L52](../layout/src/text3/glyphs.rs#L32-L52):
```rust
pub struct SimpleGlyphRun {
    pub glyphs: Vec<GlyphInstance>,
    pub color: ColorU,
    pub background_color: Option<ColorU>,
    pub font_hash: u64,
    pub font_size_px: f32,
    // ❌ KEIN source_node_id Feld!
}
```

### 4.4 Behauptung: Display List generiert kein HitTestArea für Text

**Status**: ✅ BESTÄTIGT

Die `DisplayListItem` enum hat keinen Variant für Text-Hit-Testing. Text wird nur zum Rendering ausgegeben, ohne Hit-Test-Informationen.

---

## 5. Sauberer Architektur-Fix

### Phase 1: NodeId durch Text-Pipeline propagieren

#### 5.1 StyledRun erweitern

**Datei**: [layout/src/text3/cache.rs#L811](../layout/src/text3/cache.rs#L811)

```rust
#[derive(Debug, Clone, Hash)]
pub struct StyledRun {
    pub text: String,
    pub style: Arc<StyleProperties>,
    pub logical_start_byte: usize,
    // NEU: Optional weil nicht alle Runs aus DOM kommen (z.B. list markers)
    pub source_node_id: Option<NodeId>,
}
```

#### 5.2 InlineContent Collection updaten

**Datei**: [layout/src/solver3/fc.rs#L4800](../layout/src/solver3/fc.rs#L4800)

```rust
content.push(InlineContent::Text(StyledRun {
    text: text_content.to_string(),
    style: Arc::new(get_style_properties(ctx.styled_dom, dom_id)),
    logical_start_byte: 0,
    source_node_id: Some(dom_id),  // NEU
}));
```

#### 5.3 SimpleGlyphRun erweitern

**Datei**: [layout/src/text3/glyphs.rs#L32](../layout/src/text3/glyphs.rs#L32)

```rust
pub struct SimpleGlyphRun {
    pub glyphs: Vec<GlyphInstance>,
    pub color: ColorU,
    // ... existing fields ...
    pub source_node_id: Option<NodeId>,  // NEU
}
```

### Phase 2: Hit-Test-Bereiche für Text generieren

#### 5.4 Display List Builder erweitern

**Datei**: [layout/src/solver3/display_list.rs](../layout/src/solver3/display_list.rs)

Neue `DisplayListItem` Variante:
```rust
pub enum DisplayListItem {
    // ... existing variants ...
    
    /// Hit-test area for text selection and cursor resolution
    TextHitArea {
        bounds: LogicalRect,
        dom_id: DomId,
        node_id: NodeId,
        text_run_index: u16,
    },
}
```

### Phase 3: Text-Child-Detection Hack entfernen

#### 5.5 CursorTypeHitTest vereinfachen

**Datei**: [layout/src/hit_test.rs#L70](../layout/src/hit_test.rs#L70)

```rust
impl CursorTypeHitTest {
    pub fn new(hit_test: &FullHitTest, layout_window: &LayoutWindow) -> Self {
        // 1. Suche zuerst in TAG_TYPE_CURSOR namespace (direkte Text-Hits)
        // 2. Falls nicht gefunden, suche in TAG_TYPE_DOM_NODE (Container mit cursor property)
        // 3. Kein Text-Child-Detection mehr nötig!
    }
}
```

### Phase 4: Tag Assignment korrigieren

#### 5.6 Text Nodes direkt taggen

**Datei**: [core/src/prop_cache.rs#L909](../core/src/prop_cache.rs#L909)

```rust
// Statt Container mit Text-Kindern zu taggen:
// ❌ if node_has_selectable_text { node_should_have_tag = true; }

// Direkt prüfen ob dieser Node ein Text ist:
// ✅ if matches!(node_data.get_node_type(), NodeType::Text(_)) {
//        node_should_have_tag = true;
//    }
```

---

## 6. Implementierungsreihenfolge

| # | Datei | Änderung | Risiko |
|---|-------|----------|--------|
| 1 | `layout/src/text3/cache.rs` | `source_node_id` zu StyledRun hinzufügen | Niedrig |
| 2 | `layout/src/solver3/fc.rs` | NodeId beim Erstellen von StyledRun übergeben | Niedrig |
| 3 | `layout/src/text3/glyphs.rs` | `source_node_id` zu SimpleGlyphRun hinzufügen | Niedrig |
| 4 | `layout/src/solver3/display_list.rs` | TextHitArea Item generieren | Mittel |
| 5 | `layout/src/hit_test.rs` | Text-Child-Hack entfernen | Hoch |
| 6 | `core/src/prop_cache.rs` | Tag Assignment korrigieren | Hoch |

---

## 7. Risiken und Mitigation

### 7.1 Breaking Changes

- **SimpleGlyphRun Struct-Erweiterung**: Alle Stellen die diese Struktur konstruieren müssen angepasst werden
- **StyledRun Hash-Änderung**: Cache-Invalidierung könnte Performance beeinflussen

### 7.2 Regressionspotential

- Scrollbars könnten beeinträchtigt werden wenn Hit-Test-Order sich ändert
- Bestehende Callbacks könnten nicht mehr ausgelöst werden wenn Tag Assignment sich ändert

### 7.3 Empfohlene Test-Strategie

1. Vor jedem Commit: `cargo test` in allen Crates
2. Visueller Test: hello-world Beispiel mit Button und Text
3. Interaktions-Test: Button-Click, Text-Selection, Scrolling

---

## 8. Fazit

Die Gemini-Analyse ist **korrekt**. Das Kernproblem ist:

> **Text-Nodes (`InlineContent::Text`) erzeugen keine Hit-Test-Bereiche im WebRender Display-List.**

Die existierenden "Hacks" (Text-Child-Detection in hit_test.rs, selectable-text in prop_cache.rs) sind Symptom-Behandlungen, die das Grundproblem nicht lösen.

### Empfohlener Ansatz

1. **Schrittweise Implementation**: Jede Phase einzeln committen und testen
2. **Rückwärtskompatibilität**: Neue Felder als `Option<T>` hinzufügen
3. **Feature-Flag**: Neues Verhalten hinter `#[cfg(feature = "text_hittest")]` verstecken bis stabil

---

## Anhang: Relevante Code-Stellen

### A. StyledRun Definition
```rust
// layout/src/text3/cache.rs:811
pub struct StyledRun {
    pub text: String,
    pub style: Arc<StyleProperties>,
    pub logical_start_byte: usize,
}
```

### B. InlineContent Collection
```rust
// layout/src/solver3/fc.rs:4800
content.push(InlineContent::Text(StyledRun {
    text: text_content.to_string(),
    style: Arc::new(get_style_properties(ctx.styled_dom, dom_id)),
    logical_start_byte: 0,
}));
```

### C. SimpleGlyphRun Definition
```rust
// layout/src/text3/glyphs.rs:32
pub struct SimpleGlyphRun {
    pub glyphs: Vec<GlyphInstance>,
    pub color: ColorU,
    pub background_color: Option<ColorU>,
    pub font_hash: u64,
    pub font_size_px: f32,
    pub text_decoration: TextDecoration,
    pub is_ime_preview: bool,
}
```

### D. Text-Child-Detection Hack
```rust
// layout/src/hit_test.rs:87-118
// Wenn Container keinen expliziten Cursor hat, prüfe Text-Kinder
let hier = &node_hierarchy[*node_id];
if let Some(first_child) = hier.first_child_id(*node_id) {
    // ... iteriert durch alle Kinder und prüft auf Text
}
```

### E. Selectable-Text Tag Assignment
```rust
// core/src/prop_cache.rs:909-947
let node_has_selectable_text = {
    let has_text_children = { /* ... */ };
    if has_text_children { /* ... */ }
};
if node_has_selectable_text {
    node_should_have_tag = true;
}
```
