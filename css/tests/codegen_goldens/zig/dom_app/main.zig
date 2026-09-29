const std = @import("std");
const styles = @import("styles.zig");

pub fn main() void {
    const renderUi = styles.renderUi();
    std.debug.print("renderUi: {d} properties\n", .{renderUi.len});
}
