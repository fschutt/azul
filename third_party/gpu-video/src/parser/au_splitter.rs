use h264_reader::nal::slice::PicOrderCountLsb;

use crate::parser::nalu_parser::{Nalu, ParsedNalu};

use super::nalu_parser::Slice;

#[derive(Default)]
pub(crate) struct AUSplitter {
    buffered_nals: Vec<Nalu>,
}

impl AUSplitter {
    pub(crate) fn put_nalu(&mut self, nalu: Nalu) -> Option<AccessUnit> {
        let result = match self.is_new_au(&nalu) {
            true => self.take_until_last_slice(),
            false => None,
        };

        self.buffered_nals.push(nalu);
        result
    }

    pub(crate) fn flush(&mut self) -> Option<AccessUnit> {
        // TODO: this is not the correct way to do this, since it assumes the user calls this method
        // in the wrong way. Only a temporary fix.
        self.take_until_last_slice()
    }

    fn take_until_last_slice(&mut self) -> Option<AccessUnit> {
        // if there are any slice NALUs, the last one is the end of the AU
        let i = self
            .buffered_nals
            .iter()
            .enumerate()
            .rev()
            .find(|(_, nalu)| matches!(nalu.parsed, ParsedNalu::Slice(_)))
            .map(|(i, _)| i);
        let au = match i {
            Some(i) => self.buffered_nals.drain(..=i).collect::<Vec<_>>(),
            None => Vec::new(),
        };
        if !au.is_empty() {
            Some(AccessUnit(au.into_boxed_slice()))
        } else {
            None
        }
    }

    /// returns `true` if `slice` is a first slice in an Access Unit
    fn is_new_au(&self, nalu: &Nalu) -> bool {
        let ParsedNalu::Slice(slice) = &nalu.parsed else {
            return false;
        };

        let Some(ParsedNalu::Slice(last)) = self
            .buffered_nals
            .iter()
            .rev()
            .map(|nalu| &nalu.parsed)
            .find(|nalu| matches!(nalu, ParsedNalu::Slice(_)))
        else {
            return true;
        };

        first_mb_in_slice_zero(slice)
            || frame_num_differs(last, slice)
            || pps_id_differs(last, slice)
            || field_pic_flag_differs(last, slice)
            || nal_ref_idc_differs_one_zero(last, slice)
            || pic_order_cnt_zero_check(last, slice)
            || idr_and_non_idr(last, slice)
            || idrs_where_idr_pic_id_differs(last, slice)
    }
}

// The below code is taken from Membrane's AU Splitter in their h264 parser. The comments contain
// elixir versions of the functions below them.

// defguardp first_mb_in_slice_zero(a)
//           when a.first_mb_in_slice == 0 and
//                  a.nal_unit_type in [1, 2, 5]
fn first_mb_in_slice_zero(slice: &Slice) -> bool {
    slice.header.first_mb_in_slice == 0
}

// defguardp frame_num_differs(a, b) when a.frame_num != b.frame_num
fn frame_num_differs(last: &Slice, curr: &Slice) -> bool {
    last.header.frame_num != curr.header.frame_num
}

// defguardp pic_parameter_set_id_differs(a, b)
//           when a.pic_parameter_set_id != b.pic_parameter_set_id
fn pps_id_differs(last: &Slice, curr: &Slice) -> bool {
    last.pps_id != curr.pps_id
}

// defguardp field_pic_flag_differs(a, b) when a.field_pic_flag != b.field_pic_flag
//
// defguardp bottom_field_flag_differs(a, b) when a.bottom_field_flag != b.bottom_field_flag
fn field_pic_flag_differs(last: &Slice, curr: &Slice) -> bool {
    last.header.field_pic != curr.header.field_pic
}

// defguardp nal_ref_idc_differs_one_zero(a, b)
//           when (a.nal_ref_idc == 0 or b.nal_ref_idc == 0) and
//                  a.nal_ref_idc != b.nal_ref_idc
fn nal_ref_idc_differs_one_zero(last: &Slice, curr: &Slice) -> bool {
    (last.nal_header.nal_ref_idc() == 0 || curr.nal_header.nal_ref_idc() == 0)
        && last.nal_header.nal_ref_idc() != curr.nal_header.nal_ref_idc()
}

// defguardp pic_order_cnt_zero_check(a, b)
//           when a.pic_order_cnt_type == 0 and b.pic_order_cnt_type == 0 and
//                  (a.pic_order_cnt_lsb != b.pic_order_cnt_lsb or
//                     a.delta_pic_order_cnt_bottom != b.delta_pic_order_cnt_bottom)
fn pic_order_cnt_zero_check(last: &Slice, curr: &Slice) -> bool {
    let (last_pic_order_cnt_lsb, last_delta_pic_order_cnt_bottom) =
        match last.header.pic_order_cnt_lsb {
            Some(PicOrderCountLsb::Frame(pic_order_cnt_lsb)) => (pic_order_cnt_lsb, 0),
            Some(PicOrderCountLsb::FieldsAbsolute {
                pic_order_cnt_lsb,
                delta_pic_order_cnt_bottom,
            }) => (pic_order_cnt_lsb, delta_pic_order_cnt_bottom),
            _ => return false,
        };

    let (curr_pic_order_cnt_lsb, curr_delta_pic_order_cnt_bottom) =
        match curr.header.pic_order_cnt_lsb {
            Some(PicOrderCountLsb::Frame(pic_order_cnt_lsb)) => (pic_order_cnt_lsb, 0),
            Some(PicOrderCountLsb::FieldsAbsolute {
                pic_order_cnt_lsb,
                delta_pic_order_cnt_bottom,
            }) => (pic_order_cnt_lsb, delta_pic_order_cnt_bottom),
            _ => return false,
        };

    last_pic_order_cnt_lsb != curr_pic_order_cnt_lsb
        || last_delta_pic_order_cnt_bottom != curr_delta_pic_order_cnt_bottom
}

// TODO
// defguardp pic_order_cnt_one_check_zero(a, b)
//           when a.pic_order_cnt_type == 1 and b.pic_order_cnt_type == 1 and
//                  hd(a.delta_pic_order_cnt) != hd(b.delta_pic_order_cnt)

// TODO
// defguardp pic_order_cnt_one_check_one(a, b)
//           when a.pic_order_cnt_type == 1 and b.pic_order_cnt_type == 1 and
//                  hd(hd(a.delta_pic_order_cnt)) != hd(hd(b.delta_pic_order_cnt))

// defguardp idr_and_non_idr(a, b)
//           when (a.nal_unit_type == 5 or b.nal_unit_type == 5) and
//                  a.nal_unit_type != b.nal_unit_type
fn idr_and_non_idr(last: &Slice, curr: &Slice) -> bool {
    (last.nal_header.nal_unit_type().id() == 5) ^ (curr.nal_header.nal_unit_type().id() == 5)
}

// defguardp idrs_with_idr_pic_id_differ(a, b)
//           when a.nal_unit_type == 5 and b.nal_unit_type == 5 and a.idr_pic_id != b.idr_pic_id
fn idrs_where_idr_pic_id_differs(last: &Slice, curr: &Slice) -> bool {
    match (last.header.idr_pic_id, curr.header.idr_pic_id) {
        (Some(last), Some(curr)) => last != curr,
        _ => false,
    }
}

/// Group of [`Nalu`]s representing one frame
pub struct AccessUnit(pub Box<[Nalu]>);
