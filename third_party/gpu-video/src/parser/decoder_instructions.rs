use h264_reader::nal::{pps::PicParameterSet, sps::SeqParameterSet};

use crate::parser::{
    h264::{AccessUnit, ParsedNalu},
    reference_manager::DecodeInformation,
    reference_manager::{ReferenceContext, ReferenceId, ReferenceManagementError},
};

#[derive(Debug, Clone)]
pub(crate) enum DecoderInstruction {
    Decode {
        decode_info: DecodeInformation,
        reference_id: ReferenceId,
    },

    Idr {
        decode_info: DecodeInformation,
        reference_id: ReferenceId,
    },

    Drop {
        reference_ids: Vec<ReferenceId>,
    },

    Sps(SeqParameterSet),

    Pps(PicParameterSet),
}

pub(crate) fn compile_to_decoder_instructions(
    reference_ctx: &mut ReferenceContext,
    access_units: Vec<AccessUnit>,
) -> Result<Vec<DecoderInstruction>, ReferenceManagementError> {
    let mut instructions = Vec::new();
    for AccessUnit(nalus) in access_units {
        let mut slices = Vec::new();
        for nalu in nalus {
            match nalu.parsed {
                ParsedNalu::Sps(seq_parameter_set) => {
                    instructions.push(DecoderInstruction::Sps(seq_parameter_set))
                }
                ParsedNalu::Pps(pic_parameter_set) => {
                    instructions.push(DecoderInstruction::Pps(pic_parameter_set))
                }
                ParsedNalu::Slice(slice) => {
                    slices.push((slice, nalu.pts));
                }

                ParsedNalu::Other(_) => {}
            }
        }

        // TODO: warn when not all pts are equal here
        let mut inst = reference_ctx.put_picture(slices)?;
        instructions.append(&mut inst);
    }

    Ok(instructions)
}
