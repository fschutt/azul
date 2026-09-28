const std = @import("std");
const azul = @import("azul.zig");

const MyDataModel = struct {
    counter: u32,
};

const MyModelRef = azul.ReflectModel(MyDataModel).Ref;

fn onClick(model: MyModelRef, _: azul.CallbackInfo) azul.Update {
    const m = model.get();
    m.counter += 1;
    return .RefreshDom;
}

fn layout(model: MyModelRef, _: azul.LayoutCallbackInfo) azul.Dom {
    const m = model.get();

    var buf: [16]u8 = undefined;
    const slice = std.fmt.bufPrint(&buf, "{d}", .{m.counter}) catch return azul.Dom.createBody();
    
    var label = azul.Dom.createPWithText(slice);
    label.setCss("font-size: 32px; margin: 0;");

    var button = azul.Button.create("Increase counter");
    button.setButtonType(azul.C.AzButtonType_Primary);
    
    button.setOnClick(model.clone(), onClick);
    
    var body = azul.Dom.createBody();
    body.addChild(label.inner);
    body.addChild(button.dom().inner);
    return body;
}

pub fn main(init: std.process.Init) !void {
    _ = init; 
    
    // Deviation from the guide: a named model type, not `.{ .counter = 5 }` (an anonymous literal has no runtime layout and never matches MyModelRef's type id).
    const data = MyDataModel{ .counter = 5 };

    var window = azul.WindowCreateOptions.create(layout);
    
    window.inner.window_state.title = azul.C.AzString_fromUtf8("Hello World".ptr, 11);
    window.inner.window_state.size.dimensions.width = 400.0;
    window.inner.window_state.size.dimensions.height = 300.0;

    var app = azul.App.create(data, azul.AppConfig.create());
    app.run(window.inner);
}
