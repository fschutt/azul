# Tag ID System – Vollständige Analyse & Bug Report

> Erstellt: 2026-02-12  
> Scope: `core/`, `layout/`, `dll/src/desktop/`

---

## Inhaltsverzeichnis

1. [Architektur-Überblick](#1-architektur-überblick)
2. [Tag-Typ-Namespaces](#2-tag-typ-namespaces)
3. [Datenfluss: Von Generierung bis Hit-Test-Auflösung](#3-datenfluss)
4. [Gefundene Bugs](#4-gefundene-bugs)
5. [Design-Schwächen](#5-design-schwächen)
6. [Vorschlag: TagId von StyledNode entfernen](#6-vorschlag-tagid-von-stylednode-entfernen)

---

## 1. Architektur-Überblick

### TagId-Generierung

**Datei:** `core/src/prop_cache.rs` (`CssPropertyCache::restyle()`)

```
TAG_ID: AtomicUsize (global, startet bei 1, wird nie zurückgesetzt)
  │ TagId::unique() → CAS-Loop, monoton steigend
  ▼
CssPropertyCache::restyle()
  - prüft jede Node gegen 11 Kriterien
  - erzeugt Vec<TagIdToNodeIdMapping> für qualifizierte Nodes
  ▼
StyledDom speichert doppelt:
  1. styled_nodes[node_id].tag_id = OptionTagId::Some(tag)     ← pro Node
  2. tag_ids_to_node_ids: Vec<{tag_id, node_id, tab_index}>    ← Lookup-Tabelle
```

### Kriterien für TagId-Vergabe (11 Stück)

| # | Bedingung | Zweck |
|---|-----------|-------|
| 1 | `display != None` | Grundvoraussetzung |
| 2 | Hat context_menu | Kontextmenü klickbar |
| 3 | Hat tab_index | Fokus/Tab-Navigation |
| 4 | Hat `:hover` CSS-Properties | Hover-Styling |
| 5 | Hat `:active` CSS-Properties | Active-Styling |
| 6 | Hat `:focus` CSS-Properties | Focus-Styling |
| 7 | Hat Nicht-Window-Callbacks | Event-Handling |
| 8 | Hat nicht-default `cursor:` | Cursor-Icon |
| 9 | Hat `overflow: scroll/auto` | Scroll-Events |
| 10 | Hat selektierbare Text-Kinder | Textselektion |
| 11 | Hat context_menu (Zweite Prüfung) | Kontextmenü |

### Doppelspeicherung

Die `tag_id` wird an **zwei** Orten gespeichert:

1. **`StyledNode.tag_id`** (`core/src/styled_dom.rs:321`) – wird gelesen in:
   - `layout/src/solver3/display_list.rs` → `get_tag_id()` (Zeile ~3225)
   - `layout/src/solver3/display_list.rs` → `find_styled_node_for_hit_test` (Zeile ~1655)
   
2. **`StyledDom.tag_ids_to_node_ids`** (`core/src/styled_dom.rs:764`) – wird gelesen in:
   - `dll/src/desktop/wr_translate2.rs:704` → Hit-Test-Rückauflösung (lineare Suche!)

**Nicht** gelesen in `restyle_nodes_hover`, `restyle_nodes_active`, `restyle_nodes_focus` – diese Funktionen arbeiten ausschließlich mit `StyledNodeState` und `CssPropertyCache`.

---

## 2. Tag-Typ-Namespaces

**Definiert in:** `core/src/hit_test_tag.rs`

| Namespace | Konstante | `tag.0` Kodierung | `tag.1` Kodierung | Indirektion | Erzeugt in |
|-----------|-----------|-------------------|-------------------|-------------|------------|
| 0x0100 | `TAG_TYPE_DOM_NODE` | `TagId.inner` (sequentiell) | `0x0100` fest | Ja: `tag_ids_to_node_ids` | `display_list.rs` |
| 0x0200 | `TAG_TYPE_SCROLLBAR` | `(DomId << 32) \| NodeId` | `0x0200 \| component` | Nein: direkte NodeId | `compositor2.rs` |
| 0x0300 | `TAG_TYPE_SELECTION` | — | — | — | **Nirgends** (dead code) |
| 0x0400 | `TAG_TYPE_CURSOR` | `(DomId << 32) \| NodeId` | `0x0400 \| cursor_type` | Nein: direkte NodeId | `display_list.rs` |
| 0x0500 | `TAG_TYPE_SCROLL_CONTAINER` | `scroll_id` (node_data_hash) | `0x0500` fest | Ja: `scroll_id_to_node_id` | `compositor2.rs` |

### Inkonsistenz: 3 verschiedene Kodierungsstrategien

1. **TagId-Indirektion** (0x0100): `tag.0 = TagId` → lookup in `tag_ids_to_node_ids` → NodeId
2. **Direkte NodeId-Kodierung** (0x0200, 0x0400): `tag.0 = (DomId << 32) | NodeId.index()`
3. **Hash-Indirektion** (0x0500): `tag.0 = scroll_id` → lookup in `scroll_id_to_node_id` → NodeId

---

## 3. Datenfluss

### Hit-Test-Pipeline: Mausklick → Callback

```
1. Platform-Event (z.B. macOS mouseDown)
   ↓
2. update_hit_test_at(position) [event_v2.rs]
   ├── WebRender hit_test(physical_pos) → wr_result.items
   ├── Pass 1: 0x0500 → scroll_id_to_node_id Lookup → scroll_hit_test_nodes
   ├── Pass 2: 0x0400 → direkte NodeId-Dekodierung → cursor_hit_test_nodes
   └── Pass 3: 0x0100 → tag_ids_to_node_ids Lookup → regular_hit_test_nodes
   ↓
3. hover_manager.push_hit_test(InputPointId::Mouse, hit_test)
   ↓
4. determine_all_events() → SyntheticEvent(MouseDown, ...)
   ↓
5. invoke_callbacks_v2(RootNodes, Hover(LeftMouseDown))
   ├── hover_manager.get_current(Mouse) → FullHitTest
   ├── Finde tiefsten Node in regular_hit_test_nodes
   ├── Bubble: target → parent → ... → root
   └── invoke_single_callback() je Node mit passendem EventFilter
```

### Layout-Pipeline: DOM → Display List

```
1. User Layout-Callback → user_styled_dom
   ↓ (TagIds werden in Dom::style() → restyle() generiert)
2. inject_software_titlebar() → styled_dom (mit CSD)
   ↓ (append_child shiftet NodeIds in tag_ids_to_node_ids)
3. reconcile_dom() + transfer_states() + update_managers()
   ↓
4. apply_runtime_states_before_layout()
   ↓ (liest HoverManager NodeIds → setzt hover/active/focus auf StyledNodes)
5. layout_and_generate_display_list()
   ↓ (liest styled_node.tag_id → erzeugt HitTestArea Items)
6. compositor2 rendert Display List → WebRender
```

---

## 4. Gefundene Bugs

### BUG-1: HoverManager wird nicht remappt nach DOM-Regenerierung [HOCH]

**Ort:** `dll/src/desktop/shell2/common/layout_v2.rs:498` (`update_managers_with_node_moves`)

**Problem:** `update_managers_with_node_moves()` remappt:
- ✅ `FocusManager`
- ✅ `ScrollManager` 
- ✅ `CursorManager`
- ✅ `SelectionManager`
- ❌ **`HoverManager` — FEHLT!**

Der `HoverManager` (`layout/src/managers/hover.rs:34`) speichert `hover_histories: BTreeMap<InputPointId, VecDeque<FullHitTest>>`. Jeder `FullHitTest` enthält `hovered_nodes: BTreeMap<DomId, HitTest>`, und `HitTest` enthält `regular_hit_test_nodes: BTreeMap<NodeId, HitTestItem>`.

**Auswirkung:** In `apply_runtime_states_before_layout()` (Zeile 393-478) werden **alte NodeIds** aus dem HoverManager gelesen und auf den **neuen** StyledDom angewandt:

```rust
// layout_v2.rs:393-401
if let Some(last_hit_test) = layout_window.hover_manager.get_current_mouse() {
    if let Some(hit_test) = last_hit_test.hovered_nodes.get(&dom_id) {
        for (node_id, _hit_item) in hit_test.regular_hit_test_nodes.iter() {
            if let Some(styled_node) = styled_nodes.get_mut(*node_id) {
                styled_node.styled_node_state.hover = true; // ← FALSCHER NODE!
            }
        }
    }
}
```

Nach CSD-Injektion + DOM-Regenerierung zeigen die NodeIds im HoverManager auf die falschen Nodes. Das bedeutet:
- Falsche Nodes bekommen `hover=true` / `active=true`
- CSS `:hover` / `:active` Styling wird auf falsche Elemente angewandt
- Korrigiert sich erst beim nächsten Hit-Test (nächste Mausbewegung)

**Fix-Optionen:**
1. `HoverManager` braucht eine `remap_node_ids()` Methode
2. Oder: History nach DOM-Regenerierung leeren (einfacher, verliert aber Gesture-History für DragStart-Erkennung)

---

### BUG-2: GestureAndDragManager wird nicht remappt [MITTEL]

**Ort:** `dll/src/desktop/shell2/common/layout_v2.rs:498` (`update_managers_with_node_moves`)

**Problem:** `GestureAndDragManager` (`layout/src/managers/gesture.rs:365`) hat `active_drag: Option<DragContext>`. `DragContext` (`core/src/drag.rs:283`) enthält:

- `TextSelectionDrag.anchor_ifc_node: NodeId` — der IFC-Root-Node der Textselektion
- `TextSelectionDrag.auto_scroll_container: Option<NodeId>` — Scroll-Container für Auto-Scroll
- `ScrollbarThumbDrag.scroll_container_node: NodeId` — der gescrollte Container
- `NodeDrag.node_id: NodeId` — der gezogene Node
- `NodeDrag.current_drop_target: OptionDomNodeId` — aktuelles Drop-Target

Keine dieser NodeIds wird nach DOM-Regenerierung remappt.

**Auswirkung:** Wenn während eines Drags (Textselektion, Scrollbar-Thumb, Node-Drag) ein DOM-Rebuild ausgelöst wird (z.B. durch Callback), zeigen die gespeicherten NodeIds auf falsche Nodes. Das kann zu:
- Fehlerhafter Textselektion (falscher Anker-Node)
- Scrollbar-Drag auf falschem Container
- Drag-and-Drop auf falschem Node

**Schweregrad:** Mittel — DOM-Rebuilds während eines Drags sind selten, aber möglich (z.B. Timer-Callback während Textselektion).

---

### BUG-3: PendingContentEditableFocus wird nicht remappt [NIEDRIG]

**Ort:** `layout/src/managers/focus_cursor.rs:25` (`PendingContentEditableFocus`)

**Problem:** `FocusManager` hat `pending_contenteditable_focus: Option<PendingContentEditableFocus>` mit `container_node_id: NodeId` und `text_node_id: NodeId`. In `update_managers_with_node_moves()` wird nur `focused_node` remappt, aber **nicht** die pending-Felder.

**Auswirkung:** Wenn zwischen Focus-Request und Cursor-Initialisierung (die nach Layout passiert) ein DOM-Rebuild auftritt, zeigen die NodeIds auf falsche Nodes.

**Schweregrad:** Niedrig — Edge Case, der nur auftritt wenn Focus und DOM-Rebuild im selben Frame-Tick zusammentreffen.

---

### BUG-4: tag_ids_to_node_ids Lookup ist O(n) pro Hit-Test-Item [PERFORMANCE]

**Ort:** `dll/src/desktop/wr_translate2.rs:700-708`

**Problem:**
```rust
let node_id = layout_result
    .styled_dom
    .tag_ids_to_node_ids
    .iter()
    .find(|q| q.tag_id.inner == i.tag.0)?  // ← O(n) lineare Suche!
    .node_id
    .into_crate_internal()?;
```

Pro Hit-Test-Item wird die gesamte `tag_ids_to_node_ids`-Tabelle linear durchsucht. Bei einer DOM mit 1000 interaktiven Nodes und 50 Hit-Test-Items = 50.000 Vergleiche pro Frame.

**Fix:** `BTreeMap<u64, NodeHierarchyItemId>` oder `HashMap<u64, NodeHierarchyItemId>` statt `Vec<TagIdToNodeIdMapping>` verwenden. Alternative: Da TagIds monoton steigend sind, wäre binäre Suche auf sortiertem Vec möglich.

---

### BUG-5: Third-Pass-Filter ist nicht positiv — unbekannte Tag-Typen werden als DOM-Nodes behandelt [NIEDRIG]

**Ort:** `dll/src/desktop/wr_translate2.rs:695-698`

**Problem:**
```rust
let tag_type_marker = i.tag.1 & 0xFF00;
// Skip scrollbar tags (0x0200), cursor tags (0x0400), and scroll container tags (0x0500)
if tag_type_marker == TAG_TYPE_SCROLLBAR || tag_type_marker == TAG_TYPE_CURSOR || tag_type_marker == TAG_TYPE_SCROLL_CONTAINER {
    return None;
}
```

Dies ist ein **negativer Filter** (Blacklist). Wenn in Zukunft neue Tag-Typen hinzugefügt werden (z.B. 0x0600 für Resize-Handles) aber dieser Filter nicht aktualisiert wird, werden sie fälschlicherweise als DOM-Nodes interpretiert und bei der `tag_ids_to_node_ids`-Suche still fehlschlagen (`find()` gibt `None` zurück → `filter_map` filtert raus).

**Fix:** Positiven Filter verwenden:
```rust
if tag_type_marker != TAG_TYPE_DOM_NODE {
    return None;
}
```

---

### BUG-6: TAG_TYPE_SELECTION (0x0300) ist Dead Code [KOSMETISCH]

**Ort:** `core/src/hit_test_tag.rs:57`

**Problem:** `TAG_TYPE_SELECTION = 0x0300` ist definiert und hat Encoding/Decoding-Logik im `HitTestTag`-Enum, wird aber **nirgends im Display-List-Generator oder Compositor gepusht**. Text-Selektion verwendet stattdessen `TAG_TYPE_CURSOR` (0x0400) für Hit-Testing.

**Fix:** Entweder entfernen oder für tatsächliche Text-Selektions-Handles implementieren.

---

### BUG-7: HitTestTag-Enum wird nicht durchgängig genutzt — rohe Bit-Manipulation [DESIGN]

**Ort:** `core/src/hit_test_tag.rs` (gesamte Datei)

**Problem:** Es existiert ein sauberes `HitTestTag`-Enum mit Varianten `DomNode`, `Scrollbar`, `Cursor`, `ScrollContainer`, aber der tatsächliche Code in `display_list.rs`, `compositor2.rs` und `wr_translate2.rs` arbeitet mit **rohen `(u64, u16)` Tuples** und manueller Bit-Manipulation. Das Enum wird de facto nur für Tests/Dokumentation genutzt.

**Auswirkung:** Fehleranfällig bei Erweiterungen — Encoding und Decoding müssen manuell synchron gehalten werden.

---

## 5. Design-Schwächen

### 5.1 Doppelspeicherung von TagId ist unnötig

`StyledNode.tag_id` (pro Node) und `StyledDom.tag_ids_to_node_ids` (zentrale Tabelle) speichern dieselbe Information. Die Konsumenten sind:

| Konsument | Liest von | Datei |
|-----------|-----------|-------|
| `get_tag_id()` | `styled_node.tag_id` | `display_list.rs:3225` |
| `find_styled_node_for_hit_test()` | `styled_node.tag_id` | `display_list.rs:~1655` |
| Hit-Test-Rückauflösung | `tag_ids_to_node_ids` | `wr_translate2.rs:704` |

Die Display-List-Generierung könnte stattdessen direkt `tag_ids_to_node_ids` lesen (eine `HashMap<NodeId, TagId>` wäre O(1)). Die Doppelspeicherung erhöht die Fehleranfälligkeit bei `append_child` / `restyle`.

### 5.2 Inkonsistente Indirektionsstrategien

- DOM-Nodes (0x0100): TagId → `tag_ids_to_node_ids` → NodeId
- Scrollbar (0x0200): Direkte `(DomId << 32) | NodeId` Kodierung
- Cursor (0x0400): Direkte `(DomId << 32) | NodeId` Kodierung
- Scroll-Container (0x0500): `scroll_id` (Hash) → `scroll_id_to_node_id` → NodeId

Alle könnten die direkte Kodierung verwenden (wie Scrollbar/Cursor), was die `tag_ids_to_node_ids`-Tabelle und den O(n)-Lookup eliminieren würde. Der einzige Grund für die TagId-Indirektion ist, dass TagId-Werte kleiner als NodeIds sind — aber bei 32-Bit-Encoding in `tag.0` ist das irrelevant, da NodeIds ebenfalls in 32 Bit passen.

### 5.3 TagId-Generierung ist zu früh

TagIds werden in `Dom::style()` → `restyle()` generiert, **vor** CSD-Injektion und vor dem Layout. Theoretisch könnten sie **nach** CSD-Injektion generiert werden, da die einzigen Konsumenten erst in der Display-List-Generierung (nach Layout) aktiv werden.

Allerdings löst das allein nicht die Manager-Bugs (BUG-1, BUG-2, BUG-3), weil Manager NodeIds speichern — nicht TagIds.

---

## 6. Vorschlag: TagId von StyledNode entfernen

### Motivation

`StyledNode.tag_id` ist eine denormalisierte Kopie, die:
- Fehler bei `append_child` ermöglicht (obwohl aktuell korrekt geshiftet)
- Bei `restyle()` komplett neu generiert werden muss
- Nur von 2 Stellen in `display_list.rs` gelesen wird
- **Nicht** von `restyle_nodes_hover/active/focus` gelesen wird (diese brauchen keine TagIds)

### Vorgehensweise

1. **`StyledNode.tag_id` entfernen** — nur `styled_node_state` behalten
2. **`tag_ids_to_node_ids`** durch `BTreeMap<NodeId, TagId>` ersetzen (oder besser: `HashMap<NodeId, TagId>`) — als einzige Quelle der Wahrheit
3. **`get_tag_id()` in `display_list.rs`** ändern: statt `styled_node.tag_id` lesen → `tag_ids_to_node_ids.get(&node_id)` verwenden
4. **Hit-Test-Rückauflösung**: `BTreeMap<TagId.inner, NodeId>` für O(log n) Lookup (oder `HashMap` für O(1))

### Weitergehend: DOM-Node-Tags auf direkte NodeId-Kodierung umstellen

Die radikalste Vereinfachung wäre, DOM-Node-Tags (0x0100) auf dasselbe Encoding wie Cursor/Scrollbar umzustellen:

```
tag.0 = (DomId.inner << 32) | NodeId.index()
tag.1 = 0x0100
```

Das würde eliminieren:
- Die gesamte `tag_ids_to_node_ids`-Tabelle
- Den globalen `TAG_ID: AtomicUsize` Counter
- Die `TagId`-Generierung in `restyle()`
- Die O(n) lineare Suche bei Hit-Test-Rückauflösung
- Die Doppelspeicherung

**Aber Achtung**: Die `tab_index` Information, die aktuell in `TagIdToNodeIdMapping` gespeichert wird, müsste separat verfügbar gemacht werden (z.B. direkt aus `NodeData` lesen, was ohnehin schon passiert in `wr_translate2.rs:733`).

---

## Zusammenfassung

| # | Bug | Schweregrad | Aufwand |
|---|-----|-------------|---------|
| BUG-1 | HoverManager nicht remappt | 🔴 HOCH | Mittel |
| BUG-2 | GestureAndDragManager nicht remappt | 🟠 MITTEL | Mittel |
| BUG-3 | PendingContentEditableFocus nicht remappt | 🟡 NIEDRIG | Klein |
| BUG-4 | O(n) lineare Suche bei Hit-Test | 🟡 PERFORMANCE | Klein |
| BUG-5 | Negativer Tag-Typ-Filter | 🟡 NIEDRIG | Klein |
| BUG-6 | TAG_TYPE_SELECTION ist Dead Code | ⚪ KOSMETISCH | Klein |
| BUG-7 | HitTestTag-Enum nicht genutzt | ⚪ DESIGN | Groß |
