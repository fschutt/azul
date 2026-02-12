#include "azul.h"
#include <stdio.h>
#include <string.h>

/*
 * Drag-and-Drop Test Example
 *
 * Layout (like an HTML page):
 *
 *   ┌─────────────────────────────────────────────────┐
 *   │ Drag & Drop Test                                │
 *   ├─────────────────────────────────────────────────┤
 *   │                                                 │
 *   │  ┌──────────────┐                               │
 *   │  │  Drag Me     │  (draggable=true, blue box)   │
 *   │  └──────────────┘                               │
 *   │                                                 │
 *   │  ┌──────────────────┐  ┌──────────────────┐     │
 *   │  │  Drop Zone A     │  │  Drop Zone B     │     │
 *   │  │  (text/plain)    │  │  (text/html)     │     │
 *   │  │                  │  │                  │     │
 *   │  │                  │  │                  │     │
 *   │  └──────────────────┘  └──────────────────┘     │
 *   │                                                 │
 *   │  Status: <status text updates here>             │
 *   └─────────────────────────────────────────────────┘
 *
 * This example tests whether:
 * 1. DragStart / Drag / DragEnd events fire on draggable nodes
 * 2. MouseOver / MouseEnter / MouseLeave fire on drop zones
 * 3. isDragging / getDragState work in callbacks
 */

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

// ── Data model ──────────────────────────────────────────────────────────

typedef struct {
    char status[512];
    int drag_start_count;
    int drag_count;
    int drag_end_count;
    int zone_a_enter_count;
    int zone_a_leave_count;
    int zone_b_enter_count;
    int zone_b_leave_count;
} DragDropModel;

void DragDropModel_destructor(void* m) { (void)m; }

AzJson DragDropModel_toJson(AzRefAny refany);
AzResultRefAnyString DragDropModel_fromJson(AzJson json);
AZ_REFLECT_JSON(DragDropModel, DragDropModel_destructor, DragDropModel_toJson, DragDropModel_fromJson);

AzJson DragDropModel_toJson(AzRefAny refany) {
    (void)refany;
    return AzJson_null();
}

AzResultRefAnyString DragDropModel_fromJson(AzJson json) {
    (void)json;
    DragDropModel model = {0};
    snprintf(model.status, sizeof(model.status), "Waiting for drag...");
    return AzResultRefAnyString_ok(DragDropModel_upcast(model));
}

// ── Callbacks ───────────────────────────────────────────────────────────

// Called when a drag gesture starts on the draggable box
AzUpdate on_drag_start(AzRefAny data, AzCallbackInfo info) {
    DragDropModelRefMut d = DragDropModelRefMut_create(&data);
    if (!DragDropModel_downcastMut(&data, &d)) {
        fprintf(stderr, "[DRAG-TEST] on_drag_start: downcast FAILED\n");
        return AzUpdate_DoNothing;
    }

    d.ptr->drag_start_count += 1;
    bool is_dragging = AzCallbackInfo_isDragging(&info);
    bool is_drag_active = AzCallbackInfo_isDragActive(&info);
    bool is_node_drag = AzCallbackInfo_isNodeDragActive(&info);

    AzDomNodeId hit = AzCallbackInfo_getHitNode(&info);
    AzOptionLogicalPosition cursor_opt = AzCallbackInfo_getCursorPosition(&info);

    float cx = 0, cy = 0;
    if (!AzOptionLogicalPosition_isNone(&cursor_opt)) {
        cx = cursor_opt.Some.payload.x;
        cy = cursor_opt.Some.payload.y;
    }

    snprintf(d.ptr->status, sizeof(d.ptr->status),
        "DragStart #%d | isDragging=%d isDragActive=%d isNodeDrag=%d | "
        "hitNode=(dom=%zu,node=%zu) | cursor=(%.1f, %.1f)",
        d.ptr->drag_start_count,
        is_dragging, is_drag_active, is_node_drag,
        hit.dom.inner, hit.node.inner,
        cx, cy);

    fprintf(stderr, "[DRAG-TEST] %s\n", d.ptr->status);

    // Also try to get drag state
    AzOptionDragState drag_state_opt = AzCallbackInfo_getDragState(&info);
    if (!AzOptionDragState_isNone(&drag_state_opt)) {
        AzDragState ds = drag_state_opt.Some.payload;
        fprintf(stderr, "[DRAG-TEST]   DragState: type=%d\n", ds.drag_type);
        if (!AzOptionDomNodeId_isNone(&ds.source_node)) {
            fprintf(stderr, "[DRAG-TEST]   source_node=(dom=%zu,node=%zu)\n",
                ds.source_node.Some.payload.dom.inner,
                ds.source_node.Some.payload.node.inner);
        }
    } else {
        fprintf(stderr, "[DRAG-TEST]   DragState: None\n");
    }

    DragDropModelRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

// Called during continuous drag movement
AzUpdate on_drag(AzRefAny data, AzCallbackInfo info) {
    DragDropModelRefMut d = DragDropModelRefMut_create(&data);
    if (!DragDropModel_downcastMut(&data, &d)) {
        fprintf(stderr, "[DRAG-TEST] on_drag: downcast FAILED\n");
        return AzUpdate_DoNothing;
    }

    d.ptr->drag_count += 1;
    bool is_dragging = AzCallbackInfo_isDragging(&info);

    AzOptionLogicalPosition cursor_opt = AzCallbackInfo_getCursorPosition(&info);
    float cx = 0, cy = 0;
    if (!AzOptionLogicalPosition_isNone(&cursor_opt)) {
        cx = cursor_opt.Some.payload.x;
        cy = cursor_opt.Some.payload.y;
    }

    // Only update status text every 10th drag event to avoid spam
    if (d.ptr->drag_count % 10 == 0) {
        snprintf(d.ptr->status, sizeof(d.ptr->status),
            "Drag #%d | isDragging=%d | cursor=(%.1f, %.1f)",
            d.ptr->drag_count, is_dragging, cx, cy);
    }

    // Always log to stderr
    fprintf(stderr, "[DRAG-TEST] Drag #%d | isDragging=%d | cursor=(%.1f, %.1f)\n",
        d.ptr->drag_count, is_dragging, cx, cy);

    DragDropModelRefMut_delete(&d);
    return (d.ptr->drag_count % 10 == 0) ? AzUpdate_RefreshDom : AzUpdate_DoNothing;
}

// Called when drag ends (mouse released)
AzUpdate on_drag_end(AzRefAny data, AzCallbackInfo info) {
    DragDropModelRefMut d = DragDropModelRefMut_create(&data);
    if (!DragDropModel_downcastMut(&data, &d)) {
        fprintf(stderr, "[DRAG-TEST] on_drag_end: downcast FAILED\n");
        return AzUpdate_DoNothing;
    }

    d.ptr->drag_end_count += 1;
    bool is_dragging = AzCallbackInfo_isDragging(&info);

    AzOptionLogicalPosition cursor_opt = AzCallbackInfo_getCursorPosition(&info);
    float cx = 0, cy = 0;
    if (!AzOptionLogicalPosition_isNone(&cursor_opt)) {
        cx = cursor_opt.Some.payload.x;
        cy = cursor_opt.Some.payload.y;
    }

    snprintf(d.ptr->status, sizeof(d.ptr->status),
        "DragEnd #%d | isDragging=%d | cursor=(%.1f, %.1f) | "
        "totals: starts=%d drags=%d ends=%d",
        d.ptr->drag_end_count, is_dragging, cx, cy,
        d.ptr->drag_start_count, d.ptr->drag_count, d.ptr->drag_end_count);

    fprintf(stderr, "[DRAG-TEST] %s\n", d.ptr->status);

    DragDropModelRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

// Called when mouse enters Drop Zone A
AzUpdate on_zone_a_enter(AzRefAny data, AzCallbackInfo info) {
    DragDropModelRefMut d = DragDropModelRefMut_create(&data);
    if (!DragDropModel_downcastMut(&data, &d)) return AzUpdate_DoNothing;

    d.ptr->zone_a_enter_count += 1;
    bool is_dragging = AzCallbackInfo_isDragging(&info);

    snprintf(d.ptr->status, sizeof(d.ptr->status),
        "Zone A: MouseEnter #%d | isDragging=%d",
        d.ptr->zone_a_enter_count, is_dragging);

    fprintf(stderr, "[DRAG-TEST] %s\n", d.ptr->status);
    DragDropModelRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

// Called when mouse leaves Drop Zone A
AzUpdate on_zone_a_leave(AzRefAny data, AzCallbackInfo info) {
    DragDropModelRefMut d = DragDropModelRefMut_create(&data);
    if (!DragDropModel_downcastMut(&data, &d)) return AzUpdate_DoNothing;

    d.ptr->zone_a_leave_count += 1;
    bool is_dragging = AzCallbackInfo_isDragging(&info);

    snprintf(d.ptr->status, sizeof(d.ptr->status),
        "Zone A: MouseLeave #%d | isDragging=%d",
        d.ptr->zone_a_leave_count, is_dragging);

    fprintf(stderr, "[DRAG-TEST] %s\n", d.ptr->status);
    DragDropModelRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

// Called when mouse enters Drop Zone B
AzUpdate on_zone_b_enter(AzRefAny data, AzCallbackInfo info) {
    DragDropModelRefMut d = DragDropModelRefMut_create(&data);
    if (!DragDropModel_downcastMut(&data, &d)) return AzUpdate_DoNothing;

    d.ptr->zone_b_enter_count += 1;
    bool is_dragging = AzCallbackInfo_isDragging(&info);

    snprintf(d.ptr->status, sizeof(d.ptr->status),
        "Zone B: MouseEnter #%d | isDragging=%d",
        d.ptr->zone_b_enter_count, is_dragging);

    fprintf(stderr, "[DRAG-TEST] %s\n", d.ptr->status);
    DragDropModelRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

// Called when mouse leaves Drop Zone B
AzUpdate on_zone_b_leave(AzRefAny data, AzCallbackInfo info) {
    DragDropModelRefMut d = DragDropModelRefMut_create(&data);
    if (!DragDropModel_downcastMut(&data, &d)) return AzUpdate_DoNothing;

    d.ptr->zone_b_leave_count += 1;
    bool is_dragging = AzCallbackInfo_isDragging(&info);

    snprintf(d.ptr->status, sizeof(d.ptr->status),
        "Zone B: MouseLeave #%d | isDragging=%d",
        d.ptr->zone_b_leave_count, is_dragging);

    fprintf(stderr, "[DRAG-TEST] %s\n", d.ptr->status);
    DragDropModelRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

// Window-level mouse-down callback for general debugging
AzUpdate on_window_mouse_down(AzRefAny data, AzCallbackInfo info) {
    (void)data;

    AzDomNodeId hit = AzCallbackInfo_getHitNode(&info);
    AzOptionLogicalPosition cursor_opt = AzCallbackInfo_getCursorPosition(&info);
    float cx = 0, cy = 0;
    if (!AzOptionLogicalPosition_isNone(&cursor_opt)) {
        cx = cursor_opt.Some.payload.x;
        cy = cursor_opt.Some.payload.y;
    }

    fprintf(stderr, "[DRAG-TEST] WindowMouseDown: hitNode=(dom=%zu,node=%zu) cursor=(%.1f, %.1f)\n",
        hit.dom.inner, hit.node.inner, cx, cy);

    return AzUpdate_DoNothing;
}

// Window-level drag-start for debugging
AzUpdate on_window_drag_start(AzRefAny data, AzCallbackInfo info) {
    (void)data;

    bool is_dragging = AzCallbackInfo_isDragging(&info);
    bool is_drag_active = AzCallbackInfo_isDragActive(&info);
    bool has_gesture_history = AzCallbackInfo_hasSufficientHistoryForGestures(&info);

    AzOptionLogicalPosition cursor_opt = AzCallbackInfo_getCursorPosition(&info);
    float cx = 0, cy = 0;
    if (!AzOptionLogicalPosition_isNone(&cursor_opt)) {
        cx = cursor_opt.Some.payload.x;
        cy = cursor_opt.Some.payload.y;
    }

    fprintf(stderr, "[DRAG-TEST] WindowDragStart: isDragging=%d isDragActive=%d hasGestureHistory=%d cursor=(%.1f, %.1f)\n",
        is_dragging, is_drag_active, has_gesture_history, cx, cy);

    return AzUpdate_DoNothing;
}

// Window-level drag for debugging
AzUpdate on_window_drag(AzRefAny data, AzCallbackInfo info) {
    (void)data;

    bool is_dragging = AzCallbackInfo_isDragging(&info);

    AzOptionLogicalPosition cursor_opt = AzCallbackInfo_getCursorPosition(&info);
    float cx = 0, cy = 0;
    if (!AzOptionLogicalPosition_isNone(&cursor_opt)) {
        cx = cursor_opt.Some.payload.x;
        cy = cursor_opt.Some.payload.y;
    }

    fprintf(stderr, "[DRAG-TEST] WindowDrag: isDragging=%d cursor=(%.1f, %.1f)\n",
        is_dragging, cx, cy);

    return AzUpdate_DoNothing;
}

// Window-level drag-end for debugging
AzUpdate on_window_drag_end(AzRefAny data, AzCallbackInfo info) {
    (void)data;

    bool is_dragging = AzCallbackInfo_isDragging(&info);

    fprintf(stderr, "[DRAG-TEST] WindowDragEnd: isDragging=%d\n", is_dragging);

    return AzUpdate_DoNothing;
}

// ── Layout ──────────────────────────────────────────────────────────────

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)info;

    DragDropModelRef d = DragDropModelRef_create(&data);
    if (!DragDropModel_downcastRef(&data, &d)) {
        return AzStyledDom_default();
    }

    char status_buf[600];
    snprintf(status_buf, sizeof(status_buf), "Status: %s", d.ptr->status);

    char stats_buf[256];
    snprintf(stats_buf, sizeof(stats_buf),
        "Stats: starts=%d drags=%d ends=%d | zoneA(enter=%d leave=%d) zoneB(enter=%d leave=%d)",
        d.ptr->drag_start_count, d.ptr->drag_count, d.ptr->drag_end_count,
        d.ptr->zone_a_enter_count, d.ptr->zone_a_leave_count,
        d.ptr->zone_b_enter_count, d.ptr->zone_b_leave_count);

    DragDropModelRef_delete(&d);

    // ── Title ──
    AzDom title = AzDom_withInlineStyle(
        AzDom_h2(AZ_STR("Drag & Drop Test")),
        AZ_STR("margin-bottom: 10px; color: white;")
    );

    // ── Draggable box ──
    // Set draggable="true" attribute + register DragStart/Drag/DragEnd callbacks
    AzRefAny data1 = AzRefAny_clone(&data);
    AzRefAny data2 = AzRefAny_clone(&data);
    AzRefAny data3 = AzRefAny_clone(&data);
    AzDom drag_box = AzDom_withInlineStyle(
        AzDom_createDiv(),
        AZ_STR("width: 150px; height: 60px; background: #3b82f6; color: white; "
               "font-size: 16px; display: flex; align-items: center; "
               "justify-content: center; border-radius: 8px; cursor: grab; "
               "margin-bottom: 20px;")
    );
    drag_box = AzDom_withAttribute(drag_box, AzAttributeType_draggable(true));
    drag_box = AzDom_withChild(drag_box, AzDom_createText(AZ_STR("Drag Me")));
    drag_box = AzDom_withCallback(drag_box,
        AzEventFilter_hover(AzHoverEventFilter_dragStart()),
        data1, on_drag_start);
    drag_box = AzDom_withCallback(drag_box,
        AzEventFilter_hover(AzHoverEventFilter_drag()),
        data2, on_drag);
    drag_box = AzDom_withCallback(drag_box,
        AzEventFilter_hover(AzHoverEventFilter_dragEnd()),
        data3, on_drag_end);

    // ── Drop zones container (flex row) ──
    AzDom zones_container = AzDom_withInlineStyle(
        AzDom_createDiv(),
        AZ_STR("display: flex; flex-direction: row; gap: 20px; margin-bottom: 20px;")
    );

    // ── Drop Zone A ──
    AzRefAny data4 = AzRefAny_clone(&data);
    AzRefAny data5 = AzRefAny_clone(&data);
    AzDom zone_a = AzDom_withInlineStyle(
        AzDom_createDiv(),
        AZ_STR("width: 200px; height: 150px; background: #1e3a5f; "
               "border: 2px dashed #60a5fa; border-radius: 8px; "
               "display: flex; flex-direction: column; align-items: center; "
               "justify-content: center; color: #93c5fd;")
    );
    zone_a = AzDom_withChild(zone_a, AzDom_withInlineStyle(
        AzDom_createText(AZ_STR("Drop Zone A")),
        AZ_STR("font-size: 16px; font-weight: bold;")
    ));
    zone_a = AzDom_withChild(zone_a, AzDom_withInlineStyle(
        AzDom_createText(AZ_STR("(text/plain)")),
        AZ_STR("font-size: 12px; margin-top: 5px; color: #60a5fa;")
    ));
    zone_a = AzDom_withCallback(zone_a,
        AzEventFilter_hover(AzHoverEventFilter_mouseEnter()),
        data4, on_zone_a_enter);
    zone_a = AzDom_withCallback(zone_a,
        AzEventFilter_hover(AzHoverEventFilter_mouseLeave()),
        data5, on_zone_a_leave);

    // ── Drop Zone B ──
    AzRefAny data6 = AzRefAny_clone(&data);
    AzRefAny data7 = AzRefAny_clone(&data);
    AzDom zone_b = AzDom_withInlineStyle(
        AzDom_createDiv(),
        AZ_STR("width: 200px; height: 150px; background: #3b1e0f; "
               "border: 2px dashed #fb923c; border-radius: 8px; "
               "display: flex; flex-direction: column; align-items: center; "
               "justify-content: center; color: #fdba74;")
    );
    zone_b = AzDom_withChild(zone_b, AzDom_withInlineStyle(
        AzDom_createText(AZ_STR("Drop Zone B")),
        AZ_STR("font-size: 16px; font-weight: bold;")
    ));
    zone_b = AzDom_withChild(zone_b, AzDom_withInlineStyle(
        AzDom_createText(AZ_STR("(text/html)")),
        AZ_STR("font-size: 12px; margin-top: 5px; color: #fb923c;")
    ));
    zone_b = AzDom_withCallback(zone_b,
        AzEventFilter_hover(AzHoverEventFilter_mouseEnter()),
        data6, on_zone_b_enter);
    zone_b = AzDom_withCallback(zone_b,
        AzEventFilter_hover(AzHoverEventFilter_mouseLeave()),
        data7, on_zone_b_leave);

    AzDom_addChild(&zones_container, zone_a);
    AzDom_addChild(&zones_container, zone_b);

    // ── Status display ──
    AzDom status_text = AzDom_withInlineStyle(
        AzDom_createText(AzString_copyFromBytes(
            (const uint8_t*)status_buf, 0, strlen(status_buf))),
        AZ_STR("font-size: 14px; color: #e2e8f0; background: #1e293b; "
               "padding: 10px; border-radius: 4px; font-family: monospace;")
    );

    AzDom stats_text = AzDom_withInlineStyle(
        AzDom_createText(AzString_copyFromBytes(
            (const uint8_t*)stats_buf, 0, strlen(stats_buf))),
        AZ_STR("font-size: 12px; color: #94a3b8; background: #1e293b; "
               "padding: 8px; border-radius: 4px; margin-top: 5px; "
               "font-family: monospace;")
    );

    // ── Body ──
    AzDom body = AzDom_withInlineStyle(
        AzDom_createBody(),
        AZ_STR("padding: 20px; background: #0f172a; font-family: sans-serif;")
    );

    // Register WINDOW-level drag events for global debugging
    AzRefAny data_w1 = AzRefAny_clone(&data);
    AzRefAny data_w2 = AzRefAny_clone(&data);
    AzRefAny data_w3 = AzRefAny_clone(&data);
    AzRefAny data_w4 = AzRefAny_clone(&data);
    body = AzDom_withCallback(body,
        AzEventFilter_window(AzWindowEventFilter_leftMouseDown()),
        data_w1, on_window_mouse_down);
    body = AzDom_withCallback(body,
        AzEventFilter_window(AzWindowEventFilter_dragStart()),
        data_w2, on_window_drag_start);
    body = AzDom_withCallback(body,
        AzEventFilter_window(AzWindowEventFilter_drag()),
        data_w3, on_window_drag);
    body = AzDom_withCallback(body,
        AzEventFilter_window(AzWindowEventFilter_dragEnd()),
        data_w4, on_window_drag_end);

    AzDom_addChild(&body, title);
    AzDom_addChild(&body, drag_box);
    AzDom_addChild(&body, zones_container);
    AzDom_addChild(&body, status_text);
    AzDom_addChild(&body, stats_text);

    return AzDom_style(&body, AzCss_empty());
}

// ── Main ────────────────────────────────────────────────────────────────

int main() {
    DragDropModel model = {0};
    snprintf(model.status, sizeof(model.status), "Waiting for drag...");
    AzRefAny data = DragDropModel_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Drag & Drop Test");
    window.window_state.size.dimensions.width = 500.0;
    window.window_state.size.dimensions.height = 450.0;

    // Use software CSD titlebar (same as hello-world)
    window.window_state.flags.decorations = AzWindowDecorations_NoTitleAutoInject;
    window.window_state.flags.background_material = AzWindowBackgroundMaterial_Sidebar;

    fprintf(stderr, "[DRAG-TEST] Starting drag-drop test app...\n");
    fprintf(stderr, "[DRAG-TEST] Events logged with [DRAG-TEST] prefix to stderr.\n");
    fprintf(stderr, "[DRAG-TEST] Try: click+drag the blue box, hover over drop zones.\n");

    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);

    return 0;
}
