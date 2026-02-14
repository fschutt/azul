## Ehrliche Bewertung: Was wurde implementiert vs. was geplant war

### Phase A (Sizing Bug) — ✅ VOLLSTÄNDIG
Der Fix in `cache.rs` prüft `is_scroll_container` + `containing_block_size.height.is_finite()` — exakt wie Gemini empfohlen hat, inklusive der Edge-Case-Absicherung.

### Phase B (Velocity Scrolling) — ⚠️ OBERFLÄCHLICH
Was gemacht wurde:
- `ScrollPhysicsState` Struct hinzugefügt (velocity + is_user_interacting)
- `physics_tick()` mit exponentiellem Decay
- `add_scroll_impulse()` Methode

Was **fehlt**:
- **`record_sample()` wurde NICHT geändert** — es setzt immer noch direkt die Position (`set_scroll_position`), anstatt über Impulse/Velocity zu gehen. Der Plan sagte: "Update record_sample to inject velocity instead of setting position directly".
- **Keine Unterscheidung Wheel vs. Trackpad** — Der Plan sagte explizit: Wheel → adds impulse, Trackpad → sets offset directly (OS handles physics). Nichts davon implementiert.
- **`physics_tick` wird nirgends aufgerufen** außer im macOS `render_and_present` — Windows/Linux ignoriert.
- **`dt` ist hardcoded `1.0/60.0`** — kein echtes Zeitdelta zwischen Frames.

### Phase C (Overscroll) — ⚠️ NUR PHYSIK-CODE
Spring-Force-Berechnung in `physics_tick()` ist da, aber:
- **WebRender bekommt immer noch die geclampte Position** — `scroll_all_nodes()` liest `current_offset`, aber `set_scroll_position()` clampt weiterhin hart. Die visuelle Overscroll-Position wird nie an WebRender durchgereicht.
- **Kein visuelles Rubber-Band** — der Container-Hintergrund wird nicht korrekt gemalt wenn Content "zurückspringt".

### Phase D (Drag-Select Auto-Scroll) — ⚠️ NUR API
`auto_scroll_for_drag()` und `find_scroll_parent()` existieren als Methoden, aber:
- **Werden nirgends aufgerufen** — weder im Render-Loop noch in Event-Handling.
- Die existierende `auto_scroll_timer_callback` in event_v2.rs (Z. 248) hat einen **TODO-Stub** — genau hier hätten die neuen Methoden eingebaut werden müssen.

### Phase E (Clean Architecture) — ⚠️ NUR DOCS
Nur Modul-Dokumentation und `needs_animation_frame()` hinzugefügt. Keine tatsächliche Refactoring-Arbeit. Die Architektur war aber bereits relativ sauber (gesture.rs fasst Offsets nicht an).

### Phase F (Build & Test) — ❌ NICHT DURCHGEFÜHRT

---

## Deine Timer-basierte Architektur: Analyse

Dein Vorschlag: **ScrollManager als reiner Datenrecorder, Scroll-Timer als User-Space-Callback.**

```
Input → ScrollManager.record_input(metadata)
      → starts/stops SCROLL_PHYSICS_TIMER
      
Timer fires → TimerCallbackInfo.get_scroll_inputs()
            → berechnet Physik (velocity, spring, decay)
            → push_change(CallbackChange::ScrollTo { position })
            → return continue_and_update() / terminate_unchanged()
```

### Warum das DEUTLICH sauberer ist:

1. **Eliminiert den Sonderfall in `render_and_present`**. Der `physics_tick(1.0/60.0)` im macOS-Render-Loop ist ein Hack: hardcoded dt, nur macOS, nicht im normalen Timer-System. Mit einem reservierten Timer läuft die Physik plattformunabhängig — überall wo Timer funktionieren.

2. **Das Timer-System existiert bereits und hat das richtige API**:
   - `AUTO_SCROLL_TIMER_ID` (0xABCD_1234) zeigt, dass reservierte Timer-IDs schon Praxis sind
   - `TimerCallbackInfo` hat Zugriff auf `CallbackInfo` → `get_current_window_state()` → Mouse-Position, Timestamps
   - `TimerCallbackReturn` hat `continue_and_update()` → triggert automatisch Repaint
   - `TerminateTimer::Terminate` → sauberes Aufräumen wenn Velocity = 0

3. **Echte Zeitdeltas statt hardcoded 1/60**: Der Timer hat `frame_start` und `call_count` — damit kann das Callback den echten dt berechnen.

4. **Saubere Start/Stop-Semantik**:
   - Scroll-Event kommt rein → `record_sample()` speichert Input-Metadata + startet `SCROLL_PHYSICS_TIMER` (falls nicht schon läuft)
   - Timer feuert jeden Frame → berechnet Physik → `ScrollTo` via CallbackChange
   - Velocity < threshold → `TerminateTimer::Terminate`
   - Kein manuelles "brauche ich noch Animation-Frames?" — der Timer managed sich selbst

5. **Mutable-Access-Problem ist gelöst**: Der TODO in `auto_scroll_timer_callback` (Z. 282) sagt: *"scroll_selection_into_view requires &mut LayoutWindow, but we only have &CallbackInfo which has \*const LayoutWindow"*. Die Lösung ist `push_change(CallbackChange::ScrollTo { ... })` — das ist ein transaktionales Muster das schon für alles andere funktioniert.

6. **Testbarkeit**: Timer-Callbacks können einzeln getestet werden — ScrollManager.get_pending_inputs() → simulate timer → assert ScrollTo changes.

### Konkrete Architektur:

```rust
const SCROLL_PHYSICS_TIMER_ID: usize = 0xABCD_2000;

// ScrollManager wird zu reinem Recorder:
impl ScrollManager {
    /// Records scroll input with full metadata (nicht mehr direkt Position setzen)
    pub fn record_scroll_input(&mut self, input: ScrollInput) {
        self.pending_inputs.push(input);
        self.needs_physics_timer = true; // flag for caller
    }
    
    /// Timer callback asks: what inputs do I need to process?
    pub fn take_pending_inputs(&mut self) -> Vec<ScrollInput> {
        core::mem::take(&mut self.pending_inputs)
    }
}

struct ScrollInput {
    dom_id: DomId,
    node_id: NodeId,
    delta: LogicalPosition,
    timestamp: Instant,
    source: ScrollInputSource, // Wheel, Trackpad, Programmatic
}

enum ScrollInputSource {
    /// Discrete wheel steps (apply as impulse → momentum)
    WheelDiscrete,
    /// Continuous trackpad stream (set position directly, OS handles momentum)
    TrackpadContinuous,
    /// Programmatic scroll (easing animation)
    Programmatic { duration: Duration, easing: EasingFunction },
}

// Timer callback:
extern "C" fn scroll_physics_timer(
    data: RefAny, // contains ScrollPhysicsState
    timer_info: TimerCallbackInfo,
) -> TimerCallbackReturn {
    let dt = /* berechne aus timer_info.frame_start - letzte Ausführung */;
    let physics: &mut ScrollPhysicsState = data.downcast_mut();
    
    // 1. Consume pending inputs
    let inputs = timer_info.callback_info.take_scroll_inputs();
    for input in inputs {
        match input.source {
            WheelDiscrete => physics.add_impulse(input.delta),
            TrackpadContinuous => physics.set_position(input.delta),
            Programmatic { .. } => physics.start_easing(input),
        }
    }
    
    // 2. Integrate physics
    let any_active = physics.tick(dt);
    
    // 3. Apply new positions via transaction
    for (node, position) in physics.get_updated_positions() {
        timer_info.callback_info.push_change(
            CallbackChange::ScrollTo { dom_id, node_id, position }
        );
    }
    
    if any_active {
        TimerCallbackReturn::continue_and_update()
    } else {
        TimerCallbackReturn::terminate_unchanged()
    }
}
```

### Was sich ändern muss:

Das bestehende `CallbackChange::ScrollTo` + `CallbackInfo.push_change()` erlaubt dem Timer, Scroll-Positionen transaktional zu setzen — genau wie user-space Events.

Der **einzige Blocker** ist: `take_scroll_inputs()` braucht `&mut` auf den ScrollManager, aber `CallbackInfo` hat nur `*const`. Das muss entweder:
- Als neuer `CallbackChange`-Variant laufen (ScrollManager schreibt Inputs in einen separaten `Arc<Mutex<>>`)
- Oder `CallbackInfoRefData` bekommt einen `&ScrollManager`-Zugriff (read-only reicht, Inputs werden per Mutex rausgegeben)

**Fazit**: Ja, deine Architektur ist deutlich sauberer. Die aktuellen Commits B-E sind im Wesentlichen Scaffolding-Code, der nirgends wirklich verdrahtet ist. Die Physik-Methoden existieren, aber der eigentliche Datenfluss (Input → Physics → WebRender) ist nicht connected. Die Timer-basierte Variante würde den Datenfluss durch ein bereits bewährtes System leiten, anstatt einen neuen Sonderpfad in den Render-Loop zu bauen.

---

## IFrame-Analyse: Warum infinity.c nichts anzeigt

Siehe `IFRAME_ANALYSIS.md` für die vollständige Analyse. Kurzfassung:

### Bug #1 (CRITICAL): `scan_for_iframes` Race Condition
`layout_and_generate_display_list()` ruft `scan_for_iframes()` auf (Z. 861), aber
`layout_results` wird erst danach eingefügt (Z. 887). `scan_for_iframes` macht
`self.layout_results.get(&dom_id)?` → `None` → leerer Vec → keine IFrames gefunden.

### Bug #2 (DESIGN GAP): Scroll → IFrame nicht verdrahtet
`IFrameManager.check_reinvoke()` mit `EdgeScrolled`-Detection existiert und ist korrekt
implementiert, wird aber **von keinem Scroll-Event aufgerufen**. Es gibt keinen Codepfad
von "User scrollt" → "IFrame wird re-invoked".

### Bug #3: `invoke_iframe_callback` hat dasselbe Problem
Auch `invoke_iframe_callback()` (Z. 1042) greift auf `layout_results` zu, das beim
Initial-Render noch nicht existiert.

### Unterschied Rust vs. C
- `infinity.rs`: Workaround via `on_scroll` + `Update::RefreshDom` (Full DOM Rebuild)
- `infinity.c`: Verlässt sich auf IFrame-Re-Invokation (kaputt wegen Bug #1 + #2)

---

## Holistischer Aktionsplan: Scroll + IFrame Refactoring

**Grundsatz**: Wir können alles brechen. Phases B-E sind Scaffolding ohne echte Verdrahtung.
Das Ziel ist ein sauberer Datenfluss: Input → ScrollManager → Timer → WebRender + IFrame.

### Schritt 1: `scan_for_iframes` Fix (5 min)

**Dateien**: `layout/src/window.rs`

`scan_for_iframes` bekommt `&StyledDom` als Parameter statt `layout_results` zu lesen:

```rust
fn scan_for_iframes(
    &self,
    styled_dom: &StyledDom,    // ← NEU: direkt übergeben
    layout_tree: &LayoutTree,
    calculated_positions: &BTreeMap<usize, LogicalPosition>,
) -> Vec<(NodeId, LogicalRect)> {
    layout_tree.nodes.iter().enumerate().filter_map(|(idx, node)| {
        let node_dom_id = node.dom_node_id?;
        let node_data = &styled_dom.node_data.as_container()[node_dom_id];
        if matches!(node_data.get_node_type(), NodeType::IFrame(_)) {
            let pos = calculated_positions.get(&idx).copied().unwrap_or_default();
            let size = node.used_size.unwrap_or_default();
            Some((node_dom_id, LogicalRect::new(pos, size)))
        } else {
            None
        }
    }).collect()
}
```

Gleicher Fix für `invoke_iframe_callback`: `styled_dom` als Parameter durchreichen statt
aus `layout_results` zu lesen.

### Schritt 2: ScrollManager als reiner Recorder (30 min)

**Dateien**: `layout/src/managers/scroll_state.rs`

Rückbau von Phases B-E Scaffolding. ScrollManager wird zu:

```rust
pub struct ScrollManager {
    // Bestehend (behalten):
    scroll_nodes: BTreeMap<(DomId, NodeId), ScrollNodeState>,
    
    // NEU: Input-Queue statt direkte Position
    pending_inputs: Vec<ScrollInput>,
    physics_timer_active: bool,
}

pub struct ScrollInput {
    pub dom_id: DomId,
    pub node_id: NodeId,
    pub delta: LogicalPosition,
    pub timestamp: Instant,
    pub source: ScrollInputSource,
}

pub enum ScrollInputSource {
    WheelDiscrete,       // Mouse-Rad → Impulse + Momentum
    TrackpadContinuous,  // Trackpad → Position direkt (macOS-Physik)
    Programmatic,        // scroll_to() API
}

impl ScrollManager {
    /// Records input — SETZT KEINE POSITION MEHR
    pub fn record_scroll_input(&mut self, input: ScrollInput) {
        self.pending_inputs.push(input);
        self.physics_timer_active = true;
    }
    
    /// Timer holt pending inputs
    pub fn take_pending_inputs(&mut self) -> Vec<ScrollInput> {
        core::mem::take(&mut self.pending_inputs)
    }
    
    // Bestehend behalten:
    pub fn get_current_offset(&self, ...) -> Option<LogicalPosition> { ... }
    pub fn update_node_bounds(&mut self, ...) { ... }
    pub fn set_scroll_position(&mut self, ...) { ... }  // Nur noch vom Timer aufgerufen
}
```

**Entfernen**:
- `ScrollPhysicsState` (Phase B) → wird Teil des Timer-Callbacks
- `physics_tick()` (Phase B/C) → wird im Timer-Callback gemacht
- `add_scroll_impulse()` (Phase B) → durch `record_scroll_input` ersetzt
- `auto_scroll_for_drag()` (Phase D) → wird separater Timer
- `find_scroll_parent()` (Phase D) → bleibt, ist nützlich
- `needs_animation_frame()` (Phase E) → Timer managed sich selbst

### Schritt 4: Scroll-Physik-Timer implementieren (60 min)

**Dateien**: `dll/src/desktop/shell2/common/event_v2.rs`, neues Modul

```rust
const SCROLL_PHYSICS_TIMER_ID: usize = 0xABCD_2000;

extern "C" fn scroll_physics_timer(
    data: RefAny,
    timer_info: TimerCallbackInfo,
) -> TimerCallbackReturn {
    let physics = data.downcast_mut::<ScrollPhysicsState>();
    let callback_info = &timer_info.callback_info;
    
    // 1. dt berechnen
    let now = timer_info.frame_start;
    let dt = now.duration_since(physics.last_tick).as_secs_f32();
    physics.last_tick = now;
    
    // 2. Pending Inputs konsumieren (via neuen CallbackInfo-Accessor)
    let inputs = callback_info.take_scroll_inputs();
    for input in inputs {
        match input.source {
            WheelDiscrete => physics.add_impulse(input.dom_id, input.node_id, input.delta),
            TrackpadContinuous => physics.set_offset(input.dom_id, input.node_id, input.delta),
            Programmatic => physics.start_easing(input),
        }
    }
    
    // 3. Physik integrieren (velocity decay, spring force für overscroll)
    let any_active = physics.tick(dt);
    
    // 4. Neue Positionen als CallbackChange publishen
    for (dom_id, node_id, position) in physics.drain_updated_positions() {
        callback_info.push_change(CallbackChange::ScrollTo { dom_id, node_id, position });
    }
    
    // 5. IFrame Edge-Detection: Prüfe ob gescrollte Nodes IFrame-Parents sind
    for (dom_id, node_id) in physics.get_scrolled_nodes() {
        if callback_info.node_is_iframe_parent(dom_id, node_id) {
            callback_info.push_change(CallbackChange::UpdateIFrame { dom_id, node_id });
        }
    }
    
    if any_active {
        TimerCallbackReturn { should_update: Update::RefreshDom, should_terminate: TerminateTimer::Continue }
    } else {
        TimerCallbackReturn { should_update: Update::DoNothing, should_terminate: TerminateTimer::Terminate }
    }
}
```

### Schritt 5: Event-Handler verdrahten (30 min)

**Dateien**: `dll/src/desktop/shell2/common/event_v2.rs`, macOS/Windows/Linux-spezifische Module

In `process_scroll_event()` (oder äquivalent):

```rust
fn handle_scroll_event(&mut self, delta: LogicalPosition, source: ScrollInputSource) {
    // 1. Hit-Test: welcher Node wird gescrollt?
    let (dom_id, node_id) = self.hit_test_scroll_target(mouse_pos);
    
    // 2. Input recorden (NICHT Position setzen!)
    self.scroll_manager.record_scroll_input(ScrollInput {
        dom_id, node_id, delta, timestamp: now, source,
    });
    
    // 3. Physik-Timer starten falls nötig
    if self.scroll_manager.physics_timer_active && !self.has_timer(SCROLL_PHYSICS_TIMER_ID) {
        self.add_timer(SCROLL_PHYSICS_TIMER_ID, Timer::new(
            RefAny::new(ScrollPhysicsState::new()),
            scroll_physics_timer,
            Duration::from_millis(16), // ~60 Hz
        ));
    }
}
```

### Schritt 6: macOS `physics_tick` Hack entfernen (5 min)

**Dateien**: `dll/src/desktop/shell2/macos/mod.rs`

Den hardcoded `physics_tick(1.0/60.0)` aus `render_and_present` entfernen —
wird jetzt vom Timer erledigt.

### Schritt 7: IFrame Re-Invokation bei Scroll (20 min)

**Dateien**: `layout/src/window.rs`

Zwei Ansätze:

**A) Via `CallbackChange::UpdateIFrame` (bevorzugt)**:
Der Scroll-Physik-Timer (Schritt 4) prüft nach jeder Position-Änderung, ob der gescrollte
Node ein IFrame-Parent ist, und queued `CallbackChange::UpdateIFrame`. Die bestehende
`process_iframe_updates()`-Pipeline handled den Rest.

**B) Via neuem `scroll_triggered_iframe_check()` (Fallback)**:
Nach `set_scroll_position` in `ScrollManager` prüfen ob es einen IFrame für
`(dom_id, node_id)` gibt → `iframe_manager.check_reinvoke()` → ggf. re-invoke.

### Schritt 8: Tests (30 min)

- Unit-Test: `ScrollManager.record_scroll_input()` → `take_pending_inputs()` Roundtrip
- Unit-Test: Timer-Callback mit simulierten Inputs → prüfe CallbackChange::ScrollTo Outputs
- Integration-Test: IFrame InitialRender funktioniert (scan_for_iframes findet IFrames)
- Integration-Test: Scroll → EdgeScrolled → IFrame re-invoke
- Visueller Test: `infinity.c` kompilieren und ausführen

---

## Zusammenfassung der Dateien die sich ändern

| Datei | Änderung | Schritt |
|-------|---------|---------|
| `layout/src/window.rs` | `scan_for_iframes` Fix + `invoke_iframe_callback` Fix | 1-2 |
| `layout/src/managers/scroll_state.rs` | ScrollManager zu reinem Recorder umbauen | 3 |
| `dll/src/desktop/shell2/common/event_v2.rs` | Scroll-Physik-Timer implementieren | 4-5 |
| `dll/src/desktop/shell2/macos/mod.rs` | `physics_tick` Hack entfernen | 6 |
| `layout/src/window.rs` | IFrame Re-Invokation bei Scroll | 7 |
| `layout/tests/` | Tests | 8 |

## Reihenfolge der Implementierung

```
Schritt 1-2: scan_for_iframes Fix          ← infinity.c zeigt überhaupt was an
Schritt 3:   ScrollManager Recorder         ← Clean API, Phases B-E Rückbau
Schritt 4:   Scroll-Physik-Timer            ← Velocity, Momentum, Overscroll
Schritt 5:   Event-Handler verdrahten       ← Input → Timer → Position
Schritt 6:   macOS Hack entfernen           ← Aufräumen  
Schritt 7:   IFrame Scroll Re-Invoke        ← EdgeScrolled funktioniert
Schritt 8:   Tests                          ← Absicherung
```

Schritte 1-2 sind unabhängig und können sofort gemacht werden — sie fixen infinity.c
für InitialRender. Schritte 3-7 sind das Timer-Refactoring.

## Risiken

1. **`take_scroll_inputs()` braucht `&mut ScrollManager`** — der Timer hat nur `&CallbackInfo`
   mit `*const LayoutWindow`. Lösung: `pending_inputs` in `Arc<Mutex<Vec<ScrollInput>>>`
   lagern, oder einen neuen Accessor auf `CallbackInfo` bauen der Mutex-Lock macht.

2. **Input-Unterscheidung Wheel vs. Trackpad**: macOS liefert `NSEvent.hasPreciseScrollingDeltas`
   — muss in `ScrollInputSource` übersetzt werden. Windows/Linux haben kein OS-Momentum, dort
   ist alles `WheelDiscrete`.

3. **Timer-Latenz**: 16ms Timer-Intervall statt vsync-synchron. In der Praxis irrelevant,
   da `CVDisplayLink` / `RequestAnimationFrame` sowieso ~16ms sind.