/**
 * Simple C link test - uses minimal declarations to verify linking works.
 * This bypasses header generation issues by declaring only what we need.
 * 
 * Build with:
 * clang -o simple_link_test simple_link_test.c -L../../target/release -lazul_dll
 * 
 * Run with:
 * DYLD_LIBRARY_PATH=../../target/release ./simple_link_test
 */

#include <stdio.h>
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

// Minimal type declarations for testing
typedef struct AzString {
    const uint8_t* vec_ptr;
    size_t len;
    size_t cap;
} AzString;

typedef struct AzDom {
    void* root; // Opaque pointer
    size_t head;
    size_t tail;
} AzDom;

typedef struct AzCss {
    void* ptr;
    bool run_destructor;
} AzCss;

typedef struct AzStyledDom {
    void* ptr;
    bool run_destructor;
} AzStyledDom;

// External function declarations
extern AzCss AzCss_empty(void);
extern AzDom AzDom_body(void);
extern AzDom AzDom_div(void);

int main() {
    printf("Testing azul DLL linking...\n");
    
    // Test 1: Create empty CSS
    AzCss css = AzCss_empty();
    printf("✓ AzCss_empty() returned (ptr=%p)\n", css.ptr);
    
    // Test 2: Create body DOM
    AzDom body = AzDom_body();
    printf("✓ AzDom_body() returned (root=%p)\n", body.root);
    
    // Test 3: Create div
    AzDom div = AzDom_div();
    printf("✓ AzDom_div() returned (root=%p)\n", div.root);
    
    printf("\n✓ All link tests passed - DLL is working!\n");
    return 0;
}
