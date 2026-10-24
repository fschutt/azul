/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/

use crate::estimator::add_policy_estimator::DictionaryAddPolicy;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum MatchingType {
    #[default]
    Greedy,
    Lazy {
        good_length: u16,
        max_lazy: u16,
    },
}

pub struct PreflateParserConfig {
    pub match_type: MatchingType,

    // used for testing
    #[allow(dead_code)]
    pub dictionary_add_policy: DictionaryAddPolicy,

    /// if we get this length of a match we immediately stop searching for more
    pub nice_length: u32,
    pub max_chain: u32,
}

pub const ZLIB_PREFLATE_PARSER_SETTINGS: [PreflateParserConfig; 3] = [
    // these three levels are used by zlib

    // max speed, no lazy matches (the lazy field means
    // the maximum length that is added to the dictionary during
    // a match)
    PreflateParserConfig {
        match_type: MatchingType::Greedy,
        dictionary_add_policy: DictionaryAddPolicy::AddFirst(4),
        nice_length: 8,
        max_chain: 4,
    },
    PreflateParserConfig {
        match_type: MatchingType::Greedy,
        dictionary_add_policy: DictionaryAddPolicy::AddFirst(5),
        nice_length: 16,
        max_chain: 8,
    },
    PreflateParserConfig {
        match_type: MatchingType::Greedy,
        dictionary_add_policy: DictionaryAddPolicy::AddFirst(6),
        nice_length: 32,
        max_chain: 32,
    },
];

pub const SLOW_PREFLATE_PARSER_SETTINGS: [PreflateParserConfig; 6] = [
    // 4
    PreflateParserConfig {
        match_type: MatchingType::Lazy {
            good_length: 4,
            max_lazy: 4,
        },
        dictionary_add_policy: DictionaryAddPolicy::AddAll,
        nice_length: 16,
        max_chain: 16,
    },
    // 5
    PreflateParserConfig {
        match_type: MatchingType::Lazy {
            good_length: 8,
            max_lazy: 16,
        },
        dictionary_add_policy: DictionaryAddPolicy::AddAll,
        nice_length: 32,
        max_chain: 32,
    },
    // 6
    PreflateParserConfig {
        match_type: MatchingType::Lazy {
            good_length: 8,
            max_lazy: 16,
        },
        dictionary_add_policy: DictionaryAddPolicy::AddAll,
        nice_length: 128,
        max_chain: 128,
    },
    // 7
    PreflateParserConfig {
        match_type: MatchingType::Lazy {
            good_length: 8,
            max_lazy: 32,
        },
        dictionary_add_policy: DictionaryAddPolicy::AddAll,
        nice_length: 128,
        max_chain: 256,
    },
    // 8
    PreflateParserConfig {
        match_type: MatchingType::Lazy {
            good_length: 32,
            max_lazy: 128,
        },
        dictionary_add_policy: DictionaryAddPolicy::AddAll,
        nice_length: 258,
        max_chain: 1024,
    },
    // 9
    PreflateParserConfig {
        match_type: MatchingType::Lazy {
            good_length: 32,
            max_lazy: 258,
        },
        dictionary_add_policy: DictionaryAddPolicy::AddAll,
        nice_length: 258,
        max_chain: 4096,
    }, // max compression
];
