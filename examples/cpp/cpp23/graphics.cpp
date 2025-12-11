// Graphics Stress Test - C++23
// g++ -std=c++23 -o graphics graphics.cpp -lazul

#include <azul.hpp>
#include <array>
using namespace azul;

// No AZ_REFLECT needed in C++17+ - RefAny uses template metaprogramming
struct StressTestData { uint32_t frame; };

// Reusable style components - String supports constexpr operator+
constexpr String SIZE_L   = "width:200px; height:120px; "_s;
constexpr String SIZE_M   = "width:180px; height:100px; "_s;
constexpr String RAD_L    = "border-radius:15px; "_s;
constexpr String RAD_S    = "border-radius:10px; "_s;
constexpr String SHADOW   = "box-shadow:0 8px 25px rgba(0,0,0,0.5);"_s;

// Gradients
constexpr String BG_LINEAR = "background:linear-gradient(135deg,#667eea,#764ba2); "_s;
constexpr String BG_RADIAL = "background:radial-gradient(circle,#f093fb,#f5576c); "_s;
constexpr String BG_CONIC  = "background:conic-gradient(#f00,#ff0,#0f0,#0ff,#00f,#f0f,#f00); "_s;

// Filters  
constexpr String BG_BLUE   = "background:#4a90d9; "_s;
constexpr String BG_GLASS  = "background:rgba(255,255,255,0.2); "_s;
constexpr String BG_PINK   = "background:#e91e63; "_s;
constexpr String FX_GRAY   = "filter:grayscale(100%);"_s;
constexpr String FX_BLUR   = "backdrop-filter:blur(10px);"_s;
constexpr String FX_FADE   = "opacity:0.6;"_s;

// Borders
constexpr String BD_RED    = "border:3px solid #f44336; "_s;
constexpr String BD_GREEN  = "border:3px solid #4caf50; "_s;
constexpr String BD_BLUE   = "border:3px solid #2196f3; "_s;
constexpr String BG_RED_L  = "background:#ffebee;"_s;
constexpr String BG_GREEN_L= "background:#e8f5e9;"_s;
constexpr String BG_BLUE_L = "background:#e3f2fd;"_s;

// Composed styles using operator+ - grouped as arrays
constexpr std::array<String, 3> GRADIENTS = {
    SIZE_L + RAD_L + BG_LINEAR + SHADOW,
    SIZE_L + RAD_L + BG_RADIAL + SHADOW,
    SIZE_L + RAD_L + BG_CONIC  + SHADOW
};

constexpr std::array<String, 3> FILTERS = {
    SIZE_M + RAD_S + BG_BLUE  + FX_GRAY,
    SIZE_M + RAD_S + BG_GLASS + FX_BLUR,
    SIZE_M + RAD_S + BG_PINK  + FX_FADE
};

constexpr std::array<String, 3> BORDERS = {
    SIZE_M + BD_RED   + RAD_S + BG_RED_L,
    SIZE_M + BD_GREEN + RAD_S + BG_GREEN_L,
    SIZE_M + BD_BLUE  + RAD_S + BG_BLUE_L
};

constexpr String ROW      = "display:flex; gap:20px; margin-bottom:20px;"_s;
constexpr String ROW_LAST = "display:flex; gap:20px;"_s;
constexpr String ROOT     = "display:flex; flex-direction:column; "_s
                          + "width:100%; height:100%; "_s
                          + "padding:20px;"_s;

auto row(const String& style, const std::array<String, 3>& styles) {
    return Dom::div()
        .with_inline_style(style)
        .with_child(Dom::div().with_inline_style(styles[0]))
        .with_child(Dom::div().with_inline_style(styles[1]))
        .with_child(Dom::div().with_inline_style(styles[2]));
}

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto root = Dom::div()
        .with_inline_style(ROOT)
        .with_child(row(ROW,      GRADIENTS))
        .with_child(row(ROW,      FILTERS))
        .with_child(row(ROW_LAST, BORDERS));
    
    return root.style(Css::empty());
}

int main() {
    auto data = RefAny::new(StressTestData{.frame = 0});
    
    auto window = WindowCreateOptions::new(layout);
    window.set_title("Graphics Stress Test");
    window.set_size(LogicalSize(800, 600));
    
    auto app = App::new(data, AppConfig::default());
    app.run(window);
}
