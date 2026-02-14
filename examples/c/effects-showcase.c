// Effects Showcase - C
// Demonstrates gradients, box shadows, text shadows, filters, opacity, backdrop-filter
// cc -o effects-showcase effects-showcase.c -lazul -L../../target/release
#include "azul.h"
#include <stdio.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

typedef struct { int dummy; } EffectsData;
void EffectsData_destructor(void* m) { }
AZ_REFLECT(EffectsData, EffectsData_destructor);

static AzDom make_styled_div(const char* css, const char* text) {
    AzDom div = AzDom_createDiv();
    AzDom_setInlineStyle(&div, AZ_STR(css));
    if (text && text[0]) {
        AzDom_addChild(&div, AzDom_createText(AZ_STR(text)));
    }
    return div;
}

static AzDom make_section(const char* title) {
    AzDom section = AzDom_createDiv();
    AzDom_setInlineStyle(&section, AZ_STR(
        "margin: 10px; padding: 8px; border-bottom: 1px solid #ccc;"
    ));
    AzDom label = AzDom_createDiv();
    AzDom_setInlineStyle(&label, AZ_STR(
        "font-size: 18px; font-weight: bold; margin-bottom: 8px; color: #333;"
    ));
    AzDom_addChild(&label, AzDom_createText(AZ_STR(title)));
    AzDom_addChild(&section, label);
    return section;
}

static void add_child(AzDom* parent, AzDom child) {
    AzDom_addChild(parent, child);
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {

    AzDom body = AzDom_createBody();
    AzDom_setInlineStyle(&body, AZ_STR(
        "padding: 20px; background-color: #f0f0f0; font-size: 14px; color: #222;"
    ));

    // ── Title ──
    AzDom title = AzDom_createDiv();
    AzDom_setInlineStyle(&title, AZ_STR(
        "font-size: 28px; font-weight: bold; margin-bottom: 16px; color: #111;"
    ));
    AzDom_addChild(&title, AzDom_createText(AZ_STR("Effects Showcase")));
    add_child(&body, title);

    // ── 1. Linear Gradients ──
    {
        AzDom section = make_section("Linear Gradients");
        AzDom row = AzDom_createDiv();
        AzDom_setInlineStyle(&row, AZ_STR("flex-direction: row; gap: 10px;"));

        add_child(&row, make_styled_div(
            "width: 120px; height: 80px; border-radius: 8px;"
            "background: linear-gradient(to right, #ff0000, #0000ff);",
            ""
        ));
        add_child(&row, make_styled_div(
            "width: 120px; height: 80px; border-radius: 8px;"
            "background: linear-gradient(135deg, #ff6b6b, #feca57, #48dbfb);",
            ""
        ));
        add_child(&row, make_styled_div(
            "width: 120px; height: 80px; border-radius: 8px;"
            "background: linear-gradient(to bottom, #a29bfe, #6c5ce7);",
            ""
        ));
        add_child(&row, make_styled_div(
            "width: 120px; height: 80px; border-radius: 8px;"
            "background: linear-gradient(45deg, #00b894 0%, #00cec9 50%, #0984e3 100%);",
            ""
        ));

        add_child(&section, row);
        add_child(&body, section);
    }

    // ── 2. Radial Gradients ──
    {
        AzDom section = make_section("Radial Gradients");
        AzDom row = AzDom_createDiv();
        AzDom_setInlineStyle(&row, AZ_STR("flex-direction: row; gap: 10px;"));

        add_child(&row, make_styled_div(
            "width: 120px; height: 80px; border-radius: 8px;"
            "background: radial-gradient(circle, #fdcb6e, #e17055);",
            ""
        ));
        add_child(&row, make_styled_div(
            "width: 120px; height: 80px; border-radius: 8px;"
            "background: radial-gradient(ellipse, #dfe6e9, #2d3436);",
            ""
        ));
        add_child(&row, make_styled_div(
            "width: 120px; height: 80px; border-radius: 50%;"
            "background: radial-gradient(circle, #fff 0%, #74b9ff 50%, #0984e3 100%);",
            ""
        ));

        add_child(&section, row);
        add_child(&body, section);
    }

    // ── 3. Conic Gradients ──
    {
        AzDom section = make_section("Conic Gradients");
        AzDom row = AzDom_createDiv();
        AzDom_setInlineStyle(&row, AZ_STR("flex-direction: row; gap: 10px;"));

        add_child(&row, make_styled_div(
            "width: 100px; height: 100px; border-radius: 50%;"
            "background: conic-gradient(#ff0000, #ff8800, #ffff00, #00ff00, #0000ff, #8800ff, #ff0000);",
            ""
        ));
        add_child(&row, make_styled_div(
            "width: 100px; height: 100px; border-radius: 50%;"
            "background: conic-gradient(from 90deg, #e74c3c, #f39c12, #2ecc71, #3498db, #e74c3c);",
            ""
        ));

        add_child(&section, row);
        add_child(&body, section);
    }

    // ── 4. Box Shadows ──
    {
        AzDom section = make_section("Box Shadows");
        AzDom row = AzDom_createDiv();
        AzDom_setInlineStyle(&row, AZ_STR("flex-direction: row; gap: 20px; padding: 15px;"));

        add_child(&row, make_styled_div(
            "width: 100px; height: 70px; background-color: white; border-radius: 8px;"
            "box-shadow: 3px 3px 10px rgba(0,0,0,0.3);",
            "Soft"
        ));
        add_child(&row, make_styled_div(
            "width: 100px; height: 70px; background-color: white; border-radius: 8px;"
            "box-shadow: 0px 8px 25px rgba(0,0,0,0.5);",
            "Deep"
        ));
        add_child(&row, make_styled_div(
            "width: 100px; height: 70px; background-color: #6c5ce7; border-radius: 8px;"
            "box-shadow: 0px 4px 15px rgba(108,92,231,0.6); color: white;",
            "Colored"
        ));
        add_child(&row, make_styled_div(
            "width: 100px; height: 70px; background-color: white; border-radius: 8px;"
            "box-shadow: inset 0px 2px 8px rgba(0,0,0,0.3);",
            "Inset"
        ));

        add_child(&section, row);
        add_child(&body, section);
    }

    // ── 5. Text Shadows ──
    {
        AzDom section = make_section("Text Shadows");
        AzDom row = AzDom_createDiv();
        AzDom_setInlineStyle(&row, AZ_STR("flex-direction: row; gap: 20px;"));

        add_child(&row, make_styled_div(
            "font-size: 24px; font-weight: bold; color: #2d3436;"
            "text-shadow: 2px 2px 4px rgba(0,0,0,0.3);",
            "Soft Shadow"
        ));
        add_child(&row, make_styled_div(
            "font-size: 24px; font-weight: bold; color: #e74c3c;"
            "text-shadow: 0px 0px 10px rgba(231,76,60,0.8);",
            "Glow Effect"
        ));
        add_child(&row, make_styled_div(
            "font-size: 24px; font-weight: bold; color: white;"
            "text-shadow: 1px 1px 0px #333;",
            "Outline"
        ));

        add_child(&section, row);
        add_child(&body, section);
    }

    // ── 6. Opacity ──
    {
        AzDom section = make_section("Opacity");
        AzDom row = AzDom_createDiv();
        AzDom_setInlineStyle(&row, AZ_STR("flex-direction: row; gap: 10px;"));

        add_child(&row, make_styled_div(
            "width: 80px; height: 60px; background-color: #e74c3c; border-radius: 6px;"
            "opacity: 1.0;",
            "100%"
        ));
        add_child(&row, make_styled_div(
            "width: 80px; height: 60px; background-color: #e74c3c; border-radius: 6px;"
            "opacity: 0.75;",
            "75%"
        ));
        add_child(&row, make_styled_div(
            "width: 80px; height: 60px; background-color: #e74c3c; border-radius: 6px;"
            "opacity: 0.5;",
            "50%"
        ));
        add_child(&row, make_styled_div(
            "width: 80px; height: 60px; background-color: #e74c3c; border-radius: 6px;"
            "opacity: 0.25;",
            "25%"
        ));

        add_child(&section, row);
        add_child(&body, section);
    }

    // ── 7. CSS Filters ──
    {
        AzDom section = make_section("CSS Filters");
        AzDom row = AzDom_createDiv();
        AzDom_setInlineStyle(&row, AZ_STR("flex-direction: row; gap: 10px; flex-wrap: wrap;"));

        // Blur
        add_child(&row, make_styled_div(
            "width: 100px; height: 70px; border-radius: 6px;"
            "background: linear-gradient(135deg, #ff6b6b, #feca57);"
            "filter: blur(3px);",
            ""
        ));
        // Grayscale
        add_child(&row, make_styled_div(
            "width: 100px; height: 70px; border-radius: 6px;"
            "background: linear-gradient(135deg, #ff6b6b, #feca57);"
            "filter: grayscale(100%);",
            ""
        ));
        // Sepia
        add_child(&row, make_styled_div(
            "width: 100px; height: 70px; border-radius: 6px;"
            "background: linear-gradient(135deg, #48dbfb, #a29bfe);"
            "filter: sepia(100%);",
            ""
        ));
        // Brightness
        add_child(&row, make_styled_div(
            "width: 100px; height: 70px; border-radius: 6px;"
            "background: linear-gradient(135deg, #00b894, #00cec9);"
            "filter: brightness(150%);",
            ""
        ));
        // Contrast
        add_child(&row, make_styled_div(
            "width: 100px; height: 70px; border-radius: 6px;"
            "background: linear-gradient(135deg, #fdcb6e, #e17055);"
            "filter: contrast(200%);",
            ""
        ));
        // Invert
        add_child(&row, make_styled_div(
            "width: 100px; height: 70px; border-radius: 6px;"
            "background: linear-gradient(135deg, #dfe6e9, #636e72);"
            "filter: invert(100%);",
            ""
        ));
        // Hue-rotate
        add_child(&row, make_styled_div(
            "width: 100px; height: 70px; border-radius: 6px;"
            "background: linear-gradient(135deg, #e74c3c, #3498db);"
            "filter: hue-rotate(90deg);",
            ""
        ));
        // Saturate
        add_child(&row, make_styled_div(
            "width: 100px; height: 70px; border-radius: 6px;"
            "background: linear-gradient(135deg, #b2bec3, #636e72);"
            "filter: saturate(300%);",
            ""
        ));

        // Labels row
        AzDom labels = AzDom_createDiv();
        AzDom_setInlineStyle(&labels, AZ_STR(
            "flex-direction: row; gap: 10px; flex-wrap: wrap; font-size: 11px; color: #666;"
        ));
        add_child(&labels, make_styled_div("width: 100px; text-align: center;", "blur(3px)"));
        add_child(&labels, make_styled_div("width: 100px; text-align: center;", "grayscale"));
        add_child(&labels, make_styled_div("width: 100px; text-align: center;", "sepia"));
        add_child(&labels, make_styled_div("width: 100px; text-align: center;", "brightness"));
        add_child(&labels, make_styled_div("width: 100px; text-align: center;", "contrast"));
        add_child(&labels, make_styled_div("width: 100px; text-align: center;", "invert"));
        add_child(&labels, make_styled_div("width: 100px; text-align: center;", "hue-rotate"));
        add_child(&labels, make_styled_div("width: 100px; text-align: center;", "saturate"));

        add_child(&section, row);
        add_child(&section, labels);
        add_child(&body, section);
    }

    // ── 8. Backdrop Filter (overlapping rects) ──
    {
        AzDom section = make_section("Backdrop Filter (Overlapping)");

        // Container with relative positioning
        AzDom container = AzDom_createDiv();
        AzDom_setInlineStyle(&container, AZ_STR(
            "position: relative; width: 500px; height: 150px;"
        ));

        // Background: colorful gradient
        add_child(&container, make_styled_div(
            "position: absolute; left: 0px; top: 0px; width: 500px; height: 150px;"
            "background: linear-gradient(135deg, #e74c3c, #f39c12, #2ecc71, #3498db, #9b59b6);"
            "border-radius: 10px;",
            ""
        ));

        // Overlapping blurred rect
        add_child(&container, make_styled_div(
            "position: absolute; left: 30px; top: 20px; width: 200px; height: 110px;"
            "backdrop-filter: blur(8px);"
            "background-color: rgba(255,255,255,0.2);"
            "border-radius: 12px; border: 1px solid rgba(255,255,255,0.3);",
            "Blur Backdrop"
        ));

        // Another overlapping blurred rect
        add_child(&container, make_styled_div(
            "position: absolute; left: 260px; top: 20px; width: 200px; height: 110px;"
            "backdrop-filter: blur(4px) grayscale(80%);"
            "background-color: rgba(255,255,255,0.15);"
            "border-radius: 12px; border: 1px solid rgba(255,255,255,0.3);",
            "Blur + Grayscale"
        ));

        add_child(&section, container);
        add_child(&body, section);
    }

    // ── 9. Combined Effects ──
    {
        AzDom section = make_section("Combined Effects");
        AzDom row = AzDom_createDiv();
        AzDom_setInlineStyle(&row, AZ_STR("flex-direction: row; gap: 15px; padding: 10px;"));

        // Gradient + shadow + border-radius
        add_child(&row, make_styled_div(
            "width: 140px; height: 90px; border-radius: 16px;"
            "background: linear-gradient(135deg, #667eea, #764ba2);"
            "box-shadow: 0px 10px 30px rgba(102,126,234,0.5);",
            ""
        ));

        // Gradient + opacity + shadow
        add_child(&row, make_styled_div(
            "width: 140px; height: 90px; border-radius: 16px;"
            "background: radial-gradient(circle, #f093fb, #f5576c);"
            "opacity: 0.85;"
            "box-shadow: 0px 8px 20px rgba(245,87,108,0.4);",
            ""
        ));

        // Text with shadow on gradient bg
        add_child(&row, make_styled_div(
            "width: 160px; height: 90px; border-radius: 16px;"
            "background: linear-gradient(to right, #4facfe, #00f2fe);"
            "box-shadow: 0px 6px 20px rgba(79,172,254,0.5);"
            "font-size: 20px; font-weight: bold; color: white;"
            "text-shadow: 1px 2px 4px rgba(0,0,0,0.4);"
            "padding: 10px; text-align: center;",
            "Hello Azul!"
        ));

        add_child(&section, row);
        add_child(&body, section);
    }

    return AzDom_style(&body, AzCss_empty());
}

int main() {
    EffectsData model = { .dummy = 0 };
    AzRefAny data = EffectsData_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Effects Showcase");
    window.window_state.size.dimensions.width = 800.0;
    window.window_state.size.dimensions.height = 900.0;
    window.window_state.flags.decorations = AzWindowDecorations_NoTitleAutoInject;
    window.window_state.flags.background_material = AzWindowBackgroundMaterial_Sidebar;

    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
