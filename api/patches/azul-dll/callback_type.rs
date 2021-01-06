pub type AzCallbackReturn = AzUpdateScreen;
/// Callback for responding to window events
pub type AzCallbackType = extern "C" fn(&mut AzRefAny, AzCallbackInfo) -> AzCallbackReturn;