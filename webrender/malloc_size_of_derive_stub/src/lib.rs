// Stub derive macro for MallocSizeOf - implements empty trait since we don't need memory profiling
extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(MallocSizeOf)]
pub fn malloc_size_of_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    
    // Generate empty implementation of MallocSizeOf trait
    // The trait is re-exported at crate root via pub(crate) use
    let expanded = quote! {
        impl crate::MallocSizeOf for #name {}
    };
    
    TokenStream::from(expanded)
}
