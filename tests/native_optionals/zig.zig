// Zig: `null` (or the value) where an Option is asked for, `?T` back from
// fields and downcasts (`orelse return .DoNothing`), an error union for a
// Result (`try` in a `!azul.Update` callback: the error is logged and the
// callback does nothing). Built and run by scripts/native_optionals_smoke.sh.
const std = @import("std");
const azul = @import("azul.zig");

const Model = struct {
    folder: []u8 = &.{},
    link: []const u8 = "no scheme",
};
const ModelRef = azul.ReflectModel(Model).Ref;

// The guide's file-dialog example, as written there (hello-world/zig.md).
fn onChooseFolder(model: ModelRef, _: azul.CallbackInfo) azul.Update {
    _ = azul.FileDialog.openDirectory("Choose a folder", null, model.clone(), onPickFolder);
    return .DoNothing;
}

fn onPickFolder(model: ModelRef, _: azul.CallbackInfo, result: azul.C.AzRefAny) !azul.Update {
    var picked = azul.FileOpenResult.downcast(result) orelse return .DoNothing;
    defer picked.deinit();
    var path = picked.getPath() orelse return .DoNothing; // cancelled
    defer path.deinit();
    model.get().folder = try path.getInner(std.heap.page_allocator);
    return .RefreshDom;
}

fn onOpenLink(model: ModelRef, _: azul.CallbackInfo) !azul.Update {
    var url = try azul.Url.parse(model.get().link); // an Err is logged, the callback does nothing
    defer url.deinit();
    return .RefreshDom;
}

fn check(ok: bool, what: []const u8) void {
    if (!ok) {
        std.debug.print("FAIL: {s}\n", .{what});
        std.process.exit(1);
    }
}

fn text(s: azul.String) []const u8 {
    return s.inner.vec.ptr[0..s.inner.vec.len];
}

pub fn main() void {
    // A field: null in, null out; a value in, the value out.
    var icons = azul.IconVariants.createDefault();
    check(icons.getLight() == null, "a default IconVariants has no light icon");
    icons.setLight("light.png");
    const light = icons.getLight() orelse return check(false, "Some in, Some out");
    check(std.mem.eql(u8, text(light), "light.png"), "Some in, Some out");
    icons.setLight(null);
    check(icons.getLight() == null, "None in, None out");

    // A result: Ok is the value, Err an error.
    var ok = azul.Url.parse("https://azul.rs/guide") catch return check(false, "Url.parse Ok");
    ok.deinit();
    if (azul.Url.parse("no scheme")) |_| check(false, "Url.parse Err") else |err| check(err == error.UrlParseError, "Url.parse Err");

    // The callbacks compile against the kinds that take them.
    _ = azul._asCallback(azul.C.AzResumeCallbackType, onPickFolder);
    _ = azul._asCallback(azul.C.AzCallbackType, onChooseFolder);
    _ = azul._asCallback(azul.C.AzCallbackType, onOpenLink);
    std.debug.print("zig native optionals: ok\n", .{});
}
