// g++ -std=c++23 -o hello-world hello-world.cpp -lazul
//
// On a modules-aware toolchain you can replace the includes below with
// `import std; import azul;` after precompiling the sibling `azul.cppm`:
//   clang++ -std=c++23 -fmodules -c azul.cppm
//   clang++ -std=c++23 -fmodules -o hello-world hello-world.cpp -lazul

#include "azul23.hpp"
#include <expected>
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
static std::expected<AzUrl, AzUrlParseError> parse_homepage_url() {
    return Url::parse("https://example.com/"sv);
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = downcast_ref<MyDataModel>(data_wrapper);
    if (!d) return AzDom_createBody();

    auto homepage = parse_homepage_url();
    Css css = Css::from_string(homepage.has_value()
        ? String(R"(body { background-color: #efefef; })")
        : String(R"(body { background-color: #ffaaaa; })"));

    // Deducing-`this`: every `with_*` method is a member template
    // `auto with_xxx(this Self&& self, …)`. Same one method body works on
    // l-values and r-values - so the chain can mix freely. Below: `body` is
    // an l-value passed to `.with_child` (deducing-`this` substitutes
    // `Self = Dom&`); the chain on the right starts from an r-value
    // (`Self = Dom`).
    Dom body = Dom::create_body();
    body = body.with_child(Dom::p_with_text(String(std::to_string(d->counter).c_str()))
        .with_css("font-size: 50px;"sv));
    body = body.with_child(Button::create("Increase counter"sv)
        .with_button_type(AzButtonType_Primary)
        .with_on_click(data_wrapper.clone(), on_click)
        .dom());
    return body.style(std::move(css)).release();
}

AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    RefAny data_wrapper(data);
    auto* d = downcast_mut<MyDataModel>(data_wrapper);
    if (!d) return AzUpdate_DoNothing;
    d->counter += 1;
    return AzUpdate_RefreshDom;
}

int main() {
    MyDataModel model = { 5 };
    RefAny data = upcast<MyDataModel>(std::move(model));

    WindowCreateOptions window = WindowCreateOptions::create(layout);
    App app = App::create(std::move(data), AppConfig::default_());
    app.run(std::move(window));
    return 0;
}
