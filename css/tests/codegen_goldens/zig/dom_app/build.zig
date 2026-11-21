// Copy target/codegen/azul.zig + azul.h and libazul next to this file, then: zig build run
const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const exe_mod = b.createModule(.{
        .root_source_file = b.path("main.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = true,
    });
    exe_mod.addIncludePath(b.path("."));
    exe_mod.addLibraryPath(b.path("."));
    exe_mod.linkSystemLibrary("azul", .{});
    const exe = b.addExecutable(.{ .name = "azul-app", .root_module = exe_mod });
    b.installArtifact(exe);
    const run = b.addRunArtifact(exe);
    b.step("run", "Run the app").dependOn(&run.step);
}
