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

    impl ::core::fmt::Debug for AzCallback                   { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzLayoutCallback             { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzGlCallback                 { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzIFrameCallback             { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzTimerCallback              { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzWriteBackCallback          { fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzThreadDestructorFn         { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzLibraryReceiveThreadMsgFn  { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzLibrarySendThreadMsgFn     { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzCheckThreadFinishedFn      { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzGetSystemTimeFn            { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzCreateThreadFn             { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzThreadRecvFn               { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzThreadReceiverDestructorFn { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzThreadSenderDestructorFn   { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzInstantPtrDestructorFn     { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzInstantPtrCloneFn          { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}
    impl ::core::fmt::Debug for AzThreadSendFn               { fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result { write!(f, "{:x}", self.cb as usize) }}

    impl ::core::fmt::Debug for AzRefCount {
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            let ptr = unsafe { &*self.ptr };
            write!(f, "RefAnyRefCount {{\r\n")?;
            write!(f, "    num_copies: {}\r\n", ptr.num_copies)?;
            write!(f, "    num_refs: {}\r\n", ptr.num_refs)?;
            write!(f, "    num_mutable_refs: {}\r\n", ptr.num_mutable_refs)?;
            write!(f, "    _internal_len: {}\r\n", ptr._internal_len)?;
            write!(f, "    _internal_layout_size: {}\r\n", ptr._internal_layout_size)?;
            write!(f, "    _internal_layout_align: {}\r\n", ptr._internal_layout_align)?;
            write!(f, "    type_name: \"{}\"\r\n", ptr.type_name.as_str())?;
            write!(f, "    type_id: {}\r\n", ptr.type_id)?;
            write!(f, "    custom_destructor: 0x{:x}\r\n", ptr.custom_destructor as usize)?;
            write!(f, "}}\r\n")?;
            Ok(())
        }
    }

    impl PartialEq for AzCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzLayoutCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzGlCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzIFrameCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzTimerCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzWriteBackCallback { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzThreadDestructorFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzLibraryReceiveThreadMsgFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzLibrarySendThreadMsgFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzCheckThreadFinishedFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzGetSystemTimeFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzCreateThreadFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzThreadRecvFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzThreadReceiverDestructorFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzThreadSenderDestructorFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzInstantPtrDestructorFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzInstantPtrCloneFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }
    impl PartialEq for AzThreadSendFn { fn eq(&self, rhs: &Self) -> bool { (self.cb as usize).eq(&(rhs.cb as usize)) } }

    impl PartialEq for AzRefCountInner {
        fn eq(&self, rhs: &Self) -> bool {
            (self.num_copies).eq(&rhs.num_copies) &&
            (self.num_refs).eq(&rhs.num_refs) &&
            (self.num_mutable_refs).eq(&(rhs.num_mutable_refs)) &&
            (self.type_id).eq(&rhs.type_id)
        }
    }

    impl PartialEq for AzRefCount {
        fn eq(&self, rhs: &Self) -> bool {
            let ptr = unsafe { &*self.ptr };
            let rhs = unsafe { &*rhs.ptr };
            ptr.eq(rhs)
        }
    }

    impl PartialOrd for AzCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzLayoutCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzGlCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzIFrameCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzTimerCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzWriteBackCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzThreadDestructorFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzLibraryReceiveThreadMsgFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzLibrarySendThreadMsgFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzCheckThreadFinishedFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzGetSystemTimeFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzCreateThreadFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzThreadRecvFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzThreadReceiverDestructorFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzThreadSenderDestructorFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzInstantPtrDestructorFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzInstantPtrCloneFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzThreadSendFn { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }

    impl PartialOrd for AzRefCountInner {
        fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> {
            (self.type_id).partial_cmp(&rhs.type_id)
        }
    }

    impl PartialOrd for AzRefCount {
        fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> {
            let ptr = unsafe { &*self.ptr };
            let rhs = unsafe { &*rhs.ptr };
            ptr.partial_cmp(rhs)
        }
    }

