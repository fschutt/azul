    /// Return type of a regular callback - currently `AzUpdateScreen`
    pub type AzCallbackReturn = AzUpdateScreen;
    /// Callback for responding to window events
    pub type AzCallbackType = extern "C" fn(AzCallbackInfoPtr) -> AzCallbackReturn;
    /// Callback fn that returns the DOM of the app
    pub type AzLayoutCallbackType = extern "C" fn(AzRefAny, AzLayoutInfoPtr) -> AzDom;
    /// Callback for rendering to an OpenGL texture
    pub type AzGlCallbackType = extern "C" fn(AzGlCallbackInfoPtr) -> AzGlCallbackReturn;
    /// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
    pub type AzIFrameCallbackType = extern "C" fn(AzIFrameCallbackInfoPtr) -> AzIFrameCallbackReturn;
    /// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
    pub type AzRefAnyDestructorType = extern "C" fn(*const c_void);
    /// Callback for the `Timer` class
    pub type AzTimerCallbackType = extern "C" fn(AzTimerCallbackInfoPtr) -> AzTimerCallbackReturn;
    /// Callback for the `Thread` class
    pub type AzThreadCallbackType = extern "C" fn(AzRefAny) -> AzRefAny;
    /// Callback for the `Task` class
    pub type AzTaskCallbackType= extern "C" fn(AzArcMutexRefAnyPtr, AzDropCheckPtr) -> AzUpdateScreen;
