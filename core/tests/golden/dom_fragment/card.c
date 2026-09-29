#include "azul.h"
#include <string.h>

AzDom render_card(void) {
    AzDom n0 = AzDom_createDiv();
    n0 = AzDom_withCss(n0, AZ_STR("padding: 8px"));
    n0 = AzDom_withClass(n0, AZ_STR("card"));
    AzDom n1 = AzDom_createH1WithText(AZ_STR("Title"));
    AzDom_addChild(&n0, n1);
    AzDom n2 = AzDom_createP();
    AzDom_addChild(&n2, AzDom_createTextDoNotUseWithoutBlockLevelWrapper(AZ_STR("Hello ")));
    AzDom n3 = AzDom_createBWithText(AZ_STR("world"));
    AzDom_addChild(&n2, n3);
    AzDom_addChild(&n0, n2);
    AzDom n4 = AzDom_createANoA11y(AZ_STR("https://azul.rs"), AzOptionString_some(AZ_STR("Docs")));
    AzDom_addChild(&n0, n4);
    AzDom n5 = AzDom_createButtonNoA11y(AZ_STR("Go"));
    AzDom_addChild(&n0, n5);
    return n0;
}
