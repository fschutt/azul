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

    impl AzString {
        #[inline]
        pub fn as_str(&self) -> &str {
            unsafe { std::str::from_utf8_unchecked(self.as_bytes()) }
        }
        #[inline]
        pub fn as_bytes(&self) -> &[u8] {
            unsafe { std::slice::from_raw_parts(self.vec.ptr, self.vec.len) }
        }
    }

    impl ::std::fmt::Debug for AzCallback                   { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzLayoutCallback             { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzGlCallback                 { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzIFrameCallback             { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzTimerCallback              { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzWriteBackCallback          { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::std::fmt::Debug for AzRefAny                     {
        fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            write!(f, "RefAny {{")?;
            write!(f, "_internal_ptr: {:x}", self._internal_ptr as usize)?;
            write!(f, "_internal_len: {}", self._internal_len)?;
            write!(f, "_internal_layout_size: {}", self._internal_layout_size)?;
            write!(f, "_internal_layout_align: {}", self._internal_layout_align)?;
            write!(f, "type_id: {}", self.type_id)?;
            write!(f, "type_name: \"{}\"", self.type_name.as_str())?;
            write!(f, "_sharing_info_ptr: \"{}\"", self._sharing_info_ptr as usize)?;
            write!(f, "custom_destructor: \"{}\"", self.custom_destructor as usize)?;
            write!(f, "}}")?;
            Ok(())
        }
    }