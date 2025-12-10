/**
 * Graphics Stress Test - C Version
 * 
 * This example tests various graphical features:
 * - Linear, Radial, and Conic gradients with rounded corners and box shadows
 * - Bordered boxes
 * - CSS filters, backdrop blur, and opacity
 *
 * Compile with:
 *   cc -o graphics_stress_test graphics_stress_test.c -L../../target/debug -lazul_dll -Wl,-rpath,../../target/debug
 *
 * Or on macOS:
 *   clang -o graphics_stress_test graphics_stress_test.c -L../../target/debug -lazul_dll -Wl,-rpath,@executable_path/../../target/debug
 */

#include "../../target/codegen/azul.h"
#include <stdio.h>
#include <string.h>

/* Helper macro to create AzString from C string literal */
#define AZ_STRING(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

/* Simple data structure for the stress test */
typedef struct {
    uint32_t frame_count;
} StressTestData;

/* Layout callback - builds the DOM */
AzStyledDom stress_test_layout(AzRefAny* data, AzLayoutCallbackInfo* info) {
    fprintf(stderr, "[stress_test_layout] Called!\n");
    
    /* Build the main container */
    AzDom root = AzDom_div();
    AzDom_setInlineStyle(&root, AZ_STRING(
        "display: flex;"
        "flex-direction: column;"
        "width: 100%;"
        "height: 100%;"
        "padding: 20px;"
        "background-color: #1a1a2e;"
    ));
    
    /* === ROW 1: Gradients with rounded corners and box shadows === */
    AzDom row1 = AzDom_div();
    AzDom_setInlineStyle(&row1, AZ_STRING(
        "display: flex; flex-direction: row; margin-bottom: 20px; gap: 20px;"
    ));
    
    /* Linear Gradient */
    AzDom linear_grad = AzDom_div();
    AzDom_setInlineStyle(&linear_grad, AZ_STRING(
        "width: 200px;"
        "height: 120px;"
        "border-radius: 15px;"
        "box-shadow: 0px 8px 25px rgba(0, 0, 0, 0.5);"
        "background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);"
    ));
    AzDom_addChild(&row1, linear_grad);
    
    /* Radial Gradient */
    AzDom radial_grad = AzDom_div();
    AzDom_setInlineStyle(&radial_grad, AZ_STRING(
        "width: 200px;"
        "height: 120px;"
        "border-radius: 15px;"
        "box-shadow: 0px 8px 25px rgba(0, 0, 0, 0.5);"
        "background: radial-gradient(circle at center, #f093fb 0%, #f5576c 100%);"
    ));
    AzDom_addChild(&row1, radial_grad);
    
    /* Conic Gradient (rainbow) */
    AzDom conic_grad = AzDom_div();
    AzDom_setInlineStyle(&conic_grad, AZ_STRING(
        "width: 200px;"
        "height: 120px;"
        "border-radius: 15px;"
        "box-shadow: 0px 8px 25px rgba(0, 0, 0, 0.5);"
        "background: conic-gradient(from 0deg, #ff0000, #ff7f00, #ffff00, #00ff00, #0000ff, #9400d3, #ff0000);"
    ));
    AzDom_addChild(&row1, conic_grad);
    
    AzDom_addChild(&root, row1);
    
    /* === ROW 2: Filter effect boxes === */
    AzDom row2 = AzDom_div();
    AzDom_setInlineStyle(&row2, AZ_STRING(
        "display: flex; flex-direction: row; margin-bottom: 20px; gap: 20px;"
    ));
    
    /* Grayscale filter */
    AzDom grayscale_box = AzDom_div();
    AzDom_setInlineStyle(&grayscale_box, AZ_STRING(
        "width: 180px;"
        "height: 100px;"
        "border-radius: 10px;"
        "background-color: #4a90d9;"
        "filter: grayscale(100%);"
    ));
    AzDom_addChild(&row2, grayscale_box);
    
    /* Backdrop blur (semi-transparent) */
    AzDom blur_box = AzDom_div();
    AzDom_setInlineStyle(&blur_box, AZ_STRING(
        "width: 180px;"
        "height: 100px;"
        "border-radius: 10px;"
        "background-color: rgba(255, 255, 255, 0.2);"
        "backdrop-filter: blur(10px);"
        "border: 1px solid rgba(255, 255, 255, 0.3);"
    ));
    AzDom_addChild(&row2, blur_box);
    
    /* Opacity */
    AzDom opacity_box = AzDom_div();
    AzDom_setInlineStyle(&opacity_box, AZ_STRING(
        "width: 180px;"
        "height: 100px;"
        "border-radius: 10px;"
        "background-color: #e91e63;"
        "opacity: 0.6;"
    ));
    AzDom_addChild(&row2, opacity_box);
    
    AzDom_addChild(&root, row2);
    
    /* === ROW 3: Bordered boxes === */
    AzDom row3 = AzDom_div();
    AzDom_setInlineStyle(&row3, AZ_STRING(
        "display: flex; flex-direction: row; margin-bottom: 20px; gap: 20px;"
    ));
    
    /* Red bordered box */
    AzDom red_box = AzDom_div();
    AzDom_setInlineStyle(&red_box, AZ_STRING(
        "width: 180px;"
        "height: 100px;"
        "border: 3px solid #f44336;"
        "border-radius: 10px;"
        "background-color: #ffebee;"
    ));
    AzDom_addChild(&row3, red_box);
    
    /* Green bordered box */
    AzDom green_box = AzDom_div();
    AzDom_setInlineStyle(&green_box, AZ_STRING(
        "width: 180px;"
        "height: 100px;"
        "border: 3px solid #4caf50;"
        "border-radius: 10px;"
        "background-color: #e8f5e9;"
    ));
    AzDom_addChild(&row3, green_box);
    
    /* Blue bordered box */
    AzDom blue_box = AzDom_div();
    AzDom_setInlineStyle(&blue_box, AZ_STRING(
        "width: 180px;"
        "height: 100px;"
        "border: 3px solid #2196f3;"
        "border-radius: 10px;"
        "background-color: #e3f2fd;"
    ));
    AzDom_addChild(&row3, blue_box);
    
    AzDom_addChild(&root, row3);
    
    /* === ROW 4: Shadow cascade === */
    AzDom row4 = AzDom_div();
    AzDom_setInlineStyle(&row4, AZ_STRING(
        "display: flex; flex-direction: row; gap: 20px;"
    ));
    
    AzDom shadow_box = AzDom_div();
    AzDom_setInlineStyle(&shadow_box, AZ_STRING(
        "width: 150px;"
        "height: 150px;"
        "background: linear-gradient(180deg, #4facfe 0%, #00f2fe 100%);"
        "border-radius: 20px;"
        "box-shadow: 0px 20px 40px rgba(0, 0, 0, 0.3);"
    ));
    AzDom_addChild(&row4, shadow_box);
    
    AzDom_addChild(&root, row4);
    
    fprintf(stderr, "[stress_test_layout] DOM created\n");
    
    /* Style the DOM with empty CSS (inline styles will be used) */
    AzCss css = AzCss_empty();
    AzStyledDom styled = AzDom_style(&root, css);
    
    fprintf(stderr, "[stress_test_layout] StyledDom has %zu nodes\n", AzStyledDom_nodeCount(&styled));
    
    return styled;
}

int main(int argc, char** argv) {
    fprintf(stderr, "===========================================\n");
    fprintf(stderr, "    Graphics Stress Test (C Version)       \n");
    fprintf(stderr, "===========================================\n");
    fprintf(stderr, "\n");
    fprintf(stderr, "Testing:\n");
    fprintf(stderr, "  - Linear, Radial, Conic gradients\n");
    fprintf(stderr, "  - Rounded corners (border-radius)\n");
    fprintf(stderr, "  - Box shadows\n");
    fprintf(stderr, "  - Bordered boxes\n");
    fprintf(stderr, "  - CSS filters (grayscale)\n");
    fprintf(stderr, "  - Backdrop blur\n");
    fprintf(stderr, "  - Opacity\n");
    fprintf(stderr, "\n");
    
    /* Create initial data */
    StressTestData model = { .frame_count = 0 };
    
    /* Create RefAny from the model
     * Note: In C, we use AzRefAny_newC which requires:
     * - pointer to data (wrapped in AzGlVoidPtrConst)
     * - size of data
     * - alignment of data
     * - type_id (we use 0 for simple cases)
     * - type_name string
     * - destructor (NULL if no cleanup needed)
     */
    AzGlVoidPtrConst model_ptr = { .ptr = &model, .run_destructor = false };
    AzRefAny data = AzRefAny_newC(
        model_ptr,
        sizeof(StressTestData),
        _Alignof(StressTestData),
        0,  /* type_id */
        AZ_STRING("StressTestData"),
        NULL  /* no destructor needed for stack-allocated data */
    );
    
    /* Create app config */
    AzAppConfig config = AzAppConfig_new();
    
    /* Create app */
    AzApp app = AzApp_new(data, config);
    
    /* Create window options */
    AzWindowCreateOptions window = AzWindowCreateOptions_new(stress_test_layout);
    window.state.title = AZ_STRING("Graphics Stress Test (C)");
    window.state.size.dimensions.width = 800.0;
    window.state.size.dimensions.height = 600.0;
    
    /* Run the application */
    AzApp_run(&app, window);
    
    /* Cleanup */
    AzApp_delete(&app);
    
    return 0;
}
