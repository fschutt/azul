const std = @import("std");
const azul = @import("azul.zig");

const MyDataModel = struct {
    counter: u32,
};

const MyModelRef = azul.ReflectModel(MyDataModel).Ref;

fn onClick(model: MyModelRef, _: azul.C.AzCallbackInfo) azul.C.AzUpdate {
    const m = model.get();
    m.counter += 1;
    return azul.C.AzUpdate_RefreshDom;
}

fn layout(model: MyModelRef, _: azul.C.AzLayoutCallbackInfo) azul.C.AzDom {
    const m = model.get();

    var buf: [16]u8 = undefined;
    const slice = std.fmt.bufPrint(&buf, "{d}", .{m.counter}) catch return azul.C.AzDom_createBody();
    
    var label = azul.Dom.createPWithText(slice);
    label.setCss("font-size: 32px; margin: 0;");

    var button = azul.Button.create("Increase counter");
    button.setButtonType(azul.C.AzButtonType_Primary);
    button.setOnClick(model.clone(), onClick);
    
    var body = azul.Dom.createBody();
    body.addChild(label.inner);
    body.addChild(button.dom().inner);
    return body.inner;
}

pub fn main(init: std.process.Init) !void {
    _ = init; 
    
    // Create the model
    const data = .{ .counter = 5 };

    var window = azul.WindowCreateOptions.create(layout);
    window.inner.window_state.title = azul.C.AzString_fromUtf8("Hello World".ptr, 11);
    window.inner.window_state.size.dimensions.width = 400.0;
    window.inner.window_state.size.dimensions.height = 300.0;

    var app = azul.App.create(data, azul.AppConfig.create());
    app.run(window.inner);
}
