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
    impl ::core::fmt::Debug for AzRefAny                     {
        fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            write!(f, "RefAny {{\r\n")?;
            write!(f, "    is_dead: {:?}\r\n", &self.is_dead)?;
            write!(f, "    sharing_info: {:?}\r\n", &self.sharing_info)?;
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
    impl PartialEq for AzRefAny { fn eq(&self, rhs: &Self) -> bool { (self._internal_ptr as usize).eq(&(rhs._internal_ptr as usize)) } }

    impl PartialOrd for AzCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzLayoutCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzGlCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzIFrameCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzTimerCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzWriteBackCallback { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self.cb as usize).partial_cmp(&(rhs.cb as usize)) } }
    impl PartialOrd for AzRefAny { fn partial_cmp(&self, rhs: &Self) -> Option<::core::cmp::Ordering> { (self._internal_ptr as usize).partial_cmp(&(rhs._internal_ptr as usize)) } }

    impl ::core::fmt::Debug for AzRefCount {
        fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            let ptr = unsafe { &*self.ptr };
            write!(f, "RefAnyRefCount {{\r\n")?;
            write!(f, "    num_copies: {}\r\n", ptr.num_copies)?;
            write!(f, "    num_refs: {}\r\n", ptr.num_refs)?;
            write!(f, "    num_mutable_refs: {}\r\n", ptr.num_mutable_refs)?;
            write!(f, "    _internal_ptr: 0x{:x}\r\n", ptr._internal_ptr as usize)?;
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

