# Shell2 TODO Analysis and Implementation Plan

**Date**: 20. Oktober 2025

## Complete TODO Inventory

### Category 1: CRITICAL - Core Rendering (Priority: HIGH)

**compositor2/mod.rs** - Display List Translation
1. ❌ Handle border_radius for Rect (line 66)
2. ❌ Implement proper border rendering (line 122)
3. ❌ Handle rounded corners for PushClip (line 185)
4. ❌ Implement clip stack for PushClip/PopClip (lines 185, 192)
5. ❌ Implement scroll frames (PushScrollFrame/PopScrollFrame) (lines 200, 205)
6. ❌ Attach tag for DOM node hit-testing in HitTestArea (line 218)
7. ❌ Implement text rendering (line 222)
8. ❌ Implement image rendering (line 226)
9. ❌ Implement iframe embedding (line 230)

**Impact**: Without these, the renderer can only display solid rectangles and scrollbars. No text, images, borders, or clipping.

### Category 2: CRITICAL - Event System (Priority: HIGH)

**events.rs** - Event Handlers
1. ❌ Jump scroll to clicked position (scrollbar track click) (line 107)
2. ❌ Update keyboard_state properly (line 492)
3. ❌ Proper RawWindowHandle instead of default() (line 564)
4. ❌ Update scroll positions for scrollable nodes on scroll wheel (line 760)
5. ❌ Filter window-level key callbacks (keyDown) (line 775)
6. ❌ Filter window-level key callbacks (keyUp) (line 785)
7. ❌ Look up callbacks for FileDrop events (line 795)
8. ❌ Notify WebRender of new size on resize (line 824)
9. ❌ Resize GL viewport or CPU framebuffer (line 825)

**Impact**: Missing critical event handling, scroll wheel doesn't work, keyboard state broken, window resize broken.

### Category 3: IMPORTANT - Resource Management (Priority: MEDIUM)

**events.rs** - Callback Results
1. ❌ Update image cache and send to WebRender (line 863)
2. ❌ Start/stop timers (line 869)
3. ❌ Start/stop threads (line 875)

**wr_translate2.rs** - WebRender Integration
1. ❌ Implement shader cache (line 72)
2. ❌ Translate debug flags from options (line 94)
3. ❌ Proper texture lookup using get_opengl_texture (line 116)
4. ❌ Re-enable iframe hit testing (line 318)
5. ❌ Re-enable scroll hit testing once scrollable_nodes available (line 354)
6. ❌ Scroll all nodes - requires scroll_states integration (line 494)
7. ❌ Synchronize GPU values (transforms, opacities) (line 497)

**Impact**: Resource leaks, GPU state not synchronized, iframes don't work, scroll doesn't work.

### Category 4: IMPORTANT - Window Management (Priority: MEDIUM)

**mod.rs** - Window State
1. ❌ Proper window ID tracking instead of 0 (line 849)
2. ❌ Implement redraw request (line 1425)

**Impact**: Multi-window support broken, manual redraw requests don't work.

### Category 5: LOW PRIORITY - Platform Stubs (Priority: LOW)

**windows/mod.rs** - Windows Implementation
1. ❌ Implement in Phase 4 (entire file)
2. ❌ Win32 window handle (line 11)
3. ❌ Add tests in Phase 4 (line 20)

**linux/x11/mod.rs** - X11 Implementation
1. ❌ Implement in Phase 3 (entire file)
2. ❌ X11 window handle (line 11)
3. ❌ Add tests in Phase 3 (line 20)

**linux/wayland/mod.rs** - Wayland Implementation
1. ❌ Implement in Phase 5 (entire file)
2. ❌ Wayland window handle (line 11)
3. ❌ Add tests in Phase 5 (line 20)

**shell2/mod.rs** - Platform Routing
1. ❌ Implement Windows in Phase 4 (line 69)
2. ❌ Implement X11/Wayland in Phase 3/5 (line 74)

**Impact**: macOS-only, no Windows/Linux support yet (planned for future phases).

### Category 6: LOW PRIORITY - Additional Features (Priority: LOW)

**menu.rs** - Menu System
1. ❌ Set action and target for callback dispatch (line 97)
2. ❌ Set keyboard accelerator if present (line 102)
3. ❌ Add more tests for menu creation (line 147)

**common/compositor.rs** - Hardware Detection
1. ❌ Implement actual OpenGL detection (line 136)
2. ❌ Detect Vulkan support (line 142)

**common/cpu_compositor.rs** - Software Rendering
1. ❌ Implement based on webrender's sw_compositor.rs (line 5)
2. ❌ Implement actual rasterization (line 57)

**Impact**: Menu callbacks don't work, hardware detection is stubbed, software rendering not implemented.

---

## Priority Ranking

### P0: BLOCKING (Must fix immediately for basic functionality)

1. **Text rendering** (compositor2:222) - Without this, nothing displays
2. **Scroll wheel handling** (events.rs:760) - Basic scrolling broken
3. **Window resize** (events.rs:824-825) - Window can't be resized
4. **Keyboard state** (events.rs:492) - Keyboard input broken
5. **Clip stack** (compositor2:185,192) - Clipping doesn't work

**Estimated effort**: 2-3 days

### P1: CRITICAL (Breaks core features)

6. **Image rendering** (compositor2:226) - No images display
7. **Border rendering** (compositor2:122) - UI looks wrong
8. **Border radius** (compositor2:66,185) - Rounded corners don't work
9. **DOM hit-testing tags** (compositor2:218) - Click events broken
10. **RawWindowHandle** (events.rs:564) - Proper window handle needed
11. **Scroll frames** (compositor2:200,205) - Nested scrolling broken
12. **GPU value sync** (wr_translate2:497) - Animations/transforms broken

**Estimated effort**: 3-4 days

### P2: IMPORTANT (Quality of life)

13. **Image cache updates** (events.rs:863) - Image changes don't update
14. **Timer management** (events.rs:869) - Timers don't work
15. **Thread management** (events.rs:875) - Background tasks broken
16. **Window ID tracking** (mod.rs:849) - Multi-window issues
17. **Redraw requests** (mod.rs:1425) - Manual redraws don't work
18. **Key callbacks** (events.rs:775,785) - Key events not filtered
19. **FileDrop callbacks** (events.rs:795) - Drag-drop broken
20. **Scrollbar track jump** (events.rs:107) - Click on track doesn't jump

**Estimated effort**: 2-3 days

### P3: ENHANCEMENT (Nice to have)

21. **IFrame embedding** (compositor2:230) - Nested DOMs
22. **IFrame hit-testing** (wr_translate2:318) - IFrame events
23. **Scroll hit-testing** (wr_translate2:354) - Better scroll detection
24. **Menu callbacks** (menu.rs:97,102) - Menu system
25. **Shader cache** (wr_translate2:72) - Performance
26. **Debug flags** (wr_translate2:94) - Debugging tools
27. **Texture lookup** (wr_translate2:116) - Better texture handling

**Estimated effort**: 2-3 days

### P4: FUTURE (Platform support)

28. **Windows implementation** - Phase 4
29. **X11 implementation** - Phase 3
30. **Wayland implementation** - Phase 5
31. **Hardware detection** - GPU/Vulkan
32. **CPU compositor** - Software rendering
33. **Menu tests** - Test coverage

**Estimated effort**: 2-3 weeks per platform

---

## Implementation Plan

### Phase A: Core Rendering (Days 1-3)

**Goal**: Make basic UI actually render

**Order**:
1. Text rendering (P0)
   - Integrate with webrender's glyph atlas
   - Map FontRef to webrender FontInstanceKey
   - Push glyph instances to display list
   
2. Clip stack management (P0)
   - Track clip_id stack in builder
   - Push/pop clips correctly
   - Apply clips to all items
   
3. Border rendering (P1)
   - Implement webrender border sides
   - Map BorderRadius to ComplexClipRegion
   - Handle border colors/widths
   
4. Border radius (P1)
   - Add ComplexClipRegion for rounded corners
   - Apply to Rect, PushClip

5. Image rendering (P1)
   - Look up ImageKey in resource cache
   - Push image items to display list

**Deliverable**: UI with text, images, borders, rounded corners

### Phase B: Event System (Days 4-5)

**Goal**: Fix broken event handling

**Order**:
1. Scroll wheel handling (P0)
   - Call gpu_scroll() on scroll wheel events
   - Update scroll position in scroll_states
   
2. Window resize (P0)
   - Update WebRender viewport
   - Resize GL framebuffer or CPU buffer
   - Trigger relayout
   
3. Keyboard state (P0)
   - Track pressed keys in keyboard_state
   - Update on keyDown/keyUp
   
4. RawWindowHandle (P1)
   - Get proper handle from NSWindow
   - Pass to callbacks correctly
   
5. DOM hit-testing tags (P1)
   - Encode NodeId in ItemTag
   - Attach to HitTestArea items
   - Decode in hit-test results

**Deliverable**: Scroll wheel, resize, keyboard, click events all work

### Phase C: Scroll Frames (Days 6-7)

**Goal**: Implement nested scrolling

**Order**:
1. Scroll frames (P1)
   - Create new SpatialId for scroll content
   - Define scroll frame in WebRender
   - Track spatial_id stack
   
2. GPU value sync (P1)
   - Sync transforms from gpu_state_manager
   - Sync opacities from gpu_state_manager
   - Apply to display list items

**Deliverable**: Nested scrolling, GPU animations work

### Phase D: Resource Management (Days 8-9)

**Goal**: Handle dynamic resources

**Order**:
1. Image cache updates (P2)
   - Add/update/delete images in WebRender
   - Send resource updates in transaction
   
2. Timer management (P2)
   - Start timers from callback results
   - Stop timers from callback results
   - Dispatch timer events
   
3. Thread management (P2)
   - Start threads from callback results
   - Track running threads
   - Handle thread completion

**Deliverable**: Dynamic images, timers, background tasks

### Phase E: Quality of Life (Days 10-11)

**Goal**: Polish remaining issues

**Order**:
1. Window ID tracking (P2)
   - Assign unique IDs to windows
   - Track in global registry
   - Pass to callbacks
   
2. Manual redraw (P2)
   - Implement redraw_request()
   - Set frame_needs_regeneration
   - Call generate_frame_if_needed()
   
3. Key callback filtering (P2)
   - Look up key callbacks in styled_dom
   - Filter by key code
   - Dispatch to correct handlers
   
4. FileDrop callbacks (P2)
   - Implement draggingEntered/Updated/Exited
   - Look up FileDrop callbacks
   - Dispatch with file paths
   
5. Scrollbar track jump (P2)
   - Calculate target scroll position from click
   - Animate or instant jump
   - Call gpu_scroll()

**Deliverable**: All events, multi-window, manual redraws

### Phase F: Advanced Features (Days 12-14)

**Goal**: IFrames, optimization

**Order**:
1. IFrame embedding (P3)
   - Look up child DOM by child_dom_id
   - Create nested pipeline
   - Embed child display list
   
2. IFrame hit-testing (P3)
   - Transform hit-test coords to iframe space
   - Test child DOM
   - Return iframe hit info
   
3. Performance optimizations (P3)
   - Shader cache
   - Better texture lookup
   - Menu callback dispatch

**Deliverable**: IFrames work, better performance

---

## Quick Wins (Can do today)

These are simple fixes that take <30 minutes each:

1. **Window ID tracking** - Just increment a static counter
2. **Scrollbar track jump** - Calculate position math
3. **Redraw request** - Set flag and call generate_frame
4. **Keyboard state updates** - Simple hash map insert/remove

**Total time**: 2 hours for all quick wins

---

## Critical Path Analysis

**Must have for MVP**:
- Text rendering (P0)
- Scroll wheel (P0)
- Window resize (P0)
- Keyboard state (P0)
- Clip stack (P0)
- Image rendering (P1)
- Border rendering (P1)
- DOM hit-testing (P1)

**Can defer**:
- IFrames
- Advanced GPU features
- Platform-specific code
- Software rendering
- Menu system

**Total MVP time**: ~1 week (5-7 days) working full time

---

## Recommendation

**Start with Phase A (Core Rendering)** - This unblocks everything else. Without text rendering, you can't even see what you're building.

**Then Phase B (Event System)** - Makes the UI actually interactive.

**Then Phase C-E as needed** - Based on what features are most critical for your use case.

**Skip Phase F for now** - Advanced features can wait until core is solid.

## Next Action

Would you like me to:
1. **Start with quick wins** (2 hours, immediate improvement)
2. **Implement Phase A** (Text + borders + images, 2-3 days)
3. **Implement Phase B** (Event system, 1-2 days)
4. **Focus on specific feature** (Tell me which TODO is most important)
