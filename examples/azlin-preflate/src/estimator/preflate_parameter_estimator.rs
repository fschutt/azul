/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/


use crate::{
    bit_helper::bit_length,
    deflate::{deflate_constants, deflate_reader::DeflateContents},
    estimator::{add_policy_estimator::DictionaryAddPolicy, preflate_parse_config::MatchingType},
    hash_algorithm::HashAlgorithm,
    preflate_error::Result,
    preflate_input::PlainText,
};

use super::{
    add_policy_estimator::estimate_add_policy,
    complevel_estimator::estimate_preflate_comp_level,
    preflate_stream_info::{PreflateStreamInfo, extract_preflate_info},
};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct TokenPredictorParameters {
    /// Zlib does not match to first byte of a file in order to reserve 0 for the end of chain
    pub matches_to_start_detected: bool,

    /// if there are matches that have a distance larger than window_size - MAX_MATCH.
    /// Zlib does not allow these.
    pub very_far_matches_detected: bool,
    pub window_bits: u32,

    pub strategy: PreflateStrategy,
    pub nice_length: u32,

    /// if something, then we use the "fast" compressor, which only adds smaller substrings
    /// to the dictionary
    pub add_policy: DictionaryAddPolicy,

    pub max_token_count: u16,

    pub zlib_compatible: bool,
    pub max_dist_3_matches: u32,
    pub matching_type: MatchingType,
    pub max_chain: u32,
    pub min_len: u32,

    pub hash_algorithm: HashAlgorithm,

    pub block_type_strategy: BlockTypeStrategy,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum BlockTypeStrategy {
    Dynamic,
    Mixed,
    Static,
    Uncompressed,
}

/// From the plain text and the preflate blocks, estimate the preflate parameters
pub fn estimate_preflate_parameters(
    deflate_contents: &DeflateContents,
    plain_text: &PlainText,
) -> Result<TokenPredictorParameters> {
    let info = extract_preflate_info(&deflate_contents.blocks);

    let preflate_strategy = estimate_preflate_strategy(&info);
    let huff_strategy = estimate_preflate_huff_strategy(&info);

    if preflate_strategy == PreflateStrategy::Store
        || preflate_strategy == PreflateStrategy::HuffOnly
    {
        // No dictionary used
        return Ok(TokenPredictorParameters {
            window_bits: 0,
            very_far_matches_detected: false,
            matches_to_start_detected: false,
            strategy: preflate_strategy,
            nice_length: 0,
            add_policy: DictionaryAddPolicy::AddAll,
            max_token_count: 16386,
            zlib_compatible: true,
            max_dist_3_matches: 0,
            matching_type: MatchingType::Greedy,
            max_chain: 0,
            min_len: 0,
            hash_algorithm: HashAlgorithm::None,
            block_type_strategy: huff_strategy,
        });
    }

    let window_bits = estimate_preflate_window_bits(info.max_dist);
    let mem_level = estimate_preflate_mem_level(info.max_tokens_per_block);
    let add_policy = estimate_add_policy(&deflate_contents.blocks);

    //let hash_shift = 5;
    //let hash_mask = 32767;

    let max_token_count = (1 << (6 + mem_level)) - 1;

    let cl = estimate_preflate_comp_level(
        window_bits,
        mem_level,
        info.min_len,
        deflate_contents,
        plain_text,
        add_policy,
    )?;

    let zlib_compatible = !info.matches_to_start_detected
        && !cl.very_far_matches_detected
        && (info.max_dist_3_matches < 4096 || add_policy != DictionaryAddPolicy::AddAll);

    Ok(TokenPredictorParameters {
        window_bits,
        very_far_matches_detected: cl.very_far_matches_detected,
        matches_to_start_detected: info.matches_to_start_detected,
        strategy: estimate_preflate_strategy(&info),
        nice_length: cl.nice_length,
        add_policy: add_policy,
        max_token_count,
        zlib_compatible,
        max_dist_3_matches: info.max_dist_3_matches,
        matching_type: cl.match_type,
        max_chain: cl.max_chain,
        min_len: info.min_len,
        hash_algorithm: cl.hash_algorithm,
        block_type_strategy: estimate_preflate_huff_strategy(&info),
    })
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum PreflateStrategy {
    Default,
    RleOnly,
    HuffOnly,
    Store,
}

fn estimate_preflate_mem_level(max_block_size_: u32) -> u32 {
    let mut max_block_size = max_block_size_;
    let mut mbits = 0;
    while max_block_size > 0 {
        mbits += 1;
        max_block_size >>= 1;
    }
    mbits = std::cmp::min(std::cmp::max(mbits, 7), 15);
    mbits - 6
}

fn estimate_preflate_window_bits(max_dist_: u32) -> u32 {
    let mut max_dist = max_dist_;
    max_dist += deflate_constants::MIN_LOOKAHEAD;
    let wbits = bit_length(max_dist - 1);
    std::cmp::min(std::cmp::max(wbits, 9), 15)
}

fn estimate_preflate_strategy(info: &PreflateStreamInfo) -> PreflateStrategy {
    if info.count_stored_blocks == info.count_blocks {
        return PreflateStrategy::Store;
    }
    if info.count_huff_blocks == info.count_blocks {
        return PreflateStrategy::HuffOnly;
    }
    if info.count_rle_blocks == info.count_blocks {
        return PreflateStrategy::RleOnly;
    }
    PreflateStrategy::Default
}

fn estimate_preflate_huff_strategy(info: &PreflateStreamInfo) -> BlockTypeStrategy {
    if info.count_static_huff_tree_blocks == info.count_blocks {
        return BlockTypeStrategy::Static;
    }
    if info.count_stored_blocks == info.count_blocks {
        return BlockTypeStrategy::Uncompressed;
    }
    if info.count_static_huff_tree_blocks == 0 {
        return BlockTypeStrategy::Dynamic;
    }
    BlockTypeStrategy::Mixed
}

/// The parameters as bytes (azlin-preflate's layout, in place of bitcode): fixed
/// [`TokenPredictorParameters::BYTES`] bytes, integers little endian, every enum as a tag
/// followed by its fields (zero when it has none). Reading checks every tag and every range
/// the predictor relies on, so damaged bytes are an error, never a panic further on.
impl TokenPredictorParameters {
    pub const BYTES: usize = 48;

    pub fn to_bytes(&self) -> [u8; Self::BYTES] {
        let mut out = [0u8; Self::BYTES];
        out[0] = 1; // layout version
        out[1] = u8::from(self.matches_to_start_detected)
            | u8::from(self.very_far_matches_detected) << 1
            | u8::from(self.zlib_compatible) << 2;
        out[2] = self.strategy as u8;
        out[3] = self.block_type_strategy as u8;
        out[4..8].copy_from_slice(&self.window_bits.to_le_bytes());
        out[8..12].copy_from_slice(&self.nice_length.to_le_bytes());
        out[12..14].copy_from_slice(&self.max_token_count.to_le_bytes());
        out[14..18].copy_from_slice(&self.max_dist_3_matches.to_le_bytes());
        out[18..22].copy_from_slice(&self.max_chain.to_le_bytes());
        out[22..26].copy_from_slice(&self.min_len.to_le_bytes());
        let (policy, policy_value) = match self.add_policy {
            DictionaryAddPolicy::AddAll => (0u8, 0u16),
            DictionaryAddPolicy::AddFirst(v) => (1, v),
            DictionaryAddPolicy::AddFirstAndLast(v) => (2, v),
            DictionaryAddPolicy::AddFirstExcept4kBoundary => (3, 0),
            DictionaryAddPolicy::AddFirstWith32KBoundary => (4, 0),
        };
        out[26] = policy;
        out[27..29].copy_from_slice(&policy_value.to_le_bytes());
        let (matching, good, lazy) = match self.matching_type {
            MatchingType::Greedy => (0u8, 0u16, 0u16),
            MatchingType::Lazy {
                good_length,
                max_lazy,
            } => (1, good_length, max_lazy),
        };
        out[29] = matching;
        out[30..32].copy_from_slice(&good.to_le_bytes());
        out[32..34].copy_from_slice(&lazy.to_le_bytes());
        let (hash, mask, shift) = match self.hash_algorithm {
            HashAlgorithm::None => (0u8, 0u16, 0u32),
            HashAlgorithm::Zlib {
                hash_mask,
                hash_shift,
            } => (1, hash_mask, hash_shift),
            HashAlgorithm::MiniZFast => (2, 0, 0),
            HashAlgorithm::Libdeflate4Fast => (3, 0, 0),
            HashAlgorithm::Libdeflate4 => (4, 0, 0),
            HashAlgorithm::ZlibNG => (5, 0, 0),
            HashAlgorithm::RandomVector => (6, 0, 0),
            HashAlgorithm::Crc32cHash => (7, 0, 0),
        };
        out[34] = hash;
        out[35..37].copy_from_slice(&mask.to_le_bytes());
        out[37..41].copy_from_slice(&shift.to_le_bytes());
        // 41..48 zero
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let bad = |what: &str| {
            Err(crate::preflate_error::PreflateError::new(
                crate::preflate_error::ExitCode::InvalidParameterHeader,
                what,
            ))
        };
        if bytes.len() != Self::BYTES || bytes[0] != 1 || bytes[41..].iter().any(|&b| b != 0) {
            return bad("parameter bytes of another layout");
        }
        let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let u32_at = |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
        if bytes[1] & !0b111 != 0 {
            return bad("unknown parameter flags");
        }
        let strategy = match bytes[2] {
            0 => PreflateStrategy::Default,
            1 => PreflateStrategy::RleOnly,
            2 => PreflateStrategy::HuffOnly,
            3 => PreflateStrategy::Store,
            _ => return bad("unknown strategy"),
        };
        let block_type_strategy = match bytes[3] {
            0 => BlockTypeStrategy::Dynamic,
            1 => BlockTypeStrategy::Mixed,
            2 => BlockTypeStrategy::Static,
            3 => BlockTypeStrategy::Uncompressed,
            _ => return bad("unknown block type strategy"),
        };
        let policy_value = u16_at(27);
        let add_policy = match bytes[26] {
            0 => DictionaryAddPolicy::AddAll,
            1 => DictionaryAddPolicy::AddFirst(policy_value),
            2 => DictionaryAddPolicy::AddFirstAndLast(policy_value),
            3 => DictionaryAddPolicy::AddFirstExcept4kBoundary,
            4 => DictionaryAddPolicy::AddFirstWith32KBoundary,
            _ => return bad("unknown dictionary add policy"),
        };
        let matching_type = match bytes[29] {
            0 => MatchingType::Greedy,
            1 => MatchingType::Lazy {
                good_length: u16_at(30),
                max_lazy: u16_at(32),
            },
            _ => return bad("unknown matching type"),
        };
        let hash_shift = u32_at(37);
        let hash_algorithm = match bytes[34] {
            0 => HashAlgorithm::None,
            1 => HashAlgorithm::Zlib {
                hash_mask: u16_at(35),
                hash_shift,
            },
            2 => HashAlgorithm::MiniZFast,
            3 => HashAlgorithm::Libdeflate4Fast,
            4 => HashAlgorithm::Libdeflate4,
            5 => HashAlgorithm::ZlibNG,
            6 => HashAlgorithm::RandomVector,
            7 => HashAlgorithm::Crc32cHash,
            _ => return bad("unknown hash algorithm"),
        };
        let params = TokenPredictorParameters {
            matches_to_start_detected: bytes[1] & 1 != 0,
            very_far_matches_detected: bytes[1] & 2 != 0,
            zlib_compatible: bytes[1] & 4 != 0,
            window_bits: u32_at(4),
            strategy,
            nice_length: u32_at(8),
            add_policy,
            max_token_count: u16_at(12),
            max_dist_3_matches: u32_at(14),
            matching_type,
            max_chain: u32_at(18),
            min_len: u32_at(22),
            hash_algorithm,
            block_type_strategy,
        };
        // The ranges the encoder produces (and the predictor's tables assume).
        if params.window_bits > 15
            || params.nice_length > 258
            || params.min_len > 258
            || params.max_chain > 1 << 16
            || hash_shift > 31
        {
            return bad("a parameter out of range");
        }
        Ok(params)
    }
}

#[test]
fn parameter_bytes_round_trip_and_refuse_garbage() {
    let params = TokenPredictorParameters {
        matches_to_start_detected: true,
        very_far_matches_detected: false,
        zlib_compatible: true,
        window_bits: 15,
        strategy: PreflateStrategy::Default,
        nice_length: 258,
        add_policy: DictionaryAddPolicy::AddFirst(4),
        max_token_count: 16383,
        max_dist_3_matches: 4096,
        matching_type: MatchingType::Lazy {
            good_length: 8,
            max_lazy: 16,
        },
        max_chain: 4096,
        min_len: 3,
        hash_algorithm: HashAlgorithm::Zlib {
            hash_mask: 0x7fff,
            hash_shift: 5,
        },
        block_type_strategy: BlockTypeStrategy::Dynamic,
    };
    let bytes = params.to_bytes();
    assert_eq!(TokenPredictorParameters::from_bytes(&bytes).unwrap(), params);
    for at in 0..bytes.len() {
        let mut damaged = bytes;
        damaged[at] ^= 0xA5;
        let _ = TokenPredictorParameters::from_bytes(&damaged);
    }
    assert!(TokenPredictorParameters::from_bytes(&bytes[1..]).is_err());
}
