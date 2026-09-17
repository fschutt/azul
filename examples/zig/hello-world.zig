const std = @import("std");
const azul = @import("azul.zig");
const C = azul.C;

/// A generic, reusable wrapper for ANY data model
pub fn AzulRef(comptime T: type) type {
    return struct {
        // Zig guarantees a unique memory address for this global per instantiated type T
        var TYPE_TOKEN: u8 = 0;

        pub fn typeId() u64 {
            return @intFromPtr(&TYPE_TOKEN);
        }

        fn destructor(_: ?*anyopaque) callconv(.c) void {}

        pub fn upcast(model: T) C.AzRefAny {
            var local = model; // Take a local copy to push to the Azul heap
            const name = @typeName(T);
            return C.AzRefAny_newC(
                .{ .ptr = @ptrCast(&local), .run_destructor = false },
                @sizeOf(T),
                @alignOf(T),
                typeId(),
                C.AzString_fromUtf8(name.ptr, name.len),
                destructor,
                0,
                0,
            );
        }

        pub fn downcast(refany: *const C.AzRefAny) ?*T {
            if (!C.AzRefAny_isType(refany, typeId())) return null;
            const ptr = C.AzRefAny_getDataPtr(refany) orelse return null;
            return @ptrCast(@constCast(@alignCast(ptr)));
        }
    };
}

/// Ergonomic string helper to hide `.ptr` and `.len` verbosity
inline fn azStr(s: []const u8) C.AzString {
    return C.AzString_fromUtf8(s.ptr, s.len);
}

const MyDataModel = struct {
    counter: u32,
};

// We just pass our struct to the wrapper and let Zig generate the boilerplate
const MyModelRef = AzulRef(MyDataModel);

fn onClick(data: C.AzRefAny, _: C.AzCallbackInfo) callconv(.c) C.AzUpdate {
    var d = data;
    const m = MyModelRef.downcast(&d) orelse return C.AzUpdate_DoNothing;
    m.counter += 1;
    
    // std.log.debug("Update action: {t}", .{C.AzUpdate_RefreshDom});
    
    return C.AzUpdate_RefreshDom;
}

fn layout(data: C.AzRefAny, _: C.AzLayoutCallbackInfo) callconv(.c) C.AzDom {
    var d = data;
    const m = MyModelRef.downcast(&d) orelse return C.AzDom_createBody();

    var buf: [16]u8 = undefined;
    const slice = std.fmt.bufPrint(&buf, "{d}", .{m.counter}) catch return C.AzDom_createBody();
    
    var label = C.AzDom_createPWithText(azStr(slice));
    C.AzDom_setCss(&label, azStr("font-size: 32px; margin: 0;"));

    var button = C.AzButton_create(azStr("Increase counter"));
    C.AzButton_setButtonType(&button, C.AzButtonType_Primary);
    C.AzButton_setOnClick(&button, C.AzRefAny_clone(&d), onClick);
    
    var body = C.AzDom_createBody();
    C.AzDom_addChild(&body, label);
    C.AzDom_addChild(&body, C.AzButton_dom(button));
    return body;
}

pub fn main(init: std.process.Init) !void {
    _ = init; 
    
    const data = MyModelRef.upcast(.{ .counter = 5 });

    var window = C.AzWindowCreateOptions_create(layout);
    window.window_state.title = azStr("Hello World");
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;

    var app = C.AzApp_create(data, C.AzAppConfig_create());
    C.AzApp_run(&app, window);
}
