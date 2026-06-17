use std::sync::Arc;

use h264_reader::nal::{
    pps::PicParameterSet,
    slice::{
        DecRefPicMarking, MemoryManagementControlOperation, ModificationOfPicNums, NumRefIdxActive,
        RefPicListModifications, SliceHeader,
    },
    sps::SeqParameterSet,
};

use crate::{parameters::MissedFrameHandling, parser::decoder_instructions::DecoderInstruction};

use super::nalu_parser::{Slice, SpsExt};

#[derive(Debug, thiserror::Error)]
pub enum ReferenceManagementError {
    #[error("SI frames are not supported")]
    SIFramesNotSupported,

    #[error("SP frames are not supported")]
    SPFramesNotSupported,

    #[error("PicOrderCntType {0} is not supperted")]
    PicOrderCntTypeNotSupported(u8),

    #[error("The H.264 bytestream is not spec compliant: {0}.")]
    IncorrectData(String),

    #[error("Missing frame. Decoder is in a corrupted state. Waiting for IDR frame")]
    MissingFrame,

    #[error(
        "A non-existing short-term reference remains in the active reference picture list after the modification process"
    )]
    NonExistingReferenceInActiveList,
}

#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReferenceId(usize);

#[derive(Debug, Clone, Copy)]
enum BFrameReferenceListKind {
    L0,
    L1,
}

#[derive(Debug, Default)]
#[allow(non_snake_case)]
pub(crate) struct ReferenceContext {
    pictures: ReferencePictures,
    next_reference_id: ReferenceId,
    prevFrameNum: u16,
    PrevRefFrameNum: u16,
    prev_pic_order_cnt_msb: i32,
    prev_pic_order_cnt_lsb: i32,
    MaxLongTermFrameIdx: MaxLongTermFrameIdx,
    prevFrameNumOffset: i64,
    previous_picture_included_mmco_equal_5: bool,
    detected_missed_frames: bool,
    missed_frame_handling: MissedFrameHandling,
}

#[derive(Debug, Default)]
enum MaxLongTermFrameIdx {
    #[default]
    NoLongTermFrameIndices,
    Idx(u64),
}

impl ReferenceContext {
    pub fn new(missed_frame_handling: MissedFrameHandling) -> Self {
        Self {
            missed_frame_handling,
            ..Default::default()
        }
    }

    fn next_reference_id(&mut self) -> ReferenceId {
        let result = self.next_reference_id;
        self.next_reference_id = ReferenceId(result.0 + 1);
        result
    }

    fn reset_state(&mut self) {
        *self = Self {
            pictures: ReferencePictures::default(),
            next_reference_id: ReferenceId::default(),
            prevFrameNum: 0,
            PrevRefFrameNum: 0,
            prev_pic_order_cnt_msb: 0,
            prev_pic_order_cnt_lsb: 0,
            MaxLongTermFrameIdx: MaxLongTermFrameIdx::NoLongTermFrameIndices,
            prevFrameNumOffset: 0,
            previous_picture_included_mmco_equal_5: false,
            detected_missed_frames: false,
            missed_frame_handling: self.missed_frame_handling,
        };
    }

    #[allow(non_snake_case)]
    fn add_long_term_reference(
        &mut self,
        frame_num: u16,
        LongTermFrameIdx: u64,
        pic_order_cnt: [i32; 2],
    ) -> ReferenceId {
        let id = self.next_reference_id();
        self.pictures.long_term.push(LongTermReferencePicture {
            frame_num,
            id,
            LongTermFrameIdx,
            pic_order_cnt,
        });

        id
    }

    fn add_short_term_reference(&mut self, frame_num: u16, pic_order_cnt: [i32; 2]) -> ReferenceId {
        let id = self.next_reference_id();
        self.pictures.short_term.push(ShortTermReferencePicture {
            frame_num,
            id,
            pic_order_cnt,
            non_existing: false,
        });
        id
    }

    fn add_non_existing_short_term_reference(
        &mut self,
        frame_num: u16,
        pic_order_cnt: [i32; 2],
    ) -> ReferenceId {
        let id = self.next_reference_id();
        self.pictures.short_term.push(ShortTermReferencePicture {
            frame_num,
            id,
            pic_order_cnt,
            non_existing: true,
        });
        id
    }

    pub(crate) fn mark_missed_frames(&mut self) {
        self.detected_missed_frames = true;
    }

    pub(crate) fn put_picture(
        &mut self,
        mut slices: Vec<(Slice, Option<u64>)>,
    ) -> Result<Vec<DecoderInstruction>, ReferenceManagementError> {
        let header = slices.last().unwrap().0.header.clone();
        let sps = slices.last().unwrap().0.sps.clone();
        let pps = slices.last().unwrap().0.pps.clone();
        let pts = slices.last().unwrap().1;

        let is_ref_frame = header.dec_ref_pic_marking.is_some();
        let is_idr = matches!(
            &header.dec_ref_pic_marking,
            Some(DecRefPicMarking::Idr { .. })
        );
        if is_ref_frame && !is_idr && self.missed_frame_handling == MissedFrameHandling::Strict {
            self.verify_frame_num(&sps, &header)?;
        }

        let has_gap = header.frame_num != self.PrevRefFrameNum
            && header.frame_num
                != ((self.PrevRefFrameNum as u32 + 1) % sps.max_frame_num() as u32) as u16;

        let gap_instructions = if sps.gaps_in_frame_num_value_allowed_flag && !is_idr && has_gap {
            self.handle_gaps_in_frame_num(&sps, header.frame_num)?
        } else {
            Vec::new()
        };

        // maybe this should be done in a different place, but if you think about it, there's not
        // really that many places to put this code in
        let mut rbsp_bytes = Vec::new();
        let mut slice_indices = Vec::new();
        for (slice, _) in &mut slices {
            if slice.rbsp_bytes.is_empty() {
                continue;
            }
            slice_indices.push(rbsp_bytes.len());
            rbsp_bytes.append(&mut slice.rbsp_bytes);
        }

        let decode_info = self.decode_information_for_frame(
            header.clone(),
            slice_indices,
            rbsp_bytes,
            &sps,
            &pps,
            pts,
        )?;

        let decoder_instructions = match &header.clone().dec_ref_pic_marking {
            Some(DecRefPicMarking::Idr {
                long_term_reference_flag,
                ..
            }) => self.reference_picture_marking_process_idr(
                header.clone(),
                decode_info,
                *long_term_reference_flag,
            )?,

            Some(DecRefPicMarking::SlidingWindow) => self
                .reference_picture_marking_process_sliding_window(
                    &sps,
                    header.clone(),
                    decode_info,
                )?,
            Some(DecRefPicMarking::Adaptive(memory_management_control_operations)) => self
                .reference_picture_marking_process_adaptive(
                    &sps,
                    header.clone(),
                    decode_info,
                    memory_management_control_operations,
                )?,

            // this picture is not a reference
            None => {
                let reference_id = self.next_reference_id();
                vec![
                    DecoderInstruction::Decode {
                        decode_info,
                        reference_id,
                    },
                    DecoderInstruction::Drop {
                        reference_ids: vec![reference_id],
                    },
                ]
            }
        };

        self.previous_picture_included_mmco_equal_5 = header.includes_mmco_equal_5();
        self.prevFrameNum = header.frame_num;
        if is_ref_frame {
            self.PrevRefFrameNum = header.frame_num;
        }

        let mut instructions = Vec::new();
        instructions.extend(gap_instructions);
        instructions.extend(decoder_instructions);

        Ok(instructions)
    }

    fn remove_long_term_ref(
        &mut self,
        long_term_frame_idx: u64,
    ) -> Result<LongTermReferencePicture, ReferenceManagementError> {
        for (i, frame) in self.pictures.long_term.iter().enumerate() {
            if frame.LongTermFrameIdx == long_term_frame_idx {
                return Ok(self.pictures.long_term.remove(i));
            }
        }

        Err(ReferenceManagementError::IncorrectData(format!(
            "cannot remove long term reference with id {long_term_frame_idx}, because it does not exist"
        )))
    }

    #[allow(non_snake_case)]
    fn remove_short_term_ref(
        &mut self,
        current_frame_num: i64,
        sps: &SeqParameterSet,
        pic_num_to_remove: i64,
    ) -> Result<ShortTermReferencePicture, ReferenceManagementError> {
        for (i, picture) in self.pictures.short_term.iter().enumerate() {
            let PicNum = decode_picture_numbers_for_short_term_ref(
                picture.frame_num.into(),
                current_frame_num,
                sps,
            )
            .PicNum;

            if PicNum == pic_num_to_remove {
                return Ok(self.pictures.short_term.remove(i));
            }
        }

        Err(ReferenceManagementError::IncorrectData(format!(
            "cannot remove short term reference with pic num {pic_num_to_remove}, because it does not exist"
        )))
    }

    fn reference_picture_marking_process_adaptive(
        &mut self,
        sps: &SeqParameterSet,
        header: Arc<SliceHeader>,
        decode_info: DecodeInformation,
        memory_management_control_operations: &[MemoryManagementControlOperation],
    ) -> Result<Vec<DecoderInstruction>, ReferenceManagementError> {
        let mut decoder_instructions = Vec::new();

        let mut new_long_term_frame_idx = None;

        for memory_management_control_operation in memory_management_control_operations {
            match memory_management_control_operation {
                MemoryManagementControlOperation::ShortTermUnusedForRef {
                    difference_of_pic_nums_minus1,
                } => {
                    let pic_num_to_remove =
                        header.frame_num as i64 - (*difference_of_pic_nums_minus1 as i64 + 1);

                    let removed = self.remove_short_term_ref(
                        header.frame_num.into(),
                        sps,
                        pic_num_to_remove,
                    )?;

                    if !removed.non_existing {
                        decoder_instructions.push(DecoderInstruction::Drop {
                            reference_ids: vec![removed.id],
                        });
                    }
                }

                MemoryManagementControlOperation::LongTermUnusedForRef { long_term_pic_num } => {
                    let removed = self.remove_long_term_ref(*long_term_pic_num as u64)?;

                    decoder_instructions.push(DecoderInstruction::Drop {
                        reference_ids: vec![removed.id],
                    });
                }

                MemoryManagementControlOperation::ShortTermUsedForLongTerm {
                    difference_of_pic_nums_minus1,
                    long_term_frame_idx,
                } => {
                    if let Ok(removed) = self.remove_long_term_ref(*long_term_frame_idx as u64) {
                        decoder_instructions.push(DecoderInstruction::Drop {
                            reference_ids: vec![removed.id],
                        });
                    }

                    let pic_num_to_remove =
                        header.frame_num as i64 - (*difference_of_pic_nums_minus1 as i64 + 1);

                    let picture = self.remove_short_term_ref(
                        header.frame_num.into(),
                        sps,
                        pic_num_to_remove,
                    )?;

                    if picture.non_existing {
                        return Err(ReferenceManagementError::IncorrectData(format!(
                            "MMCO 3 targets a non-existing short-term reference picture (pic_num = {pic_num_to_remove}), which is forbidden by H.264 §8.2.5.2 constraint d"
                        )));
                    }

                    self.pictures.long_term.push(LongTermReferencePicture {
                        frame_num: picture.frame_num,
                        LongTermFrameIdx: *long_term_frame_idx as u64,
                        pic_order_cnt: picture.pic_order_cnt,
                        id: picture.id,
                    });
                }

                MemoryManagementControlOperation::MaxUsedLongTermFrameRef {
                    max_long_term_frame_idx_plus1,
                } => {
                    if *max_long_term_frame_idx_plus1 != 0 {
                        self.MaxLongTermFrameIdx =
                            MaxLongTermFrameIdx::Idx(*max_long_term_frame_idx_plus1 as u64 - 1);
                    } else {
                        self.MaxLongTermFrameIdx = MaxLongTermFrameIdx::NoLongTermFrameIndices;
                    }

                    let max_idx = *max_long_term_frame_idx_plus1 as i128 - 1;

                    let reference_ids_to_remove = self
                        .pictures
                        .long_term
                        .iter()
                        .filter(|p| p.LongTermFrameIdx as i128 > max_idx)
                        .map(|p| p.id)
                        .collect();

                    self.pictures.long_term = self
                        .pictures
                        .long_term
                        .iter()
                        .filter(|p| p.LongTermFrameIdx as i128 <= max_idx)
                        .cloned()
                        .collect();

                    decoder_instructions.push(DecoderInstruction::Drop {
                        reference_ids: reference_ids_to_remove,
                    })
                }

                MemoryManagementControlOperation::AllRefPicturesUnused => {
                    let reference_ids = self
                        .pictures
                        .short_term
                        .drain(..)
                        .filter(|p| !p.non_existing)
                        .map(|p| p.id)
                        .chain(self.pictures.long_term.drain(..).map(|p| p.id))
                        .collect();

                    self.MaxLongTermFrameIdx = MaxLongTermFrameIdx::NoLongTermFrameIndices;

                    decoder_instructions.push(DecoderInstruction::Drop { reference_ids })
                }
                MemoryManagementControlOperation::CurrentUsedForLongTerm {
                    long_term_frame_idx,
                } => {
                    if let Ok(picture) = self.remove_long_term_ref(*long_term_frame_idx as u64) {
                        decoder_instructions.push(DecoderInstruction::Drop {
                            reference_ids: vec![picture.id],
                        });
                    }

                    new_long_term_frame_idx = Some(*long_term_frame_idx as u64);
                }
            }
        }

        let reference_id = match new_long_term_frame_idx {
            Some(long_term_frame_idx) => self.add_long_term_reference(
                header.frame_num,
                long_term_frame_idx,
                decode_info.picture_info.PicOrderCnt_as_reference_pic,
            ),
            None => self.add_short_term_reference(
                header.frame_num,
                decode_info.picture_info.PicOrderCnt_as_reference_pic,
            ),
        };

        decoder_instructions.insert(
            0,
            DecoderInstruction::Decode {
                decode_info,
                reference_id,
            },
        );

        if let MaxLongTermFrameIdx::Idx(max) = self.MaxLongTermFrameIdx {
            if self.pictures.long_term.len() > max as usize + 1 {
                return Err(ReferenceManagementError::IncorrectData(format!(
                    "there are {} long-term references, but there shouldn't be more than {max}",
                    self.pictures.long_term.len()
                )));
            }
        }

        Ok(decoder_instructions)
    }

    fn reference_picture_marking_process_sliding_window(
        &mut self,
        sps: &SeqParameterSet,
        header: Arc<SliceHeader>,
        decode_info: DecodeInformation,
    ) -> Result<Vec<DecoderInstruction>, ReferenceManagementError> {
        let reference_id = self.add_short_term_reference(
            header.frame_num,
            decode_info.picture_info.PicOrderCnt_as_reference_pic,
        );

        let mut decoder_instructions = vec![DecoderInstruction::Decode {
            decode_info,
            reference_id,
        }];

        decoder_instructions
            .extend(self.evict_oldest_short_term_if_over_capacity(sps, header.frame_num));

        Ok(decoder_instructions)
    }

    fn evict_oldest_short_term_if_over_capacity(
        &mut self,
        sps: &SeqParameterSet,
        current_frame_num: u16,
    ) -> Option<DecoderInstruction> {
        let max_num_ref = sps.max_num_ref_frames.max(1) as usize;
        if self.pictures.short_term.len() + self.pictures.long_term.len() <= max_num_ref {
            return None;
        }
        if self.pictures.short_term.is_empty() {
            return None;
        }

        let (idx, _) = self
            .pictures
            .short_term
            .iter()
            .enumerate()
            .min_by_key(|(_, reference)| {
                decode_picture_numbers_for_short_term_ref(
                    reference.frame_num.into(),
                    current_frame_num.into(),
                    sps,
                )
                .FrameNumWrap
            })
            .unwrap();

        let removed = self.pictures.short_term.remove(idx);
        if removed.non_existing {
            None
        } else {
            Some(DecoderInstruction::Drop {
                reference_ids: vec![removed.id],
            })
        }
    }

    fn reference_picture_marking_process_idr(
        &mut self,
        header: Arc<SliceHeader>,
        decode_info: DecodeInformation,
        long_term_reference_flag: bool,
    ) -> Result<Vec<DecoderInstruction>, ReferenceManagementError> {
        self.reset_state();

        let reference_id = if long_term_reference_flag {
            self.MaxLongTermFrameIdx = MaxLongTermFrameIdx::Idx(0);
            self.add_long_term_reference(
                header.frame_num,
                0,
                decode_info.picture_info.PicOrderCnt_as_reference_pic,
            )
        } else {
            self.MaxLongTermFrameIdx = MaxLongTermFrameIdx::NoLongTermFrameIndices;
            self.add_short_term_reference(
                header.frame_num,
                decode_info.picture_info.PicOrderCnt_as_reference_pic,
            )
        };

        Ok(vec![DecoderInstruction::Idr {
            decode_info,
            reference_id,
        }])
    }

    #[allow(non_snake_case)]
    fn decode_information_for_frame(
        &mut self,
        header: Arc<SliceHeader>,
        slice_indices: Vec<usize>,
        rbsp_bytes: Vec<u8>,
        sps: &SeqParameterSet,
        pps: &PicParameterSet,
        pts: Option<u64>,
    ) -> Result<DecodeInformation, ReferenceManagementError> {
        let PicOrderCnt_for_decoding = self.decode_pic_order_cnt(&header, sps)?;
        let PicOrderCnt_as_reference_pic = if header.includes_mmco_equal_5() {
            [0, 0]
        } else {
            PicOrderCnt_for_decoding
        };

        let (reference_list_l0, reference_list_l1) = match header.slice_type.family {
            h264_reader::nal::slice::SliceFamily::P => {
                let num_ref_idx_l0_active = header.num_ref_idx_l0_active(pps);

                let mut reference_list_l0 =
                    self.initialize_reference_picture_list_for_p_frame(&header, sps)?;

                match &header.ref_pic_list_modification {
                    Some(RefPicListModifications::P {
                        ref_pic_list_modification_l0,
                    }) => {
                        self.modify_reference_picture_list(
                            sps,
                            &header,
                            &mut reference_list_l0,
                            ref_pic_list_modification_l0,
                        )?;
                    }

                    None
                    | Some(RefPicListModifications::I)
                    | Some(RefPicListModifications::B { .. }) => return Err(ReferenceManagementError::IncorrectData(
                        "a slice marked 'P' slice family contains a reference picture list for a different family".into()
                    ))?,
                }

                reference_list_l0.truncate(num_ref_idx_l0_active as usize);

                if reference_list_l0.iter().any(|p| p.non_existing) {
                    return Err(ReferenceManagementError::NonExistingReferenceInActiveList);
                }

                (Some(reference_list_l0), None)
            }
            h264_reader::nal::slice::SliceFamily::I => (None, None),
            h264_reader::nal::slice::SliceFamily::B => {
                let num_ref_idx_l0_active = header.num_ref_idx_l0_active(pps);
                let num_ref_idx_l1_active = header.num_ref_idx_l1_active(pps)?;

                let mut reference_list_l0 = self.initialize_reference_picture_list_for_b_frame(
                    sps,
                    PicOrderCnt_for_decoding,
                    BFrameReferenceListKind::L0,
                )?;
                let mut reference_list_l1 = self.initialize_reference_picture_list_for_b_frame(
                    sps,
                    PicOrderCnt_for_decoding,
                    BFrameReferenceListKind::L1,
                )?;

                match &header.ref_pic_list_modification {
                    Some(RefPicListModifications::B {
                        ref_pic_list_modification_l0,
                        ref_pic_list_modification_l1,
                    }) => {
                        self.modify_reference_picture_list(
                            sps,
                            &header,
                            &mut reference_list_l0,
                            ref_pic_list_modification_l0,
                        )?;

                        self.modify_reference_picture_list(
                            sps,
                            &header,
                            &mut reference_list_l1,
                            ref_pic_list_modification_l1
                        )?;
                    }

                    None
                    | Some(RefPicListModifications::I)
                    | Some(RefPicListModifications::P { .. }) => return Err(ReferenceManagementError::IncorrectData(
                        "a slice marked 'B' slice family contains a reference picture list for a different family".into()
                    ))?,
                }

                reference_list_l0.truncate(num_ref_idx_l0_active as usize);
                reference_list_l1.truncate(num_ref_idx_l1_active as usize);

                if reference_list_l0.iter().any(|p| p.non_existing)
                    || reference_list_l1.iter().any(|p| p.non_existing)
                {
                    return Err(ReferenceManagementError::NonExistingReferenceInActiveList);
                }

                (Some(reference_list_l0), Some(reference_list_l1))
            }
            h264_reader::nal::slice::SliceFamily::SP => {
                return Err(ReferenceManagementError::SPFramesNotSupported);
            }
            h264_reader::nal::slice::SliceFamily::SI => {
                return Err(ReferenceManagementError::SIFramesNotSupported);
            }
        };

        Ok(DecodeInformation {
            reference_list_l0,
            reference_list_l1,
            header: header.clone(),
            slice_indices,
            rbsp_bytes,
            sps_id: sps.id().id(),
            pps_id: pps.pic_parameter_set_id.id(),
            picture_info: PictureInfo {
                non_existing: false,
                used_for_long_term_reference: false,
                PicOrderCnt_for_decoding,
                PicOrderCnt_as_reference_pic,
                FrameNum: header.frame_num,
            },
            pts,
        })
    }

    // This is outside of spec, but I think it would not be good if a malicious bitstream could
    // trivially force us to do an arbitrary amount of work
    const MAX_GAP_SIZE: u32 = 512;

    #[allow(non_snake_case)]
    fn handle_gaps_in_frame_num(
        &mut self,
        sps: &SeqParameterSet,
        target_frame_num: u16,
    ) -> Result<Vec<DecoderInstruction>, ReferenceManagementError> {
        let MaxFrameNum = sps.max_frame_num() as u32;
        let mut UnusedShortTermFrameNum = (self.PrevRefFrameNum as u32 + 1) % MaxFrameNum;
        let mut instructions = Vec::new();
        let mut iterations = 0u32;

        while UnusedShortTermFrameNum != target_frame_num as u32 {
            iterations += 1;
            if iterations > Self::MAX_GAP_SIZE {
                return Err(ReferenceManagementError::IncorrectData(format!(
                    "gap in frame_num exceeds {}: PrevRefFrameNum={}, target={}",
                    Self::MAX_GAP_SIZE,
                    self.PrevRefFrameNum,
                    target_frame_num
                )));
            }
            let frame_num = UnusedShortTermFrameNum as u16;

            let pic_order_cnt = match sps.pic_order_cnt {
                h264_reader::nal::sps::PicOrderCntType::TypeZero { .. } => [0; 2],
                h264_reader::nal::sps::PicOrderCntType::TypeOne { .. } => {
                    return Err(ReferenceManagementError::PicOrderCntTypeNotSupported(1));
                }
                h264_reader::nal::sps::PicOrderCntType::TypeTwo => {
                    self.decode_pic_order_cnt_type_two(sps, frame_num, false, true)
                }
            };

            self.add_non_existing_short_term_reference(frame_num, pic_order_cnt);
            // we only have to do this, because since vulkan never sees the non-existing frames, the
            // only effect they have on it is removing some existing entries
            instructions.extend(self.evict_oldest_short_term_if_over_capacity(sps, frame_num));

            self.prevFrameNum = frame_num;
            self.PrevRefFrameNum = frame_num;
            self.previous_picture_included_mmco_equal_5 = false;

            UnusedShortTermFrameNum = (UnusedShortTermFrameNum + 1) % MaxFrameNum;
        }

        Ok(instructions)
    }

    fn decode_pic_order_cnt(
        &mut self,
        header: &SliceHeader,
        sps: &SeqParameterSet,
    ) -> Result<[i32; 2], ReferenceManagementError> {
        match sps.pic_order_cnt {
            h264_reader::nal::sps::PicOrderCntType::TypeZero {
                log2_max_pic_order_cnt_lsb_minus4,
            } => self.decode_pic_order_cnt_type_zero(header, log2_max_pic_order_cnt_lsb_minus4),

            h264_reader::nal::sps::PicOrderCntType::TypeOne { .. } => {
                Err(ReferenceManagementError::PicOrderCntTypeNotSupported(1))
            }

            h264_reader::nal::sps::PicOrderCntType::TypeTwo => Ok(self
                .decode_pic_order_cnt_type_two(
                    sps,
                    header.frame_num,
                    header.idr_pic_id.is_some(),
                    header.dec_ref_pic_marking.is_some(),
                )),
        }
    }

    #[allow(non_snake_case)]
    fn decode_pic_order_cnt_type_two(
        &mut self,
        sps: &SeqParameterSet,
        frame_num: u16,
        is_idr: bool,
        is_reference: bool,
    ) -> [i32; 2] {
        let FrameNumOffset = if is_idr {
            0
        } else {
            let prevFrameNumOffset = if self.previous_picture_included_mmco_equal_5 {
                0
            } else {
                self.prevFrameNumOffset
            };

            if self.prevFrameNum > frame_num {
                prevFrameNumOffset + sps.max_frame_num()
            } else {
                prevFrameNumOffset
            }
        };

        let tempPicOrderCnt = if is_idr {
            0
        } else if !is_reference {
            2 * (FrameNumOffset as i32 + frame_num as i32) - 1
        } else {
            2 * (FrameNumOffset as i32 + frame_num as i32)
        };

        self.prevFrameNumOffset = FrameNumOffset;

        [tempPicOrderCnt; 2]
    }

    fn decode_pic_order_cnt_type_zero(
        &mut self,
        header: &SliceHeader,
        log2_max_pic_order_cnt_lsb_minus4: u8,
    ) -> Result<[i32; 2], ReferenceManagementError> {
        let pic_order_cnt_lsb =
            header
                .pic_order_cnt_lsb
                .as_ref()
                .ok_or(ReferenceManagementError::IncorrectData(
                "pic_order_cnt_lsb is not present in a slice header, but is required for decoding"
                    .into(),
            ))?;

        let (pic_order_cnt_lsb, delta_pic_order_cnt_bottom) = match pic_order_cnt_lsb{
            h264_reader::nal::slice::PicOrderCountLsb::Frame(pic_order_cnt_lsb) => {
                (*pic_order_cnt_lsb, 0)
            }

            h264_reader::nal::slice::PicOrderCountLsb::FieldsAbsolute {
                pic_order_cnt_lsb,
                delta_pic_order_cnt_bottom,
            } => (*pic_order_cnt_lsb, *delta_pic_order_cnt_bottom),

            h264_reader::nal::slice::PicOrderCountLsb::FieldsDelta(_) => {
                return Err(ReferenceManagementError::IncorrectData("pic_order_cnt_lsb is not present in a slice header, but is required for decoding".into()))
            }
        };

        let is_idr = header.idr_pic_id.is_some();
        let pic_order_cnt_lsb = pic_order_cnt_lsb as i32;
        let is_frame = header.field_pic == h264_reader::nal::slice::FieldPic::Frame;

        // this section is very hard to read, but all of this code is just copied from the
        // h.264 spec, where it looks almost exactly like this
        let max_pic_order_cnt_lsb = 2_i32.pow(log2_max_pic_order_cnt_lsb_minus4 as u32 + 4);

        let (prev_pic_order_cnt_msb, prev_pic_order_cnt_lsb) = if is_idr {
            (0, 0)
        } else {
            (self.prev_pic_order_cnt_msb, self.prev_pic_order_cnt_lsb)
        };

        let pic_order_cnt_msb = if pic_order_cnt_lsb < prev_pic_order_cnt_lsb
            && prev_pic_order_cnt_lsb - pic_order_cnt_lsb >= max_pic_order_cnt_lsb / 2
        {
            prev_pic_order_cnt_msb + max_pic_order_cnt_lsb
        } else if pic_order_cnt_lsb > prev_pic_order_cnt_lsb
            && pic_order_cnt_lsb - prev_pic_order_cnt_lsb > max_pic_order_cnt_lsb / 2
        {
            prev_pic_order_cnt_msb - max_pic_order_cnt_lsb
        } else {
            prev_pic_order_cnt_msb
        };

        let pic_order_cnt = if is_frame {
            let top_field_order_cnt = pic_order_cnt_msb + pic_order_cnt_lsb;

            let bottom_field_order_cnt = top_field_order_cnt + delta_pic_order_cnt_bottom;

            top_field_order_cnt.min(bottom_field_order_cnt)
        } else {
            pic_order_cnt_msb + pic_order_cnt_lsb
        };

        self.prev_pic_order_cnt_msb = pic_order_cnt_msb;
        self.prev_pic_order_cnt_lsb = pic_order_cnt_lsb;

        Ok([pic_order_cnt; 2])
    }

    fn initialize_short_term_reference_picture_list_for_p_frame(
        &self,
        header: &SliceHeader,
        sps: &SeqParameterSet,
    ) -> Vec<ReferencePictureInfo> {
        let mut short_term_reference_list = self
            .pictures
            .short_term
            .iter()
            .map(|reference| {
                (
                    reference,
                    decode_picture_numbers_for_short_term_ref(
                        reference.frame_num.into(),
                        header.frame_num.into(),
                        sps,
                    ),
                )
            })
            .collect::<Vec<_>>();

        short_term_reference_list.sort_by_key(|(_, numbers)| -numbers.PicNum);

        short_term_reference_list
            .into_iter()
            .map(|(reference, numbers)| ReferencePictureInfo {
                id: reference.id,
                LongTermPicNum: None,
                FrameNum: numbers.FrameNum as u16,
                non_existing: reference.non_existing,
                PicOrderCnt: reference.pic_order_cnt,
            })
            .collect()
    }

    fn initialize_long_term_reference_picture_list_for_frame(&self) -> Vec<ReferencePictureInfo> {
        let mut long_term_reference_list = self.pictures.long_term.clone();

        long_term_reference_list.sort_by_key(|pic| pic.LongTermFrameIdx);

        long_term_reference_list
            .into_iter()
            .map(|pic| ReferencePictureInfo {
                id: pic.id,
                LongTermPicNum: Some(pic.LongTermFrameIdx),
                PicOrderCnt: pic.pic_order_cnt,
                non_existing: false,
                FrameNum: pic.frame_num,
            })
            .collect()
    }

    fn initialize_reference_picture_list_for_p_frame(
        &self,
        header: &SliceHeader,
        sps: &SeqParameterSet,
    ) -> Result<Vec<ReferencePictureInfo>, ReferenceManagementError> {
        let short_term_reference_list =
            self.initialize_short_term_reference_picture_list_for_p_frame(header, sps);

        let long_term_reference_list = self.initialize_long_term_reference_picture_list_for_frame();

        let reference_list = short_term_reference_list
            .into_iter()
            .chain(long_term_reference_list)
            .collect::<Vec<_>>();

        Ok(reference_list)
    }

    #[allow(non_snake_case)]
    fn initialize_reference_picture_list_for_b_frame(
        &self,
        sps: &SeqParameterSet,
        CurrPicOrderCnt: [i32; 2],
        list_kind: BFrameReferenceListKind,
    ) -> Result<Vec<ReferencePictureInfo>, ReferenceManagementError> {
        let short_term_reference_list = self
            .initialize_short_term_reference_picture_list_for_b_frame(
                sps,
                CurrPicOrderCnt,
                list_kind,
            )?;

        let long_term_reference_list = self.initialize_long_term_reference_picture_list_for_frame();

        let reference_list = short_term_reference_list
            .into_iter()
            .chain(long_term_reference_list)
            .collect();

        Ok(reference_list)
    }

    fn verify_frame_num(
        &mut self,
        sps: &SeqParameterSet,
        header: &SliceHeader,
    ) -> Result<(), ReferenceManagementError> {
        let is_expected_frame_num = !sps.gaps_in_frame_num_value_allowed_flag
            && header.frame_num != self.PrevRefFrameNum
            && header.frame_num != ((self.PrevRefFrameNum as i64 + 1) % sps.max_frame_num()) as u16;
        if is_expected_frame_num || self.detected_missed_frames {
            self.detected_missed_frames = true;
            return Err(ReferenceManagementError::MissingFrame);
        }

        Ok(())
    }

    #[allow(non_snake_case)]
    fn initialize_short_term_reference_picture_list_for_b_frame(
        &self,
        sps: &SeqParameterSet,
        CurrPicOrderCnt: [i32; 2],
        list_kind: BFrameReferenceListKind,
    ) -> Result<Vec<ReferencePictureInfo>, ReferenceManagementError> {
        let is_poc_type_zero = matches!(
            sps.pic_order_cnt,
            h264_reader::nal::sps::PicOrderCntType::TypeZero { .. }
        );

        let eligible = self
            .pictures
            .short_term
            .iter()
            .filter(|pic| !is_poc_type_zero || !pic.non_existing);

        let (mut primary, mut remaining): (Vec<_>, Vec<_>) =
            eligible.partition(|pic| match list_kind {
                BFrameReferenceListKind::L0 => pic.pic_order_cnt < CurrPicOrderCnt,
                BFrameReferenceListKind::L1 => pic.pic_order_cnt > CurrPicOrderCnt,
            });

        primary.sort_by_key(|pic| match list_kind {
            BFrameReferenceListKind::L0 => -pic.pic_order_cnt[0],
            BFrameReferenceListKind::L1 => pic.pic_order_cnt[0],
        });

        remaining.sort_by_key(|pic| match list_kind {
            BFrameReferenceListKind::L0 => pic.pic_order_cnt[0],
            BFrameReferenceListKind::L1 => -pic.pic_order_cnt[0],
        });

        let reference_list = primary
            .into_iter()
            .chain(remaining)
            .map(|pic| ReferencePictureInfo {
                LongTermPicNum: None,
                FrameNum: pic.frame_num,
                non_existing: pic.non_existing,
                PicOrderCnt: pic.pic_order_cnt,
                id: pic.id,
            })
            .collect();

        Ok(reference_list)
    }

    #[allow(non_snake_case)]
    fn modify_reference_picture_list(
        &self,
        sps: &SeqParameterSet,
        header: &SliceHeader,
        reference_list: &mut Vec<ReferencePictureInfo>,
        ref_pic_list_modifications: &[ModificationOfPicNums],
    ) -> Result<(), ReferenceManagementError> {
        // 0 is Subtract, 1 is Add, 2 is LongTermRef
        let mut refIdxLX = 0;
        let mut picNumLXPred = header.frame_num as i64;

        for ref_pic_list_modification in ref_pic_list_modifications {
            match ref_pic_list_modification {
                ModificationOfPicNums::Subtract(_) | ModificationOfPicNums::Add(_) => {
                    self.modify_short_term_reference_picture_list(
                        sps,
                        header,
                        reference_list,
                        ref_pic_list_modification,
                        &mut refIdxLX,
                        &mut picNumLXPred,
                    )?;
                }

                ModificationOfPicNums::LongTermRef(long_term_pic_num) => {
                    self.modify_long_term_reference_picture_list(
                        reference_list,
                        *long_term_pic_num,
                        &mut refIdxLX,
                    )?;
                }
            }
        }

        Ok(())
    }

    #[allow(non_snake_case)]
    fn modify_long_term_reference_picture_list(
        &self,
        reference_list: &mut Vec<ReferencePictureInfo>,
        picture_to_shift: u32,
        refIdxLX: &mut usize,
    ) -> Result<(), ReferenceManagementError> {
        let shifted_picture_idx = reference_list
            .iter()
            .enumerate()
            .find(|(_, pic)| match pic.LongTermPicNum {
                Some(num) => num == picture_to_shift as u64,
                None => false,
            })
            .map(|(i, _)| i)
            .ok_or(ReferenceManagementError::IncorrectData(
                format!("picture with LongTermPicNum = {picture_to_shift} is not present in the reference list during modification")
            ))?;

        if reference_list[shifted_picture_idx].non_existing {
            return Err(ReferenceManagementError::IncorrectData(
                "a reference picture marked for shifting in the long-term reference list modification process is marked as non-existing".into()
            ));
        }

        let shifted_picture = reference_list.remove(shifted_picture_idx);
        reference_list.insert(*refIdxLX, shifted_picture);
        *refIdxLX += 1;

        Ok(())
    }

    #[allow(non_snake_case)]
    fn modify_short_term_reference_picture_list(
        &self,
        sps: &SeqParameterSet,
        header: &SliceHeader,
        reference_list: &mut Vec<ReferencePictureInfo>,
        ref_pic_list_modification: &ModificationOfPicNums,
        refIdxLX: &mut usize,
        picNumLXPred: &mut i64,
    ) -> Result<(), ReferenceManagementError> {
        let picNumLXNoWrap = match *ref_pic_list_modification {
            ModificationOfPicNums::Subtract(abs_diff_pic_num_minus_1) => {
                let abs_diff_pic_num = abs_diff_pic_num_minus_1 as i64 + 1;
                if *picNumLXPred - abs_diff_pic_num < 0 {
                    *picNumLXPred - abs_diff_pic_num + sps.max_frame_num()
                } else {
                    *picNumLXPred - abs_diff_pic_num
                }
            }
            ModificationOfPicNums::Add(abs_diff_pic_num_minus_1) => {
                let abs_diff_pic_num = abs_diff_pic_num_minus_1 as i64 + 1;
                if *picNumLXPred + abs_diff_pic_num >= sps.max_frame_num() {
                    *picNumLXPred + abs_diff_pic_num - sps.max_frame_num()
                } else {
                    *picNumLXPred + abs_diff_pic_num
                }
            }
            ModificationOfPicNums::LongTermRef(_) => return Ok(()),
        };

        *picNumLXPred = picNumLXNoWrap;

        let picNumLX = if picNumLXNoWrap > header.frame_num as i64 {
            picNumLXNoWrap - sps.max_frame_num()
        } else {
            picNumLXNoWrap
        };

        let mut shifted_picture_idx = reference_list
            .iter()
            .enumerate()
            .find(|(_, picture_info)| decode_picture_numbers_for_short_term_ref(picture_info.FrameNum.into(), header.frame_num.into(), sps).PicNum == picNumLX)
            .map(|(i, _)| i)
            .ok_or(ReferenceManagementError::IncorrectData(
                format!("picture with picNumLX = {picNumLX} is not present in the reference list during modification")
            ))?;

        if reference_list[shifted_picture_idx].non_existing {
            return Err(ReferenceManagementError::IncorrectData(
                "a short-term reference picture marked for shifting in the reference list modification process is marked as non-existing".into()
            ));
        }

        if reference_list[shifted_picture_idx].is_long_term() {
            return Err(ReferenceManagementError::IncorrectData(
                "a long-term reference picture marked for shifting in the short-term reference list modification process".into()
            ));
        }

        let shifted_picture_info = reference_list[shifted_picture_idx];
        if *refIdxLX <= reference_list.len() {
            reference_list.insert(*refIdxLX, shifted_picture_info);
            shifted_picture_idx = if *refIdxLX <= shifted_picture_idx {
                shifted_picture_idx + 1
            } else {
                shifted_picture_idx
            };
        }
        *refIdxLX += 1;
        reference_list.remove(shifted_picture_idx);

        Ok(())
    }
}

#[derive(Debug)]
struct ShortTermReferencePicture {
    frame_num: u16,
    id: ReferenceId,
    pic_order_cnt: [i32; 2],
    non_existing: bool,
}

#[allow(non_snake_case)]
fn decode_picture_numbers_for_short_term_ref(
    frame_num: i64,
    current_frame_num: i64,
    sps: &SeqParameterSet,
) -> ShortTermReferencePictureNumbers {
    let MaxFrameNum = sps.max_frame_num();

    let FrameNum = frame_num;

    let FrameNumWrap = if FrameNum > current_frame_num {
        FrameNum - MaxFrameNum
    } else {
        FrameNum
    };

    // this assumes we're dealing with a short-term reference frame
    let PicNum = FrameNumWrap;

    ShortTermReferencePictureNumbers {
        FrameNum,
        FrameNumWrap,
        PicNum,
    }
}

#[derive(Debug, Clone)]
#[allow(non_snake_case)]
struct LongTermReferencePicture {
    frame_num: u16,
    LongTermFrameIdx: u64,
    id: ReferenceId,
    pic_order_cnt: [i32; 2],
}

#[allow(non_snake_case)]
struct ShortTermReferencePictureNumbers {
    FrameNum: i64,

    FrameNumWrap: i64,

    PicNum: i64,
}

#[derive(Debug, Default)]
struct ReferencePictures {
    long_term: Vec<LongTermReferencePicture>,
    short_term: Vec<ShortTermReferencePicture>,
}

trait SliceHeaderExt {
    fn num_ref_idx_l0_active(&self, pps: &PicParameterSet) -> u32;
    fn num_ref_idx_l1_active(&self, pps: &PicParameterSet)
    -> Result<u32, ReferenceManagementError>;
    fn includes_mmco_equal_5(&self) -> bool;
}

impl SliceHeaderExt for SliceHeader {
    fn num_ref_idx_l0_active(&self, pps: &PicParameterSet) -> u32 {
        self.num_ref_idx_active
            .as_ref()
            .map(|num| match num {
                NumRefIdxActive::P {
                    num_ref_idx_l0_active_minus1,
                } => *num_ref_idx_l0_active_minus1,
                NumRefIdxActive::B {
                    num_ref_idx_l0_active_minus1,
                    ..
                } => *num_ref_idx_l0_active_minus1,
            })
            .unwrap_or(pps.num_ref_idx_l0_default_active_minus1)
            + 1
    }

    fn num_ref_idx_l1_active(
        &self,
        pps: &PicParameterSet,
    ) -> Result<u32, ReferenceManagementError> {
        Ok(
            self
                .num_ref_idx_active
                .as_ref()
                .map(|num| match num {
                    NumRefIdxActive::P { .. } => Err(ReferenceManagementError::IncorrectData(
                        "requested num_ref_idx_l1_active, but the header contains the information for a P-frame, which does not include it".into()
                    )),
                    NumRefIdxActive::B { num_ref_idx_l1_active_minus1, .. } => Ok(*num_ref_idx_l1_active_minus1)
                })
                .unwrap_or(Ok(pps.num_ref_idx_l1_default_active_minus1))? + 1
        )
    }

    fn includes_mmco_equal_5(&self) -> bool {
        let Some(DecRefPicMarking::Adaptive(ref mmcos)) = self.dec_ref_pic_marking else {
            return false;
        };

        mmcos
            .iter()
            .any(|mmco| matches!(mmco, MemoryManagementControlOperation::AllRefPicturesUnused))
    }
}

#[derive(Clone, derivative::Derivative)]
#[derivative(Debug)]
pub struct DecodeInformation {
    pub(crate) reference_list_l0: Option<Vec<ReferencePictureInfo>>,
    pub(crate) reference_list_l1: Option<Vec<ReferencePictureInfo>>,
    #[derivative(Debug = "ignore")]
    pub(crate) rbsp_bytes: Vec<u8>,
    pub(crate) slice_indices: Vec<usize>,
    #[derivative(Debug = "ignore")]
    pub(crate) header: Arc<SliceHeader>,
    pub(crate) sps_id: u8,
    pub(crate) pps_id: u8,
    pub(crate) picture_info: PictureInfo,
    pub(crate) pts: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
#[allow(non_snake_case)]
pub(crate) struct ReferencePictureInfo {
    pub(crate) id: ReferenceId,
    pub(crate) LongTermPicNum: Option<u64>,
    pub(crate) non_existing: bool,
    pub(crate) FrameNum: u16,
    pub(crate) PicOrderCnt: [i32; 2],
}

impl ReferencePictureInfo {
    pub fn is_long_term(&self) -> bool {
        self.LongTermPicNum.is_some()
    }
}

#[derive(Debug, Clone, Copy)]
#[allow(non_snake_case)]
pub(crate) struct PictureInfo {
    pub(crate) used_for_long_term_reference: bool,
    pub(crate) non_existing: bool,
    pub(crate) FrameNum: u16,
    pub(crate) PicOrderCnt_for_decoding: [i32; 2],
    pub(crate) PicOrderCnt_as_reference_pic: [i32; 2],
}
