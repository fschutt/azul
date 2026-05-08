// examples/zig/hello-world.zig
//
// Zig port of examples/c/hello-world.c.
//
// Same data model (a `MyDataModel` struct with a uint32 counter), same
// callback semantics (mouse click increments, layout renders the
// counter and a button).
//
// ---------------------------------------------------------------------
// What you need next to this file:
//
//   * `azul.zig` — the generated bindings.
//   * `azul.h`   — the C header (parsed by `@cImport` inside `azul.zig`).
//   * the prebuilt native library:
//        * Linux:   `libazul.so`
//        * macOS:   `libazul.dylib`
//        * Windows: `azul.dll`
//
// Build:    zig build
// Run:      zig build run
// ---------------------------------------------------------------------

const std = @import("std");
const azul = @import("azul.zig");
const C = azul.C;

// ── Data model ─────────────────────────────────────────────────────────
//
// The data lives in a heap-allocated `AzRefAny` so the framework can
// share ownership between the layout callback, the on-click callback,
// and the application root.

const MyDataModel = extern struct {
    counter: u32,
};

// Stable, arbitrary 64-bit RTTI id for `MyDataModel`. Any value works
// as long as no other RefAny in the process uses the same id; pick a
// high one to avoid clashing with built-in azul types.
const MY_DATA_MODEL_RTTI_ID: u64 = 0xA2010001;

fn modelDestructor(_: ?*anyopaque) callconv(.C) void {
    // MyDataModel owns no heap memory — nothing to free.
}

fn azString(literal: []const u8) C.AzString {
    return C.AzString_copyFromBytes(literal.ptr, 0, literal.len);
}

fn upcastModel(model: *MyDataModel) C.AzRefAny {
    const name = azString("MyDataModel");
    return C.AzRefAny_newC(
        @as(?*anyopaque, @ptrCast(model)),
        @sizeOf(MyDataModel),
        @alignOf(MyDataModel),
        MY_DATA_MODEL_RTTI_ID,
        name,
        modelDestructor,
    );
}

fn downcastMut(refany: *C.AzRefAny) ?*MyDataModel {
    if (!C.AzRefAny_isType(refany, MY_DATA_MODEL_RTTI_ID)) {
        return null;
    }
    const ptr = C.AzRefAny_getDataPtr(refany);
    return @as(?*MyDataModel, @ptrCast(@alignCast(ptr)));
}

// ── Callback: increment counter on click ──────────────────────────────

fn onClick(data: C.AzRefAny, info: C.AzCallbackInfo) callconv(.C) C.AzUpdate {
    _ = info;
    var data_local = data;
    const m = downcastMut(&data_local) orelse return C.AzUpdate_DoNothing;
    m.counter += 1;
    return C.AzUpdate_RefreshDom;
}

// ── Layout callback ───────────────────────────────────────────────────

fn layout(data: C.AzRefAny, info: C.AzLayoutCallbackInfo) callconv(.C) C.AzDom {
    _ = info;
    var data_local = data;
    const m = downcastMut(&data_local) orelse return C.AzDom_createBody();

    // Counter label, wrapped in a div so it lays out as block.
    var buf: [20]u8 = undefined;
    const written = std.fmt.bufPrint(&buf, "{d}", .{m.counter}) catch buf[0..0];
    const txt = C.AzString_copyFromBytes(buf[0..].ptr, 0, written.len);
    var label = C.AzDom_createText(txt);
    var label_wrapper = C.AzDom_createDiv();
    C.AzDom_addCssProperty(
        &label_wrapper,
        C.AzCssPropertyWithConditions_simple(
            C.AzCssProperty_fontSize(C.AzStyleFontSize_px(32.0)),
        ),
    );
    C.AzDom_addChild(&label_wrapper, label);

    // Button.
    const button_text = azString("Increase counter");
    var button = C.AzButton_create(button_text);
    C.AzButton_setButtonType(&button, C.AzButtonType_Primary);
    const data_clone = C.AzRefAny_clone(&data_local);
    C.AzButton_setOnClick(&button, data_clone, onClick);
    const button_dom = C.AzButton_dom(button);

    // Body.
    var body = C.AzDom_createBody();
    C.AzDom_addChild(&body, label_wrapper);
    C.AzDom_addChild(&body, button_dom);

    return C.AzDom_style(body, C.AzCss_empty());
}

// ── Main ──────────────────────────────────────────────────────────────

pub fn main() !void {
    var model = MyDataModel{ .counter = 5 };
    const data = upcastModel(&model);

    var window = C.AzWindowCreateOptions_create(layout);
    window.window_state.title = azString("Hello World");
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;

    // NoTitleAutoInject: OS draws close/min/max buttons,
    // framework auto-injects a Titlebar with drag support.
    window.window_state.flags.decorations = C.AzWindowDecorations_NoTitleAutoInject;
    window.window_state.flags.background_material = C.AzWindowBackgroundMaterial_Sidebar;

    // Idiomatic Zig: pair `App.create(...)` with `defer app.deinit();`.
    var app = azul.App.create(data, C.AzAppConfig_create());
    defer app.deinit();

    app.run(window);
}
