/**
 * Dropdown Widget Demo
 *
 * Demonstrates the DropDown widget which opens a native popup menu on click.
 * The framework handles all popup window creation, positioning, and callbacks.
 *
 * Build: cc dropdown.c -lazul -L../../target/release/ -Wl,-rpath,../../target/release -o dropdown
 */

#include "azul.h"
#include <stdio.h>
#include <string.h>

typedef struct {
    size_t selected_fruit;
    size_t selected_color;
} AppData;

void AppData_destructor(void* ptr) { (void)ptr; }
AZ_REFLECT(AppData, AppData_destructor);

// Helper: create AzString from C string literal
static AzString az(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

// Helper: build AzStringVec from C string array
static AzStringVec make_choices(const char** items, size_t count) {
    AzString buf[16];
    for (size_t i = 0; i < count && i < 16; i++) {
        buf[i] = az(items[i]);
    }
    return AzStringVec_copyFromPtr(buf, count);
}

static const char* FRUITS[] = { "Apple", "Banana", "Cherry", "Date", "Elderberry", "Fig", "Grape" };
static const size_t NUM_FRUITS = 7;

static const char* COLORS[] = { "Red", "Green", "Blue", "Yellow", "Purple" };
static const size_t NUM_COLORS = 5;

AzUpdate on_fruit_change(AzRefAny data, AzCallbackInfo info, size_t choice_index);
AzUpdate on_color_change(AzRefAny data, AzCallbackInfo info, size_t choice_index);

AzDom layout(AzRefAny data_ref, AzLayoutCallbackInfo info) {
    AppDataRef d = AppDataRef_create(&data_ref);
    if (!AppData_downcastRef(&data_ref, &d)) {
        return AzDom_createBody();
    }
    size_t sel_fruit = d.ptr->selected_fruit;
    size_t sel_color = d.ptr->selected_color;
    AppDataRef_delete(&d);

    AzDom body = AzDom_createBody();

    // Title — wrapped in <p> for block formatting
    AzDom title = AzDom_createP();
    AzDom_addCssProperty(&title, AzCssPropertyWithConditions_simple(
        AzCssProperty_fontSize(AzStyleFontSize_px(20.0))));
    AzDom_addChild(&title, AzDom_createText(az("Dropdown Widget Demo")));
    AzDom_addChild(&body, title);

    // Fruit label — wrapped in <p>
    AzDom fruit_label = AzDom_createP();
    AzDom_addChild(&fruit_label, AzDom_createText(az("Fruit:")));
    AzDom_addChild(&body, fruit_label);

    AzDropDown fruit_dd = AzDropDown_create(make_choices(FRUITS, NUM_FRUITS));
    fruit_dd.selected = sel_fruit;
    AzDropDown_setOnChoiceChange(&fruit_dd, AzRefAny_clone(&data_ref), on_fruit_change);
    AzDom_addChild(&body, AzDropDown_dom(fruit_dd));

    // Color label — wrapped in <p>
    AzDom color_label = AzDom_createP();
    AzDom_addChild(&color_label, AzDom_createText(az("Color:")));
    AzDom_addChild(&body, color_label);

    AzDropDown color_dd = AzDropDown_create(make_choices(COLORS, NUM_COLORS));
    color_dd.selected = sel_color;
    AzDropDown_setOnChoiceChange(&color_dd, AzRefAny_clone(&data_ref), on_color_change);
    AzDom_addChild(&body, AzDropDown_dom(color_dd));

    // Status — wrapped in <p>
    char status[128];
    int len = snprintf(status, sizeof(status), "Selected: %s, %s",
        FRUITS[sel_fruit], COLORS[sel_color]);
    AzDom status_p = AzDom_createP();
    AzDom_addChild(&status_p, AzDom_createText(
        AzString_copyFromBytes((const uint8_t*)status, 0, len)));
    AzDom_addChild(&body, status_p);

    return body;
}

AzUpdate on_fruit_change(AzRefAny data, AzCallbackInfo info, size_t choice_index) {
    AppDataRefMut d = AppDataRefMut_create(&data);
    if (!AppData_downcastMut(&data, &d)) return AzUpdate_DoNothing;
    d.ptr->selected_fruit = choice_index;
    printf("Fruit: %s\n", FRUITS[choice_index]);
    AppDataRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

AzUpdate on_color_change(AzRefAny data, AzCallbackInfo info, size_t choice_index) {
    AppDataRefMut d = AppDataRefMut_create(&data);
    if (!AppData_downcastMut(&data, &d)) return AzUpdate_DoNothing;
    d.ptr->selected_color = choice_index;
    printf("Color: %s\n", COLORS[choice_index]);
    AppDataRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

int main() {
    AppData model = { .selected_fruit = 0, .selected_color = 2 };
    AzRefAny data = AppData_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = az("Dropdown Widget Demo");
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;

    AzAppConfig config = AzAppConfig_default();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
