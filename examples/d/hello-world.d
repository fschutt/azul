// Azul counter example — D.
//
// Build (libazul on the link path, generated azul.d next to this file):
//   dmd hello-world.d azul.d -L-L. -L-lazul        // or ldc2

import azul;

struct MyDataModel {
    uint counter;
}

// The type id is the address of a process-wide token (__gshared → one
// instance, not thread-local), matching what the C AZ_REFLECT macro does.
__gshared ubyte MY_DATA_TYPE_TOKEN = 0;

ulong my_data_type_id() {
    return cast(ulong) cast(size_t) &MY_DATA_TYPE_TOKEN;
}

extern(C) void my_data_destructor(void* ptr) {
    // MyDataModel is plain old data: nothing to free.
}

// AzString_fromUtf8 copies the bytes into a refcounted heap buffer, so
// passing a stack/literal pointer is safe.
AzString azString(string s) {
    return AzString_fromUtf8(cast(ubyte*) s.ptr, s.length);
}

AzRefAny my_data_upcast(MyDataModel model) {
    // AzRefAny_newC copies the bytes into its own allocation, so a stack
    // pointer is fine; run_destructor=false = don't free the caller's ptr.
    MyDataModel local = model;
    AzGlVoidPtrConst ptr_wrapper;
    ptr_wrapper.ptr = &local;
    ptr_wrapper.run_destructor = false;
    return AzRefAny_newC(
        ptr_wrapper,
        MyDataModel.sizeof,
        MyDataModel.alignof,
        my_data_type_id(),
        azString("MyDataModel"),
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

// Must be extern(C) — its address is handed to the C-ABI setter directly.
extern(C) AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    AzRefAny d = data;
    MyDataModel* m = my_data_downcast(&d);
    if (m is null) {
        return AzUpdate.DoNothing;
    }
    m.counter += 1;
    return AzUpdate.RefreshDom;
}

// u32 -> decimal into `buf`, returning the length. Avoids pulling in Phobos
// formatting so `layout` stays a leaf extern(C) function.
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

    // Counter label, wrapped in a div so the font-size sticks.
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

    // AzButton_setOnClick takes the bare fn-pointer typedef directly.
    AzButton button = AzButton_create(azString("Increase counter"));
    AzButton_setButtonType(&button, AzButtonType.Primary);
    AzRefAny data_clone = AzRefAny_clone(&d);
    AzButton_setOnClick(&button, data_clone, &on_click);
    AzDom button_dom = AzButton_dom(button);

    AzDom root_body = AzDom_createBody();
    AzDom_addChild(&root_body, label_wrapper);
    AzDom_addChild(&root_body, button_dom);
    return root_body;
}

void main() {
    MyDataModel model;
    model.counter = 5;
    AzRefAny data = my_data_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(&layout);
    window.window_state.title = azString("Hello World");
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;

    // NoTitleAutoInject: OS draws the window buttons; the framework
    // auto-injects a draggable titlebar.
    window.window_state.flags.decorations = AzWindowDecorations.NoTitleAutoInject;
    window.window_state.flags.background_material = AzWindowBackgroundMaterial.Sidebar;

    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);
}
