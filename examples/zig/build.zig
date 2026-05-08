// build.zig — build manifest for examples/zig/hello-world.zig.
//
// This file is the canonical example output of
// `azul-doc codegen` -> `lang_zig::build_zig::generate_build_zig()`.
// Drop `azul.h`, `azul.zig`, and the prebuilt native library
// (`libazul.so` / `libazul.dylib` / `azul.dll`) into this directory,
// then run `zig build run`.
//
// Tested against Zig 0.11.x and 0.12.x. Older / newer Zig versions
// may require small API tweaks (e.g. `addExecutable` argument shape).

const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const exe = b.addExecutable(.{
        .name = "hello-world",
        .root_source_file = .{ .path = "hello-world.zig" },
        .target = target,
        .optimize = optimize,
    });

    // `@cImport(@cInclude("azul.h"))` needs the header on the C path.
    exe.addIncludePath(.{ .path = "." });

    // `@cImport` requires libc.
    exe.linkLibC();

    // Find and link the prebuilt native library. Adjust the path if the
    // shared library lives elsewhere.
    exe.addLibraryPath(.{ .path = "." });
    exe.linkSystemLibrary("azul");

    b.installArtifact(exe);

    const run_cmd = b.addRunArtifact(exe);
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| {
        run_cmd.addArgs(args);
    }

    const run_step = b.step("run", "Run the example");
    run_step.dependOn(&run_cmd.step);
}
