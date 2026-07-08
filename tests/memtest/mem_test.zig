// Memory test for the azul Zig binding. See tests/memtest/README.md.
//
// The harness (scripts/run_memtest.sh) measures peak RSS across a small and a
// large AZ_MEMTEST_N (RSS that scales with N is a LEAK) and fails on any crash.
// This file only exercises the create/consume/DROP paths in a loop and exits 0.
// No event loop (AzApp_run needs a display and hangs headless).
//
// Talks raw cgo-parsed C against azul.h via `azul.C.*`, like examples/zig.

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
        0, // no serialize_fn
        0, // no deserialize_fn
    );
}

pub fn main() !void {
    var n: usize = 200000;
    if (std.posix.getenv("AZ_MEMTEST_N")) |v| {
        n = std.fmt.parseInt(usize, v, 10) catch 200000;
    }

    // 1. The consume-by-value DROP path: AzApp_create consumes the AppConfig
    //    (whose nested SystemStyle was one of the bitwise-cloned + double-freed
    //    types). AzApp_delete then drops the App exactly once.
    {
        const data = myDataUpcast(MyDataModel{ .counter = 5 });
        var app = C.AzApp_create(data, C.AzAppConfig_create());
        C.AzApp_delete(&app);
    }

    // 2. Leak loop: create/destroy a droppable AppConfig N times. AzAppConfig_delete
    //    drops the nested SystemStyle every iteration.
    var i: usize = 0;
    while (i < n) : (i += 1) {
        var cfg = C.AzAppConfig_create();
        C.AzAppConfig_delete(&cfg);
    }

    // std.debug.print is stable across the 0.11..0.16 std.io churn.
    std.debug.print("memtest zig OK (N={d})\n", .{n});
}
