/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/

//! Codes the predictor's corrections. The action list (runs of "the prediction was right" and
//! the corrections between them, written once the stream is done) is preflate-rs 0.7.6's
//! `cabac_codec.rs`. Changed for azlin-preflate: the bits go through [`crate::range_coder`]
//! instead of the `cabac` crate, and the arithmetic wraps instead of panicking on damaged input
//! (valid streams never wrap).

use crate::range_coder::{Prob, RangeDecoder, RangeEncoder};

use crate::{
    bit_helper::bit_length,
    statistical_codec::{
        CodecCorrection, CountNonDefaultActions, PredictionDecoder, PredictionEncoder,
    },
};

/// calculates the difference between two values, keeping track
/// of the sign as the lowest bit so that the difference is never negative
pub fn encode_difference(pred_val: u32, act_val: u32) -> u32 {
    if pred_val >= act_val {
        (pred_val - act_val) << 1
    } else {
        ((act_val - pred_val) << 1) | 1
    }
}

/// decodes the result of the previous calculation
pub fn decode_difference(pred_val: u32, encoded_val: u32) -> u32 {
    if encoded_val & 1 == 0 {
        pred_val.wrapping_sub(encoded_val >> 1)
    } else {
        pred_val.wrapping_add(encoded_val >> 1)
    }
}

#[test]
fn test_encode_decode_difference() {
    for i in 0..10 {
        assert_eq!(i, decode_difference(0, encode_difference(0, i)));
        assert_eq!(i, decode_difference(10, encode_difference(10, i)));
        assert_eq!(i, decode_difference(100, encode_difference(100, i)));
    }
}

const ACTIONS: usize = CodecCorrection::MAX as usize;

/// The adaptive probabilities, per kind of correction.
struct Contexts {
    default_signal: [Prob; ACTIONS],
    default: [[Prob; 32]; ACTIONS],
    default_bits: [[Prob; 32]; ACTIONS],
    correction: [[Prob; 32]; ACTIONS],
    correction_bits: [[Prob; 32]; ACTIONS],
}

impl Default for Contexts {
    fn default() -> Self {
        Contexts {
            default_signal: [Prob::default(); ACTIONS],
            default: [[Prob::default(); 32]; ACTIONS],
            default_bits: [[Prob::default(); 32]; ACTIONS],
            correction: [[Prob::default(); 32]; ACTIONS],
            correction_bits: [[Prob::default(); 32]; ACTIONS],
        }
    }
}

/// `value` as its bit length (unary), then its bits below the highest.
fn write_exp_encoded(
    value: u32,
    lengths: &mut [Prob; 32],
    bits: &mut [Prob; 32],
    writer: &mut RangeEncoder,
) {
    let bl = bit_length(value) as usize;
    writer.put_unary(bl, lengths);
    if bl > 1 {
        let mask = ((1u64 << (bl - 1)) - 1) as u32;
        writer.put_bits(value & mask, bl - 1, bits);
    }
}

fn read_exp_value(lengths: &mut [Prob; 32], bits: &mut [Prob; 32], reader: &mut RangeDecoder<'_>) -> u32 {
    match reader.get_unary(lengths) {
        0 => 0,
        1 => 1,
        found => reader.get_bits(found - 1, bits) | (1u32 << (found - 1)),
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Actions {
    Default(CodecCorrection, u32),
    Correction(CodecCorrection, u32),
}

pub struct PredictionEncoderRc {
    context: Contexts,

    actions: Vec<Actions>,

    last_action: [Option<usize>; ACTIONS],

    action_seen: [bool; ACTIONS],

    count: CountNonDefaultActions,
    writer: RangeEncoder,
    /// The coded corrections, once `finish` ran.
    bytes: Vec<u8>,
}

impl Default for PredictionEncoderRc {
    fn default() -> Self {
        Self::new()
    }
}

impl PredictionEncoderRc {
    pub fn new() -> Self {
        Self {
            context: Contexts::default(),
            writer: RangeEncoder::default(),
            actions: Default::default(),
            last_action: Default::default(),
            count: CountNonDefaultActions::default(),
            action_seen: Default::default(),
            bytes: Vec::new(),
        }
    }

    /// The coded corrections (empty before `finish`).
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// for debugging
    #[allow(dead_code)]
    pub fn print(&self) {
        self.count.print();
    }
}

impl PredictionEncoder for PredictionEncoderRc {
    fn encode_verify_state(&mut self, _message: &'static str, _checksum: u64) {}

    fn encode_correction(&mut self, c: CodecCorrection, value: u32) {
        let i = c as usize;

        // if it is a non-zero correction, then we just insert it
        if value != 0 {
            self.last_action[i] = Some(self.actions.len());
            self.actions.push(Actions::Correction(c, value));
            self.action_seen[i] = true;

            self.count.record_correction(c, value);
            return;
        }

        // otherwise we look to see if the last action was a default action
        // and if so, just increment it
        if let Some(l) = self.last_action[i] {
            let a = &mut self.actions[l];
            if let Actions::Default(_, x) = a {
                *x += 1;
                return;
            }
        }

        // otherwise we add a new default action
        self.last_action[c as usize] = Some(self.actions.len());
        self.actions.push(Actions::Default(c, 1));
    }

    fn finish(&mut self) {
        // Now that we've collected everything, we can write out the actions
        // in the correct order so that the callers can just read them in order
        //
        // This is necessary since the reads are done the first time we need
        // something, so we can't write them out in the same order that we get them.

        // first write the bit set of things we've seen so we can completely exclude
        let mut actions_seen_ctx = Prob::default();
        for &s in self.action_seen.iter() {
            self.writer.put(s, &mut actions_seen_ctx);
        }

        // now write all the actions
        for &a in self.actions.iter() {
            match a {
                Actions::Default(c, value) => {
                    let i = c as usize;

                    if self.action_seen[i] {
                        self.writer.put(true, &mut self.context.default_signal[i]);

                        write_exp_encoded(
                            value - 1,
                            &mut self.context.default[i],
                            &mut self.context.default_bits[i],
                            &mut self.writer,
                        );
                    }
                }
                Actions::Correction(c, value) => {
                    let i = c as usize;
                    self.writer.put(false, &mut self.context.default_signal[i]);
                    write_exp_encoded(
                        value - 1,
                        &mut self.context.correction[i],
                        &mut self.context.correction_bits[i],
                        &mut self.writer,
                    );
                }
            }
        }

        self.bytes = std::mem::take(&mut self.writer).finish();
    }
}

pub struct PredictionDecoderRc<'a> {
    context: Contexts,
    reader: RangeDecoder<'a>,

    actions_seen: [bool; ACTIONS],
    default_actions: [u32; ACTIONS],
}

impl<'a> PredictionDecoderRc<'a> {
    pub fn new(corrections: &'a [u8]) -> Self {
        let mut reader = RangeDecoder::new(corrections);
        let mut actions_seen = [false; ACTIONS];
        let mut actions_seen_ctx = Prob::default();
        for seen in actions_seen.iter_mut() {
            *seen = reader.get(&mut actions_seen_ctx);
        }

        Self {
            context: Contexts::default(),
            default_actions: Default::default(),
            actions_seen,
            reader,
        }
    }

    #[cold]
    fn decode_correction_slow(&mut self, c: usize) -> u32 {
        if self.reader.get(&mut self.context.default_signal[c]) {
            let value = read_exp_value(
                &mut self.context.default[c],
                &mut self.context.default_bits[c],
                &mut self.reader,
            );

            self.default_actions[c] = value;
            0
        } else {
            read_exp_value(
                &mut self.context.correction[c],
                &mut self.context.correction_bits[c],
                &mut self.reader,
            )
            .wrapping_add(1)
        }
    }
}

impl PredictionDecoder for PredictionDecoderRc<'_> {
    fn decode_verify_state(&mut self, _message: &'static str, _checksum: u64) {}

    fn overran(&self) -> bool {
        self.reader.overran()
    }

    #[inline]
    fn decode_correction(&mut self, correction: CodecCorrection) -> u32 {
        // if the action hasn't been seen at all, then always return 0
        let c = correction as usize;
        if !self.actions_seen[c] {
            return 0;
        }

        // otherwise if we still have default actions left, use those
        if self.default_actions[c] > 0 {
            self.default_actions[c] -= 1;
            return 0;
        }

        self.decode_correction_slow(c)
    }
}

#[test]
fn corrections_round_trip() {
    use crate::statistical_codec::{CodecAction, drive_encoder, verify_decoder};

    let actions = [
        CodecAction::Correction(CodecCorrection::DistanceCountCorrection, 5),
        CodecAction::Correction(CodecCorrection::BlockTypeCorrection, 1),
        CodecAction::Correction(CodecCorrection::BlockTypeCorrection, 2),
        CodecAction::Correction(CodecCorrection::BlockTypeCorrection, 0),
        CodecAction::Correction(CodecCorrection::BlockTypeCorrection, 0),
        CodecAction::Correction(CodecCorrection::BlockTypeCorrection, 4),
        CodecAction::Correction(CodecCorrection::DistAfterLenCorrection, 0),
        CodecAction::Correction(CodecCorrection::DistOnlyCorrection, 0),
        CodecAction::Correction(CodecCorrection::LDTypeCorrection, 7),
        CodecAction::Correction(CodecCorrection::LenCorrection, 9),
        CodecAction::Correction(CodecCorrection::LiteralPredictionWrong, 0),
        CodecAction::Correction(CodecCorrection::TokenCount, 100000),
        CodecAction::Correction(CodecCorrection::TokenCount, u32::MAX),
        CodecAction::Correction(CodecCorrection::ReferencePredictionWrong, 1),
    ];
    let mut encoder = PredictionEncoderRc::new();
    drive_encoder(&mut encoder, &actions);
    encoder.finish();
    let bytes = encoder.into_bytes();
    let mut decoder = PredictionDecoderRc::new(&bytes);
    verify_decoder(&mut decoder, &actions);
    assert!(!decoder.overran());
}
