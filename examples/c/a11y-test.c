#include "azul.h"
#include <stdio.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct { int dummy; } AppData;
void AppData_destructor(void* d) { }

AzJson AppData_toJson(AzRefAny refany);
AzResultRefAnyString AppData_fromJson(AzJson json);
AZ_REFLECT_JSON(AppData, AppData_destructor, AppData_toJson, AppData_fromJson);

AzJson AppData_toJson(AzRefAny refany) { return AzJson_null(); }
AzResultRefAnyString AppData_fromJson(AzJson json) {
    AppData m = { .dummy = 0 };
    return AzResultRefAnyString_ok(AppData_upcast(m));
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzString css_str = AZ_STR(
        "body { font-size: 14px; padding: 20px; font-family: sans-serif; }"
        "h1 { font-size: 24px; margin-bottom: 16px; }"
        "h2 { font-size: 18px; margin-top: 16px; margin-bottom: 8px; }"
        ".section { margin-bottom: 20px; padding: 10px; border: 1px solid #ccc; }"
        ".row { margin-bottom: 8px; }"
        ".editable { border: 1px solid #999; padding: 8px; min-height: 30px; }"
        "button { padding: 6px 12px; margin-right: 8px; }"
        "a { color: blue; }"
        "table { border-collapse: collapse; }"
        "th, td { border: 1px solid #ccc; padding: 4px 8px; }"
        "li { margin-left: 20px; }"
    );
    AzCss css = AzCss_fromString(css_str);

    AzDom body = AzDom_createBody();

    // === Heading ===
    AzDom h1 = AzDom_createNode(AzNodeType_h1());
    AzDom_addChild(&h1, AzDom_createText(AZ_STR("Accessibility Test Page")));
    AzDom_addChild(&body, h1);

    // === Section 1: Text content ===
    {
        AzDom section = AzDom_createDiv();
        AzDom_addClass(&section, AZ_STR("section"));

        AzDom h2 = AzDom_createNode(AzNodeType_h2());
        AzDom_addChild(&h2, AzDom_createText(AZ_STR("1. Text Content")));
        AzDom_addChild(&section, h2);

        AzDom p1 = AzDom_createP();
        AzDom_addChild(&p1, AzDom_createText(AZ_STR("This is a paragraph of text. VoiceOver should read this content.")));
        AzDom_addChild(&section, p1);

        AzDom p2 = AzDom_createP();
        AzDom_addChild(&p2, AzDom_createText(AZ_STR("This is a second paragraph with different content.")));
        AzDom_addChild(&section, p2);

        AzDom_addChild(&body, section);
    }

    // === Section 2: Buttons ===
    {
        AzDom section = AzDom_createDiv();
        AzDom_addClass(&section, AZ_STR("section"));

        AzDom h2 = AzDom_createNode(AzNodeType_h2());
        AzDom_addChild(&h2, AzDom_createText(AZ_STR("2. Buttons")));
        AzDom_addChild(&section, h2);

        AzDom btn1 = AzDom_createButton(AZ_STR("Click Me"));
        AzDom_addChild(&section, btn1);

        AzDom btn2 = AzDom_createButton(AZ_STR("Submit"));
        AzDom_addChild(&section, btn2);

        AzDom btn3 = AzDom_createButton(AZ_STR("Cancel"));
        AzDom_addChild(&section, btn3);

        AzDom_addChild(&body, section);
    }

    // === Section 3: Links ===
    {
        AzDom section = AzDom_createDiv();
        AzDom_addClass(&section, AZ_STR("section"));

        AzDom h2 = AzDom_createNode(AzNodeType_h2());
        AzDom_addChild(&h2, AzDom_createText(AZ_STR("3. Links")));
        AzDom_addChild(&section, h2);

        AzDom link1 = AzDom_createA(AZ_STR("https://example.com"), AzOptionString_some(AZ_STR("Example Website")));
        AzDom_addChild(&section, link1);

        AzDom_addChild(&section, AzDom_createText(AZ_STR(" | ")));

        AzDom link2 = AzDom_createA(AZ_STR("https://azul.rs"), AzOptionString_some(AZ_STR("Azul Homepage")));
        AzDom_addChild(&section, link2);

        AzDom_addChild(&body, section);
    }

    // === Section 4: Form inputs ===
    {
        AzDom section = AzDom_createDiv();
        AzDom_addClass(&section, AZ_STR("section"));

        AzDom h2 = AzDom_createNode(AzNodeType_h2());
        AzDom_addChild(&h2, AzDom_createText(AZ_STR("4. Form Inputs")));
        AzDom_addChild(&section, h2);

        AzDom input1 = AzDom_createInput(AZ_STR("text"), AZ_STR("username"), AZ_STR("Username:"));
        AzDom_addChild(&section, input1);

        AzDom input2 = AzDom_createInput(AZ_STR("password"), AZ_STR("password"), AZ_STR("Password:"));
        AzDom_addChild(&section, input2);

        AzDom input3 = AzDom_createInput(AZ_STR("checkbox"), AZ_STR("agree"), AZ_STR("I agree to terms"));
        AzDom_addChild(&section, input3);

        AzDom_addChild(&body, section);
    }

    // === Section 5: Lists ===
    {
        AzDom section = AzDom_createDiv();
        AzDom_addClass(&section, AZ_STR("section"));

        AzDom h2 = AzDom_createNode(AzNodeType_h2());
        AzDom_addChild(&h2, AzDom_createText(AZ_STR("5. Lists")));
        AzDom_addChild(&section, h2);

        AzDom ul = AzDom_createUl();
        AzDom li1 = AzDom_createLi();
        AzDom_addChild(&li1, AzDom_createText(AZ_STR("First item")));
        AzDom_addChild(&ul, li1);
        AzDom li2 = AzDom_createLi();
        AzDom_addChild(&li2, AzDom_createText(AZ_STR("Second item")));
        AzDom_addChild(&ul, li2);
        AzDom li3 = AzDom_createLi();
        AzDom_addChild(&li3, AzDom_createText(AZ_STR("Third item")));
        AzDom_addChild(&ul, li3);
        AzDom_addChild(&section, ul);

        AzDom_addChild(&body, section);
    }

    // === Section 6: Table ===
    {
        AzDom section = AzDom_createDiv();
        AzDom_addClass(&section, AZ_STR("section"));

        AzDom h2 = AzDom_createNode(AzNodeType_h2());
        AzDom_addChild(&h2, AzDom_createText(AZ_STR("6. Table")));
        AzDom_addChild(&section, h2);

        AzDom table = AzDom_createTable();

        // Header row
        AzDom tr_head = AzDom_createTr();
        AzDom th1 = AzDom_createTh();
        AzDom_addChild(&th1, AzDom_createText(AZ_STR("Name")));
        AzDom_addChild(&tr_head, th1);
        AzDom th2 = AzDom_createTh();
        AzDom_addChild(&th2, AzDom_createText(AZ_STR("Role")));
        AzDom_addChild(&tr_head, th2);
        AzDom_addChild(&table, tr_head);

        // Data rows
        AzDom tr1 = AzDom_createTr();
        AzDom td1a = AzDom_createTd();
        AzDom_addChild(&td1a, AzDom_createText(AZ_STR("Alice")));
        AzDom_addChild(&tr1, td1a);
        AzDom td1b = AzDom_createTd();
        AzDom_addChild(&td1b, AzDom_createText(AZ_STR("Developer")));
        AzDom_addChild(&tr1, td1b);
        AzDom_addChild(&table, tr1);

        AzDom tr2 = AzDom_createTr();
        AzDom td2a = AzDom_createTd();
        AzDom_addChild(&td2a, AzDom_createText(AZ_STR("Bob")));
        AzDom_addChild(&tr2, td2a);
        AzDom td2b = AzDom_createTd();
        AzDom_addChild(&td2b, AzDom_createText(AZ_STR("Designer")));
        AzDom_addChild(&tr2, td2b);
        AzDom_addChild(&table, tr2);

        AzDom_addChild(&section, table);
        AzDom_addChild(&body, section);
    }

    // === Section 7: Contenteditable ===
    {
        AzDom section = AzDom_createDiv();
        AzDom_addClass(&section, AZ_STR("section"));

        AzDom h2 = AzDom_createNode(AzNodeType_h2());
        AzDom_addChild(&h2, AzDom_createText(AZ_STR("7. Contenteditable")));
        AzDom_addChild(&section, h2);

        AzDom editable = AzDom_createDiv();
        AzDom_addClass(&editable, AZ_STR("editable"));
        AzDom_setContenteditable(&editable, true);
        AzDom_addChild(&editable, AzDom_createText(AZ_STR("Click here and type to edit this text.")));
        AzDom_addChild(&section, editable);

        AzDom_addChild(&body, section);
    }

    return AzDom_style(body, css);
}

int main() {
    AppData model = { .dummy = 0 };
    AzRefAny data = AppData_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Accessibility Test");
    window.window_state.size.dimensions.width = 700.0;
    window.window_state.size.dimensions.height = 600.0;

    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
