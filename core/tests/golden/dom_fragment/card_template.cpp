#include "azul20.hpp"
#include <string>

using namespace azul;

Dom render_card(const std::string& title, const std::string& text, const std::string& href, const std::string& author) {
    return Dom::create_div()
        .with_class(String("card"))
        .with_child(Dom::create_h2_with_text(String(title)))
        .with_child(Dom::create_p_with_text(String(text)))
        .with_child(Dom::create_a_no_a11y(String(href), OptionString::some(String("Read more"))))
        .with_child(Dom::create_span_with_text(String(std::string("by ") + author)));
}
