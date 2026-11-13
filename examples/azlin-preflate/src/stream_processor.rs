/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE in this crate for license information.
 *--------------------------------------------------------------------------------------------*/

//! Responsible for performing preflate and recreation on a chunk by chunk basis
//!
//! Changed for azlin-preflate: the corrections go through the range coder
//! (`correction_codec`), the parameters are written as explicit bytes
//! (`TokenPredictorParameters::to_bytes`) instead of bitcode, a stream the predictor does not
//! consume fully is an error instead of a panic, and a rebuild stops with an error when its
//! output passes the length the caller expects or its corrections run out (damaged
//! corrections could otherwise make it write empty blocks forever).

use std::io::{BufRead, Cursor};


use crate::{
    PreflateConfig, Result,
    correction_codec::{PredictionDecoderRc, PredictionEncoderRc},
    deflate::{
        deflate_reader::DeflateParser, deflate_token::DeflateTokenBlock,
        deflate_writer::DeflateWriter,
    },
    estimator::preflate_parameter_estimator::{
        TokenPredictorParameters, estimate_preflate_parameters,
    },
    preflate_error::{AddContext, ExitCode, PreflateError},
    preflate_input::{PlainText, PreflateInput},
    statistical_codec::{CodecCorrection, PredictionDecoder, PredictionEncoder},
    token_predictor::TokenPredictor,
};

/// the data required to reconstruct the deflate stream exactly the way that it was: the
/// parameters' bytes (`TokenPredictorParameters::BYTES`), then the coded corrections
struct ReconstructionData<'a> {
    pub parameters: TokenPredictorParameters,
    pub corrections: &'a [u8],
}

impl<'a> ReconstructionData<'a> {
    pub fn read(data: &'a [u8]) -> Result<Self> {
        let Some((params, corrections)) = data.split_at_checked(TokenPredictorParameters::BYTES)
        else {
            return Err(PreflateError::new(
                ExitCode::InvalidCompressedWrapper,
                "the reconstruction data is shorter than its parameters",
            ));
        };
        Ok(Self {
            parameters: TokenPredictorParameters::from_bytes(params)?,
            corrections,
        })
    }

    pub fn write(parameters: &TokenPredictorParameters, corrections: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(TokenPredictorParameters::BYTES + corrections.len());
        out.extend_from_slice(&parameters.to_bytes());
        out.extend_from_slice(corrections);
        out
    }
}

/// Result of a call to PreflateStreamProcessor::decompress
pub struct PreflateStreamChunkResult {
    /// the extra data that is needed to reconstruct the deflate stream exactly as it was written
    pub corrections: Vec<u8>,

    /// the number of bytes that were processed from the compressed stream (this will be exactly the
    /// data that will be recreated using the cabac_encoded data)
    pub compressed_size: usize,

    /// the parameters that were used to compress the stream. Only returned for the first
    /// chunk that is passed in.
    pub parameters: Option<TokenPredictorParameters>,

    pub blocks: Vec<DeflateTokenBlock>,
}

/// Takes a stream of deflate compressed data removes the deflate compression, recording the data
/// that can be used to reconstruct it along with the plain-text.
#[derive(Debug)]
pub struct PreflateStreamProcessor {
    predictor: Option<TokenPredictor>,
    validator: Option<RecreateStreamProcessor>,
    parser: DeflateParser,
    max_chain_length: u32,
}

impl PreflateStreamProcessor {
    /// Creates a new PreflateStreamProcessor
    /// plain_text_limit: the maximum size of the plain text that will decompressed to memory
    /// verify: if true, the decompressed data will be recompressed and compared to the original as it is run
    pub fn new(config: &PreflateConfig) -> Self {
        Self {
            predictor: None,
            parser: DeflateParser::new(config.plain_text_limit),
            max_chain_length: config.max_chain_length,
            validator: if config.verify_compression {
                Some(RecreateStreamProcessor::new())
            } else {
                None
            },
        }
    }

    pub fn is_done(&self) -> bool {
        self.parser.is_done()
    }

    pub fn plain_text(&self) -> &PlainText {
        &self.parser.plain_text()
    }

    pub fn shrink_to_dictionary(&mut self) {
        self.parser.shrink_to_dictionary();
    }

    pub fn detach_plain_text(self) -> PlainText {
        self.parser.detach_plain_text()
    }

    /// decompresses a deflate stream and returns the plaintext and cabac_encoded data that can be used to reconstruct it
    pub fn decompress(&mut self, compressed_data: &[u8]) -> Result<PreflateStreamChunkResult> {
        let contents = self.parser.parse(compressed_data)?;

        let mut cabac_encoder = PredictionEncoderRc::new();

        if let Some(predictor) = &mut self.predictor {
            let mut input = PreflateInput::new(&self.parser.plain_text());

            // we are missing the last couple hashes in the dictionary since we didn't
            // have the full plaintext yet.
            predictor.add_missing_previous_hash(&input);

            predict_blocks(&contents.blocks, predictor, &mut cabac_encoder, &mut input)?;

            cabac_encoder.finish();
            let cabac_encoded = cabac_encoder.into_bytes();

            if let Some(validator) = &mut self.validator {
                let (recompressed, _rec_blocks) = validator.recompress(
                    &mut Cursor::new(self.parser.plain_text().text()),
                    &cabac_encoded,
                )?;

                #[cfg(test)]
                for i in 0..contents.blocks.len() {
                    crate::utils::assert_block_eq(&contents.blocks[i], &_rec_blocks[i]);
                }

                // we should always succeed here in test code
                #[cfg(test)]
                crate::utils::assert_eq_array(
                    &recompressed,
                    &compressed_data[..contents.compressed_size],
                );

                if recompressed[..] != compressed_data[..contents.compressed_size] {
                    return Err(PreflateError::new(
                        ExitCode::RoundtripMismatch,
                        "recompressed data does not match original",
                    ));
                }
            }

            Ok(PreflateStreamChunkResult {
                corrections: cabac_encoded,
                compressed_size: contents.compressed_size,
                parameters: None,
                blocks: contents.blocks,
            })
        } else {
            let params =
                estimate_preflate_parameters(&contents, &self.parser.plain_text()).context()?;

            if params.max_chain > self.max_chain_length {
                return Err(PreflateError::new(
                    ExitCode::NoCompressionCandidates,
                    format!(
                        "max_chain {} is larger than configured max_chain_length {}",
                        params.max_chain, self.max_chain_length
                    ),
                ));
            }

            let mut input = PreflateInput::new(&self.parser.plain_text());

            let mut token_predictor = TokenPredictor::new(&params);

            predict_blocks(
                &contents.blocks,
                &mut token_predictor,
                &mut cabac_encoder,
                &mut input,
            )?;

            cabac_encoder.finish();

            let reconstruction_data =
                ReconstructionData::write(&params, &cabac_encoder.into_bytes());

            self.predictor = Some(token_predictor);

            if let Some(validator) = &mut self.validator {
                let (recompressed, _rec_blocks) = validator.recompress(
                    &mut Cursor::new(self.parser.plain_text().text()),
                    &reconstruction_data,
                )?;

                #[cfg(test)]
                for i in 0..contents.blocks.len() {
                    crate::utils::assert_block_eq(&contents.blocks[i], &_rec_blocks[i]);
                }

                // we should always succeed here in test code
                #[cfg(test)]
                crate::utils::assert_eq_array(
                    &recompressed,
                    &compressed_data[..contents.compressed_size],
                );

                if recompressed[..] != compressed_data[..contents.compressed_size] {
                    return Err(PreflateError::new(
                        ExitCode::RoundtripMismatch,
                        "recompressed data does not match original",
                    ));
                }
            }

            Ok(PreflateStreamChunkResult {
                corrections: reconstruction_data,
                compressed_size: contents.compressed_size,
                parameters: Some(params),
                blocks: contents.blocks,
            })
        }
    }
}

/// Decompresses a deflate stream and returns the plaintext and diff data that can be used to reconstruct it
/// via recreate_whole_deflate_stream
pub fn preflate_whole_deflate_stream(
    compressed_data: &[u8],
    config: &PreflateConfig,
) -> Result<(PreflateStreamChunkResult, PlainText)> {
    let mut state = PreflateStreamProcessor::new(config);
    let r = state.decompress(compressed_data)?;

    Ok((r, state.parser.detach_plain_text()))
}

/// recreates the original deflate stream, piece-by-piece
#[derive(Debug)]
pub struct RecreateStreamProcessor {
    predictor: Option<TokenPredictor>,
    writer: DeflateWriter,
    plain_text: PlainText,
    /// The most bytes the rebuilt stream may take (the caller knows the original's length).
    max_output: usize,
}

impl RecreateStreamProcessor {
    pub fn new() -> Self {
        Self::with_max_output(usize::MAX)
    }

    /// A processor whose rebuilt stream may take at most `max_output` bytes.
    pub fn with_max_output(max_output: usize) -> Self {
        Self {
            predictor: None,
            writer: DeflateWriter::new(),
            plain_text: PlainText::new(),
            max_output,
        }
    }

    pub fn recompress(
        &mut self,
        plain_text: &mut impl BufRead,
        corrections: &[u8],
    ) -> Result<(Vec<u8>, Vec<DeflateTokenBlock>)> {
        loop {
            let buf = plain_text.fill_buf().context()?;
            let buf_len = buf.len();
            if buf_len == 0 {
                break;
            }

            self.plain_text.append(&buf);

            plain_text.consume(buf_len);
        }

        let mut input = PreflateInput::new(&self.plain_text);

        if let Some(predictor) = &mut self.predictor {
            let mut cabac_decoder = PredictionDecoderRc::new(corrections);

            predictor.add_missing_previous_hash(&input);

            let blocks = recreate_blocks(
                predictor,
                &mut cabac_decoder,
                &mut self.writer,
                &mut input,
                self.max_output,
            )
            .context()?;

            self.plain_text.shrink_to_dictionary();

            self.writer.flush();

            Ok((self.writer.detach_output(), blocks))
        } else {
            let r = ReconstructionData::read(corrections)?;

            let mut predictor = TokenPredictor::new(&r.parameters);

            let mut cabac_decoder = PredictionDecoderRc::new(r.corrections);

            let blocks = recreate_blocks(
                &mut predictor,
                &mut cabac_decoder,
                &mut self.writer,
                &mut input,
                self.max_output,
            )
            .context()?;

            self.predictor = Some(predictor);

            self.plain_text.shrink_to_dictionary();

            self.writer.flush();

            Ok((self.writer.detach_output(), blocks))
        }
    }
}

/// Rebuilds a deflate stream from its plain text and the corrections
/// `preflate_whole_deflate_stream` returned; the stream is at most `max_len` bytes (else the
/// corrections are damaged).
pub fn recreate_whole_deflate_stream(
    plain_text: &[u8],
    prediction_corrections: &[u8],
    max_len: usize,
) -> Result<Vec<u8>> {
    let mut state = RecreateStreamProcessor::with_max_output(max_len);

    let (recompressed, _) =
        state.recompress(&mut Cursor::new(&plain_text), prediction_corrections)?;

    // The blocks are checked as they are written; the last partial byte comes with the flush.
    if recompressed.len() > max_len {
        return Err(PreflateError::new(
            ExitCode::InvalidCompressedWrapper,
            "the corrections are damaged: the rebuilt stream runs past its length",
        ));
    }
    Ok(recompressed)
}

fn predict_blocks(
    blocks: &[DeflateTokenBlock],
    token_predictor: &mut TokenPredictor,
    encoder: &mut impl PredictionEncoder,
    input: &mut PreflateInput,
) -> Result<()> {
    for i in 0..blocks.len() {
        token_predictor.predict_block(&blocks[i], encoder, input, i == blocks.len() - 1)?;
        // end of stream normally is the last block
        encoder.encode_correction_bool(
            CodecCorrection::EndOfChunk,
            i == blocks.len() - 1,
            input.remaining() == 0,
        );
    }
    if input.remaining() != 0 {
        return Err(PreflateError::new(
            ExitCode::InvalidDeflate,
            "the predictor did not consume the whole plain text",
        ));
    }
    Ok(())
}

fn recreate_blocks<D: PredictionDecoder>(
    token_predictor: &mut TokenPredictor,
    decoder: &mut D,
    deflate_writer: &mut DeflateWriter,
    input: &mut PreflateInput,
    max_output: usize,
) -> Result<Vec<DeflateTokenBlock>> {
    let mut output_blocks = Vec::new();
    loop {
        let block = token_predictor.recreate_block(decoder, input)?;

        deflate_writer.encode_block(&block)?;

        // Every block writes at least three bits, so this ends any run of empty blocks.
        if deflate_writer.output_len() > max_output || decoder.overran() {
            return Err(PreflateError::new(
                ExitCode::InvalidCompressedWrapper,
                "the corrections are damaged: the rebuilt stream runs past its length",
            ));
        }

        output_blocks.push(block);

        // end of stream normally is the last block
        let last =
            decoder.decode_correction_bool(CodecCorrection::EndOfChunk, input.remaining() == 0);

        if last {
            break;
        }
    }
    Ok(output_blocks)
}
