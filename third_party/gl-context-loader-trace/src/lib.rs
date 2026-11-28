// Copyright 2014 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(non_snake_case)]

#[cfg(feature = "std")]
extern crate core;
#[macro_use]
extern crate alloc;

use core::{ptr, mem};

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::string::ToString;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

#[cfg(feature = "gleam_trait")]
extern crate gleam;

#[allow(non_camel_case_types, dead_code)]
pub mod ctypes {
    pub enum c_void {}
    pub type c_char = i8;
    pub type c_schar = i8;
    pub type c_uchar = u8;
    pub type c_short = i16;
    pub type c_ushort = u16;
    pub type c_int = i32;
    pub type c_uint = u32;
    pub type c_long = i64;
    pub type c_ulong = u64;
    pub type c_longlong = i64;
    pub type c_ulonglong = u64;
    pub type c_float = f32;
    pub type c_double = f64;
    pub type __int8 = i8;
    pub type __uint8 = u8;
    pub type __int16 = i16;
    pub type __uint16 = u16;
    pub type __int32 = i32;
    pub type __uint32 = u32;
    pub type __int64 = i64;
    pub type __uint64 = u64;
    pub type wchar_t = u16;
}

#[allow(non_camel_case_types, non_upper_case_globals)]
pub mod gl {
    use super::GLenum;

    pub const ACCUM: GLenum = 0x0100;
    pub const MAP_PERSISTENT_BIT: GLenum = 64;
    pub const ACCUM_ALPHA_BITS: GLenum = 0x0D5B;
    pub const ACCUM_BLUE_BITS: GLenum = 0x0D5A;
    pub const ACCUM_BUFFER_BIT: GLenum = 0x00000200;
    pub const ACCUM_CLEAR_VALUE: GLenum = 0x0B80;
    pub const ACCUM_GREEN_BITS: GLenum = 0x0D59;
    pub const ACCUM_RED_BITS: GLenum = 0x0D58;
    pub const ACTIVE_ATTRIBUTES: GLenum = 0x8B89;
    pub const ACTIVE_ATTRIBUTE_MAX_LENGTH: GLenum = 0x8B8A;
    pub const ACTIVE_TEXTURE: GLenum = 0x84E0;
    pub const ACTIVE_UNIFORMS: GLenum = 0x8B86;
    pub const ACTIVE_UNIFORM_BLOCKS: GLenum = 0x8A36;
    pub const ACTIVE_UNIFORM_BLOCK_MAX_NAME_LENGTH: GLenum = 0x8A35;
    pub const ACTIVE_UNIFORM_MAX_LENGTH: GLenum = 0x8B87;
    pub const ADD: GLenum = 0x0104;
    pub const ADD_SIGNED: GLenum = 0x8574;
    pub const ALIASED_LINE_WIDTH_RANGE: GLenum = 0x846E;
    pub const ALIASED_POINT_SIZE_RANGE: GLenum = 0x846D;
    pub const ALL_ATTRIB_BITS: GLenum = 0xFFFFFFFF;
    pub const ALPHA: GLenum = 0x1906;
    pub const ALPHA12: GLenum = 0x803D;
    pub const ALPHA16: GLenum = 0x803E;
    pub const ALPHA16F_EXT: GLenum = 0x881C;
    pub const ALPHA32F_EXT: GLenum = 0x8816;
    pub const ALPHA4: GLenum = 0x803B;
    pub const ALPHA8: GLenum = 0x803C;
    pub const ALPHA8_EXT: GLenum = 0x803C;
    pub const ALPHA_BIAS: GLenum = 0x0D1D;
    pub const ALPHA_BITS: GLenum = 0x0D55;
    pub const ALPHA_INTEGER: GLenum = 0x8D97;
    pub const ALPHA_SCALE: GLenum = 0x0D1C;
    pub const ALPHA_TEST: GLenum = 0x0BC0;
    pub const ALPHA_TEST_FUNC: GLenum = 0x0BC1;
    pub const ALPHA_TEST_REF: GLenum = 0x0BC2;
    pub const ALREADY_SIGNALED: GLenum = 0x911A;
    pub const ALWAYS: GLenum = 0x0207;
    pub const AMBIENT: GLenum = 0x1200;
    pub const AMBIENT_AND_DIFFUSE: GLenum = 0x1602;
    pub const AND: GLenum = 0x1501;
    pub const AND_INVERTED: GLenum = 0x1504;
    pub const AND_REVERSE: GLenum = 0x1502;
    pub const ANY_SAMPLES_PASSED: GLenum = 0x8C2F;
    pub const ANY_SAMPLES_PASSED_CONSERVATIVE: GLenum = 0x8D6A;
    pub const ARRAY_BUFFER: GLenum = 0x8892;
    pub const ARRAY_BUFFER_BINDING: GLenum = 0x8894;
    pub const ATTACHED_SHADERS: GLenum = 0x8B85;
    pub const ATTRIB_STACK_DEPTH: GLenum = 0x0BB0;
    pub const AUTO_NORMAL: GLenum = 0x0D80;
    pub const AUX0: GLenum = 0x0409;
    pub const AUX1: GLenum = 0x040A;
    pub const AUX2: GLenum = 0x040B;
    pub const AUX3: GLenum = 0x040C;
    pub const AUX_BUFFERS: GLenum = 0x0C00;
    pub const BACK: GLenum = 0x0405;
    pub const BACK_LEFT: GLenum = 0x0402;
    pub const BACK_RIGHT: GLenum = 0x0403;
    pub const BGR: GLenum = 0x80E0;
    pub const BGRA: GLenum = 0x80E1;
    pub const BGRA8_EXT: GLenum = 0x93A1;
    pub const BGRA_EXT: GLenum = 0x80E1;
    pub const BGRA_INTEGER: GLenum = 0x8D9B;
    pub const BGR_INTEGER: GLenum = 0x8D9A;
    pub const BITMAP: GLenum = 0x1A00;
    pub const BITMAP_TOKEN: GLenum = 0x0704;
    pub const BLEND: GLenum = 0x0BE2;
    pub const BLEND_ADVANCED_COHERENT_KHR: GLenum = 0x9285;
    pub const BLEND_COLOR: GLenum = 0x8005;
    pub const BLEND_DST: GLenum = 0x0BE0;
    pub const BLEND_DST_ALPHA: GLenum = 0x80CA;
    pub const BLEND_DST_RGB: GLenum = 0x80C8;
    pub const BLEND_EQUATION: GLenum = 0x8009;
    pub const BLEND_EQUATION_ALPHA: GLenum = 0x883D;
    pub const BLEND_EQUATION_RGB: GLenum = 0x8009;
    pub const BLEND_SRC: GLenum = 0x0BE1;
    pub const BLEND_SRC_ALPHA: GLenum = 0x80CB;
    pub const BLEND_SRC_RGB: GLenum = 0x80C9;
    pub const BLUE: GLenum = 0x1905;
    pub const BLUE_BIAS: GLenum = 0x0D1B;
    pub const BLUE_BITS: GLenum = 0x0D54;
    pub const BLUE_INTEGER: GLenum = 0x8D96;
    pub const BLUE_SCALE: GLenum = 0x0D1A;
    pub const BOOL: GLenum = 0x8B56;
    pub const BOOL_VEC2: GLenum = 0x8B57;
    pub const BOOL_VEC3: GLenum = 0x8B58;
    pub const BOOL_VEC4: GLenum = 0x8B59;
    pub const BUFFER: GLenum = 0x82E0;
    pub const BUFFER_ACCESS: GLenum = 0x88BB;
    pub const BUFFER_ACCESS_FLAGS: GLenum = 0x911F;
    pub const BUFFER_KHR: GLenum = 0x82E0;
    pub const BUFFER_MAPPED: GLenum = 0x88BC;
    pub const BUFFER_MAP_LENGTH: GLenum = 0x9120;
    pub const BUFFER_MAP_OFFSET: GLenum = 0x9121;
    pub const BUFFER_MAP_POINTER: GLenum = 0x88BD;
    pub const BUFFER_SIZE: GLenum = 0x8764;
    pub const BUFFER_USAGE: GLenum = 0x8765;
    pub const BYTE: GLenum = 0x1400;
    pub const C3F_V3F: GLenum = 0x2A24;
    pub const C4F_N3F_V3F: GLenum = 0x2A26;
    pub const C4UB_V2F: GLenum = 0x2A22;
    pub const C4UB_V3F: GLenum = 0x2A23;
    pub const CCW: GLenum = 0x0901;
    pub const CLAMP: GLenum = 0x2900;
    pub const CLAMP_FRAGMENT_COLOR: GLenum = 0x891B;
    pub const CLAMP_READ_COLOR: GLenum = 0x891C;
    pub const CLAMP_TO_BORDER: GLenum = 0x812D;
    pub const CLAMP_TO_EDGE: GLenum = 0x812F;
    pub const CLAMP_VERTEX_COLOR: GLenum = 0x891A;
    pub const CLEAR: GLenum = 0x1500;
    pub const CLIENT_ACTIVE_TEXTURE: GLenum = 0x84E1;
    pub const CLIENT_ALL_ATTRIB_BITS: GLenum = 0xFFFFFFFF;
    pub const CLIENT_ATTRIB_STACK_DEPTH: GLenum = 0x0BB1;
    pub const CLIENT_PIXEL_STORE_BIT: GLenum = 0x00000001;
    pub const CLIENT_VERTEX_ARRAY_BIT: GLenum = 0x00000002;
    pub const CLIP_DISTANCE0: GLenum = 0x3000;
    pub const CLIP_DISTANCE1: GLenum = 0x3001;
    pub const CLIP_DISTANCE2: GLenum = 0x3002;
    pub const CLIP_DISTANCE3: GLenum = 0x3003;
    pub const CLIP_DISTANCE4: GLenum = 0x3004;
    pub const CLIP_DISTANCE5: GLenum = 0x3005;
    pub const CLIP_DISTANCE6: GLenum = 0x3006;
    pub const CLIP_DISTANCE7: GLenum = 0x3007;
    pub const CLIP_PLANE0: GLenum = 0x3000;
    pub const CLIP_PLANE1: GLenum = 0x3001;
    pub const CLIP_PLANE2: GLenum = 0x3002;
    pub const CLIP_PLANE3: GLenum = 0x3003;
    pub const CLIP_PLANE4: GLenum = 0x3004;
    pub const CLIP_PLANE5: GLenum = 0x3005;
    pub const COEFF: GLenum = 0x0A00;
    pub const COLOR: GLenum = 0x1800;
    pub const COLORBURN_KHR: GLenum = 0x929A;
    pub const COLORDODGE_KHR: GLenum = 0x9299;
    pub const COLOR_ARRAY: GLenum = 0x8076;
    pub const COLOR_ARRAY_BUFFER_BINDING: GLenum = 0x8898;
    pub const COLOR_ARRAY_POINTER: GLenum = 0x8090;
    pub const COLOR_ARRAY_SIZE: GLenum = 0x8081;
    pub const COLOR_ARRAY_STRIDE: GLenum = 0x8083;
    pub const COLOR_ARRAY_TYPE: GLenum = 0x8082;
    pub const COLOR_ATTACHMENT0: GLenum = 0x8CE0;
    pub const COLOR_ATTACHMENT1: GLenum = 0x8CE1;
    pub const COLOR_ATTACHMENT10: GLenum = 0x8CEA;
    pub const COLOR_ATTACHMENT11: GLenum = 0x8CEB;
    pub const COLOR_ATTACHMENT12: GLenum = 0x8CEC;
    pub const COLOR_ATTACHMENT13: GLenum = 0x8CED;
    pub const COLOR_ATTACHMENT14: GLenum = 0x8CEE;
    pub const COLOR_ATTACHMENT15: GLenum = 0x8CEF;
    pub const COLOR_ATTACHMENT16: GLenum = 0x8CF0;
    pub const COLOR_ATTACHMENT17: GLenum = 0x8CF1;
    pub const COLOR_ATTACHMENT18: GLenum = 0x8CF2;
    pub const COLOR_ATTACHMENT19: GLenum = 0x8CF3;
    pub const COLOR_ATTACHMENT2: GLenum = 0x8CE2;
    pub const COLOR_ATTACHMENT20: GLenum = 0x8CF4;
    pub const COLOR_ATTACHMENT21: GLenum = 0x8CF5;
    pub const COLOR_ATTACHMENT22: GLenum = 0x8CF6;
    pub const COLOR_ATTACHMENT23: GLenum = 0x8CF7;
    pub const COLOR_ATTACHMENT24: GLenum = 0x8CF8;
    pub const COLOR_ATTACHMENT25: GLenum = 0x8CF9;
    pub const COLOR_ATTACHMENT26: GLenum = 0x8CFA;
    pub const COLOR_ATTACHMENT27: GLenum = 0x8CFB;
    pub const COLOR_ATTACHMENT28: GLenum = 0x8CFC;
    pub const COLOR_ATTACHMENT29: GLenum = 0x8CFD;
    pub const COLOR_ATTACHMENT3: GLenum = 0x8CE3;
    pub const COLOR_ATTACHMENT30: GLenum = 0x8CFE;
    pub const COLOR_ATTACHMENT31: GLenum = 0x8CFF;
    pub const COLOR_ATTACHMENT4: GLenum = 0x8CE4;
    pub const COLOR_ATTACHMENT5: GLenum = 0x8CE5;
    pub const COLOR_ATTACHMENT6: GLenum = 0x8CE6;
    pub const COLOR_ATTACHMENT7: GLenum = 0x8CE7;
    pub const COLOR_ATTACHMENT8: GLenum = 0x8CE8;
    pub const COLOR_ATTACHMENT9: GLenum = 0x8CE9;
    pub const COLOR_BUFFER_BIT: GLenum = 0x00004000;
    pub const COLOR_CLEAR_VALUE: GLenum = 0x0C22;
    pub const COLOR_INDEX: GLenum = 0x1900;
    pub const COLOR_INDEXES: GLenum = 0x1603;
    pub const COLOR_LOGIC_OP: GLenum = 0x0BF2;
    pub const COLOR_MATERIAL: GLenum = 0x0B57;
    pub const COLOR_MATERIAL_FACE: GLenum = 0x0B55;
    pub const COLOR_MATERIAL_PARAMETER: GLenum = 0x0B56;
    pub const COLOR_SUM: GLenum = 0x8458;
    pub const COLOR_WRITEMASK: GLenum = 0x0C23;
    pub const COMBINE: GLenum = 0x8570;
    pub const COMBINE_ALPHA: GLenum = 0x8572;
    pub const COMBINE_RGB: GLenum = 0x8571;
    pub const COMPARE_REF_TO_TEXTURE: GLenum = 0x884E;
    pub const COMPARE_R_TO_TEXTURE: GLenum = 0x884E;
    pub const COMPILE: GLenum = 0x1300;
    pub const COMPILE_AND_EXECUTE: GLenum = 0x1301;
    pub const COMPILE_STATUS: GLenum = 0x8B81;
    pub const COMPRESSED_ALPHA: GLenum = 0x84E9;
    pub const COMPRESSED_INTENSITY: GLenum = 0x84EC;
    pub const COMPRESSED_LUMINANCE: GLenum = 0x84EA;
    pub const COMPRESSED_LUMINANCE_ALPHA: GLenum = 0x84EB;
    pub const COMPRESSED_R11_EAC: GLenum = 0x9270;
    pub const COMPRESSED_RED: GLenum = 0x8225;
    pub const COMPRESSED_RED_RGTC1: GLenum = 0x8DBB;
    pub const COMPRESSED_RG: GLenum = 0x8226;
    pub const COMPRESSED_RG11_EAC: GLenum = 0x9272;
    pub const COMPRESSED_RGB: GLenum = 0x84ED;
    pub const COMPRESSED_RGB8_ETC2: GLenum = 0x9274;
    pub const COMPRESSED_RGB8_PUNCHTHROUGH_ALPHA1_ETC2: GLenum = 0x9276;
    pub const COMPRESSED_RGBA: GLenum = 0x84EE;
    pub const COMPRESSED_RGBA8_ETC2_EAC: GLenum = 0x9278;
    pub const COMPRESSED_RG_RGTC2: GLenum = 0x8DBD;
    pub const COMPRESSED_SIGNED_R11_EAC: GLenum = 0x9271;
    pub const COMPRESSED_SIGNED_RED_RGTC1: GLenum = 0x8DBC;
    pub const COMPRESSED_SIGNED_RG11_EAC: GLenum = 0x9273;
    pub const COMPRESSED_SIGNED_RG_RGTC2: GLenum = 0x8DBE;
    pub const COMPRESSED_SLUMINANCE: GLenum = 0x8C4A;
    pub const COMPRESSED_SLUMINANCE_ALPHA: GLenum = 0x8C4B;
    pub const COMPRESSED_SRGB: GLenum = 0x8C48;
    pub const COMPRESSED_SRGB8_ALPHA8_ETC2_EAC: GLenum = 0x9279;
    pub const COMPRESSED_SRGB8_ETC2: GLenum = 0x9275;
    pub const COMPRESSED_SRGB8_PUNCHTHROUGH_ALPHA1_ETC2: GLenum = 0x9277;
    pub const COMPRESSED_SRGB_ALPHA: GLenum = 0x8C49;
    pub const COMPRESSED_TEXTURE_FORMATS: GLenum = 0x86A3;
    pub const CONDITION_SATISFIED: GLenum = 0x911C;
    pub const CONSTANT: GLenum = 0x8576;
    pub const CONSTANT_ALPHA: GLenum = 0x8003;
    pub const CONSTANT_ATTENUATION: GLenum = 0x1207;
    pub const CONSTANT_COLOR: GLenum = 0x8001;
    pub const CONTEXT_COMPATIBILITY_PROFILE_BIT: GLenum = 0x00000002;
    pub const CONTEXT_CORE_PROFILE_BIT: GLenum = 0x00000001;
    pub const CONTEXT_FLAGS: GLenum = 0x821E;
    pub const CONTEXT_FLAG_DEBUG_BIT: GLenum = 0x00000002;
    pub const CONTEXT_FLAG_DEBUG_BIT_KHR: GLenum = 0x00000002;
    pub const CONTEXT_FLAG_FORWARD_COMPATIBLE_BIT: GLenum = 0x00000001;
    pub const CONTEXT_PROFILE_MASK: GLenum = 0x9126;
    pub const COORD_REPLACE: GLenum = 0x8862;
    pub const COPY: GLenum = 0x1503;
    pub const COPY_INVERTED: GLenum = 0x150C;
    pub const COPY_PIXEL_TOKEN: GLenum = 0x0706;
    pub const COPY_READ_BUFFER: GLenum = 0x8F36;
    pub const COPY_READ_BUFFER_BINDING: GLenum = 0x8F36;
    pub const COPY_WRITE_BUFFER: GLenum = 0x8F37;
    pub const COPY_WRITE_BUFFER_BINDING: GLenum = 0x8F37;
    pub const CULL_FACE: GLenum = 0x0B44;
    pub const CULL_FACE_MODE: GLenum = 0x0B45;
    pub const CURRENT_BIT: GLenum = 0x00000001;
    pub const CURRENT_COLOR: GLenum = 0x0B00;
    pub const CURRENT_FOG_COORD: GLenum = 0x8453;
    pub const CURRENT_FOG_COORDINATE: GLenum = 0x8453;
    pub const CURRENT_INDEX: GLenum = 0x0B01;
    pub const CURRENT_NORMAL: GLenum = 0x0B02;
    pub const CURRENT_PROGRAM: GLenum = 0x8B8D;
    pub const CURRENT_QUERY: GLenum = 0x8865;
    pub const CURRENT_QUERY_EXT: GLenum = 0x8865;
    pub const CURRENT_RASTER_COLOR: GLenum = 0x0B04;
    pub const CURRENT_RASTER_DISTANCE: GLenum = 0x0B09;
    pub const CURRENT_RASTER_INDEX: GLenum = 0x0B05;
    pub const CURRENT_RASTER_POSITION: GLenum = 0x0B07;
    pub const CURRENT_RASTER_POSITION_VALID: GLenum = 0x0B08;
    pub const CURRENT_RASTER_SECONDARY_COLOR: GLenum = 0x845F;
    pub const CURRENT_RASTER_TEXTURE_COORDS: GLenum = 0x0B06;
    pub const CURRENT_SECONDARY_COLOR: GLenum = 0x8459;
    pub const CURRENT_TEXTURE_COORDS: GLenum = 0x0B03;
    pub const CURRENT_VERTEX_ATTRIB: GLenum = 0x8626;
    pub const CW: GLenum = 0x0900;
    pub const DARKEN_KHR: GLenum = 0x9297;
    pub const DEBUG_CALLBACK_FUNCTION: GLenum = 0x8244;
    pub const DEBUG_CALLBACK_FUNCTION_KHR: GLenum = 0x8244;
    pub const DEBUG_CALLBACK_USER_PARAM: GLenum = 0x8245;
    pub const DEBUG_CALLBACK_USER_PARAM_KHR: GLenum = 0x8245;
    pub const DEBUG_GROUP_STACK_DEPTH: GLenum = 0x826D;
    pub const DEBUG_GROUP_STACK_DEPTH_KHR: GLenum = 0x826D;
    pub const DEBUG_LOGGED_MESSAGES: GLenum = 0x9145;
    pub const DEBUG_LOGGED_MESSAGES_KHR: GLenum = 0x9145;
    pub const DEBUG_NEXT_LOGGED_MESSAGE_LENGTH: GLenum = 0x8243;
    pub const DEBUG_NEXT_LOGGED_MESSAGE_LENGTH_KHR: GLenum = 0x8243;
    pub const DEBUG_OUTPUT: GLenum = 0x92E0;
    pub const DEBUG_OUTPUT_KHR: GLenum = 0x92E0;
    pub const DEBUG_OUTPUT_SYNCHRONOUS: GLenum = 0x8242;
    pub const DEBUG_OUTPUT_SYNCHRONOUS_KHR: GLenum = 0x8242;
    pub const DEBUG_SEVERITY_HIGH: GLenum = 0x9146;
    pub const DEBUG_SEVERITY_HIGH_KHR: GLenum = 0x9146;
    pub const DEBUG_SEVERITY_LOW: GLenum = 0x9148;
    pub const DEBUG_SEVERITY_LOW_KHR: GLenum = 0x9148;
    pub const DEBUG_SEVERITY_MEDIUM: GLenum = 0x9147;
    pub const DEBUG_SEVERITY_MEDIUM_KHR: GLenum = 0x9147;
    pub const DEBUG_SEVERITY_NOTIFICATION: GLenum = 0x826B;
    pub const DEBUG_SEVERITY_NOTIFICATION_KHR: GLenum = 0x826B;
    pub const DEBUG_SOURCE_API: GLenum = 0x8246;
    pub const DEBUG_SOURCE_API_KHR: GLenum = 0x8246;
    pub const DEBUG_SOURCE_APPLICATION: GLenum = 0x824A;
    pub const DEBUG_SOURCE_APPLICATION_KHR: GLenum = 0x824A;
    pub const DEBUG_SOURCE_OTHER: GLenum = 0x824B;
    pub const DEBUG_SOURCE_OTHER_KHR: GLenum = 0x824B;
    pub const DEBUG_SOURCE_SHADER_COMPILER: GLenum = 0x8248;
    pub const DEBUG_SOURCE_SHADER_COMPILER_KHR: GLenum = 0x8248;
    pub const DEBUG_SOURCE_THIRD_PARTY: GLenum = 0x8249;
    pub const DEBUG_SOURCE_THIRD_PARTY_KHR: GLenum = 0x8249;
    pub const DEBUG_SOURCE_WINDOW_SYSTEM: GLenum = 0x8247;
    pub const DEBUG_SOURCE_WINDOW_SYSTEM_KHR: GLenum = 0x8247;
    pub const DEBUG_TYPE_DEPRECATED_BEHAVIOR: GLenum = 0x824D;
    pub const DEBUG_TYPE_DEPRECATED_BEHAVIOR_KHR: GLenum = 0x824D;
    pub const DEBUG_TYPE_ERROR: GLenum = 0x824C;
    pub const DEBUG_TYPE_ERROR_KHR: GLenum = 0x824C;
    pub const DEBUG_TYPE_MARKER: GLenum = 0x8268;
    pub const DEBUG_TYPE_MARKER_KHR: GLenum = 0x8268;
    pub const DEBUG_TYPE_OTHER: GLenum = 0x8251;
    pub const DEBUG_TYPE_OTHER_KHR: GLenum = 0x8251;
    pub const DEBUG_TYPE_PERFORMANCE: GLenum = 0x8250;
    pub const DEBUG_TYPE_PERFORMANCE_KHR: GLenum = 0x8250;
    pub const DEBUG_TYPE_POP_GROUP: GLenum = 0x826A;
    pub const DEBUG_TYPE_POP_GROUP_KHR: GLenum = 0x826A;
    pub const DEBUG_TYPE_PORTABILITY: GLenum = 0x824F;
    pub const DEBUG_TYPE_PORTABILITY_KHR: GLenum = 0x824F;
    pub const DEBUG_TYPE_PUSH_GROUP: GLenum = 0x8269;
    pub const DEBUG_TYPE_PUSH_GROUP_KHR: GLenum = 0x8269;
    pub const DEBUG_TYPE_UNDEFINED_BEHAVIOR: GLenum = 0x824E;
    pub const DEBUG_TYPE_UNDEFINED_BEHAVIOR_KHR: GLenum = 0x824E;
    pub const DECAL: GLenum = 0x2101;
    pub const DECR: GLenum = 0x1E03;
    pub const DECR_WRAP: GLenum = 0x8508;
    pub const DELETE_STATUS: GLenum = 0x8B80;
    pub const DEPTH: GLenum = 0x1801;
    pub const DEPTH24_STENCIL8: GLenum = 0x88F0;
    pub const DEPTH32F_STENCIL8: GLenum = 0x8CAD;
    pub const DEPTH_ATTACHMENT: GLenum = 0x8D00;
    pub const DEPTH_BIAS: GLenum = 0x0D1F;
    pub const DEPTH_BITS: GLenum = 0x0D56;
    pub const DEPTH_BUFFER_BIT: GLenum = 0x00000100;
    pub const DEPTH_CLAMP: GLenum = 0x864F;
    pub const DEPTH_CLEAR_VALUE: GLenum = 0x0B73;
    pub const DEPTH_COMPONENT: GLenum = 0x1902;
    pub const DEPTH_COMPONENT16: GLenum = 0x81A5;
    pub const DEPTH_COMPONENT24: GLenum = 0x81A6;
    pub const DEPTH_COMPONENT32: GLenum = 0x81A7;
    pub const DEPTH_COMPONENT32F: GLenum = 0x8CAC;
    pub const DEPTH_FUNC: GLenum = 0x0B74;
    pub const DEPTH_RANGE: GLenum = 0x0B70;
    pub const DEPTH_SCALE: GLenum = 0x0D1E;
    pub const DEPTH_STENCIL: GLenum = 0x84F9;
    pub const DEPTH_STENCIL_ATTACHMENT: GLenum = 0x821A;
    pub const DEPTH_TEST: GLenum = 0x0B71;
    pub const DEPTH_TEXTURE_MODE: GLenum = 0x884B;
    pub const DEPTH_WRITEMASK: GLenum = 0x0B72;
    pub const DIFFERENCE_KHR: GLenum = 0x929E;
    pub const DIFFUSE: GLenum = 0x1201;
    pub const DISPLAY_LIST: GLenum = 0x82E7;
    pub const DITHER: GLenum = 0x0BD0;
    pub const DOMAIN: GLenum = 0x0A02;
    pub const DONT_CARE: GLenum = 0x1100;
    pub const DOT3_RGB: GLenum = 0x86AE;
    pub const DOT3_RGBA: GLenum = 0x86AF;
    pub const DOUBLE: GLenum = 0x140A;
    pub const DOUBLEBUFFER: GLenum = 0x0C32;
    pub const DRAW_BUFFER: GLenum = 0x0C01;
    pub const DRAW_BUFFER0: GLenum = 0x8825;
    pub const DRAW_BUFFER1: GLenum = 0x8826;
    pub const DRAW_BUFFER10: GLenum = 0x882F;
    pub const DRAW_BUFFER11: GLenum = 0x8830;
    pub const DRAW_BUFFER12: GLenum = 0x8831;
    pub const DRAW_BUFFER13: GLenum = 0x8832;
    pub const DRAW_BUFFER14: GLenum = 0x8833;
    pub const DRAW_BUFFER15: GLenum = 0x8834;
    pub const DRAW_BUFFER2: GLenum = 0x8827;
    pub const DRAW_BUFFER3: GLenum = 0x8828;
    pub const DRAW_BUFFER4: GLenum = 0x8829;
    pub const DRAW_BUFFER5: GLenum = 0x882A;
    pub const DRAW_BUFFER6: GLenum = 0x882B;
    pub const DRAW_BUFFER7: GLenum = 0x882C;
    pub const DRAW_BUFFER8: GLenum = 0x882D;
    pub const DRAW_BUFFER9: GLenum = 0x882E;
    pub const DRAW_FRAMEBUFFER: GLenum = 0x8CA9;
    pub const DRAW_FRAMEBUFFER_BINDING: GLenum = 0x8CA6;
    pub const DRAW_PIXELS_APPLE: GLenum = 0x8A0A;
    pub const DRAW_PIXEL_TOKEN: GLenum = 0x0705;
    pub const DST_ALPHA: GLenum = 0x0304;
    pub const DST_COLOR: GLenum = 0x0306;
    pub const DYNAMIC_COPY: GLenum = 0x88EA;
    pub const DYNAMIC_DRAW: GLenum = 0x88E8;
    pub const DYNAMIC_READ: GLenum = 0x88E9;
    pub const EDGE_FLAG: GLenum = 0x0B43;
    pub const EDGE_FLAG_ARRAY: GLenum = 0x8079;
    pub const EDGE_FLAG_ARRAY_BUFFER_BINDING: GLenum = 0x889B;
    pub const EDGE_FLAG_ARRAY_POINTER: GLenum = 0x8093;
    pub const EDGE_FLAG_ARRAY_STRIDE: GLenum = 0x808C;
    pub const ELEMENT_ARRAY_BUFFER: GLenum = 0x8893;
    pub const ELEMENT_ARRAY_BUFFER_BINDING: GLenum = 0x8895;
    pub const EMISSION: GLenum = 0x1600;
    pub const ENABLE_BIT: GLenum = 0x00002000;
    pub const EQUAL: GLenum = 0x0202;
    pub const EQUIV: GLenum = 0x1509;
    pub const EVAL_BIT: GLenum = 0x00010000;
    pub const EXCLUSION_KHR: GLenum = 0x92A0;
    pub const EXP: GLenum = 0x0800;
    pub const EXP2: GLenum = 0x0801;
    pub const EXTENSIONS: GLenum = 0x1F03;
    pub const EYE_LINEAR: GLenum = 0x2400;
    pub const EYE_PLANE: GLenum = 0x2502;
    pub const FALSE: GLenum = 0;
    pub const FASTEST: GLenum = 0x1101;
    pub const FEEDBACK: GLenum = 0x1C01;
    pub const FEEDBACK_BUFFER_POINTER: GLenum = 0x0DF0;
    pub const FEEDBACK_BUFFER_SIZE: GLenum = 0x0DF1;
    pub const FEEDBACK_BUFFER_TYPE: GLenum = 0x0DF2;
    pub const FENCE_APPLE: GLenum = 0x8A0B;
    pub const FILL: GLenum = 0x1B02;
    pub const FIRST_VERTEX_CONVENTION: GLenum = 0x8E4D;
    pub const FIXED: GLenum = 0x140C;
    pub const FIXED_ONLY: GLenum = 0x891D;
    pub const FLAT: GLenum = 0x1D00;
    pub const FLOAT: GLenum = 0x1406;
    pub const FLOAT_32_UNSIGNED_INT_24_8_REV: GLenum = 0x8DAD;
    pub const FLOAT_MAT2: GLenum = 0x8B5A;
    pub const FLOAT_MAT2x3: GLenum = 0x8B65;
    pub const FLOAT_MAT2x4: GLenum = 0x8B66;
    pub const FLOAT_MAT3: GLenum = 0x8B5B;
    pub const FLOAT_MAT3x2: GLenum = 0x8B67;
    pub const FLOAT_MAT3x4: GLenum = 0x8B68;
    pub const FLOAT_MAT4: GLenum = 0x8B5C;
    pub const FLOAT_MAT4x2: GLenum = 0x8B69;
    pub const FLOAT_MAT4x3: GLenum = 0x8B6A;
    pub const FLOAT_VEC2: GLenum = 0x8B50;
    pub const FLOAT_VEC3: GLenum = 0x8B51;
    pub const FLOAT_VEC4: GLenum = 0x8B52;
    pub const FOG: GLenum = 0x0B60;
    pub const FOG_BIT: GLenum = 0x00000080;
    pub const FOG_COLOR: GLenum = 0x0B66;
    pub const FOG_COORD: GLenum = 0x8451;
    pub const FOG_COORDINATE: GLenum = 0x8451;
    pub const FOG_COORDINATE_ARRAY: GLenum = 0x8457;
    pub const FOG_COORDINATE_ARRAY_BUFFER_BINDING: GLenum = 0x889D;
    pub const FOG_COORDINATE_ARRAY_POINTER: GLenum = 0x8456;
    pub const FOG_COORDINATE_ARRAY_STRIDE: GLenum = 0x8455;
    pub const FOG_COORDINATE_ARRAY_TYPE: GLenum = 0x8454;
    pub const FOG_COORDINATE_SOURCE: GLenum = 0x8450;
    pub const FOG_COORD_ARRAY: GLenum = 0x8457;
    pub const FOG_COORD_ARRAY_BUFFER_BINDING: GLenum = 0x889D;
    pub const FOG_COORD_ARRAY_POINTER: GLenum = 0x8456;
    pub const FOG_COORD_ARRAY_STRIDE: GLenum = 0x8455;
    pub const FOG_COORD_ARRAY_TYPE: GLenum = 0x8454;
    pub const FOG_COORD_SRC: GLenum = 0x8450;
    pub const FOG_DENSITY: GLenum = 0x0B62;
    pub const FOG_END: GLenum = 0x0B64;
    pub const FOG_HINT: GLenum = 0x0C54;
    pub const FOG_INDEX: GLenum = 0x0B61;
    pub const FOG_MODE: GLenum = 0x0B65;
    pub const FOG_START: GLenum = 0x0B63;
    pub const FRAGMENT_DEPTH: GLenum = 0x8452;
    pub const FRAGMENT_SHADER: GLenum = 0x8B30;
    pub const FRAGMENT_SHADER_DERIVATIVE_HINT: GLenum = 0x8B8B;
    pub const FRAMEBUFFER: GLenum = 0x8D40;
    pub const FRAMEBUFFER_ATTACHMENT_ALPHA_SIZE: GLenum = 0x8215;
    pub const FRAMEBUFFER_ATTACHMENT_ANGLE: GLenum = 0x93A3;
    pub const FRAMEBUFFER_ATTACHMENT_BLUE_SIZE: GLenum = 0x8214;
    pub const FRAMEBUFFER_ATTACHMENT_COLOR_ENCODING: GLenum = 0x8210;
    pub const FRAMEBUFFER_ATTACHMENT_COMPONENT_TYPE: GLenum = 0x8211;
    pub const FRAMEBUFFER_ATTACHMENT_DEPTH_SIZE: GLenum = 0x8216;
    pub const FRAMEBUFFER_ATTACHMENT_GREEN_SIZE: GLenum = 0x8213;
    pub const FRAMEBUFFER_ATTACHMENT_LAYERED: GLenum = 0x8DA7;
    pub const FRAMEBUFFER_ATTACHMENT_OBJECT_NAME: GLenum = 0x8CD1;
    pub const FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE: GLenum = 0x8CD0;
    pub const FRAMEBUFFER_ATTACHMENT_RED_SIZE: GLenum = 0x8212;
    pub const FRAMEBUFFER_ATTACHMENT_STENCIL_SIZE: GLenum = 0x8217;
    pub const FRAMEBUFFER_ATTACHMENT_TEXTURE_CUBE_MAP_FACE: GLenum = 0x8CD3;
    pub const FRAMEBUFFER_ATTACHMENT_TEXTURE_LAYER: GLenum = 0x8CD4;
    pub const FRAMEBUFFER_ATTACHMENT_TEXTURE_LEVEL: GLenum = 0x8CD2;
    pub const FRAMEBUFFER_BINDING: GLenum = 0x8CA6;
    pub const FRAMEBUFFER_COMPLETE: GLenum = 0x8CD5;
    pub const FRAMEBUFFER_DEFAULT: GLenum = 0x8218;
    pub const FRAMEBUFFER_INCOMPLETE_ATTACHMENT: GLenum = 0x8CD6;
    pub const FRAMEBUFFER_INCOMPLETE_DIMENSIONS: GLenum = 0x8CD9;
    pub const FRAMEBUFFER_INCOMPLETE_DRAW_BUFFER: GLenum = 0x8CDB;
    pub const FRAMEBUFFER_INCOMPLETE_LAYER_TARGETS: GLenum = 0x8DA8;
    pub const FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT: GLenum = 0x8CD7;
    pub const FRAMEBUFFER_INCOMPLETE_MULTISAMPLE: GLenum = 0x8D56;
    pub const FRAMEBUFFER_INCOMPLETE_READ_BUFFER: GLenum = 0x8CDC;
    pub const FRAMEBUFFER_SRGB: GLenum = 0x8DB9;
    pub const FRAMEBUFFER_UNDEFINED: GLenum = 0x8219;
    pub const FRAMEBUFFER_UNSUPPORTED: GLenum = 0x8CDD;
    pub const FRONT: GLenum = 0x0404;
    pub const FRONT_AND_BACK: GLenum = 0x0408;
    pub const FRONT_FACE: GLenum = 0x0B46;
    pub const FRONT_LEFT: GLenum = 0x0400;
    pub const FRONT_RIGHT: GLenum = 0x0401;
    pub const FUNC_ADD: GLenum = 0x8006;
    pub const FUNC_REVERSE_SUBTRACT: GLenum = 0x800B;
    pub const FUNC_SUBTRACT: GLenum = 0x800A;
    pub const GENERATE_MIPMAP: GLenum = 0x8191;
    pub const GENERATE_MIPMAP_HINT: GLenum = 0x8192;
    pub const GEOMETRY_INPUT_TYPE: GLenum = 0x8917;
    pub const GEOMETRY_OUTPUT_TYPE: GLenum = 0x8918;
    pub const GEOMETRY_SHADER: GLenum = 0x8DD9;
    pub const GEOMETRY_VERTICES_OUT: GLenum = 0x8916;
    pub const GEQUAL: GLenum = 0x0206;
    pub const GPU_DISJOINT_EXT: GLenum = 0x8FBB;
    pub const GREATER: GLenum = 0x0204;
    pub const GREEN: GLenum = 0x1904;
    pub const GREEN_BIAS: GLenum = 0x0D19;
    pub const GREEN_BITS: GLenum = 0x0D53;
    pub const GREEN_INTEGER: GLenum = 0x8D95;
    pub const GREEN_SCALE: GLenum = 0x0D18;
    pub const HALF_FLOAT: GLenum = 0x140B;
    pub const HALF_FLOAT_OES: GLenum = 0x8D61;
    pub const HARDLIGHT_KHR: GLenum = 0x929B;
    pub const HIGH_FLOAT: GLenum = 0x8DF2;
    pub const HIGH_INT: GLenum = 0x8DF5;
    pub const HINT_BIT: GLenum = 0x00008000;
    pub const HSL_COLOR_KHR: GLenum = 0x92AF;
    pub const HSL_HUE_KHR: GLenum = 0x92AD;
    pub const HSL_LUMINOSITY_KHR: GLenum = 0x92B0;
    pub const HSL_SATURATION_KHR: GLenum = 0x92AE;
    pub const IMPLEMENTATION_COLOR_READ_FORMAT: GLenum = 0x8B9B;
    pub const IMPLEMENTATION_COLOR_READ_TYPE: GLenum = 0x8B9A;
    pub const INCR: GLenum = 0x1E02;
    pub const INCR_WRAP: GLenum = 0x8507;
    pub const INDEX: GLenum = 0x8222;
    pub const INDEX_ARRAY: GLenum = 0x8077;
    pub const INDEX_ARRAY_BUFFER_BINDING: GLenum = 0x8899;
    pub const INDEX_ARRAY_POINTER: GLenum = 0x8091;
    pub const INDEX_ARRAY_STRIDE: GLenum = 0x8086;
    pub const INDEX_ARRAY_TYPE: GLenum = 0x8085;
    pub const INDEX_BITS: GLenum = 0x0D51;
    pub const INDEX_CLEAR_VALUE: GLenum = 0x0C20;
    pub const INDEX_LOGIC_OP: GLenum = 0x0BF1;
    pub const INDEX_MODE: GLenum = 0x0C30;
    pub const INDEX_OFFSET: GLenum = 0x0D13;
    pub const INDEX_SHIFT: GLenum = 0x0D12;
    pub const INDEX_WRITEMASK: GLenum = 0x0C21;
    pub const INFO_LOG_LENGTH: GLenum = 0x8B84;
    pub const INT: GLenum = 0x1404;
    pub const INTENSITY: GLenum = 0x8049;
    pub const INTENSITY12: GLenum = 0x804C;
    pub const INTENSITY16: GLenum = 0x804D;
    pub const INTENSITY4: GLenum = 0x804A;
    pub const INTENSITY8: GLenum = 0x804B;
    pub const INTERLEAVED_ATTRIBS: GLenum = 0x8C8C;
    pub const INTERPOLATE: GLenum = 0x8575;
    pub const INT_2_10_10_10_REV: GLenum = 0x8D9F;
    pub const INT_SAMPLER_1D: GLenum = 0x8DC9;
    pub const INT_SAMPLER_1D_ARRAY: GLenum = 0x8DCE;
    pub const INT_SAMPLER_2D: GLenum = 0x8DCA;
    pub const INT_SAMPLER_2D_ARRAY: GLenum = 0x8DCF;
    pub const INT_SAMPLER_2D_MULTISAMPLE: GLenum = 0x9109;
    pub const INT_SAMPLER_2D_MULTISAMPLE_ARRAY: GLenum = 0x910C;
    pub const INT_SAMPLER_2D_RECT: GLenum = 0x8DCD;
    pub const INT_SAMPLER_3D: GLenum = 0x8DCB;
    pub const INT_SAMPLER_BUFFER: GLenum = 0x8DD0;
    pub const INT_SAMPLER_CUBE: GLenum = 0x8DCC;
    pub const INT_VEC2: GLenum = 0x8B53;
    pub const INT_VEC3: GLenum = 0x8B54;
    pub const INT_VEC4: GLenum = 0x8B55;
    pub const INVALID_ENUM: GLenum = 0x0500;
    pub const INVALID_FRAMEBUFFER_OPERATION: GLenum = 0x0506;
    pub const INVALID_INDEX: GLenum = 0xFFFFFFFF;
    pub const INVALID_OPERATION: GLenum = 0x0502;
    pub const INVALID_VALUE: GLenum = 0x0501;
    pub const INVERT: GLenum = 0x150A;
    pub const KEEP: GLenum = 0x1E00;
    pub const LAST_VERTEX_CONVENTION: GLenum = 0x8E4E;
    pub const LEFT: GLenum = 0x0406;
    pub const LEQUAL: GLenum = 0x0203;
    pub const LESS: GLenum = 0x0201;
    pub const LIGHT0: GLenum = 0x4000;
    pub const LIGHT1: GLenum = 0x4001;
    pub const LIGHT2: GLenum = 0x4002;
    pub const LIGHT3: GLenum = 0x4003;
    pub const LIGHT4: GLenum = 0x4004;
    pub const LIGHT5: GLenum = 0x4005;
    pub const LIGHT6: GLenum = 0x4006;
    pub const LIGHT7: GLenum = 0x4007;
    pub const LIGHTEN_KHR: GLenum = 0x9298;
    pub const LIGHTING: GLenum = 0x0B50;
    pub const LIGHTING_BIT: GLenum = 0x00000040;
    pub const LIGHT_MODEL_AMBIENT: GLenum = 0x0B53;
    pub const LIGHT_MODEL_COLOR_CONTROL: GLenum = 0x81F8;
    pub const LIGHT_MODEL_LOCAL_VIEWER: GLenum = 0x0B51;
    pub const LIGHT_MODEL_TWO_SIDE: GLenum = 0x0B52;
    pub const LINE: GLenum = 0x1B01;
    pub const LINEAR: GLenum = 0x2601;
    pub const LINEAR_ATTENUATION: GLenum = 0x1208;
    pub const LINEAR_MIPMAP_LINEAR: GLenum = 0x2703;
    pub const LINEAR_MIPMAP_NEAREST: GLenum = 0x2701;
    pub const LINES: GLenum = 0x0001;
    pub const LINES_ADJACENCY: GLenum = 0x000A;
    pub const LINE_BIT: GLenum = 0x00000004;
    pub const LINE_LOOP: GLenum = 0x0002;
    pub const LINE_RESET_TOKEN: GLenum = 0x0707;
    pub const LINE_SMOOTH: GLenum = 0x0B20;
    pub const LINE_SMOOTH_HINT: GLenum = 0x0C52;
    pub const LINE_STIPPLE: GLenum = 0x0B24;
    pub const LINE_STIPPLE_PATTERN: GLenum = 0x0B25;
    pub const LINE_STIPPLE_REPEAT: GLenum = 0x0B26;
    pub const LINE_STRIP: GLenum = 0x0003;
    pub const LINE_STRIP_ADJACENCY: GLenum = 0x000B;
    pub const LINE_TOKEN: GLenum = 0x0702;
    pub const LINE_WIDTH: GLenum = 0x0B21;
    pub const LINE_WIDTH_GRANULARITY: GLenum = 0x0B23;
    pub const LINE_WIDTH_RANGE: GLenum = 0x0B22;
    pub const LINK_STATUS: GLenum = 0x8B82;
    pub const LIST_BASE: GLenum = 0x0B32;
    pub const LIST_BIT: GLenum = 0x00020000;
    pub const LIST_INDEX: GLenum = 0x0B33;
    pub const LIST_MODE: GLenum = 0x0B30;
    pub const LOAD: GLenum = 0x0101;
    pub const LOGIC_OP: GLenum = 0x0BF1;
    pub const LOGIC_OP_MODE: GLenum = 0x0BF0;
    pub const LOWER_LEFT: GLenum = 0x8CA1;
    pub const LOW_FLOAT: GLenum = 0x8DF0;
    pub const LOW_INT: GLenum = 0x8DF3;
    pub const LUMINANCE: GLenum = 0x1909;
    pub const LUMINANCE12: GLenum = 0x8041;
    pub const LUMINANCE12_ALPHA12: GLenum = 0x8047;
    pub const LUMINANCE12_ALPHA4: GLenum = 0x8046;
    pub const LUMINANCE16: GLenum = 0x8042;
    pub const LUMINANCE16F_EXT: GLenum = 0x881E;
    pub const LUMINANCE16_ALPHA16: GLenum = 0x8048;
    pub const LUMINANCE32F_EXT: GLenum = 0x8818;
    pub const LUMINANCE4: GLenum = 0x803F;
    pub const LUMINANCE4_ALPHA4: GLenum = 0x8043;
    pub const LUMINANCE6_ALPHA2: GLenum = 0x8044;
    pub const LUMINANCE8: GLenum = 0x8040;
    pub const LUMINANCE8_ALPHA8: GLenum = 0x8045;
    pub const LUMINANCE8_ALPHA8_EXT: GLenum = 0x8045;
    pub const LUMINANCE8_EXT: GLenum = 0x8040;
    pub const LUMINANCE_ALPHA: GLenum = 0x190A;
    pub const LUMINANCE_ALPHA16F_EXT: GLenum = 0x881F;
    pub const LUMINANCE_ALPHA32F_EXT: GLenum = 0x8819;
    pub const MAJOR_VERSION: GLenum = 0x821B;
    pub const MAP1_COLOR_4: GLenum = 0x0D90;
    pub const MAP1_GRID_DOMAIN: GLenum = 0x0DD0;
    pub const MAP1_GRID_SEGMENTS: GLenum = 0x0DD1;
    pub const MAP1_INDEX: GLenum = 0x0D91;
    pub const MAP1_NORMAL: GLenum = 0x0D92;
    pub const MAP1_TEXTURE_COORD_1: GLenum = 0x0D93;
    pub const MAP1_TEXTURE_COORD_2: GLenum = 0x0D94;
    pub const MAP1_TEXTURE_COORD_3: GLenum = 0x0D95;
    pub const MAP1_TEXTURE_COORD_4: GLenum = 0x0D96;
    pub const MAP1_VERTEX_3: GLenum = 0x0D97;
    pub const MAP1_VERTEX_4: GLenum = 0x0D98;
    pub const MAP2_COLOR_4: GLenum = 0x0DB0;
    pub const MAP2_GRID_DOMAIN: GLenum = 0x0DD2;
    pub const MAP2_GRID_SEGMENTS: GLenum = 0x0DD3;
    pub const MAP2_INDEX: GLenum = 0x0DB1;
    pub const MAP2_NORMAL: GLenum = 0x0DB2;
    pub const MAP2_TEXTURE_COORD_1: GLenum = 0x0DB3;
    pub const MAP2_TEXTURE_COORD_2: GLenum = 0x0DB4;
    pub const MAP2_TEXTURE_COORD_3: GLenum = 0x0DB5;
    pub const MAP2_TEXTURE_COORD_4: GLenum = 0x0DB6;
    pub const MAP2_VERTEX_3: GLenum = 0x0DB7;
    pub const MAP2_VERTEX_4: GLenum = 0x0DB8;
    pub const MAP_COLOR: GLenum = 0x0D10;
    pub const MAP_FLUSH_EXPLICIT_BIT: GLenum = 0x0010;
    pub const MAP_INVALIDATE_BUFFER_BIT: GLenum = 0x0008;
    pub const MAP_INVALIDATE_RANGE_BIT: GLenum = 0x0004;
    pub const MAP_READ_BIT: GLenum = 0x0001;
    pub const MAP_STENCIL: GLenum = 0x0D11;
    pub const MAP_UNSYNCHRONIZED_BIT: GLenum = 0x0020;
    pub const MAP_WRITE_BIT: GLenum = 0x0002;
    pub const MATRIX_MODE: GLenum = 0x0BA0;
    pub const MAX: GLenum = 0x8008;
    pub const MAX_3D_TEXTURE_SIZE: GLenum = 0x8073;
    pub const MAX_ARRAY_TEXTURE_LAYERS: GLenum = 0x88FF;
    pub const MAX_ATTRIB_STACK_DEPTH: GLenum = 0x0D35;
    pub const MAX_CLIENT_ATTRIB_STACK_DEPTH: GLenum = 0x0D3B;
    pub const MAX_CLIP_DISTANCES: GLenum = 0x0D32;
    pub const MAX_CLIP_PLANES: GLenum = 0x0D32;
    pub const MAX_COLOR_ATTACHMENTS: GLenum = 0x8CDF;
    pub const MAX_COLOR_TEXTURE_SAMPLES: GLenum = 0x910E;
    pub const MAX_COMBINED_FRAGMENT_UNIFORM_COMPONENTS: GLenum = 0x8A33;
    pub const MAX_COMBINED_GEOMETRY_UNIFORM_COMPONENTS: GLenum = 0x8A32;
    pub const MAX_COMBINED_TEXTURE_IMAGE_UNITS: GLenum = 0x8B4D;
    pub const MAX_COMBINED_UNIFORM_BLOCKS: GLenum = 0x8A2E;
    pub const MAX_COMBINED_VERTEX_UNIFORM_COMPONENTS: GLenum = 0x8A31;
    pub const MAX_CUBE_MAP_TEXTURE_SIZE: GLenum = 0x851C;
    pub const MAX_DEBUG_GROUP_STACK_DEPTH: GLenum = 0x826C;
    pub const MAX_DEBUG_GROUP_STACK_DEPTH_KHR: GLenum = 0x826C;
    pub const MAX_DEBUG_LOGGED_MESSAGES: GLenum = 0x9144;
    pub const MAX_DEBUG_LOGGED_MESSAGES_KHR: GLenum = 0x9144;
    pub const MAX_DEBUG_MESSAGE_LENGTH: GLenum = 0x9143;
    pub const MAX_DEBUG_MESSAGE_LENGTH_KHR: GLenum = 0x9143;
    pub const MAX_DEPTH_TEXTURE_SAMPLES: GLenum = 0x910F;
    pub const MAX_DRAW_BUFFERS: GLenum = 0x8824;
    pub const MAX_DUAL_SOURCE_DRAW_BUFFERS: GLenum = 0x88FC;
    pub const MAX_ELEMENTS_INDICES: GLenum = 0x80E9;
    pub const MAX_ELEMENTS_VERTICES: GLenum = 0x80E8;
    pub const MAX_ELEMENT_INDEX: GLenum = 0x8D6B;
    pub const MAX_EVAL_ORDER: GLenum = 0x0D30;
    pub const MAX_FRAGMENT_INPUT_COMPONENTS: GLenum = 0x9125;
    pub const MAX_FRAGMENT_UNIFORM_BLOCKS: GLenum = 0x8A2D;
    pub const MAX_FRAGMENT_UNIFORM_COMPONENTS: GLenum = 0x8B49;
    pub const MAX_FRAGMENT_UNIFORM_VECTORS: GLenum = 0x8DFD;
    pub const MAX_GEOMETRY_INPUT_COMPONENTS: GLenum = 0x9123;
    pub const MAX_GEOMETRY_OUTPUT_COMPONENTS: GLenum = 0x9124;
    pub const MAX_GEOMETRY_OUTPUT_VERTICES: GLenum = 0x8DE0;
    pub const MAX_GEOMETRY_TEXTURE_IMAGE_UNITS: GLenum = 0x8C29;
    pub const MAX_GEOMETRY_TOTAL_OUTPUT_COMPONENTS: GLenum = 0x8DE1;
    pub const MAX_GEOMETRY_UNIFORM_BLOCKS: GLenum = 0x8A2C;
    pub const MAX_GEOMETRY_UNIFORM_COMPONENTS: GLenum = 0x8DDF;
    pub const MAX_INTEGER_SAMPLES: GLenum = 0x9110;
    pub const MAX_LABEL_LENGTH: GLenum = 0x82E8;
    pub const MAX_LABEL_LENGTH_KHR: GLenum = 0x82E8;
    pub const MAX_LIGHTS: GLenum = 0x0D31;
    pub const MAX_LIST_NESTING: GLenum = 0x0B31;
    pub const MAX_MODELVIEW_STACK_DEPTH: GLenum = 0x0D36;
    pub const MAX_NAME_STACK_DEPTH: GLenum = 0x0D37;
    pub const MAX_PIXEL_MAP_TABLE: GLenum = 0x0D34;
    pub const MAX_PROGRAM_TEXEL_OFFSET: GLenum = 0x8905;
    pub const MAX_PROJECTION_STACK_DEPTH: GLenum = 0x0D38;
    pub const MAX_RECTANGLE_TEXTURE_SIZE: GLenum = 0x84F8;
    pub const MAX_RECTANGLE_TEXTURE_SIZE_ARB: GLenum = 0x84F8;
    pub const MAX_RENDERBUFFER_SIZE: GLenum = 0x84E8;
    pub const MAX_SAMPLES: GLenum = 0x8D57;
    pub const MAX_SAMPLE_MASK_WORDS: GLenum = 0x8E59;
    pub const MAX_SERVER_WAIT_TIMEOUT: GLenum = 0x9111;
    pub const MAX_SHADER_PIXEL_LOCAL_STORAGE_FAST_SIZE_EXT: GLenum = 0x8F63;
    pub const MAX_SHADER_PIXEL_LOCAL_STORAGE_SIZE_EXT: GLenum = 0x8F67;
    pub const MAX_TEXTURE_BUFFER_SIZE: GLenum = 0x8C2B;
    pub const MAX_TEXTURE_COORDS: GLenum = 0x8871;
    pub const MAX_TEXTURE_IMAGE_UNITS: GLenum = 0x8872;
    pub const MAX_TEXTURE_LOD_BIAS: GLenum = 0x84FD;
    pub const MAX_TEXTURE_MAX_ANISOTROPY_EXT: GLenum = 0x84FF;
    pub const MAX_TEXTURE_SIZE: GLenum = 0x0D33;
    pub const MAX_TEXTURE_STACK_DEPTH: GLenum = 0x0D39;
    pub const MAX_TEXTURE_UNITS: GLenum = 0x84E2;
    pub const MAX_TRANSFORM_FEEDBACK_INTERLEAVED_COMPONENTS: GLenum = 0x8C8A;
    pub const MAX_TRANSFORM_FEEDBACK_SEPARATE_ATTRIBS: GLenum = 0x8C8B;
    pub const MAX_TRANSFORM_FEEDBACK_SEPARATE_COMPONENTS: GLenum = 0x8C80;
    pub const MAX_UNIFORM_BLOCK_SIZE: GLenum = 0x8A30;
    pub const MAX_UNIFORM_BUFFER_BINDINGS: GLenum = 0x8A2F;
    pub const MAX_VARYING_COMPONENTS: GLenum = 0x8B4B;
    pub const MAX_VARYING_FLOATS: GLenum = 0x8B4B;
    pub const MAX_VARYING_VECTORS: GLenum = 0x8DFC;
    pub const MAX_VERTEX_ATTRIBS: GLenum = 0x8869;
    pub const MAX_VERTEX_OUTPUT_COMPONENTS: GLenum = 0x9122;
    pub const MAX_VERTEX_TEXTURE_IMAGE_UNITS: GLenum = 0x8B4C;
    pub const MAX_VERTEX_UNIFORM_BLOCKS: GLenum = 0x8A2B;
    pub const MAX_VERTEX_UNIFORM_COMPONENTS: GLenum = 0x8B4A;
    pub const MAX_VERTEX_UNIFORM_VECTORS: GLenum = 0x8DFB;
    pub const MAX_VIEWPORT_DIMS: GLenum = 0x0D3A;
    pub const MEDIUM_FLOAT: GLenum = 0x8DF1;
    pub const MEDIUM_INT: GLenum = 0x8DF4;
    pub const MIN: GLenum = 0x8007;
    pub const MINOR_VERSION: GLenum = 0x821C;
    pub const MIN_PROGRAM_TEXEL_OFFSET: GLenum = 0x8904;
    pub const MIRRORED_REPEAT: GLenum = 0x8370;
    pub const MODELVIEW: GLenum = 0x1700;
    pub const MODELVIEW_MATRIX: GLenum = 0x0BA6;
    pub const MODELVIEW_STACK_DEPTH: GLenum = 0x0BA3;
    pub const MODULATE: GLenum = 0x2100;
    pub const MULT: GLenum = 0x0103;
    pub const MULTIPLY_KHR: GLenum = 0x9294;
    pub const MULTISAMPLE: GLenum = 0x809D;
    pub const MULTISAMPLE_BIT: GLenum = 0x20000000;
    pub const N3F_V3F: GLenum = 0x2A25;
    pub const NAME_STACK_DEPTH: GLenum = 0x0D70;
    pub const NAND: GLenum = 0x150E;
    pub const NEAREST: GLenum = 0x2600;
    pub const NEAREST_MIPMAP_LINEAR: GLenum = 0x2702;
    pub const NEAREST_MIPMAP_NEAREST: GLenum = 0x2700;
    pub const NEVER: GLenum = 0x0200;
    pub const NICEST: GLenum = 0x1102;
    pub const NONE: GLenum = 0;
    pub const NOOP: GLenum = 0x1505;
    pub const NOR: GLenum = 0x1508;
    pub const NORMALIZE: GLenum = 0x0BA1;
    pub const NORMAL_ARRAY: GLenum = 0x8075;
    pub const NORMAL_ARRAY_BUFFER_BINDING: GLenum = 0x8897;
    pub const NORMAL_ARRAY_POINTER: GLenum = 0x808F;
    pub const NORMAL_ARRAY_STRIDE: GLenum = 0x807F;
    pub const NORMAL_ARRAY_TYPE: GLenum = 0x807E;
    pub const NORMAL_MAP: GLenum = 0x8511;
    pub const NOTEQUAL: GLenum = 0x0205;
    pub const NO_ERROR: GLenum = 0;
    pub const NUM_COMPRESSED_TEXTURE_FORMATS: GLenum = 0x86A2;
    pub const NUM_EXTENSIONS: GLenum = 0x821D;
    pub const NUM_PROGRAM_BINARY_FORMATS: GLenum = 0x87FE;
    pub const NUM_SAMPLE_COUNTS: GLenum = 0x9380;
    pub const NUM_SHADER_BINARY_FORMATS: GLenum = 0x8DF9;
    pub const OBJECT_LINEAR: GLenum = 0x2401;
    pub const OBJECT_PLANE: GLenum = 0x2501;
    pub const OBJECT_TYPE: GLenum = 0x9112;
    pub const ONE: GLenum = 1;
    pub const ONE_MINUS_CONSTANT_ALPHA: GLenum = 0x8004;
    pub const ONE_MINUS_CONSTANT_COLOR: GLenum = 0x8002;
    pub const ONE_MINUS_DST_ALPHA: GLenum = 0x0305;
    pub const ONE_MINUS_DST_COLOR: GLenum = 0x0307;
    pub const ONE_MINUS_SRC1_ALPHA: GLenum = 0x88FB;
    pub const ONE_MINUS_SRC1_COLOR: GLenum = 0x88FA;
    pub const ONE_MINUS_SRC_ALPHA: GLenum = 0x0303;
    pub const ONE_MINUS_SRC_COLOR: GLenum = 0x0301;
    pub const OPERAND0_ALPHA: GLenum = 0x8598;
    pub const OPERAND0_RGB: GLenum = 0x8590;
    pub const OPERAND1_ALPHA: GLenum = 0x8599;
    pub const OPERAND1_RGB: GLenum = 0x8591;
    pub const OPERAND2_ALPHA: GLenum = 0x859A;
    pub const OPERAND2_RGB: GLenum = 0x8592;
    pub const OR: GLenum = 0x1507;
    pub const ORDER: GLenum = 0x0A01;
    pub const OR_INVERTED: GLenum = 0x150D;
    pub const OR_REVERSE: GLenum = 0x150B;
    pub const OUT_OF_MEMORY: GLenum = 0x0505;
    pub const OVERLAY_KHR: GLenum = 0x9296;
    pub const PACK_ALIGNMENT: GLenum = 0x0D05;
    pub const PACK_IMAGE_HEIGHT: GLenum = 0x806C;
    pub const PACK_LSB_FIRST: GLenum = 0x0D01;
    pub const PACK_ROW_LENGTH: GLenum = 0x0D02;
    pub const PACK_SKIP_IMAGES: GLenum = 0x806B;
    pub const PACK_SKIP_PIXELS: GLenum = 0x0D04;
    pub const PACK_SKIP_ROWS: GLenum = 0x0D03;
    pub const PACK_SWAP_BYTES: GLenum = 0x0D00;
    pub const PASS_THROUGH_TOKEN: GLenum = 0x0700;
    pub const PERSPECTIVE_CORRECTION_HINT: GLenum = 0x0C50;
    pub const PIXEL_MAP_A_TO_A: GLenum = 0x0C79;
    pub const PIXEL_MAP_A_TO_A_SIZE: GLenum = 0x0CB9;
    pub const PIXEL_MAP_B_TO_B: GLenum = 0x0C78;
    pub const PIXEL_MAP_B_TO_B_SIZE: GLenum = 0x0CB8;
    pub const PIXEL_MAP_G_TO_G: GLenum = 0x0C77;
    pub const PIXEL_MAP_G_TO_G_SIZE: GLenum = 0x0CB7;
    pub const PIXEL_MAP_I_TO_A: GLenum = 0x0C75;
    pub const PIXEL_MAP_I_TO_A_SIZE: GLenum = 0x0CB5;
    pub const PIXEL_MAP_I_TO_B: GLenum = 0x0C74;
    pub const PIXEL_MAP_I_TO_B_SIZE: GLenum = 0x0CB4;
    pub const PIXEL_MAP_I_TO_G: GLenum = 0x0C73;
    pub const PIXEL_MAP_I_TO_G_SIZE: GLenum = 0x0CB3;
    pub const PIXEL_MAP_I_TO_I: GLenum = 0x0C70;
    pub const PIXEL_MAP_I_TO_I_SIZE: GLenum = 0x0CB0;
    pub const PIXEL_MAP_I_TO_R: GLenum = 0x0C72;
    pub const PIXEL_MAP_I_TO_R_SIZE: GLenum = 0x0CB2;
    pub const PIXEL_MAP_R_TO_R: GLenum = 0x0C76;
    pub const PIXEL_MAP_R_TO_R_SIZE: GLenum = 0x0CB6;
    pub const PIXEL_MAP_S_TO_S: GLenum = 0x0C71;
    pub const PIXEL_MAP_S_TO_S_SIZE: GLenum = 0x0CB1;
    pub const PIXEL_MODE_BIT: GLenum = 0x00000020;
    pub const PIXEL_PACK_BUFFER: GLenum = 0x88EB;
    pub const PIXEL_PACK_BUFFER_BINDING: GLenum = 0x88ED;
    pub const PIXEL_UNPACK_BUFFER: GLenum = 0x88EC;
    pub const PIXEL_UNPACK_BUFFER_BINDING: GLenum = 0x88EF;
    pub const POINT: GLenum = 0x1B00;
    pub const POINTS: GLenum = 0x0000;
    pub const POINT_BIT: GLenum = 0x00000002;
    pub const POINT_DISTANCE_ATTENUATION: GLenum = 0x8129;
    pub const POINT_FADE_THRESHOLD_SIZE: GLenum = 0x8128;
    pub const POINT_SIZE: GLenum = 0x0B11;
    pub const POINT_SIZE_GRANULARITY: GLenum = 0x0B13;
    pub const POINT_SIZE_MAX: GLenum = 0x8127;
    pub const POINT_SIZE_MIN: GLenum = 0x8126;
    pub const POINT_SIZE_RANGE: GLenum = 0x0B12;
    pub const POINT_SMOOTH: GLenum = 0x0B10;
    pub const POINT_SMOOTH_HINT: GLenum = 0x0C51;
    pub const POINT_SPRITE: GLenum = 0x8861;
    pub const POINT_SPRITE_COORD_ORIGIN: GLenum = 0x8CA0;
    pub const POINT_TOKEN: GLenum = 0x0701;
    pub const POLYGON: GLenum = 0x0009;
    pub const POLYGON_BIT: GLenum = 0x00000008;
    pub const POLYGON_MODE: GLenum = 0x0B40;
    pub const POLYGON_OFFSET_FACTOR: GLenum = 0x8038;
    pub const POLYGON_OFFSET_FILL: GLenum = 0x8037;
    pub const POLYGON_OFFSET_LINE: GLenum = 0x2A02;
    pub const POLYGON_OFFSET_POINT: GLenum = 0x2A01;
    pub const POLYGON_OFFSET_UNITS: GLenum = 0x2A00;
    pub const POLYGON_SMOOTH: GLenum = 0x0B41;
    pub const POLYGON_SMOOTH_HINT: GLenum = 0x0C53;
    pub const POLYGON_STIPPLE: GLenum = 0x0B42;
    pub const POLYGON_STIPPLE_BIT: GLenum = 0x00000010;
    pub const POLYGON_TOKEN: GLenum = 0x0703;
    pub const POSITION: GLenum = 0x1203;
    pub const PREVIOUS: GLenum = 0x8578;
    pub const PRIMARY_COLOR: GLenum = 0x8577;
    pub const PRIMITIVES_GENERATED: GLenum = 0x8C87;
    pub const PRIMITIVE_RESTART: GLenum = 0x8F9D;
    pub const PRIMITIVE_RESTART_FIXED_INDEX: GLenum = 0x8D69;
    pub const PRIMITIVE_RESTART_INDEX: GLenum = 0x8F9E;
    pub const PROGRAM: GLenum = 0x82E2;
    pub const PROGRAM_BINARY_FORMATS: GLenum = 0x87FF;
    pub const PROGRAM_BINARY_LENGTH: GLenum = 0x8741;
    pub const PROGRAM_BINARY_RETRIEVABLE_HINT: GLenum = 0x8257;
    pub const PROGRAM_KHR: GLenum = 0x82E2;
    pub const PROGRAM_PIPELINE: GLenum = 0x82E4;
    pub const PROGRAM_PIPELINE_KHR: GLenum = 0x82E4;
    pub const PROGRAM_POINT_SIZE: GLenum = 0x8642;
    pub const PROJECTION: GLenum = 0x1701;
    pub const PROJECTION_MATRIX: GLenum = 0x0BA7;
    pub const PROJECTION_STACK_DEPTH: GLenum = 0x0BA4;
    pub const PROVOKING_VERTEX: GLenum = 0x8E4F;
    pub const PROXY_TEXTURE_1D: GLenum = 0x8063;
    pub const PROXY_TEXTURE_1D_ARRAY: GLenum = 0x8C19;
    pub const PROXY_TEXTURE_2D: GLenum = 0x8064;
    pub const PROXY_TEXTURE_2D_ARRAY: GLenum = 0x8C1B;
    pub const PROXY_TEXTURE_2D_MULTISAMPLE: GLenum = 0x9101;
    pub const PROXY_TEXTURE_2D_MULTISAMPLE_ARRAY: GLenum = 0x9103;
    pub const PROXY_TEXTURE_3D: GLenum = 0x8070;
    pub const PROXY_TEXTURE_CUBE_MAP: GLenum = 0x851B;
    pub const PROXY_TEXTURE_RECTANGLE: GLenum = 0x84F7;
    pub const PROXY_TEXTURE_RECTANGLE_ARB: GLenum = 0x84F7;
    pub const Q: GLenum = 0x2003;
    pub const QUADRATIC_ATTENUATION: GLenum = 0x1209;
    pub const QUADS: GLenum = 0x0007;
    pub const QUADS_FOLLOW_PROVOKING_VERTEX_CONVENTION: GLenum = 0x8E4C;
    pub const QUAD_STRIP: GLenum = 0x0008;
    pub const QUERY: GLenum = 0x82E3;
    pub const QUERY_BY_REGION_NO_WAIT: GLenum = 0x8E16;
    pub const QUERY_BY_REGION_WAIT: GLenum = 0x8E15;
    pub const QUERY_COUNTER_BITS: GLenum = 0x8864;
    pub const QUERY_COUNTER_BITS_EXT: GLenum = 0x8864;
    pub const QUERY_KHR: GLenum = 0x82E3;
    pub const QUERY_NO_WAIT: GLenum = 0x8E14;
    pub const QUERY_RESULT: GLenum = 0x8866;
    pub const QUERY_RESULT_AVAILABLE: GLenum = 0x8867;
    pub const QUERY_RESULT_AVAILABLE_EXT: GLenum = 0x8867;
    pub const QUERY_RESULT_EXT: GLenum = 0x8866;
    pub const QUERY_WAIT: GLenum = 0x8E13;
    pub const R: GLenum = 0x2002;
    pub const R11F_G11F_B10F: GLenum = 0x8C3A;
    pub const R16: GLenum = 0x822A;
    pub const R16F: GLenum = 0x822D;
    pub const R16F_EXT: GLenum = 0x822D;
    pub const R16I: GLenum = 0x8233;
    pub const R16UI: GLenum = 0x8234;
    pub const R16_SNORM: GLenum = 0x8F98;
    pub const R32F: GLenum = 0x822E;
    pub const R32F_EXT: GLenum = 0x822E;
    pub const R32I: GLenum = 0x8235;
    pub const R32UI: GLenum = 0x8236;
    pub const R3_G3_B2: GLenum = 0x2A10;
    pub const R8: GLenum = 0x8229;
    pub const R8I: GLenum = 0x8231;
    pub const R8UI: GLenum = 0x8232;
    pub const R8_EXT: GLenum = 0x8229;
    pub const R8_SNORM: GLenum = 0x8F94;
    pub const RASTERIZER_DISCARD: GLenum = 0x8C89;
    pub const READ_BUFFER: GLenum = 0x0C02;
    pub const READ_FRAMEBUFFER: GLenum = 0x8CA8;
    pub const READ_FRAMEBUFFER_BINDING: GLenum = 0x8CAA;
    pub const READ_ONLY: GLenum = 0x88B8;
    pub const READ_WRITE: GLenum = 0x88BA;
    pub const RED: GLenum = 0x1903;
    pub const RED_BIAS: GLenum = 0x0D15;
    pub const RED_BITS: GLenum = 0x0D52;
    pub const RED_INTEGER: GLenum = 0x8D94;
    pub const RED_SCALE: GLenum = 0x0D14;
    pub const REFLECTION_MAP: GLenum = 0x8512;
    pub const RENDER: GLenum = 0x1C00;
    pub const RENDERBUFFER: GLenum = 0x8D41;
    pub const RENDERBUFFER_ALPHA_SIZE: GLenum = 0x8D53;
    pub const RENDERBUFFER_BINDING: GLenum = 0x8CA7;
    pub const RENDERBUFFER_BLUE_SIZE: GLenum = 0x8D52;
    pub const RENDERBUFFER_DEPTH_SIZE: GLenum = 0x8D54;
    pub const RENDERBUFFER_GREEN_SIZE: GLenum = 0x8D51;
    pub const RENDERBUFFER_HEIGHT: GLenum = 0x8D43;
    pub const RENDERBUFFER_INTERNAL_FORMAT: GLenum = 0x8D44;
    pub const RENDERBUFFER_RED_SIZE: GLenum = 0x8D50;
    pub const RENDERBUFFER_SAMPLES: GLenum = 0x8CAB;
    pub const RENDERBUFFER_STENCIL_SIZE: GLenum = 0x8D55;
    pub const RENDERBUFFER_WIDTH: GLenum = 0x8D42;
    pub const RENDERER: GLenum = 0x1F01;
    pub const RENDER_MODE: GLenum = 0x0C40;
    pub const REPEAT: GLenum = 0x2901;
    pub const REPLACE: GLenum = 0x1E01;
    pub const REQUIRED_TEXTURE_IMAGE_UNITS_OES: GLenum = 0x8D68;
    pub const RESCALE_NORMAL: GLenum = 0x803A;
    pub const RETURN: GLenum = 0x0102;
    pub const RG: GLenum = 0x8227;
    pub const RG16: GLenum = 0x822C;
    pub const RG16F: GLenum = 0x822F;
    pub const RG16F_EXT: GLenum = 0x822F;
    pub const RG16I: GLenum = 0x8239;
    pub const RG16UI: GLenum = 0x823A;
    pub const RG16_SNORM: GLenum = 0x8F99;
    pub const RG32F: GLenum = 0x8230;
    pub const RG32F_EXT: GLenum = 0x8230;
    pub const RG32I: GLenum = 0x823B;
    pub const RG32UI: GLenum = 0x823C;
    pub const RG8: GLenum = 0x822B;
    pub const RG8I: GLenum = 0x8237;
    pub const RG8UI: GLenum = 0x8238;
    pub const RG8_EXT: GLenum = 0x822B;
    pub const RG8_SNORM: GLenum = 0x8F95;
    pub const RGB: GLenum = 0x1907;
    pub const RGB10: GLenum = 0x8052;
    pub const RGB10_A2: GLenum = 0x8059;
    pub const RGB10_A2UI: GLenum = 0x906F;
    pub const RGB10_A2_EXT: GLenum = 0x8059;
    pub const RGB10_EXT: GLenum = 0x8052;
    pub const RGB12: GLenum = 0x8053;
    pub const RGB16: GLenum = 0x8054;
    pub const RGB16F: GLenum = 0x881B;
    pub const RGB16F_EXT: GLenum = 0x881B;
    pub const RGB16I: GLenum = 0x8D89;
    pub const RGB16UI: GLenum = 0x8D77;
    pub const RGB16_SNORM: GLenum = 0x8F9A;
    pub const RGB32F: GLenum = 0x8815;
    pub const RGB32F_EXT: GLenum = 0x8815;
    pub const RGB32I: GLenum = 0x8D83;
    pub const RGB32UI: GLenum = 0x8D71;
    pub const RGB4: GLenum = 0x804F;
    pub const RGB5: GLenum = 0x8050;
    pub const RGB565: GLenum = 0x8D62;
    pub const RGB5_A1: GLenum = 0x8057;
    pub const RGB8: GLenum = 0x8051;
    pub const RGB8I: GLenum = 0x8D8F;
    pub const RGB8UI: GLenum = 0x8D7D;
    pub const RGB8_SNORM: GLenum = 0x8F96;
    pub const RGB9_E5: GLenum = 0x8C3D;
    pub const RGBA: GLenum = 0x1908;
    pub const RGBA12: GLenum = 0x805A;
    pub const RGBA16: GLenum = 0x805B;
    pub const RGBA16F: GLenum = 0x881A;
    pub const RGBA16F_EXT: GLenum = 0x881A;
    pub const RGBA16I: GLenum = 0x8D88;
    pub const RGBA16UI: GLenum = 0x8D76;
    pub const RGBA16_SNORM: GLenum = 0x8F9B;
    pub const RGBA2: GLenum = 0x8055;
    pub const RGBA32F: GLenum = 0x8814;
    pub const RGBA32F_EXT: GLenum = 0x8814;
    pub const RGBA32I: GLenum = 0x8D82;
    pub const RGBA32UI: GLenum = 0x8D70;
    pub const RGBA4: GLenum = 0x8056;
    pub const RGBA8: GLenum = 0x8058;
    pub const RGBA8I: GLenum = 0x8D8E;
    pub const RGBA8UI: GLenum = 0x8D7C;
    pub const RGBA8_SNORM: GLenum = 0x8F97;
    pub const RGBA_INTEGER: GLenum = 0x8D99;
    pub const RGBA_MODE: GLenum = 0x0C31;
    pub const RGB_INTEGER: GLenum = 0x8D98;
    pub const RGB_SCALE: GLenum = 0x8573;
    pub const RG_INTEGER: GLenum = 0x8228;
    pub const RIGHT: GLenum = 0x0407;
    pub const S: GLenum = 0x2000;
    pub const SAMPLER: GLenum = 0x82E6;
    pub const SAMPLER_1D: GLenum = 0x8B5D;
    pub const SAMPLER_1D_ARRAY: GLenum = 0x8DC0;
    pub const SAMPLER_1D_ARRAY_SHADOW: GLenum = 0x8DC3;
    pub const SAMPLER_1D_SHADOW: GLenum = 0x8B61;
    pub const SAMPLER_2D: GLenum = 0x8B5E;
    pub const SAMPLER_2D_ARRAY: GLenum = 0x8DC1;
    pub const SAMPLER_2D_ARRAY_SHADOW: GLenum = 0x8DC4;
    pub const SAMPLER_2D_MULTISAMPLE: GLenum = 0x9108;
    pub const SAMPLER_2D_MULTISAMPLE_ARRAY: GLenum = 0x910B;
    pub const SAMPLER_2D_RECT: GLenum = 0x8B63;
    pub const SAMPLER_2D_RECT_SHADOW: GLenum = 0x8B64;
    pub const SAMPLER_2D_SHADOW: GLenum = 0x8B62;
    pub const SAMPLER_3D: GLenum = 0x8B5F;
    pub const SAMPLER_BINDING: GLenum = 0x8919;
    pub const SAMPLER_BUFFER: GLenum = 0x8DC2;
    pub const SAMPLER_CUBE: GLenum = 0x8B60;
    pub const SAMPLER_CUBE_SHADOW: GLenum = 0x8DC5;
    pub const SAMPLER_EXTERNAL_OES: GLenum = 0x8D66;
    pub const SAMPLER_KHR: GLenum = 0x82E6;
    pub const SAMPLES: GLenum = 0x80A9;
    pub const SAMPLES_PASSED: GLenum = 0x8914;
    pub const SAMPLE_ALPHA_TO_COVERAGE: GLenum = 0x809E;
    pub const SAMPLE_ALPHA_TO_ONE: GLenum = 0x809F;
    pub const SAMPLE_BUFFERS: GLenum = 0x80A8;
    pub const SAMPLE_COVERAGE: GLenum = 0x80A0;
    pub const SAMPLE_COVERAGE_INVERT: GLenum = 0x80AB;
    pub const SAMPLE_COVERAGE_VALUE: GLenum = 0x80AA;
    pub const SAMPLE_MASK: GLenum = 0x8E51;
    pub const SAMPLE_MASK_VALUE: GLenum = 0x8E52;
    pub const SAMPLE_POSITION: GLenum = 0x8E50;
    pub const SCISSOR_BIT: GLenum = 0x00080000;
    pub const SCISSOR_BOX: GLenum = 0x0C10;
    pub const SCISSOR_TEST: GLenum = 0x0C11;
    pub const SCREEN_KHR: GLenum = 0x9295;
    pub const SECONDARY_COLOR_ARRAY: GLenum = 0x845E;
    pub const SECONDARY_COLOR_ARRAY_BUFFER_BINDING: GLenum = 0x889C;
    pub const SECONDARY_COLOR_ARRAY_POINTER: GLenum = 0x845D;
    pub const SECONDARY_COLOR_ARRAY_SIZE: GLenum = 0x845A;
    pub const SECONDARY_COLOR_ARRAY_STRIDE: GLenum = 0x845C;
    pub const SECONDARY_COLOR_ARRAY_TYPE: GLenum = 0x845B;
    pub const SELECT: GLenum = 0x1C02;
    pub const SELECTION_BUFFER_POINTER: GLenum = 0x0DF3;
    pub const SELECTION_BUFFER_SIZE: GLenum = 0x0DF4;
    pub const SEPARATE_ATTRIBS: GLenum = 0x8C8D;
    pub const SEPARATE_SPECULAR_COLOR: GLenum = 0x81FA;
    pub const SET: GLenum = 0x150F;
    pub const SHADER: GLenum = 0x82E1;
    pub const SHADER_BINARY_FORMATS: GLenum = 0x8DF8;
    pub const SHADER_COMPILER: GLenum = 0x8DFA;
    pub const SHADER_KHR: GLenum = 0x82E1;
    pub const SHADER_PIXEL_LOCAL_STORAGE_EXT: GLenum = 0x8F64;
    pub const SHADER_SOURCE_LENGTH: GLenum = 0x8B88;
    pub const SHADER_TYPE: GLenum = 0x8B4F;
    pub const SHADE_MODEL: GLenum = 0x0B54;
    pub const SHADING_LANGUAGE_VERSION: GLenum = 0x8B8C;
    pub const SHININESS: GLenum = 0x1601;
    pub const SHORT: GLenum = 0x1402;
    pub const SIGNALED: GLenum = 0x9119;
    pub const SIGNED_NORMALIZED: GLenum = 0x8F9C;
    pub const SINGLE_COLOR: GLenum = 0x81F9;
    pub const SLUMINANCE: GLenum = 0x8C46;
    pub const SLUMINANCE8: GLenum = 0x8C47;
    pub const SLUMINANCE8_ALPHA8: GLenum = 0x8C45;
    pub const SLUMINANCE_ALPHA: GLenum = 0x8C44;
    pub const SMOOTH: GLenum = 0x1D01;
    pub const SMOOTH_LINE_WIDTH_GRANULARITY: GLenum = 0x0B23;
    pub const SMOOTH_LINE_WIDTH_RANGE: GLenum = 0x0B22;
    pub const SMOOTH_POINT_SIZE_GRANULARITY: GLenum = 0x0B13;
    pub const SMOOTH_POINT_SIZE_RANGE: GLenum = 0x0B12;
    pub const SOFTLIGHT_KHR: GLenum = 0x929C;
    pub const SOURCE0_ALPHA: GLenum = 0x8588;
    pub const SOURCE0_RGB: GLenum = 0x8580;
    pub const SOURCE1_ALPHA: GLenum = 0x8589;
    pub const SOURCE1_RGB: GLenum = 0x8581;
    pub const SOURCE2_ALPHA: GLenum = 0x858A;
    pub const SOURCE2_RGB: GLenum = 0x8582;
    pub const SPECULAR: GLenum = 0x1202;
    pub const SPHERE_MAP: GLenum = 0x2402;
    pub const SPOT_CUTOFF: GLenum = 0x1206;
    pub const SPOT_DIRECTION: GLenum = 0x1204;
    pub const SPOT_EXPONENT: GLenum = 0x1205;
    pub const SRC0_ALPHA: GLenum = 0x8588;
    pub const SRC0_RGB: GLenum = 0x8580;
    pub const SRC1_ALPHA: GLenum = 0x8589;
    pub const SRC1_COLOR: GLenum = 0x88F9;
    pub const SRC1_RGB: GLenum = 0x8581;
    pub const SRC2_ALPHA: GLenum = 0x858A;
    pub const SRC2_RGB: GLenum = 0x8582;
    pub const SRC_ALPHA: GLenum = 0x0302;
    pub const SRC_ALPHA_SATURATE: GLenum = 0x0308;
    pub const SRC_COLOR: GLenum = 0x0300;
    pub const SRGB: GLenum = 0x8C40;
    pub const SRGB8: GLenum = 0x8C41;
    pub const SRGB8_ALPHA8: GLenum = 0x8C43;
    pub const SRGB_ALPHA: GLenum = 0x8C42;
    pub const STACK_OVERFLOW: GLenum = 0x0503;
    pub const STACK_OVERFLOW_KHR: GLenum = 0x0503;
    pub const STACK_UNDERFLOW: GLenum = 0x0504;
    pub const STACK_UNDERFLOW_KHR: GLenum = 0x0504;
    pub const STATIC_COPY: GLenum = 0x88E6;
    pub const STATIC_DRAW: GLenum = 0x88E4;
    pub const STATIC_READ: GLenum = 0x88E5;
    pub const STENCIL: GLenum = 0x1802;
    pub const STENCIL_ATTACHMENT: GLenum = 0x8D20;
    pub const STENCIL_BACK_FAIL: GLenum = 0x8801;
    pub const STENCIL_BACK_FUNC: GLenum = 0x8800;
    pub const STENCIL_BACK_PASS_DEPTH_FAIL: GLenum = 0x8802;
    pub const STENCIL_BACK_PASS_DEPTH_PASS: GLenum = 0x8803;
    pub const STENCIL_BACK_REF: GLenum = 0x8CA3;
    pub const STENCIL_BACK_VALUE_MASK: GLenum = 0x8CA4;
    pub const STENCIL_BACK_WRITEMASK: GLenum = 0x8CA5;
    pub const STENCIL_BITS: GLenum = 0x0D57;
    pub const STENCIL_BUFFER_BIT: GLenum = 0x00000400;
    pub const STENCIL_CLEAR_VALUE: GLenum = 0x0B91;
    pub const STENCIL_FAIL: GLenum = 0x0B94;
    pub const STENCIL_FUNC: GLenum = 0x0B92;
    pub const STENCIL_INDEX: GLenum = 0x1901;
    pub const STENCIL_INDEX1: GLenum = 0x8D46;
    pub const STENCIL_INDEX16: GLenum = 0x8D49;
    pub const STENCIL_INDEX4: GLenum = 0x8D47;
    pub const STENCIL_INDEX8: GLenum = 0x8D48;
    pub const STENCIL_PASS_DEPTH_FAIL: GLenum = 0x0B95;
    pub const STENCIL_PASS_DEPTH_PASS: GLenum = 0x0B96;
    pub const STENCIL_REF: GLenum = 0x0B97;
    pub const STENCIL_TEST: GLenum = 0x0B90;
    pub const STENCIL_VALUE_MASK: GLenum = 0x0B93;
    pub const STENCIL_WRITEMASK: GLenum = 0x0B98;
    pub const STEREO: GLenum = 0x0C33;
    pub const STORAGE_CACHED_APPLE: GLenum = 0x85BE;
    pub const STORAGE_PRIVATE_APPLE: GLenum = 0x85BD;
    pub const STORAGE_SHARED_APPLE: GLenum = 0x85BF;
    pub const STREAM_COPY: GLenum = 0x88E2;
    pub const STREAM_DRAW: GLenum = 0x88E0;
    pub const STREAM_READ: GLenum = 0x88E1;
    pub const SUBPIXEL_BITS: GLenum = 0x0D50;
    pub const SUBTRACT: GLenum = 0x84E7;
    pub const SYNC_CONDITION: GLenum = 0x9113;
    pub const SYNC_FENCE: GLenum = 0x9116;
    pub const SYNC_FLAGS: GLenum = 0x9115;
    pub const SYNC_FLUSH_COMMANDS_BIT: GLenum = 0x00000001;
    pub const SYNC_GPU_COMMANDS_COMPLETE: GLenum = 0x9117;
    pub const SYNC_STATUS: GLenum = 0x9114;
    pub const T: GLenum = 0x2001;
    pub const T2F_C3F_V3F: GLenum = 0x2A2A;
    pub const T2F_C4F_N3F_V3F: GLenum = 0x2A2C;
    pub const T2F_C4UB_V3F: GLenum = 0x2A29;
    pub const T2F_N3F_V3F: GLenum = 0x2A2B;
    pub const T2F_V3F: GLenum = 0x2A27;
    pub const T4F_C4F_N3F_V4F: GLenum = 0x2A2D;
    pub const T4F_V4F: GLenum = 0x2A28;
    pub const TEXTURE: GLenum = 0x1702;
    pub const TEXTURE0: GLenum = 0x84C0;
    pub const TEXTURE1: GLenum = 0x84C1;
    pub const TEXTURE10: GLenum = 0x84CA;
    pub const TEXTURE11: GLenum = 0x84CB;
    pub const TEXTURE12: GLenum = 0x84CC;
    pub const TEXTURE13: GLenum = 0x84CD;
    pub const TEXTURE14: GLenum = 0x84CE;
    pub const TEXTURE15: GLenum = 0x84CF;
    pub const TEXTURE16: GLenum = 0x84D0;
    pub const TEXTURE17: GLenum = 0x84D1;
    pub const TEXTURE18: GLenum = 0x84D2;
    pub const TEXTURE19: GLenum = 0x84D3;
    pub const TEXTURE2: GLenum = 0x84C2;
    pub const TEXTURE20: GLenum = 0x84D4;
    pub const TEXTURE21: GLenum = 0x84D5;
    pub const TEXTURE22: GLenum = 0x84D6;
    pub const TEXTURE23: GLenum = 0x84D7;
    pub const TEXTURE24: GLenum = 0x84D8;
    pub const TEXTURE25: GLenum = 0x84D9;
    pub const TEXTURE26: GLenum = 0x84DA;
    pub const TEXTURE27: GLenum = 0x84DB;
    pub const TEXTURE28: GLenum = 0x84DC;
    pub const TEXTURE29: GLenum = 0x84DD;
    pub const TEXTURE3: GLenum = 0x84C3;
    pub const TEXTURE30: GLenum = 0x84DE;
    pub const TEXTURE31: GLenum = 0x84DF;
    pub const TEXTURE4: GLenum = 0x84C4;
    pub const TEXTURE5: GLenum = 0x84C5;
    pub const TEXTURE6: GLenum = 0x84C6;
    pub const TEXTURE7: GLenum = 0x84C7;
    pub const TEXTURE8: GLenum = 0x84C8;
    pub const TEXTURE9: GLenum = 0x84C9;
    pub const TEXTURE_1D: GLenum = 0x0DE0;
    pub const TEXTURE_1D_ARRAY: GLenum = 0x8C18;
    pub const TEXTURE_2D: GLenum = 0x0DE1;
    pub const TEXTURE_2D_ARRAY: GLenum = 0x8C1A;
    pub const TEXTURE_2D_MULTISAMPLE: GLenum = 0x9100;
    pub const TEXTURE_2D_MULTISAMPLE_ARRAY: GLenum = 0x9102;
    pub const TEXTURE_3D: GLenum = 0x806F;
    pub const TEXTURE_ALPHA_SIZE: GLenum = 0x805F;
    pub const TEXTURE_ALPHA_TYPE: GLenum = 0x8C13;
    pub const TEXTURE_BASE_LEVEL: GLenum = 0x813C;
    pub const TEXTURE_BINDING_1D: GLenum = 0x8068;
    pub const TEXTURE_BINDING_1D_ARRAY: GLenum = 0x8C1C;
    pub const TEXTURE_BINDING_2D: GLenum = 0x8069;
    pub const TEXTURE_BINDING_2D_ARRAY: GLenum = 0x8C1D;
    pub const TEXTURE_BINDING_2D_MULTISAMPLE: GLenum = 0x9104;
    pub const TEXTURE_BINDING_2D_MULTISAMPLE_ARRAY: GLenum = 0x9105;
    pub const TEXTURE_BINDING_3D: GLenum = 0x806A;
    pub const TEXTURE_BINDING_BUFFER: GLenum = 0x8C2C;
    pub const TEXTURE_BINDING_CUBE_MAP: GLenum = 0x8514;
    pub const TEXTURE_BINDING_EXTERNAL_OES: GLenum = 0x8D67;
    pub const TEXTURE_BINDING_RECTANGLE: GLenum = 0x84F6;
    pub const TEXTURE_BINDING_RECTANGLE_ARB: GLenum = 0x84F6;
    pub const TEXTURE_BIT: GLenum = 0x00040000;
    pub const TEXTURE_BLUE_SIZE: GLenum = 0x805E;
    pub const TEXTURE_BLUE_TYPE: GLenum = 0x8C12;
    pub const TEXTURE_BORDER: GLenum = 0x1005;
    pub const TEXTURE_BORDER_COLOR: GLenum = 0x1004;
    pub const TEXTURE_BUFFER: GLenum = 0x8C2A;
    pub const TEXTURE_BUFFER_DATA_STORE_BINDING: GLenum = 0x8C2D;
    pub const TEXTURE_COMPARE_FUNC: GLenum = 0x884D;
    pub const TEXTURE_COMPARE_MODE: GLenum = 0x884C;
    pub const TEXTURE_COMPONENTS: GLenum = 0x1003;
    pub const TEXTURE_COMPRESSED: GLenum = 0x86A1;
    pub const TEXTURE_COMPRESSED_IMAGE_SIZE: GLenum = 0x86A0;
    pub const TEXTURE_COMPRESSION_HINT: GLenum = 0x84EF;
    pub const TEXTURE_COORD_ARRAY: GLenum = 0x8078;
    pub const TEXTURE_COORD_ARRAY_BUFFER_BINDING: GLenum = 0x889A;
    pub const TEXTURE_COORD_ARRAY_POINTER: GLenum = 0x8092;
    pub const TEXTURE_COORD_ARRAY_SIZE: GLenum = 0x8088;
    pub const TEXTURE_COORD_ARRAY_STRIDE: GLenum = 0x808A;
    pub const TEXTURE_COORD_ARRAY_TYPE: GLenum = 0x8089;
    pub const TEXTURE_CUBE_MAP: GLenum = 0x8513;
    pub const TEXTURE_CUBE_MAP_NEGATIVE_X: GLenum = 0x8516;
    pub const TEXTURE_CUBE_MAP_NEGATIVE_Y: GLenum = 0x8518;
    pub const TEXTURE_CUBE_MAP_NEGATIVE_Z: GLenum = 0x851A;
    pub const TEXTURE_CUBE_MAP_POSITIVE_X: GLenum = 0x8515;
    pub const TEXTURE_CUBE_MAP_POSITIVE_Y: GLenum = 0x8517;
    pub const TEXTURE_CUBE_MAP_POSITIVE_Z: GLenum = 0x8519;
    pub const TEXTURE_CUBE_MAP_SEAMLESS: GLenum = 0x884F;
    pub const TEXTURE_DEPTH: GLenum = 0x8071;
    pub const TEXTURE_DEPTH_SIZE: GLenum = 0x884A;
    pub const TEXTURE_DEPTH_TYPE: GLenum = 0x8C16;
    pub const TEXTURE_ENV: GLenum = 0x2300;
    pub const TEXTURE_ENV_COLOR: GLenum = 0x2201;
    pub const TEXTURE_ENV_MODE: GLenum = 0x2200;
    pub const TEXTURE_EXTERNAL_OES: GLenum = 0x8D65;
    pub const TEXTURE_FILTER_CONTROL: GLenum = 0x8500;
    pub const TEXTURE_FIXED_SAMPLE_LOCATIONS: GLenum = 0x9107;
    pub const TEXTURE_GEN_MODE: GLenum = 0x2500;
    pub const TEXTURE_GEN_Q: GLenum = 0x0C63;
    pub const TEXTURE_GEN_R: GLenum = 0x0C62;
    pub const TEXTURE_GEN_S: GLenum = 0x0C60;
    pub const TEXTURE_GEN_T: GLenum = 0x0C61;
    pub const TEXTURE_GREEN_SIZE: GLenum = 0x805D;
    pub const TEXTURE_GREEN_TYPE: GLenum = 0x8C11;
    pub const TEXTURE_HEIGHT: GLenum = 0x1001;
    pub const TEXTURE_IMMUTABLE_FORMAT: GLenum = 0x912F;
    pub const TEXTURE_IMMUTABLE_FORMAT_EXT: GLenum = 0x912F;
    pub const TEXTURE_IMMUTABLE_LEVELS: GLenum = 0x82DF;
    pub const TEXTURE_INTENSITY_SIZE: GLenum = 0x8061;
    pub const TEXTURE_INTENSITY_TYPE: GLenum = 0x8C15;
    pub const TEXTURE_INTERNAL_FORMAT: GLenum = 0x1003;
    pub const TEXTURE_LOD_BIAS: GLenum = 0x8501;
    pub const TEXTURE_LUMINANCE_SIZE: GLenum = 0x8060;
    pub const TEXTURE_LUMINANCE_TYPE: GLenum = 0x8C14;
    pub const TEXTURE_MAG_FILTER: GLenum = 0x2800;
    pub const TEXTURE_MATRIX: GLenum = 0x0BA8;
    pub const TEXTURE_MAX_ANISOTROPY_EXT: GLenum = 0x84FE;
    pub const TEXTURE_MAX_LEVEL: GLenum = 0x813D;
    pub const TEXTURE_MAX_LOD: GLenum = 0x813B;
    pub const TEXTURE_MIN_FILTER: GLenum = 0x2801;
    pub const TEXTURE_MIN_LOD: GLenum = 0x813A;
    pub const TEXTURE_PRIORITY: GLenum = 0x8066;
    pub const TEXTURE_RANGE_LENGTH_APPLE: GLenum = 0x85B7;
    pub const TEXTURE_RANGE_POINTER_APPLE: GLenum = 0x85B8;
    pub const TEXTURE_RECTANGLE: GLenum = 0x84F5;
    pub const TEXTURE_RECTANGLE_ARB: GLenum = 0x84F5;
    pub const TEXTURE_RED_SIZE: GLenum = 0x805C;
    pub const TEXTURE_RED_TYPE: GLenum = 0x8C10;
    pub const TEXTURE_RESIDENT: GLenum = 0x8067;
    pub const TEXTURE_SAMPLES: GLenum = 0x9106;
    pub const TEXTURE_SHARED_SIZE: GLenum = 0x8C3F;
    pub const TEXTURE_STACK_DEPTH: GLenum = 0x0BA5;
    pub const TEXTURE_STENCIL_SIZE: GLenum = 0x88F1;
    pub const TEXTURE_STORAGE_HINT_APPLE: GLenum = 0x85BC;
    pub const TEXTURE_SWIZZLE_A: GLenum = 0x8E45;
    pub const TEXTURE_SWIZZLE_B: GLenum = 0x8E44;
    pub const TEXTURE_SWIZZLE_G: GLenum = 0x8E43;
    pub const TEXTURE_SWIZZLE_R: GLenum = 0x8E42;
    pub const TEXTURE_SWIZZLE_RGBA: GLenum = 0x8E46;
    pub const TEXTURE_USAGE_ANGLE: GLenum = 0x93A2;
    pub const TEXTURE_WIDTH: GLenum = 0x1000;
    pub const TEXTURE_WRAP_R: GLenum = 0x8072;
    pub const TEXTURE_WRAP_S: GLenum = 0x2802;
    pub const TEXTURE_WRAP_T: GLenum = 0x2803;
    pub const TIMEOUT_EXPIRED: GLenum = 0x911B;
    pub const TIMEOUT_IGNORED: u64 = 0xFFFFFFFFFFFFFFFF;
    pub const TIMESTAMP: GLenum = 0x8E28;
    pub const TIMESTAMP_EXT: GLenum = 0x8E28;
    pub const TIME_ELAPSED: GLenum = 0x88BF;
    pub const TIME_ELAPSED_EXT: GLenum = 0x88BF;
    pub const TRANSFORM_BIT: GLenum = 0x00001000;
    pub const TRANSFORM_FEEDBACK: GLenum = 0x8E22;
    pub const TRANSFORM_FEEDBACK_ACTIVE: GLenum = 0x8E24;
    pub const TRANSFORM_FEEDBACK_BINDING: GLenum = 0x8E25;
    pub const TRANSFORM_FEEDBACK_BUFFER: GLenum = 0x8C8E;
    pub const TRANSFORM_FEEDBACK_BUFFER_BINDING: GLenum = 0x8C8F;
    pub const TRANSFORM_FEEDBACK_BUFFER_MODE: GLenum = 0x8C7F;
    pub const TRANSFORM_FEEDBACK_BUFFER_SIZE: GLenum = 0x8C85;
    pub const TRANSFORM_FEEDBACK_BUFFER_START: GLenum = 0x8C84;
    pub const TRANSFORM_FEEDBACK_PAUSED: GLenum = 0x8E23;
    pub const TRANSFORM_FEEDBACK_PRIMITIVES_WRITTEN: GLenum = 0x8C88;
    pub const TRANSFORM_FEEDBACK_VARYINGS: GLenum = 0x8C83;
    pub const TRANSFORM_FEEDBACK_VARYING_MAX_LENGTH: GLenum = 0x8C76;
    pub const TRANSPOSE_COLOR_MATRIX: GLenum = 0x84E6;
    pub const TRANSPOSE_MODELVIEW_MATRIX: GLenum = 0x84E3;
    pub const TRANSPOSE_PROJECTION_MATRIX: GLenum = 0x84E4;
    pub const TRANSPOSE_TEXTURE_MATRIX: GLenum = 0x84E5;
    pub const TRIANGLES: GLenum = 0x0004;
    pub const TRIANGLES_ADJACENCY: GLenum = 0x000C;
    pub const TRIANGLE_FAN: GLenum = 0x0006;
    pub const TRIANGLE_STRIP: GLenum = 0x0005;
    pub const TRIANGLE_STRIP_ADJACENCY: GLenum = 0x000D;
    pub const TRUE: GLenum = 1;
    pub const UNIFORM_ARRAY_STRIDE: GLenum = 0x8A3C;
    pub const UNIFORM_BLOCK_ACTIVE_UNIFORMS: GLenum = 0x8A42;
    pub const UNIFORM_BLOCK_ACTIVE_UNIFORM_INDICES: GLenum = 0x8A43;
    pub const UNIFORM_BLOCK_BINDING: GLenum = 0x8A3F;
    pub const UNIFORM_BLOCK_DATA_SIZE: GLenum = 0x8A40;
    pub const UNIFORM_BLOCK_INDEX: GLenum = 0x8A3A;
    pub const UNIFORM_BLOCK_NAME_LENGTH: GLenum = 0x8A41;
    pub const UNIFORM_BLOCK_REFERENCED_BY_FRAGMENT_SHADER: GLenum = 0x8A46;
    pub const UNIFORM_BLOCK_REFERENCED_BY_GEOMETRY_SHADER: GLenum = 0x8A45;
    pub const UNIFORM_BLOCK_REFERENCED_BY_VERTEX_SHADER: GLenum = 0x8A44;
    pub const UNIFORM_BUFFER: GLenum = 0x8A11;
    pub const UNIFORM_BUFFER_BINDING: GLenum = 0x8A28;
    pub const UNIFORM_BUFFER_OFFSET_ALIGNMENT: GLenum = 0x8A34;
    pub const UNIFORM_BUFFER_SIZE: GLenum = 0x8A2A;
    pub const UNIFORM_BUFFER_START: GLenum = 0x8A29;
    pub const UNIFORM_IS_ROW_MAJOR: GLenum = 0x8A3E;
    pub const UNIFORM_MATRIX_STRIDE: GLenum = 0x8A3D;
    pub const UNIFORM_NAME_LENGTH: GLenum = 0x8A39;
    pub const UNIFORM_OFFSET: GLenum = 0x8A3B;
    pub const UNIFORM_SIZE: GLenum = 0x8A38;
    pub const UNIFORM_TYPE: GLenum = 0x8A37;
    pub const UNPACK_ALIGNMENT: GLenum = 0x0CF5;
    pub const UNPACK_CLIENT_STORAGE_APPLE: GLenum = 0x85B2;
    pub const UNPACK_IMAGE_HEIGHT: GLenum = 0x806E;
    pub const UNPACK_LSB_FIRST: GLenum = 0x0CF1;
    pub const UNPACK_ROW_LENGTH: GLenum = 0x0CF2;
    pub const UNPACK_SKIP_IMAGES: GLenum = 0x806D;
    pub const UNPACK_SKIP_PIXELS: GLenum = 0x0CF4;
    pub const UNPACK_SKIP_ROWS: GLenum = 0x0CF3;
    pub const UNPACK_SWAP_BYTES: GLenum = 0x0CF0;
    pub const UNSIGNALED: GLenum = 0x9118;
    pub const UNSIGNED_BYTE: GLenum = 0x1401;
    pub const UNSIGNED_BYTE_2_3_3_REV: GLenum = 0x8362;
    pub const UNSIGNED_BYTE_3_3_2: GLenum = 0x8032;
    pub const UNSIGNED_INT: GLenum = 0x1405;
    pub const UNSIGNED_INT_10F_11F_11F_REV: GLenum = 0x8C3B;
    pub const UNSIGNED_INT_10_10_10_2: GLenum = 0x8036;
    pub const UNSIGNED_INT_24_8: GLenum = 0x84FA;
    pub const UNSIGNED_INT_2_10_10_10_REV: GLenum = 0x8368;
    pub const UNSIGNED_INT_5_9_9_9_REV: GLenum = 0x8C3E;
    pub const UNSIGNED_INT_8_8_8_8: GLenum = 0x8035;
    pub const UNSIGNED_INT_8_8_8_8_REV: GLenum = 0x8367;
    pub const UNSIGNED_INT_SAMPLER_1D: GLenum = 0x8DD1;
    pub const UNSIGNED_INT_SAMPLER_1D_ARRAY: GLenum = 0x8DD6;
    pub const UNSIGNED_INT_SAMPLER_2D: GLenum = 0x8DD2;
    pub const UNSIGNED_INT_SAMPLER_2D_ARRAY: GLenum = 0x8DD7;
    pub const UNSIGNED_INT_SAMPLER_2D_MULTISAMPLE: GLenum = 0x910A;
    pub const UNSIGNED_INT_SAMPLER_2D_MULTISAMPLE_ARRAY: GLenum = 0x910D;
    pub const UNSIGNED_INT_SAMPLER_2D_RECT: GLenum = 0x8DD5;
    pub const UNSIGNED_INT_SAMPLER_3D: GLenum = 0x8DD3;
    pub const UNSIGNED_INT_SAMPLER_BUFFER: GLenum = 0x8DD8;
    pub const UNSIGNED_INT_SAMPLER_CUBE: GLenum = 0x8DD4;
    pub const UNSIGNED_INT_VEC2: GLenum = 0x8DC6;
    pub const UNSIGNED_INT_VEC3: GLenum = 0x8DC7;
    pub const UNSIGNED_INT_VEC4: GLenum = 0x8DC8;
    pub const UNSIGNED_NORMALIZED: GLenum = 0x8C17;
    pub const UNSIGNED_SHORT: GLenum = 0x1403;
    pub const UNSIGNED_SHORT_1_5_5_5_REV: GLenum = 0x8366;
    pub const UNSIGNED_SHORT_4_4_4_4: GLenum = 0x8033;
    pub const UNSIGNED_SHORT_4_4_4_4_REV: GLenum = 0x8365;
    pub const UNSIGNED_SHORT_5_5_5_1: GLenum = 0x8034;
    pub const UNSIGNED_SHORT_5_6_5: GLenum = 0x8363;
    pub const UNSIGNED_SHORT_5_6_5_REV: GLenum = 0x8364;
    pub const UPPER_LEFT: GLenum = 0x8CA2;
    pub const V2F: GLenum = 0x2A20;
    pub const V3F: GLenum = 0x2A21;
    pub const VALIDATE_STATUS: GLenum = 0x8B83;
    pub const VENDOR: GLenum = 0x1F00;
    pub const VERSION: GLenum = 0x1F02;
    pub const VERTEX_ARRAY: GLenum = 0x8074;
    pub const VERTEX_ARRAY_BINDING: GLenum = 0x85B5;
    pub const VERTEX_ARRAY_BINDING_APPLE: GLenum = 0x85B5;
    pub const VERTEX_ARRAY_BUFFER_BINDING: GLenum = 0x8896;
    pub const VERTEX_ARRAY_KHR: GLenum = 0x8074;
    pub const VERTEX_ARRAY_POINTER: GLenum = 0x808E;
    pub const VERTEX_ARRAY_SIZE: GLenum = 0x807A;
    pub const VERTEX_ARRAY_STRIDE: GLenum = 0x807C;
    pub const VERTEX_ARRAY_TYPE: GLenum = 0x807B;
    pub const VERTEX_ATTRIB_ARRAY_BUFFER_BINDING: GLenum = 0x889F;
    pub const VERTEX_ATTRIB_ARRAY_DIVISOR: GLenum = 0x88FE;
    pub const VERTEX_ATTRIB_ARRAY_ENABLED: GLenum = 0x8622;
    pub const VERTEX_ATTRIB_ARRAY_INTEGER: GLenum = 0x88FD;
    pub const VERTEX_ATTRIB_ARRAY_NORMALIZED: GLenum = 0x886A;
    pub const VERTEX_ATTRIB_ARRAY_POINTER: GLenum = 0x8645;
    pub const VERTEX_ATTRIB_ARRAY_SIZE: GLenum = 0x8623;
    pub const VERTEX_ATTRIB_ARRAY_STRIDE: GLenum = 0x8624;
    pub const VERTEX_ATTRIB_ARRAY_TYPE: GLenum = 0x8625;
    pub const VERTEX_PROGRAM_POINT_SIZE: GLenum = 0x8642;
    pub const VERTEX_PROGRAM_TWO_SIDE: GLenum = 0x8643;
    pub const VERTEX_SHADER: GLenum = 0x8B31;
    pub const VIEWPORT: GLenum = 0x0BA2;
    pub const VIEWPORT_BIT: GLenum = 0x00000800;
    pub const WAIT_FAILED: GLenum = 0x911D;
    pub const WEIGHT_ARRAY_BUFFER_BINDING: GLenum = 0x889E;
    pub const WRITE_ONLY: GLenum = 0x88B9;
    pub const XOR: GLenum = 0x1506;
    pub const ZERO: GLenum = 0;
    pub const ZOOM_X: GLenum = 0x0D16;
    pub const ZOOM_Y: GLenum = 0x0D17;
}

pub use self::ctypes::*;

/// Typedef for an OpenGL handle
pub type GLuint = u32;
pub type GLint = i32;
pub type GLint64 = i64;
pub type GLuint64 = u64;
pub type GLenum = u32;
pub type GLintptr = isize;
pub type GLboolean = u8;
pub type GLsizeiptr = isize;
pub type GLvoid = core::ffi::c_void;
pub type GLbitfield = u32;
pub type GLsizei = i32;
pub type GLclampf = f32;
pub type GLfloat = f32;
pub type GLchar = c_char; // = c_char
pub type GLclampd = f64;
pub type GLubyte = c_uchar;

#[cfg(feature = "gleam_trait")]
pub type GLeglImageOES = gleam::gl::GLeglImageOES; // *const c_void;
#[cfg(not(feature = "gleam_trait"))]
pub type GLeglImageOES = *const c_void;

#[cfg(feature = "gleam_trait")]
pub type GLsync = gleam::gl::GLsync;
#[cfg(not(feature = "gleam_trait"))]
pub type GLsync = *const c_void;

#[cfg(feature = "gleam_trait")]
pub use gleam::gl::DebugMessage;

#[cfg(not(feature = "gleam_trait"))]
#[repr(C)]
#[derive(Debug)]
pub struct DebugMessage {
    pub message: String,
    pub source: GLenum,
    pub ty: GLenum,
    pub id: GLenum,
    pub severity: GLenum,
}

#[cfg(feature = "gleam_trait")]
pub use gleam::gl::GlType;

#[cfg(not(feature = "gleam_trait"))]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlType {
    Gl,
    GlEs,
}

pub struct GenericGlContext {
    pub glAccum: *mut c_void,
    pub glActiveTexture: *mut c_void,
    pub glAlphaFunc: *mut c_void,
    pub glAreTexturesResident: *mut c_void,
    pub glArrayElement: *mut c_void,
    pub glAttachShader: *mut c_void,
    pub glBegin: *mut c_void,
    pub glBeginConditionalRender: *mut c_void,
    pub glBeginQuery: *mut c_void,
    pub glBeginTransformFeedback: *mut c_void,
    pub glBindAttribLocation: *mut c_void,
    pub glBindBuffer: *mut c_void,
    pub glBindBufferBase: *mut c_void,
    pub glBindBufferRange: *mut c_void,
    pub glBindFragDataLocation: *mut c_void,
    pub glBindFragDataLocationIndexed: *mut c_void,
    pub glBindFramebuffer: *mut c_void,
    pub glBindRenderbuffer: *mut c_void,
    pub glBindSampler: *mut c_void,
    pub glBindTexture: *mut c_void,
    pub glBindVertexArray: *mut c_void,
    pub glBindVertexArrayAPPLE: *mut c_void,
    pub glBitmap: *mut c_void,
    pub glBlendBarrierKHR: *mut c_void,
    pub glBlendColor: *mut c_void,
    pub glBlendEquation: *mut c_void,
    pub glBlendEquationSeparate: *mut c_void,
    pub glBlendFunc: *mut c_void,
    pub glBlendFuncSeparate: *mut c_void,
    pub glBlitFramebuffer: *mut c_void,
    pub glBufferData: *mut c_void,
    pub glBufferStorage: *mut c_void,
    pub glBufferSubData: *mut c_void,
    pub glCallList: *mut c_void,
    pub glCallLists: *mut c_void,
    pub glCheckFramebufferStatus: *mut c_void,
    pub glClampColor: *mut c_void,
    pub glClear: *mut c_void,
    pub glClearAccum: *mut c_void,
    pub glClearBufferfi: *mut c_void,
    pub glClearBufferfv: *mut c_void,
    pub glClearBufferiv: *mut c_void,
    pub glClearBufferuiv: *mut c_void,
    pub glClearColor: *mut c_void,
    pub glClearDepth: *mut c_void,
    pub glClearIndex: *mut c_void,
    pub glClearStencil: *mut c_void,
    pub glClientActiveTexture: *mut c_void,
    pub glClientWaitSync: *mut c_void,
    pub glClipPlane: *mut c_void,
    pub glColor3b: *mut c_void,
    pub glColor3bv: *mut c_void,
    pub glColor3d: *mut c_void,
    pub glColor3dv: *mut c_void,
    pub glColor3f: *mut c_void,
    pub glColor3fv: *mut c_void,
    pub glColor3i: *mut c_void,
    pub glColor3iv: *mut c_void,
    pub glColor3s: *mut c_void,
    pub glColor3sv: *mut c_void,
    pub glColor3ub: *mut c_void,
    pub glColor3ubv: *mut c_void,
    pub glColor3ui: *mut c_void,
    pub glColor3uiv: *mut c_void,
    pub glColor3us: *mut c_void,
    pub glColor3usv: *mut c_void,
    pub glColor4b: *mut c_void,
    pub glColor4bv: *mut c_void,
    pub glColor4d: *mut c_void,
    pub glColor4dv: *mut c_void,
    pub glColor4f: *mut c_void,
    pub glColor4fv: *mut c_void,
    pub glColor4i: *mut c_void,
    pub glColor4iv: *mut c_void,
    pub glColor4s: *mut c_void,
    pub glColor4sv: *mut c_void,
    pub glColor4ub: *mut c_void,
    pub glColor4ubv: *mut c_void,
    pub glColor4ui: *mut c_void,
    pub glColor4uiv: *mut c_void,
    pub glColor4us: *mut c_void,
    pub glColor4usv: *mut c_void,
    pub glColorMask: *mut c_void,
    pub glColorMaski: *mut c_void,
    pub glColorMaterial: *mut c_void,
    pub glColorP3ui: *mut c_void,
    pub glColorP3uiv: *mut c_void,
    pub glColorP4ui: *mut c_void,
    pub glColorP4uiv: *mut c_void,
    pub glColorPointer: *mut c_void,
    pub glCompileShader: *mut c_void,
    pub glCompressedTexImage1D: *mut c_void,
    pub glCompressedTexImage2D: *mut c_void,
    pub glCompressedTexImage3D: *mut c_void,
    pub glCompressedTexSubImage1D: *mut c_void,
    pub glCompressedTexSubImage2D: *mut c_void,
    pub glCompressedTexSubImage3D: *mut c_void,
    pub glCopyBufferSubData: *mut c_void,
    pub glCopyImageSubData: *mut c_void,
    pub glCopyPixels: *mut c_void,
    pub glCopyTexImage1D: *mut c_void,
    pub glCopyTexImage2D: *mut c_void,
    pub glCopyTexSubImage1D: *mut c_void,
    pub glCopyTexSubImage2D: *mut c_void,
    pub glCopyTexSubImage3D: *mut c_void,
    pub glCreateProgram: *mut c_void,
    pub glCreateShader: *mut c_void,
    pub glCullFace: *mut c_void,
    pub glDebugMessageCallback: *mut c_void,
    pub glDebugMessageCallbackKHR: *mut c_void,
    pub glDebugMessageControl: *mut c_void,
    pub glDebugMessageControlKHR: *mut c_void,
    pub glDebugMessageInsert: *mut c_void,
    pub glDebugMessageInsertKHR: *mut c_void,
    pub glDeleteBuffers: *mut c_void,
    pub glDeleteFencesAPPLE: *mut c_void,
    pub glDeleteFramebuffers: *mut c_void,
    pub glDeleteLists: *mut c_void,
    pub glDeleteProgram: *mut c_void,
    pub glDeleteQueries: *mut c_void,
    pub glDeleteRenderbuffers: *mut c_void,
    pub glDeleteSamplers: *mut c_void,
    pub glDeleteShader: *mut c_void,
    pub glDeleteSync: *mut c_void,
    pub glDeleteTextures: *mut c_void,
    pub glDeleteVertexArrays: *mut c_void,
    pub glDeleteVertexArraysAPPLE: *mut c_void,
    pub glDepthFunc: *mut c_void,
    pub glDepthMask: *mut c_void,
    pub glDepthRange: *mut c_void,
    pub glDetachShader: *mut c_void,
    pub glDisable: *mut c_void,
    pub glDisableClientState: *mut c_void,
    pub glDisableVertexAttribArray: *mut c_void,
    pub glDisablei: *mut c_void,
    pub glDrawArrays: *mut c_void,
    pub glDrawArraysInstanced: *mut c_void,
    pub glDrawBuffer: *mut c_void,
    pub glDrawBuffers: *mut c_void,
    pub glDrawElements: *mut c_void,
    pub glDrawElementsBaseVertex: *mut c_void,
    pub glDrawElementsInstanced: *mut c_void,
    pub glDrawElementsInstancedBaseVertex: *mut c_void,
    pub glDrawPixels: *mut c_void,
    pub glDrawRangeElements: *mut c_void,
    pub glDrawRangeElementsBaseVertex: *mut c_void,
    pub glEdgeFlag: *mut c_void,
    pub glEdgeFlagPointer: *mut c_void,
    pub glEdgeFlagv: *mut c_void,
    pub glEnable: *mut c_void,
    pub glEnableClientState: *mut c_void,
    pub glEnableVertexAttribArray: *mut c_void,
    pub glEnablei: *mut c_void,
    pub glEnd: *mut c_void,
    pub glEndConditionalRender: *mut c_void,
    pub glEndList: *mut c_void,
    pub glEndQuery: *mut c_void,
    pub glEndTransformFeedback: *mut c_void,
    pub glEvalCoord1d: *mut c_void,
    pub glEvalCoord1dv: *mut c_void,
    pub glEvalCoord1f: *mut c_void,
    pub glEvalCoord1fv: *mut c_void,
    pub glEvalCoord2d: *mut c_void,
    pub glEvalCoord2dv: *mut c_void,
    pub glEvalCoord2f: *mut c_void,
    pub glEvalCoord2fv: *mut c_void,
    pub glEvalMesh1: *mut c_void,
    pub glEvalMesh2: *mut c_void,
    pub glEvalPoint1: *mut c_void,
    pub glEvalPoint2: *mut c_void,
    pub glFeedbackBuffer: *mut c_void,
    pub glFenceSync: *mut c_void,
    pub glFinish: *mut c_void,
    pub glFinishFenceAPPLE: *mut c_void,
    pub glFinishObjectAPPLE: *mut c_void,
    pub glFlush: *mut c_void,
    pub glFlushMappedBufferRange: *mut c_void,
    pub glFogCoordPointer: *mut c_void,
    pub glFogCoordd: *mut c_void,
    pub glFogCoorddv: *mut c_void,
    pub glFogCoordf: *mut c_void,
    pub glFogCoordfv: *mut c_void,
    pub glFogf: *mut c_void,
    pub glFogfv: *mut c_void,
    pub glFogi: *mut c_void,
    pub glFogiv: *mut c_void,
    pub glFramebufferRenderbuffer: *mut c_void,
    pub glFramebufferTexture: *mut c_void,
    pub glFramebufferTexture1D: *mut c_void,
    pub glFramebufferTexture2D: *mut c_void,
    pub glFramebufferTexture3D: *mut c_void,
    pub glFramebufferTextureLayer: *mut c_void,
    pub glFrontFace: *mut c_void,
    pub glFrustum: *mut c_void,
    pub glGenBuffers: *mut c_void,
    pub glGenFencesAPPLE: *mut c_void,
    pub glGenFramebuffers: *mut c_void,
    pub glGenLists: *mut c_void,
    pub glGenQueries: *mut c_void,
    pub glGenRenderbuffers: *mut c_void,
    pub glGenSamplers: *mut c_void,
    pub glGenTextures: *mut c_void,
    pub glGenVertexArrays: *mut c_void,
    pub glGenVertexArraysAPPLE: *mut c_void,
    pub glGenerateMipmap: *mut c_void,
    pub glGetActiveAttrib: *mut c_void,
    pub glGetActiveUniform: *mut c_void,
    pub glGetActiveUniformBlockName: *mut c_void,
    pub glGetActiveUniformBlockiv: *mut c_void,
    pub glGetActiveUniformName: *mut c_void,
    pub glGetActiveUniformsiv: *mut c_void,
    pub glGetAttachedShaders: *mut c_void,
    pub glGetAttribLocation: *mut c_void,
    pub glGetBooleani_v: *mut c_void,
    pub glGetBooleanv: *mut c_void,
    pub glGetBufferParameteri64v: *mut c_void,
    pub glGetBufferParameteriv: *mut c_void,
    pub glGetBufferPointerv: *mut c_void,
    pub glGetBufferSubData: *mut c_void,
    pub glGetClipPlane: *mut c_void,
    pub glGetCompressedTexImage: *mut c_void,
    pub glGetDebugMessageLog: *mut c_void,
    pub glGetDebugMessageLogKHR: *mut c_void,
    pub glGetDoublev: *mut c_void,
    pub glGetError: *mut c_void,
    pub glGetFloatv: *mut c_void,
    pub glGetFragDataIndex: *mut c_void,
    pub glGetFragDataLocation: *mut c_void,
    pub glGetFramebufferAttachmentParameteriv: *mut c_void,
    pub glGetInteger64i_v: *mut c_void,
    pub glGetInteger64v: *mut c_void,
    pub glGetIntegeri_v: *mut c_void,
    pub glGetIntegerv: *mut c_void,
    pub glGetLightfv: *mut c_void,
    pub glGetLightiv: *mut c_void,
    pub glGetMapdv: *mut c_void,
    pub glGetMapfv: *mut c_void,
    pub glGetMapiv: *mut c_void,
    pub glGetMaterialfv: *mut c_void,
    pub glGetMaterialiv: *mut c_void,
    pub glGetMultisamplefv: *mut c_void,
    pub glGetObjectLabel: *mut c_void,
    pub glGetObjectLabelKHR: *mut c_void,
    pub glGetObjectPtrLabel: *mut c_void,
    pub glGetObjectPtrLabelKHR: *mut c_void,
    pub glGetPixelMapfv: *mut c_void,
    pub glGetPixelMapuiv: *mut c_void,
    pub glGetPixelMapusv: *mut c_void,
    pub glGetPointerv: *mut c_void,
    pub glGetPointervKHR: *mut c_void,
    pub glGetPolygonStipple: *mut c_void,
    pub glGetProgramBinary: *mut c_void,
    pub glGetProgramInfoLog: *mut c_void,
    pub glGetProgramiv: *mut c_void,
    pub glGetQueryObjecti64v: *mut c_void,
    pub glGetQueryObjectiv: *mut c_void,
    pub glGetQueryObjectui64v: *mut c_void,
    pub glGetQueryObjectuiv: *mut c_void,
    pub glGetQueryiv: *mut c_void,
    pub glGetRenderbufferParameteriv: *mut c_void,
    pub glGetSamplerParameterIiv: *mut c_void,
    pub glGetSamplerParameterIuiv: *mut c_void,
    pub glGetSamplerParameterfv: *mut c_void,
    pub glGetSamplerParameteriv: *mut c_void,
    pub glGetShaderInfoLog: *mut c_void,
    pub glGetShaderSource: *mut c_void,
    pub glGetShaderiv: *mut c_void,
    pub glGetString: *mut c_void,
    pub glGetStringi: *mut c_void,
    pub glGetSynciv: *mut c_void,
    pub glGetTexEnvfv: *mut c_void,
    pub glGetTexEnviv: *mut c_void,
    pub glGetTexGendv: *mut c_void,
    pub glGetTexGenfv: *mut c_void,
    pub glGetTexGeniv: *mut c_void,
    pub glGetTexImage: *mut c_void,
    pub glGetTexLevelParameterfv: *mut c_void,
    pub glGetTexLevelParameteriv: *mut c_void,
    pub glGetTexParameterIiv: *mut c_void,
    pub glGetTexParameterIuiv: *mut c_void,
    pub glGetTexParameterPointervAPPLE: *mut c_void,
    pub glGetTexParameterfv: *mut c_void,
    pub glGetTexParameteriv: *mut c_void,
    pub glGetTransformFeedbackVarying: *mut c_void,
    pub glGetUniformBlockIndex: *mut c_void,
    pub glGetUniformIndices: *mut c_void,
    pub glGetUniformLocation: *mut c_void,
    pub glGetUniformfv: *mut c_void,
    pub glGetUniformiv: *mut c_void,
    pub glGetUniformuiv: *mut c_void,
    pub glGetVertexAttribIiv: *mut c_void,
    pub glGetVertexAttribIuiv: *mut c_void,
    pub glGetVertexAttribPointerv: *mut c_void,
    pub glGetVertexAttribdv: *mut c_void,
    pub glGetVertexAttribfv: *mut c_void,
    pub glGetVertexAttribiv: *mut c_void,
    pub glHint: *mut c_void,
    pub glIndexMask: *mut c_void,
    pub glIndexPointer: *mut c_void,
    pub glIndexd: *mut c_void,
    pub glIndexdv: *mut c_void,
    pub glIndexf: *mut c_void,
    pub glIndexfv: *mut c_void,
    pub glIndexi: *mut c_void,
    pub glIndexiv: *mut c_void,
    pub glIndexs: *mut c_void,
    pub glIndexsv: *mut c_void,
    pub glIndexub: *mut c_void,
    pub glIndexubv: *mut c_void,
    pub glInitNames: *mut c_void,
    pub glInsertEventMarkerEXT: *mut c_void,
    pub glInterleavedArrays: *mut c_void,
    pub glInvalidateBufferData: *mut c_void,
    pub glInvalidateBufferSubData: *mut c_void,
    pub glInvalidateFramebuffer: *mut c_void,
    pub glInvalidateSubFramebuffer: *mut c_void,
    pub glInvalidateTexImage: *mut c_void,
    pub glInvalidateTexSubImage: *mut c_void,
    pub glIsBuffer: *mut c_void,
    pub glIsEnabled: *mut c_void,
    pub glIsEnabledi: *mut c_void,
    pub glIsFenceAPPLE: *mut c_void,
    pub glIsFramebuffer: *mut c_void,
    pub glIsList: *mut c_void,
    pub glIsProgram: *mut c_void,
    pub glIsQuery: *mut c_void,
    pub glIsRenderbuffer: *mut c_void,
    pub glIsSampler: *mut c_void,
    pub glIsShader: *mut c_void,
    pub glIsSync: *mut c_void,
    pub glIsTexture: *mut c_void,
    pub glIsVertexArray: *mut c_void,
    pub glIsVertexArrayAPPLE: *mut c_void,
    pub glLightModelf: *mut c_void,
    pub glLightModelfv: *mut c_void,
    pub glLightModeli: *mut c_void,
    pub glLightModeliv: *mut c_void,
    pub glLightf: *mut c_void,
    pub glLightfv: *mut c_void,
    pub glLighti: *mut c_void,
    pub glLightiv: *mut c_void,
    pub glLineStipple: *mut c_void,
    pub glLineWidth: *mut c_void,
    pub glLinkProgram: *mut c_void,
    pub glListBase: *mut c_void,
    pub glLoadIdentity: *mut c_void,
    pub glLoadMatrixd: *mut c_void,
    pub glLoadMatrixf: *mut c_void,
    pub glLoadName: *mut c_void,
    pub glLoadTransposeMatrixd: *mut c_void,
    pub glLoadTransposeMatrixf: *mut c_void,
    pub glLogicOp: *mut c_void,
    pub glMap1d: *mut c_void,
    pub glMap1f: *mut c_void,
    pub glMap2d: *mut c_void,
    pub glMap2f: *mut c_void,
    pub glMapBuffer: *mut c_void,
    pub glMapBufferRange: *mut c_void,
    pub glMapGrid1d: *mut c_void,
    pub glMapGrid1f: *mut c_void,
    pub glMapGrid2d: *mut c_void,
    pub glMapGrid2f: *mut c_void,
    pub glMaterialf: *mut c_void,
    pub glMaterialfv: *mut c_void,
    pub glMateriali: *mut c_void,
    pub glMaterialiv: *mut c_void,
    pub glMatrixMode: *mut c_void,
    pub glMultMatrixd: *mut c_void,
    pub glMultMatrixf: *mut c_void,
    pub glMultTransposeMatrixd: *mut c_void,
    pub glMultTransposeMatrixf: *mut c_void,
    pub glMultiDrawArrays: *mut c_void,
    pub glMultiDrawElements: *mut c_void,
    pub glMultiDrawElementsBaseVertex: *mut c_void,
    pub glMultiTexCoord1d: *mut c_void,
    pub glMultiTexCoord1dv: *mut c_void,
    pub glMultiTexCoord1f: *mut c_void,
    pub glMultiTexCoord1fv: *mut c_void,
    pub glMultiTexCoord1i: *mut c_void,
    pub glMultiTexCoord1iv: *mut c_void,
    pub glMultiTexCoord1s: *mut c_void,
    pub glMultiTexCoord1sv: *mut c_void,
    pub glMultiTexCoord2d: *mut c_void,
    pub glMultiTexCoord2dv: *mut c_void,
    pub glMultiTexCoord2f: *mut c_void,
    pub glMultiTexCoord2fv: *mut c_void,
    pub glMultiTexCoord2i: *mut c_void,
    pub glMultiTexCoord2iv: *mut c_void,
    pub glMultiTexCoord2s: *mut c_void,
    pub glMultiTexCoord2sv: *mut c_void,
    pub glMultiTexCoord3d: *mut c_void,
    pub glMultiTexCoord3dv: *mut c_void,
    pub glMultiTexCoord3f: *mut c_void,
    pub glMultiTexCoord3fv: *mut c_void,
    pub glMultiTexCoord3i: *mut c_void,
    pub glMultiTexCoord3iv: *mut c_void,
    pub glMultiTexCoord3s: *mut c_void,
    pub glMultiTexCoord3sv: *mut c_void,
    pub glMultiTexCoord4d: *mut c_void,
    pub glMultiTexCoord4dv: *mut c_void,
    pub glMultiTexCoord4f: *mut c_void,
    pub glMultiTexCoord4fv: *mut c_void,
    pub glMultiTexCoord4i: *mut c_void,
    pub glMultiTexCoord4iv: *mut c_void,
    pub glMultiTexCoord4s: *mut c_void,
    pub glMultiTexCoord4sv: *mut c_void,
    pub glMultiTexCoordP1ui: *mut c_void,
    pub glMultiTexCoordP1uiv: *mut c_void,
    pub glMultiTexCoordP2ui: *mut c_void,
    pub glMultiTexCoordP2uiv: *mut c_void,
    pub glMultiTexCoordP3ui: *mut c_void,
    pub glMultiTexCoordP3uiv: *mut c_void,
    pub glMultiTexCoordP4ui: *mut c_void,
    pub glMultiTexCoordP4uiv: *mut c_void,
    pub glNewList: *mut c_void,
    pub glNormal3b: *mut c_void,
    pub glNormal3bv: *mut c_void,
    pub glNormal3d: *mut c_void,
    pub glNormal3dv: *mut c_void,
    pub glNormal3f: *mut c_void,
    pub glNormal3fv: *mut c_void,
    pub glNormal3i: *mut c_void,
    pub glNormal3iv: *mut c_void,
    pub glNormal3s: *mut c_void,
    pub glNormal3sv: *mut c_void,
    pub glNormalP3ui: *mut c_void,
    pub glNormalP3uiv: *mut c_void,
    pub glNormalPointer: *mut c_void,
    pub glObjectLabel: *mut c_void,
    pub glObjectLabelKHR: *mut c_void,
    pub glObjectPtrLabel: *mut c_void,
    pub glObjectPtrLabelKHR: *mut c_void,
    pub glOrtho: *mut c_void,
    pub glPassThrough: *mut c_void,
    pub glPixelMapfv: *mut c_void,
    pub glPixelMapuiv: *mut c_void,
    pub glPixelMapusv: *mut c_void,
    pub glPixelStoref: *mut c_void,
    pub glPixelStorei: *mut c_void,
    pub glPixelTransferf: *mut c_void,
    pub glPixelTransferi: *mut c_void,
    pub glPixelZoom: *mut c_void,
    pub glPointParameterf: *mut c_void,
    pub glPointParameterfv: *mut c_void,
    pub glPointParameteri: *mut c_void,
    pub glPointParameteriv: *mut c_void,
    pub glPointSize: *mut c_void,
    pub glPolygonMode: *mut c_void,
    pub glPolygonOffset: *mut c_void,
    pub glPolygonStipple: *mut c_void,
    pub glPopAttrib: *mut c_void,
    pub glPopClientAttrib: *mut c_void,
    pub glPopDebugGroup: *mut c_void,
    pub glPopDebugGroupKHR: *mut c_void,
    pub glPopGroupMarkerEXT: *mut c_void,
    pub glPopMatrix: *mut c_void,
    pub glPopName: *mut c_void,
    pub glPrimitiveRestartIndex: *mut c_void,
    pub glPrioritizeTextures: *mut c_void,
    pub glProgramBinary: *mut c_void,
    pub glProgramParameteri: *mut c_void,
    pub glProvokingVertex: *mut c_void,
    pub glPushAttrib: *mut c_void,
    pub glPushClientAttrib: *mut c_void,
    pub glPushDebugGroup: *mut c_void,
    pub glPushDebugGroupKHR: *mut c_void,
    pub glPushGroupMarkerEXT: *mut c_void,
    pub glPushMatrix: *mut c_void,
    pub glPushName: *mut c_void,
    pub glQueryCounter: *mut c_void,
    pub glRasterPos2d: *mut c_void,
    pub glRasterPos2dv: *mut c_void,
    pub glRasterPos2f: *mut c_void,
    pub glRasterPos2fv: *mut c_void,
    pub glRasterPos2i: *mut c_void,
    pub glRasterPos2iv: *mut c_void,
    pub glRasterPos2s: *mut c_void,
    pub glRasterPos2sv: *mut c_void,
    pub glRasterPos3d: *mut c_void,
    pub glRasterPos3dv: *mut c_void,
    pub glRasterPos3f: *mut c_void,
    pub glRasterPos3fv: *mut c_void,
    pub glRasterPos3i: *mut c_void,
    pub glRasterPos3iv: *mut c_void,
    pub glRasterPos3s: *mut c_void,
    pub glRasterPos3sv: *mut c_void,
    pub glRasterPos4d: *mut c_void,
    pub glRasterPos4dv: *mut c_void,
    pub glRasterPos4f: *mut c_void,
    pub glRasterPos4fv: *mut c_void,
    pub glRasterPos4i: *mut c_void,
    pub glRasterPos4iv: *mut c_void,
    pub glRasterPos4s: *mut c_void,
    pub glRasterPos4sv: *mut c_void,
    pub glReadBuffer: *mut c_void,
    pub glReadPixels: *mut c_void,
    pub glRectd: *mut c_void,
    pub glRectdv: *mut c_void,
    pub glRectf: *mut c_void,
    pub glRectfv: *mut c_void,
    pub glRecti: *mut c_void,
    pub glRectiv: *mut c_void,
    pub glRects: *mut c_void,
    pub glRectsv: *mut c_void,
    pub glRenderMode: *mut c_void,
    pub glRenderbufferStorage: *mut c_void,
    pub glRenderbufferStorageMultisample: *mut c_void,
    pub glRotated: *mut c_void,
    pub glRotatef: *mut c_void,
    pub glSampleCoverage: *mut c_void,
    pub glSampleMaski: *mut c_void,
    pub glSamplerParameterIiv: *mut c_void,
    pub glSamplerParameterIuiv: *mut c_void,
    pub glSamplerParameterf: *mut c_void,
    pub glSamplerParameterfv: *mut c_void,
    pub glSamplerParameteri: *mut c_void,
    pub glSamplerParameteriv: *mut c_void,
    pub glScaled: *mut c_void,
    pub glScalef: *mut c_void,
    pub glScissor: *mut c_void,
    pub glSecondaryColor3b: *mut c_void,
    pub glSecondaryColor3bv: *mut c_void,
    pub glSecondaryColor3d: *mut c_void,
    pub glSecondaryColor3dv: *mut c_void,
    pub glSecondaryColor3f: *mut c_void,
    pub glSecondaryColor3fv: *mut c_void,
    pub glSecondaryColor3i: *mut c_void,
    pub glSecondaryColor3iv: *mut c_void,
    pub glSecondaryColor3s: *mut c_void,
    pub glSecondaryColor3sv: *mut c_void,
    pub glSecondaryColor3ub: *mut c_void,
    pub glSecondaryColor3ubv: *mut c_void,
    pub glSecondaryColor3ui: *mut c_void,
    pub glSecondaryColor3uiv: *mut c_void,
    pub glSecondaryColor3us: *mut c_void,
    pub glSecondaryColor3usv: *mut c_void,
    pub glSecondaryColorP3ui: *mut c_void,
    pub glSecondaryColorP3uiv: *mut c_void,
    pub glSecondaryColorPointer: *mut c_void,
    pub glSelectBuffer: *mut c_void,
    pub glSetFenceAPPLE: *mut c_void,
    pub glShadeModel: *mut c_void,
    pub glShaderSource: *mut c_void,
    pub glShaderStorageBlockBinding: *mut c_void,
    pub glStencilFunc: *mut c_void,
    pub glStencilFuncSeparate: *mut c_void,
    pub glStencilMask: *mut c_void,
    pub glStencilMaskSeparate: *mut c_void,
    pub glStencilOp: *mut c_void,
    pub glStencilOpSeparate: *mut c_void,
    pub glTestFenceAPPLE: *mut c_void,
    pub glTestObjectAPPLE: *mut c_void,
    pub glTexBuffer: *mut c_void,
    pub glTexCoord1d: *mut c_void,
    pub glTexCoord1dv: *mut c_void,
    pub glTexCoord1f: *mut c_void,
    pub glTexCoord1fv: *mut c_void,
    pub glTexCoord1i: *mut c_void,
    pub glTexCoord1iv: *mut c_void,
    pub glTexCoord1s: *mut c_void,
    pub glTexCoord1sv: *mut c_void,
    pub glTexCoord2d: *mut c_void,
    pub glTexCoord2dv: *mut c_void,
    pub glTexCoord2f: *mut c_void,
    pub glTexCoord2fv: *mut c_void,
    pub glTexCoord2i: *mut c_void,
    pub glTexCoord2iv: *mut c_void,
    pub glTexCoord2s: *mut c_void,
    pub glTexCoord2sv: *mut c_void,
    pub glTexCoord3d: *mut c_void,
    pub glTexCoord3dv: *mut c_void,
    pub glTexCoord3f: *mut c_void,
    pub glTexCoord3fv: *mut c_void,
    pub glTexCoord3i: *mut c_void,
    pub glTexCoord3iv: *mut c_void,
    pub glTexCoord3s: *mut c_void,
    pub glTexCoord3sv: *mut c_void,
    pub glTexCoord4d: *mut c_void,
    pub glTexCoord4dv: *mut c_void,
    pub glTexCoord4f: *mut c_void,
    pub glTexCoord4fv: *mut c_void,
    pub glTexCoord4i: *mut c_void,
    pub glTexCoord4iv: *mut c_void,
    pub glTexCoord4s: *mut c_void,
    pub glTexCoord4sv: *mut c_void,
    pub glTexCoordP1ui: *mut c_void,
    pub glTexCoordP1uiv: *mut c_void,
    pub glTexCoordP2ui: *mut c_void,
    pub glTexCoordP2uiv: *mut c_void,
    pub glTexCoordP3ui: *mut c_void,
    pub glTexCoordP3uiv: *mut c_void,
    pub glTexCoordP4ui: *mut c_void,
    pub glTexCoordP4uiv: *mut c_void,
    pub glTexCoordPointer: *mut c_void,
    pub glTexEnvf: *mut c_void,
    pub glTexEnvfv: *mut c_void,
    pub glTexEnvi: *mut c_void,
    pub glTexEnviv: *mut c_void,
    pub glTexGend: *mut c_void,
    pub glTexGendv: *mut c_void,
    pub glTexGenf: *mut c_void,
    pub glTexGenfv: *mut c_void,
    pub glTexGeni: *mut c_void,
    pub glTexGeniv: *mut c_void,
    pub glTexImage1D: *mut c_void,
    pub glTexImage2D: *mut c_void,
    pub glTexImage2DMultisample: *mut c_void,
    pub glTexImage3D: *mut c_void,
    pub glTexImage3DMultisample: *mut c_void,
    pub glTexParameterIiv: *mut c_void,
    pub glTexParameterIuiv: *mut c_void,
    pub glTexParameterf: *mut c_void,
    pub glTexParameterfv: *mut c_void,
    pub glTexParameteri: *mut c_void,
    pub glTexParameteriv: *mut c_void,
    pub glTexStorage1D: *mut c_void,
    pub glTexStorage2D: *mut c_void,
    pub glTexStorage3D: *mut c_void,
    pub glTexSubImage1D: *mut c_void,
    pub glTexSubImage2D: *mut c_void,
    pub glTexSubImage3D: *mut c_void,
    pub glTextureRangeAPPLE: *mut c_void,
    pub glTransformFeedbackVaryings: *mut c_void,
    pub glTranslated: *mut c_void,
    pub glTranslatef: *mut c_void,
    pub glUniform1f: *mut c_void,
    pub glUniform1fv: *mut c_void,
    pub glUniform1i: *mut c_void,
    pub glUniform1iv: *mut c_void,
    pub glUniform1ui: *mut c_void,
    pub glUniform1uiv: *mut c_void,
    pub glUniform2f: *mut c_void,
    pub glUniform2fv: *mut c_void,
    pub glUniform2i: *mut c_void,
    pub glUniform2iv: *mut c_void,
    pub glUniform2ui: *mut c_void,
    pub glUniform2uiv: *mut c_void,
    pub glUniform3f: *mut c_void,
    pub glUniform3fv: *mut c_void,
    pub glUniform3i: *mut c_void,
    pub glUniform3iv: *mut c_void,
    pub glUniform3ui: *mut c_void,
    pub glUniform3uiv: *mut c_void,
    pub glUniform4f: *mut c_void,
    pub glUniform4fv: *mut c_void,
    pub glUniform4i: *mut c_void,
    pub glUniform4iv: *mut c_void,
    pub glUniform4ui: *mut c_void,
    pub glUniform4uiv: *mut c_void,
    pub glUniformBlockBinding: *mut c_void,
    pub glUniformMatrix2fv: *mut c_void,
    pub glUniformMatrix2x3fv: *mut c_void,
    pub glUniformMatrix2x4fv: *mut c_void,
    pub glUniformMatrix3fv: *mut c_void,
    pub glUniformMatrix3x2fv: *mut c_void,
    pub glUniformMatrix3x4fv: *mut c_void,
    pub glUniformMatrix4fv: *mut c_void,
    pub glUniformMatrix4x2fv: *mut c_void,
    pub glUniformMatrix4x3fv: *mut c_void,
    pub glUnmapBuffer: *mut c_void,
    pub glUseProgram: *mut c_void,
    pub glValidateProgram: *mut c_void,
    pub glVertex2d: *mut c_void,
    pub glVertex2dv: *mut c_void,
    pub glVertex2f: *mut c_void,
    pub glVertex2fv: *mut c_void,
    pub glVertex2i: *mut c_void,
    pub glVertex2iv: *mut c_void,
    pub glVertex2s: *mut c_void,
    pub glVertex2sv: *mut c_void,
    pub glVertex3d: *mut c_void,
    pub glVertex3dv: *mut c_void,
    pub glVertex3f: *mut c_void,
    pub glVertex3fv: *mut c_void,
    pub glVertex3i: *mut c_void,
    pub glVertex3iv: *mut c_void,
    pub glVertex3s: *mut c_void,
    pub glVertex3sv: *mut c_void,
    pub glVertex4d: *mut c_void,
    pub glVertex4dv: *mut c_void,
    pub glVertex4f: *mut c_void,
    pub glVertex4fv: *mut c_void,
    pub glVertex4i: *mut c_void,
    pub glVertex4iv: *mut c_void,
    pub glVertex4s: *mut c_void,
    pub glVertex4sv: *mut c_void,
    pub glVertexAttrib1d: *mut c_void,
    pub glVertexAttrib1dv: *mut c_void,
    pub glVertexAttrib1f: *mut c_void,
    pub glVertexAttrib1fv: *mut c_void,
    pub glVertexAttrib1s: *mut c_void,
    pub glVertexAttrib1sv: *mut c_void,
    pub glVertexAttrib2d: *mut c_void,
    pub glVertexAttrib2dv: *mut c_void,
    pub glVertexAttrib2f: *mut c_void,
    pub glVertexAttrib2fv: *mut c_void,
    pub glVertexAttrib2s: *mut c_void,
    pub glVertexAttrib2sv: *mut c_void,
    pub glVertexAttrib3d: *mut c_void,
    pub glVertexAttrib3dv: *mut c_void,
    pub glVertexAttrib3f: *mut c_void,
    pub glVertexAttrib3fv: *mut c_void,
    pub glVertexAttrib3s: *mut c_void,
    pub glVertexAttrib3sv: *mut c_void,
    pub glVertexAttrib4Nbv: *mut c_void,
    pub glVertexAttrib4Niv: *mut c_void,
    pub glVertexAttrib4Nsv: *mut c_void,
    pub glVertexAttrib4Nub: *mut c_void,
    pub glVertexAttrib4Nubv: *mut c_void,
    pub glVertexAttrib4Nuiv: *mut c_void,
    pub glVertexAttrib4Nusv: *mut c_void,
    pub glVertexAttrib4bv: *mut c_void,
    pub glVertexAttrib4d: *mut c_void,
    pub glVertexAttrib4dv: *mut c_void,
    pub glVertexAttrib4f: *mut c_void,
    pub glVertexAttrib4fv: *mut c_void,
    pub glVertexAttrib4iv: *mut c_void,
    pub glVertexAttrib4s: *mut c_void,
    pub glVertexAttrib4sv: *mut c_void,
    pub glVertexAttrib4ubv: *mut c_void,
    pub glVertexAttrib4uiv: *mut c_void,
    pub glVertexAttrib4usv: *mut c_void,
    pub glVertexAttribDivisor: *mut c_void,
    pub glVertexAttribI1i: *mut c_void,
    pub glVertexAttribI1iv: *mut c_void,
    pub glVertexAttribI1ui: *mut c_void,
    pub glVertexAttribI1uiv: *mut c_void,
    pub glVertexAttribI2i: *mut c_void,
    pub glVertexAttribI2iv: *mut c_void,
    pub glVertexAttribI2ui: *mut c_void,
    pub glVertexAttribI2uiv: *mut c_void,
    pub glVertexAttribI3i: *mut c_void,
    pub glVertexAttribI3iv: *mut c_void,
    pub glVertexAttribI3ui: *mut c_void,
    pub glVertexAttribI3uiv: *mut c_void,
    pub glVertexAttribI4bv: *mut c_void,
    pub glVertexAttribI4i: *mut c_void,
    pub glVertexAttribI4iv: *mut c_void,
    pub glVertexAttribI4sv: *mut c_void,
    pub glVertexAttribI4ubv: *mut c_void,
    pub glVertexAttribI4ui: *mut c_void,
    pub glVertexAttribI4uiv: *mut c_void,
    pub glVertexAttribI4usv: *mut c_void,
    pub glVertexAttribIPointer: *mut c_void,
    pub glVertexAttribP1ui: *mut c_void,
    pub glVertexAttribP1uiv: *mut c_void,
    pub glVertexAttribP2ui: *mut c_void,
    pub glVertexAttribP2uiv: *mut c_void,
    pub glVertexAttribP3ui: *mut c_void,
    pub glVertexAttribP3uiv: *mut c_void,
    pub glVertexAttribP4ui: *mut c_void,
    pub glVertexAttribP4uiv: *mut c_void,
    pub glVertexAttribPointer: *mut c_void,
    pub glVertexP2ui: *mut c_void,
    pub glVertexP2uiv: *mut c_void,
    pub glVertexP3ui: *mut c_void,
    pub glVertexP3uiv: *mut c_void,
    pub glVertexP4ui: *mut c_void,
    pub glVertexP4uiv: *mut c_void,
    pub glVertexPointer: *mut c_void,
    pub glViewport: *mut c_void,
    pub glWaitSync: *mut c_void,
    pub glWindowPos2d: *mut c_void,
    pub glWindowPos2dv: *mut c_void,
    pub glWindowPos2f: *mut c_void,
    pub glWindowPos2fv: *mut c_void,
    pub glWindowPos2i: *mut c_void,
    pub glWindowPos2iv: *mut c_void,
    pub glWindowPos2s: *mut c_void,
    pub glWindowPos2sv: *mut c_void,
    pub glWindowPos3d: *mut c_void,
    pub glWindowPos3dv: *mut c_void,
    pub glWindowPos3f: *mut c_void,
    pub glWindowPos3fv: *mut c_void,
    pub glWindowPos3i: *mut c_void,
    pub glWindowPos3iv: *mut c_void,
    pub glWindowPos3s: *mut c_void,
    pub glWindowPos3sv: *mut c_void,
    pub glStartTilingQCOM: *mut c_void,
    pub glEndTilingQCOM: *mut c_void,
}

fn encode_ascii(input: &str) -> Vec<i8> {
    input
    .chars()
    .filter(|c| c.is_ascii())
    .map(|c| c as i8)
    .chain(Some(0).into_iter())
    .collect::<Vec<_>>()
}

pub unsafe fn cstr_from_ptr<'a>(ptr: *const c_char) -> &'a str {

    #[inline]
    unsafe fn strlen(mut s: *const c_char) -> usize {
        let mut result = 0;
        while *s != 0 {
            s = s.offset(1);
            result += 1;
        }
        result
    }

    let len = strlen(ptr);
    let ptr = ptr as *const u8; // c_char is always one byte, safe cast
    core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len as usize + 1))
}

#[cfg(feature = "trace")]
pub mod trace;

macro_rules! impl_gl_context {
    ($($opt:ident)?) => {
        $( $opt )? fn get_type(&self) -> GlType {
            let version_string = self.get_string(gl::VERSION);
            if version_string.contains("OpenGL ES") {
                GlType::GlEs
            } else {
                GlType::Gl
            }
        }

        $( $opt )? fn buffer_data_untyped(
            &self,
            target: GLenum,
            size: GLsizeiptr,
            data: *const GLvoid,
            usage: GLenum,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBufferData", &|| format!("target={}, size={}, data={:p}, usage={}", crate::trace::short(&target), crate::trace::short(&size), data, crate::trace::short(&usage)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glBufferData"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBufferData == ptr::null_mut() {
                _gl_impl_panic("glBufferData");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLsizeiptr, *const GLvoid, GLenum) = mem::transmute(self.glBufferData);
                (func)(target, size, data, usage)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn buffer_sub_data_untyped(
            &self,
            target: GLenum,
            offset: isize,
            size: GLsizeiptr,
            data: *const GLvoid,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBufferSubData", &|| format!("target={}, offset={}, size={}, data={:p}", crate::trace::short(&target), crate::trace::short(&offset), crate::trace::short(&size), data));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glBufferSubData"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBufferSubData == ptr::null_mut() {
                _gl_impl_panic("glBufferSubData");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, isize, GLsizeiptr, *const GLvoid) = mem::transmute(self.glBufferSubData);
                (func)(target, offset, size, data)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn map_buffer(&self, target: GLenum, access: GLbitfield) -> *mut GLvoid {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glMapBuffer", &|| format!("target={}, access={}", crate::trace::short(&target), crate::trace::short(&access)));
            let __r: *mut GLvoid = (|| -> *mut GLvoid {

            #[cfg(feature = "debug")] { _gl_print_debug("glMapBuffer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glMapBuffer == ptr::null_mut() {
                _gl_impl_panic("glMapBuffer");
                return ptr::null_mut();
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLbitfield) -> *mut GLvoid = mem::transmute(self.glMapBuffer);
                (func)(target, access)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn map_buffer_range(
            &self,
            target: GLenum,
            offset: GLintptr,
            length: GLsizeiptr,
            access: GLbitfield,
        ) -> *mut GLvoid {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glMapBufferRange", &|| format!("target={}, offset={}, length={}, access={}", crate::trace::short(&target), crate::trace::short(&offset), crate::trace::short(&length), crate::trace::short(&access)));
            let __r: *mut GLvoid = (|| -> *mut GLvoid {

            #[cfg(feature = "debug")] { _gl_print_debug("glMapBufferRange"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glMapBufferRange == ptr::null_mut() {
                _gl_impl_panic("glMapBufferRange");
                return ptr::null_mut();
            }

            unsafe {
                let func: extern "system" fn( GLenum, GLintptr, GLsizeiptr, GLbitfield) -> *mut GLvoid = mem::transmute(self.glMapBufferRange);
                (func)(target, offset, length, access)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn unmap_buffer(&self, target: GLenum) -> GLboolean {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUnmapBuffer", &|| format!("target={}", crate::trace::short(&target)));
            let __r: GLboolean = (|| -> GLboolean {

            #[cfg(feature = "debug")] { _gl_print_debug("glUnmapBuffer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUnmapBuffer == ptr::null_mut() {
                _gl_impl_panic("glUnmapBuffer");
                return 1;
            }
            unsafe {
                let func: extern "system" fn(GLenum) -> GLboolean = mem::transmute(self.glUnmapBuffer);
                (func)(target)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn tex_buffer(&self, target: GLenum, internal_format: GLenum, buffer: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glTexBuffer", &|| format!("target={}, internal_format={}, buffer={}", crate::trace::short(&target), crate::trace::short(&internal_format), crate::trace::short(&buffer)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glTexBuffer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glTexBuffer == ptr::null_mut() {
                _gl_impl_panic("glTexBuffer");
                return;
            }
            unsafe {
                let func: extern "system" fn(GLenum, GLenum, GLuint) = mem::transmute(self.glTexBuffer);
                (func)(target, internal_format, buffer)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn shader_source(&self, shader: GLuint, strings: &[&[u8]]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glShaderSource", &|| format!("shader={}, strings={}", crate::trace::short(&shader), format!("{} strings, lengths {:?}", strings.len(), strings.iter().map(|s| s.len()).collect::<Vec<_>>())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glShaderSource"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            let pointers: Vec<*const u8> = strings.iter().map(|string| (*string).as_ptr()).collect();
            let lengths: Vec<GLint> = strings.iter().map(|string| string.len() as GLint).collect();

            if self.glShaderSource == ptr::null_mut() {
                _gl_impl_panic("glShaderSource");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint, GLsizei, *const *const GLchar, *const GLint) = mem::transmute(self.glShaderSource);
                (func)(shader, pointers.len() as GLsizei, pointers.as_ptr() as *const *const GLchar, lengths.as_ptr())
            }

            mem::drop(lengths);
            mem::drop(pointers);
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn read_buffer(&self, mode: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glReadBuffer", &|| format!("mode={}", crate::trace::short(&mode)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glReadBuffer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glReadBuffer == ptr::null_mut() {
                _gl_impl_panic("glReadBuffer");
                return;
            }
            unsafe {
                let func: extern "system" fn(GLenum) = mem::transmute(self.glReadBuffer);
                (func)(mode)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn read_pixels_into_buffer(
            &self,
            x: GLint,
            y: GLint,
            width: GLsizei,
            height: GLsizei,
            format: GLenum,
            pixel_type: GLenum,
            dst_buffer: &mut [u8],
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glReadPixels", &|| format!("x={}, y={}, width={}, height={}, format={}, pixel_type={}, dst_buffer={}", crate::trace::short(&x), crate::trace::short(&y), crate::trace::short(&width), crate::trace::short(&height), crate::trace::short(&format), crate::trace::short(&pixel_type), format!("len {}", dst_buffer.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glReadPixels"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glReadPixels == ptr::null_mut() {
                _gl_impl_panic("glReadPixels");
                return;
            }


            #[cfg(feature = "debug")] { _gl_print_debug("glPixelStorei"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glPixelStorei == ptr::null_mut() {
                _gl_impl_panic("glPixelStorei");
                return;
            }

            unsafe {
                let glPixelStorei: extern "system" fn(GLenum, GLint) = mem::transmute(self.glPixelStorei);
                (glPixelStorei)(gl::PACK_ALIGNMENT, 1);

                let func: extern "system" fn(GLint, GLint, GLsizei, GLsizei, GLenum, GLenum, *mut u8) = mem::transmute(self.glReadPixels);
                (func)(x, y, width, height, format, pixel_type, dst_buffer.as_mut_ptr())
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[format!("dst_buffer={}", crate::trace::short(&dst_buffer[..dst_buffer.len().min(8)]))]);
            __r
        }

        $( $opt )? fn read_pixels(
            &self,
            x: GLint,
            y: GLint,
            width: GLsizei,
            height: GLsizei,
            format: GLenum,
            pixel_type: GLenum,
        ) -> Vec<u8> {

            use gl::*;

            let bit_depth = match pixel_type {

                UNSIGNED_BYTE |
                BYTE |
                UNSIGNED_BYTE_3_3_2 |
                UNSIGNED_BYTE_2_3_3_REV => 1,

                UNSIGNED_SHORT |
                UNSIGNED_SHORT_5_6_5 |
                UNSIGNED_SHORT_5_6_5_REV |
                UNSIGNED_SHORT_4_4_4_4 |
                UNSIGNED_SHORT_4_4_4_4_REV |
                UNSIGNED_SHORT_5_5_5_1 |
                UNSIGNED_SHORT_1_5_5_5_REV |
                SHORT => 2,

                UNSIGNED_INT |
                UNSIGNED_INT_8_8_8_8 |
                UNSIGNED_INT_8_8_8_8_REV |
                UNSIGNED_INT_10_10_10_2 |
                UNSIGNED_INT_2_10_10_10_REV |
                UNSIGNED_INT_24_8 |
                UNSIGNED_INT_10F_11F_11F_REV |
                UNSIGNED_INT_5_9_9_9_REV |
                INT => 4,

                HALF_FLOAT => 2,

                FLOAT |
                FLOAT_32_UNSIGNED_INT_24_8_REV => 4,

                _ => 0,
            };

            let mut v = vec![0;width as usize * height as usize * bit_depth];
            self.read_pixels_into_buffer(x, y, width, height, format, pixel_type, &mut v[..]);
            v
        }

        $( $opt )? unsafe fn read_pixels_into_pbo(
            &self,
            x: GLint,
            y: GLint,
            width: GLsizei,
            height: GLsizei,
            format: GLenum,
            pixel_type: GLenum,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glReadPixels", &|| format!("x={}, y={}, width={}, height={}, format={}, pixel_type={}", crate::trace::short(&x), crate::trace::short(&y), crate::trace::short(&width), crate::trace::short(&height), crate::trace::short(&format), crate::trace::short(&pixel_type)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glReadPixels"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glReadPixels == ptr::null_mut() {
                _gl_impl_panic("glReadPixels");
                return;
            }

            let func: extern "system" fn(GLint, GLint, GLsizei, GLsizei, GLenum, GLenum, *mut u8) = mem::transmute(self.glReadPixels);
            (func)(x, y, width, height, format, pixel_type, ptr::null_mut())
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn sample_coverage(&self, value: GLclampf, invert: bool) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glSampleCoverage", &|| format!("value={}, invert={}", crate::trace::short(&value), crate::trace::short(&invert)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glSampleCoverage"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glSampleCoverage == ptr::null_mut() {
                _gl_impl_panic("glSampleCoverage");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLclampf, bool) = mem::transmute(self.glSampleCoverage);
                (func)(value, invert)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn polygon_offset(&self, factor: GLfloat, units: GLfloat) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glPolygonOffset", &|| format!("factor={}, units={}", crate::trace::short(&factor), crate::trace::short(&units)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glPolygonOffset"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glPolygonOffset == ptr::null_mut() {
                _gl_impl_panic("glPolygonOffset");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLfloat, GLfloat) = mem::transmute(self.glPolygonOffset);
                (func)(factor, units)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn pixel_store_i(&self, name: GLenum, param: GLint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glPixelStorei", &|| format!("name={}, param={}", crate::trace::short(&name), crate::trace::short(&param)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glPixelStorei"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glPixelStorei == ptr::null_mut() {
                _gl_impl_panic("glPixelStorei");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLint) = mem::transmute(self.glPixelStorei);
                (func)(name, param)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn gen_buffers(&self, n: GLsizei) -> Vec<GLuint> {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGenBuffers", &|| format!("n={}", crate::trace::short(&n)));
            let __r: Vec<GLuint> = (|| -> Vec<GLuint> {

            #[cfg(feature = "debug")] { _gl_print_debug("glGenBuffers"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGenBuffers == ptr::null_mut() {
                _gl_impl_panic("glGenBuffers");
                return Vec::new();
            }

            let mut v = vec![0;n.max(0) as usize];
            unsafe {
                let func: extern "system" fn(GLsizei, *mut GLuint) = mem::transmute(self.glGenBuffers);
                (func)(n, v.as_mut_ptr());
            }
            v
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn gen_renderbuffers(&self, n: GLsizei) -> Vec<GLuint> {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGenRenderbuffers", &|| format!("n={}", crate::trace::short(&n)));
            let __r: Vec<GLuint> = (|| -> Vec<GLuint> {

            #[cfg(feature = "debug")] { _gl_print_debug("glGenRenderbuffers"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGenRenderbuffers == ptr::null_mut() {
                _gl_impl_panic("glGenRenderbuffers");
                return Vec::new();
            }

            let mut v = vec![0;n.max(0) as usize];
            unsafe {
                let func: extern "system" fn(GLsizei, *mut GLuint) = mem::transmute(self.glGenRenderbuffers);
                (func)(n, v.as_mut_ptr());
            }
            v
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn gen_framebuffers(&self, n: GLsizei) -> Vec<GLuint> {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGenFramebuffers", &|| format!("n={}", crate::trace::short(&n)));
            let __r: Vec<GLuint> = (|| -> Vec<GLuint> {

            #[cfg(feature = "debug")] { _gl_print_debug("glGenFramebuffers"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGenFramebuffers == ptr::null_mut() {
                _gl_impl_panic("glGenFramebuffers");
                return Vec::new();
            }

            let mut v = vec![0;n.max(0) as usize];
            unsafe {
                let func: extern "system" fn(GLsizei, *mut GLuint) = mem::transmute(self.glGenFramebuffers);
                (func)(n, v.as_mut_ptr());
            }
            v
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn gen_textures(&self, n: GLsizei) -> Vec<GLuint> {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGenTextures", &|| format!("n={}", crate::trace::short(&n)));
            let __r: Vec<GLuint> = (|| -> Vec<GLuint> {

            #[cfg(feature = "debug")] { _gl_print_debug("glGenTextures"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGenTextures == ptr::null_mut() {
                _gl_impl_panic("glGenTextures");
                return Vec::new();
            }

            let mut v = vec![0;n.max(0) as usize];
            unsafe {
                let func: extern "system" fn(GLsizei, *mut GLuint) = mem::transmute(self.glGenTextures);
                (func)(n, v.as_mut_ptr());
            }
            v
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn gen_vertex_arrays(&self, n: GLsizei) -> Vec<GLuint> {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGenVertexArrays", &|| format!("n={}", crate::trace::short(&n)));
            let __r: Vec<GLuint> = (|| -> Vec<GLuint> {

            #[cfg(feature = "debug")] { _gl_print_debug("glGenVertexArrays"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGenVertexArrays == ptr::null_mut() {
                _gl_impl_panic("glGenVertexArrays");
                return Vec::new();
            }

            let mut v = vec![0;n.max(0) as usize];
            unsafe {
                let func: extern "system" fn(GLsizei, *mut GLuint) = mem::transmute(self.glGenVertexArrays);
                (func)(n, v.as_mut_ptr());
            }
            v
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn gen_vertex_arrays_apple(&self, n: GLsizei) -> Vec<GLuint> {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGenVertexArraysAPPLE", &|| format!("n={}", crate::trace::short(&n)));
            let __r: Vec<GLuint> = (|| -> Vec<GLuint> {

            #[cfg(feature = "debug")] { _gl_print_debug("glGenVertexArraysAPPLE"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGenVertexArraysAPPLE == ptr::null_mut() {
                _gl_impl_panic("glGenVertexArraysAPPLE");
                return Vec::new();
            }

            let mut v = vec![0;n.max(0) as usize];
            unsafe {
                let func: extern "system" fn(GLsizei, *mut GLuint) = mem::transmute(self.glGenVertexArraysAPPLE);
                (func)(n, v.as_mut_ptr());
            }
            v
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn gen_queries(&self, n: GLsizei) -> Vec<GLuint> {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGenQueries", &|| format!("n={}", crate::trace::short(&n)));
            let __r: Vec<GLuint> = (|| -> Vec<GLuint> {

            #[cfg(feature = "debug")] { _gl_print_debug("glGenQueries"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGenQueries == ptr::null_mut() {
                _gl_impl_panic("glGenQueries");
                return Vec::new();
            }

            let mut v = vec![0;n.max(0) as usize];
            unsafe {
                let func: extern "system" fn(GLsizei, *mut GLuint) = mem::transmute(self.glGenQueries);
                (func)(n, v.as_mut_ptr());
            }
            v
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn begin_query(&self, target: GLenum, id: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBeginQuery", &|| format!("target={}, id={}", crate::trace::short(&target), crate::trace::short(&id)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glBeginQuery"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBeginQuery == ptr::null_mut() {
                _gl_impl_panic("glBeginQuery");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLuint) = mem::transmute(self.glBeginQuery);
                (func)(target, id)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn end_query(&self, target: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glEndQuery", &|| format!("target={}", crate::trace::short(&target)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glEndQuery"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glEndQuery == ptr::null_mut() {
                _gl_impl_panic("glEndQuery");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum) = mem::transmute(self.glEndQuery);
                (func)(target)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn query_counter(&self, id: GLuint, target: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glQueryCounter", &|| format!("id={}, target={}", crate::trace::short(&id), crate::trace::short(&target)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glQueryCounter"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glQueryCounter == ptr::null_mut() {
                _gl_impl_panic("glQueryCounter");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint, GLenum) = mem::transmute(self.glQueryCounter);
                (func)(id, target)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_query_object_iv(&self, id: GLuint, pname: GLenum) -> i32 {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetQueryObjectiv", &|| format!("id={}, pname={}", crate::trace::short(&id), crate::trace::short(&pname)));
            let __r: i32 = (|| -> i32 {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetQueryObjectiv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetQueryObjectiv == ptr::null_mut() {
                _gl_impl_panic("glGetQueryObjectiv");
                return 0;
            }

            unsafe {
                let func: extern "system" fn(GLuint, GLenum) -> i32 = mem::transmute(self.glGetQueryObjectiv);
                (func)(id, pname)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_query_object_uiv(&self, id: GLuint, pname: GLenum) -> u32 {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetQueryObjectuiv", &|| format!("id={}, pname={}", crate::trace::short(&id), crate::trace::short(&pname)));
            let __r: u32 = (|| -> u32 {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetQueryObjectuiv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetQueryObjectuiv == ptr::null_mut() {
                _gl_impl_panic("glGetQueryObjectuiv");
                return 0;
            }

            unsafe {
                let func: extern "system" fn(GLuint, GLenum) -> u32 = mem::transmute(self.glGetQueryObjectuiv);
                (func)(id, pname)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_query_object_i64v(&self, id: GLuint, pname: GLenum) -> i64 {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetQueryObjecti64v", &|| format!("id={}, pname={}", crate::trace::short(&id), crate::trace::short(&pname)));
            let __r: i64 = (|| -> i64 {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetQueryObjecti64v"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetQueryObjecti64v == ptr::null_mut() {
                _gl_impl_panic("glGetQueryObjecti64v");
                return 0;
            }

            unsafe {
                let func: extern "system" fn(GLuint, GLenum) -> i64 = mem::transmute(self.glGetQueryObjecti64v);
                (func)(id, pname)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_query_object_ui64v(&self, id: GLuint, pname: GLenum) -> u64 {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetQueryObjectui64v", &|| format!("id={}, pname={}", crate::trace::short(&id), crate::trace::short(&pname)));
            let __r: u64 = (|| -> u64 {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetQueryObjectui64v"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetQueryObjectui64v == ptr::null_mut() {
                _gl_impl_panic("glGetQueryObjectui64v");
                return 0;
            }

            unsafe {
                let func: extern "system" fn(GLuint, GLenum) -> u64 = mem::transmute(self.glGetQueryObjectui64v);
                (func)(id, pname)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn delete_queries(&self, queries: &[GLuint]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDeleteQueries", &|| format!("queries={}", crate::trace::short(&queries)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glDeleteQueries"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDeleteQueries == ptr::null_mut() {
                _gl_impl_panic("glDeleteQueries");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLsizei, *const GLuint) = mem::transmute(self.glDeleteQueries);
                (func)(queries.len() as GLsizei, queries.as_ptr())
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn delete_vertex_arrays(&self, vertex_arrays: &[GLuint]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDeleteVertexArrays", &|| format!("vertex_arrays={}", crate::trace::short(&vertex_arrays)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glDeleteVertexArrays"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDeleteVertexArrays == ptr::null_mut() {
                _gl_impl_panic("glDeleteVertexArrays");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLsizei, *const GLuint) = mem::transmute(self.glDeleteVertexArrays);
                (func)(vertex_arrays.len() as GLsizei, vertex_arrays.as_ptr())
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn delete_vertex_arrays_apple(&self, vertex_arrays: &[GLuint]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDeleteVertexArraysAPPLE", &|| format!("vertex_arrays={}", crate::trace::short(&vertex_arrays)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glDeleteVertexArraysAPPLE"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDeleteVertexArraysAPPLE == ptr::null_mut() {
                _gl_impl_panic("glDeleteVertexArraysAPPLE");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLsizei, *const GLuint) = mem::transmute(self.glDeleteVertexArraysAPPLE);
                (func)(vertex_arrays.len() as GLsizei, vertex_arrays.as_ptr())
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn delete_buffers(&self, buffers: &[GLuint]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDeleteBuffers", &|| format!("buffers={}", crate::trace::short(&buffers)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glDeleteBuffers"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDeleteBuffers == ptr::null_mut() {
                _gl_impl_panic("glDeleteBuffers");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLsizei, *const GLuint) = mem::transmute(self.glDeleteBuffers);
                (func)(buffers.len() as GLsizei, buffers.as_ptr())
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn delete_renderbuffers(&self, renderbuffers: &[GLuint]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDeleteRenderbuffers", &|| format!("renderbuffers={}", crate::trace::short(&renderbuffers)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glDeleteRenderbuffers"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDeleteRenderbuffers == ptr::null_mut() {
                _gl_impl_panic("glDeleteRenderbuffers");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLsizei, *const GLuint) = mem::transmute(self.glDeleteRenderbuffers);
                (func)(renderbuffers.len() as GLsizei, renderbuffers.as_ptr())
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn delete_framebuffers(&self, framebuffers: &[GLuint]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDeleteFramebuffers", &|| format!("framebuffers={}", crate::trace::short(&framebuffers)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glDeleteFramebuffers"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDeleteFramebuffers == ptr::null_mut() {
                _gl_impl_panic("glDeleteFramebuffers");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLsizei, *const GLuint) = mem::transmute(self.glDeleteFramebuffers);
                (func)(framebuffers.len() as GLsizei, framebuffers.as_ptr())
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn delete_textures(&self, textures: &[GLuint]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDeleteTextures", &|| format!("textures={}", crate::trace::short(&textures)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glDeleteTextures"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDeleteTextures == ptr::null_mut() {
                _gl_impl_panic("glDeleteTextures");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLsizei, *const GLuint) = mem::transmute(self.glDeleteTextures);
                (func)(textures.len() as GLsizei, textures.as_ptr())
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn framebuffer_renderbuffer(
            &self,
            target: GLenum,
            attachment: GLenum,
            renderbuffertarget: GLenum,
            renderbuffer: GLuint,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glFramebufferRenderbuffer", &|| format!("target={}, attachment={}, renderbuffertarget={}, renderbuffer={}", crate::trace::short(&target), crate::trace::short(&attachment), crate::trace::short(&renderbuffertarget), crate::trace::short(&renderbuffer)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glFramebufferRenderbuffer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glFramebufferRenderbuffer == ptr::null_mut() {
                _gl_impl_panic("glFramebufferRenderbuffer");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLenum, GLenum, GLuint) = mem::transmute(self.glFramebufferRenderbuffer);
                (func)( target, attachment, renderbuffertarget, renderbuffer)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn renderbuffer_storage(
            &self,
            target: GLenum,
            internalformat: GLenum,
            width: GLsizei,
            height: GLsizei,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glRenderbufferStorage", &|| format!("target={}, internalformat={}, width={}, height={}", crate::trace::short(&target), crate::trace::short(&internalformat), crate::trace::short(&width), crate::trace::short(&height)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glRenderbufferStorage"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glRenderbufferStorage == ptr::null_mut() {
                _gl_impl_panic("glRenderbufferStorage");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLenum, GLsizei, GLsizei) = mem::transmute(self.glRenderbufferStorage);
                (func)(target, internalformat, width, height)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn depth_func(&self, func: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDepthFunc", &|| format!("func={}", crate::trace::short(&func)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glDepthFunc"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDepthFunc == ptr::null_mut() {
                _gl_impl_panic("glDepthFunc");
                return;
            }

            unsafe {
                let glDepthFunc: extern "system" fn(GLenum) = mem::transmute(self.glDepthFunc);
                (glDepthFunc)(func)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn active_texture(&self, texture: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glActiveTexture", &|| format!("texture={}", crate::trace::short(&texture)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glActiveTexture"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glActiveTexture == ptr::null_mut() {
                _gl_impl_panic("glActiveTexture");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum) = mem::transmute(self.glActiveTexture);
                (func)(texture)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn attach_shader(&self, program: GLuint, shader: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glAttachShader", &|| format!("program={}, shader={}", crate::trace::short(&program), crate::trace::short(&shader)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glAttachShader"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glAttachShader == ptr::null_mut() {
                _gl_impl_panic("glAttachShader");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint, GLuint) = mem::transmute(self.glAttachShader);
                (func)(program, shader)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn bind_attrib_location(&self, program: GLuint, index: GLuint, name: &str) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBindAttribLocation", &|| format!("program={}, index={}, name={}", crate::trace::short(&program), crate::trace::short(&index), crate::trace::short(&name)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glBindAttribLocation"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBindAttribLocation == ptr::null_mut() {
                _gl_impl_panic("glBindAttribLocation");
                return;
            }

            let cstr = encode_ascii(name);

            unsafe {
                let func: extern "system" fn(GLuint, GLuint, *const c_char) = mem::transmute(self.glBindAttribLocation);
                (func)(program, index, cstr.as_ptr())
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? unsafe fn get_uniform_iv(&self, program: GLuint, location: GLint, result: &mut [GLint]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetUniformiv", &|| format!("program={}, location={}, result={}", crate::trace::short(&program), crate::trace::short(&location), format!("len {}", result.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetUniformiv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetUniformiv == ptr::null_mut() {
                _gl_impl_panic("glGetUniformiv");
                return;
            }

            let func: extern "system" fn(GLuint, GLint, *mut GLint) = mem::transmute(self.glGetUniformiv);
            (func)(program, location, result.as_mut_ptr())
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[format!("result={}", crate::trace::short(&result[..result.len().min(8)]))]);
            __r
        }

        $( $opt )? unsafe fn get_uniform_fv(&self, program: GLuint, location: GLint, result: &mut [GLfloat]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetUniformfv", &|| format!("program={}, location={}, result={}", crate::trace::short(&program), crate::trace::short(&location), format!("len {}", result.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetUniformfv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetUniformfv == ptr::null_mut() {
                _gl_impl_panic("glGetUniformfv");
                return;
            }

            let func: extern "system" fn(GLuint, GLint, *mut GLfloat) = mem::transmute(self.glGetUniformfv);
            (func)(program, location, result.as_mut_ptr())
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[format!("result={}", crate::trace::short(&result[..result.len().min(8)]))]);
            __r
        }

        $( $opt )? fn get_uniform_block_index(&self, program: GLuint, name: &str) -> GLuint {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetUniformBlockIndex", &|| format!("program={}, name={}", crate::trace::short(&program), crate::trace::short(&name)));
            let __r: GLuint = (|| -> GLuint {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetUniformBlockIndex"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetUniformBlockIndex == ptr::null_mut() {
                _gl_impl_panic("glGetUniformBlockIndex");
                return 0;
            }

            let cstr = encode_ascii(name);

            unsafe {
                let func: extern "system" fn(GLuint, *const c_char) -> GLuint = mem::transmute(self.glGetUniformBlockIndex);
                (func)(program, cstr.as_ptr())
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        // ---------------------------------------------------------------------------------------------------------------------------
        // ---------------------------------------------------------------------------------------------------------------------------
        // ---------------------------------------------------------------------------------------------------------------------------
        // ---------------------------------------------------------------------------------------------------------------------------

        $( $opt )? fn get_uniform_indices(&self, program: GLuint, names: &[&str]) -> Vec<GLuint> {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetUniformIndices", &|| format!("program={}, names={}", crate::trace::short(&program), crate::trace::short(&names)));
            let __r: Vec<GLuint> = (|| -> Vec<GLuint> {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetUniformIndices"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetUniformIndices == ptr::null_mut() {
                _gl_impl_panic("glGetUniformIndices");
                return Vec::new();
            }

            let c_strings: Vec<Vec<i8>> = names.iter().map(|n| encode_ascii(*n)).collect();
            let pointers: Vec<*const GLchar> = c_strings.iter().map(|string| string.as_ptr()).collect();
            let mut result = vec![0;c_strings.len()];
            unsafe {
                let func: extern "system" fn(GLuint, GLsizei, *const *const GLchar, *const GLuint) -> GLuint = mem::transmute(self.glGetUniformIndices);
                (func)(
                    program,
                    pointers.len() as GLsizei,
                    pointers.as_ptr(),
                    result.as_mut_ptr(),
                );
            }
            result
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn bind_buffer_base(&self, target: GLenum, index: GLuint, buffer: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBindBufferBase", &|| format!("target={}, index={}, buffer={}", crate::trace::short(&target), crate::trace::short(&index), crate::trace::short(&buffer)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glBindBufferBase"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBindBufferBase == ptr::null_mut() {
                _gl_impl_panic("glBindBufferBase");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLuint, GLuint) = mem::transmute(self.glBindBufferBase);
                (func)(target, index, buffer);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn bind_buffer_range(
            &self,
            target: GLenum,
            index: GLuint,
            buffer: GLuint,
            offset: GLintptr,
            size: GLsizeiptr,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBindBufferRange", &|| format!("target={}, index={}, buffer={}, offset={}, size={}", crate::trace::short(&target), crate::trace::short(&index), crate::trace::short(&buffer), crate::trace::short(&offset), crate::trace::short(&size)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glBindBufferRange"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBindBufferRange == ptr::null_mut() {
                _gl_impl_panic("glBindBufferRange");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLuint, GLuint, GLintptr, GLsizeiptr) = mem::transmute(self.glBindBufferRange);
                (func)(target, index, buffer, offset, size);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_block_binding(
            &self,
            program: GLuint,
            uniform_block_index: GLuint,
            uniform_block_binding: GLuint,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniformBlockBinding", &|| format!("program={}, uniform_block_index={}, uniform_block_binding={}", crate::trace::short(&program), crate::trace::short(&uniform_block_index), crate::trace::short(&uniform_block_binding)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniformBlockBinding"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniformBlockBinding == ptr::null_mut() {
                _gl_impl_panic("glUniformBlockBinding");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint, GLuint, GLuint) = mem::transmute(self.glUniformBlockBinding);
                (func)(program, uniform_block_index, uniform_block_binding);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn bind_buffer(&self, target: GLenum, buffer: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBindBuffer", &|| format!("target={}, buffer={}", crate::trace::short(&target), crate::trace::short(&buffer)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glBindBuffer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBindBuffer == ptr::null_mut() {
                _gl_impl_panic("glBindBuffer");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLuint) = mem::transmute(self.glBindBuffer);
                (func)(target, buffer);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn bind_vertex_array(&self, vao: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBindVertexArray", &|| format!("vao={}", crate::trace::short(&vao)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glBindVertexArray"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBindVertexArray == ptr::null_mut() {
                _gl_impl_panic("glBindVertexArray");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint) = mem::transmute(self.glBindVertexArray);
                (func)(vao);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn bind_vertex_array_apple(&self, vao: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBindVertexArrayAPPLE", &|| format!("vao={}", crate::trace::short(&vao)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glBindVertexArrayAPPLE"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBindVertexArrayAPPLE == ptr::null_mut() {
                _gl_impl_panic("glBindVertexArrayAPPLE");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint) = mem::transmute(self.glBindVertexArrayAPPLE);
                (func)(vao)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn bind_renderbuffer(&self, target: GLenum, renderbuffer: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBindRenderbuffer", &|| format!("target={}, renderbuffer={}", crate::trace::short(&target), crate::trace::short(&renderbuffer)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glBindRenderbuffer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBindRenderbuffer == ptr::null_mut() {
                _gl_impl_panic("glBindRenderbuffer");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLuint) = mem::transmute(self.glBindRenderbuffer);
                (func)(target, renderbuffer);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn bind_framebuffer(&self, target: GLenum, framebuffer: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBindFramebuffer", &|| format!("target={}, framebuffer={}", crate::trace::short(&target), crate::trace::short(&framebuffer)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glBindFramebuffer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBindFramebuffer == ptr::null_mut() {
                _gl_impl_panic("glBindFramebuffer");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLuint) = mem::transmute(self.glBindFramebuffer);
                (func)(target, framebuffer);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn bind_texture(&self, target: GLenum, texture: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBindTexture", &|| format!("target={}, texture={}", crate::trace::short(&target), crate::trace::short(&texture)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glBindTexture"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBindTexture == ptr::null_mut() {
                _gl_impl_panic("glBindTexture");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLuint) = mem::transmute(self.glBindTexture);
                (func)(target, texture);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn draw_buffers(&self, bufs: &[GLenum]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDrawBuffers", &|| format!("bufs={}", crate::trace::short(&bufs)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glDrawBuffers"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDrawBuffers == ptr::null_mut() {
                _gl_impl_panic("glDrawBuffers");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLsizei, *const GLenum) = mem::transmute(self.glDrawBuffers);
                (func)(bufs.len() as GLsizei, bufs.as_ptr());
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        // FIXME: Does not verify buffer size -- unsafe!
        $( $opt )? fn tex_image_2d(
            &self,
            target: GLenum,
            level: GLint,
            internal_format: GLint,
            width: GLsizei,
            height: GLsizei,
            border: GLint,
            format: GLenum,
            ty: GLenum,
            opt_data: Option<&[u8]>,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glTexImage2D", &|| format!("target={}, level={}, internal_format={}, width={}, height={}, border={}, format={}, ty={}, opt_data={}", crate::trace::short(&target), crate::trace::short(&level), crate::trace::short(&internal_format), crate::trace::short(&width), crate::trace::short(&height), crate::trace::short(&border), crate::trace::short(&format), crate::trace::short(&ty), match &opt_data { Some(s) => format!("len {}", s.len()), None => "none".to_string() }));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glTexImage2D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glTexImage2D == ptr::null_mut() {
                _gl_impl_panic("glTexImage2D");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLint, GLint, GLsizei, GLsizei, GLint, GLenum, GLenum, *const GLvoid) = mem::transmute(self.glTexImage2D);

                match opt_data {
                    Some(data) => {
                        (func)(
                            target,
                            level,
                            internal_format,
                            width,
                            height,
                            border,
                            format,
                            ty,
                            data.as_ptr() as *const GLvoid,
                        );
                    },
                    None => {
                        (func)(
                            target,
                            level,
                            internal_format,
                            width,
                            height,
                            border,
                            format,
                            ty,
                            ptr::null(),
                        );
                    },
                }
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn compressed_tex_image_2d(
            &self,
            target: GLenum,
            level: GLint,
            internal_format: GLenum,
            width: GLsizei,
            height: GLsizei,
            border: GLint,
            data: &[u8],
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glCompressedTexImage2D", &|| format!("target={}, level={}, internal_format={}, width={}, height={}, border={}, data={}", crate::trace::short(&target), crate::trace::short(&level), crate::trace::short(&internal_format), crate::trace::short(&width), crate::trace::short(&height), crate::trace::short(&border), format!("len {}", data.len())));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glCompressedTexImage2D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glCompressedTexImage2D == ptr::null_mut() {
                _gl_impl_panic("glCompressedTexImage2D");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLint, GLenum, GLsizei, GLsizei, GLint, GLsizei, *const GLvoid) = mem::transmute(self.glCompressedTexImage2D);
                (func)(
                    target,
                    level,
                    internal_format,
                    width,
                    height,
                    border,
                    data.len() as GLsizei,
                    data.as_ptr() as *const GLvoid,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn compressed_tex_sub_image_2d(
            &self,
            target: GLenum,
            level: GLint,
            xoffset: GLint,
            yoffset: GLint,
            width: GLsizei,
            height: GLsizei,
            format: GLenum,
            data: &[u8],
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glCompressedTexSubImage2D", &|| format!("target={}, level={}, xoffset={}, yoffset={}, width={}, height={}, format={}, data={}", crate::trace::short(&target), crate::trace::short(&level), crate::trace::short(&xoffset), crate::trace::short(&yoffset), crate::trace::short(&width), crate::trace::short(&height), crate::trace::short(&format), format!("len {}", data.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glCompressedTexSubImage2D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glCompressedTexSubImage2D == ptr::null_mut() {
                _gl_impl_panic("glCompressedTexSubImage2D");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLint, GLint, GLint, GLsizei, GLsizei, GLenum, GLsizei, *const GLvoid) = mem::transmute(self.glCompressedTexSubImage2D);
                (func)(
                    target,
                    level,
                    xoffset,
                    yoffset,
                    width,
                    height,
                    format,
                    data.len() as GLsizei,
                    data.as_ptr() as *const GLvoid,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        // FIXME: Does not verify buffer size -- unsafe!
        $( $opt )? fn tex_image_3d(
            &self,
            target: GLenum,
            level: GLint,
            internal_format: GLint,
            width: GLsizei,
            height: GLsizei,
            depth: GLsizei,
            border: GLint,
            format: GLenum,
            ty: GLenum,
            opt_data: Option<&[u8]>,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glTexImage3D", &|| format!("target={}, level={}, internal_format={}, width={}, height={}, depth={}, border={}, format={}, ty={}, opt_data={}", crate::trace::short(&target), crate::trace::short(&level), crate::trace::short(&internal_format), crate::trace::short(&width), crate::trace::short(&height), crate::trace::short(&depth), crate::trace::short(&border), crate::trace::short(&format), crate::trace::short(&ty), match &opt_data { Some(s) => format!("len {}", s.len()), None => "none".to_string() }));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glTexImage3D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glTexImage3D == ptr::null_mut() {
                _gl_impl_panic("glTexImage3D");
                return;
            }

            unsafe {
                let pdata = match opt_data {
                    Some(data) => mem::transmute(data.as_ptr()),
                    None => ptr::null(),
                };
                let func: extern "system" fn(GLenum, GLint, GLint, GLsizei, GLsizei, GLsizei, GLint, GLenum, GLenum, *const GLvoid) = mem::transmute(self.glTexImage3D);
                (func)(
                    target,
                    level,
                    internal_format,
                    width,
                    height,
                    depth,
                    border,
                    format,
                    ty,
                    pdata,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn copy_tex_image_2d(
            &self,
            target: GLenum,
            level: GLint,
            internal_format: GLenum,
            x: GLint,
            y: GLint,
            width: GLsizei,
            height: GLsizei,
            border: GLint,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glCopyTexImage2D", &|| format!("target={}, level={}, internal_format={}, x={}, y={}, width={}, height={}, border={}", crate::trace::short(&target), crate::trace::short(&level), crate::trace::short(&internal_format), crate::trace::short(&x), crate::trace::short(&y), crate::trace::short(&width), crate::trace::short(&height), crate::trace::short(&border)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glCopyTexImage2D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glCopyTexImage2D == ptr::null_mut() {
                _gl_impl_panic("glCopyTexImage2D");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum,GLint,GLenum,GLint,GLint,GLsizei,GLsizei,GLint) = mem::transmute(self.glCopyTexImage2D);
                (func)(
                    target,
                    level,
                    internal_format,
                    x,
                    y,
                    width,
                    height,
                    border,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn copy_tex_sub_image_2d(
            &self,
            target: GLenum,
            level: GLint,
            xoffset: GLint,
            yoffset: GLint,
            x: GLint,
            y: GLint,
            width: GLsizei,
            height: GLsizei,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glCopyTexSubImage2D", &|| format!("target={}, level={}, xoffset={}, yoffset={}, x={}, y={}, width={}, height={}", crate::trace::short(&target), crate::trace::short(&level), crate::trace::short(&xoffset), crate::trace::short(&yoffset), crate::trace::short(&x), crate::trace::short(&y), crate::trace::short(&width), crate::trace::short(&height)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glCopyTexSubImage2D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glCopyTexSubImage2D == ptr::null_mut() {
                _gl_impl_panic("glCopyTexSubImage2D");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLint, GLint, GLint, GLint, GLint, GLsizei, GLsizei) = mem::transmute(self.glCopyTexSubImage2D);
                (func)(target, level, xoffset, yoffset, x, y, width, height);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn copy_tex_sub_image_3d(
            &self,
            target: GLenum,
            level: GLint,
            xoffset: GLint,
            yoffset: GLint,
            zoffset: GLint,
            x: GLint,
            y: GLint,
            width: GLsizei,
            height: GLsizei,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glCopyTexSubImage3D", &|| format!("target={}, level={}, xoffset={}, yoffset={}, zoffset={}, x={}, y={}, width={}, height={}", crate::trace::short(&target), crate::trace::short(&level), crate::trace::short(&xoffset), crate::trace::short(&yoffset), crate::trace::short(&zoffset), crate::trace::short(&x), crate::trace::short(&y), crate::trace::short(&width), crate::trace::short(&height)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glCopyTexSubImage3D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glCopyTexSubImage3D == ptr::null_mut() {
                _gl_impl_panic("glCopyTexSubImage3D");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLint, GLint, GLint, GLint, GLint, GLint, GLsizei, GLsizei) = mem::transmute(self.glCopyTexSubImage3D);
                (func)(
                    target, level, xoffset, yoffset, zoffset, x, y, width, height,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn tex_sub_image_2d(
            &self,
            target: GLenum,
            level: GLint,
            xoffset: GLint,
            yoffset: GLint,
            width: GLsizei,
            height: GLsizei,
            format: GLenum,
            ty: GLenum,
            data: &[u8],
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glTexSubImage2D", &|| format!("target={}, level={}, xoffset={}, yoffset={}, width={}, height={}, format={}, ty={}, data={}", crate::trace::short(&target), crate::trace::short(&level), crate::trace::short(&xoffset), crate::trace::short(&yoffset), crate::trace::short(&width), crate::trace::short(&height), crate::trace::short(&format), crate::trace::short(&ty), format!("len {}", data.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glTexSubImage2D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glTexSubImage2D == ptr::null_mut() {
                _gl_impl_panic("glTexSubImage2D");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLint, GLint, GLint, GLsizei, GLsizei, GLenum, GLenum, *const c_void) = mem::transmute(self.glTexSubImage2D);
                (func)(
                    target,
                    level,
                    xoffset,
                    yoffset,
                    width,
                    height,
                    format,
                    ty,
                    data.as_ptr() as *const c_void,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn tex_sub_image_2d_pbo(
            &self,
            target: GLenum,
            level: GLint,
            xoffset: GLint,
            yoffset: GLint,
            width: GLsizei,
            height: GLsizei,
            format: GLenum,
            ty: GLenum,
            offset: usize,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glTexSubImage2D", &|| format!("target={}, level={}, xoffset={}, yoffset={}, width={}, height={}, format={}, ty={}, offset={}", crate::trace::short(&target), crate::trace::short(&level), crate::trace::short(&xoffset), crate::trace::short(&yoffset), crate::trace::short(&width), crate::trace::short(&height), crate::trace::short(&format), crate::trace::short(&ty), crate::trace::short(&offset)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glTexSubImage2D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glTexSubImage2D == ptr::null_mut() {
                _gl_impl_panic("glTexSubImage2D");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLint, GLint, GLint, GLsizei, GLsizei, GLenum, GLenum, *const c_void) = mem::transmute(self.glTexSubImage2D);
                (func)(
                    target,
                    level,
                    xoffset,
                    yoffset,
                    width,
                    height,
                    format,
                    ty,
                    offset as *const c_void,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn tex_sub_image_3d(
            &self,
            target: GLenum,
            level: GLint,
            xoffset: GLint,
            yoffset: GLint,
            zoffset: GLint,
            width: GLsizei,
            height: GLsizei,
            depth: GLsizei,
            format: GLenum,
            ty: GLenum,
            data: &[u8],
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glTexSubImage3D", &|| format!("target={}, level={}, xoffset={}, yoffset={}, zoffset={}, width={}, height={}, depth={}, format={}, ty={}, data={}", crate::trace::short(&target), crate::trace::short(&level), crate::trace::short(&xoffset), crate::trace::short(&yoffset), crate::trace::short(&zoffset), crate::trace::short(&width), crate::trace::short(&height), crate::trace::short(&depth), crate::trace::short(&format), crate::trace::short(&ty), format!("len {}", data.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glTexSubImage3D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glTexSubImage3D == ptr::null_mut() {
                _gl_impl_panic("glTexSubImage3D");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLint, GLint, GLint, GLint, GLsizei, GLsizei, GLsizei, GLenum, GLenum, *const c_void) = mem::transmute(self.glTexSubImage3D);
                (func)(
                    target,
                    level,
                    xoffset,
                    yoffset,
                    zoffset,
                    width,
                    height,
                    depth,
                    format,
                    ty,
                    data.as_ptr() as *const c_void,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn tex_sub_image_3d_pbo(
            &self,
            target: GLenum,
            level: GLint,
            xoffset: GLint,
            yoffset: GLint,
            zoffset: GLint,
            width: GLsizei,
            height: GLsizei,
            depth: GLsizei,
            format: GLenum,
            ty: GLenum,
            offset: usize,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glTexSubImage3D", &|| format!("target={}, level={}, xoffset={}, yoffset={}, zoffset={}, width={}, height={}, depth={}, format={}, ty={}, offset={}", crate::trace::short(&target), crate::trace::short(&level), crate::trace::short(&xoffset), crate::trace::short(&yoffset), crate::trace::short(&zoffset), crate::trace::short(&width), crate::trace::short(&height), crate::trace::short(&depth), crate::trace::short(&format), crate::trace::short(&ty), crate::trace::short(&offset)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glTexSubImage3D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glTexSubImage3D == ptr::null_mut() {
                _gl_impl_panic("glTexSubImage3D");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLint, GLint, GLint, GLint, GLsizei, GLsizei, GLsizei, GLenum, GLenum, *const c_void) = mem::transmute(self.glTexSubImage3D);
                (func)(
                    target,
                    level,
                    xoffset,
                    yoffset,
                    zoffset,
                    width,
                    height,
                    depth,
                    format,
                    ty,
                    offset as *const c_void,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn tex_storage_2d(
            &self,
            target: GLenum,
            levels: GLint,
            internal_format: GLenum,
            width: GLsizei,
            height: GLsizei,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glTexStorage2D", &|| format!("target={}, levels={}, internal_format={}, width={}, height={}", crate::trace::short(&target), crate::trace::short(&levels), crate::trace::short(&internal_format), crate::trace::short(&width), crate::trace::short(&height)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glTexStorage2D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glTexStorage2D == ptr::null_mut() {
                _gl_impl_panic("glTexStorage2D");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLsizei, GLenum, GLsizei, GLsizei) = mem::transmute(self.glTexStorage2D);
                (func)(target, levels, internal_format, width, height);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn tex_storage_3d(
            &self,
            target: GLenum,
            levels: GLint,
            internal_format: GLenum,
            width: GLsizei,
            height: GLsizei,
            depth: GLsizei,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glTexStorage3D", &|| format!("target={}, levels={}, internal_format={}, width={}, height={}, depth={}", crate::trace::short(&target), crate::trace::short(&levels), crate::trace::short(&internal_format), crate::trace::short(&width), crate::trace::short(&height), crate::trace::short(&depth)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glTexStorage3D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glTexStorage3D == ptr::null_mut() {
                _gl_impl_panic("glTexStorage3D");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLsizei, GLenum, GLsizei, GLsizei, GLsizei) = mem::transmute(self.glTexStorage3D);
                (func)(target, levels, internal_format, width, height, depth);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_tex_image_into_buffer(
            &self,
            target: GLenum,
            level: GLint,
            format: GLenum,
            ty: GLenum,
            output: &mut [u8],
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetTexImage", &|| format!("target={}, level={}, format={}, ty={}, output={}", crate::trace::short(&target), crate::trace::short(&level), crate::trace::short(&format), crate::trace::short(&ty), format!("len {}", output.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetTexImage"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetTexImage == ptr::null_mut() {
                _gl_impl_panic("glGetTexImage");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLint, GLenum, GLenum, *mut GLvoid) = mem::transmute(self.glGetTexImage);
                (func)(target, level, format, ty, output.as_mut_ptr() as *mut _);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[format!("output={}", crate::trace::short(&output[..output.len().min(8)]))]);
            __r
        }

        $( $opt )? unsafe fn copy_image_sub_data(
            &self,
            src_name: GLuint,
            src_target: GLenum,
            src_level: GLint,
            src_x: GLint,
            src_y: GLint,
            src_z: GLint,
            dst_name: GLuint,
            dst_target: GLenum,
            dst_level: GLint,
            dst_x: GLint,
            dst_y: GLint,
            dst_z: GLint,
            src_width: GLsizei,
            src_height: GLsizei,
            src_depth: GLsizei,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glCopyImageSubData", &|| format!("src_name={}, src_target={}, src_level={}, src_x={}, src_y={}, src_z={}, dst_name={}, dst_target={}, dst_level={}, dst_x={}, dst_y={}, dst_z={}, src_width={}, src_height={}, src_depth={}", crate::trace::short(&src_name), crate::trace::short(&src_target), crate::trace::short(&src_level), crate::trace::short(&src_x), crate::trace::short(&src_y), crate::trace::short(&src_z), crate::trace::short(&dst_name), crate::trace::short(&dst_target), crate::trace::short(&dst_level), crate::trace::short(&dst_x), crate::trace::short(&dst_y), crate::trace::short(&dst_z), crate::trace::short(&src_width), crate::trace::short(&src_height), crate::trace::short(&src_depth)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glCopyImageSubData"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glCopyImageSubData == ptr::null_mut() {
                _gl_impl_panic("glCopyImageSubData");
                return;
            }

            let func: extern "system" fn(GLuint, GLenum, GLint, GLint, GLint, GLint, GLuint, GLenum, GLint, GLint, GLint, GLint, GLsizei, GLsizei, GLsizei) = mem::transmute(self.glCopyImageSubData);
            (func)(
                src_name, src_target, src_level, src_x, src_y, src_z, dst_name, dst_target, dst_level,
                dst_x, dst_y, dst_z, src_width, src_height, src_depth,
            );
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn invalidate_framebuffer(&self, target: GLenum, attachments: &[GLenum]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glInvalidateFramebuffer", &|| format!("target={}, attachments={}", crate::trace::short(&target), crate::trace::short(&attachments)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glInvalidateFramebuffer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glInvalidateFramebuffer == ptr::null_mut() {
                _gl_impl_panic("glInvalidateFramebuffer");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLsizei, *const GLenum) = mem::transmute(self.glInvalidateFramebuffer);
                (func)(
                    target,
                    attachments.len() as GLsizei,
                    attachments.as_ptr(),
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn invalidate_sub_framebuffer(
            &self,
            target: GLenum,
            attachments: &[GLenum],
            xoffset: GLint,
            yoffset: GLint,
            width: GLsizei,
            height: GLsizei,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glInvalidateSubFramebuffer", &|| format!("target={}, attachments={}, xoffset={}, yoffset={}, width={}, height={}", crate::trace::short(&target), crate::trace::short(&attachments), crate::trace::short(&xoffset), crate::trace::short(&yoffset), crate::trace::short(&width), crate::trace::short(&height)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glInvalidateSubFramebuffer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glInvalidateSubFramebuffer == ptr::null_mut() {
                _gl_impl_panic("glInvalidateSubFramebuffer");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLsizei, *const GLenum, GLint, GLint, GLsizei, GLsizei) = mem::transmute(self.glInvalidateSubFramebuffer);
                (func)(
                    target,
                    attachments.len() as GLsizei,
                    attachments.as_ptr(),
                    xoffset,
                    yoffset,
                    width,
                    height,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? unsafe fn get_integer_v(&self, name: GLenum, result: &mut [GLint]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetIntegerv", &|| format!("name={}, result={}", crate::trace::short(&name), format!("len {}", result.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetIntegerv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetIntegerv == ptr::null_mut() {
                _gl_impl_panic("glGetIntegerv");
                return;
            }

            let func: extern "system" fn(GLenum, *mut GLint) = mem::transmute(self.glGetIntegerv);
            (func)(name, result.as_mut_ptr());
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[format!("result={}", crate::trace::short(&result[..result.len().min(8)]))]);
            __r
        }

        $( $opt )? unsafe fn get_integer_64v(&self, name: GLenum, result: &mut [GLint64]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetInteger64v", &|| format!("name={}, result={}", crate::trace::short(&name), format!("len {}", result.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetInteger64v"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetInteger64v == ptr::null_mut() {
                _gl_impl_panic("glGetInteger64v");
                return;
            }
            let func: extern "system" fn(GLenum, *mut GLint64) = mem::transmute(self.glGetInteger64v);
            (func)(name, result.as_mut_ptr());
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[format!("result={}", crate::trace::short(&result[..result.len().min(8)]))]);
            __r
        }

        $( $opt )? unsafe fn get_integer_iv(&self, name: GLenum, index: GLuint, result: &mut [GLint]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetIntegeri_v", &|| format!("name={}, index={}, result={}", crate::trace::short(&name), crate::trace::short(&index), format!("len {}", result.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetIntegeri_v"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetIntegeri_v == ptr::null_mut() {
                _gl_impl_panic("glGetIntegeri_v");
                return;
            }
            let func: extern "system" fn(GLenum, GLuint, *mut GLint) = mem::transmute(self.glGetIntegeri_v);
            (func)(name, index, result.as_mut_ptr());
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[format!("result={}", crate::trace::short(&result[..result.len().min(8)]))]);
            __r
        }

        #[inline]
        $( $opt )? unsafe fn get_integer_64iv(&self, name: GLenum, index: GLuint, result: &mut [GLint64]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetInteger64i_v", &|| format!("name={}, index={}, result={}", crate::trace::short(&name), crate::trace::short(&index), format!("len {}", result.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetInteger64i_v"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetInteger64i_v == ptr::null_mut() {
                _gl_impl_panic("glGetInteger64i_v");
                return;
            }
            let func: extern "system" fn(GLenum, GLuint, *mut GLint64) = mem::transmute(self.glGetInteger64i_v);
            (func)(name, index, result.as_mut_ptr());
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[format!("result={}", crate::trace::short(&result[..result.len().min(8)]))]);
            __r
        }

        #[inline]
        $( $opt )? unsafe fn get_boolean_v(&self, name: GLenum, result: &mut [GLboolean]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetBooleanv", &|| format!("name={}, result={}", crate::trace::short(&name), format!("len {}", result.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetBooleanv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetBooleanv == ptr::null_mut() {
                _gl_impl_panic("glGetBooleanv");
                return;
            }
            let func: extern "system" fn(GLenum, *mut GLboolean) = mem::transmute(self.glGetBooleanv);
            (func)(name, result.as_mut_ptr());
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[format!("result={}", crate::trace::short(&result[..result.len().min(8)]))]);
            __r
        }

        #[inline]
        $( $opt )? unsafe fn get_float_v(&self, name: GLenum, result: &mut [GLfloat]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetFloatv", &|| format!("name={}, result={}", crate::trace::short(&name), format!("len {}", result.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetFloatv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetFloatv == ptr::null_mut() {
                _gl_impl_panic("glGetFloatv");
                return;
            }
            let func: extern "system" fn(GLenum, *mut GLfloat) = mem::transmute(self.glGetFloatv);
            (func)(name, result.as_mut_ptr());
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[format!("result={}", crate::trace::short(&result[..result.len().min(8)]))]);
            __r
        }

        $( $opt )? fn get_framebuffer_attachment_parameter_iv(
            &self,
            target: GLenum,
            attachment: GLenum,
            pname: GLenum,
        ) -> GLint {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetFramebufferAttachmentParameteriv", &|| format!("target={}, attachment={}, pname={}", crate::trace::short(&target), crate::trace::short(&attachment), crate::trace::short(&pname)));
            let __r: GLint = (|| -> GLint {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetFramebufferAttachmentParameteriv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetFramebufferAttachmentParameteriv == ptr::null_mut() {
                _gl_impl_panic("glGetFramebufferAttachmentParameteriv");
                return 0;
            }
            let mut result: GLint = 0;
            unsafe {
                let func: extern "system" fn(GLenum, GLenum, GLenum, *mut GLint) = mem::transmute(self.glGetFramebufferAttachmentParameteriv);
                (func)(
                    target,
                    attachment,
                    pname,
                    &mut result,
                );
            }
            result
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_renderbuffer_parameter_iv(&self, target: GLenum, pname: GLenum) -> GLint {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetRenderbufferParameteriv", &|| format!("target={}, pname={}", crate::trace::short(&target), crate::trace::short(&pname)));
            let __r: GLint = (|| -> GLint {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetRenderbufferParameteriv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetRenderbufferParameteriv == ptr::null_mut() {
                _gl_impl_panic("glGetRenderbufferParameteriv");
                return 0;
            }

            let mut result: GLint = 0;
            unsafe {
                let func: extern "system" fn(GLenum, GLenum, *mut GLint) = mem::transmute(self.glGetRenderbufferParameteriv);
                (func)(target, pname, &mut result);
            }
            result
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_tex_parameter_iv(&self, target: GLenum, pname: GLenum) -> GLint {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetTexParameteriv", &|| format!("target={}, pname={}", crate::trace::short(&target), crate::trace::short(&pname)));
            let __r: GLint = (|| -> GLint {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetTexParameteriv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetTexParameteriv == ptr::null_mut() {
                _gl_impl_panic("glGetTexParameteriv");
                return 0;
            }

            let mut result: GLint = 0;
            unsafe {
                let func: extern "system" fn(GLenum, GLenum, *mut GLint) = mem::transmute(self.glGetTexParameteriv);
                (func)(target, pname, &mut result);
            }
            result
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_tex_parameter_fv(&self, target: GLenum, pname: GLenum) -> GLfloat {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetTexParameterfv", &|| format!("target={}, pname={}", crate::trace::short(&target), crate::trace::short(&pname)));
            let __r: GLfloat = (|| -> GLfloat {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetTexParameterfv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetTexParameterfv == ptr::null_mut() {
                _gl_impl_panic("glGetTexParameterfv");
                return 0.0;
            }

            let mut result: GLfloat = 0.0;
            unsafe {
                let func: extern "system" fn(GLenum, GLenum, *mut GLfloat) = mem::transmute(self.glGetTexParameterfv);
                (func)(target, pname, &mut result);
            }
            result
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn tex_parameter_i(&self, target: GLenum, pname: GLenum, param: GLint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glTexParameteri", &|| format!("target={}, pname={}, param={}", crate::trace::short(&target), crate::trace::short(&pname), crate::trace::short(&param)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glTexParameteri"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glTexParameteri == ptr::null_mut() {
                _gl_impl_panic("glTexParameteri");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLenum, GLint) = mem::transmute(self.glTexParameteri);
                (func)(target, pname, param);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn tex_parameter_f(&self, target: GLenum, pname: GLenum, param: GLfloat) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glTexParameterf", &|| format!("target={}, pname={}, param={}", crate::trace::short(&target), crate::trace::short(&pname), crate::trace::short(&param)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glTexParameterf"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glTexParameterf == ptr::null_mut() {
                _gl_impl_panic("glTexParameterf");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLenum, GLfloat) = mem::transmute(self.glTexParameterf);
                (func)(target, pname, param);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn framebuffer_texture_2d(
            &self,
            target: GLenum,
            attachment: GLenum,
            textarget: GLenum,
            texture: GLuint,
            level: GLint,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glFramebufferTexture2D", &|| format!("target={}, attachment={}, textarget={}, texture={}, level={}", crate::trace::short(&target), crate::trace::short(&attachment), crate::trace::short(&textarget), crate::trace::short(&texture), crate::trace::short(&level)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glFramebufferTexture2D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glFramebufferTexture2D == ptr::null_mut() {
                _gl_impl_panic("glFramebufferTexture2D");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLenum, GLenum, GLuint, GLint) = mem::transmute(self.glFramebufferTexture2D);
                (func)(target, attachment, textarget, texture, level);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn framebuffer_texture_layer(
            &self,
            target: GLenum,
            attachment: GLenum,
            texture: GLuint,
            level: GLint,
            layer: GLint,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glFramebufferTextureLayer", &|| format!("target={}, attachment={}, texture={}, level={}, layer={}", crate::trace::short(&target), crate::trace::short(&attachment), crate::trace::short(&texture), crate::trace::short(&level), crate::trace::short(&layer)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glFramebufferTextureLayer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glFramebufferTextureLayer == ptr::null_mut() {
                _gl_impl_panic("glFramebufferTextureLayer");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLenum, GLuint, GLint, GLint) = mem::transmute(self.glFramebufferTextureLayer);
                (func)(target, attachment, texture, level, layer);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn blit_framebuffer(
            &self,
            src_x0: GLint,
            src_y0: GLint,
            src_x1: GLint,
            src_y1: GLint,
            dst_x0: GLint,
            dst_y0: GLint,
            dst_x1: GLint,
            dst_y1: GLint,
            mask: GLbitfield,
            filter: GLenum,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBlitFramebuffer", &|| format!("src_x0={}, src_y0={}, src_x1={}, src_y1={}, dst_x0={}, dst_y0={}, dst_x1={}, dst_y1={}, mask={}, filter={}", crate::trace::short(&src_x0), crate::trace::short(&src_y0), crate::trace::short(&src_x1), crate::trace::short(&src_y1), crate::trace::short(&dst_x0), crate::trace::short(&dst_y0), crate::trace::short(&dst_x1), crate::trace::short(&dst_y1), crate::trace::short(&mask), crate::trace::short(&filter)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glBlitFramebuffer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBlitFramebuffer == ptr::null_mut() {
                _gl_impl_panic("glBlitFramebuffer");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLint, GLint, GLint, GLint, GLint, GLint, GLint, GLbitfield, GLenum) = mem::transmute(self.glBlitFramebuffer);
                (func)(
                    src_x0, src_y0, src_x1, src_y1, dst_x0, dst_y0, dst_x1, dst_y1, mask, filter,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn vertex_attrib_4f(&self, index: GLuint, x: GLfloat, y: GLfloat, z: GLfloat, w: GLfloat) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glVertexAttrib4f", &|| format!("index={}, x={}, y={}, z={}, w={}", crate::trace::short(&index), crate::trace::short(&x), crate::trace::short(&y), crate::trace::short(&z), crate::trace::short(&w)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glVertexAttrib4f"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glVertexAttrib4f == ptr::null_mut() {
                _gl_impl_panic("glVertexAttrib4f");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint, GLfloat, GLfloat, GLfloat, GLfloat) = mem::transmute(self.glVertexAttrib4f);
                (func)(index, x, y, z, w)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn vertex_attrib_pointer_f32(
            &self,
            index: GLuint,
            size: GLint,
            normalized: bool,
            stride: GLsizei,
            offset: GLuint,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glVertexAttribPointer", &|| format!("index={}, size={}, normalized={}, stride={}, offset={}", crate::trace::short(&index), crate::trace::short(&size), crate::trace::short(&normalized), crate::trace::short(&stride), crate::trace::short(&offset)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glVertexAttribPointer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glVertexAttribPointer == ptr::null_mut() {
                _gl_impl_panic("glVertexAttribPointer");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint, GLint, GLenum, GLboolean, GLsizei, *const GLvoid) = mem::transmute(self.glVertexAttribPointer);
                (func)(
                    index,
                    size,
                    gl::FLOAT,
                    normalized as GLboolean,
                    stride,
                    offset as *const GLvoid,
                )
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn vertex_attrib_pointer(
            &self,
            index: GLuint,
            size: GLint,
            type_: GLenum,
            normalized: bool,
            stride: GLsizei,
            offset: GLuint,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glVertexAttribPointer", &|| format!("index={}, size={}, type_={}, normalized={}, stride={}, offset={}", crate::trace::short(&index), crate::trace::short(&size), crate::trace::short(&type_), crate::trace::short(&normalized), crate::trace::short(&stride), crate::trace::short(&offset)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glVertexAttribPointer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glVertexAttribPointer == ptr::null_mut() {
                _gl_impl_panic("glVertexAttribPointer");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint, GLint, GLenum, GLboolean, GLsizei, *const GLvoid) = mem::transmute(self.glVertexAttribPointer);
                (func)(
                    index,
                    size,
                    type_,
                    normalized as GLboolean,
                    stride,
                    offset as *const GLvoid,
                )
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn vertex_attrib_i_pointer(
            &self,
            index: GLuint,
            size: GLint,
            type_: GLenum,
            stride: GLsizei,
            offset: GLuint,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glVertexAttribIPointer", &|| format!("index={}, size={}, type_={}, stride={}, offset={}", crate::trace::short(&index), crate::trace::short(&size), crate::trace::short(&type_), crate::trace::short(&stride), crate::trace::short(&offset)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glVertexAttribIPointer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glVertexAttribIPointer == ptr::null_mut() {
                _gl_impl_panic("glVertexAttribIPointer");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint, GLint, GLenum, GLsizei, *const GLvoid) = mem::transmute(self.glVertexAttribIPointer);
                (func)(index, size, type_, stride, offset as *const GLvoid)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn vertex_attrib_divisor(&self, index: GLuint, divisor: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glVertexAttribDivisor", &|| format!("index={}, divisor={}", crate::trace::short(&index), crate::trace::short(&divisor)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glVertexAttribDivisor"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glVertexAttribDivisor == ptr::null_mut() {
                _gl_impl_panic("glVertexAttribDivisor");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint, GLuint) = mem::transmute(self.glVertexAttribDivisor);
                (func)(index, divisor)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn viewport(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glViewport", &|| format!("x={}, y={}, width={}, height={}", crate::trace::short(&x), crate::trace::short(&y), crate::trace::short(&width), crate::trace::short(&height)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glViewport"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glViewport == ptr::null_mut() {
                _gl_impl_panic("glViewport");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLint, GLsizei, GLsizei) = mem::transmute(self.glViewport);
                (func)(x, y, width, height);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn scissor(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glScissor", &|| format!("x={}, y={}, width={}, height={}", crate::trace::short(&x), crate::trace::short(&y), crate::trace::short(&width), crate::trace::short(&height)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glScissor"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glScissor == ptr::null_mut() {
                _gl_impl_panic("glScissor");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLint, GLsizei, GLsizei) = mem::transmute(self.glScissor);
                (func)(x, y, width, height);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn line_width(&self, width: GLfloat) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glLineWidth", &|| format!("width={}", crate::trace::short(&width)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glLineWidth"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glLineWidth == ptr::null_mut() {
                _gl_impl_panic("glLineWidth");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLfloat) = mem::transmute(self.glLineWidth);
                (func)(width);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn use_program(&self, program: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUseProgram", &|| format!("program={}", crate::trace::short(&program)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUseProgram"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUseProgram == ptr::null_mut() {
                _gl_impl_panic("glUseProgram");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint) = mem::transmute(self.glUseProgram);
                (func)(program);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn validate_program(&self, program: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glValidateProgram", &|| format!("program={}", crate::trace::short(&program)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glValidateProgram"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glValidateProgram == ptr::null_mut() {
                _gl_impl_panic("glValidateProgram");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint) = mem::transmute(self.glValidateProgram);
                (func)(program);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn draw_arrays(&self, mode: GLenum, first: GLint, count: GLsizei) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDrawArrays", &|| format!("mode={}, first={}, count={}", crate::trace::short(&mode), crate::trace::short(&first), crate::trace::short(&count)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glDrawArrays"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDrawArrays == ptr::null_mut() {
                _gl_impl_panic("glDrawArrays");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLint, GLsizei) = mem::transmute(self.glDrawArrays);
                return (func)(mode, first, count);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn draw_arrays_instanced(
            &self,
            mode: GLenum,
            first: GLint,
            count: GLsizei,
            primcount: GLsizei,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDrawArraysInstanced", &|| format!("mode={}, first={}, count={}, primcount={}", crate::trace::short(&mode), crate::trace::short(&first), crate::trace::short(&count), crate::trace::short(&primcount)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glDrawArraysInstanced"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDrawArraysInstanced == ptr::null_mut() {
                _gl_impl_panic("glDrawArraysInstanced");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum,GLint,GLsizei,GLsizei) = mem::transmute(self.glDrawArraysInstanced);
                return (func)(mode, first, count, primcount);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn draw_elements(
            &self,
            mode: GLenum,
            count: GLsizei,
            element_type: GLenum,
            indices_offset: GLuint,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDrawElements", &|| format!("mode={}, count={}, element_type={}, indices_offset={}", crate::trace::short(&mode), crate::trace::short(&count), crate::trace::short(&element_type), crate::trace::short(&indices_offset)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glDrawElements"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDrawElements == ptr::null_mut() {
                _gl_impl_panic("glDrawElements");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLsizei, GLenum, *const c_void) = mem::transmute(self.glDrawElements);
                return (func)(
                    mode,
                    count,
                    element_type,
                    indices_offset as *const c_void,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn draw_elements_instanced(
            &self,
            mode: GLenum,
            count: GLsizei,
            element_type: GLenum,
            indices_offset: GLuint,
            primcount: GLsizei,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDrawElementsInstanced", &|| format!("mode={}, count={}, element_type={}, indices_offset={}, primcount={}", crate::trace::short(&mode), crate::trace::short(&count), crate::trace::short(&element_type), crate::trace::short(&indices_offset), crate::trace::short(&primcount)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glDrawElementsInstanced"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDrawElementsInstanced == ptr::null_mut() {
                _gl_impl_panic("glDrawElementsInstanced");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLsizei, GLenum, *const c_void, GLsizei) = mem::transmute(self.glDrawElementsInstanced);
                return (func)(
                    mode,
                    count,
                    element_type,
                    indices_offset as *const c_void,
                    primcount,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn blend_color(&self, r: f32, g: f32, b: f32, a: f32) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBlendColor", &|| format!("r={}, g={}, b={}, a={}", crate::trace::short(&r), crate::trace::short(&g), crate::trace::short(&b), crate::trace::short(&a)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glBlendColor"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBlendColor == ptr::null_mut() {
                _gl_impl_panic("glBlendColor");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLclampf, GLclampf, GLclampf, GLclampf) = mem::transmute(self.glBlendColor);
                (func)(r, g, b, a);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn blend_func(&self, sfactor: GLenum, dfactor: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBlendFunc", &|| format!("sfactor={}, dfactor={}", crate::trace::short(&sfactor), crate::trace::short(&dfactor)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glBlendFunc"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBlendFunc == ptr::null_mut() {
                _gl_impl_panic("glBlendFunc");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLenum) = mem::transmute(self.glBlendFunc);
                (func)(sfactor, dfactor);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn blend_func_separate(
            &self,
            src_rgb: GLenum,
            dest_rgb: GLenum,
            src_alpha: GLenum,
            dest_alpha: GLenum,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBlendFuncSeparate", &|| format!("src_rgb={}, dest_rgb={}, src_alpha={}, dest_alpha={}", crate::trace::short(&src_rgb), crate::trace::short(&dest_rgb), crate::trace::short(&src_alpha), crate::trace::short(&dest_alpha)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glBlendFuncSeparate"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBlendFuncSeparate == ptr::null_mut() {
                _gl_impl_panic("glBlendFuncSeparate");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLenum, GLenum, GLenum) = mem::transmute(self.glBlendFuncSeparate);
                (func)(src_rgb, dest_rgb, src_alpha, dest_alpha);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn blend_equation(&self, mode: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBlendEquation", &|| format!("mode={}", crate::trace::short(&mode)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glBlendEquation"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBlendEquation == ptr::null_mut() {
                _gl_impl_panic("glBlendEquation");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum) = mem::transmute(self.glBlendEquation);
                (func)(mode);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn blend_equation_separate(&self, mode_rgb: GLenum, mode_alpha: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBlendEquationSeparate", &|| format!("mode_rgb={}, mode_alpha={}", crate::trace::short(&mode_rgb), crate::trace::short(&mode_alpha)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glBlendEquationSeparate"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBlendEquationSeparate == ptr::null_mut() {
                _gl_impl_panic("glBlendEquationSeparate");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLenum) = mem::transmute(self.glBlendEquationSeparate);
                (func)(mode_rgb, mode_alpha);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn color_mask(&self, r: bool, g: bool, b: bool, a: bool) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glColorMask", &|| format!("r={}, g={}, b={}, a={}", crate::trace::short(&r), crate::trace::short(&g), crate::trace::short(&b), crate::trace::short(&a)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glColorMask"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glColorMask == ptr::null_mut() {
                _gl_impl_panic("glColorMask");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLboolean, GLboolean, GLboolean, GLboolean) = mem::transmute(self.glColorMask);
                (func)(
                    r as GLboolean,
                    g as GLboolean,
                    b as GLboolean,
                    a as GLboolean,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn cull_face(&self, mode: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glCullFace", &|| format!("mode={}", crate::trace::short(&mode)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glCullFace"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glCullFace == ptr::null_mut() {
                _gl_impl_panic("glCullFace");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum) = mem::transmute(self.glCullFace);
                (func)(mode);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn front_face(&self, mode: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glFrontFace", &|| format!("mode={}", crate::trace::short(&mode)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glFrontFace"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glFrontFace == ptr::null_mut() {
                _gl_impl_panic("glFrontFace");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum) = mem::transmute(self.glFrontFace);
                (func)(mode);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn enable(&self, cap: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glEnable", &|| format!("cap={}", crate::trace::short(&cap)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glEnable"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glEnable == ptr::null_mut() {
                _gl_impl_panic("glEnable");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum) = mem::transmute(self.glEnable);
                (func)(cap);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn disable(&self, cap: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDisable", &|| format!("cap={}", crate::trace::short(&cap)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glDisable"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDisable == ptr::null_mut() {
                _gl_impl_panic("glDisable");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum) = mem::transmute(self.glDisable);
                (func)(cap);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn hint(&self, param_name: GLenum, param_val: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glHint", &|| format!("param_name={}, param_val={}", crate::trace::short(&param_name), crate::trace::short(&param_val)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glHint"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glHint == ptr::null_mut() {
                _gl_impl_panic("glHint");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLenum) = mem::transmute(self.glHint);
                (func)(param_name, param_val);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn is_enabled(&self, cap: GLenum) -> GLboolean {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glIsEnabled", &|| format!("cap={}", crate::trace::short(&cap)));
            let __r: GLboolean = (|| -> GLboolean {


            #[cfg(feature = "debug")] { _gl_print_debug("glIsEnabled"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glIsEnabled == ptr::null_mut() {
                _gl_impl_panic("glIsEnabled");
                return 0;
            }

            unsafe {
                let func: extern "system" fn(GLenum) -> GLboolean = mem::transmute(self.glIsEnabled);
                (func)(cap)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn is_shader(&self, shader: GLuint) -> GLboolean {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glIsShader", &|| format!("shader={}", crate::trace::short(&shader)));
            let __r: GLboolean = (|| -> GLboolean {


            #[cfg(feature = "debug")] { _gl_print_debug("glIsShader"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glIsShader == ptr::null_mut() {
                _gl_impl_panic("glIsShader");
                return 0;
            }

            unsafe {
                let func: extern "system" fn(GLuint) -> GLboolean = mem::transmute(self.glIsShader);
                (func)(shader)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn is_texture(&self, texture: GLenum) -> GLboolean {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glIsTexture", &|| format!("texture={}", crate::trace::short(&texture)));
            let __r: GLboolean = (|| -> GLboolean {


            #[cfg(feature = "debug")] { _gl_print_debug("glIsTexture"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glIsTexture == ptr::null_mut() {
                _gl_impl_panic("glIsTexture");
                return 0;
            }

            unsafe {
                let func: extern "system" fn(GLenum) -> GLboolean = mem::transmute(self.glIsTexture);
                (func)(texture)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn is_framebuffer(&self, framebuffer: GLenum) -> GLboolean {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glIsFramebuffer", &|| format!("framebuffer={}", crate::trace::short(&framebuffer)));
            let __r: GLboolean = (|| -> GLboolean {


            #[cfg(feature = "debug")] { _gl_print_debug("glIsFramebuffer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glIsFramebuffer == ptr::null_mut() {
                _gl_impl_panic("glIsFramebuffer");
                return 0;
            }

            unsafe {
                let func: extern "system" fn(GLenum) -> GLboolean = mem::transmute(self.glIsFramebuffer);
                (func)(framebuffer)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn is_renderbuffer(&self, renderbuffer: GLenum) -> GLboolean {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glIsRenderbuffer", &|| format!("renderbuffer={}", crate::trace::short(&renderbuffer)));
            let __r: GLboolean = (|| -> GLboolean {


            #[cfg(feature = "debug")] { _gl_print_debug("glIsRenderbuffer"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glIsRenderbuffer == ptr::null_mut() {
                _gl_impl_panic("glIsRenderbuffer");
                return 0;
            }

            unsafe {
                let func: extern "system" fn(GLenum) -> GLboolean = mem::transmute(self.glIsRenderbuffer);
                (func)(renderbuffer)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn check_frame_buffer_status(&self, target: GLenum) -> GLenum {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glCheckFramebufferStatus", &|| format!("target={}", crate::trace::short(&target)));
            let __r: GLenum = (|| -> GLenum {


            #[cfg(feature = "debug")] { _gl_print_debug("glCheckFramebufferStatus"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glCheckFramebufferStatus == ptr::null_mut() {
                _gl_impl_panic("glCheckFramebufferStatus");
                return 0;
            }

            unsafe {
                let func: extern "system" fn(GLenum) -> GLenum = mem::transmute(self.glCheckFramebufferStatus);
                (func)(target)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn enable_vertex_attrib_array(&self, index: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glEnableVertexAttribArray", &|| format!("index={}", crate::trace::short(&index)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glEnableVertexAttribArray"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glEnableVertexAttribArray == ptr::null_mut() {
                _gl_impl_panic("glEnableVertexAttribArray");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint) = mem::transmute(self.glEnableVertexAttribArray);
                (func)(index);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn disable_vertex_attrib_array(&self, index: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDisableVertexAttribArray", &|| format!("index={}", crate::trace::short(&index)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glDisableVertexAttribArray"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDisableVertexAttribArray == ptr::null_mut() {
                _gl_impl_panic("glDisableVertexAttribArray");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint) = mem::transmute(self.glDisableVertexAttribArray);
                (func)(index);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_1f(&self, location: GLint, v0: GLfloat) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform1f", &|| format!("location={}, v0={}", crate::trace::short(&location), crate::trace::short(&v0)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform1f"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform1f == ptr::null_mut() {
                _gl_impl_panic("glUniform1f");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLfloat) = mem::transmute(self.glUniform1f);
                (func)(location, v0);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_1fv(&self, location: GLint, values: &[f32]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform1fv", &|| format!("location={}, values={}", crate::trace::short(&location), crate::trace::short(&values)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform1fv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform1fv == ptr::null_mut() {
                _gl_impl_panic("glUniform1fv");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLsizei, *const GLfloat) = mem::transmute(self.glUniform1fv);
                (func)(location, values.len() as GLsizei, values.as_ptr());
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_1i(&self, location: GLint, v0: GLint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform1i", &|| format!("location={}, v0={}", crate::trace::short(&location), crate::trace::short(&v0)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform1i"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform1i == ptr::null_mut() {
                _gl_impl_panic("glUniform1i");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLint) = mem::transmute(self.glUniform1i);
                (func)(location, v0);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_1iv(&self, location: GLint, values: &[i32]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform1iv", &|| format!("location={}, values={}", crate::trace::short(&location), crate::trace::short(&values)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform1iv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform1iv == ptr::null_mut() {
                _gl_impl_panic("glUniform1iv");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLsizei, *const GLint) = mem::transmute(self.glUniform1iv);
                (func)(location, values.len() as GLsizei, values.as_ptr());
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_1ui(&self, location: GLint, v0: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform1ui", &|| format!("location={}, v0={}", crate::trace::short(&location), crate::trace::short(&v0)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform1ui"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform1ui == ptr::null_mut() {
                _gl_impl_panic("glUniform1ui");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLuint) = mem::transmute(self.glUniform1ui);
                (func)(location, v0);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_2f(&self, location: GLint, v0: GLfloat, v1: GLfloat) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform2f", &|| format!("location={}, v0={}, v1={}", crate::trace::short(&location), crate::trace::short(&v0), crate::trace::short(&v1)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform2f"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform2f == ptr::null_mut() {
                _gl_impl_panic("glUniform2f");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLfloat, GLfloat) = mem::transmute(self.glUniform2f);
                (func)(location, v0, v1);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_2fv(&self, location: GLint, values: &[f32]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform2fv", &|| format!("location={}, values={}", crate::trace::short(&location), crate::trace::short(&values)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform2fv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform2fv == ptr::null_mut() {
                _gl_impl_panic("glUniform2fv");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLsizei, *const GLfloat) = mem::transmute(self.glUniform2fv);
                (func)(location, (values.len() / 2) as GLsizei, values.as_ptr());
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_2i(&self, location: GLint, v0: GLint, v1: GLint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform2i", &|| format!("location={}, v0={}, v1={}", crate::trace::short(&location), crate::trace::short(&v0), crate::trace::short(&v1)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform2i"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform2i == ptr::null_mut() {
                _gl_impl_panic("glUniform2i");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLint, GLint) = mem::transmute(self.glUniform2i);
                (func)(location, v0, v1);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_2iv(&self, location: GLint, values: &[i32]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform2iv", &|| format!("location={}, values={}", crate::trace::short(&location), crate::trace::short(&values)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform2iv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform2iv == ptr::null_mut() {
                _gl_impl_panic("glUniform2iv");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLsizei, *const GLint) = mem::transmute(self.glUniform2iv);
                (func)(location, (values.len() / 2) as GLsizei, values.as_ptr());
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_2ui(&self, location: GLint, v0: GLuint, v1: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform2ui", &|| format!("location={}, v0={}, v1={}", crate::trace::short(&location), crate::trace::short(&v0), crate::trace::short(&v1)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform2ui"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform2ui == ptr::null_mut() {
                _gl_impl_panic("glUniform2ui");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLuint, GLuint) = mem::transmute(self.glUniform2ui);
                (func)(location, v0, v1);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_3f(&self, location: GLint, v0: GLfloat, v1: GLfloat, v2: GLfloat) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform3f", &|| format!("location={}, v0={}, v1={}, v2={}", crate::trace::short(&location), crate::trace::short(&v0), crate::trace::short(&v1), crate::trace::short(&v2)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform3f"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform3f == ptr::null_mut() {
                _gl_impl_panic("glUniform3f");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLfloat, GLfloat, GLfloat) = mem::transmute(self.glUniform3f);
                (func)(location, v0, v1, v2);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_3fv(&self, location: GLint, values: &[f32]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform3fv", &|| format!("location={}, values={}", crate::trace::short(&location), crate::trace::short(&values)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform3fv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform3fv == ptr::null_mut() {
                _gl_impl_panic("glUniform3fv");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLsizei, *const GLfloat) = mem::transmute(self.glUniform3fv);
                (func)(location, (values.len() / 3) as GLsizei, values.as_ptr());
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_3i(&self, location: GLint, v0: GLint, v1: GLint, v2: GLint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform3i", &|| format!("location={}, v0={}, v1={}, v2={}", crate::trace::short(&location), crate::trace::short(&v0), crate::trace::short(&v1), crate::trace::short(&v2)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform3i"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform3i == ptr::null_mut() {
                _gl_impl_panic("glUniform3i");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLint, GLint, GLint) = mem::transmute(self.glUniform3i);
                (func)(location, v0, v1, v2);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_3iv(&self, location: GLint, values: &[i32]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform3iv", &|| format!("location={}, values={}", crate::trace::short(&location), crate::trace::short(&values)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform3iv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform3iv == ptr::null_mut() {
                _gl_impl_panic("glUniform3iv");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLsizei, *const GLint) = mem::transmute(self.glUniform3iv);
                (func)(location, (values.len() / 3) as GLsizei, values.as_ptr());
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_3ui(&self, location: GLint, v0: GLuint, v1: GLuint, v2: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform3ui", &|| format!("location={}, v0={}, v1={}, v2={}", crate::trace::short(&location), crate::trace::short(&v0), crate::trace::short(&v1), crate::trace::short(&v2)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform3ui"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform3ui == ptr::null_mut() {
                _gl_impl_panic("glUniform3ui");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLuint, GLuint, GLuint) = mem::transmute(self.glUniform3ui);
                (func)(location, v0, v1, v2);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_4f(&self, location: GLint, x: GLfloat, y: GLfloat, z: GLfloat, w: GLfloat) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform4f", &|| format!("location={}, x={}, y={}, z={}, w={}", crate::trace::short(&location), crate::trace::short(&x), crate::trace::short(&y), crate::trace::short(&z), crate::trace::short(&w)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform4f"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform4f == ptr::null_mut() {
                _gl_impl_panic("glUniform4f");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLfloat, GLfloat, GLfloat, GLfloat) = mem::transmute(self.glUniform4f);
                (func)(location, x, y, z, w);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_4i(&self, location: GLint, x: GLint, y: GLint, z: GLint, w: GLint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform4i", &|| format!("location={}, x={}, y={}, z={}, w={}", crate::trace::short(&location), crate::trace::short(&x), crate::trace::short(&y), crate::trace::short(&z), crate::trace::short(&w)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform4i"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform4i == ptr::null_mut() {
                _gl_impl_panic("glUniform4i");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLint, GLint, GLint, GLint) = mem::transmute(self.glUniform4i);
                (func)(location, x, y, z, w);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_4iv(&self, location: GLint, values: &[i32]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform4iv", &|| format!("location={}, values={}", crate::trace::short(&location), crate::trace::short(&values)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform4iv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform4iv == ptr::null_mut() {
                _gl_impl_panic("glUniform4iv");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLsizei, *const GLint) = mem::transmute(self.glUniform4iv);
                (func)(location, (values.len() / 4) as GLsizei, values.as_ptr());
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_4ui(&self, location: GLint, x: GLuint, y: GLuint, z: GLuint, w: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform4ui", &|| format!("location={}, x={}, y={}, z={}, w={}", crate::trace::short(&location), crate::trace::short(&x), crate::trace::short(&y), crate::trace::short(&z), crate::trace::short(&w)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform4ui"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform4ui == ptr::null_mut() {
                _gl_impl_panic("glUniform4ui");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLuint, GLuint, GLuint, GLuint) = mem::transmute(self.glUniform4ui);
                (func)(location, x, y, z, w);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_4fv(&self, location: GLint, values: &[f32]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniform4fv", &|| format!("location={}, values={}", crate::trace::short(&location), crate::trace::short(&values)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniform4fv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniform4fv == ptr::null_mut() {
                _gl_impl_panic("glUniform4fv");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLsizei, *const GLfloat) = mem::transmute(self.glUniform4fv);
                (func)(location, (values.len() / 4) as GLsizei, values.as_ptr());
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_matrix_2fv(&self, location: GLint, transpose: bool, value: &[f32]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniformMatrix2fv", &|| format!("location={}, transpose={}, value={}", crate::trace::short(&location), crate::trace::short(&transpose), crate::trace::short(&value)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniformMatrix2fv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniformMatrix2fv == ptr::null_mut() {
                _gl_impl_panic("glUniformMatrix2fv");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLsizei, GLboolean, *const GLfloat) = mem::transmute(self.glUniformMatrix2fv);
                (func)(
                    location,
                    (value.len() / 4) as GLsizei,
                    transpose as GLboolean,
                    value.as_ptr(),
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_matrix_3fv(&self, location: GLint, transpose: bool, value: &[f32]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniformMatrix3fv", &|| format!("location={}, transpose={}, value={}", crate::trace::short(&location), crate::trace::short(&transpose), crate::trace::short(&value)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniformMatrix3fv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniformMatrix3fv == ptr::null_mut() {
                _gl_impl_panic("glUniformMatrix3fv");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLsizei, GLboolean, *const GLfloat) = mem::transmute(self.glUniformMatrix3fv);
                (func)(
                    location,
                    (value.len() / 9) as GLsizei,
                    transpose as GLboolean,
                    value.as_ptr(),
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn uniform_matrix_4fv(&self, location: GLint, transpose: bool, value: &[f32]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glUniformMatrix4fv", &|| format!("location={}, transpose={}, value={}", crate::trace::short(&location), crate::trace::short(&transpose), crate::trace::short(&value)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glUniformMatrix4fv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glUniformMatrix4fv == ptr::null_mut() {
                _gl_impl_panic("glUniformMatrix4fv");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint, GLsizei, GLboolean, *const GLfloat) = mem::transmute(self.glUniformMatrix4fv);
                (func)(
                    location,
                    (value.len() / 16) as GLsizei,
                    transpose as GLboolean,
                    value.as_ptr(),
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn depth_mask(&self, flag: bool) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDepthMask", &|| format!("flag={}", crate::trace::short(&flag)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glDepthMask"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDepthMask == ptr::null_mut() {
                _gl_impl_panic("glDepthMask");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLboolean) = mem::transmute(self.glDepthMask);
                (func)(flag as GLboolean);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn depth_range(&self, near: f64, far: f64) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDepthRange", &|| format!("near={}, far={}", crate::trace::short(&near), crate::trace::short(&far)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glDepthRange"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDepthRange == ptr::null_mut() {
                _gl_impl_panic("glDepthRange");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLclampd, GLclampd) = mem::transmute(self.glDepthRange);
                (func)(near as GLclampd, far as GLclampd);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_active_attrib(&self, program: GLuint, index: GLuint) -> (i32, u32, String) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetActiveAttrib", &|| format!("program={}, index={}", crate::trace::short(&program), crate::trace::short(&index)));
            let __r: (i32, u32, String) = (|| -> (i32, u32, String) {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetActiveAttrib"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetActiveAttrib == ptr::null_mut() {
                _gl_impl_panic("glGetActiveAttrib");
                return (0, 0, String::new());
            }

            let mut buf_size = [0];
            unsafe {
                self.get_program_iv(program, gl::ACTIVE_ATTRIBUTE_MAX_LENGTH, &mut buf_size);
            }
            let mut name = vec![0u8; buf_size[0] as usize];
            let mut length = 0 as GLsizei;
            let mut size = 0 as i32;
            let mut type_ = 0 as u32;
            unsafe {
                let func: extern "system" fn(GLuint, GLuint, GLsizei, *mut GLsizei, *mut GLint, *mut GLenum, *mut GLchar) = mem::transmute(self.glGetActiveAttrib);
                (func)(
                    program,
                    index,
                    buf_size[0],
                    &mut length,
                    &mut size,
                    &mut type_,
                    name.as_mut_ptr() as *mut GLchar,
                );
            }
            name.truncate(if length > 0 { length as usize } else { 0 });
            (size, type_, String::from_utf8(name).unwrap())
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_active_uniform(&self, program: GLuint, index: GLuint) -> (i32, u32, String) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetActiveUniform", &|| format!("program={}, index={}", crate::trace::short(&program), crate::trace::short(&index)));
            let __r: (i32, u32, String) = (|| -> (i32, u32, String) {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetActiveUniform"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetActiveUniform == ptr::null_mut() {
                _gl_impl_panic("glGetActiveUniform");
                return (0, 0, String::new());
            }

            let mut buf_size = [0];
            unsafe {
                self.get_program_iv(program, gl::ACTIVE_UNIFORM_MAX_LENGTH, &mut buf_size);
            }
            let mut name = vec![0 as u8; buf_size[0] as usize];
            let mut length: GLsizei = 0;
            let mut size: i32 = 0;
            let mut type_: u32 = 0;

            unsafe {
                let func: extern "system" fn(GLuint, GLuint, GLsizei, *mut GLsizei, *mut GLint, *mut GLenum, *mut GLchar) = mem::transmute(self.glGetActiveUniform);
                (func)(
                    program,
                    index,
                    buf_size[0],
                    &mut length,
                    &mut size,
                    &mut type_,
                    name.as_mut_ptr() as *mut GLchar,
                );
            }

            name.truncate(if length > 0 { length as usize } else { 0 });

            (size, type_, String::from_utf8(name).unwrap())
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_active_uniforms_iv(
            &self,
            program: GLuint,
            indices: Vec<GLuint>,
            pname: GLenum,
        ) -> Vec<GLint> {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetActiveUniformsiv", &|| format!("program={}, indices={}, pname={}", crate::trace::short(&program), crate::trace::short(&indices), crate::trace::short(&pname)));
            let __r: Vec<GLint> = (|| -> Vec<GLint> {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetActiveUniformsiv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetActiveUniformsiv == ptr::null_mut() {
                _gl_impl_panic("glGetActiveUniformsiv");
                return Vec::new();
            }

            let mut result = Vec::with_capacity(indices.len());
            unsafe {
                result.set_len(indices.len());
                let func: extern "system" fn(GLuint, GLsizei, *const GLuint, GLenum, *mut GLint) = mem::transmute(self.glGetActiveUniformsiv);
                (func)(
                    program,
                    indices.len() as GLsizei,
                    indices.as_ptr(),
                    pname,
                    result.as_mut_ptr(),
                );
            }
            result
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_active_uniform_block_i(&self, program: GLuint, index: GLuint, pname: GLenum) -> GLint {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetActiveUniformBlockiv", &|| format!("program={}, index={}, pname={}", crate::trace::short(&program), crate::trace::short(&index), crate::trace::short(&pname)));
            let __r: GLint = (|| -> GLint {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetActiveUniformBlockiv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetActiveUniformBlockiv == ptr::null_mut() {
                _gl_impl_panic("glGetActiveUniformBlockiv");
                return 0;
            }

            let mut result = 0;
            unsafe {
                let func: extern "system" fn(GLuint, GLuint, GLenum, *mut GLint) = mem::transmute(self.glGetActiveUniformBlockiv);
                (func)(program, index, pname, &mut result);
            }
            result
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_active_uniform_block_iv(
            &self,
            program: GLuint,
            index: GLuint,
            pname: GLenum,
        ) -> Vec<GLint> {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetActiveUniformBlockiv", &|| format!("program={}, index={}, pname={}", crate::trace::short(&program), crate::trace::short(&index), crate::trace::short(&pname)));
            let __r: Vec<GLint> = (|| -> Vec<GLint> {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetActiveUniformBlockiv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetActiveUniformBlockiv == ptr::null_mut() {
                _gl_impl_panic("glGetActiveUniformBlockiv");
                return Vec::new();
            }

            let count = self.get_active_uniform_block_i(program, index, gl::UNIFORM_BLOCK_ACTIVE_UNIFORMS);
            let mut result = Vec::with_capacity(count as usize);
            unsafe {
                result.set_len(count as usize);
                let func: extern "system" fn(GLuint, GLuint, GLenum, *mut GLint) = mem::transmute(self.glGetActiveUniformBlockiv);
                (func)(program, index, pname, result.as_mut_ptr());
            }
            result
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_active_uniform_block_name(&self, program: GLuint, index: GLuint) -> String {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetActiveUniformBlockName", &|| format!("program={}, index={}", crate::trace::short(&program), crate::trace::short(&index)));
            let __r: String = (|| -> String {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetActiveUniformBlockName"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetActiveUniformBlockName == ptr::null_mut() {
                _gl_impl_panic("glGetActiveUniformBlockName");
                return String::new();
            }

            let buf_size = self.get_active_uniform_block_i(program, index, gl::UNIFORM_BLOCK_NAME_LENGTH);
            let mut name = vec![0 as u8; buf_size as usize];
            let mut length: GLsizei = 0;
            unsafe {
                let func: extern "system" fn(GLuint, GLuint, GLsizei, *mut GLsizei, *mut GLchar) = mem::transmute(self.glGetActiveUniformBlockName);
                (func)(
                    program,
                    index,
                    buf_size,
                    &mut length,
                    name.as_mut_ptr() as *mut GLchar,
                );
            }
            name.truncate(if length > 0 { length as usize } else { 0 });

            String::from_utf8(name).unwrap()
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_attrib_location(&self, program: GLuint, name: &str) -> c_int {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetAttribLocation", &|| format!("program={}, name={}", crate::trace::short(&program), crate::trace::short(&name)));
            let __r: c_int = (|| -> c_int {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetAttribLocation"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetAttribLocation == ptr::null_mut() {
                _gl_impl_panic("glGetAttribLocation");
                return 0;
            }

            let name = encode_ascii(name);
            unsafe {
                let func: extern "system" fn(GLuint, *const GLchar) -> c_int = mem::transmute(self.glGetAttribLocation);
                (func)(program, name.as_ptr())
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_frag_data_location(&self, program: GLuint, name: &str) -> c_int {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetFragDataLocation", &|| format!("program={}, name={}", crate::trace::short(&program), crate::trace::short(&name)));
            let __r: c_int = (|| -> c_int {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetFragDataLocation"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetFragDataLocation == ptr::null_mut() {
                _gl_impl_panic("glGetFragDataLocation");
                return 0;
            }

            let name = encode_ascii(name);
            unsafe {
                let func: extern "system" fn(GLuint, *const c_char)  -> c_int = mem::transmute(self.glGetFragDataLocation);
                (func)(program, name.as_ptr())
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_uniform_location(&self, program: GLuint, name: &str) -> c_int {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetUniformLocation", &|| format!("program={}, name={}", crate::trace::short(&program), crate::trace::short(&name)));
            let __r: c_int = (|| -> c_int {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetUniformLocation"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetUniformLocation == ptr::null_mut() {
                _gl_impl_panic("glGetUniformLocation");
                return 0;
            }

            let name = encode_ascii(name);
            unsafe {
                let func: extern "system" fn(GLuint, *const GLchar)  -> c_int = mem::transmute(self.glGetUniformLocation);
                (func)(program, name.as_ptr())
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_program_info_log(&self, program: GLuint) -> String {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetProgramInfoLog", &|| format!("program={}", crate::trace::short(&program)));
            let __r: String = (|| -> String {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetProgramInfoLog"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetProgramInfoLog == ptr::null_mut() {
                _gl_impl_panic("glGetProgramInfoLog");
                return String::new();
            }

            let mut max_len = [0];
            unsafe {
                self.get_program_iv(program, gl::INFO_LOG_LENGTH, &mut max_len);
            }
            if max_len[0] == 0 {
                return String::new();
            }
            let mut result = vec![0u8; max_len[0] as usize];
            let mut result_len = 0 as GLsizei;
            unsafe {
                let func: extern "system" fn(GLuint, GLsizei, *mut GLsizei, *mut GLchar) = mem::transmute(self.glGetProgramInfoLog);
                (func)(
                    program,
                    max_len[0] as GLsizei,
                    &mut result_len,
                    result.as_mut_ptr() as *mut GLchar,
                );
            }
            result.truncate(if result_len > 0 {
                result_len as usize
            } else {
                0
            });
            String::from_utf8(result).unwrap()
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        #[inline]
        $( $opt )? unsafe fn get_program_iv(&self, program: GLuint, pname: GLenum, result: &mut [GLint]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetProgramiv", &|| format!("program={}, pname={}, result={}", crate::trace::short(&program), crate::trace::short(&pname), format!("len {}", result.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetProgramiv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetProgramiv == ptr::null_mut() {
                _gl_impl_panic("glGetProgramiv");
                return;
            }
            let func: extern "system" fn(GLuint, GLenum, *mut GLint) = mem::transmute(self.glGetProgramiv);
            (func)(program, pname, result.as_mut_ptr());
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[format!("result={}", crate::trace::short(&result[..result.len().min(8)]))]);
            __r
        }

        $( $opt )? fn get_program_binary(&self, program: GLuint) -> (Vec<u8>, GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetProgramBinary", &|| format!("program={}", crate::trace::short(&program)));
            let __r: (Vec<u8>, GLenum) = (|| -> (Vec<u8>, GLenum) {

            const NONE: GLenum = 0;


            #[cfg(feature = "debug")] { _gl_print_debug("glGetProgramBinary"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetProgramBinary == ptr::null_mut() {
                _gl_impl_panic("glGetProgramBinary");
                return (Vec::new(), NONE);
            }

            let mut len = [0];
            unsafe {
                self.get_program_iv(program, gl::PROGRAM_BINARY_LENGTH, &mut len);
            }
            if len[0] <= 0 {
                return (Vec::new(), NONE);
            }
            let mut binary: Vec<u8> = Vec::with_capacity(len[0] as usize);
            let mut format = NONE;
            let mut out_len = 0;
            unsafe {
                binary.set_len(len[0] as usize);
                let func: extern "system" fn(GLuint, GLsizei, *mut GLsizei, *mut GLenum, *mut c_void) = mem::transmute(self.glGetProgramBinary);
                (func)(
                    program,
                    len[0],
                    &mut out_len as *mut GLsizei,
                    &mut format,
                    binary.as_mut_ptr() as *mut c_void,
                );
            }
            if len[0] != out_len {
                return (Vec::new(), NONE);
            }

            (binary, format)
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn program_binary(&self, program: GLuint, format: GLenum, binary: &[u8]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glProgramBinary", &|| format!("program={}, format={}, binary={}", crate::trace::short(&program), crate::trace::short(&format), format!("len {}", binary.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glProgramBinary"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glProgramBinary == ptr::null_mut() {
                _gl_impl_panic("glProgramBinary");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint, GLenum, *const c_void, GLsizei) = mem::transmute(self.glProgramBinary);
                (func)(
                    program,
                    format,
                    binary.as_ptr() as *const c_void,
                    binary.len() as GLsizei,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn program_parameter_i(&self, program: GLuint, pname: GLenum, value: GLint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glProgramParameteri", &|| format!("program={}, pname={}, value={}", crate::trace::short(&program), crate::trace::short(&pname), crate::trace::short(&value)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glProgramParameteri"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glProgramParameteri == ptr::null_mut() {
                _gl_impl_panic("glProgramParameteri");
                return;
            }
            unsafe {
                let func: extern "system" fn(GLuint, GLenum, GLint) = mem::transmute(self.glProgramParameteri);
                (func)(program, pname, value);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        #[inline]
        $( $opt )? unsafe fn get_vertex_attrib_iv(&self, index: GLuint, pname: GLenum, result: &mut [GLint]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetVertexAttribiv", &|| format!("index={}, pname={}, result={}", crate::trace::short(&index), crate::trace::short(&pname), format!("len {}", result.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetVertexAttribiv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetVertexAttribiv == ptr::null_mut() {
                _gl_impl_panic("glGetVertexAttribiv");
                return;
            }
            let func: extern "system" fn(GLuint, GLenum, *mut GLint) = mem::transmute(self.glGetVertexAttribiv);
            (func)(index, pname, result.as_mut_ptr());
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[format!("result={}", crate::trace::short(&result[..result.len().min(8)]))]);
            __r
        }

        #[inline]
        $( $opt )? unsafe fn get_vertex_attrib_fv(&self, index: GLuint, pname: GLenum, result: &mut [GLfloat]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetVertexAttribfv", &|| format!("index={}, pname={}, result={}", crate::trace::short(&index), crate::trace::short(&pname), format!("len {}", result.len())));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glGetVertexAttribfv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetVertexAttribfv == ptr::null_mut() {
                _gl_impl_panic("glGetVertexAttribfv");
                return;
            }
            let func: extern "system" fn(GLuint, GLenum, *mut GLfloat) = mem::transmute(self.glGetVertexAttribfv);
            (func)(index, pname, result.as_mut_ptr());
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[format!("result={}", crate::trace::short(&result[..result.len().min(8)]))]);
            __r
        }

        $( $opt )? fn get_vertex_attrib_pointer_v(&self, index: GLuint, pname: GLenum) -> GLsizeiptr {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetVertexAttribPointerv", &|| format!("index={}, pname={}", crate::trace::short(&index), crate::trace::short(&pname)));
            let __r: GLsizeiptr = (|| -> GLsizeiptr {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetVertexAttribPointerv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetVertexAttribPointerv == ptr::null_mut() {
                _gl_impl_panic("glGetVertexAttribPointerv");
                return 0;
            }

            let mut result = 0 as *mut GLvoid;
            unsafe {
                let func: extern "system" fn(GLuint, GLenum, *mut *mut GLvoid) = mem::transmute(self.glGetVertexAttribPointerv);
                (func)(index, pname, &mut result)
            }
            result as GLsizeiptr
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_buffer_parameter_iv(&self, target: GLuint, pname: GLenum) -> GLint {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetBufferParameteriv", &|| format!("target={}, pname={}", crate::trace::short(&target), crate::trace::short(&pname)));
            let __r: GLint = (|| -> GLint {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetBufferParameteriv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetBufferParameteriv == ptr::null_mut() {
                _gl_impl_panic("glGetBufferParameteriv");
                return 0;
            }

            let mut result = 0 as GLint;
            unsafe {
                let func: extern "system" fn(GLenum, GLenum, *mut GLint) = mem::transmute(self.glGetBufferParameteriv);
                (func)(target, pname, &mut result);
            }
            result
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_shader_info_log(&self, shader: GLuint) -> String {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetShaderInfoLog", &|| format!("shader={}", crate::trace::short(&shader)));
            let __r: String = (|| -> String {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetShaderInfoLog"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetShaderInfoLog == ptr::null_mut() {
                _gl_impl_panic("glGetShaderInfoLog");
                return String::new();
            }

            let mut max_len = [0];
            unsafe {
                self.get_shader_iv(shader, gl::INFO_LOG_LENGTH, &mut max_len);
            }
            if max_len[0] == 0 {
                return String::new();
            }
            let mut result = vec![0u8; max_len[0] as usize];
            let mut result_len = 0 as GLsizei;
            unsafe {
                let func: extern "system" fn(GLuint, GLsizei, *mut GLsizei, *mut GLchar) = mem::transmute(self.glGetShaderInfoLog);
                (func)(
                    shader,
                    max_len[0] as GLsizei,
                    &mut result_len,
                    result.as_mut_ptr() as *mut GLchar,
                );
            }
            result.truncate(if result_len > 0 {
                result_len as usize
            } else {
                0
            });
            String::from_utf8(result).unwrap()
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_string(&self, which: GLenum) -> String {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetString", &|| format!("which={}", crate::trace::short(&which)));
            let __r: String = (|| -> String {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetString"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetString == ptr::null_mut() {
                _gl_impl_panic("glGetString");
                return String::new();
            }

            unsafe {
                let func: extern "system" fn(GLenum) -> *mut GLubyte = mem::transmute(self.glGetString);
                let llstr = (func)(which);
                if !llstr.is_null() {
                    cstr_from_ptr(llstr as *const c_char).to_string()
                } else {
                    String::new()
                }
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_string_i(&self, which: GLenum, index: GLuint) -> String {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetStringi", &|| format!("which={}, index={}", crate::trace::short(&which), crate::trace::short(&index)));
            let __r: String = (|| -> String {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetStringi"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetStringi == ptr::null_mut() {
                _gl_impl_panic("glGetStringi");
                return String::new();
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLuint) -> *mut GLubyte = mem::transmute(self.glGetStringi);
                let llstr = (func)(which, index);
                if !llstr.is_null() {
                    cstr_from_ptr(llstr as *const c_char).to_string()
                } else {
                    String::new()
                }
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? unsafe fn get_shader_iv(&self, shader: GLuint, pname: GLenum, result: &mut [GLint]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetShaderiv", &|| format!("shader={}, pname={}, result={}", crate::trace::short(&shader), crate::trace::short(&pname), format!("len {}", result.len())));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetShaderiv"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetShaderiv == ptr::null_mut() {
                _gl_impl_panic("glGetShaderiv");
                return;
            }

            let func: extern "system" fn(GLuint, GLenum, *mut GLint) = mem::transmute(self.glGetShaderiv);
            (func)(shader, pname, result.as_mut_ptr());
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[format!("result={}", crate::trace::short(&result[..result.len().min(8)]))]);
            __r
        }

        $( $opt )? fn get_shader_precision_format(
            &self,
            _shader_type: GLuint,
            precision_type: GLuint,
        ) -> (GLint, GLint, GLint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetShaderPrecisionFormat", &|| format!("_shader_type={}, precision_type={}", crate::trace::short(&_shader_type), crate::trace::short(&precision_type)));
            let __r: (GLint, GLint, GLint) = (|| -> (GLint, GLint, GLint) {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetShaderPrecisionFormat"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            // if self.glGetShaderPrecisionFormat == ptr::null_mut() {
            //     _gl_impl_panic("glGetShaderPrecisionFormat");
            //     return (0, 0, 0);
            // }

            // gl.GetShaderPrecisionFormat is not available until OpenGL 4.1.
            // Fallback to OpenGL standard precissions that most desktop hardware support.
            match precision_type {
                gl::LOW_FLOAT | gl::MEDIUM_FLOAT | gl::HIGH_FLOAT => {
                    // Fallback to IEEE 754 single precision
                    // Range: from -2^127 to 2^127
                    // Significand precision: 23 bits
                    (127, 127, 23)
                }
                gl::LOW_INT | gl::MEDIUM_INT | gl::HIGH_INT => {
                    // Fallback to single precision integer
                    // Range: from -2^24 to 2^24
                    // Precision: For integer formats this value is always 0
                    (24, 24, 0)
                }
                _ => (0, 0, 0),
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn compile_shader(&self, shader: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glCompileShader", &|| format!("shader={}", crate::trace::short(&shader)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glCompileShader"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glCompileShader == ptr::null_mut() {
                _gl_impl_panic("glCompileShader");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint) = mem::transmute(self.glCompileShader);
                (func)(shader);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn create_program(&self) -> GLuint {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glCreateProgram", &|| format!(""));
            let __r: GLuint = (|| -> GLuint {


            #[cfg(feature = "debug")] { _gl_print_debug("glCreateProgram"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glCreateProgram == ptr::null_mut() {
                _gl_impl_panic("glCreateProgram");
                return 0;
            }

            unsafe {
                let func: extern "system" fn() -> GLuint = mem::transmute(self.glCreateProgram);
                return (func)();
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn delete_program(&self, program: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDeleteProgram", &|| format!("program={}", crate::trace::short(&program)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glDeleteProgram"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDeleteProgram == ptr::null_mut() {
                _gl_impl_panic("glDeleteProgram");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint) = mem::transmute(self.glDeleteProgram);
                (func)(program);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn create_shader(&self, shader_type: GLenum) -> GLuint {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glCreateShader", &|| format!("shader_type={}", crate::trace::short(&shader_type)));
            let __r: GLuint = (|| -> GLuint {


            #[cfg(feature = "debug")] { _gl_print_debug("glCreateShader"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glCreateShader == ptr::null_mut() {
                _gl_impl_panic("glCreateShader");
                return 0;
            }

            unsafe {
                let func: extern "system" fn(GLenum) -> GLuint = mem::transmute(self.glCreateShader);
                return (func)(shader_type);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn delete_shader(&self, shader: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDeleteShader", &|| format!("shader={}", crate::trace::short(&shader)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glDeleteShader"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDeleteShader == ptr::null_mut() {
                _gl_impl_panic("glDeleteShader");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint) = mem::transmute(self.glDeleteShader);
                (func)(shader);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn detach_shader(&self, program: GLuint, shader: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDetachShader", &|| format!("program={}, shader={}", crate::trace::short(&program), crate::trace::short(&shader)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glDetachShader"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDetachShader == ptr::null_mut() {
                _gl_impl_panic("glDetachShader");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint, GLuint) = mem::transmute(self.glDetachShader);
                (func)(program, shader);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn link_program(&self, program: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glLinkProgram", &|| format!("program={}", crate::trace::short(&program)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glLinkProgram"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glLinkProgram == ptr::null_mut() {
                _gl_impl_panic("glLinkProgram");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint) = mem::transmute(self.glLinkProgram);
                return (func)(program);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn clear_color(&self, r: f32, g: f32, b: f32, a: f32) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glClearColor", &|| format!("r={}, g={}, b={}, a={}", crate::trace::short(&r), crate::trace::short(&g), crate::trace::short(&b), crate::trace::short(&a)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glClearColor"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glClearColor == ptr::null_mut() {
                _gl_impl_panic("glClearColor");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLclampf, GLclampf, GLclampf, GLclampf) = mem::transmute(self.glClearColor);
                (func)(r, g, b, a);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn clear(&self, buffer_mask: GLbitfield) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glClear", &|| format!("buffer_mask={}", crate::trace::short(&buffer_mask)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glClear"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glClear == ptr::null_mut() {
                _gl_impl_panic("glClear");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLbitfield) = mem::transmute(self.glClear);
                (func)(buffer_mask);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn clear_depth(&self, depth: f64) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glClearDepth", &|| format!("depth={}", crate::trace::short(&depth)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glClearDepth"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glClearDepth == ptr::null_mut() {
                _gl_impl_panic("glClearDepth");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLclampd) = mem::transmute(self.glClearDepth);
                (func)(depth as GLclampd);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn clear_stencil(&self, s: GLint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glClearStencil", &|| format!("s={}", crate::trace::short(&s)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glClearStencil"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glClearStencil == ptr::null_mut() {
                _gl_impl_panic("glClearStencil");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLint) = mem::transmute(self.glClearStencil);
                (func)(s);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn flush(&self) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glFlush", &|| format!(""));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glFlush"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glFlush == ptr::null_mut() {
                _gl_impl_panic("glFlush");
                return;
            }

            unsafe {
                let func: extern "system" fn() = mem::transmute(self.glFlush);
                (func)();
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn finish(&self) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glFinish", &|| format!(""));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glFinish"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glFinish == ptr::null_mut() {
                _gl_impl_panic("glFinish");
                return;
            }

            unsafe {
                let func: extern "system" fn() = mem::transmute(self.glFinish);
                (func)();
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_error(&self) -> GLenum {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetError", &|| format!(""));
            let __r: GLenum = (|| -> GLenum {


            #[cfg(feature = "debug")] { #[cfg(not(feature = "error"))] { _gl_print_debug("glGetError"); } }

            if self.glGetError == ptr::null_mut() {
                _gl_impl_panic("glGetError");
                return 0;
            }

            unsafe {
                let func: extern "system" fn() -> GLenum = mem::transmute(self.glGetError);
                (func)()
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn stencil_mask(&self, mask: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glStencilMask", &|| format!("mask={}", crate::trace::short(&mask)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glStencilMask"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glStencilMask == ptr::null_mut() {
                _gl_impl_panic("glStencilMask");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint) = mem::transmute(self.glStencilMask);
                (func)(mask)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn stencil_mask_separate(&self, face: GLenum, mask: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glStencilMaskSeparate", &|| format!("face={}, mask={}", crate::trace::short(&face), crate::trace::short(&mask)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glStencilMaskSeparate"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glStencilMaskSeparate == ptr::null_mut() {
                _gl_impl_panic("glStencilMaskSeparate");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLuint) = mem::transmute(self.glStencilMaskSeparate);
                (func)(face, mask)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn stencil_func(&self, func: GLenum, ref_: GLint, mask: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glStencilFunc", &|| format!("func={}, ref_={}, mask={}", crate::trace::short(&func), crate::trace::short(&ref_), crate::trace::short(&mask)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glStencilFunc"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glStencilFunc == ptr::null_mut() {
                _gl_impl_panic("glStencilFunc");
                return;
            }

            unsafe {
                let glStencilFunc: extern "system" fn(GLenum, GLint, GLuint) = mem::transmute(self.glStencilFunc);
                (glStencilFunc)(func, ref_, mask)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn stencil_func_separate(&self, face: GLenum, func: GLenum, ref_: GLint, mask: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glStencilFuncSeparate", &|| format!("face={}, func={}, ref_={}, mask={}", crate::trace::short(&face), crate::trace::short(&func), crate::trace::short(&ref_), crate::trace::short(&mask)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glStencilFuncSeparate"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glStencilFuncSeparate == ptr::null_mut() {
                _gl_impl_panic("glStencilFuncSeparate");
                return;
            }

            unsafe {
                let glStencilFuncSeparate: extern "system" fn(GLenum, GLenum, GLint, GLuint) = mem::transmute(self.glStencilFuncSeparate);
                (glStencilFuncSeparate)(face, func, ref_, mask)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn stencil_op(&self, sfail: GLenum, dpfail: GLenum, dppass: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glStencilOp", &|| format!("sfail={}, dpfail={}, dppass={}", crate::trace::short(&sfail), crate::trace::short(&dpfail), crate::trace::short(&dppass)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glStencilOp"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glStencilOp == ptr::null_mut() {
                _gl_impl_panic("glStencilOp");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLenum, GLenum) = mem::transmute(self.glStencilOp);
                (func)(sfail, dpfail, dppass)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn stencil_op_separate(&self, face: GLenum, sfail: GLenum, dpfail: GLenum, dppass: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glStencilOpSeparate", &|| format!("face={}, sfail={}, dpfail={}, dppass={}", crate::trace::short(&face), crate::trace::short(&sfail), crate::trace::short(&dpfail), crate::trace::short(&dppass)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glStencilOpSeparate"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glStencilOpSeparate == ptr::null_mut() {
                _gl_impl_panic("glStencilOpSeparate");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLenum, GLenum, GLenum) = mem::transmute(self.glStencilOpSeparate);
                (func)(face, sfail, dpfail, dppass)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        #[allow(unused_variables)]
        $( $opt )? fn egl_image_target_texture2d_oes(&self, target: GLenum, image: GLeglImageOES) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glEglImageTargetTexture2dOes", &|| format!("target={}, image={}", crate::trace::short(&target), crate::trace::short(&image)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glEglImageTargetTexture2dOes"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            return; // not supported

            // if self.glEglImageTargetTexture2dOes == ptr::null_mut() {
            //     _gl_impl_panic("glEglImageTargetTexture2dOes");
            //     return;
            // }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        #[allow(unused_variables)]
        $( $opt )? fn egl_image_target_renderbuffer_storage_oes(&self, target: GLenum, image: GLeglImageOES) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glEglImageTargetRenderbufferStorageOes", &|| format!("target={}, image={}", crate::trace::short(&target), crate::trace::short(&image)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glEglImageTargetRenderbufferStorageOes"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            return; // not supported

            // if self.glEglImageTargetRenderbufferStorageOes == ptr::null_mut() {
            //     _gl_impl_panic("glEglImageTargetRenderbufferStorageOes");
            //     return;
            // }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn generate_mipmap(&self, target: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGenerateMipmap", &|| format!("target={}", crate::trace::short(&target)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glGenerateMipmap"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGenerateMipmap == ptr::null_mut() {
                _gl_impl_panic("glGenerateMipmap");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum) = mem::transmute(self.glGenerateMipmap);
                (func)(target)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn insert_event_marker_ext(&self, message: &str) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glInsertEventMarkerEXT", &|| format!("message={}", crate::trace::short(&message)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glInsertEventMarkerEXT"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glInsertEventMarkerEXT == ptr::null_mut() {
                _gl_impl_panic("glInsertEventMarkerEXT");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLsizei, *const GLchar) = mem::transmute(self.glInsertEventMarkerEXT);
                (func)(message.len() as GLsizei, message.as_ptr() as *const _);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn push_group_marker_ext(&self, message: &str) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glPushGroupMarkerEXT", &|| format!("message={}", crate::trace::short(&message)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glPushGroupMarkerEXT"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glPushGroupMarkerEXT == ptr::null_mut() {
                _gl_impl_panic("glPushGroupMarkerEXT");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLsizei, *const GLchar) = mem::transmute(self.glPushGroupMarkerEXT);
                (func)(message.len() as GLsizei, message.as_ptr() as *const _);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn pop_group_marker_ext(&self) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glPopGroupMarkerEXT", &|| format!(""));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glPopGroupMarkerEXT"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glPopGroupMarkerEXT == ptr::null_mut() {
                _gl_impl_panic("glPopGroupMarkerEXT");
                return;
            }

            unsafe {
                let func: extern "system" fn() = mem::transmute(self.glPopGroupMarkerEXT);
                (func)();
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn debug_message_insert_khr(&self, source: GLenum, type_: GLenum, id: GLuint, severity: GLenum, message: &str) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDebugMessageInsertKHR", &|| format!("source={}, type_={}, id={}, severity={}, message={}", crate::trace::short(&source), crate::trace::short(&type_), crate::trace::short(&id), crate::trace::short(&severity), crate::trace::short(&message)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glDebugMessageInsertKHR"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDebugMessageInsertKHR == ptr::null_mut() {
                _gl_impl_panic("glDebugMessageInsertKHR");
                return;
            }

            unsafe {
                // TODO: correct?
                let func: extern "system" fn(GLenum, GLenum, GLuint, GLenum, GLsizei, *const c_char) = mem::transmute(self.glDebugMessageInsertKHR);
                (func)(source, type_, id, severity, message.len() as GLsizei, message.as_ptr() as *const _);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn push_debug_group_khr(&self, source: GLenum, id: GLuint, message: &str) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glPushDebugGroupKHR", &|| format!("source={}, id={}, message={}", crate::trace::short(&source), crate::trace::short(&id), crate::trace::short(&message)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glPushDebugGroupKHR"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glPushDebugGroupKHR == ptr::null_mut() {
                _gl_impl_panic("glPushDebugGroupKHR");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLuint, GLsizei, *const c_char) = mem::transmute(self.glPushDebugGroupKHR);
                (func)(source, id, message.len() as GLsizei, message.as_ptr() as *const _);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn pop_debug_group_khr(&self) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glPopDebugGroupKHR", &|| format!(""));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glPopDebugGroupKHR"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glPopDebugGroupKHR == ptr::null_mut() {
                _gl_impl_panic("glPopDebugGroupKHR");
                return;
            }

            unsafe {
                let func: extern "system" fn() = mem::transmute(self.glPopDebugGroupKHR);
                (func)();
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn fence_sync(&self, condition: GLenum, flags: GLbitfield) -> GLsync {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glFenceSync", &|| format!("condition={}, flags={}", crate::trace::short(&condition), crate::trace::short(&flags)));
            let __r: GLsync = (|| -> GLsync {


            #[cfg(feature = "debug")] { _gl_print_debug("glFenceSync"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glFenceSync == ptr::null_mut() {
                _gl_impl_panic("glFenceSync");
                return ptr::null_mut();
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLbitfield) -> GLsync = mem::transmute(self.glFenceSync);
                (func)(condition, flags) as *const _
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn client_wait_sync(&self, sync: GLsync, flags: GLbitfield, timeout: GLuint64) -> GLenum {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glClientWaitSync", &|| format!("sync={}, flags={}, timeout={}", crate::trace::short(&sync), crate::trace::short(&flags), crate::trace::short(&timeout)));
            let __r: GLenum = (|| -> GLenum {


            #[cfg(feature = "debug")] { _gl_print_debug("glClientWaitSync"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glClientWaitSync == ptr::null_mut() {
                _gl_impl_panic("glClientWaitSync");
                return 0;
            }

            unsafe {
                let func: extern "system" fn(GLsync, GLbitfield, GLuint64) -> GLenum = mem::transmute(self.glClientWaitSync);
                (func)(sync as *const _, flags, timeout)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn wait_sync(&self, sync: GLsync, flags: GLbitfield, timeout: GLuint64) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glWaitSync", &|| format!("sync={}, flags={}, timeout={}", crate::trace::short(&sync), crate::trace::short(&flags), crate::trace::short(&timeout)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glWaitSync"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glWaitSync == ptr::null_mut() {
                _gl_impl_panic("glWaitSync");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLsync, GLbitfield, GLuint64) = mem::transmute(self.glWaitSync);
                (func)(sync as *const _, flags, timeout);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn texture_range_apple(&self, target: GLenum, data: &[u8]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glTextureRangeAPPLE", &|| format!("target={}, data={}", crate::trace::short(&target), format!("len {}", data.len())));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glTextureRangeAPPLE"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glTextureRangeAPPLE == ptr::null_mut() {
                _gl_impl_panic("glTextureRangeAPPLE");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLsizei, *const c_void) = mem::transmute(self.glTextureRangeAPPLE);
                (func)(
                    target,
                    data.len() as GLsizei,
                    data.as_ptr() as *const c_void,
                );
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn delete_sync(&self, sync: GLsync) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDeleteSync", &|| format!("sync={}", crate::trace::short(&sync)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glDeleteSync"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDeleteSync == ptr::null_mut() {
                _gl_impl_panic("glDeleteSync");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLsync) = mem::transmute(self.glDeleteSync);
                (func)(sync as *const _);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn gen_fences_apple(&self, n: GLsizei) -> Vec<GLuint> {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGenFencesAPPLE", &|| format!("n={}", crate::trace::short(&n)));
            let __r: Vec<GLuint> = (|| -> Vec<GLuint> {

            let mut result = vec![0 as GLuint; n as usize];


            #[cfg(feature = "debug")] { _gl_print_debug("glGenFencesAPPLE"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGenFencesAPPLE == ptr::null_mut() {
                _gl_impl_panic("glGenFencesAPPLE");
                return result;
            }

            unsafe {
                let func: extern "system" fn(GLsizei, *mut GLuint) = mem::transmute(self.glGenFencesAPPLE);
                (func)(n, result.as_mut_ptr());
            }
            result
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn delete_fences_apple(&self, fences: &[GLuint]) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glDeleteFencesAPPLE", &|| format!("fences={}", crate::trace::short(&fences)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glDeleteFencesAPPLE"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glDeleteFencesAPPLE == ptr::null_mut() {
                _gl_impl_panic("glDeleteFencesAPPLE");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLsizei, *const GLuint) = mem::transmute(self.glDeleteFencesAPPLE);
                (func)(fences.len() as GLsizei, fences.as_ptr());
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn set_fence_apple(&self, fence: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glSetFenceAPPLE", &|| format!("fence={}", crate::trace::short(&fence)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glSetFenceAPPLE"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glSetFenceAPPLE == ptr::null_mut() {
                _gl_impl_panic("glSetFenceAPPLE");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint) = mem::transmute(self.glSetFenceAPPLE);
                (func)(fence);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn finish_fence_apple(&self, fence: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glFinishFenceAPPLE", &|| format!("fence={}", crate::trace::short(&fence)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glFinishFenceAPPLE"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glFinishFenceAPPLE == ptr::null_mut() {
                _gl_impl_panic("glFinishFenceAPPLE");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint) = mem::transmute(self.glFinishFenceAPPLE);
                (func)(fence);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn test_fence_apple(&self, fence: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glTestFenceAPPLE", &|| format!("fence={}", crate::trace::short(&fence)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glTestFenceAPPLE"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glTestFenceAPPLE == ptr::null_mut() {
                _gl_impl_panic("glTestFenceAPPLE");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLuint) = mem::transmute(self.glTestFenceAPPLE);
                (func)(fence);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn test_object_apple(&self, object: GLenum, name: GLuint) -> GLboolean {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glTestObjectAPPLE", &|| format!("object={}, name={}", crate::trace::short(&object), crate::trace::short(&name)));
            let __r: GLboolean = (|| -> GLboolean {


            #[cfg(feature = "debug")] { _gl_print_debug("glTestObjectAPPLE"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glTestObjectAPPLE == ptr::null_mut() {
                _gl_impl_panic("glTestObjectAPPLE");
                return 0;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLuint) -> GLboolean = mem::transmute(self.glTestObjectAPPLE);
                (func)(object, name)
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn finish_object_apple(&self, object: GLenum, name: GLuint) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glFinishObjectAPPLE", &|| format!("object={}, name={}", crate::trace::short(&object), crate::trace::short(&name)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glFinishObjectAPPLE"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glTestObjectAPPLE == ptr::null_mut() {
                _gl_impl_panic("glFinishObjectAPPLE");
                return;
            }

            unsafe {
                // the spec has a typo for name as GLint instead of GLuint
                let func: extern "system" fn(GLenum, GLint) = mem::transmute(self.glFinishObjectAPPLE);
                (func)(object, name as GLint);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        // GL_ARB_blend_func_extended
        $( $opt )? fn bind_frag_data_location_indexed(
            &self,
            program: GLuint,
            color_number: GLuint,
            index: GLuint,
            name: &str,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBindFragDataLocationIndexed", &|| format!("program={}, color_number={}, index={}, name={}", crate::trace::short(&program), crate::trace::short(&color_number), crate::trace::short(&index), crate::trace::short(&name)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glBindFragDataLocationIndexed"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBindFragDataLocationIndexed == ptr::null_mut() {
                _gl_impl_panic("glBindFragDataLocationIndexed");
                return;
            }

            let c_string = encode_ascii(name);

            unsafe {
                let func: extern "system" fn(GLuint, GLuint, GLuint, *const c_char) = mem::transmute(self.glBindFragDataLocationIndexed);
                (func)(
                    program,
                    color_number,
                    index,
                    c_string.as_ptr(),
                )
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn get_frag_data_index(&self, program: GLuint, name: &str) -> GLint {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetFragDataIndex", &|| format!("program={}, name={}", crate::trace::short(&program), crate::trace::short(&name)));
            let __r: GLint = (|| -> GLint {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetFragDataIndex"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetFragDataIndex == ptr::null_mut() {
                _gl_impl_panic("glGetFragDataIndex");
                return -1;
            }

            let c_string = encode_ascii(name);

            unsafe {
                let func: extern "system" fn(GLuint, *const c_char) -> GLint = mem::transmute(self.glGetFragDataIndex);
                (func)(program, c_string.as_ptr())
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        // GL_KHR_debug
        $( $opt )? fn get_debug_messages(&self) -> Vec<DebugMessage> {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glGetDebugMessageLog", &|| format!(""));
            let __r: Vec<DebugMessage> = (|| -> Vec<DebugMessage> {


            #[cfg(feature = "debug")] { _gl_print_debug("glGetDebugMessageLog"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glGetDebugMessageLog == ptr::null_mut() {
                _gl_impl_panic("glGetDebugMessageLog");
                return Vec::new();
            }

            let mut max_message_len = [0];
            unsafe {
                self.get_integer_v(gl::MAX_DEBUG_MESSAGE_LENGTH, &mut max_message_len[..])
            }

            let mut output = Vec::new();
            const CAPACITY: usize = 4;

            let mut msg_data = vec![0u8; CAPACITY * max_message_len[0] as usize];
            let mut sources = [0 as GLenum; CAPACITY];
            let mut types = [0 as GLenum; CAPACITY];
            let mut severities = [0 as GLenum; CAPACITY];
            let mut ids = [0 as GLuint; CAPACITY];
            let mut lengths = [0 as GLsizei; CAPACITY];

            loop {
                let count = unsafe {
                    let func: extern "system" fn(GLuint, GLsizei, *mut GLenum, *mut GLenum, *mut GLuint, *mut GLenum, *mut GLsizei, *mut GLchar) -> GLuint = mem::transmute(self.glGetDebugMessageLog);
                    (func)(
                        CAPACITY as _,
                        msg_data.len() as _,
                        sources.as_mut_ptr(),
                        types.as_mut_ptr(),
                        ids.as_mut_ptr(),
                        severities.as_mut_ptr(),
                        lengths.as_mut_ptr(),
                        msg_data.as_mut_ptr() as *mut _,
                    )
                };

                let mut offset = 0;
                output.extend((0..count as usize).map(|i| {
                    let len = lengths[i] as usize;
                    let slice = &msg_data[offset..offset + len];
                    offset += len;
                    DebugMessage {
                        message: String::from_utf8_lossy(slice).to_string(),
                        source: sources[i],
                        ty: types[i],
                        id: ids[i],
                        severity: severities[i],
                    }
                }));

                if (count as usize) < CAPACITY {
                    return output;
                }
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn provoking_vertex_angle(&self, _mode: GLenum) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glProvokingVertexAngle", &|| format!("_mode={}", crate::trace::short(&_mode)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glProvokingVertexAngle"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            _gl_impl_panic("glProvokingVertexAngle"); // GLES only
            return;
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        // GL_KHR_blend_equation_advanced
        $( $opt )? fn blend_barrier_khr(&self) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBlendBarrierKHR", &|| format!(""));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glBlendBarrierKHR"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBlendBarrierKHR == ptr::null_mut() {
                _gl_impl_panic("glBlendBarrierKHR");
                return;
            }

            unsafe {
                let func: extern "system" fn() = mem::transmute(self.glBlendBarrierKHR);
                (func)();
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        // GL_CHROMIUM_copy_texture
        $( $opt )? fn copy_texture_chromium(&self,
            _source_id: GLuint, _source_level: GLint,
            _dest_target: GLenum, _dest_id: GLuint, _dest_level: GLint,
            _internal_format: GLint, _dest_type: GLenum,
            _unpack_flip_y: GLboolean, _unpack_premultiply_alpha: GLboolean, _unpack_unmultiply_alpha: GLboolean)
        {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glCopyTextureChromium", &|| format!("_source_id={}, _source_level={}, _dest_target={}, _dest_id={}, _dest_level={}, _internal_format={}, _dest_type={}, _unpack_flip_y={}, _unpack_premultiply_alpha={}, _unpack_unmultiply_alpha={}", crate::trace::short(&_source_id), crate::trace::short(&_source_level), crate::trace::short(&_dest_target), crate::trace::short(&_dest_id), crate::trace::short(&_dest_level), crate::trace::short(&_internal_format), crate::trace::short(&_dest_type), crate::trace::short(&_unpack_flip_y), crate::trace::short(&_unpack_premultiply_alpha), crate::trace::short(&_unpack_unmultiply_alpha)));
            let __r: () = (|| -> () {
            #[cfg(feature = "debug")] { _gl_print_debug("glCopyTextureChromium"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            _gl_impl_panic("glCopyTextureChromium"); // GLES only
            return;
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }
        $( $opt )? fn copy_sub_texture_chromium(&self,
            _source_id: GLuint, _source_level: GLint,
            _dest_target: GLenum, _dest_id: GLuint, _dest_level: GLint,
            _x_offset: GLint, _y_offset: GLint, _x: GLint, _y: GLint, _width: GLsizei, _height: GLsizei,
            _unpack_flip_y: GLboolean, _unpack_premultiply_alpha: GLboolean, _unpack_unmultiply_alpha: GLboolean)
        {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glCopySubTextureChromium", &|| format!("_source_id={}, _source_level={}, _dest_target={}, _dest_id={}, _dest_level={}, _x_offset={}, _y_offset={}, _x={}, _y={}, _width={}, _height={}, _unpack_flip_y={}, _unpack_premultiply_alpha={}, _unpack_unmultiply_alpha={}", crate::trace::short(&_source_id), crate::trace::short(&_source_level), crate::trace::short(&_dest_target), crate::trace::short(&_dest_id), crate::trace::short(&_dest_level), crate::trace::short(&_x_offset), crate::trace::short(&_y_offset), crate::trace::short(&_x), crate::trace::short(&_y), crate::trace::short(&_width), crate::trace::short(&_height), crate::trace::short(&_unpack_flip_y), crate::trace::short(&_unpack_premultiply_alpha), crate::trace::short(&_unpack_unmultiply_alpha)));
            let __r: () = (|| -> () {
            #[cfg(feature = "debug")] { _gl_print_debug("glCopySubTextureChromium"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            _gl_impl_panic("glCopySubTextureChromium"); // GLES only
            return;
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        // GL_ANGLE_copy_texture_3d
        $( $opt )? fn copy_texture_3d_angle(
            &self,
            _source_id: GLuint,
            _source_level: GLint,
            _dest_target: GLenum,
            _dest_id: GLuint,
            _dest_level: GLint,
            _internal_format: GLint,
            _dest_type: GLenum,
            _unpack_flip_y: GLboolean,
            _unpack_premultiply_alpha: GLboolean,
            _unpack_unmultiply_alpha: GLboolean,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glANGLECopyTexture3D", &|| format!("_source_id={}, _source_level={}, _dest_target={}, _dest_id={}, _dest_level={}, _internal_format={}, _dest_type={}, _unpack_flip_y={}, _unpack_premultiply_alpha={}, _unpack_unmultiply_alpha={}", crate::trace::short(&_source_id), crate::trace::short(&_source_level), crate::trace::short(&_dest_target), crate::trace::short(&_dest_id), crate::trace::short(&_dest_level), crate::trace::short(&_internal_format), crate::trace::short(&_dest_type), crate::trace::short(&_unpack_flip_y), crate::trace::short(&_unpack_premultiply_alpha), crate::trace::short(&_unpack_unmultiply_alpha)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glANGLECopyTexture3D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }
            _gl_impl_panic("glANGLECopyTexture3D"); // ANGLE only
            return;
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn copy_sub_texture_3d_angle(
            &self,
            _source_id: GLuint,
            _source_level: GLint,
            _dest_target: GLenum,
            _dest_id: GLuint,
            _dest_level: GLint,
            _x_offset: GLint,
            _y_offset: GLint,
            _z_offset: GLint,
            _x: GLint,
            _y: GLint,
            _z: GLint,
            _width: GLsizei,
            _height: GLsizei,
            _depth: GLsizei,
            _unpack_flip_y: GLboolean,
            _unpack_premultiply_alpha: GLboolean,
            _unpack_unmultiply_alpha: GLboolean,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glANGLECopySubTexture3D", &|| format!("_source_id={}, _source_level={}, _dest_target={}, _dest_id={}, _dest_level={}, _x_offset={}, _y_offset={}, _z_offset={}, _x={}, _y={}, _z={}, _width={}, _height={}, _depth={}, _unpack_flip_y={}, _unpack_premultiply_alpha={}, _unpack_unmultiply_alpha={}", crate::trace::short(&_source_id), crate::trace::short(&_source_level), crate::trace::short(&_dest_target), crate::trace::short(&_dest_id), crate::trace::short(&_dest_level), crate::trace::short(&_x_offset), crate::trace::short(&_y_offset), crate::trace::short(&_z_offset), crate::trace::short(&_x), crate::trace::short(&_y), crate::trace::short(&_z), crate::trace::short(&_width), crate::trace::short(&_height), crate::trace::short(&_depth), crate::trace::short(&_unpack_flip_y), crate::trace::short(&_unpack_premultiply_alpha), crate::trace::short(&_unpack_unmultiply_alpha)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glANGLECopySubTexture3D"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            _gl_impl_panic("glANGLECopySubTexture3D"); // ANGLE only
            // return;
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn buffer_storage(
            &self,
            target: GLenum,
            size: GLsizeiptr,
            data: *const GLvoid,
            flags: GLbitfield,
        ) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glBufferStorage", &|| format!("target={}, size={}, data={:p}, flags={}", crate::trace::short(&target), crate::trace::short(&size), data, crate::trace::short(&flags)));
            let __r: () = (|| -> () {

            #[cfg(feature = "debug")] { _gl_print_debug("glBufferStorage"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glBufferStorage == ptr::null_mut() {
                _gl_impl_panic("glBufferStorage");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLsizeiptr, *const GLvoid, GLbitfield) = mem::transmute(self.glBufferStorage);
                (func)(target, size, data, flags);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }

        $( $opt )? fn flush_mapped_buffer_range(&self, target: GLenum, offset: GLintptr, length: GLsizeiptr) {
            #[cfg(feature = "trace")]
            let __g = crate::trace::enter(self, "glFlushMappedBufferRange", &|| format!("target={}, offset={}, length={}", crate::trace::short(&target), crate::trace::short(&offset), crate::trace::short(&length)));
            let __r: () = (|| -> () {


            #[cfg(feature = "debug")] { _gl_print_debug("glFlushMappedBufferRange"); }
            #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

            if self.glFlushMappedBufferRange == ptr::null_mut() {
                _gl_impl_panic("glFlushMappedBufferRange");
                return;
            }

            unsafe {
                let func: extern "system" fn(GLenum, GLintptr, GLsizeiptr) = mem::transmute(self.glFlushMappedBufferRange);
                (func)(target, offset, length);
            }
        })();
            #[cfg(feature = "trace")]
            __g.after(&__r, &[]);
            __r
        }
    };
}

#[cfg(feature = "gleam_trait")]
impl gleam::gl::Gl for GenericGlContext {
    impl_gl_context!();
}

#[cfg(not(feature = "gleam_trait"))]
#[allow(dead_code)]
impl GenericGlContext {
    impl_gl_context!(pub);

    pub fn start_tiling_qcom(&self, x: GLuint, y: GLuint, width: GLuint, height: GLuint, preserve_mask: GLbitfield) {


        #[cfg(feature = "debug")] { _gl_print_debug("glStartTilingQCOM"); }
        #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

        if self.glStartTilingQCOM == ptr::null_mut() {
            _gl_impl_panic("glStartTilingQCOM");
            return;
        }

        unsafe {
            let func: extern "system" fn(GLuint, GLuint, GLuint, GLuint, GLbitfield) = mem::transmute(self.glStartTilingQCOM);
            (func)(x, y, width, height, preserve_mask);
        }
    }

    pub fn end_tiling_qcom(&self, preserve_mask: GLbitfield) {


        #[cfg(feature = "debug")] { _gl_print_debug("glEndTilingQCOM"); }
        #[cfg(feature = "error")] { _gl_check_for_last_error(self); }

        if self.glEndTilingQCOM == ptr::null_mut() {
            _gl_impl_panic("glEndTilingQCOM");
            return;
        }

        unsafe {
            let func: extern "system" fn(GLbitfield) = mem::transmute(self.glEndTilingQCOM);
            (func)(preserve_mask);
        }
    }
}


#[cfg(feature = "std")]
fn _gl_impl_panic(s: &str) { println!("OpenGL function not loaded: {}", s); }

#[cfg(not(feature = "std"))]
#[inline(always)]
fn _gl_impl_panic(_s: &str) {  }

#[cfg(feature = "debug")]
fn _gl_print_debug(s: &str) { println!("Called OpenGL function: {}", s); }

#[cfg(feature = "error")]
fn _gl_check_for_last_error(gl: &GenericGlContext) {
    // if more than 20 errors occur at once, this likely indicates an infinite loop
    let mut error_count = 20;
    let mut last = gl.get_error();
    while last != gl::NO_ERROR && error_count > 0 {
        let e_string = match last {
            gl::INVALID_ENUM => "INVALID_ENUM".to_string(),
            gl::INVALID_VALUE => "INVALID_VALUE".to_string(),
            gl::INVALID_OPERATION => "INVALID_OPERATION".to_string(),
            gl::INVALID_FRAMEBUFFER_OPERATION => "INVALID_FRAMEBUFFER_OPERATION".to_string(),
            gl::OUT_OF_MEMORY => "OUT_OF_MEMORY".to_string(),
            gl::STACK_UNDERFLOW => "STACK_UNDERFLOW".to_string(),
            gl::STACK_OVERFLOW => "STACK_OVERFLOW".to_string(),
            _ => format!("Unknown error: {:0x}", last),
        };

        println!("OPENGL ERROR OCCURRED: {}", e_string);
        last = gl.get_error();
        error_count -= 1;
    }
}