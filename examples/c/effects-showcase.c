// Effects Showcase - C
// Demonstrates gradients, box shadows, text shadows, filters, opacity, backdrop-filter
// Uses CSS grid layout. Requires overflow-y:scroll on body for scrolling.
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

static AzDom make_label(const char* text) {
    AzDom label = AzDom_createDiv();
    AzDom_setInlineStyle(&label, AZ_STR(
        "font-size: 10px; color: #888; text-align: center; margin-top: 4px;"
    ));
    AzDom_addChild(&label, AzDom_createText(AZ_STR(text)));
    return label;
}

static AzDom make_card(const char* effect_css, const char* label_text) {
    AzDom card = AzDom_createDiv();
    AzDom_setInlineStyle(&card, AZ_STR("flex-direction: column; align-items: center;"));
    AzDom_addChild(&card, make_styled_div(effect_css, ""));
    AzDom_addChild(&card, make_label(label_text));
    return card;
}

static AzDom make_card_with_text(const char* effect_css, const char* inner_text, const char* label_text) {
    AzDom card = AzDom_createDiv();
    AzDom_setInlineStyle(&card, AZ_STR("flex-direction: column; align-items: center;"));
    AzDom_addChild(&card, make_styled_div(effect_css, inner_text));
    AzDom_addChild(&card, make_label(label_text));
    return card;
}

static void add_child(AzDom* parent, AzDom child) {
    AzDom_addChild(parent, child);
}

static AzDom make_section_header(const char* title) {
    AzDom h = AzDom_createDiv();
    AzDom_setInlineStyle(&h, AZ_STR(
        "font-size: 18px; font-weight: bold; color: #333; margin-bottom: 8px;"
        "grid-column-start: 1; grid-column-end: 5;"
        "border-bottom: 1px solid #ddd; padding-bottom: 4px;"
    ));
    AzDom_addChild(&h, AzDom_createText(AZ_STR(title)));
    return h;
}

static AzDom make_full_width_container(const char* css) {
    AzDom c = AzDom_createDiv();
    char buf[2048];
    snprintf(buf, sizeof(buf),
        "grid-column-start: 1; grid-column-end: 5; %s", css);
    AzDom_setInlineStyle(&c, AZ_STR(buf));
    return c;
}

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {

    AzDom body = AzDom_createBody();
    AzDom_setInlineStyle(&body, AZ_STR(
        "padding: 20px; background-color: #f0f0f0; font-size: 14px; color: #222;"
        "overflow: scroll;"
    ));

    // Title
    AzDom title = AzDom_createDiv();
    AzDom_setInlineStyle(&title, AZ_STR(
        "font-size: 28px; font-weight: bold; margin-bottom: 16px; color: #111;"
    ));
    AzDom_addChild(&title, AzDom_createText(AZ_STR("Effects Showcase")));
    add_child(&body, title);

    // Main grid: 4 columns
    AzDom grid = AzDom_createDiv();
    AzDom_setInlineStyle(&grid, AZ_STR(
        "display: grid;"
        "grid-template-columns: repeat(4, 1fr);"
        "gap: 16px;"
        "padding: 10px;"
    ));

    // === 1. LINEAR GRADIENTS ===
    add_child(&grid, make_section_header("Linear Gradients"));
    add_child(&grid, make_card(
        "width: 160px; height: 100px; border-radius: 8px;"
        "background: linear-gradient(to right, #ff0000, #0000ff);",
        "to right"));
    add_child(&grid, make_card(
        "width: 160px; height: 100px; border-radius: 8px;"
        "background: linear-gradient(135deg, #ff6b6b, #feca57, #48dbfb);",
        "135deg 3-stop"));
    add_child(&grid, make_card(
        "width: 160px; height: 100px; border-radius: 8px;"
        "background: linear-gradient(to bottom, #a29bfe, #6c5ce7);",
        "to bottom"));
    add_child(&grid, make_card(
        "width: 160px; height: 100px; border-radius: 8px;"
        "background: linear-gradient(45deg, #00b894 0%, #00cec9 50%, #0984e3 100%);",
        "45deg 3-stop"));

    // === 2. RADIAL GRADIENTS ===
    add_child(&grid, make_section_header("Radial Gradients"));
    add_child(&grid, make_card(
        "width: 160px; height: 100px; border-radius: 8px;"
        "background: radial-gradient(circle, #fdcb6e, #e17055);",
        "circle"));
    add_child(&grid, make_card(
        "width: 160px; height: 100px; border-radius: 8px;"
        "background: radial-gradient(ellipse, #dfe6e9, #2d3436);",
        "ellipse"));
    add_child(&grid, make_card(
        "width: 160px; height: 100px; border-radius: 50%;"
        "background: radial-gradient(circle, #fff 0%, #74b9ff 50%, #0984e3 100%);",
        "circle 3-stop"));
    add_child(&grid, make_card(
        "width: 160px; height: 100px; border-radius: 8px;"
        "background: radial-gradient(circle, #f093fb, #f5576c);",
        "warm radial"));

    // === 3. CONIC GRADIENTS ===
    add_child(&grid, make_section_header("Conic Gradients"));
    add_child(&grid, make_card(
        "width: 120px; height: 120px; border-radius: 50%;"
        "background: conic-gradient(#ff0000, #ff8800, #ffff00, #00ff00, #0000ff, #8800ff, #ff0000);",
        "rainbow"));
    add_child(&grid, make_card(
        "width: 120px; height: 120px; border-radius: 50%;"
        "background: conic-gradient(from 90deg, #e74c3c, #f39c12, #2ecc71, #3498db, #e74c3c);",
        "from 90deg"));
    add_child(&grid, make_card(
        "width: 120px; height: 120px; border-radius: 50%;"
        "background: conic-gradient(#fff, #000, #fff, #000, #fff);",
        "checkerboard"));
    add_child(&grid, make_card(
        "width: 120px; height: 120px; border-radius: 8px;"
        "background: conic-gradient(from 45deg, #667eea, #764ba2, #667eea);",
        "square conic"));

    // === 4. BOX SHADOWS ===
    add_child(&grid, make_section_header("Box Shadows"));
    add_child(&grid, make_card_with_text(
        "width: 140px; height: 80px; background-color: white; border-radius: 8px;"
        "box-shadow: 3px 3px 10px rgba(0,0,0,0.3); padding: 8px; font-size: 12px;",
        "Soft", "soft shadow"));
    add_child(&grid, make_card_with_text(
        "width: 140px; height: 80px; background-color: white; border-radius: 8px;"
        "box-shadow: 0px 8px 25px rgba(0,0,0,0.5); padding: 8px; font-size: 12px;",
        "Deep", "deep shadow"));
    add_child(&grid, make_card_with_text(
        "width: 140px; height: 80px; background-color: #6c5ce7; border-radius: 8px;"
        "box-shadow: 0px 4px 15px rgba(108,92,231,0.6); padding: 8px; font-size: 12px; color: white;",
        "Colored", "colored shadow"));
    add_child(&grid, make_card_with_text(
        "width: 140px; height: 80px; background-color: white; border-radius: 8px;"
        "box-shadow: inset 0px 2px 8px rgba(0,0,0,0.3); padding: 8px; font-size: 12px;",
        "Inset", "inset shadow"));

    // === 5. TEXT SHADOWS ===
    add_child(&grid, make_section_header("Text Shadows"));
    add_child(&grid, make_styled_div(
        "font-size: 22px; font-weight: bold; color: #2d3436;"
        "text-shadow: 2px 2px 4px rgba(0,0,0,0.3);",
        "Soft Shadow"));
    add_child(&grid, make_styled_div(
        "font-size: 22px; font-weight: bold; color: #e74c3c;"
        "text-shadow: 0px 0px 10px rgba(231,76,60,0.8);",
        "Glow Effect"));
    add_child(&grid, make_styled_div(
        "font-size: 22px; font-weight: bold; color: white;"
        "text-shadow: 1px 1px 0px #333;",
        "Outline"));
    add_child(&grid, make_styled_div(
        "font-size: 22px; font-weight: bold; color: #0984e3;"
        "text-shadow: 3px 3px 0px rgba(9,132,227,0.3);",
        "Retro"));

    // === 6. OPACITY ===
    add_child(&grid, make_section_header("Opacity"));
    add_child(&grid, make_card_with_text(
        "width: 140px; height: 70px; background-color: #e74c3c; border-radius: 6px;"
        "opacity: 1.0; padding: 6px; color: white; font-size: 12px;",
        "100%", "opacity: 1.0"));
    add_child(&grid, make_card_with_text(
        "width: 140px; height: 70px; background-color: #e74c3c; border-radius: 6px;"
        "opacity: 0.75; padding: 6px; color: white; font-size: 12px;",
        "75%", "opacity: 0.75"));
    add_child(&grid, make_card_with_text(
        "width: 140px; height: 70px; background-color: #e74c3c; border-radius: 6px;"
        "opacity: 0.5; padding: 6px; color: white; font-size: 12px;",
        "50%", "opacity: 0.5"));
    add_child(&grid, make_card_with_text(
        "width: 140px; height: 70px; background-color: #e74c3c; border-radius: 6px;"
        "opacity: 0.25; padding: 6px; color: white; font-size: 12px;",
        "25%", "opacity: 0.25"));

    // === 7. CSS FILTERS ===
    add_child(&grid, make_section_header("CSS Filters"));
    add_child(&grid, make_card(
        "width: 140px; height: 80px; border-radius: 6px;"
        "background: linear-gradient(135deg, #ff6b6b, #feca57);"
        "filter: blur(3px);",
        "blur(3px)"));
    add_child(&grid, make_card(
        "width: 140px; height: 80px; border-radius: 6px;"
        "background: linear-gradient(135deg, #ff6b6b, #feca57);"
        "filter: grayscale(100%);",
        "grayscale(100%)"));
    add_child(&grid, make_card(
        "width: 140px; height: 80px; border-radius: 6px;"
        "background: linear-gradient(135deg, #48dbfb, #a29bfe);"
        "filter: sepia(100%);",
        "sepia(100%)"));
    add_child(&grid, make_card(
        "width: 140px; height: 80px; border-radius: 6px;"
        "background: linear-gradient(135deg, #00b894, #00cec9);"
        "filter: brightness(150%);",
        "brightness(150%)"));
    add_child(&grid, make_card(
        "width: 140px; height: 80px; border-radius: 6px;"
        "background: linear-gradient(135deg, #fdcb6e, #e17055);"
        "filter: contrast(200%);",
        "contrast(200%)"));
    add_child(&grid, make_card(
        "width: 140px; height: 80px; border-radius: 6px;"
        "background: linear-gradient(135deg, #dfe6e9, #636e72);"
        "filter: invert(100%);",
        "invert(100%)"));
    add_child(&grid, make_card(
        "width: 140px; height: 80px; border-radius: 6px;"
        "background: linear-gradient(135deg, #e74c3c, #3498db);"
        "filter: hue-rotate(90deg);",
        "hue-rotate(90deg)"));
    add_child(&grid, make_card(
        "width: 140px; height: 80px; border-radius: 6px;"
        "background: linear-gradient(135deg, #b2bec3, #636e72);"
        "filter: saturate(300%);",
        "saturate(300%)"));

    // === 8. OVERLAPPING RECTS WITH OPACITY ===
    add_child(&grid, make_section_header("Overlapping Rects with Opacity"));
    {
        AzDom container = make_full_width_container(
            "position: relative; width: 700px; height: 200px;"
            "background-color: #ecf0f1; border-radius: 10px;"
        );
        add_child(&container, make_styled_div(
            "position: absolute; left: 20px; top: 20px; width: 300px; height: 160px;"
            "background: linear-gradient(135deg, #e74c3c, #f39c12);"
            "border-radius: 12px; opacity: 0.9;", ""));
        add_child(&container, make_styled_div(
            "position: absolute; left: 100px; top: 40px; width: 250px; height: 120px;"
            "background: linear-gradient(135deg, #3498db, #2ecc71);"
            "border-radius: 12px; opacity: 0.7;", ""));
        add_child(&container, make_styled_div(
            "position: absolute; left: 200px; top: 60px; width: 200px; height: 100px;"
            "background: linear-gradient(135deg, #9b59b6, #e74c3c);"
            "border-radius: 12px; opacity: 0.6;", ""));
        add_child(&container, make_styled_div(
            "position: absolute; left: 350px; top: 30px; width: 180px; height: 140px;"
            "background-color: rgba(255,255,255,0.3);"
            "border-radius: 12px; border: 2px solid rgba(255,255,255,0.5);", ""));
        add_child(&container, make_styled_div(
            "position: absolute; left: 380px; top: 80px; width: 120px; height: 30px;"
            "font-size: 14px; font-weight: bold; color: white;"
            "text-shadow: 1px 1px 3px rgba(0,0,0,0.5);",
            "Overlapping!"));
        add_child(&grid, container);
    }

    // === 9. BACKDROP FILTER ===
    add_child(&grid, make_section_header("Backdrop Filter (Overlapping)"));
    {
        AzDom container = make_full_width_container(
            "position: relative; width: 700px; height: 180px;"
        );
        add_child(&container, make_styled_div(
            "position: absolute; left: 0px; top: 0px; width: 700px; height: 180px;"
            "background: linear-gradient(135deg, #e74c3c, #f39c12, #2ecc71, #3498db, #9b59b6);"
            "border-radius: 10px;", ""));
        add_child(&container, make_styled_div(
            "position: absolute; left: 30px; top: 20px; width: 200px; height: 140px;"
            "backdrop-filter: blur(8px);"
            "background-color: rgba(255,255,255,0.2);"
            "border-radius: 12px; border: 1px solid rgba(255,255,255,0.3);"
            "padding: 12px; font-size: 14px; color: white; font-weight: bold;"
            "text-shadow: 1px 1px 2px rgba(0,0,0,0.4);",
            "Blur Backdrop"));
        add_child(&container, make_styled_div(
            "position: absolute; left: 260px; top: 20px; width: 200px; height: 140px;"
            "backdrop-filter: blur(4px) grayscale(80%);"
            "background-color: rgba(255,255,255,0.15);"
            "border-radius: 12px; border: 1px solid rgba(255,255,255,0.3);"
            "padding: 12px; font-size: 14px; color: white; font-weight: bold;"
            "text-shadow: 1px 1px 2px rgba(0,0,0,0.4);",
            "Blur + Grayscale"));
        add_child(&container, make_styled_div(
            "position: absolute; left: 490px; top: 20px; width: 180px; height: 140px;"
            "backdrop-filter: blur(12px) brightness(120%);"
            "background-color: rgba(0,0,0,0.1);"
            "border-radius: 12px; border: 1px solid rgba(255,255,255,0.2);"
            "padding: 12px; font-size: 14px; color: white; font-weight: bold;"
            "text-shadow: 1px 1px 2px rgba(0,0,0,0.4);",
            "Blur + Bright"));
        add_child(&grid, container);
    }

    // === 10. COMBINED EFFECTS ===
    add_child(&grid, make_section_header("Combined Effects"));
    add_child(&grid, make_card(
        "width: 160px; height: 100px; border-radius: 16px;"
        "background: linear-gradient(135deg, #667eea, #764ba2);"
        "box-shadow: 0px 10px 30px rgba(102,126,234,0.5);",
        "gradient + shadow"));
    add_child(&grid, make_card(
        "width: 160px; height: 100px; border-radius: 16px;"
        "background: radial-gradient(circle, #f093fb, #f5576c);"
        "opacity: 0.85;"
        "box-shadow: 0px 8px 20px rgba(245,87,108,0.4);",
        "radial + opacity"));
    add_child(&grid, make_card_with_text(
        "width: 160px; height: 100px; border-radius: 16px;"
        "background: linear-gradient(to right, #4facfe, #00f2fe);"
        "box-shadow: 0px 6px 20px rgba(79,172,254,0.5);"
        "font-size: 18px; font-weight: bold; color: white;"
        "text-shadow: 1px 2px 4px rgba(0,0,0,0.4); padding: 10px;",
        "Hello Azul!", "text + gradient + shadow"));
    add_child(&grid, make_card(
        "width: 160px; height: 100px; border-radius: 16px;"
        "background: linear-gradient(135deg, #ff6b6b, #feca57);"
        "filter: blur(2px);"
        "box-shadow: 0px 6px 15px rgba(0,0,0,0.2);",
        "blur + shadow"));

    add_child(&body, grid);
    return AzDom_style(&body, AzCss_empty());
}

int main() {
    EffectsData model = { .dummy = 0 };
    AzRefAny data = EffectsData_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Effects Showcase");
    window.window_state.size.dimensions.width = 850.0;
    window.window_state.size.dimensions.height = 900.0;
    window.window_state.flags.decorations = AzWindowDecorations_NoTitleAutoInject;
    window.window_state.flags.background_material = AzWindowBackgroundMaterial_Sidebar;

    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
