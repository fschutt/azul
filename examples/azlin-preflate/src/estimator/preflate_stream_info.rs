/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/

use crate::deflate::deflate_token::{
    DeflateHuffmanType, DeflateToken, DeflateTokenBlock, DeflateTokenBlockType,
};

pub struct PreflateStreamInfo {
    pub token_count: u32,
    pub literal_count: u32,
    pub reference_count: u32,
    pub max_dist: u32,
    pub min_len: u32,
    pub max_tokens_per_block: u32,
    pub count_blocks: u32,
    pub count_stored_blocks: u32,
    pub count_huff_blocks: u32,
    pub count_rle_blocks: u32,
    pub count_static_huff_tree_blocks: u32,
    pub max_dist_3_matches: u32,
    pub matches_to_start_detected: bool,
}

fn process_tokens(tokens: &[DeflateToken], result: &mut PreflateStreamInfo, position: &mut u32) {
    result.token_count += tokens.len() as u32;
    result.max_tokens_per_block = std::cmp::max(result.max_tokens_per_block, tokens.len() as u32);
    let mut block_max_dist = 0;
    let mut block_min_len = u32::MAX;

    for j in 0..tokens.len() {
        match &tokens[j] {
            DeflateToken::Literal(_) => {
                result.literal_count += 1;
                *position += 1;
            }
            DeflateToken::Reference(t) => {
                if t.dist() == *position {
                    result.matches_to_start_detected = true;
                }

                result.reference_count += 1;
                block_max_dist = std::cmp::max(block_max_dist, t.dist());
                block_min_len = std::cmp::min(block_min_len, t.len());
                *position += t.len();

                if t.len() == 3 {
                    result.max_dist_3_matches = std::cmp::max(result.max_dist_3_matches, t.dist());
                }
            }
        }
    }
    result.max_dist = std::cmp::max(result.max_dist, block_max_dist);
    result.min_len = std::cmp::min(result.min_len, block_min_len);

    if block_max_dist == 0 {
        result.count_huff_blocks += 1;
    } else if block_max_dist == 1 {
        result.count_rle_blocks += 1;
    }
}

pub(crate) fn extract_preflate_info(blocks: &[DeflateTokenBlock]) -> PreflateStreamInfo {
    let mut result: PreflateStreamInfo = PreflateStreamInfo {
        count_blocks: blocks.len() as u32,
        count_stored_blocks: 0,
        count_static_huff_tree_blocks: 0,
        token_count: 0,
        max_tokens_per_block: 0,
        literal_count: 0,
        reference_count: 0,
        min_len: u32::MAX,
        max_dist: 0,
        count_huff_blocks: 0,
        count_rle_blocks: 0,
        max_dist_3_matches: 0,
        matches_to_start_detected: false,
    };

    let mut position = 0;
    for i in 0..blocks.len() {
        match &blocks[i].block_type {
            DeflateTokenBlockType::Stored { uncompressed, .. } => {
                result.count_stored_blocks += 1;
                position += uncompressed.len() as u32;
            }
            DeflateTokenBlockType::Huffman {
                tokens,
                huffman_type,
                ..
            } => {
                if let DeflateHuffmanType::Static { .. } = huffman_type {
                    result.count_static_huff_tree_blocks += 1;
                }
                process_tokens(&tokens, &mut result, &mut position);
            }
        }
    }
    result
}
