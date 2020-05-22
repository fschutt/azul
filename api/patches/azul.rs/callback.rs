pub type CallbackReturn = AzUpdateScreen;
/// Callback for responding to window events
pub type Callback = fn(AzCallbackInfoPtr) -> AzCallbackReturn;