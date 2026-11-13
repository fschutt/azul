/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/

//! Deflate reconstruction: splits a deflate stream into its plain text and the few bytes of
//! corrections that rebuild the exact same stream, so the plain text can be compressed with a
//! better codec and the original bytes still come back bit for bit.
//!
//! This is preflate-rs 0.7.6 (Microsoft, Apache-2.0, crates.io), vendored by AzDrive with two
//! changes, both so that no LGPL code is linked and the stored format is ours to keep:
//! - the corrections are coded with an LZMA-style range coder (`range_coder`, adapted from
//!   lzma-rs, MIT) instead of the `cabac` crate (LGPL-3.0-or-later);
//! - the predictor's parameters are written as explicit bytes instead of with `bitcode`.
//!
//! Its output is therefore NOT compatible with upstream preflate-rs. Upstream's tests that need
//! its sample files are left out; azlin-compress tests the round trips.

// forbid lints that we already have eliminated from the codebase so they don't show up in the future
#![forbid(unsafe_code)]
#![forbid(trivial_casts)]
#![forbid(trivial_numeric_casts)]
#![forbid(non_ascii_idents)]
#![forbid(redundant_lifetimes)]
#![forbid(single_use_lifetimes)]
#![forbid(macro_use_extern_crate)]
#![forbid(missing_unsafe_on_extern)]
#![forbid(unused_import_braces)]
#![warn(unused_crate_dependencies)]
#![warn(unused_extern_crates)]
#![warn(unused_lifetimes)]
#![warn(unused_macro_rules)]
// Vendored code, in upstream's style (preflate-rs 0.7.6): the clippy lints below find style,
// complexity and cleanliness in the ORIGINAL, and the crate stays close to it so a newer release
// merges. None is a correctness or safety finding - those stay denied (the panic lints just below,
// and clippy's correctness group). Listed one by one, not as a group, so a NEW kind of finding in
// code written here shows up.
#![allow(
    clippy::assertions_on_constants,
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::derivable_impls,
    clippy::empty_line_after_doc_comments,
    clippy::identity_op,
    clippy::int_plus_one,
    clippy::io_other_error,
    clippy::len_without_is_empty,
    clippy::let_and_return,
    clippy::manual_clamp,
    clippy::manual_div_ceil,
    clippy::manual_is_multiple_of,
    clippy::manual_memcpy,
    clippy::match_ref_pats,
    clippy::needless_borrow,
    clippy::needless_late_init,
    clippy::needless_range_loop,
    clippy::needless_return,
    clippy::new_without_default,
    clippy::ptr_arg,
    clippy::redundant_field_names,
    clippy::same_item_push,
    clippy::single_match,
    clippy::too_many_arguments,
    clippy::unneeded_struct_pattern,
    clippy::unused_enumerate_index,
    clippy::useless_conversion,
    clippy::while_let_on_iterator
)]
// panic = "abort" in every shipped profile: a panic reachable from a hostile deflate stream ends
// AzDrive. The analysis path has none that the compiler can see: an unwrap, an expect, a panic,
// a todo or an unreachable is a build error outside the tests (ARCH28 RF50 / AR5b-5). Plain
// asserts and indexing stay - they are the fuzz targets' to find (`fuzz/`).
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented
    )
)]

mod bit_helper;
mod correction_codec;
mod range_coder;
mod deflate;
mod estimator;
mod hash_algorithm;
mod hash_chain;
mod hash_chain_holder;

mod preflate_error;
mod preflate_input;

mod statistical_codec;
mod stream_processor;
mod token_predictor;
mod tree_predictor;
mod utils;

pub use estimator::preflate_parameter_estimator::TokenPredictorParameters;
pub use hash_algorithm::HashAlgorithm;
pub use preflate_error::ExitCode;
pub use preflate_error::{AddContext, PreflateError, Result, err_exit_code};
pub use preflate_input::{PlainText, PreflateInput};

pub use statistical_codec::{CodecCorrection, CountNonDefaultActions};
pub use stream_processor::{
    PreflateStreamChunkResult, PreflateStreamProcessor, RecreateStreamProcessor,
    preflate_whole_deflate_stream, recreate_whole_deflate_stream,
};

/// Configure the preflate stream processor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreflateConfig {
    /// Maximum number of lookups we will do in the hash chain. This will limit the
    /// amount of CPU time we spend on each chunk.
    pub max_chain_length: u32,

    /// The maximum size of the plain text that will be decompressed to memory before
    /// being recompressed. This limits memory usage and processing time, especially
    /// for files that are extremely compressed (eg zip bombs)
    pub plain_text_limit: usize,

    /// Decompress the data and verify that it can be recompressed to exactly the same bytes.
    /// This will slow down processing is isn't necessary if you are already doing end-to-end
    /// verification of the data.
    pub verify_compression: bool,
}

impl Default for PreflateConfig {
    fn default() -> Self {
        Self {
            max_chain_length: 4096,
            plain_text_limit: 128 * 1024 * 1024,
            verify_compression: true,
        }
    }
}
