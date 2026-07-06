// Azul counter example — D.
//
// Build (with libazul.{so,dylib}/azul.dll on the link path and the
// generated binding azul.d next to this file):
//
//   dmd hello-world.d azul.d -L-L. -L-lazul        # or ldc2
//
// Callbacks are C-direct: `on_click` and `layout` are plain `extern(C)`
// functions whose address is passed straight to the C-ABI setters — no
// host-invoker, exactly like the C / Zig / Odin bindings.

import azul;

// ── Data model ────────────────────────────────────────────────────────
//
// A compile-time-unique type id (the address of a module global we never
// read/write), plus upcast/downcast to/from an AzRefAny. Plain old data →
// empty destructor.

struct MyDataModel {
    uint counter;
}

__gshared ubyte MY_DATA_TYPE_TOKEN = 0;

ulong my_data_type_id() {
    return cast(ulong) cast(size_t) &MY_DATA_TYPE_TOKEN;
}

extern(C) void my_data_destructor(void* ptr) {
}

AzRefAny my_data_upcast(MyDataModel model) {
    // AzRefAny_newC copies the bytes into its own heap allocation, so a
    // stack pointer is fine here; run_destructor=false means libazul won't
    // free the caller's pointer.
    MyDataModel local = model;
    string type_name_bytes = "MyDataModel";
    AzString type_name = AzString_fromUtf8(
        cast(ubyte*) type_name_bytes.ptr, type_name_bytes.length);
    AzGlVoidPtrConst ptr_wrapper;
    ptr_wrapper.ptr = &local;
    ptr_wrapper.run_destructor = false;
    return AzRefAny_newC(
        ptr_wrapper,
        MyDataModel.sizeof,
        MyDataModel.alignof,
        my_data_type_id(),
        type_name,
        &my_data_destructor,
        0, // no serialize_fn
        0, // no deserialize_fn
    );
}

MyDataModel* my_data_downcast(AzRefAny* refany) {
    if (!AzRefAny_isType(refany, my_data_type_id())) {
        return null;
    }
    void* ptr = AzRefAny_getDataPtr(refany);
    if (ptr is null) {
        return null;
    }
    return cast(MyDataModel*) ptr;
}

// ── Callback: button click ────────────────────────────────────────────

extern(C) AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    AzRefAny d = data;
    MyDataModel* m = my_data_downcast(&d);
    if (m is null) {
        return AzUpdate.DoNothing;
    }
    m.counter += 1;
    return AzUpdate.RefreshDom;
}

// ── Layout callback ───────────────────────────────────────────────────

// u32 -> decimal, written into `buf`; returns the length. Avoids pulling
// in Phobos formatting so `layout` stays a leaf `extern(C)` function.
size_t u32_write(uint n, ubyte[] buf) {
    if (n == 0) {
        buf[0] = '0';
        return 1;
    }
    ubyte[10] tmp;
    size_t i = 0;
    uint v = n;
    while (v > 0) {
        tmp[i] = cast(ubyte)('0' + (v % 10));
        v /= 10;
        i += 1;
    }
    size_t j = 0;
    while (j < i) {
        buf[j] = tmp[i - 1 - j];
        j += 1;
    }
    return i;
}

extern(C) AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzRefAny d = data;
    MyDataModel* m = my_data_downcast(&d);
    if (m is null) {
        return AzDom_createBody();
    }

    // Counter label (wrapped in a div so the font-size sticks).
    ubyte[16] buf;
    size_t n = u32_write(m.counter, buf[]);
    AzString counter_str = AzString_fromUtf8(buf.ptr, n);
    AzDom label = AzDom_createText(counter_str);

    AzDom label_wrapper = AzDom_createDiv();
    AzStyleFontSize font_size = AzStyleFontSize_px(32.0);
    AzCssProperty css_prop = AzCssProperty_fontSize(font_size);
    AzCssPropertyWithConditions cond = AzCssPropertyWithConditions_simple(css_prop);
    AzDom_addCssProperty(&label_wrapper, cond);
    AzDom_addChild(&label_wrapper, label);

    // Increment button. The typed AzButton_setOnClick takes the bare
    // fn-pointer typedef directly — `on_click` is a plain extern(C) fn.
    string btn_label_bytes = "Increase counter";
    AzString btn_label = AzString_fromUtf8(
        cast(ubyte*) btn_label_bytes.ptr, btn_label_bytes.length);
    AzButton button = AzButton_create(btn_label);
    AzButton_setButtonType(&button, AzButtonType.Primary);
    AzRefAny data_clone = AzRefAny_clone(&d);
    AzButton_setOnClick(&button, data_clone, &on_click);
    AzDom button_dom = AzButton_dom(button);

    // Body.
    AzDom root_body = AzDom_createBody();
    AzDom_addChild(&root_body, label_wrapper);
    AzDom_addChild(&root_body, button_dom);
    return root_body;
}

// ── Main ──────────────────────────────────────────────────────────────

void main() {
    MyDataModel model;
    model.counter = 5;
    AzRefAny data = my_data_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(&layout);
    string title_bytes = "Hello World";
    window.window_state.title = AzString_fromUtf8(
        cast(ubyte*) title_bytes.ptr, title_bytes.length);
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;

    // NoTitleAutoInject: OS draws close/min/max buttons; framework
    // auto-injects a Titlebar with drag support.
    window.window_state.flags.decorations = AzWindowDecorations.NoTitleAutoInject;
    window.window_state.flags.background_material = AzWindowBackgroundMaterial.Sidebar;

    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);
}
