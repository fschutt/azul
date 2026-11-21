#include <stdio.h>
#include "styles.h"

int main(void) {
    AzCssPropertyWithConditionsVec style_btn_value = style_btn();
    printf("style_btn: %zu properties\n", style_btn_value.len);
    AzCssPropertyWithConditionsVec_delete(&style_btn_value);
    return 0;
}
