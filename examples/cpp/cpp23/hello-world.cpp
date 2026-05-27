// g++ -std=c++23 -o hello-world hello-world.cpp -lazul
//
// On a modules-aware toolchain you can replace the includes below with
// `import std; import azul;` after precompiling the sibling `azul.cppm`:
//   clang++ -std=c++23 -fmodules -c azul.cppm
//   clang++ -std=c++23 -fmodules -o hello-world hello-world.cpp -lazul

#include "azul23.hpp"
#include <version>
#if defined(__cpp_lib_expected)
#include <expected>
#endif
#include <string>
#include <string_view>

using namespace azul;
using namespace std::string_view_literals;

struct MyDataModel {
    uint32_t counter;
};

AzUpdate on_click(AzRefAny data, AzCallbackInfo info);

// Every ResultXxx wrapper has `toStdExpected() &&` (and an implicit conversion
// operator) generated from its sibling enum's Ok/Err payload types. The
// Url::parse demo here returns AzResultUrlUrlParseError — moved into a
// std::expected, monadic chaining via and_then/or_else just works.
//
// std::expected is C++23 *library* support, which some toolchains lack even
// with -std=c++2b (e.g. the CI clang). Guard the demo on __cpp_lib_expected;
// `homepage_ok()` is the portable result used by layout() either way.
#if defined(__cpp_lib_expected)
static std::expected<AzUrl, AzUrlParseError> parse_homepage_url() {
    return Url::parse("https://example.com/"sv);
}
static bool homepage_ok() { return parse_homepage_url().has_value(); }
#else
static bool homepage_ok() { return Url::parse("https://example.com/"sv).isOk(); }
#endif

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = data_wrapper.downcast_ref<MyDataModel>();
    if (!d) return AzDom_createBody();

    String css = homepage_ok()
        ? String(R"(body { background-color: #efefef; })")
        : String(R"(body { background-color: #ffaaaa; })");

    // Deducing-`this`: every `with_*` method is a member template
    // `auto with_xxx(this Self&& self, …)`. Same one method body works on
    // l-values and r-values - so the chain can mix freely. Below: `body` is
    // an l-value passed to `.with_child` (deducing-`this` substitutes
    // `Self = Dom&`); the chain on the right starts from an r-value
    // (`Self = Dom`).
    Dom body = Dom::create_body();
    body = body.with_child(Dom::create_p_with_text(String(std::to_string(d->counter).c_str()))
        .with_css("font-size: 50px;"sv));
    body = body.with_child(Button::create("Increase counter"sv)
        .with_button_type(AzButtonType_Primary)
        .with_on_click(data_wrapper.clone(), on_click)
        .dom());
    body = body.with_css(std::move(css));
    return std::move(body);
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = data_wrapper.downcast_mut<MyDataModel>();
    if (!d) return AzUpdate_DoNothing;
    d->counter += 1;
    return AzUpdate_RefreshDom;
}

int main() {
    MyDataModel model = { 5 };
    RefAny data = RefAny::create(std::move(model));

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
