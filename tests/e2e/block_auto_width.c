// E2E test: block element with width:auto should fill parent
// This tests the scenario where a block element has no explicit width
// and should fill its parent's content box.
// This matches the ProgressBar scenario.

#include <azul.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct {
    int dummy;
} TestData;

void TestData_destructor(void* data) {
    // Nothing to free
}

AZ_REFLECT(TestData, TestData_destructor);

AzStyledDom render(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)data;
    (void)info;
    
    // Body with explicit width (simulating parent)
    AzDom body = AzDom_createBody();
    AzString body_style = AzString_copyFromBytes(
        "width: 400px; height: 200px; padding: 0px;", 0, 42
    );
    AzDom_setInlineStyle(&body, body_style);
    
    // Inner container (like progressbar container) - NO WIDTH SET (should be auto)
    // This is the key test: width:auto on a block element should fill parent
    AzDom container = AzDom_createDiv();
    AzString container_class = AzString_copyFromBytes("container", 0, 9);
    AzDom_addClass(&container, container_class);
    // Only height is set, width is auto (default)
    AzString container_style = AzString_copyFromBytes(
        "display: block; height: 20px; background-color: #ff6666;", 0, 56
    );
    AzDom_setInlineStyle(&container, container_style);
    
    // Child element with absolute positioning and percentage width
    {
        AzDom child = AzDom_createDiv();
        AzString class_name = AzString_copyFromBytes("bar", 0, 3);
        AzDom_addClass(&child, class_name);
        AzString style = AzString_copyFromBytes(
            "position: absolute; top: 0px; left: 0px; width: 50%; height: 100%; background-color: #66ff66;", 0, 94
        );
        AzDom_setInlineStyle(&child, style);
        AzDom_addChild(&container, child);
    }
    
    AzDom_addChild(&body, container);
    
    return AzStyledDom_fromDom(body, AzCss_empty(), info.window_handle, info.renderer_resources);
}

int main() {
    TestData data = { .dummy = 0 };
    AzRefAny model = AzRefAny_new(&data, sizeof(TestData), 0, 0, TestData_destructor, TestData_destructor);
    
    AzAppConfig config = AzAppConfig_new(AzLayoutSolver_Default);
    AzApp app = AzApp_new(model, config);
    
    AzWindowCreateOptions options = AzWindowCreateOptions_new(render);
    AzWindow window = AzWindow_new(app, options);
    
    AzApp_run(app, window);
    
    return 0;
}
