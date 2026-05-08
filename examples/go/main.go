// examples/go/main.go
//
// Go (cgo) port of examples/c/hello-world.c.
//
// Same data model (a `MyDataModel` struct with a uint32 counter), same
// callback semantics (mouse click increments, layout renders the
// counter and a button), same visual output.
//
// ---------------------------------------------------------------------
// What you need next to this file:
//
//   * `azul.h`   - the C header (parsed by cgo at build time).
//   * the prebuilt native library:
//        * Linux:   `libazul.so`
//        * macOS:   `libazul.dylib`
//        * Windows: `azul.dll`
//   * the generated Go bindings (`azul.go`, `types.go`, `functions.go`,
//     `wrappers.go`, `go.mod`) imported as the `azul` package.
//
// Build:    CGO_CFLAGS="-I." CGO_LDFLAGS="-L." go build
// Run:      LD_LIBRARY_PATH=. ./hello-world      (Linux)
//           DYLD_LIBRARY_PATH=. ./hello-world    (macOS)
// ---------------------------------------------------------------------

package main

/*
#cgo LDFLAGS: -lazul
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include "azul.h"

// MyDataModel and its plumbing live in the cgo prelude because:
//
//   1. `AZ_REFLECT_JSON` is a C preprocessor macro that emits
//      `MyDataModel_upcast`, `MyDataModel_downcastRef`,
//      `MyDataModel_downcastMut`, `MyDataModelRef_create`,
//      etc. We need those C-side identifiers reachable.
//
//   2. The two callbacks (on_click, layout) are passed to azul as
//      function pointers; they MUST have C calling convention.
//
//   3. cgo can call Go-exported functions from C, but the conversion
//      adds overhead and is fiddly when the function takes by-value
//      C structs (AzCallbackInfo / AzLayoutCallbackInfo). Keeping
//      everything on the C side is cleaner for a faithful port.

typedef struct { uint32_t counter; } MyDataModel;
static void MyDataModel_destructor(void* m) { (void)m; }

static AzJson MyDataModel_toJson(AzRefAny refany);
static AzResultRefAnyString MyDataModel_fromJson(AzJson json);
AZ_REFLECT_JSON(MyDataModel, MyDataModel_destructor, MyDataModel_toJson, MyDataModel_fromJson);

static AzString go_str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

static AzJson MyDataModel_toJson(AzRefAny refany) {
    MyDataModelRef ref = MyDataModelRef_create(&refany);
    if (!MyDataModel_downcastRef(&refany, &ref)) {
        return AzJson_null();
    }
    int64_t counter = (int64_t)ref.ptr->counter;
    MyDataModelRef_delete(&ref);
    return AzJson_int(counter);
}

static AzResultRefAnyString MyDataModel_fromJson(AzJson json) {
    AzOptionI64 counter_opt = AzJson_asInt(&json);
    if (counter_opt.None.tag == AzOptionI64_Tag_None) {
        return AzResultRefAnyString_err(go_str("Expected integer"));
    }
    MyDataModel model = { .counter = (uint32_t)counter_opt.Some.payload };
    return AzResultRefAnyString_ok(MyDataModel_upcast(model));
}

// ── Callback: increment counter on click ─────────────────────────────

static AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    (void)info;
    MyDataModelRefMut d = MyDataModelRefMut_create(&data);
    if (!MyDataModel_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->counter += 1;
    MyDataModelRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

// ── Layout callback ───────────────────────────────────────────────────

static AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)info;
    MyDataModelRef d = MyDataModelRef_create(&data);
    if (!MyDataModel_downcastRef(&data, &d)) {
        return AzDom_createBody();
    }

    char buffer[20];
    int written = snprintf(buffer, 20, "%d", d.ptr->counter);
    MyDataModelRef_delete(&d);

    // Counter label, wrapped in a div so it lays out as block.
    AzString label_text = AzString_copyFromBytes((const uint8_t*)buffer, 0, written);
    AzDom label = AzDom_createText(label_text);
    AzDom label_wrapper = AzDom_createDiv();
    AzDom_addCssProperty(&label_wrapper, AzCssPropertyWithConditions_simple(
        AzCssProperty_fontSize(AzStyleFontSize_px(32.0f))
    ));
    AzDom_addChild(&label_wrapper, label);

    // Button.
    AzButton button = AzButton_create(go_str("Increase counter"));
    AzButton_setButtonType(&button, AzButtonType_Primary);
    AzRefAny data_clone = AzRefAny_clone(&data);
    AzButton_setOnClick(&button, data_clone, on_click);
    AzDom button_dom = AzButton_dom(button);

    // Body.
    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, label_wrapper);
    AzDom_addChild(&body, button_dom);

    return AzDom_style(body, AzCss_empty());
}

// Bridge function returning a fully-built AzWindowCreateOptions ready
// to hand to AzApp_run. Doing this on the C side avoids having to
// poke window_state.* sub-fields from Go, which cgo finds difficult
// because of nested unions.
static AzWindowCreateOptions make_window(void) {
    AzWindowCreateOptions w = AzWindowCreateOptions_create(layout);
    w.window_state.title = go_str("Hello World");
    w.window_state.size.dimensions.width = 400.0f;
    w.window_state.size.dimensions.height = 300.0f;
    // NoTitleAutoInject: OS draws close/min/max buttons,
    // framework auto-injects a Titlebar with drag support.
    w.window_state.flags.decorations = AzWindowDecorations_NoTitleAutoInject;
    w.window_state.flags.background_material = AzWindowBackgroundMaterial_Sidebar;
    return w;
}

// Build the initial RefAny holding `MyDataModel { counter: 5 }`.
static AzRefAny make_initial_data(void) {
    MyDataModel model = { .counter = 5 };
    return MyDataModel_upcast(model);
}
*/
import "C"

import (
	"github.com/azul/azul-go"
)

func main() {
	// Initial application state, wrapped in the C-side RefAny.
	data := C.make_initial_data()

	// Window options + layout callback.
	window := C.make_window()

	// Idiomatic Go: pair `azul.NewApp(...)` with `defer app.Close()`.
	// `azul.NewApp` registers a `runtime.SetFinalizer` safety net so
	// even if `Close()` is forgotten the destructor eventually runs.
	app := azul.NewApp(data, C.AzAppConfig_create())
	defer app.Close()

	app.Run(window)
}
