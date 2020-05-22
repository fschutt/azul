pub type AzCallbackReturn = AzUpdateScreen;
/// Callback for responding to window events
pub type AzCallback = fn(AzCallbackInfoPtr) -> AzCallbackReturn;