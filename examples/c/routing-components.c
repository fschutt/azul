// Guards the APIs documented in doc/guide/en/architecture/routing.md and
// .../components.md against drift.
//
// Every azul function called here is one those two guides tell a reader to
// call. If a signature changes, this stops compiling; if a symbol stops being
// exported, this stops linking; if the documented BEHAVIOUR changes (which
// pattern matches, what a route parameter reads back as), the assertions
// below fail at runtime. Run headless under the e2e runner:
//
//     AZ_BACKEND=headless AZ_E2E=tests/e2e/routing_components.json ./routing-components

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static AzString str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

// `AzString` is a byte vec, not NUL-terminated, so compare against its length.
static int str_eq(const AzString* s, const char* expected) {
    size_t n = strlen(expected);
    return s->vec.len == n && (n == 0 || memcmp(s->vec.ptr, expected, n) == 0);
}

static int failures = 0;

static void expect_str(AzString actual, const char* expected, const char* what) {
    if (!str_eq(&actual, expected)) {
        fprintf(stderr, "DOC DRIFT: %s: expected \"%s\", got \"%.*s\"\n",
                what, expected, (int)actual.vec.len, (const char*)actual.vec.ptr);
        failures += 1;
    }
    AzString_delete(&actual);
}

typedef struct { uint32_t visited_user; } Model;
static void Model_destructor(void* m) { (void)m; }
AZ_REFLECT(Model, Model_destructor);

// --------------------------------------------------------------------------
// components.md: "The registration callback is a repr(C) function pointer, so
// a plain function pointer is enough on the C side."
//
// `AzRegisterComponentLibraryFnType` is `AzComponentLibrary (*)(void)`.
// ComponentLibrary exports no constructor, so C fills the repr(C) struct in
// directly - the fields are the ones components.md documents.
// --------------------------------------------------------------------------
static AzComponentLibrary register_mylib(void) {
    AzComponentLibrary lib;
    lib.name = str("mylib");
    lib.version = str("1.0.0");
    lib.description = str("doc-drift guard");
    lib.components = AzComponentDefVec_create();
    lib.exportable = true;
    lib.modifiable = false;
    lib.data_models = AzComponentDataModelVec_create();
    lib.enum_models = AzComponentEnumModelVec_create();
    return lib;
}

// --------------------------------------------------------------------------
// routing.md: "Switching routes from a callback"
// --------------------------------------------------------------------------
static AzUpdate on_open_user(AzRefAny data, AzCallbackInfo info) {
    (void)data;

    // "Inside an event CallbackInfo, the same pair reads the same state."
    expect_str(AzCallbackInfo_getRoutePattern(&info), "/",
               "CallbackInfo.get_route_pattern() before switch_route");

    // "set_route_param(key, value) modifies a param in place"
    AzCallbackInfo_setRouteParam(&info, str("page"), str("2"));
    expect_str(AzCallbackInfo_getRouteParam(&info, str("page")), "2",
               "CallbackInfo.get_route_param() after set_route_param");

    // "info.switch_route(\"/user/:id\".into(), params)"
    AzStringPair pair;
    pair.key = str("id");
    pair.value = str("42");
    AzStringPairVec params = AzStringPairVec_copyFromPtr(&pair, 1);
    AzCallbackInfo_switchRoute(&info, str("/user/:id"), params);

    return AzUpdate_RefreshDom;
}

// --------------------------------------------------------------------------
// routing.md: "Reading the active route"
// --------------------------------------------------------------------------
static AzDom layout_home(AzRefAny data, AzLayoutCallbackInfo info) {
    // "info.get_route_pattern() is the pattern the framework matched"
    expect_str(AzLayoutCallbackInfo_getRoutePattern(&info), "/",
               "LayoutCallbackInfo.get_route_pattern() on the initial route");

    // "get_route_param(key) reads one extracted parameter, empty when there
    // is none" - "/" captures nothing.
    expect_str(AzLayoutCallbackInfo_getRouteParam(&info, str("id")), "",
               "LayoutCallbackInfo.get_route_param() for an absent param");

    AzDom label = AzDom_createPWithText(str("home"));

    AzButton button = AzButton_create(str("Open user"));
    AzRefAny data_clone = AzRefAny_clone(&data);
    AzButton_setOnClick(&button, data_clone, on_open_user);

    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, label);
    AzDom_addChild(&body, AzButton_dom(button));
    return body;
}

static AzDom layout_user(AzRefAny data, AzLayoutCallbackInfo info) {
    // The pattern is the PATTERN, not the concrete path.
    expect_str(AzLayoutCallbackInfo_getRoutePattern(&info), "/user/:id",
               "LayoutCallbackInfo.get_route_pattern() after switch_route");

    // routing.md's table: "/user/:id" + "/user/42" captures id = "42".
    expect_str(AzLayoutCallbackInfo_getRouteParam(&info, str("id")), "42",
               "LayoutCallbackInfo.get_route_param(\"id\") after switch_route");

    ModelRefMut d = ModelRefMut_create(&data);
    if (Model_downcastMut(&data, &d)) {
        d.ptr->visited_user = 1;
    }
    ModelRefMut_delete(&d);

    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, AzDom_createPWithText(str("user")));
    return body;
}

static AzDom layout_settings(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)data;
    (void)info;
    return AzDom_createBody();
}

int main(void) {
    Model model = { .visited_user = 0 };
    AzRefAny data = Model_upcast(model);

    // routing.md: "Register routes on the AppConfig before passing it to
    // App::create". Adding a route that already exists replaces it, and the
    // explicit "/" is the initial layout.
    AzAppConfig config = AzAppConfig_create();
    AzAppConfig_addRoute(&config, str("/"), layout_home);
    AzAppConfig_addRoute(&config, str("/user/:id"), layout_user);
    AzAppConfig_addRoute(&config, str("/settings"), layout_settings);

    // components.md, "From C".
    // `AzString_fromCStr` takes `const int8_t*`, so a plain string literal
    // needs the cast to compile without -Wpointer-sign.
    AzAppConfig_addComponentLibrary(&config, AzString_fromCStr((const int8_t*)"mylib"),
                                    register_mylib);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout_home);
    window.window_state.title = str("Routing + components");

    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);

    if (failures != 0) {
        fprintf(stderr, "%d documented behaviour(s) drifted\n", failures);
        return 1;
    }
    return 0;
}
