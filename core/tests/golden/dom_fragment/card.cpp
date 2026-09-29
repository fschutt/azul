#include "azul20.hpp"
#include <string>

using namespace azul;

Dom render_card() {
    return Dom::create_div()
        .with_css(String("padding: 8px"))
        .with_class(String("card"))
        .with_child(Dom::create_h1_with_text(String("Title")))
        .with_child(Dom::create_p()
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(String("Hello ")))
            .with_child(Dom::create_b_with_text(String("world"))))
        .with_child(Dom::create_a_no_a11y(String("https://azul.rs"), OptionString::some(String("Docs"))))
        .with_child(Dom::create_button_no_a11y(String("Go")));
}
