const std = @import("std");
const azul = @import("azul.zig");
const C = azul.C;

const MyDataModel = struct {
    counter: u32,
};

var MY_DATA_TYPE_TOKEN: u8 = 0;
fn myDataTypeId() u64 {
    return @intFromPtr(&MY_DATA_TYPE_TOKEN);
}

fn myDataDestructor(_: ?*anyopaque) callconv(.c) void {}

fn myDataUpcast(model: MyDataModel) C.AzRefAny {

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
        0,
        0,
    );
}

fn myDataDowncast(refany: *const C.AzRefAny) ?*MyDataModel {
    if (!C.AzRefAny_isType(refany, myDataTypeId())) return null;
    const ptr = C.AzRefAny_getDataPtr(refany) orelse return null;
    return @constCast(@as(*const MyDataModel, @ptrCast(@alignCast(ptr))));
}

fn onClick(data: C.AzRefAny, _: C.AzCallbackInfo) callconv(.c) C.AzUpdate {
    var d = data;
    const m = myDataDowncast(&d) orelse return C.AzUpdate_DoNothing;
    m.counter += 1;
    return C.AzUpdate_RefreshDom;
}

fn layout(data: C.AzRefAny, _: C.AzLayoutCallbackInfo) callconv(.c) C.AzDom {
    var d = data;
    const m = myDataDowncast(&d) orelse return C.AzDom_createBody();

    var buf: [16]u8 = undefined;
    const slice = std.fmt.bufPrint(&buf, "{d}", .{m.counter}) catch return C.AzDom_createBody();
    const counter_str = C.AzString_fromUtf8(slice.ptr, slice.len);
    var label = C.AzDom_createPWithText(counter_str);
    const css = "font-size: 32px;";
    C.AzDom_setCss(&label, C.AzString_fromUtf8(css.ptr, css.len));

    const btn_label_bytes = "Increase counter";
    const btn_label = C.AzString_fromUtf8(btn_label_bytes.ptr, btn_label_bytes.len);
    var button = C.AzButton_create(btn_label);
    C.AzButton_setButtonType(&button, C.AzButtonType_Primary);
    const data_clone = C.AzRefAny_clone(&d);
    C.AzButton_setOnClick(&button, data_clone, onClick);
    const button_dom = C.AzButton_dom(button);

    var body = C.AzDom_createBody();
    C.AzDom_addChild(&body, label);
    C.AzDom_addChild(&body, button_dom);
    return body;
}

pub fn main() !void {
    const model = MyDataModel{ .counter = 5 };
    const data = myDataUpcast(model);

    var window = C.AzWindowCreateOptions_create(layout);
    const title_bytes = "Hello World";
    window.window_state.title = C.AzString_fromUtf8(title_bytes.ptr, title_bytes.len);
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;

    var app = C.AzApp_create(data, C.AzAppConfig_create());
    C.AzApp_run(&app, window);
}
