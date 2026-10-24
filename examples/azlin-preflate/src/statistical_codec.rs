/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/

use crate::correction_codec::{decode_difference, encode_difference};

/// correction indictions, which are followed by a 16 bit value
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CodecCorrection {
    TokenCount,
    BlockTypeCorrection,
    LenCorrection,
    DistOnlyCorrection,
    DistAfterLenCorrection,
    TreeCodeBitLengthCorrection,
    LDTypeCorrection,
    RepeatCountCorrection,
    LDBitLengthCorrection,

    TreeCodeCountCorrection,
    LiteralCountCorrection,
    DistanceCountCorrection,
    UncompressBlockLenCorrection,

    Last,
    EndOfChunk,
    LiteralPredictionWrong,
    ReferencePredictionWrong,
    MAX,
}

pub trait PredictionEncoder {
    fn encode_correction(&mut self, action: CodecCorrection, value: u32);

    fn encode_verify_state(&mut self, message: &'static str, checksum: u64);

    fn finish(&mut self);

    fn encode_correction_diff(
        &mut self,
        action: CodecCorrection,
        actual_value: u32,
        predicted_value: u32,
    ) {
        self.encode_correction(action, encode_difference(predicted_value, actual_value));
    }

    fn encode_misprediction(&mut self, action: CodecCorrection, actual_value: bool) {
        self.encode_correction(action, actual_value as u32);
    }

    /// if a bool is not equal, then we encode a 1, otherwise a 0
    fn encode_correction_bool(
        &mut self,
        action: CodecCorrection,
        actual_value: bool,
        predicted_value: bool,
    ) {
        self.encode_correction(action, (actual_value != predicted_value) as u32)
    }
}

pub trait PredictionDecoder {
    fn decode_correction(&mut self, correction: CodecCorrection) -> u32;
    fn decode_verify_state(&mut self, message: &'static str, checksum: u64);

    /// azlin-preflate: whether the decoder read past its coded bytes (they are damaged).
    fn overran(&self) -> bool {
        false
    }

    #[inline]
    fn decode_correction_diff(&mut self, correction: CodecCorrection, predicted_value: u32) -> u32 {
        let actual_value = self.decode_correction(correction);
        decode_difference(predicted_value, actual_value)
    }

    #[inline]
    fn decode_misprediction(&mut self, correction: CodecCorrection) -> bool {
        self.decode_correction(correction) != 0
    }

    /// if we encoded a 1 then swap the predicted value
    fn decode_correction_bool(
        &mut self,
        correction: CodecCorrection,
        predicted_value: bool,
    ) -> bool {
        predicted_value ^ (self.decode_correction(correction) != 0)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CodecAction {
    Correction(CodecCorrection, u32),
    VerifyState(&'static str, u64),
}

#[derive(Default)]
pub struct CountNonDefaultActions {
    pub total_non_default: u32,

    pub corrections_count: [u32; CodecCorrection::MAX as usize],
}

impl CountNonDefaultActions {
    pub fn record_correction(&mut self, correction: CodecCorrection, value: u32) {
        if value != 0 {
            self.corrections_count[correction as usize] += 1;
            self.total_non_default += 1;
        }
    }

    pub fn print(&self) {
        use CodecCorrection::*;

        let corr = [
            TokenCount,
            BlockTypeCorrection,
            LenCorrection,
            DistOnlyCorrection,
            DistAfterLenCorrection,
            TreeCodeBitLengthCorrection,
            LDTypeCorrection,
            RepeatCountCorrection,
            LDBitLengthCorrection,
            TreeCodeCountCorrection,
            LiteralCountCorrection,
            DistanceCountCorrection,
            UncompressBlockLenCorrection,
            Last,
            EndOfChunk,
            LiteralPredictionWrong,
            ReferencePredictionWrong,
        ];

        assert_eq!(
            corr.len(),
            CodecCorrection::MAX as usize,
            "need to update array if you add an enum"
        );

        for i in corr {
            if self.corrections_count[i as usize] != 0 {
                println!("{:?}: {}", i, self.corrections_count[i as usize]);
            }
        }
    }
}

pub struct VerifyPredictionDecoder {
    actions: Vec<CodecAction>,
    index: usize,
}

#[derive(Default)]
pub struct VerifyPredictionEncoder {
    actions: Vec<CodecAction>,
    count: CountNonDefaultActions,
}

// used for testing mostly
#[allow(dead_code)]
impl VerifyPredictionEncoder {
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
            count: CountNonDefaultActions::default(),
        }
    }

    pub fn actions(&self) -> Vec<CodecAction> {
        self.actions.clone()
    }

    pub fn print(&self) {
        self.count.print();
    }

    pub fn count_nondefault_actions(&self) -> usize {
        self.count.total_non_default as usize
    }
}

impl PredictionEncoder for VerifyPredictionEncoder {
    fn encode_verify_state(&mut self, message: &'static str, checksum: u64) {
        self.actions
            .push(CodecAction::VerifyState(message, checksum));
    }

    fn encode_correction(&mut self, action: CodecCorrection, value: u32) {
        self.actions.push(CodecAction::Correction(action, value));
        self.count.record_correction(action, value);
    }
    fn finish(&mut self) {}
}

// used for testing mostly
#[allow(dead_code)]
impl VerifyPredictionDecoder {
    pub fn new(actions: Vec<CodecAction>) -> Self {
        Self { actions, index: 0 }
    }

    fn pop(&mut self) -> Option<CodecAction> {
        if self.index >= self.actions.len() {
            None
        } else {
            self.index += 1;
            Some(self.actions[self.index - 1])
        }
    }
}

impl PredictionDecoder for VerifyPredictionDecoder {
    fn decode_verify_state(&mut self, message: &'static str, checksum: u64) {
        let x = self.pop().unwrap();
        assert_eq!(
            x,
            CodecAction::VerifyState(message, checksum),
            "mismatch {} (left encode, right decode)",
            self.index
        );
    }

    fn decode_correction(&mut self, correction: CodecCorrection) -> u32 {
        let x = self.pop().unwrap();
        match x {
            CodecAction::Correction(c, value) => {
                assert_eq!(correction, c);
                return value;
            }
            CodecAction::VerifyState(s, _h) => {
                panic!("found VerifyState {}, expected {:?}", s, correction);
            }
        }
    }
}

#[cfg(test)]
pub fn drive_encoder<T: PredictionEncoder>(encoder: &mut T, actions: &[CodecAction]) {
    for action in actions {
        match action {
            &CodecAction::Correction(correction, value) => {
                encoder.encode_correction(correction, value);
            }
            &CodecAction::VerifyState(message, checksum) => {
                encoder.encode_verify_state(message, checksum);
            }
        }
    }
}

#[cfg(test)]
pub fn verify_decoder<T: PredictionDecoder>(decoder: &mut T, actions: &[CodecAction]) {
    for action in actions {
        match action {
            &CodecAction::Correction(correction, value) => {
                let x = decoder.decode_correction(correction);
                assert_eq!(x, value);
            }
            &CodecAction::VerifyState(message, checksum) => {
                decoder.decode_verify_state(message, checksum);
            }
        }
    }
}

#[cfg(test)]
#[derive(Default)]
pub struct AssertDefaultOnlyDecoder {}

#[cfg(test)]
impl PredictionDecoder for AssertDefaultOnlyDecoder {
    fn decode_correction(&mut self, _correction: CodecCorrection) -> u32 {
        0
    }

    fn decode_verify_state(&mut self, _message: &'static str, _checksum: u64) {}
}

/// This implements a prediction encoder that tees the input to two different
/// encoders. This allows us to verify that the behavior of two encoders is the same
impl<A, B> PredictionEncoder for (A, B)
where
    A: PredictionEncoder,
    B: PredictionEncoder,
{
    fn encode_verify_state(&mut self, message: &'static str, checksum: u64) {
        self.0.encode_verify_state(message, checksum);
        self.1.encode_verify_state(message, checksum);
    }

    fn encode_correction(&mut self, action: CodecCorrection, value: u32) {
        self.0.encode_correction(action, value);
        self.1.encode_correction(action, value);
    }

    fn finish(&mut self) {
        self.0.finish();
        self.1.finish();
    }
}

/// Implement the same for decoders, where we verify that the output
/// is identical for both decoders
impl<A, B> PredictionDecoder for (A, B)
where
    A: PredictionDecoder,
    B: PredictionDecoder,
{
    fn decode_correction(&mut self, correction: CodecCorrection) -> u32 {
        let a = self.0.decode_correction(correction);
        let b = self.1.decode_correction(correction);
        assert_eq!(a, b);
        a
    }

    fn decode_verify_state(&mut self, message: &'static str, checksum: u64) {
        self.0.decode_verify_state(message, checksum);
        self.1.decode_verify_state(message, checksum);
    }
}
