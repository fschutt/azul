// Graphics Stress Test - C++17
// g++ -std=c++17 -o graphics graphics.cpp -lazul

#include <azul.hpp>
#include <array>
using namespace azul;
using namespace std::string_view_literals;

// No AZ_REFLECT needed in C++17+ - RefAny uses template metaprogramming
struct StressTestData { uint32_t frame; };

constexpr auto ROW_STYLE      = "display:flex; gap:20px; margin-bottom:20px;"sv;
constexpr auto ROW_STYLE_LAST = "display:flex; gap:20px;"sv;
constexpr auto ROOT_STYLE     = "display:flex; flex-direction:column; "
                                "width:100%; height:100%; "
                                "padding:20px;"sv;

// Composed styles using adjacent string literal concatenation
const std::array<const char*, 3> GRADIENTS = {{
    "width:200px; height:120px; "
    "border-radius:15px; "
    "background:linear-gradient(135deg,#667eea,#764ba2); "
    "box-shadow:0 8px 25px rgba(0,0,0,0.5);",
    
    "width:200px; height:120px; "
    "border-radius:15px; "
    "background:radial-gradient(circle,#f093fb,#f5576c); "
    "box-shadow:0 8px 25px rgba(0,0,0,0.5);",
    
    "width:200px; height:120px; "
    "border-radius:15px; "
    "background:conic-gradient(#f00,#ff0,#0f0,#0ff,#00f,#f0f,#f00); "
    "box-shadow:0 8px 25px rgba(0,0,0,0.5);"
}};

const std::array<const char*, 3> FILTERS = {{
    "width:180px; height:100px; "
    "border-radius:10px; "
    "background:#4a90d9; "
    "filter:grayscale(100%);",
    
    "width:180px; height:100px; "
    "border-radius:10px; "
    "background:rgba(255,255,255,0.2); "
    "backdrop-filter:blur(10px);",
    
    "width:180px; height:100px; "
    "border-radius:10px; "
    "background:#e91e63; "
    "opacity:0.6;"
}};

const std::array<const char*, 3> BORDERS = {{
    "width:180px; height:100px; "
    "border:3px solid #f44336; "
    "border-radius:10px; "
    "background:#ffebee;",
    
    "width:180px; height:100px; "
    "border:3px solid #4caf50; "
    "border-radius:10px; "
    "background:#e8f5e9;",
    
    "width:180px; height:100px; "
    "border:3px solid #2196f3; "
    "border-radius:10px; "
    "background:#e3f2fd;"
}};

auto row(std::string_view row_style, const std::array<const char*, 3>& styles) {
    return Dom::div()
        .with_inline_style(row_style)
        .with_child(Dom::div().with_inline_style(styles[0]))
        .with_child(Dom::div().with_inline_style(styles[1]))
        .with_child(Dom::div().with_inline_style(styles[2]));
}

StyledDom layout(RefAny& data, LayoutCallbackInfo& info) {
    auto root = Dom::div()
        .with_inline_style(ROOT_STYLE)
        .with_child(row(ROW_STYLE,      GRADIENTS))
        .with_child(row(ROW_STYLE,      FILTERS))
        .with_child(row(ROW_STYLE_LAST, BORDERS));
    
    return root.style(Css::empty());
}

int main() {
    StressTestData model{0};
    auto data = RefAny::new(model);
    
    auto window = WindowCreateOptions::new(layout);
    window.set_title("Graphics Stress Test"sv);
    window.set_size(LogicalSize(800, 600));
    
    auto app = App::new(data, AppConfig::default());
    app.run(window);
}
