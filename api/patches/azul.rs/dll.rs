    /// Return type of a regular callback - currently `AzUpdateScreen`
    pub type AzCallbackReturn = AzUpdateScreen;
    /// Callback for responding to window events
    pub type AzCallbackType = fn(AzCallbackInfoPtr) -> AzCallbackReturn;
    /// Callback fn that returns the DOM of the app
    pub type AzLayoutCallbackType = fn(AzRefAny, AzLayoutInfoPtr) -> AzDom;
    /// Callback for rendering to an OpenGL texture
    pub type AzGlCallbackType = fn(AzGlCallbackInfoPtr) -> AzGlCallbackReturn;
    /// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
    pub type AzIFrameCallbackType = fn(AzIFrameCallbackInfoPtr) -> AzIFrameCallbackReturn;
    /// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
    pub type AzRefAnyDestructorType = fn(*const c_void);
    /// Callback for the `Timer` class
    pub type AzTimerCallbackType = fn(AzTimerCallbackInfoPtr) -> AzTimerCallbackReturn;
    /// Callback for the `Thread` class
    pub type AzThreadCallbackType = fn(AzRefAny) -> AzRefAny;
    /// Callback for the `Task` class
    pub type AzTaskCallbackType= fn(AzArcMutexRefAnyPtr, AzDropCheckPtr) -> AzUpdateScreen;

