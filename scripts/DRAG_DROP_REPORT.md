# Drag & Drop Implementation Report

## Übersicht

Drei fehlende Kernbereiche für vollständiges HTML5-kompatibles Drag & Drop in azul:

1. **Visuelles Drag-Feedback** — GPU-Transforms für "schwebende" Nodes
2. **Drop-Zone-Filterung** — MIME-basiertes `accept` + Cursor-Feedback
3. **CSS Drag-Pseudo-Klassen** — Styling während Drag-Operationen

---

## 1. Visuelles Drag-Feedback (GPU-Transforms)

### Was HTML tut

Wenn ein `draggable="true"` Element in einem Browser gezogen wird:

1. **DragStart**: Browser erzeugt automatisch ein halbtransparentes "Geisterbild"
   (Bitmap-Snapshot des Elements) als Drag-Feedback
2. **Während Drag**: Das Geisterbild folgt dem Maus-Cursor mit einem Offset
   (die Position, an der der User innerhalb des Elements geklickt hat)
3. **Optionales `setDragImage()`**: Der Entwickler kann ein eigenes Bild setzen
4. Das **Original-Element bleibt an Ort und Stelle** (optional gestylt mit
   reduzierter Opacity via CSS-Klasse)

### Was azul aktuell hat

- `NodeDrag` Struct in [core/src/drag.rs](core/src/drag.rs):
  ```
  dom_id, node_id, start_position, current_position,
  current_drop_target, drag_data
  ```
- **KEIN** `drag_offset` (Click-Position relativ zum Node)
- **KEIN** GPU-Transform-Update für den gedraggten Node
- `gpu_state.rs` behandelt **nur** Scrollbar-Thumb-Transforms
- Kein `setDragImage` / `drag_bitmap` Konzept

### Implementierungsplan

#### Option A: CSS-Transform-basierter Ansatz (empfohlen für azul)

Da azul kein Compositor-basiertes Geisterbild wie Browser hat, stattdessen:

1. **Neues Feld `drag_offset: LogicalPosition` in `NodeDrag`**
   - Berechnet bei DragStart: `click_position - node_top_left`
   - Ermöglicht korrekte Positionierung relativ zum Cursor

2. **Neues Feld `visual_transform: Option<ComputedTransform3D>` in `NodeDrag`**

3. **GPU-Transform-Update in `gpu_state.rs`**:
   ```rust
   fn update_node_drag_transform(
       gpu_cache: &mut GpuValueCache,
       changes: &mut GpuEventChanges,
       node_drag: &NodeDrag,
       node_layout_position: LogicalPosition,
   ) {
       let delta_x = node_drag.current_position.x
                    - node_drag.start_position.x;
       let delta_y = node_drag.current_position.y
                    - node_drag.start_position.y;
       let transform = ComputedTransform3D::translate(delta_x, delta_y, 0.0);
       update_transform_key(gpu_cache, changes, dom_id, node_id, transform);
   }
   ```

4. **Opacity-Änderung**: Gedraggter Node bekommt z.B. `opacity: 0.6` via
   GPU-Property-Update (nicht CSS, sondern direkte GPU-Werte)

5. **Z-Index**: Gedraggter Node wird auf höchsten Z-Index gesetzt

6. **Bei DragEnd**: Transform auf `identity()` zurücksetzen, Opacity
   wiederherstellen

#### Option B: Geisterbild-Ansatz (wie Browser)

Alternativ: Bei DragStart den Node als Bitmap rendern und als Overlay zeichnen.
Komplexer, aber näher am Browser-Verhalten.

**Empfehlung: Option A** — einfacher, nutzt bestehendes GPU-Transform-System,
funktioniert für Kanban-Boards wo der User das Element direkt "anfasst".

---

## 2. Drop-Zone-Filterung

### Was HTML tut

HTML5 DnD hat ein raffiniertes Filterungssystem:

#### DragStart (Quelle):
```javascript
event.dataTransfer.setData("text/plain", "Hello");
event.dataTransfer.setData("application/x-kanban-task", taskId);
event.dataTransfer.effectAllowed = "move"; // copy|move|link|copyMove|...
```

#### DragOver (Ziel) — Entscheidet ob Drop erlaubt ist:
```javascript
target.addEventListener("dragover", (event) => {
    // MIME-Type-Prüfung
    if (event.dataTransfer.types.includes("application/x-kanban-task")) {
        event.preventDefault(); // ← DAS macht es zum gültigen Drop-Target
        event.dataTransfer.dropEffect = "move";
    }
    // KEIN preventDefault() = Drop nicht erlaubt = "Verbotszeichen"-Cursor
});
```

#### Zusammenfassung des HTML5-Modells:
| Konzept | Verantwortlich | Wann |
|---------|---------------|------|
| `dataTransfer.setData(type, data)` | Quell-Node | `dragstart` |
| `dataTransfer.effectAllowed` | Quell-Node | `dragstart` |
| `event.dataTransfer.types` | Browser | `dragenter`/`dragover` (readonly) |
| `event.preventDefault()` | Ziel-Node | `dragover` → erlaubt Drop |
| `event.dataTransfer.dropEffect` | Ziel-Node | `dragover` |
| `event.dataTransfer.getData(type)` | Ziel-Node | `drop` (nur hier lesbar!) |

**Wichtig**: Während `dragover` sind die Daten aus Sicherheitsgründen im
"Protected Mode" — nur die MIME-Types (`.types`) sind sichtbar, nicht die
eigentlichen Daten. Erst im `drop`-Event kann `getData()` aufgerufen werden.

#### Cursor-Feedback:
- `dropEffect = "copy"` → Cursor mit Plus-Zeichen
- `dropEffect = "move"` → Normaler Drag-Cursor
- `dropEffect = "link"` → Cursor mit Link-Symbol
- `dropEffect = "none"` → 🚫 Verbotszeichen-Cursor (kein Drop möglich)

### Was azul aktuell hat

- `DragData` Struct hat `BTreeMap<AzString, Vec<u8>>` → ✅ MIME→Daten
- `DragEffect` enum (Copy, Move, Link, All, None) → ✅ effectAllowed
- `DropEffect` enum (Copy, Move, Link, None) → ✅ dropEffect
- **KEIN** `accept`-Attribut auf DOM-Nodes
- **KEINE** Filterlogik in der Event-Verarbeitung
- **DragEnter/DragOver/DragLeave/Drop Events werden NICHT generiert!**
  Sie sind nur als EventType definiert, werden auf MouseEnter/MouseLeave gemappt

### Implementierungsplan

#### Schritt 1: DragEnter / DragOver / DragLeave / Drop Events generieren

Aktuell in `process_events.rs`:
```rust
// Mapping (aktuell):
E::DragEnter => vec![EF::Hover(H::MouseEnter)],
E::DragOver  => vec![EF::Hover(H::MouseOver)],
E::DragLeave => vec![EF::Hover(H::MouseLeave)],
E::Drop      => vec![EF::Hover(H::DroppedFile)],
```

**Problem**: Diese Events werden NIE eigenständig generiert. Sie müssen in
der Event-Loop erkannt werden: Wenn ein `NodeDrag` aktiv ist UND die Maus
über einen neuen Node fährt, MUSS:
- `DragEnter` auf dem neuen Node gefeuert werden
- `DragLeave` auf dem vorherigen Node gefeuert werden
- `DragOver` alle ~350ms auf dem aktuellen Node gefeuert werden
- `Drop` beim Loslassen der Maus auf dem aktuellen Node gefeuert werden

#### Schritt 2: DataTransfer-Konzept für Callbacks

Neue Methoden auf `CallbackInfo`:
```rust
impl CallbackInfo {
    /// Verfügbare MIME-Types des aktiven Drags (im Protected Mode)
    /// Nutzbar in DragEnter/DragOver/DragLeave
    fn get_drag_types(&self) -> Vec<AzString>;

    /// Tatsächliche Daten lesen (nur im Drop-Event!)
    fn get_drag_data(&self, mime_type: &str) -> Option<Vec<u8>>;

    /// effectAllowed der Quelle
    fn get_drag_effect_allowed(&self) -> DragEffect;

    /// dropEffect setzen (in DragOver)
    fn set_drop_effect(&mut self, effect: DropEffect);

    /// Drop akzeptieren (= preventDefault() in HTML)
    fn accept_drop(&mut self);
}
```

#### Schritt 3: Accept-Attribut (optional, vereinfacht HTML5)

Azul-spezifische Vereinfachung (HTML5 hat kein explizites `accept`-Attribut
für Drop-Zones, stattdessen passiert die Filterung im JavaScript):

```rust
// Auf DOM-Node-Ebene:
AzDom_withDropZone(dom, AzStringVec::from(&["text/plain", "application/x-task"]));
```

Alternativ: Kein `accept`-Attribut, stattdessen muss der Callback selbst
`accept_drop()` aufrufen (genau wie HTML5 mit `preventDefault()`).

**Empfehlung**: HTML5-Modell folgen — kein `accept`-Attribut, sondern:
- Callback prüft `get_drag_types()` auf gewünschte MIME-Types
- Ruft `accept_drop()` auf wenn passend
- Wenn KEIN Callback `accept_drop()` ruft → automatisch `dropEffect = "none"`
  → Verbotszeichen-Cursor

#### Schritt 4: Cursor-Management

In `sync_window_state` oder direkt im Event-Processing:
```rust
match current_drop_effect {
    DropEffect::None => set_cursor(CursorIcon::NoDrop),
    DropEffect::Copy => set_cursor(CursorIcon::Copy),
    DropEffect::Move => set_cursor(CursorIcon::Grabbing),
    DropEffect::Link => set_cursor(CursorIcon::Alias),
}
```

---

## 3. CSS Drag-Pseudo-Klassen

### Was CSS aktuell bietet (Browser)

**Es gibt KEINE standardisierten CSS-Pseudo-Klassen für Drag & Drop!**

Das ist ein wichtiger Punkt: Auch in echtem CSS/HTML gibt es:

- ❌ Kein `:drag` (war als Proposal in CSS Selectors 4, wurde entfernt)
- ❌ Kein `:drop()` (war in CSS Selectors 4 Draft, nie implementiert)
- ❌ Kein `:drag-over`

Stattdessen verwenden **alle** realen Implementierungen **JavaScript-basierte
Klassen-Toggle**:

```javascript
// DragStart: Quell-Element stylen
source.addEventListener("dragstart", (e) => {
    e.target.classList.add("dragging");    // → opacity: 0.5
});
source.addEventListener("dragend", (e) => {
    e.target.classList.remove("dragging");
});

// DragEnter/DragLeave: Drop-Zone stylen
target.addEventListener("dragenter", (e) => {
    e.target.classList.add("drag-over");   // → border: 2px solid blue
});
target.addEventListener("dragleave", (e) => {
    e.target.classList.remove("drag-over");
});
target.addEventListener("drop", (e) => {
    e.target.classList.remove("drag-over");
});
```

Typische CSS-Klassen:
```css
.dragging { opacity: 0.5; }
.drag-over { border: 2px solid #3b82f6; background: rgba(59,130,246,0.1); }
.drag-over.invalid { border-color: red; }
```

### Was azul bieten sollte

Da azul kein dynamisches `classList.add()` hat wie Browser, gibt es zwei Optionen:

#### Option A: Automatische CSS-Pseudo-Klassen (azul-spezifisch, empfohlen)

Azul fügt **eigene** Pseudo-Klassen hinzu, die automatisch gesetzt werden:

| Pseudo-Klasse | Wann aktiv | Auf welchem Node |
|---|---|---|
| `:dragging` | Zwischen DragStart und DragEnd | Quell-Node |
| `:drag-over` | Während DragOver, wenn Drop erlaubt | Ziel-Node |
| `:drag-over-invalid` | Während DragOver, wenn Drop NICHT erlaubt | Ziel-Node |

```css
/* Quell-Element während Drag */
.task:dragging {
    opacity: 0.4;
    transform: scale(1.05);
    box-shadow: 0 4px 12px rgba(0,0,0,0.3);
}

/* Gültige Drop-Zone */
.column:drag-over {
    border: 2px solid #3b82f6;
    background: rgba(59,130,246,0.1);
}

/* Ungültige Drop-Zone (MIME mismatch) */
.column:drag-over-invalid {
    border: 2px solid #ef4444;
    background: rgba(239,68,68,0.05);
}
```

**Implementierung**:

1. Neues `PseudoStateType` Variant:
   ```rust
   pub enum PseudoStateType {
       Normal, Hover, Active, Focus, Disabled, Checked,
       FocusWithin, Visited, Backdrop,
       Dragging,         // NEU
       DragOver,         // NEU
       DragOverInvalid,  // NEU
   }
   ```

2. In CSS-Parser (`css/src/parser.rs`):
   ```rust
   "dragging" => PseudoStateType::Dragging,
   "drag-over" => PseudoStateType::DragOver,
   "drag-over-invalid" => PseudoStateType::DragOverInvalid,
   ```

3. In Pseudo-State-Berechnung (pro Frame):
   - Wenn `NodeDrag` aktiv: `source_node` bekommt `:dragging`
   - Wenn Cursor über Node X und Drag aktiv:
     - Wenn `accept_drop()` gecallt wurde → `:drag-over`
     - Wenn nicht → `:drag-over-invalid`

4. **Pseudo-State-Änderung triggert CSS-Recalc** → Styles aktualisieren
   sich automatisch, kein manuelles `classList`-Toggling nötig.

#### Option B: Callback-basiert (wie HTML5)

User muss im Callback selbst Klassen setzen:
```c
AzUpdate on_drag_enter(AzRefAny data, AzCallbackInfo info) {
    AzCallbackInfo_addCssClass(&info, hit_node, "drag-over");
    return AzUpdate_RefreshDom;
}
```

**Empfehlung: Option A** — azul kann hier besser sein als HTML, weil die
Pseudo-Klassen automatisch und ohne Boilerplate funktionieren. HTML macht es
so umständlich weil CSS die Pseudo-Klassen nie standardisiert hat.

---

## 4. Fehlende Event-Generierung (Kritischstes Problem)

### Aktueller Zustand

DragEnter, DragOver, DragLeave, Drop Events werden **NIE** generiert.
Sie sind als `EventType` definiert und auf Hover-Events gemappt, aber die
Event-Loop erzeugt sie nicht.

### HTML5-Lifecycle der Drop-Target-Events

```
User bewegt Maus über Element X (während Drag aktiv):
  1. DragEnter auf X       (einmal, beim Eintreten)
  2. DragOver auf X        (alle ~350ms, wiederholt)
  3. DragLeave von X       (einmal, beim Verlassen)

Wenn User Maus loslässt über X:
  4. Drop auf X            (einmal, Daten lesbar)

Danach immer:
  5. DragEnd auf Quell-Node (einmal, mit dropEffect-Info)
```

### Implementierungsplan

In `process_events.rs`, nach der Hit-Test-Berechnung:

```rust
// Pseudocode:
if let Some(node_drag) = active_node_drag {
    let hovered_node = hit_test.get_deepest_node_at(cursor_pos);

    // DragEnter / DragLeave
    if hovered_node != node_drag.previous_hover_target {
        if let Some(prev) = node_drag.previous_hover_target {
            fire_event(DragLeave, prev);
        }
        if let Some(curr) = hovered_node {
            fire_event(DragEnter, curr);
        }
        node_drag.previous_hover_target = hovered_node;
    }

    // DragOver (throttled, ~350ms)
    if node_drag.last_drag_over_time.elapsed() > Duration::from_millis(350) {
        if let Some(curr) = hovered_node {
            fire_event(DragOver, curr);
        }
        node_drag.last_drag_over_time = Instant::now();
    }

    // Drop (auf Maus-Release)
    if mouse_released {
        if let Some(curr) = hovered_node {
            fire_event(Drop, curr);
        }
        fire_event(DragEnd, source_node);
    }
}
```

---

## 5. Implementierungsreihenfolge (Empfehlung)

| Schritt | Aufwand | Prio | Beschreibung |
|---------|---------|------|-------------|
| 1 | Mittel | 🔴 | **DragEnter/DragOver/DragLeave/Drop Events generieren** — ohne diese Events funktioniert nichts |
| 2 | Klein | 🔴 | **`drag_offset` in NodeDrag** — Click-Position relativ zum Node |
| 3 | Mittel | 🔴 | **GPU-Transform für gedraggten Node** — translate(dx, dy) |
| 4 | Klein | 🟡 | **Opacity-Änderung** für gedraggten Node |
| 5 | Mittel | 🟡 | **DataTransfer-API** auf CallbackInfo (get_drag_types, get_drag_data, accept_drop) |
| 6 | Mittel | 🟡 | **Drop-Zone-Validierung** mit Cursor-Feedback (NoDrop vs. Grabbing) |
| 7 | Mittel | 🟢 | **CSS Pseudo-Klassen** (:dragging, :drag-over, :drag-over-invalid) |
| 8 | Klein | 🟢 | **Z-Index-Override** für gedraggten Node |

Gesamt: ~3-5 Tage Implementierung für vollständiges Drag & Drop.

---

## 6. Kanban-Board Beispiel (Zielzustand)

```c
// DragStart: MIME-Type + Daten setzen
AzUpdate on_task_drag_start(AzRefAny data, AzCallbackInfo info) {
    AzCallbackInfo_setDragData(&info, "application/x-task", task_id_bytes, len);
    AzCallbackInfo_setDragEffectAllowed(&info, AzDragEffect_Move);
    return AzUpdate_DoNothing;
}

// DragOver auf Column: Prüfen ob Task-Type akzeptiert wird
AzUpdate on_column_drag_over(AzRefAny data, AzCallbackInfo info) {
    AzStringVec types = AzCallbackInfo_getDragTypes(&info);
    if (AzStringVec_contains(&types, "application/x-task")) {
        AzCallbackInfo_acceptDrop(&info);  // → :drag-over aktiv
        AzCallbackInfo_setDropEffect(&info, AzDropEffect_Move);
    }
    // Wenn acceptDrop() NICHT gecallt wird:
    //   → :drag-over-invalid aktiv
    //   → Cursor = NoDrop (🚫)
    return AzUpdate_DoNothing;
}

// Drop: Daten lesen + Task verschieben
AzUpdate on_column_drop(AzRefAny data, AzCallbackInfo info) {
    AzOptionU8Vec task_data = AzCallbackInfo_getDragData(&info, "application/x-task");
    if (!AzOptionU8Vec_isNone(&task_data)) {
        // Task von alter Column entfernen, in neue einfügen
        move_task(data, task_data.Some.payload);
    }
    return AzUpdate_RefreshDom;
}
```

```css
.task:dragging {
    opacity: 0.4;
    box-shadow: 0 4px 12px rgba(0,0,0,0.15);
}

.column:drag-over {
    border: 2px solid #3b82f6;
    background-color: rgba(59, 130, 246, 0.08);
}

.column:drag-over-invalid {
    border: 2px dashed #94a3b8;
}
```
