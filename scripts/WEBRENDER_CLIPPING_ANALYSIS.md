# WebRender Clipping und Scroll Frames - Technische Analyse

## Zusammenfassung

Diese Analyse untersucht wie Clipping und Scroll Frames in WebRender zusammenarbeiten, um das Problem zu diagnostizieren, warum Scroll-Container-Inhalte nicht korrekt geclippt werden.

---

## 1. Schlüssel-Datenstrukturen

### 1.1 SpatialNode und SpatialTree

Die `SpatialTree` verwaltet eine Hierarchie von räumlichen Knoten (`SpatialNode`), die Transformationen und Koordinatensysteme definieren.

**Drei Typen von SpatialNodes:**

```rust
pub enum SpatialNodeType {
    /// Sticky Positionierung (CSS position: sticky)
    StickyFrame(StickyFrameInfo),
    
    /// Scroll Frame - Transformiert Inhalt, aber clippt NICHT automatisch!
    ScrollFrame(ScrollFrameInfo),
    
    /// Referenz Frame - etabliert ein neues Koordinatensystem
    ReferenceFrame(ReferenceFrameInfo),
}
```

**KRITISCH:** Ein `ScrollFrame` ist NUR eine **Transformation** - er definiert, wie Inhalt scrollt (Offset-Transformation), führt aber **KEIN Clipping** durch!

### 1.2 ClipNodes und ClipChains

**ClipNode:** Ein einzelner Clip (Rechteck, abgerundetes Rechteck, Image-Maske, Box-Shadow).

**ClipChain:** Eine verkettete Liste von ClipNodes, die auf Primitives angewendet werden. ClipChains können einen Parent haben und bilden so eine Hierarchie.

```rust
struct ClipChain {
    parent: Option<usize>,  // Index des Parent-ClipChain
    clips: Vec<ClipDataHandle>,  // Die Clips in dieser Chain
}
```

### 1.3 ClipTree

Der `ClipTree` wird während des Scene-Buildings aufgebaut und während des Frame-Buildings verwendet:

```rust
pub struct ClipTree {
    nodes: Vec<ClipTreeNode>,
    leaves: Vec<ClipTreeLeaf>,
    clip_root_stack: Vec<ClipNodeId>,
}

pub struct ClipTreeLeaf {
    pub node_id: ClipNodeId,
    pub local_clip_rect: LayoutRect,  // Lokaler Clip-Rect vom Primitive
}
```

---

## 2. Wie Clips auf Display Items angewendet werden

### 2.1 CommonItemProperties

Jedes Display Item hat `CommonItemProperties`:

```rust
pub struct CommonItemProperties {
    /// Bounding box für das Primitive
    pub clip_rect: LayoutRect,
    /// Clip Chain für zusätzliches Clipping
    pub clip_chain_id: ClipChainId,
    /// Das Koordinatensystem (Spatial Node)
    pub spatial_id: SpatialId,
    /// Flags (z.B. backface-visibility)
    pub flags: PrimitiveFlags,
}
```

**Der Clip wird durch ZWEI Mechanismen angewendet:**

1. **`clip_rect`:** Ein lokales Rechteck-Clip, das direkt im Vertex-Shader angewendet wird (schneller Pfad)
2. **`clip_chain_id`:** Referenz auf eine ClipChain für komplexe Clips (benötigt möglicherweise Clip-Masken)

### 2.2 Clip Processing Flow

```
Display Item kommt an
         ↓
process_common_properties() wird aufgerufen
         ↓
get_clip_node() holt ClipNodeId für clip_chain_id
         ↓
Beim Frame Building: set_active_clips() sammelt alle relevanten Clips
         ↓
build_clip_chain_instance() erstellt optimierte Clip-Instanz
         ↓
Clips werden zu local_clip_rect zusammengeführt oder als Masken gerendert
```

---

## 3. Scroll Frames und Clipping - DIE KERNPROBLEMATIK

### 3.1 Was define_scroll_frame macht

```rust
pub fn define_scroll_frame(
    &mut self,
    parent_space: SpatialId,
    external_id: ExternalScrollId,
    content_rect: LayoutRect,    // Größe des scrollbaren Inhalts
    frame_rect: LayoutRect,      // Sichtbarer Viewport (Clip-Bereich)
    external_scroll_offset: LayoutVector2D,
    ...
) -> SpatialId
```

**Was es erstellt:**
- Einen neuen `SpatialNodeIndex` in der Spatial Tree
- Speichert `viewport_rect` (frame_rect) und `scrollable_size`
- Erstellt **KEINEN automatischen Clip!**

### 3.2 DAS PROBLEM: Scroll Frame clippt NICHT automatisch

**WebRender-Design-Philosophie:**
- `SpatialId` definiert das Koordinatensystem (wo etwas ist)
- `ClipChainId` definiert das Clipping (was sichtbar ist)
- Diese sind **GETRENNT** und müssen beide korrekt gesetzt werden!

**Ein ScrollFrame alleine führt KEIN Clipping durch!**

---

## 4. Die richtige Vorgehensweise für Scroll-Container

### 4.1 Korrekte Implementierung

Für einen korrekt clippenden Scroll-Container sind **DREI Schritte** notwendig:

```rust
// 1. ScrollFrame definieren (für Scroll-Transformation)
let scroll_spatial_id = builder.define_scroll_frame(
    parent_space,
    external_scroll_id,
    content_rect,      // Gesamtgröße des scrollbaren Inhalts
    frame_rect,        // Sichtbarer Bereich = Clip-Bereich
    ...
);

// 2. Clip-Rect definieren (für Clipping!)
// WICHTIG: Clip muss im PARENT-Space definiert werden, nicht im Scroll-Space!
let scroll_clip_id = builder.define_clip_rect(
    parent_space,      // <-- NICHT scroll_spatial_id!
    frame_rect,        // Der sichtbare Bereich
);

// 3. ClipChain erstellen
let scroll_clip_chain = builder.define_clip_chain(
    parent_clip_chain, // Parent Chain (oder None)
    [scroll_clip_id],  // Der Clip
);

// 4. Content mit korrektem spatial_id UND clip_chain_id pushen
let info = CommonItemProperties {
    clip_rect: ...,
    clip_chain_id: scroll_clip_chain,  // Clip Chain für Clipping
    spatial_id: scroll_spatial_id,      // Scroll Space für Transformation
    ..
};
builder.push_rect(&info, bounds, color);
```

### 4.2 Warum der Clip im Parent-Space sein muss

Der Clip-Rect repräsentiert den **stationären Viewport** - er bewegt sich nicht, wenn gescrollt wird. Der Inhalt bewegt sich (im scroll_spatial_id-Space), aber der Clip bleibt stehen.

**Falsch:**
```rust
// FALSCH! Clip scrollt mit dem Inhalt mit
let clip = builder.define_clip_rect(scroll_spatial_id, frame_rect);
```

**Richtig:**
```rust
// RICHTIG! Clip bleibt stationär im Parent-Space
let clip = builder.define_clip_rect(parent_space, frame_rect);
```

---

## 5. Koordinatensystem-Erwartungen

### 5.1 Parent-relative Koordinaten

Alle Rechtecke in WebRender sind **relativ zu ihrem Parent-Space**:

- `frame_rect` in `define_scroll_frame`: Relativ zum Parent-Space
- `content_rect` in `define_scroll_frame`: Relativ zum Parent-Space (Origin ist normalerweise gleich wie frame_rect.origin)
- `clip_rect` in `define_clip_rect`: Relativ zum angegebenen spatial_id

### 5.2 Content-Koordinaten nach Scroll

Nachdem ein ScrollFrame definiert wurde:
- Inhalt im ScrollFrame-Space wird durch den Scroll-Offset transformiert
- Der Scroll-Offset wird als **negierter Wert** gespeichert (scroll nach unten = negativer Y-Offset für Inhalt)

---

## 6. Analyse des aktuellen Azul-Codes

### 6.1 Aktueller Code in compositor2.rs (Zeilen 566-633)

```rust
DisplayListItem::PushScrollFrame { clip_bounds, content_size, scroll_id } => {
    // ... frame_rect und content_rect Setup ...
    
    let parent_space = *spatial_stack.last().unwrap();
    
    // ScrollFrame definieren
    let scroll_spatial_id = builder.define_scroll_frame(
        parent_space,
        external_scroll_id,
        content_rect,
        frame_rect,
        ...
    );
    spatial_stack.push(scroll_spatial_id);
    
    // Clip definieren - KORREKT im Parent-Space
    let scroll_clip_id = builder.define_clip_rect(parent_space, frame_rect);
    
    // ClipChain erstellen
    let scroll_clip_chain = builder.define_clip_chain(parent_clip, [scroll_clip_id]);
    clip_stack.push(scroll_clip_chain);
}
```

**Dieser Code sieht korrekt aus!** Der Clip wird im `parent_space` definiert.

### 6.2 Mögliche Problemursachen

1. **clip_rect in CommonItemProperties stimmt nicht:**
   - Der `clip_rect` in `CommonItemProperties` sollte auch begrenzt sein
   
2. **Inhalt wird vor dem ScrollFrame gepusht:**
   - Wenn Inhalte gepusht werden BEVOR der ScrollFrame aktiv ist, werden sie nicht geclippt
   
3. **content_rect ist falsch:**
   - `content_rect.origin` sollte gleich `frame_rect.origin` sein
   - `content_rect.size` sollte die tatsächliche Inhaltsgröße sein (kann größer als frame_rect.size sein)

4. **Der Clip-Stack wird nicht korrekt verwendet:**
   - Prüfen ob `clip_stack.last()` tatsächlich den ScrollFrame-Clip enthält

---

## 7. Debugging-Schritte

### 7.1 Verifizieren der ClipChain-Verwendung

Für jedes Rect/Text/etc. Item:
```rust
log_debug!("Item spatial_id={:?}, clip_chain_id={:?}, clip_stack={:?}",
    spatial_stack.last(),
    clip_stack.last(),
    clip_stack
);
```

### 7.2 WebRender Debug-Flags

```rust
// In der Render-Konfiguration
debug_flags: DebugFlags::PRIMITIVE_DBG
           | DebugFlags::CLIP_DBG
           | DebugFlags::SPATIAL_DBG
```

### 7.3 Prüfen der clip_rect Werte

Jedes `CommonItemProperties` hat einen `clip_rect`. Dieser sollte:
- Innerhalb des Viewport liegen
- Mit dem Clip-Chain konsistent sein

---

## 8. Häufige Fehler

### 8.1 Fehler: Clip im falschen Space

```rust
// FALSCH - Clip scrollt mit
let clip = builder.define_clip_rect(scroll_spatial_id, frame_rect);

// RICHTIG - Clip bleibt stationär
let clip = builder.define_clip_rect(parent_space, frame_rect);
```

### 8.2 Fehler: ClipChain nicht gepusht

```rust
// FALSCH - Clip wird nie verwendet
let clip = builder.define_clip_rect(...);
let chain = builder.define_clip_chain(...);
// Vergessen: clip_stack.push(chain);
```

### 8.3 Fehler: Falsche Content-Rect Origin

```rust
// FALSCH - Content beginnt bei (0,0) statt bei frame_rect.origin
let content_rect = LayoutRect::from_origin_and_size(
    LayoutPoint::zero(),  // <-- FALSCH!
    content_size,
);

// RICHTIG - Content Origin = Frame Origin
let content_rect = LayoutRect::from_origin_and_size(
    frame_rect.origin,    // <-- RICHTIG!
    content_size,
);
```

### 8.4 Hinweis zu clip_rect in CommonItemProperties

**Achtung:** Der `clip_rect` in `CommonItemProperties` ist NICHT der Viewport-Clip!

```rust
// Dies ist korrekt - clip_rect ist die Bounds des Primitives selbst
let info = CommonItemProperties {
    clip_rect: primitive_bounds,  // Bounds des Elements
    clip_chain_id: scroll_clip_chain,  // Hier passiert das eigentliche Clipping!
    ...
};
```

Der `clip_rect` beschreibt die lokalen Bounds des Primitives (z.B. für Gradients, die logisch unendlich sind). Das Clipping durch den Scroll-Frame geschieht über die `clip_chain_id`.

### 8.5 Fehler: ClipChain-Parent-Verkettung fehlt

```rust
// FALSCH - Neue ClipChain hat keinen Parent
let clip_chain = builder.define_clip_chain(None, [clip_id]);

// RICHTIG - ClipChain erbt vom Parent
let parent_clip = if current_clip == WrClipChainId::INVALID {
    None
} else {
    Some(current_clip)
};
let clip_chain = builder.define_clip_chain(parent_clip, [clip_id]);
```

### 8.6 Fehler: Clip Space Mismatch

Der häufigste Fehler: Clip ist im falschen Koordinatensystem definiert.

**Beispiel-Szenario:**
1. ScrollFrame ist bei Position (100, 200) mit Größe (300, 400)
2. Content ist 800px hoch (scrollbar)
3. Clip muss bei (100, 200) mit Größe (300, 400) im PARENT-Space sein

```rust
// Die Clip-Rect Koordinaten müssen absolut (im Parent-Space) sein
let clip_rect = LayoutRect::from_origin_and_size(
    LayoutPoint::new(100.0, 200.0),  // Position des ScrollFrame
    LayoutSize::new(300.0, 400.0),   // Größe des sichtbaren Bereichs
);

// Clip im Parent-Space definieren
let clip_id = builder.define_clip_rect(parent_spatial_id, clip_rect);
```

---

## 9. Debugging mit WebRender Debug Server

Falls ein Debug-Server läuft, können folgende Kommandos helfen:

```bash
# Clip-Informationen abrufen
curl -X POST http://localhost:8765/ -d '{"op": "get_logs"}' | grep -i clip

# Spatial Tree Informationen
curl -X POST http://localhost:8765/ -d '{"op": "get_logs"}' | grep -i spatial
```

---

## 10. Zusammenfassung

### Die goldene Regel für Scroll-Frames in WebRender:

1. **ScrollFrame** = Koordinatensystem-Transformation (wie Inhalt scrollt)
2. **Clip** = Sichtbarkeits-Begrenzung (was sichtbar ist)
3. Diese müssen **BEIDE** korrekt gesetzt werden
4. Der Clip muss im **Parent-Space** definiert werden (nicht im Scroll-Space)
5. Alle Inhalte im ScrollFrame müssen die korrekte `clip_chain_id` verwenden

### Debugging-Checkliste:

- [ ] Wird `define_clip_rect` im Parent-Space aufgerufen?
- [ ] Wird `define_clip_chain` aufgerufen und das Ergebnis gepusht?
- [ ] Hat jedes Display Item die korrekte `clip_chain_id`?
- [ ] Ist `content_rect.origin == frame_rect.origin`?
- [ ] Werden Inhalte NACH dem PushScrollFrame gepusht?
- [ ] Wird PopScrollFrame korrekt aufgerufen (Clip-Stack wird gepoppt)?
