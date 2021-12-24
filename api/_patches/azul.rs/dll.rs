
    #[cfg(not(feature = "link_static"))]
    impl AzString {
        #[inline]
        pub fn as_str(&self) -> &str {
            unsafe { core::str::from_utf8_unchecked(self.as_bytes()) }
        }
        #[inline]
        pub fn as_bytes(&self) -> &[u8] {
            unsafe { core::slice::from_raw_parts(self.vec.ptr, self.vec.len) }
        }
    }

    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzCallback                          { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzLayoutCallbackInner               { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzMarshaledLayoutCallbackInner      { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzRenderImageCallback               { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzIFrameCallback                    { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzTimerCallback                     { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzWriteBackCallback                 { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzThreadDestructorFn                { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzLibraryReceiveThreadMsgFn         { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzLibrarySendThreadMsgFn            { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzCheckThreadFinishedFn             { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzGetSystemTimeFn                   { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzCreateThreadFn                    { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzThreadRecvFn                      { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzThreadReceiverDestructorFn        { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzThreadSenderDestructorFn          { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzInstantPtrDestructorFn            { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzInstantPtrCloneFn                 { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzThreadSendFn                      { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}

    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzFileInputOnPathChangeCallback             { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzCheckBoxOnToggleCallback                  { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzColorInputOnValueChangeCallback           { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzTextInputOnTextInputCallback              { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzTextInputOnVirtualKeyDownCallback         { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzTextInputOnFocusLostCallback              { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzNumberInputOnFocusLostCallback            { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzNumberInputOnValueChangeCallback          { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzNodeGraphOnNodeAddedCallback              { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzNodeGraphOnNodeRemovedCallback            { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzNodeGraphOnNodeDraggedCallback            { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzNodeGraphOnNodeGraphDraggedCallback       { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzNodeGraphOnNodeConnectedCallback          { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzNodeGraphOnNodeInputDisconnectedCallback  { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzNodeGraphOnNodeOutputDisconnectedCallback { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzNodeGraphOnNodeFieldEditedCallback        { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzDropDownOnChoiceChangeCallback            { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    #[cfg(not(feature = "link_static"))]
    impl ::core::fmt::Debug for AzTabOnClickCallback                        { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}

    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzLayoutCallbackInner { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzMarshaledLayoutCallbackInner { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzRenderImageCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzIFrameCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzTimerCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzWriteBackCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzThreadDestructorFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzLibraryReceiveThreadMsgFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzLibrarySendThreadMsgFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzCheckThreadFinishedFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzGetSystemTimeFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzCreateThreadFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzThreadRecvFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzThreadReceiverDestructorFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzThreadSenderDestructorFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzInstantPtrDestructorFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzInstantPtrCloneFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzThreadSendFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }

    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzFileInputOnPathChangeCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzCheckBoxOnToggleCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzColorInputOnValueChangeCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzTextInputOnTextInputCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzTextInputOnVirtualKeyDownCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzTextInputOnFocusLostCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzNumberInputOnFocusLostCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzNumberInputOnValueChangeCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzNodeGraphOnNodeAddedCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzNodeGraphOnNodeRemovedCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzNodeGraphOnNodeDraggedCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzNodeGraphOnNodeGraphDraggedCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzNodeGraphOnNodeConnectedCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzNodeGraphOnNodeInputDisconnectedCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzNodeGraphOnNodeOutputDisconnectedCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzNodeGraphOnNodeFieldEditedCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzDropDownOnChoiceChangeCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialEq for AzTabOnClickCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }

    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzLayoutCallbackInner { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzMarshaledLayoutCallbackInner { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzRenderImageCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzIFrameCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzTimerCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzWriteBackCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzThreadDestructorFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzLibraryReceiveThreadMsgFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzLibrarySendThreadMsgFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzCheckThreadFinishedFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzGetSystemTimeFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzCreateThreadFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzThreadRecvFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzThreadReceiverDestructorFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzThreadSenderDestructorFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzInstantPtrDestructorFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzInstantPtrCloneFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzThreadSendFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }

    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzFileInputOnPathChangeCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) }}
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzCheckBoxOnToggleCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) }}
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzColorInputOnValueChangeCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) }}
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzTextInputOnTextInputCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) }}
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzTextInputOnVirtualKeyDownCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) }}
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzTextInputOnFocusLostCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) }}
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzNumberInputOnFocusLostCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) }}
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzNumberInputOnValueChangeCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) }}
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzNodeGraphOnNodeAddedCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzNodeGraphOnNodeRemovedCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzNodeGraphOnNodeDraggedCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzNodeGraphOnNodeGraphDraggedCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzNodeGraphOnNodeConnectedCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzNodeGraphOnNodeInputDisconnectedCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzNodeGraphOnNodeOutputDisconnectedCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzNodeGraphOnNodeFieldEditedCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzDropDownOnChoiceChangeCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    #[cfg(not(feature = "link_static"))]
    impl PartialOrd for AzTabOnClickCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
