/**
 * Minimal C test to verify linking against libazul_dll.dylib
 * 
 * Build with:
 * clang -o minimal_test minimal_test.c -I../../target/codegen -L../../target/release -lazul_dll
 * 
 * Run with:
 * DYLD_LIBRARY_PATH=../../target/release ./minimal_test
 */

#include "azul.h"
#include <stdio.h>

int main() {
    printf("Testing azul C bindings...\n");
    
    // Test 1: Create an empty CSS
    AzCss css = AzCss_empty();
    printf("✓ AzCss_empty() works\n");
    
    // Test 2: Create a simple DOM
    AzDom body = AzDom_body();
    printf("✓ AzDom_body() works\n");
    
    // Test 3: Create a div
    AzDom div = AzDom_div();
    printf("✓ AzDom_div() works\n");
    
    // Test 4: Add child
    AzDom_addChild(&body, div);
    printf("✓ AzDom_addChild() works\n");
    
    // Test 5: Style the DOM
    AzStyledDom styled = AzDom_style(&body, css);
    printf("✓ AzDom_style() works\n");
    
    printf("\nAll basic C binding tests passed!\n");
    return 0;
}
