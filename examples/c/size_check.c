#include <azul.h>
#include <stdio.h>

int main() {
    printf("C Size checks:\n");
    printf("  sizeof(AzRefAny) = %zu\n", sizeof(AzRefAny));
    printf("  sizeof(AzRefCount) = %zu\n", sizeof(AzRefCount));
    printf("  alignof(AzRefAny) = %zu\n", _Alignof(AzRefAny));
    printf("  alignof(AzRefCount) = %zu\n", _Alignof(AzRefCount));
    printf("  sizeof(void*) = %zu\n", sizeof(void*));
    printf("  sizeof(uint64_t) = %zu\n", sizeof(uint64_t));
    printf("  sizeof(bool) = %zu\n", sizeof(bool));
    
    // Check field offsets
    AzRefAny r;
    printf("\nField offsets in AzRefAny:\n");
    printf("  _internal_ptr: %zu\n", (size_t)((char*)&r._internal_ptr - (char*)&r));
    printf("  sharing_info: %zu\n", (size_t)((char*)&r.sharing_info - (char*)&r));
    printf("  instance_id: %zu\n", (size_t)((char*)&r.instance_id - (char*)&r));
    printf("  run_destructor: %zu\n", (size_t)((char*)&r.run_destructor - (char*)&r));
    
    printf("\nApp sizes:\n");
    printf("  sizeof(AzApp) = %zu\n", sizeof(AzApp));
    AzApp a;
    printf("  offsetof(AzApp, ptr) = %zu\n", (size_t)((char*)&a.ptr - (char*)&a));
    printf("  offsetof(AzApp, run_destructor) = %zu\n", (size_t)((char*)&a.run_destructor - (char*)&a));
    
    printf("\nWindowCreateOptions sizes:\n");
    printf("  sizeof(AzWindowCreateOptions) = %zu\n", sizeof(AzWindowCreateOptions));
    printf("  sizeof(AzFullWindowState) = %zu\n", sizeof(AzFullWindowState));
    printf("  sizeof(AzAppConfig) = %zu\n", sizeof(AzAppConfig));
    
    printf("\nNested type sizes:\n");
    printf("  sizeof(AzOptionRendererOptions) = %zu\n", sizeof(AzOptionRendererOptions));
    printf("  sizeof(AzOptionWindowTheme) = %zu\n", sizeof(AzOptionWindowTheme));
    printf("  sizeof(AzOptionCallback) = %zu\n", sizeof(AzOptionCallback));
    printf("  sizeof(AzLayoutCallback) = %zu\n", sizeof(AzLayoutCallback));
    printf("  sizeof(AzKeyboardState) = %zu\n", sizeof(AzKeyboardState));
    printf("  sizeof(AzMouseState) = %zu\n", sizeof(AzMouseState));
    printf("  sizeof(AzTouchState) = %zu\n", sizeof(AzTouchState));
    printf("  sizeof(AzPlatformSpecificOptions) = %zu\n", sizeof(AzPlatformSpecificOptions));
    printf("  sizeof(AzWindowSize) = %zu\n", sizeof(AzWindowSize));
    printf("  sizeof(AzWindowFlags) = %zu\n", sizeof(AzWindowFlags));
    printf("  sizeof(AzWindowPosition) = %zu\n", sizeof(AzWindowPosition));
    printf("  sizeof(AzImePosition) = %zu\n", sizeof(AzImePosition));
    printf("  sizeof(AzDebugState) = %zu\n", sizeof(AzDebugState));
    printf("  sizeof(AzWindowTheme) = %zu\n", sizeof(AzWindowTheme));
    printf("  sizeof(AzRendererOptions) = %zu\n", sizeof(AzRendererOptions));
    printf("  sizeof(AzString) = %zu\n", sizeof(AzString));
    printf("  sizeof(AzColorU) = %zu\n", sizeof(AzColorU));
    printf("  sizeof(AzOptionU32) = %zu\n", sizeof(AzOptionU32));
    
    printf("\nPlatformSpecificOptions breakdown:\n");
    printf("  sizeof(AzWindowsWindowOptions) = %zu\n", sizeof(AzWindowsWindowOptions));
    printf("  sizeof(AzLinuxWindowOptions) = %zu\n", sizeof(AzLinuxWindowOptions));
    printf("  sizeof(AzMacWindowOptions) = %zu\n", sizeof(AzMacWindowOptions));
    printf("  sizeof(AzWasmWindowOptions) = %zu\n", sizeof(AzWasmWindowOptions));
    
    printf("\nLinuxWindowOptions breakdown:\n");
    printf("  sizeof(AzOptionX11Visual) = %zu\n", sizeof(AzOptionX11Visual));
    printf("  sizeof(AzOptionI32) = %zu\n", sizeof(AzOptionI32));
    printf("  sizeof(AzStringPairVec) = %zu\n", sizeof(AzStringPairVec));
    printf("  sizeof(AzXWindowTypeVec) = %zu\n", sizeof(AzXWindowTypeVec));
    printf("  sizeof(AzOptionString) = %zu\n", sizeof(AzOptionString));
    printf("  sizeof(AzOptionLogicalSize) = %zu\n", sizeof(AzOptionLogicalSize));
    printf("  sizeof(AzOptionWaylandTheme) = %zu\n", sizeof(AzOptionWaylandTheme));
    printf("  sizeof(AzWaylandTheme) = %zu\n", sizeof(AzWaylandTheme));
    printf("  sizeof(AzUserAttentionType) = %zu\n", sizeof(AzUserAttentionType));
    printf("  sizeof(AzOptionWindowIcon) = %zu\n", sizeof(AzOptionWindowIcon));
    printf("  sizeof(AzOptionLinuxDecorationsState) = %zu\n", sizeof(AzOptionLinuxDecorationsState));
    
    return 0;
}