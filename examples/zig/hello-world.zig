// examples/zig/hello-world.zig
//
// Minimal Zig smoke test for the Azul C ABI. Confirms that:
//   - the generated `azul.zig` is consumable by Zig's @cImport,
//   - the prebuilt native library loads,
//   - struct-by-value calls and a basic AzString round-trip succeed.
//
// Zig doesn't need the managed-FFI host-invoker plumbing — @cImport
// natively supports struct-by-value calls. Full GUI wiring (Dom
// builders, button click handlers, App.run) requires more
// wrapper-layer machinery that's separate from the C ABI surface
// exercised here.
//
// Build:    zig build
// Run:      zig build run

const std = @import("std");
const azul = @import("azul.zig");
const C = azul.C;

pub fn main() !void {
    // Build a non-empty AzString from a Zig string slice. Exercises
    // the C-side `_fromUtf8` API and a struct-by-value return crossing
    // the @cImport boundary.
    const src = "hello, azul";
    var s = C.AzString_fromUtf8(src.ptr, src.len);
    defer C.AzString_delete(&s);

    // Round-trip through clone to confirm the dylib's heap allocator
    // is wired up — _clone allocates a new buffer.
    var clone = C.AzString_clone(&s);
    defer C.AzString_delete(&clone);

    if (!C.AzString_partialEq(&s, &clone)) {
        std.debug.print("[azul] AzString_clone result not equal to source\n", .{});
        std.process.exit(1);
    }
    std.debug.print("[azul] AzString round-trip succeeded; len={d}\n", .{src.len});

    std.debug.print("[azul] @cImport init phase completed successfully.\n", .{});
    std.debug.print("[azul] (Full App.run wiring requires GUI wrapper-layer work\n", .{});
    std.debug.print("[azul]  separate from the C ABI plumbing exercised here.)\n", .{});
}
