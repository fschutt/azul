#include <cstdio>
#include "styles.hpp"

int main() {
    AzCssPropertyWithConditionsVec style_btn_value = style_btn();
    std::printf("style_btn: %zu properties\n", style_btn_value.len);
    AzCssPropertyWithConditionsVec_delete(&style_btn_value);
    return 0;
}
