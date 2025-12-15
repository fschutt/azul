// Minimal C test to check if the GUI renders and responds to events
#include "azul.h"
#include <stdio.h>
#include <string.h>
#include <stdlib.h>

// Simple data model
typedef struct { int counter; } MyData;

// Destructor for MyData (called when RefAny is dropped)
void MyData_destructor(void* ptr) {
    printf("[C] MyData destructor called\n");
    // MyData is stack-allocated via the macro, so nothing to free here
}

// Use AZ_REFLECT macro to generate RTTI functions
AZ_REFLECT(MyData, MyData_destructor);

// Layout callback - returns a simple DOM
AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    printf("[C] layout() called, window size: %.0fx%.0f\n", 
           info.window_size.dimensions.width,
           info.window_size.dimensions.height);
    
    // Create a label
    const char* text = "Hello from C!";
    AzString label_text = AzString_copyFromBytes((uint8_t*)text, 0, strlen(text));
    AzDom label = AzDom_text(label_text);
    
    // Create body and add label
    AzDom body = AzDom_body();
    AzDom_addChild(&body, label);
    
    // Create styled DOM
    AzCss css = AzCss_empty();
    AzStyledDom result = AzStyledDom_new(body, css);
    
    printf("[C] layout() returning StyledDom\n");
    return result;
}

int main() {
    printf("[C] Starting minimal test...\n");
    
    // Create app data using the AZ_REFLECT-generated upcast function
    MyData model = { .counter = 0 };
    AzRefAny data = MyData_upcast(model);
    
    // Create window with our layout callback
    AzWindowCreateOptions window = AzWindowCreateOptions_new(layout);
    
    // Set window title
    const char* title = "C Test Window";
    window.state.title = AzString_copyFromBytes((uint8_t*)title, 0, strlen(title));
    window.state.size.dimensions.width = 400.0;
    window.state.size.dimensions.height = 300.0;
    
    printf("[C] Created window options\n");
    
    // Create app with default config
    AzAppConfig config = AzAppConfig_default();
    AzApp app = AzApp_new(data, config);
    
    printf("[C] Created app, calling run()...\n");
    
    // Run the app
    AzApp_run(&app, window);
    
    printf("[C] App finished\n");
    return 0;
}
