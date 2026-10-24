/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/

/// This module is design to detect the appropriate overall parameters for the preflate compressor.
/// Getting the parameters correct means that the resulting diff between the deflate stream
/// and the predicted deflate stream will be as small as possible.
use crate::{
    deflate::deflate_reader::DeflateContents,
    hash_algorithm::HashAlgorithm,
    preflate_error::{ExitCode, Result, err_exit_code},
    preflate_input::PlainText,
};

use super::{
    add_policy_estimator::DictionaryAddPolicy,
    depth_estimator::{HashTableDepthEstimator, new_depth_estimator, run_depth_candidates},
    preflate_parse_config::{
        MatchingType, SLOW_PREFLATE_PARSER_SETTINGS, ZLIB_PREFLATE_PARSER_SETTINGS,
    },
};

#[derive(Default)]
pub struct CompLevelInfo {
    pub very_far_matches_detected: bool,
    pub hash_algorithm: HashAlgorithm,
    pub match_type: MatchingType,
    pub nice_length: u32,
    pub max_chain: u32,
}

pub fn estimate_preflate_comp_level(
    wbits: u32,
    mem_level: u32,
    min_len: u32,
    deflate_contents: &DeflateContents,
    plain_text: &PlainText,
    add_policy: DictionaryAddPolicy,
) -> Result<CompLevelInfo> {
    let hash_bits = mem_level + 7;
    let mem_hash_shift = (hash_bits + 2) / 3;
    let mem_hash_mask = ((1u32 << hash_bits) - 1) as u16;
    let wsize = 1 << wbits;

    let mut candidates: Vec<Box<dyn HashTableDepthEstimator>> = Vec::new();

    if min_len == 3 {
        candidates.push(new_depth_estimator(HashAlgorithm::MiniZFast));

        for (hash_shift, hash_mask) in [(5, 32767), (4, 2047), (mem_hash_shift, mem_hash_mask)] {
            candidates.push(new_depth_estimator(HashAlgorithm::Zlib {
                hash_mask,
                hash_shift,
            }));
        }

        // LibFlate4 candidate
        candidates.push(new_depth_estimator(HashAlgorithm::Libdeflate4));

        // RandomVector candidate
        candidates.push(new_depth_estimator(HashAlgorithm::RandomVector));
    } else {
        // Libflate4 fast (only 4 bytes or more)
        candidates.push(new_depth_estimator(HashAlgorithm::Libdeflate4Fast));

        // ZlibNG candidate
        candidates.push(new_depth_estimator(HashAlgorithm::ZlibNG));

        // Crc32c candidate
        candidates.push(new_depth_estimator(HashAlgorithm::Crc32cHash));
    }

    run_depth_candidates(add_policy, deflate_contents, plain_text, &mut candidates);

    if candidates.is_empty() {
        return err_exit_code(ExitCode::NoCompressionCandidates, "no candidates found");
    }

    let candidate = candidates
        .iter()
        .min_by(|&a, &b| a.max_chain_found().cmp(&b.max_chain_found()))
        .unwrap();

    if candidate.max_chain_found() > 4096 {
        return err_exit_code(
            ExitCode::NoCompressionCandidates,
            "no candidate found with reasonable chain length",
        );
    }

    let mut match_type = MatchingType::Greedy;
    let mut nice_length = 258;

    let max_chain = candidate.max_chain_found() + 1;

    match add_policy {
        DictionaryAddPolicy::AddFirst(_)
        | DictionaryAddPolicy::AddFirstAndLast(_)
        | DictionaryAddPolicy::AddFirstWith32KBoundary
        | DictionaryAddPolicy::AddFirstExcept4kBoundary => {
            for config in &ZLIB_PREFLATE_PARSER_SETTINGS {
                if candidate.max_chain_found() < config.max_chain {
                    match_type = config.match_type;
                    nice_length = config.nice_length;
                    break;
                }
            }
        }
        DictionaryAddPolicy::AddAll => {
            for config in &SLOW_PREFLATE_PARSER_SETTINGS {
                if candidate.max_chain_found() < config.max_chain {
                    match_type = config.match_type;
                    nice_length = config.nice_length;
                    break;
                }
            }
        }
    }

    if candidate.max_chain_found() >= 4096 {
        return err_exit_code(
            ExitCode::NoCompressionCandidates,
            format!("max_chain_found too large: {}", candidate.max_chain_found()),
        );
    }

    Ok(CompLevelInfo {
        very_far_matches_detected: candidate.very_far_matches_detected(wsize),
        match_type,
        nice_length,
        max_chain,
        hash_algorithm: candidate.hash_algorithm(),
    })
}
