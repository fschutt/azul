// Stub proc-macro for malloc_size_of derive - does nothing
extern crate proc_macro;

use proc_macro::TokenStream;

#[proc_macro_derive(MallocSizeOf)]
pub fn derive_malloc_size_of(_input: TokenStream) -> TokenStream {
    // Empty implementation - just returns nothing
    TokenStream::new()
}

#[proc_macro_derive(MallocShallowSizeOf)]
pub fn derive_malloc_shallow_size_of(_input: TokenStream) -> TokenStream {
    // Empty implementation - just returns nothing
    TokenStream::new()
}
