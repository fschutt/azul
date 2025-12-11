/**
 * Graphics Stress Test - C Version
 * 
 * Compile with:
 *   cc -o graphics graphics.c -lazul
 */

#include <azul.h>

typedef struct { uint32_t frame; } StressTestData;
void StressTestData_destructor(StressTestData* s) { }
AZ_REFLECT(StressTestData, StressTestData_destructor);

/* Style constants */
static const AzString ROOT_STYLE = AzString_fromConstStr(
    "display:flex; flex-direction:column; width:100%; height:100%; padding:20px;"
);
static const AzString ROW_STYLE = AzString_fromConstStr(
    "display:flex; gap:20px; margin-bottom:20px;"
);
static const AzString ROW_STYLE_LAST = AzString_fromConstStr(
    "display:flex; gap:20px;"
);

static const AzString GRADIENT_LINEAR = AzString_fromConstStr(
    "width:200px; height:120px; border-radius:15px; "
    "background:linear-gradient(135deg,#667eea,#764ba2); "
    "box-shadow:0 8px 25px rgba(0,0,0,0.5);"
);
static const AzString GRADIENT_RADIAL = AzString_fromConstStr(
    "width:200px; height:120px; border-radius:15px; "
    "background:radial-gradient(circle,#f093fb,#f5576c); "
    "box-shadow:0 8px 25px rgba(0,0,0,0.5);"
);
static const AzString GRADIENT_CONIC = AzString_fromConstStr(
    "width:200px; height:120px; border-radius:15px; "
    "background:conic-gradient(#f00,#ff0,#0f0,#0ff,#00f,#f0f,#f00); "
    "box-shadow:0 8px 25px rgba(0,0,0,0.5);"
);

static const AzString FILTER_GRAYSCALE = AzString_fromConstStr(
    "width:180px; height:100px; border-radius:10px; "
    "background:#4a90d9; filter:grayscale(100%);"
);
static const AzString FILTER_BLUR = AzString_fromConstStr(
    "width:180px; height:100px; border-radius:10px; "
    "background:rgba(255,255,255,0.2); backdrop-filter:blur(10px);"
);
static const AzString FILTER_OPACITY = AzString_fromConstStr(
    "width:180px; height:100px; border-radius:10px; "
    "background:#e91e63; opacity:0.6;"
);

static const AzString BORDER_RED = AzString_fromConstStr(
    "width:180px; height:100px; border:3px solid #f44336; "
    "border-radius:10px; background:#ffebee;"
);
static const AzString BORDER_GREEN = AzString_fromConstStr(
    "width:180px; height:100px; border:3px solid #4caf50; "
    "border-radius:10px; background:#e8f5e9;"
);
static const AzString BORDER_BLUE = AzString_fromConstStr(
    "width:180px; height:100px; border:3px solid #2196f3; "
    "border-radius:10px; background:#e3f2fd;"
);

AzDom row(AzString style, AzString s1, AzString s2, AzString s3) {
    AzDom r = AzDom_div();
    AzDom c1 = AzDom_div(); AzDom_setInlineStyle(&c1, s1);
    AzDom c2 = AzDom_div(); AzDom_setInlineStyle(&c2, s2);
    AzDom c3 = AzDom_div(); AzDom_setInlineStyle(&c3, s3);
    AzDom_setInlineStyle(&r, style);
    AzDom_addChild(&r, c1);
    AzDom_addChild(&r, c2);
    AzDom_addChild(&r, c3);
    return r;
}

AzStyledDom layout(AzRefAny* data, AzLayoutCallbackInfo* info) {
    AzDom root = AzDom_div();
    AzDom_setInlineStyle(&root, ROOT_STYLE);
    AzDom_addChild(&root, row(ROW_STYLE, GRADIENT_LINEAR, GRADIENT_RADIAL, GRADIENT_CONIC));
    AzDom_addChild(&root, row(ROW_STYLE, FILTER_GRAYSCALE, FILTER_BLUR, FILTER_OPACITY));
    AzDom_addChild(&root, row(ROW_STYLE_LAST, BORDER_RED, BORDER_GREEN, BORDER_BLUE));
    return AzDom_style(&root, AzCss_empty());
}

int main() {
    StressTestData model = { .frame = 0 };
    AzRefAny data = StressTestData_upcast(model);
    
    AzWindowCreateOptions window = AzWindowCreateOptions_new(layout);
    window.state.title = AzString_fromConstStr("Graphics Stress Test");
    window.state.size.dimensions.width = 800.0;
    window.state.size.dimensions.height = 600.0;
    
    AzApp app = AzApp_new(data, AzAppConfig_default());
    AzApp_run(&app, window);
    return 0;
}
