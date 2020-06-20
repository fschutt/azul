/// Callback for rendering to an OpenGL texture
pub type AzGlCallbackType = extern "C" fn(AzGlCallbackInfoPtr) -> AzGlCallbackReturn;