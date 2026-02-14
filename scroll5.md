## Vollständiger Report: scroll3.md / scroll4.md — Status & Lösungsplan

### A) Was ist FERTIG

| Schritt | Beschreibung | Commit / Status |
|---------|-------------|-----------------|
| **1-2** | `scan_for_iframes` Race Fix — `&StyledDom` direkt statt aus `layout_results` | ✅ c909daa5 |
| **3** | `ScrollInput`, `ScrollInputSource`, `ScrollInputQueue` (Arc<Mutex>), `ScrollNodeInfo`, `record_scroll_input()`, `record_scroll_from_hit_test()`, `get_input_queue()`, `get_scroll_node_info()` auf CallbackInfo + TimerCallbackInfo | ✅ f37b012a + uncommitted |
| **4** | `scroll_physics_timer_callback` in scroll_timer.rs — 291 Zeilen, TrackpadContinuous/WheelDiscrete/Programmatic, Velocity-Decay, selbst-terminierend | ✅ 96617f31 |
| **5 (macOS)** | `handle_scroll_wheel` benutzt `has_precise`, ruft `record_scroll_from_hit_test()`, startet `SCROLL_MOMENTUM_TIMER_ID` Timer | ✅ uncommitted |
| **6** | macOS `physics_tick` Hack aus `render_and_present` entfernt | ✅ 7e829b97 |

### B) Was NICHT fertig / kaputt ist

#### 1. `auto_scroll_timer_callback` — TODO-Stub
event_v2.rs: Nur Maus-Button-Check, kein Scrolling. Kommentar sagt: *"scroll_selection_into_view requires &mut LayoutWindow, but we only have &CallbackInfo"*.

**Lösung**: Genau das Pattern, das wir schon in scroll_timer.rs haben: `push_change(CallbackChange::ScrollTo { ... })`. Der Timer berechnet Delta basierend auf Mausdistanz zum Container-Rand, pushed ScrollTo, fertig.

#### 2. `AUTO_SCROLL_TIMER_ID = 0xABCD_1234` — Legacy
event_v2.rs benutzt noch einen hardcoded Magic-Wert statt `DRAG_AUTOSCROLL_TIMER_ID` aus task.rs (= `0x0003`).

#### 3. `begin_frame()` wird nie aufgerufen → EventProvider broken
`ScrollManager.begin_frame()` (scroll_state.rs) setzt `previous_offset = current_offset`. Wird von keinem dll/-Code aufgerufen → `get_pending_events()` erkennt nie Scroll-Deltas → **Scroll-SyntheticEvents werden nie generiert** → IFrame-Edge-Detection in event_v2.rs (`has_scroll_events` → `mark_frame_needs_regeneration`) feuert nie.

#### 4. IFrame-Reinvocation bei Scroll — nicht verdrahtet
`IFrameManager.check_reinvoke()` existiert und ist korrekt. Aber es wird nur über `invoke_iframe_callback_with_dom()` aufgerufen, was aus `scan_for_iframes` kommt, was nur bei Layout-Durchläufen passiert. Es gibt keinen Pfad von "Timer pushed ScrollTo" → "IFrame wird re-invoked".

#### 5. Windows/Linux/Wayland benutzen noch `record_sample()`
- mod.rs: `record_sample()` direkt
- mod.rs: `record_sample()` + `gpu_scroll()` (!= doppelt)
- events.rs: `record_sample()` direkt

Diese müssen auf `record_scroll_from_hit_test()` + Timer-Start umgestellt werden (wie macOS).

#### 6. Alte Code-Leichen
- `record_sample()` auf ScrollManager — wird nach Umstellung nicht mehr gebraucht
- `EventProvider for ScrollManager` — wird obsolet (kein `begin_frame()` nötig)
- `begin_frame()` / `end_frame()` / `FrameScrollInfo` — dead code
- `had_scroll_activity` / `had_programmatic_scroll` / `had_new_doms` Flags — nur von EventProvider genutzt
- `get_scroll_delta()` / `had_scroll_activity_for_node()` — nutzen `previous_offset` → dead code
- Alte Modul-Doku (scroll_state.rs) referenziert `record_sample()` + `tick()`

---

### C) Architektur-Plan: Transparente IFrame-Reinvocation im ScrollTo-Pfad

#### Kernidee

Der **Scroll-Timer** berechnet die Physik (Velocity, Decay, Clamp) und pushed `CallbackChange::ScrollTo { dom_id, node_id, position }`. Der Timer weiß **nichts** über IFrames.

**Im ScrollTo-Verarbeitungscode** (event_v2.rs) passiert dann:

```
1. scroll_manager.scroll_to(dom_id, node_id, position, ...)
2. iframe_manager.check_reinvoke(dom_id, node_id, &scroll_manager, layout_bounds)
3. Falls Some(reason) zurückkommt:
   a. IFrame-Callback re-invoken → neues StyledDom
   b. Layout neu berechnen (nur für diesen Sub-Tree)
   c. Display-List regenerieren
   d. result = max(ShouldRegenerateDomCurrentWindow)
4. Velocity bleibt erhalten im Timer — nächster Tick pushed wieder ScrollTo
```

**Warum das funktioniert**: Der Timer hat seine Velocity pro Node in `ScrollPhysicsState.node_velocities`. Er terminiert erst wenn `velocity < threshold`. Ob im ScrollTo-Verarbeitungscode ein IFrame re-invoked wird, ist ihm egal. Nächster Timer-Tick → nächster `get_scroll_node_info()` → liest neue Position → berechnet neues Delta → pushed neues ScrollTo. Die IFrame-Ersetzung ist **vollständig transparent**.

#### Was der ScrollManager speichern muss

`AnimatedScrollState` hat aktuell:
- `current_offset`, `previous_offset` (previous wird obsolet)
- `container_rect`, `content_rect`

**Fehlend**: `virtual_scroll_size` und `virtual_scroll_offset` von IFrameCallbackReturn. Diese bestimmen die **echten** Scroll-Limits für IFrame-Nodes. Ohne sie clampt der Timer auf `content_rect` statt auf `virtual_scroll_size`.

```rust
pub struct AnimatedScrollState {
    pub current_offset: LogicalPosition,
    pub animation: Option<ScrollAnimation>,
    pub last_activity: Instant,
    pub container_rect: LogicalRect,
    pub content_rect: LogicalRect,
    // NEU:
    pub virtual_scroll_size: Option<LogicalSize>,
    pub virtual_scroll_offset: Option<LogicalPosition>,
}
```

Die Clamp-Logik wird dann:
```rust
fn max_scroll(&self) -> (f32, f32) {
    let effective_content = self.virtual_scroll_size
        .unwrap_or(self.content_rect.size);
    let max_x = (effective_content.width - self.container_rect.size.width).max(0.0);
    let max_y = (effective_content.height - self.container_rect.size.height).max(0.0);
    (max_x, max_y)
}
```

#### Datenfluß

```
┌─────────────────────────────────────────────────────────┐
│ Platform Event Handler (macOS/Windows/Linux)            │
│  → scroll_manager.record_scroll_from_hit_test()         │
│  → start SCROLL_MOMENTUM_TIMER if first input           │
└──────────────────────┬──────────────────────────────────┘
                       │ ScrollInputQueue (Arc<Mutex>)
                       ▼
┌─────────────────────────────────────────────────────────┐
│ scroll_physics_timer_callback (every 16ms)              │
│  1. queue.take_all() — drain inputs                     │
│  2. Apply physics (impulse, velocity decay, clamp)      │
│  3. push_change(CallbackChange::ScrollTo {              │
│       dom_id, node_id, position })                      │
│  4. Return continue_and_update() or terminate           │
└──────────────────────┬──────────────────────────────────┘
                       │ CallbackChange::ScrollTo
                       ▼
┌─────────────────────────────────────────────────────────┐
│ process_callback_result_v2 (event_v2.rs)                │
│  1. scroll_manager.scroll_to(dom_id, node_id, pos, 0)  │
│  2. IF node has IFrame children:                        │
│     a. iframe_manager.check_reinvoke(dom_id, node_id,   │
│        &scroll_manager, layout_bounds)                  │
│     b. If Some(reason): re-invoke IFrame callback       │
│     c. Update layout + display list                     │
│  3. event_result = ShouldReRenderCurrentWindow          │
│     (or ShouldRegenerateDom if IFrame re-invoked)       │
└─────────────────────────────────────────────────────────┘
```

#### Konkrete Änderungen

| # | Datei | Was |
|---|-------|-----|
| 1 | scroll_state.rs | `virtual_scroll_size/offset` zu `AnimatedScrollState` hinzufügen, `previous_offset` entfernen, `record_sample()` entfernen, `begin_frame()`/`end_frame()`/`FrameScrollInfo` entfernen, `EventProvider` impl entfernen, Clamp-Logik aktualisieren, alte Flags entfernen, Modul-Doku aktualisieren |
| 2 | event_v2.rs | ScrollTo-Verarbeitung um IFrame-Check erweitern, `auto_scroll_timer_callback` mit `push_change(ScrollTo)` verdrahten, `AUTO_SCROLL_TIMER_ID` → `DRAG_AUTOSCROLL_TIMER_ID`, alte `has_scroll_events` IFrame-Detection entfernen (wird jetzt im ScrollTo-Pfad gemacht), stale Kommentare die `record_sample` referenzieren aktualisieren |
| 3 | scroll_timer.rs | Clamp-Logik auf `virtual_scroll_size`-aware `max_scroll_x/y` umstellen (kommt automatisch über `get_scroll_node_info()`) |
| 4 | windows/mod.rs | `record_sample()` → `record_scroll_from_hit_test()` + Timer-Start (wie macOS) |
| 5 | x11/events.rs | Dito |
| 6 | wayland/mod.rs | Dito + `gpu_scroll()` Doppel-Call entfernen |
| 7 | iframe.rs | IFrame-Info (scroll_size, virtual_scroll_size) auch in ScrollManager propagieren wenn `update_iframe_info()` aufgerufen wird |
