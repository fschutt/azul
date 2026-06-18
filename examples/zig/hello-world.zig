// zig build run

const std = @import("std");
const azul = @import("azul.zig");
const C = azul.C;

// ── Data model ────────────────────────────────────────────────────────
//
// Mirrors the C macro `AZ_REFLECT_JSON(MyDataModel, ...)`:
//
//   1. A compile-time-unique type id (the address of a `var` we'll
//      never read or write).
//   2. An `upcast` that wraps the struct in an `AzRefAny`.
//   3. A `downcast` that recovers a typed pointer back from the
//      refany.
//
// Plain old data → empty destructor.

const MyDataModel = struct {
    counter: u32,
};

var MY_DATA_TYPE_TOKEN: u8 = 0;
fn myDataTypeId() u64 {
    return @intFromPtr(&MY_DATA_TYPE_TOKEN);
}

fn myDataDestructor(_: ?*anyopaque) callconv(.c) void {}

fn myDataUpcast(model: MyDataModel) C.AzRefAny {
    // `AzRefAny_newC` copies the bytes into its own heap allocation,
    // so handing it a stack pointer is fine. `run_destructor=false`
    // means libazul won't try to free the caller's pointer when it
    // copies — only the heap copy is freed (via myDataDestructor +
    // libazul's internal free) when the last clone drops.
    var local = model;
    const type_name_bytes = "MyDataModel";
    const type_name = C.AzString_fromUtf8(type_name_bytes.ptr, type_name_bytes.len);
    return C.AzRefAny_newC(
        .{ .ptr = @ptrCast(&local), .run_destructor = false },
        @sizeOf(MyDataModel),
        @alignOf(MyDataModel),
        myDataTypeId(),
        type_name,
        myDataDestructor,
        0, // no serialize_fn
        0, // no deserialize_fn
    );
}

fn myDataDowncast(refany: *const C.AzRefAny) ?*MyDataModel {
    if (!C.AzRefAny_isType(refany, myDataTypeId())) return null;
    const ptr = C.AzRefAny_getDataPtr(refany) orelse return null;
    return @constCast(@as(*const MyDataModel, @ptrCast(@alignCast(ptr))));
}

// ── Callback: button click ────────────────────────────────────────────

fn onClick(data: C.AzRefAny, _: C.AzCallbackInfo) callconv(.c) C.AzUpdate {
    var d = data;
    const m = myDataDowncast(&d) orelse return C.AzUpdate_DoNothing;
    m.counter += 1;
    return C.AzUpdate_RefreshDom;
}

// ── Layout callback ───────────────────────────────────────────────────

fn layout(data: C.AzRefAny, _: C.AzLayoutCallbackInfo) callconv(.c) C.AzDom {
    var d = data;
    const m = myDataDowncast(&d) orelse return C.AzDom_createBody();

    // Counter label (wrapped in a div so the font-size sticks).
    var buf: [16]u8 = undefined;
    const slice = std.fmt.bufPrint(&buf, "{d}", .{m.counter}) catch return C.AzDom_createBody();
    const counter_str = C.AzString_fromUtf8(slice.ptr, slice.len);
    const label = C.AzDom_createText(counter_str);

    var label_wrapper = C.AzDom_createDiv();
    const font_size = C.AzStyleFontSize_px(32.0);
    const css_prop = C.AzCssProperty_fontSize(font_size);
    const cond = C.AzCssPropertyWithConditions_simple(css_prop);
    C.AzDom_addCssProperty(&label_wrapper, cond);
    C.AzDom_addChild(&label_wrapper, label);

    // Increment button. `AzCallback_create` wraps the raw fn pointer
    // in a `{ cb, ctx=None }` struct; the C ABI takes `AzCallback`.
    const btn_label_bytes = "Increase counter";
    const btn_label = C.AzString_fromUtf8(btn_label_bytes.ptr, btn_label_bytes.len);
    var button = C.AzButton_create(btn_label);
    C.AzButton_setButtonType(&button, C.AzButtonType_Primary);
    const data_clone = C.AzRefAny_clone(&d);
    const callback = C.AzCallback_create(onClick);
    C.AzButton_setOnClick(&button, data_clone, callback);
    const button_dom = C.AzButton_dom(button);

    // Body.
    var body = C.AzDom_createBody();
    C.AzDom_addChild(&body, label_wrapper);
    C.AzDom_addChild(&body, button_dom);
    return body;
}

// ── Main ──────────────────────────────────────────────────────────────

pub fn main() !void {
    const model = MyDataModel{ .counter = 5 };
    const data = myDataUpcast(model);

    var window = C.AzWindowCreateOptions_create(layout);
    const title_bytes = "Hello World";
    window.window_state.title = C.AzString_fromUtf8(title_bytes.ptr, title_bytes.len);
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;

    // NoTitleAutoInject: OS draws close/min/max buttons; framework
    // auto-injects a Titlebar with drag support.
    window.window_state.flags.decorations = C.AzWindowDecorations_NoTitleAutoInject;
    window.window_state.flags.background_material = C.AzWindowBackgroundMaterial_Sidebar;

    var app = C.AzApp_create(data, C.AzAppConfig_create());
    C.AzApp_run(&app, window);
}
