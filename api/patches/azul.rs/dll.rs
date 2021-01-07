    /// Return type of a regular callback - currently `AzUpdateScreen`
    pub type AzCallbackReturn = AzUpdateScreen;
    /// Callback for responding to window events
    pub type AzCallbackType = extern "C" fn(&mut AzRefAny, AzCallbackInfo) -> AzCallbackReturn;
    /// Callback fn that returns the DOM of the app
    pub type AzLayoutCallbackType = extern "C" fn(&AzRefAny, AzLayoutInfo) -> AzStyledDom;
    /// Callback for rendering to an OpenGL texture
    pub type AzGlCallbackType = extern "C" fn(&AzRefAny, AzGlCallbackInfo) -> AzGlCallbackReturn;
    /// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
    pub type AzIFrameCallbackType = extern "C" fn(&AzRefAny, AzIFrameCallbackInfo) -> AzIFrameCallbackReturn;
    /// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
    pub type AzRefAnyDestructorType = extern "C" fn(*const c_void);
    /// Callback for the `Timer` class
    pub type AzTimerCallbackType = extern "C" fn(&mut AzRefAny, &mut AzRefAny, AzTimerCallbackInfo) -> AzTimerCallbackReturn;
    /// Callback for the `Thread` class
    pub type AzThreadCallbackType= extern "C" fn(AzRefAny, AzThreadSender, AzThreadReceiver);
    /// Callback for the `WriteBack` class
    pub type AzWriteBackCallbackType =  extern "C" fn(&mut AzRefAny, AzRefAny, AzCallbackInfo) -> AzUpdateScreen;

    impl ::std::fmt::Debug for AzCallback                   { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzLayoutCallback             { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzGlCallback                 { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzIFrameCallback             { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzTimerCallback              { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzWriteBackCallback          { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}