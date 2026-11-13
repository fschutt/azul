const std = @import("std");
const styles = @import("styles.zig");

pub fn main() void {
    const styleBtn = styles.styleBtn();
    std.debug.print("styleBtn: {d} properties\n", .{styleBtn.len});
}
