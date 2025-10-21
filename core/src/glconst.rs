//! Constants for OpenGL 1.1

use gl_context_loader as types;

#[allow(dead_code, non_upper_case_globals)]
pub const ACCUM: types::GLenum = 0x0100;
#[allow(dead_code, non_upper_case_globals)]
pub const ACCUM_ALPHA_BITS: types::GLenum = 0x0D5B;
#[allow(dead_code, non_upper_case_globals)]
pub const ACCUM_BLUE_BITS: types::GLenum = 0x0D5A;
#[allow(dead_code, non_upper_case_globals)]
pub const ACCUM_BUFFER_BIT: types::GLenum = 0x00000200;
#[allow(dead_code, non_upper_case_globals)]
pub const ACCUM_CLEAR_VALUE: types::GLenum = 0x0B80;
#[allow(dead_code, non_upper_case_globals)]
pub const ACCUM_GREEN_BITS: types::GLenum = 0x0D59;
#[allow(dead_code, non_upper_case_globals)]
pub const ACCUM_RED_BITS: types::GLenum = 0x0D58;
#[allow(dead_code, non_upper_case_globals)]
pub const ACTIVE_ATOMIC_COUNTER_BUFFERS: types::GLenum = 0x92D9;
#[allow(dead_code, non_upper_case_globals)]
pub const ACTIVE_ATTRIBUTES: types::GLenum = 0x8B89;
#[allow(dead_code, non_upper_case_globals)]
pub const ACTIVE_ATTRIBUTE_MAX_LENGTH: types::GLenum = 0x8B8A;
#[allow(dead_code, non_upper_case_globals)]
pub const ACTIVE_PROGRAM: types::GLenum = 0x8259;
#[allow(dead_code, non_upper_case_globals)]
pub const ACTIVE_RESOURCES: types::GLenum = 0x92F5;
#[allow(dead_code, non_upper_case_globals)]
pub const ACTIVE_TEXTURE: types::GLenum = 0x84E0;
#[allow(dead_code, non_upper_case_globals)]
pub const ACTIVE_UNIFORMS: types::GLenum = 0x8B86;
#[allow(dead_code, non_upper_case_globals)]
pub const ACTIVE_UNIFORM_BLOCKS: types::GLenum = 0x8A36;
#[allow(dead_code, non_upper_case_globals)]
pub const ACTIVE_UNIFORM_BLOCK_MAX_NAME_LENGTH: types::GLenum = 0x8A35;
#[allow(dead_code, non_upper_case_globals)]
pub const ACTIVE_UNIFORM_MAX_LENGTH: types::GLenum = 0x8B87;
#[allow(dead_code, non_upper_case_globals)]
pub const ACTIVE_VARIABLES: types::GLenum = 0x9305;
#[allow(dead_code, non_upper_case_globals)]
pub const ADD: types::GLenum = 0x0104;
#[allow(dead_code, non_upper_case_globals)]
pub const ADD_SIGNED: types::GLenum = 0x8574;
#[allow(dead_code, non_upper_case_globals)]
pub const ALIASED_LINE_WIDTH_RANGE: types::GLenum = 0x846E;
#[allow(dead_code, non_upper_case_globals)]
pub const ALIASED_POINT_SIZE_RANGE: types::GLenum = 0x846D;
#[allow(dead_code, non_upper_case_globals)]
pub const ALL_ATTRIB_BITS: types::GLenum = 0xFFFFFFFF;
#[allow(dead_code, non_upper_case_globals)]
pub const ALL_BARRIER_BITS: types::GLenum = 0xFFFFFFFF;
#[allow(dead_code, non_upper_case_globals)]
pub const ALL_SHADER_BITS: types::GLenum = 0xFFFFFFFF;
#[allow(dead_code, non_upper_case_globals)]
pub const ALPHA: types::GLenum = 0x1906;
#[allow(dead_code, non_upper_case_globals)]
pub const ALPHA12: types::GLenum = 0x803D;
#[allow(dead_code, non_upper_case_globals)]
pub const ALPHA16: types::GLenum = 0x803E;
#[allow(dead_code, non_upper_case_globals)]
pub const ALPHA16F_EXT: types::GLenum = 0x881C;
#[allow(dead_code, non_upper_case_globals)]
pub const ALPHA32F_EXT: types::GLenum = 0x8816;
#[allow(dead_code, non_upper_case_globals)]
pub const ALPHA4: types::GLenum = 0x803B;
#[allow(dead_code, non_upper_case_globals)]
pub const ALPHA8: types::GLenum = 0x803C;
#[allow(dead_code, non_upper_case_globals)]
pub const ALPHA8_EXT: types::GLenum = 0x803C;
#[allow(dead_code, non_upper_case_globals)]
pub const ALPHA_BIAS: types::GLenum = 0x0D1D;
#[allow(dead_code, non_upper_case_globals)]
pub const ALPHA_BITS: types::GLenum = 0x0D55;
#[allow(dead_code, non_upper_case_globals)]
pub const ALPHA_INTEGER: types::GLenum = 0x8D97;
#[allow(dead_code, non_upper_case_globals)]
pub const ALPHA_SCALE: types::GLenum = 0x0D1C;
#[allow(dead_code, non_upper_case_globals)]
pub const ALPHA_TEST: types::GLenum = 0x0BC0;
#[allow(dead_code, non_upper_case_globals)]
pub const ALPHA_TEST_FUNC: types::GLenum = 0x0BC1;
#[allow(dead_code, non_upper_case_globals)]
pub const ALPHA_TEST_REF: types::GLenum = 0x0BC2;
#[allow(dead_code, non_upper_case_globals)]
pub const ALREADY_SIGNALED: types::GLenum = 0x911A;
#[allow(dead_code, non_upper_case_globals)]
pub const ALWAYS: types::GLenum = 0x0207;
#[allow(dead_code, non_upper_case_globals)]
pub const AMBIENT: types::GLenum = 0x1200;
#[allow(dead_code, non_upper_case_globals)]
pub const AMBIENT_AND_DIFFUSE: types::GLenum = 0x1602;
#[allow(dead_code, non_upper_case_globals)]
pub const AND: types::GLenum = 0x1501;
#[allow(dead_code, non_upper_case_globals)]
pub const AND_INVERTED: types::GLenum = 0x1504;
#[allow(dead_code, non_upper_case_globals)]
pub const AND_REVERSE: types::GLenum = 0x1502;
#[allow(dead_code, non_upper_case_globals)]
pub const ANY_SAMPLES_PASSED: types::GLenum = 0x8C2F;
#[allow(dead_code, non_upper_case_globals)]
pub const ANY_SAMPLES_PASSED_CONSERVATIVE: types::GLenum = 0x8D6A;
#[allow(dead_code, non_upper_case_globals)]
pub const ARRAY_BUFFER: types::GLenum = 0x8892;
#[allow(dead_code, non_upper_case_globals)]
pub const ARRAY_BUFFER_BINDING: types::GLenum = 0x8894;
#[allow(dead_code, non_upper_case_globals)]
pub const ARRAY_SIZE: types::GLenum = 0x92FB;
#[allow(dead_code, non_upper_case_globals)]
pub const ARRAY_STRIDE: types::GLenum = 0x92FE;
#[allow(dead_code, non_upper_case_globals)]
pub const ATOMIC_COUNTER_BARRIER_BIT: types::GLenum = 0x00001000;
#[allow(dead_code, non_upper_case_globals)]
pub const ATOMIC_COUNTER_BUFFER: types::GLenum = 0x92C0;
#[allow(dead_code, non_upper_case_globals)]
pub const ATOMIC_COUNTER_BUFFER_BINDING: types::GLenum = 0x92C1;
#[allow(dead_code, non_upper_case_globals)]
pub const ATOMIC_COUNTER_BUFFER_INDEX: types::GLenum = 0x9301;
#[allow(dead_code, non_upper_case_globals)]
pub const ATOMIC_COUNTER_BUFFER_SIZE: types::GLenum = 0x92C3;
#[allow(dead_code, non_upper_case_globals)]
pub const ATOMIC_COUNTER_BUFFER_START: types::GLenum = 0x92C2;
#[allow(dead_code, non_upper_case_globals)]
pub const ATTACHED_SHADERS: types::GLenum = 0x8B85;
#[allow(dead_code, non_upper_case_globals)]
pub const ATTRIB_STACK_DEPTH: types::GLenum = 0x0BB0;
#[allow(dead_code, non_upper_case_globals)]
pub const AUTO_NORMAL: types::GLenum = 0x0D80;
#[allow(dead_code, non_upper_case_globals)]
pub const AUX0: types::GLenum = 0x0409;
#[allow(dead_code, non_upper_case_globals)]
pub const AUX1: types::GLenum = 0x040A;
#[allow(dead_code, non_upper_case_globals)]
pub const AUX2: types::GLenum = 0x040B;
#[allow(dead_code, non_upper_case_globals)]
pub const AUX3: types::GLenum = 0x040C;
#[allow(dead_code, non_upper_case_globals)]
pub const AUX_BUFFERS: types::GLenum = 0x0C00;
#[allow(dead_code, non_upper_case_globals)]
pub const BACK: types::GLenum = 0x0405;
#[allow(dead_code, non_upper_case_globals)]
pub const BACK_LEFT: types::GLenum = 0x0402;
#[allow(dead_code, non_upper_case_globals)]
pub const BACK_RIGHT: types::GLenum = 0x0403;
#[allow(dead_code, non_upper_case_globals)]
pub const BGR: types::GLenum = 0x80E0;
#[allow(dead_code, non_upper_case_globals)]
pub const BGRA: types::GLenum = 0x80E1;
#[allow(dead_code, non_upper_case_globals)]
pub const BGRA8_EXT: types::GLenum = 0x93A1;
#[allow(dead_code, non_upper_case_globals)]
pub const BGRA_EXT: types::GLenum = 0x80E1;
#[allow(dead_code, non_upper_case_globals)]
pub const BGRA_INTEGER: types::GLenum = 0x8D9B;
#[allow(dead_code, non_upper_case_globals)]
pub const BGR_INTEGER: types::GLenum = 0x8D9A;
#[allow(dead_code, non_upper_case_globals)]
pub const BITMAP: types::GLenum = 0x1A00;
#[allow(dead_code, non_upper_case_globals)]
pub const BITMAP_TOKEN: types::GLenum = 0x0704;
#[allow(dead_code, non_upper_case_globals)]
pub const BLEND: types::GLenum = 0x0BE2;
#[allow(dead_code, non_upper_case_globals)]
pub const BLEND_ADVANCED_COHERENT_KHR: types::GLenum = 0x9285;
#[allow(dead_code, non_upper_case_globals)]
pub const BLEND_COLOR: types::GLenum = 0x8005;
#[allow(dead_code, non_upper_case_globals)]
pub const BLEND_DST: types::GLenum = 0x0BE0;
#[allow(dead_code, non_upper_case_globals)]
pub const BLEND_DST_ALPHA: types::GLenum = 0x80CA;
#[allow(dead_code, non_upper_case_globals)]
pub const BLEND_DST_RGB: types::GLenum = 0x80C8;
#[allow(dead_code, non_upper_case_globals)]
pub const BLEND_EQUATION: types::GLenum = 0x8009;
#[allow(dead_code, non_upper_case_globals)]
pub const BLEND_EQUATION_ALPHA: types::GLenum = 0x883D;
#[allow(dead_code, non_upper_case_globals)]
pub const BLEND_EQUATION_RGB: types::GLenum = 0x8009;
#[allow(dead_code, non_upper_case_globals)]
pub const BLEND_SRC: types::GLenum = 0x0BE1;
#[allow(dead_code, non_upper_case_globals)]
pub const BLEND_SRC_ALPHA: types::GLenum = 0x80CB;
#[allow(dead_code, non_upper_case_globals)]
pub const BLEND_SRC_RGB: types::GLenum = 0x80C9;
#[allow(dead_code, non_upper_case_globals)]
pub const BLOCK_INDEX: types::GLenum = 0x92FD;
#[allow(dead_code, non_upper_case_globals)]
pub const BLUE: types::GLenum = 0x1905;
#[allow(dead_code, non_upper_case_globals)]
pub const BLUE_BIAS: types::GLenum = 0x0D1B;
#[allow(dead_code, non_upper_case_globals)]
pub const BLUE_BITS: types::GLenum = 0x0D54;
#[allow(dead_code, non_upper_case_globals)]
pub const BLUE_INTEGER: types::GLenum = 0x8D96;
#[allow(dead_code, non_upper_case_globals)]
pub const BLUE_SCALE: types::GLenum = 0x0D1A;
#[allow(dead_code, non_upper_case_globals)]
pub const BOOL: types::GLenum = 0x8B56;
#[allow(dead_code, non_upper_case_globals)]
pub const BOOL_VEC2: types::GLenum = 0x8B57;
#[allow(dead_code, non_upper_case_globals)]
pub const BOOL_VEC3: types::GLenum = 0x8B58;
#[allow(dead_code, non_upper_case_globals)]
pub const BOOL_VEC4: types::GLenum = 0x8B59;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER: types::GLenum = 0x82E0;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_ACCESS: types::GLenum = 0x88BB;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_ACCESS_FLAGS: types::GLenum = 0x911F;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_BINDING: types::GLenum = 0x9302;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_DATA_SIZE: types::GLenum = 0x9303;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_IMMUTABLE_STORAGE: types::GLenum = 0x821F;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_IMMUTABLE_STORAGE_EXT: types::GLenum = 0x821F;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_KHR: types::GLenum = 0x82E0;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_MAPPED: types::GLenum = 0x88BC;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_MAP_LENGTH: types::GLenum = 0x9120;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_MAP_OFFSET: types::GLenum = 0x9121;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_MAP_POINTER: types::GLenum = 0x88BD;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_SIZE: types::GLenum = 0x8764;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_STORAGE_FLAGS: types::GLenum = 0x8220;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_STORAGE_FLAGS_EXT: types::GLenum = 0x8220;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_UPDATE_BARRIER_BIT: types::GLenum = 0x00000200;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_USAGE: types::GLenum = 0x8765;
#[allow(dead_code, non_upper_case_globals)]
pub const BUFFER_VARIABLE: types::GLenum = 0x92E5;
#[allow(dead_code, non_upper_case_globals)]
pub const BYTE: types::GLenum = 0x1400;
#[allow(dead_code, non_upper_case_globals)]
pub const C3F_V3F: types::GLenum = 0x2A24;
#[allow(dead_code, non_upper_case_globals)]
pub const C4F_N3F_V3F: types::GLenum = 0x2A26;
#[allow(dead_code, non_upper_case_globals)]
pub const C4UB_V2F: types::GLenum = 0x2A22;
#[allow(dead_code, non_upper_case_globals)]
pub const C4UB_V3F: types::GLenum = 0x2A23;
#[allow(dead_code, non_upper_case_globals)]
pub const CCW: types::GLenum = 0x0901;
#[allow(dead_code, non_upper_case_globals)]
pub const CLAMP: types::GLenum = 0x2900;
#[allow(dead_code, non_upper_case_globals)]
pub const CLAMP_FRAGMENT_COLOR: types::GLenum = 0x891B;
#[allow(dead_code, non_upper_case_globals)]
pub const CLAMP_READ_COLOR: types::GLenum = 0x891C;
#[allow(dead_code, non_upper_case_globals)]
pub const CLAMP_TO_BORDER: types::GLenum = 0x812D;
#[allow(dead_code, non_upper_case_globals)]
pub const CLAMP_TO_EDGE: types::GLenum = 0x812F;
#[allow(dead_code, non_upper_case_globals)]
pub const CLAMP_VERTEX_COLOR: types::GLenum = 0x891A;
#[allow(dead_code, non_upper_case_globals)]
pub const CLEAR: types::GLenum = 0x1500;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIENT_ACTIVE_TEXTURE: types::GLenum = 0x84E1;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIENT_ALL_ATTRIB_BITS: types::GLenum = 0xFFFFFFFF;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIENT_ATTRIB_STACK_DEPTH: types::GLenum = 0x0BB1;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIENT_MAPPED_BUFFER_BARRIER_BIT: types::GLenum = 0x00004000;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIENT_MAPPED_BUFFER_BARRIER_BIT_EXT: types::GLenum = 0x00004000;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIENT_PIXEL_STORE_BIT: types::GLenum = 0x00000001;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIENT_STORAGE_BIT: types::GLenum = 0x0200;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIENT_STORAGE_BIT_EXT: types::GLenum = 0x0200;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIENT_VERTEX_ARRAY_BIT: types::GLenum = 0x00000002;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIP_DISTANCE0: types::GLenum = 0x3000;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIP_DISTANCE1: types::GLenum = 0x3001;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIP_DISTANCE2: types::GLenum = 0x3002;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIP_DISTANCE3: types::GLenum = 0x3003;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIP_DISTANCE4: types::GLenum = 0x3004;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIP_DISTANCE5: types::GLenum = 0x3005;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIP_DISTANCE6: types::GLenum = 0x3006;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIP_DISTANCE7: types::GLenum = 0x3007;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIP_PLANE0: types::GLenum = 0x3000;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIP_PLANE1: types::GLenum = 0x3001;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIP_PLANE2: types::GLenum = 0x3002;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIP_PLANE3: types::GLenum = 0x3003;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIP_PLANE4: types::GLenum = 0x3004;
#[allow(dead_code, non_upper_case_globals)]
pub const CLIP_PLANE5: types::GLenum = 0x3005;
#[allow(dead_code, non_upper_case_globals)]
pub const COEFF: types::GLenum = 0x0A00;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR: types::GLenum = 0x1800;
#[allow(dead_code, non_upper_case_globals)]
pub const COLORBURN_KHR: types::GLenum = 0x929A;
#[allow(dead_code, non_upper_case_globals)]
pub const COLORDODGE_KHR: types::GLenum = 0x9299;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ARRAY: types::GLenum = 0x8076;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ARRAY_BUFFER_BINDING: types::GLenum = 0x8898;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ARRAY_POINTER: types::GLenum = 0x8090;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ARRAY_SIZE: types::GLenum = 0x8081;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ARRAY_STRIDE: types::GLenum = 0x8083;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ARRAY_TYPE: types::GLenum = 0x8082;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT0: types::GLenum = 0x8CE0;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT1: types::GLenum = 0x8CE1;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT10: types::GLenum = 0x8CEA;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT11: types::GLenum = 0x8CEB;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT12: types::GLenum = 0x8CEC;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT13: types::GLenum = 0x8CED;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT14: types::GLenum = 0x8CEE;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT15: types::GLenum = 0x8CEF;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT16: types::GLenum = 0x8CF0;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT17: types::GLenum = 0x8CF1;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT18: types::GLenum = 0x8CF2;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT19: types::GLenum = 0x8CF3;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT2: types::GLenum = 0x8CE2;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT20: types::GLenum = 0x8CF4;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT21: types::GLenum = 0x8CF5;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT22: types::GLenum = 0x8CF6;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT23: types::GLenum = 0x8CF7;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT24: types::GLenum = 0x8CF8;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT25: types::GLenum = 0x8CF9;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT26: types::GLenum = 0x8CFA;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT27: types::GLenum = 0x8CFB;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT28: types::GLenum = 0x8CFC;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT29: types::GLenum = 0x8CFD;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT3: types::GLenum = 0x8CE3;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT30: types::GLenum = 0x8CFE;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT31: types::GLenum = 0x8CFF;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT4: types::GLenum = 0x8CE4;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT5: types::GLenum = 0x8CE5;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT6: types::GLenum = 0x8CE6;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT7: types::GLenum = 0x8CE7;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT8: types::GLenum = 0x8CE8;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_ATTACHMENT9: types::GLenum = 0x8CE9;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_BUFFER_BIT: types::GLenum = 0x00004000;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_BUFFER_BIT0_QCOM: types::GLenum = 0x00000001;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_BUFFER_BIT1_QCOM: types::GLenum = 0x00000002;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_BUFFER_BIT2_QCOM: types::GLenum = 0x00000004;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_BUFFER_BIT3_QCOM: types::GLenum = 0x00000008;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_BUFFER_BIT4_QCOM: types::GLenum = 0x00000010;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_BUFFER_BIT5_QCOM: types::GLenum = 0x00000020;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_BUFFER_BIT6_QCOM: types::GLenum = 0x00000040;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_BUFFER_BIT7_QCOM: types::GLenum = 0x00000080;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_CLEAR_VALUE: types::GLenum = 0x0C22;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_INDEX: types::GLenum = 0x1900;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_INDEXES: types::GLenum = 0x1603;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_LOGIC_OP: types::GLenum = 0x0BF2;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_MATERIAL: types::GLenum = 0x0B57;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_MATERIAL_FACE: types::GLenum = 0x0B55;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_MATERIAL_PARAMETER: types::GLenum = 0x0B56;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_SUM: types::GLenum = 0x8458;
#[allow(dead_code, non_upper_case_globals)]
pub const COLOR_WRITEMASK: types::GLenum = 0x0C23;
#[allow(dead_code, non_upper_case_globals)]
pub const COMBINE: types::GLenum = 0x8570;
#[allow(dead_code, non_upper_case_globals)]
pub const COMBINE_ALPHA: types::GLenum = 0x8572;
#[allow(dead_code, non_upper_case_globals)]
pub const COMBINE_RGB: types::GLenum = 0x8571;
#[allow(dead_code, non_upper_case_globals)]
pub const COMMAND_BARRIER_BIT: types::GLenum = 0x00000040;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPARE_REF_TO_TEXTURE: types::GLenum = 0x884E;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPARE_R_TO_TEXTURE: types::GLenum = 0x884E;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPILE: types::GLenum = 0x1300;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPILE_AND_EXECUTE: types::GLenum = 0x1301;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPILE_STATUS: types::GLenum = 0x8B81;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_ALPHA: types::GLenum = 0x84E9;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_INTENSITY: types::GLenum = 0x84EC;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_LUMINANCE: types::GLenum = 0x84EA;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_LUMINANCE_ALPHA: types::GLenum = 0x84EB;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_R11_EAC: types::GLenum = 0x9270;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_RED: types::GLenum = 0x8225;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_RED_RGTC1: types::GLenum = 0x8DBB;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_RG: types::GLenum = 0x8226;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_RG11_EAC: types::GLenum = 0x9272;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_RGB: types::GLenum = 0x84ED;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_RGB8_ETC2: types::GLenum = 0x9274;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_RGB8_PUNCHTHROUGH_ALPHA1_ETC2: types::GLenum = 0x9276;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_RGBA: types::GLenum = 0x84EE;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_RGBA8_ETC2_EAC: types::GLenum = 0x9278;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_RG_RGTC2: types::GLenum = 0x8DBD;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_SIGNED_R11_EAC: types::GLenum = 0x9271;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_SIGNED_RED_RGTC1: types::GLenum = 0x8DBC;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_SIGNED_RG11_EAC: types::GLenum = 0x9273;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_SIGNED_RG_RGTC2: types::GLenum = 0x8DBE;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_SLUMINANCE: types::GLenum = 0x8C4A;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_SLUMINANCE_ALPHA: types::GLenum = 0x8C4B;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_SRGB: types::GLenum = 0x8C48;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_SRGB8_ALPHA8_ETC2_EAC: types::GLenum = 0x9279;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_SRGB8_ETC2: types::GLenum = 0x9275;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_SRGB8_PUNCHTHROUGH_ALPHA1_ETC2: types::GLenum = 0x9277;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_SRGB_ALPHA: types::GLenum = 0x8C49;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPRESSED_TEXTURE_FORMATS: types::GLenum = 0x86A3;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPUTE_SHADER: types::GLenum = 0x91B9;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPUTE_SHADER_BIT: types::GLenum = 0x00000020;
#[allow(dead_code, non_upper_case_globals)]
pub const COMPUTE_WORK_GROUP_SIZE: types::GLenum = 0x8267;
#[allow(dead_code, non_upper_case_globals)]
pub const CONDITION_SATISFIED: types::GLenum = 0x911C;
#[allow(dead_code, non_upper_case_globals)]
pub const CONSTANT: types::GLenum = 0x8576;
#[allow(dead_code, non_upper_case_globals)]
pub const CONSTANT_ALPHA: types::GLenum = 0x8003;
#[allow(dead_code, non_upper_case_globals)]
pub const CONSTANT_ATTENUATION: types::GLenum = 0x1207;
#[allow(dead_code, non_upper_case_globals)]
pub const CONSTANT_COLOR: types::GLenum = 0x8001;
#[allow(dead_code, non_upper_case_globals)]
pub const CONTEXT_COMPATIBILITY_PROFILE_BIT: types::GLenum = 0x00000002;
#[allow(dead_code, non_upper_case_globals)]
pub const CONTEXT_CORE_PROFILE_BIT: types::GLenum = 0x00000001;
#[allow(dead_code, non_upper_case_globals)]
pub const CONTEXT_FLAGS: types::GLenum = 0x821E;
#[allow(dead_code, non_upper_case_globals)]
pub const CONTEXT_FLAG_DEBUG_BIT: types::GLenum = 0x00000002;
#[allow(dead_code, non_upper_case_globals)]
pub const CONTEXT_FLAG_DEBUG_BIT_KHR: types::GLenum = 0x00000002;
#[allow(dead_code, non_upper_case_globals)]
pub const CONTEXT_FLAG_FORWARD_COMPATIBLE_BIT: types::GLenum = 0x00000001;
#[allow(dead_code, non_upper_case_globals)]
pub const CONTEXT_PROFILE_MASK: types::GLenum = 0x9126;
#[allow(dead_code, non_upper_case_globals)]
pub const COORD_REPLACE: types::GLenum = 0x8862;
#[allow(dead_code, non_upper_case_globals)]
pub const COPY: types::GLenum = 0x1503;
#[allow(dead_code, non_upper_case_globals)]
pub const COPY_INVERTED: types::GLenum = 0x150C;
#[allow(dead_code, non_upper_case_globals)]
pub const COPY_PIXEL_TOKEN: types::GLenum = 0x0706;
#[allow(dead_code, non_upper_case_globals)]
pub const COPY_READ_BUFFER: types::GLenum = 0x8F36;
#[allow(dead_code, non_upper_case_globals)]
pub const COPY_READ_BUFFER_BINDING: types::GLenum = 0x8F36;
#[allow(dead_code, non_upper_case_globals)]
pub const COPY_WRITE_BUFFER: types::GLenum = 0x8F37;
#[allow(dead_code, non_upper_case_globals)]
pub const COPY_WRITE_BUFFER_BINDING: types::GLenum = 0x8F37;
#[allow(dead_code, non_upper_case_globals)]
pub const CULL_FACE: types::GLenum = 0x0B44;
#[allow(dead_code, non_upper_case_globals)]
pub const CULL_FACE_MODE: types::GLenum = 0x0B45;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_BIT: types::GLenum = 0x00000001;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_COLOR: types::GLenum = 0x0B00;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_FOG_COORD: types::GLenum = 0x8453;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_FOG_COORDINATE: types::GLenum = 0x8453;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_INDEX: types::GLenum = 0x0B01;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_NORMAL: types::GLenum = 0x0B02;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_PROGRAM: types::GLenum = 0x8B8D;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_QUERY: types::GLenum = 0x8865;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_QUERY_EXT: types::GLenum = 0x8865;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_RASTER_COLOR: types::GLenum = 0x0B04;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_RASTER_DISTANCE: types::GLenum = 0x0B09;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_RASTER_INDEX: types::GLenum = 0x0B05;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_RASTER_POSITION: types::GLenum = 0x0B07;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_RASTER_POSITION_VALID: types::GLenum = 0x0B08;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_RASTER_SECONDARY_COLOR: types::GLenum = 0x845F;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_RASTER_TEXTURE_COORDS: types::GLenum = 0x0B06;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_SECONDARY_COLOR: types::GLenum = 0x8459;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_TEXTURE_COORDS: types::GLenum = 0x0B03;
#[allow(dead_code, non_upper_case_globals)]
pub const CURRENT_VERTEX_ATTRIB: types::GLenum = 0x8626;
#[allow(dead_code, non_upper_case_globals)]
pub const CW: types::GLenum = 0x0900;
#[allow(dead_code, non_upper_case_globals)]
pub const DARKEN_KHR: types::GLenum = 0x9297;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_CALLBACK_FUNCTION: types::GLenum = 0x8244;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_CALLBACK_FUNCTION_KHR: types::GLenum = 0x8244;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_CALLBACK_USER_PARAM: types::GLenum = 0x8245;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_CALLBACK_USER_PARAM_KHR: types::GLenum = 0x8245;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_GROUP_STACK_DEPTH: types::GLenum = 0x826D;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_GROUP_STACK_DEPTH_KHR: types::GLenum = 0x826D;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_LOGGED_MESSAGES: types::GLenum = 0x9145;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_LOGGED_MESSAGES_KHR: types::GLenum = 0x9145;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_NEXT_LOGGED_MESSAGE_LENGTH: types::GLenum = 0x8243;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_NEXT_LOGGED_MESSAGE_LENGTH_KHR: types::GLenum = 0x8243;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_OUTPUT: types::GLenum = 0x92E0;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_OUTPUT_KHR: types::GLenum = 0x92E0;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_OUTPUT_SYNCHRONOUS: types::GLenum = 0x8242;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_OUTPUT_SYNCHRONOUS_KHR: types::GLenum = 0x8242;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SEVERITY_HIGH: types::GLenum = 0x9146;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SEVERITY_HIGH_KHR: types::GLenum = 0x9146;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SEVERITY_LOW: types::GLenum = 0x9148;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SEVERITY_LOW_KHR: types::GLenum = 0x9148;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SEVERITY_MEDIUM: types::GLenum = 0x9147;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SEVERITY_MEDIUM_KHR: types::GLenum = 0x9147;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SEVERITY_NOTIFICATION: types::GLenum = 0x826B;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SEVERITY_NOTIFICATION_KHR: types::GLenum = 0x826B;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SOURCE_API: types::GLenum = 0x8246;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SOURCE_API_KHR: types::GLenum = 0x8246;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SOURCE_APPLICATION: types::GLenum = 0x824A;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SOURCE_APPLICATION_KHR: types::GLenum = 0x824A;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SOURCE_OTHER: types::GLenum = 0x824B;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SOURCE_OTHER_KHR: types::GLenum = 0x824B;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SOURCE_SHADER_COMPILER: types::GLenum = 0x8248;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SOURCE_SHADER_COMPILER_KHR: types::GLenum = 0x8248;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SOURCE_THIRD_PARTY: types::GLenum = 0x8249;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SOURCE_THIRD_PARTY_KHR: types::GLenum = 0x8249;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SOURCE_WINDOW_SYSTEM: types::GLenum = 0x8247;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_SOURCE_WINDOW_SYSTEM_KHR: types::GLenum = 0x8247;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_DEPRECATED_BEHAVIOR: types::GLenum = 0x824D;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_DEPRECATED_BEHAVIOR_KHR: types::GLenum = 0x824D;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_ERROR: types::GLenum = 0x824C;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_ERROR_KHR: types::GLenum = 0x824C;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_MARKER: types::GLenum = 0x8268;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_MARKER_KHR: types::GLenum = 0x8268;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_OTHER: types::GLenum = 0x8251;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_OTHER_KHR: types::GLenum = 0x8251;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_PERFORMANCE: types::GLenum = 0x8250;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_PERFORMANCE_KHR: types::GLenum = 0x8250;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_POP_GROUP: types::GLenum = 0x826A;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_POP_GROUP_KHR: types::GLenum = 0x826A;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_PORTABILITY: types::GLenum = 0x824F;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_PORTABILITY_KHR: types::GLenum = 0x824F;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_PUSH_GROUP: types::GLenum = 0x8269;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_PUSH_GROUP_KHR: types::GLenum = 0x8269;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_UNDEFINED_BEHAVIOR: types::GLenum = 0x824E;
#[allow(dead_code, non_upper_case_globals)]
pub const DEBUG_TYPE_UNDEFINED_BEHAVIOR_KHR: types::GLenum = 0x824E;
#[allow(dead_code, non_upper_case_globals)]
pub const DECAL: types::GLenum = 0x2101;
#[allow(dead_code, non_upper_case_globals)]
pub const DECR: types::GLenum = 0x1E03;
#[allow(dead_code, non_upper_case_globals)]
pub const DECR_WRAP: types::GLenum = 0x8508;
#[allow(dead_code, non_upper_case_globals)]
pub const DELETE_STATUS: types::GLenum = 0x8B80;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH: types::GLenum = 0x1801;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH24_STENCIL8: types::GLenum = 0x88F0;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH32F_STENCIL8: types::GLenum = 0x8CAD;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_ATTACHMENT: types::GLenum = 0x8D00;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_BIAS: types::GLenum = 0x0D1F;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_BITS: types::GLenum = 0x0D56;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_BUFFER_BIT: types::GLenum = 0x00000100;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_BUFFER_BIT0_QCOM: types::GLenum = 0x00000100;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_BUFFER_BIT1_QCOM: types::GLenum = 0x00000200;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_BUFFER_BIT2_QCOM: types::GLenum = 0x00000400;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_BUFFER_BIT3_QCOM: types::GLenum = 0x00000800;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_BUFFER_BIT4_QCOM: types::GLenum = 0x00001000;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_BUFFER_BIT5_QCOM: types::GLenum = 0x00002000;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_BUFFER_BIT6_QCOM: types::GLenum = 0x00004000;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_BUFFER_BIT7_QCOM: types::GLenum = 0x00008000;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_CLAMP: types::GLenum = 0x864F;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_CLEAR_VALUE: types::GLenum = 0x0B73;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_COMPONENT: types::GLenum = 0x1902;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_COMPONENT16: types::GLenum = 0x81A5;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_COMPONENT24: types::GLenum = 0x81A6;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_COMPONENT32: types::GLenum = 0x81A7;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_COMPONENT32F: types::GLenum = 0x8CAC;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_FUNC: types::GLenum = 0x0B74;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_RANGE: types::GLenum = 0x0B70;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_SCALE: types::GLenum = 0x0D1E;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_STENCIL: types::GLenum = 0x84F9;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_STENCIL_ATTACHMENT: types::GLenum = 0x821A;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_STENCIL_TEXTURE_MODE: types::GLenum = 0x90EA;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_TEST: types::GLenum = 0x0B71;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_TEXTURE_MODE: types::GLenum = 0x884B;
#[allow(dead_code, non_upper_case_globals)]
pub const DEPTH_WRITEMASK: types::GLenum = 0x0B72;
#[allow(dead_code, non_upper_case_globals)]
pub const DIFFERENCE_KHR: types::GLenum = 0x929E;
#[allow(dead_code, non_upper_case_globals)]
pub const DIFFUSE: types::GLenum = 0x1201;
#[allow(dead_code, non_upper_case_globals)]
pub const DISPATCH_INDIRECT_BUFFER: types::GLenum = 0x90EE;
#[allow(dead_code, non_upper_case_globals)]
pub const DISPATCH_INDIRECT_BUFFER_BINDING: types::GLenum = 0x90EF;
#[allow(dead_code, non_upper_case_globals)]
pub const DISPLAY_LIST: types::GLenum = 0x82E7;
#[allow(dead_code, non_upper_case_globals)]
pub const DITHER: types::GLenum = 0x0BD0;
#[allow(dead_code, non_upper_case_globals)]
pub const DOMAIN: types::GLenum = 0x0A02;
#[allow(dead_code, non_upper_case_globals)]
pub const DONT_CARE: types::GLenum = 0x1100;
#[allow(dead_code, non_upper_case_globals)]
pub const DOT3_RGB: types::GLenum = 0x86AE;
#[allow(dead_code, non_upper_case_globals)]
pub const DOT3_RGBA: types::GLenum = 0x86AF;
#[allow(dead_code, non_upper_case_globals)]
pub const DOUBLE: types::GLenum = 0x140A;
#[allow(dead_code, non_upper_case_globals)]
pub const DOUBLEBUFFER: types::GLenum = 0x0C32;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER: types::GLenum = 0x0C01;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER0: types::GLenum = 0x8825;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER1: types::GLenum = 0x8826;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER10: types::GLenum = 0x882F;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER11: types::GLenum = 0x8830;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER12: types::GLenum = 0x8831;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER13: types::GLenum = 0x8832;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER14: types::GLenum = 0x8833;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER15: types::GLenum = 0x8834;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER2: types::GLenum = 0x8827;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER3: types::GLenum = 0x8828;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER4: types::GLenum = 0x8829;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER5: types::GLenum = 0x882A;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER6: types::GLenum = 0x882B;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER7: types::GLenum = 0x882C;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER8: types::GLenum = 0x882D;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_BUFFER9: types::GLenum = 0x882E;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_FRAMEBUFFER: types::GLenum = 0x8CA9;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_FRAMEBUFFER_BINDING: types::GLenum = 0x8CA6;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_INDIRECT_BUFFER: types::GLenum = 0x8F3F;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_INDIRECT_BUFFER_BINDING: types::GLenum = 0x8F43;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_PIXELS_APPLE: types::GLenum = 0x8A0A;
#[allow(dead_code, non_upper_case_globals)]
pub const DRAW_PIXEL_TOKEN: types::GLenum = 0x0705;
#[allow(dead_code, non_upper_case_globals)]
pub const DST_ALPHA: types::GLenum = 0x0304;
#[allow(dead_code, non_upper_case_globals)]
pub const DST_COLOR: types::GLenum = 0x0306;
#[allow(dead_code, non_upper_case_globals)]
pub const DYNAMIC_COPY: types::GLenum = 0x88EA;
#[allow(dead_code, non_upper_case_globals)]
pub const DYNAMIC_DRAW: types::GLenum = 0x88E8;
#[allow(dead_code, non_upper_case_globals)]
pub const DYNAMIC_READ: types::GLenum = 0x88E9;
#[allow(dead_code, non_upper_case_globals)]
pub const DYNAMIC_STORAGE_BIT: types::GLenum = 0x0100;
#[allow(dead_code, non_upper_case_globals)]
pub const DYNAMIC_STORAGE_BIT_EXT: types::GLenum = 0x0100;
#[allow(dead_code, non_upper_case_globals)]
pub const EDGE_FLAG: types::GLenum = 0x0B43;
#[allow(dead_code, non_upper_case_globals)]
pub const EDGE_FLAG_ARRAY: types::GLenum = 0x8079;
#[allow(dead_code, non_upper_case_globals)]
pub const EDGE_FLAG_ARRAY_BUFFER_BINDING: types::GLenum = 0x889B;
#[allow(dead_code, non_upper_case_globals)]
pub const EDGE_FLAG_ARRAY_POINTER: types::GLenum = 0x8093;
#[allow(dead_code, non_upper_case_globals)]
pub const EDGE_FLAG_ARRAY_STRIDE: types::GLenum = 0x808C;
#[allow(dead_code, non_upper_case_globals)]
pub const ELEMENT_ARRAY_BARRIER_BIT: types::GLenum = 0x00000002;
#[allow(dead_code, non_upper_case_globals)]
pub const ELEMENT_ARRAY_BUFFER: types::GLenum = 0x8893;
#[allow(dead_code, non_upper_case_globals)]
pub const ELEMENT_ARRAY_BUFFER_BINDING: types::GLenum = 0x8895;
#[allow(dead_code, non_upper_case_globals)]
pub const EMISSION: types::GLenum = 0x1600;
#[allow(dead_code, non_upper_case_globals)]
pub const ENABLE_BIT: types::GLenum = 0x00002000;
#[allow(dead_code, non_upper_case_globals)]
pub const EQUAL: types::GLenum = 0x0202;
#[allow(dead_code, non_upper_case_globals)]
pub const EQUIV: types::GLenum = 0x1509;
#[allow(dead_code, non_upper_case_globals)]
pub const EVAL_BIT: types::GLenum = 0x00010000;
#[allow(dead_code, non_upper_case_globals)]
pub const EXCLUSION_KHR: types::GLenum = 0x92A0;
#[allow(dead_code, non_upper_case_globals)]
pub const EXP: types::GLenum = 0x0800;
#[allow(dead_code, non_upper_case_globals)]
pub const EXP2: types::GLenum = 0x0801;
#[allow(dead_code, non_upper_case_globals)]
pub const EXTENSIONS: types::GLenum = 0x1F03;
#[allow(dead_code, non_upper_case_globals)]
pub const EYE_LINEAR: types::GLenum = 0x2400;
#[allow(dead_code, non_upper_case_globals)]
pub const EYE_PLANE: types::GLenum = 0x2502;
#[allow(dead_code, non_upper_case_globals)]
pub const FALSE: types::GLboolean = 0;
#[allow(dead_code, non_upper_case_globals)]
pub const FASTEST: types::GLenum = 0x1101;
#[allow(dead_code, non_upper_case_globals)]
pub const FEEDBACK: types::GLenum = 0x1C01;
#[allow(dead_code, non_upper_case_globals)]
pub const FEEDBACK_BUFFER_POINTER: types::GLenum = 0x0DF0;
#[allow(dead_code, non_upper_case_globals)]
pub const FEEDBACK_BUFFER_SIZE: types::GLenum = 0x0DF1;
#[allow(dead_code, non_upper_case_globals)]
pub const FEEDBACK_BUFFER_TYPE: types::GLenum = 0x0DF2;
#[allow(dead_code, non_upper_case_globals)]
pub const FENCE_APPLE: types::GLenum = 0x8A0B;
#[allow(dead_code, non_upper_case_globals)]
pub const FILL: types::GLenum = 0x1B02;
#[allow(dead_code, non_upper_case_globals)]
pub const FIRST_VERTEX_CONVENTION: types::GLenum = 0x8E4D;
#[allow(dead_code, non_upper_case_globals)]
pub const FIXED: types::GLenum = 0x140C;
#[allow(dead_code, non_upper_case_globals)]
pub const FIXED_ONLY: types::GLenum = 0x891D;
#[allow(dead_code, non_upper_case_globals)]
pub const FLAT: types::GLenum = 0x1D00;
#[allow(dead_code, non_upper_case_globals)]
pub const FLOAT: types::GLenum = 0x1406;
#[allow(dead_code, non_upper_case_globals)]
pub const FLOAT_32_UNSIGNED_INT_24_8_REV: types::GLenum = 0x8DAD;
#[allow(dead_code, non_upper_case_globals)]
pub const FLOAT_MAT2: types::GLenum = 0x8B5A;
#[allow(dead_code, non_upper_case_globals)]
pub const FLOAT_MAT2x3: types::GLenum = 0x8B65;
#[allow(dead_code, non_upper_case_globals)]
pub const FLOAT_MAT2x4: types::GLenum = 0x8B66;
#[allow(dead_code, non_upper_case_globals)]
pub const FLOAT_MAT3: types::GLenum = 0x8B5B;
#[allow(dead_code, non_upper_case_globals)]
pub const FLOAT_MAT3x2: types::GLenum = 0x8B67;
#[allow(dead_code, non_upper_case_globals)]
pub const FLOAT_MAT3x4: types::GLenum = 0x8B68;
#[allow(dead_code, non_upper_case_globals)]
pub const FLOAT_MAT4: types::GLenum = 0x8B5C;
#[allow(dead_code, non_upper_case_globals)]
pub const FLOAT_MAT4x2: types::GLenum = 0x8B69;
#[allow(dead_code, non_upper_case_globals)]
pub const FLOAT_MAT4x3: types::GLenum = 0x8B6A;
#[allow(dead_code, non_upper_case_globals)]
pub const FLOAT_VEC2: types::GLenum = 0x8B50;
#[allow(dead_code, non_upper_case_globals)]
pub const FLOAT_VEC3: types::GLenum = 0x8B51;
#[allow(dead_code, non_upper_case_globals)]
pub const FLOAT_VEC4: types::GLenum = 0x8B52;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG: types::GLenum = 0x0B60;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_BIT: types::GLenum = 0x00000080;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_COLOR: types::GLenum = 0x0B66;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_COORD: types::GLenum = 0x8451;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_COORDINATE: types::GLenum = 0x8451;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_COORDINATE_ARRAY: types::GLenum = 0x8457;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_COORDINATE_ARRAY_BUFFER_BINDING: types::GLenum = 0x889D;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_COORDINATE_ARRAY_POINTER: types::GLenum = 0x8456;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_COORDINATE_ARRAY_STRIDE: types::GLenum = 0x8455;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_COORDINATE_ARRAY_TYPE: types::GLenum = 0x8454;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_COORDINATE_SOURCE: types::GLenum = 0x8450;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_COORD_ARRAY: types::GLenum = 0x8457;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_COORD_ARRAY_BUFFER_BINDING: types::GLenum = 0x889D;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_COORD_ARRAY_POINTER: types::GLenum = 0x8456;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_COORD_ARRAY_STRIDE: types::GLenum = 0x8455;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_COORD_ARRAY_TYPE: types::GLenum = 0x8454;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_COORD_SRC: types::GLenum = 0x8450;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_DENSITY: types::GLenum = 0x0B62;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_END: types::GLenum = 0x0B64;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_HINT: types::GLenum = 0x0C54;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_INDEX: types::GLenum = 0x0B61;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_MODE: types::GLenum = 0x0B65;
#[allow(dead_code, non_upper_case_globals)]
pub const FOG_START: types::GLenum = 0x0B63;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAGMENT_DEPTH: types::GLenum = 0x8452;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAGMENT_SHADER: types::GLenum = 0x8B30;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAGMENT_SHADER_BIT: types::GLenum = 0x00000002;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAGMENT_SHADER_DERIVATIVE_HINT: types::GLenum = 0x8B8B;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER: types::GLenum = 0x8D40;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_ATTACHMENT_ALPHA_SIZE: types::GLenum = 0x8215;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_ATTACHMENT_ANGLE: types::GLenum = 0x93A3;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_ATTACHMENT_BLUE_SIZE: types::GLenum = 0x8214;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_ATTACHMENT_COLOR_ENCODING: types::GLenum = 0x8210;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_ATTACHMENT_COMPONENT_TYPE: types::GLenum = 0x8211;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_ATTACHMENT_DEPTH_SIZE: types::GLenum = 0x8216;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_ATTACHMENT_GREEN_SIZE: types::GLenum = 0x8213;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_ATTACHMENT_LAYERED: types::GLenum = 0x8DA7;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_ATTACHMENT_OBJECT_NAME: types::GLenum = 0x8CD1;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE: types::GLenum = 0x8CD0;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_ATTACHMENT_RED_SIZE: types::GLenum = 0x8212;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_ATTACHMENT_STENCIL_SIZE: types::GLenum = 0x8217;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_ATTACHMENT_TEXTURE_CUBE_MAP_FACE: types::GLenum = 0x8CD3;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_ATTACHMENT_TEXTURE_LAYER: types::GLenum = 0x8CD4;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_ATTACHMENT_TEXTURE_LEVEL: types::GLenum = 0x8CD2;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_BARRIER_BIT: types::GLenum = 0x00000400;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_BINDING: types::GLenum = 0x8CA6;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_COMPLETE: types::GLenum = 0x8CD5;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_DEFAULT: types::GLenum = 0x8218;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_DEFAULT_FIXED_SAMPLE_LOCATIONS: types::GLenum = 0x9314;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_DEFAULT_HEIGHT: types::GLenum = 0x9311;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_DEFAULT_SAMPLES: types::GLenum = 0x9313;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_DEFAULT_WIDTH: types::GLenum = 0x9310;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_INCOMPLETE_ATTACHMENT: types::GLenum = 0x8CD6;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_INCOMPLETE_DIMENSIONS: types::GLenum = 0x8CD9;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_INCOMPLETE_DRAW_BUFFER: types::GLenum = 0x8CDB;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_INCOMPLETE_LAYER_TARGETS: types::GLenum = 0x8DA8;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT: types::GLenum = 0x8CD7;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_INCOMPLETE_MULTISAMPLE: types::GLenum = 0x8D56;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_INCOMPLETE_READ_BUFFER: types::GLenum = 0x8CDC;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_SRGB: types::GLenum = 0x8DB9;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_UNDEFINED: types::GLenum = 0x8219;
#[allow(dead_code, non_upper_case_globals)]
pub const FRAMEBUFFER_UNSUPPORTED: types::GLenum = 0x8CDD;
#[allow(dead_code, non_upper_case_globals)]
pub const FRONT: types::GLenum = 0x0404;
#[allow(dead_code, non_upper_case_globals)]
pub const FRONT_AND_BACK: types::GLenum = 0x0408;
#[allow(dead_code, non_upper_case_globals)]
pub const FRONT_FACE: types::GLenum = 0x0B46;
#[allow(dead_code, non_upper_case_globals)]
pub const FRONT_LEFT: types::GLenum = 0x0400;
#[allow(dead_code, non_upper_case_globals)]
pub const FRONT_RIGHT: types::GLenum = 0x0401;
#[allow(dead_code, non_upper_case_globals)]
pub const FUNC_ADD: types::GLenum = 0x8006;
#[allow(dead_code, non_upper_case_globals)]
pub const FUNC_REVERSE_SUBTRACT: types::GLenum = 0x800B;
#[allow(dead_code, non_upper_case_globals)]
pub const FUNC_SUBTRACT: types::GLenum = 0x800A;
#[allow(dead_code, non_upper_case_globals)]
pub const GENERATE_MIPMAP: types::GLenum = 0x8191;
#[allow(dead_code, non_upper_case_globals)]
pub const GENERATE_MIPMAP_HINT: types::GLenum = 0x8192;
#[allow(dead_code, non_upper_case_globals)]
pub const GEOMETRY_INPUT_TYPE: types::GLenum = 0x8917;
#[allow(dead_code, non_upper_case_globals)]
pub const GEOMETRY_OUTPUT_TYPE: types::GLenum = 0x8918;
#[allow(dead_code, non_upper_case_globals)]
pub const GEOMETRY_SHADER: types::GLenum = 0x8DD9;
#[allow(dead_code, non_upper_case_globals)]
pub const GEOMETRY_VERTICES_OUT: types::GLenum = 0x8916;
#[allow(dead_code, non_upper_case_globals)]
pub const GEQUAL: types::GLenum = 0x0206;
#[allow(dead_code, non_upper_case_globals)]
pub const GPU_DISJOINT_EXT: types::GLenum = 0x8FBB;
#[allow(dead_code, non_upper_case_globals)]
pub const GREATER: types::GLenum = 0x0204;
#[allow(dead_code, non_upper_case_globals)]
pub const GREEN: types::GLenum = 0x1904;
#[allow(dead_code, non_upper_case_globals)]
pub const GREEN_BIAS: types::GLenum = 0x0D19;
#[allow(dead_code, non_upper_case_globals)]
pub const GREEN_BITS: types::GLenum = 0x0D53;
#[allow(dead_code, non_upper_case_globals)]
pub const GREEN_INTEGER: types::GLenum = 0x8D95;
#[allow(dead_code, non_upper_case_globals)]
pub const GREEN_SCALE: types::GLenum = 0x0D18;
#[allow(dead_code, non_upper_case_globals)]
pub const HALF_FLOAT: types::GLenum = 0x140B;
#[allow(dead_code, non_upper_case_globals)]
pub const HALF_FLOAT_OES: types::GLenum = 0x8D61;
#[allow(dead_code, non_upper_case_globals)]
pub const HARDLIGHT_KHR: types::GLenum = 0x929B;
#[allow(dead_code, non_upper_case_globals)]
pub const HIGH_FLOAT: types::GLenum = 0x8DF2;
#[allow(dead_code, non_upper_case_globals)]
pub const HIGH_INT: types::GLenum = 0x8DF5;
#[allow(dead_code, non_upper_case_globals)]
pub const HINT_BIT: types::GLenum = 0x00008000;
#[allow(dead_code, non_upper_case_globals)]
pub const HSL_COLOR_KHR: types::GLenum = 0x92AF;
#[allow(dead_code, non_upper_case_globals)]
pub const HSL_HUE_KHR: types::GLenum = 0x92AD;
#[allow(dead_code, non_upper_case_globals)]
pub const HSL_LUMINOSITY_KHR: types::GLenum = 0x92B0;
#[allow(dead_code, non_upper_case_globals)]
pub const HSL_SATURATION_KHR: types::GLenum = 0x92AE;
#[allow(dead_code, non_upper_case_globals)]
pub const IMAGE_2D: types::GLenum = 0x904D;
#[allow(dead_code, non_upper_case_globals)]
pub const IMAGE_2D_ARRAY: types::GLenum = 0x9053;
#[allow(dead_code, non_upper_case_globals)]
pub const IMAGE_3D: types::GLenum = 0x904E;
#[allow(dead_code, non_upper_case_globals)]
pub const IMAGE_BINDING_ACCESS: types::GLenum = 0x8F3E;
#[allow(dead_code, non_upper_case_globals)]
pub const IMAGE_BINDING_FORMAT: types::GLenum = 0x906E;
#[allow(dead_code, non_upper_case_globals)]
pub const IMAGE_BINDING_LAYER: types::GLenum = 0x8F3D;
#[allow(dead_code, non_upper_case_globals)]
pub const IMAGE_BINDING_LAYERED: types::GLenum = 0x8F3C;
#[allow(dead_code, non_upper_case_globals)]
pub const IMAGE_BINDING_LEVEL: types::GLenum = 0x8F3B;
#[allow(dead_code, non_upper_case_globals)]
pub const IMAGE_BINDING_NAME: types::GLenum = 0x8F3A;
#[allow(dead_code, non_upper_case_globals)]
pub const IMAGE_CUBE: types::GLenum = 0x9050;
#[allow(dead_code, non_upper_case_globals)]
pub const IMAGE_FORMAT_COMPATIBILITY_BY_CLASS: types::GLenum = 0x90C9;
#[allow(dead_code, non_upper_case_globals)]
pub const IMAGE_FORMAT_COMPATIBILITY_BY_SIZE: types::GLenum = 0x90C8;
#[allow(dead_code, non_upper_case_globals)]
pub const IMAGE_FORMAT_COMPATIBILITY_TYPE: types::GLenum = 0x90C7;
#[allow(dead_code, non_upper_case_globals)]
pub const IMPLEMENTATION_COLOR_READ_FORMAT: types::GLenum = 0x8B9B;
#[allow(dead_code, non_upper_case_globals)]
pub const IMPLEMENTATION_COLOR_READ_TYPE: types::GLenum = 0x8B9A;
#[allow(dead_code, non_upper_case_globals)]
pub const INCR: types::GLenum = 0x1E02;
#[allow(dead_code, non_upper_case_globals)]
pub const INCR_WRAP: types::GLenum = 0x8507;
#[allow(dead_code, non_upper_case_globals)]
pub const INDEX: types::GLenum = 0x8222;
#[allow(dead_code, non_upper_case_globals)]
pub const INDEX_ARRAY: types::GLenum = 0x8077;
#[allow(dead_code, non_upper_case_globals)]
pub const INDEX_ARRAY_BUFFER_BINDING: types::GLenum = 0x8899;
#[allow(dead_code, non_upper_case_globals)]
pub const INDEX_ARRAY_POINTER: types::GLenum = 0x8091;
#[allow(dead_code, non_upper_case_globals)]
pub const INDEX_ARRAY_STRIDE: types::GLenum = 0x8086;
#[allow(dead_code, non_upper_case_globals)]
pub const INDEX_ARRAY_TYPE: types::GLenum = 0x8085;
#[allow(dead_code, non_upper_case_globals)]
pub const INDEX_BITS: types::GLenum = 0x0D51;
#[allow(dead_code, non_upper_case_globals)]
pub const INDEX_CLEAR_VALUE: types::GLenum = 0x0C20;
#[allow(dead_code, non_upper_case_globals)]
pub const INDEX_LOGIC_OP: types::GLenum = 0x0BF1;
#[allow(dead_code, non_upper_case_globals)]
pub const INDEX_MODE: types::GLenum = 0x0C30;
#[allow(dead_code, non_upper_case_globals)]
pub const INDEX_OFFSET: types::GLenum = 0x0D13;
#[allow(dead_code, non_upper_case_globals)]
pub const INDEX_SHIFT: types::GLenum = 0x0D12;
#[allow(dead_code, non_upper_case_globals)]
pub const INDEX_WRITEMASK: types::GLenum = 0x0C21;
#[allow(dead_code, non_upper_case_globals)]
pub const INFO_LOG_LENGTH: types::GLenum = 0x8B84;
#[allow(dead_code, non_upper_case_globals)]
pub const INT: types::GLenum = 0x1404;
#[allow(dead_code, non_upper_case_globals)]
pub const INTENSITY: types::GLenum = 0x8049;
#[allow(dead_code, non_upper_case_globals)]
pub const INTENSITY12: types::GLenum = 0x804C;
#[allow(dead_code, non_upper_case_globals)]
pub const INTENSITY16: types::GLenum = 0x804D;
#[allow(dead_code, non_upper_case_globals)]
pub const INTENSITY4: types::GLenum = 0x804A;
#[allow(dead_code, non_upper_case_globals)]
pub const INTENSITY8: types::GLenum = 0x804B;
#[allow(dead_code, non_upper_case_globals)]
pub const INTERLEAVED_ATTRIBS: types::GLenum = 0x8C8C;
#[allow(dead_code, non_upper_case_globals)]
pub const INTERPOLATE: types::GLenum = 0x8575;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_2_10_10_10_REV: types::GLenum = 0x8D9F;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_IMAGE_2D: types::GLenum = 0x9058;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_IMAGE_2D_ARRAY: types::GLenum = 0x905E;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_IMAGE_3D: types::GLenum = 0x9059;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_IMAGE_CUBE: types::GLenum = 0x905B;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_SAMPLER_1D: types::GLenum = 0x8DC9;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_SAMPLER_1D_ARRAY: types::GLenum = 0x8DCE;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_SAMPLER_2D: types::GLenum = 0x8DCA;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_SAMPLER_2D_ARRAY: types::GLenum = 0x8DCF;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_SAMPLER_2D_MULTISAMPLE: types::GLenum = 0x9109;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_SAMPLER_2D_MULTISAMPLE_ARRAY: types::GLenum = 0x910C;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_SAMPLER_2D_RECT: types::GLenum = 0x8DCD;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_SAMPLER_3D: types::GLenum = 0x8DCB;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_SAMPLER_BUFFER: types::GLenum = 0x8DD0;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_SAMPLER_CUBE: types::GLenum = 0x8DCC;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_VEC2: types::GLenum = 0x8B53;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_VEC3: types::GLenum = 0x8B54;
#[allow(dead_code, non_upper_case_globals)]
pub const INT_VEC4: types::GLenum = 0x8B55;
#[allow(dead_code, non_upper_case_globals)]
pub const INVALID_ENUM: types::GLenum = 0x0500;
#[allow(dead_code, non_upper_case_globals)]
pub const INVALID_FRAMEBUFFER_OPERATION: types::GLenum = 0x0506;
#[allow(dead_code, non_upper_case_globals)]
pub const INVALID_INDEX: types::GLuint = 0xFFFFFFFF;
#[allow(dead_code, non_upper_case_globals)]
pub const INVALID_OPERATION: types::GLenum = 0x0502;
#[allow(dead_code, non_upper_case_globals)]
pub const INVALID_VALUE: types::GLenum = 0x0501;
#[allow(dead_code, non_upper_case_globals)]
pub const INVERT: types::GLenum = 0x150A;
#[allow(dead_code, non_upper_case_globals)]
pub const IS_ROW_MAJOR: types::GLenum = 0x9300;
#[allow(dead_code, non_upper_case_globals)]
pub const KEEP: types::GLenum = 0x1E00;
#[allow(dead_code, non_upper_case_globals)]
pub const LAST_VERTEX_CONVENTION: types::GLenum = 0x8E4E;
#[allow(dead_code, non_upper_case_globals)]
pub const LEFT: types::GLenum = 0x0406;
#[allow(dead_code, non_upper_case_globals)]
pub const LEQUAL: types::GLenum = 0x0203;
#[allow(dead_code, non_upper_case_globals)]
pub const LESS: types::GLenum = 0x0201;
#[allow(dead_code, non_upper_case_globals)]
pub const LIGHT0: types::GLenum = 0x4000;
#[allow(dead_code, non_upper_case_globals)]
pub const LIGHT1: types::GLenum = 0x4001;
#[allow(dead_code, non_upper_case_globals)]
pub const LIGHT2: types::GLenum = 0x4002;
#[allow(dead_code, non_upper_case_globals)]
pub const LIGHT3: types::GLenum = 0x4003;
#[allow(dead_code, non_upper_case_globals)]
pub const LIGHT4: types::GLenum = 0x4004;
#[allow(dead_code, non_upper_case_globals)]
pub const LIGHT5: types::GLenum = 0x4005;
#[allow(dead_code, non_upper_case_globals)]
pub const LIGHT6: types::GLenum = 0x4006;
#[allow(dead_code, non_upper_case_globals)]
pub const LIGHT7: types::GLenum = 0x4007;
#[allow(dead_code, non_upper_case_globals)]
pub const LIGHTEN_KHR: types::GLenum = 0x9298;
#[allow(dead_code, non_upper_case_globals)]
pub const LIGHTING: types::GLenum = 0x0B50;
#[allow(dead_code, non_upper_case_globals)]
pub const LIGHTING_BIT: types::GLenum = 0x00000040;
#[allow(dead_code, non_upper_case_globals)]
pub const LIGHT_MODEL_AMBIENT: types::GLenum = 0x0B53;
#[allow(dead_code, non_upper_case_globals)]
pub const LIGHT_MODEL_COLOR_CONTROL: types::GLenum = 0x81F8;
#[allow(dead_code, non_upper_case_globals)]
pub const LIGHT_MODEL_LOCAL_VIEWER: types::GLenum = 0x0B51;
#[allow(dead_code, non_upper_case_globals)]
pub const LIGHT_MODEL_TWO_SIDE: types::GLenum = 0x0B52;
#[allow(dead_code, non_upper_case_globals)]
pub const LINE: types::GLenum = 0x1B01;
#[allow(dead_code, non_upper_case_globals)]
pub const LINEAR: types::GLenum = 0x2601;
#[allow(dead_code, non_upper_case_globals)]
pub const LINEAR_ATTENUATION: types::GLenum = 0x1208;
#[allow(dead_code, non_upper_case_globals)]
pub const LINEAR_MIPMAP_LINEAR: types::GLenum = 0x2703;
#[allow(dead_code, non_upper_case_globals)]
pub const LINEAR_MIPMAP_NEAREST: types::GLenum = 0x2701;
#[allow(dead_code, non_upper_case_globals)]
pub const LINES: types::GLenum = 0x0001;
#[allow(dead_code, non_upper_case_globals)]
pub const LINES_ADJACENCY: types::GLenum = 0x000A;
#[allow(dead_code, non_upper_case_globals)]
pub const LINE_BIT: types::GLenum = 0x00000004;
#[allow(dead_code, non_upper_case_globals)]
pub const LINE_LOOP: types::GLenum = 0x0002;
#[allow(dead_code, non_upper_case_globals)]
pub const LINE_RESET_TOKEN: types::GLenum = 0x0707;
#[allow(dead_code, non_upper_case_globals)]
pub const LINE_SMOOTH: types::GLenum = 0x0B20;
#[allow(dead_code, non_upper_case_globals)]
pub const LINE_SMOOTH_HINT: types::GLenum = 0x0C52;
#[allow(dead_code, non_upper_case_globals)]
pub const LINE_STIPPLE: types::GLenum = 0x0B24;
#[allow(dead_code, non_upper_case_globals)]
pub const LINE_STIPPLE_PATTERN: types::GLenum = 0x0B25;
#[allow(dead_code, non_upper_case_globals)]
pub const LINE_STIPPLE_REPEAT: types::GLenum = 0x0B26;
#[allow(dead_code, non_upper_case_globals)]
pub const LINE_STRIP: types::GLenum = 0x0003;
#[allow(dead_code, non_upper_case_globals)]
pub const LINE_STRIP_ADJACENCY: types::GLenum = 0x000B;
#[allow(dead_code, non_upper_case_globals)]
pub const LINE_TOKEN: types::GLenum = 0x0702;
#[allow(dead_code, non_upper_case_globals)]
pub const LINE_WIDTH: types::GLenum = 0x0B21;
#[allow(dead_code, non_upper_case_globals)]
pub const LINE_WIDTH_GRANULARITY: types::GLenum = 0x0B23;
#[allow(dead_code, non_upper_case_globals)]
pub const LINE_WIDTH_RANGE: types::GLenum = 0x0B22;
#[allow(dead_code, non_upper_case_globals)]
pub const LINK_STATUS: types::GLenum = 0x8B82;
#[allow(dead_code, non_upper_case_globals)]
pub const LIST_BASE: types::GLenum = 0x0B32;
#[allow(dead_code, non_upper_case_globals)]
pub const LIST_BIT: types::GLenum = 0x00020000;
#[allow(dead_code, non_upper_case_globals)]
pub const LIST_INDEX: types::GLenum = 0x0B33;
#[allow(dead_code, non_upper_case_globals)]
pub const LIST_MODE: types::GLenum = 0x0B30;
#[allow(dead_code, non_upper_case_globals)]
pub const LOAD: types::GLenum = 0x0101;
#[allow(dead_code, non_upper_case_globals)]
pub const LOCATION: types::GLenum = 0x930E;
#[allow(dead_code, non_upper_case_globals)]
pub const LOGIC_OP: types::GLenum = 0x0BF1;
#[allow(dead_code, non_upper_case_globals)]
pub const LOGIC_OP_MODE: types::GLenum = 0x0BF0;
#[allow(dead_code, non_upper_case_globals)]
pub const LOWER_LEFT: types::GLenum = 0x8CA1;
#[allow(dead_code, non_upper_case_globals)]
pub const LOW_FLOAT: types::GLenum = 0x8DF0;
#[allow(dead_code, non_upper_case_globals)]
pub const LOW_INT: types::GLenum = 0x8DF3;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE: types::GLenum = 0x1909;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE12: types::GLenum = 0x8041;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE12_ALPHA12: types::GLenum = 0x8047;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE12_ALPHA4: types::GLenum = 0x8046;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE16: types::GLenum = 0x8042;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE16F_EXT: types::GLenum = 0x881E;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE16_ALPHA16: types::GLenum = 0x8048;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE32F_EXT: types::GLenum = 0x8818;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE4: types::GLenum = 0x803F;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE4_ALPHA4: types::GLenum = 0x8043;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE6_ALPHA2: types::GLenum = 0x8044;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE8: types::GLenum = 0x8040;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE8_ALPHA8: types::GLenum = 0x8045;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE8_ALPHA8_EXT: types::GLenum = 0x8045;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE8_EXT: types::GLenum = 0x8040;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE_ALPHA: types::GLenum = 0x190A;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE_ALPHA16F_EXT: types::GLenum = 0x881F;
#[allow(dead_code, non_upper_case_globals)]
pub const LUMINANCE_ALPHA32F_EXT: types::GLenum = 0x8819;
#[allow(dead_code, non_upper_case_globals)]
pub const MAJOR_VERSION: types::GLenum = 0x821B;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP1_COLOR_4: types::GLenum = 0x0D90;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP1_GRID_DOMAIN: types::GLenum = 0x0DD0;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP1_GRID_SEGMENTS: types::GLenum = 0x0DD1;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP1_INDEX: types::GLenum = 0x0D91;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP1_NORMAL: types::GLenum = 0x0D92;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP1_TEXTURE_COORD_1: types::GLenum = 0x0D93;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP1_TEXTURE_COORD_2: types::GLenum = 0x0D94;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP1_TEXTURE_COORD_3: types::GLenum = 0x0D95;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP1_TEXTURE_COORD_4: types::GLenum = 0x0D96;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP1_VERTEX_3: types::GLenum = 0x0D97;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP1_VERTEX_4: types::GLenum = 0x0D98;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP2_COLOR_4: types::GLenum = 0x0DB0;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP2_GRID_DOMAIN: types::GLenum = 0x0DD2;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP2_GRID_SEGMENTS: types::GLenum = 0x0DD3;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP2_INDEX: types::GLenum = 0x0DB1;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP2_NORMAL: types::GLenum = 0x0DB2;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP2_TEXTURE_COORD_1: types::GLenum = 0x0DB3;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP2_TEXTURE_COORD_2: types::GLenum = 0x0DB4;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP2_TEXTURE_COORD_3: types::GLenum = 0x0DB5;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP2_TEXTURE_COORD_4: types::GLenum = 0x0DB6;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP2_VERTEX_3: types::GLenum = 0x0DB7;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP2_VERTEX_4: types::GLenum = 0x0DB8;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP_COHERENT_BIT: types::GLenum = 0x0080;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP_COHERENT_BIT_EXT: types::GLenum = 0x0080;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP_COLOR: types::GLenum = 0x0D10;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP_FLUSH_EXPLICIT_BIT: types::GLenum = 0x0010;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP_INVALIDATE_BUFFER_BIT: types::GLenum = 0x0008;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP_INVALIDATE_RANGE_BIT: types::GLenum = 0x0004;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP_PERSISTENT_BIT: types::GLenum = 0x0040;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP_PERSISTENT_BIT_EXT: types::GLenum = 0x0040;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP_READ_BIT: types::GLenum = 0x0001;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP_STENCIL: types::GLenum = 0x0D11;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP_UNSYNCHRONIZED_BIT: types::GLenum = 0x0020;
#[allow(dead_code, non_upper_case_globals)]
pub const MAP_WRITE_BIT: types::GLenum = 0x0002;
#[allow(dead_code, non_upper_case_globals)]
pub const MATRIX_MODE: types::GLenum = 0x0BA0;
#[allow(dead_code, non_upper_case_globals)]
pub const MATRIX_STRIDE: types::GLenum = 0x92FF;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX: types::GLenum = 0x8008;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_3D_TEXTURE_SIZE: types::GLenum = 0x8073;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_ARRAY_TEXTURE_LAYERS: types::GLenum = 0x88FF;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_ATOMIC_COUNTER_BUFFER_BINDINGS: types::GLenum = 0x92DC;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_ATOMIC_COUNTER_BUFFER_SIZE: types::GLenum = 0x92D8;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_ATTRIB_STACK_DEPTH: types::GLenum = 0x0D35;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_CLIENT_ATTRIB_STACK_DEPTH: types::GLenum = 0x0D3B;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_CLIP_DISTANCES: types::GLenum = 0x0D32;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_CLIP_PLANES: types::GLenum = 0x0D32;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COLOR_ATTACHMENTS: types::GLenum = 0x8CDF;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COLOR_TEXTURE_SAMPLES: types::GLenum = 0x910E;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMBINED_ATOMIC_COUNTERS: types::GLenum = 0x92D7;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMBINED_ATOMIC_COUNTER_BUFFERS: types::GLenum = 0x92D1;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMBINED_COMPUTE_UNIFORM_COMPONENTS: types::GLenum = 0x8266;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMBINED_FRAGMENT_UNIFORM_COMPONENTS: types::GLenum = 0x8A33;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMBINED_GEOMETRY_UNIFORM_COMPONENTS: types::GLenum = 0x8A32;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMBINED_IMAGE_UNIFORMS: types::GLenum = 0x90CF;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMBINED_IMAGE_UNITS_AND_FRAGMENT_OUTPUTS: types::GLenum = 0x8F39;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMBINED_SHADER_OUTPUT_RESOURCES: types::GLenum = 0x8F39;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMBINED_SHADER_STORAGE_BLOCKS: types::GLenum = 0x90DC;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMBINED_TEXTURE_IMAGE_UNITS: types::GLenum = 0x8B4D;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMBINED_UNIFORM_BLOCKS: types::GLenum = 0x8A2E;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMBINED_VERTEX_UNIFORM_COMPONENTS: types::GLenum = 0x8A31;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMPUTE_ATOMIC_COUNTERS: types::GLenum = 0x8265;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMPUTE_ATOMIC_COUNTER_BUFFERS: types::GLenum = 0x8264;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMPUTE_IMAGE_UNIFORMS: types::GLenum = 0x91BD;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMPUTE_SHADER_STORAGE_BLOCKS: types::GLenum = 0x90DB;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMPUTE_SHARED_MEMORY_SIZE: types::GLenum = 0x8262;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMPUTE_TEXTURE_IMAGE_UNITS: types::GLenum = 0x91BC;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMPUTE_UNIFORM_BLOCKS: types::GLenum = 0x91BB;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMPUTE_UNIFORM_COMPONENTS: types::GLenum = 0x8263;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMPUTE_WORK_GROUP_COUNT: types::GLenum = 0x91BE;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMPUTE_WORK_GROUP_INVOCATIONS: types::GLenum = 0x90EB;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_COMPUTE_WORK_GROUP_SIZE: types::GLenum = 0x91BF;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_CUBE_MAP_TEXTURE_SIZE: types::GLenum = 0x851C;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_DEBUG_GROUP_STACK_DEPTH: types::GLenum = 0x826C;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_DEBUG_GROUP_STACK_DEPTH_KHR: types::GLenum = 0x826C;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_DEBUG_LOGGED_MESSAGES: types::GLenum = 0x9144;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_DEBUG_LOGGED_MESSAGES_KHR: types::GLenum = 0x9144;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_DEBUG_MESSAGE_LENGTH: types::GLenum = 0x9143;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_DEBUG_MESSAGE_LENGTH_KHR: types::GLenum = 0x9143;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_DEPTH_TEXTURE_SAMPLES: types::GLenum = 0x910F;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_DRAW_BUFFERS: types::GLenum = 0x8824;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_DUAL_SOURCE_DRAW_BUFFERS: types::GLenum = 0x88FC;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_ELEMENTS_INDICES: types::GLenum = 0x80E9;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_ELEMENTS_VERTICES: types::GLenum = 0x80E8;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_ELEMENT_INDEX: types::GLenum = 0x8D6B;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_EVAL_ORDER: types::GLenum = 0x0D30;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_FRAGMENT_ATOMIC_COUNTERS: types::GLenum = 0x92D6;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_FRAGMENT_ATOMIC_COUNTER_BUFFERS: types::GLenum = 0x92D0;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_FRAGMENT_IMAGE_UNIFORMS: types::GLenum = 0x90CE;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_FRAGMENT_INPUT_COMPONENTS: types::GLenum = 0x9125;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_FRAGMENT_SHADER_STORAGE_BLOCKS: types::GLenum = 0x90DA;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_FRAGMENT_UNIFORM_BLOCKS: types::GLenum = 0x8A2D;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_FRAGMENT_UNIFORM_COMPONENTS: types::GLenum = 0x8B49;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_FRAGMENT_UNIFORM_VECTORS: types::GLenum = 0x8DFD;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_FRAMEBUFFER_HEIGHT: types::GLenum = 0x9316;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_FRAMEBUFFER_SAMPLES: types::GLenum = 0x9318;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_FRAMEBUFFER_WIDTH: types::GLenum = 0x9315;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_GEOMETRY_INPUT_COMPONENTS: types::GLenum = 0x9123;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_GEOMETRY_OUTPUT_COMPONENTS: types::GLenum = 0x9124;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_GEOMETRY_OUTPUT_VERTICES: types::GLenum = 0x8DE0;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_GEOMETRY_SHADER_STORAGE_BLOCKS: types::GLenum = 0x90D7;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_GEOMETRY_TEXTURE_IMAGE_UNITS: types::GLenum = 0x8C29;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_GEOMETRY_TOTAL_OUTPUT_COMPONENTS: types::GLenum = 0x8DE1;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_GEOMETRY_UNIFORM_BLOCKS: types::GLenum = 0x8A2C;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_GEOMETRY_UNIFORM_COMPONENTS: types::GLenum = 0x8DDF;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_IMAGE_UNITS: types::GLenum = 0x8F38;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_INTEGER_SAMPLES: types::GLenum = 0x9110;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_LABEL_LENGTH: types::GLenum = 0x82E8;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_LABEL_LENGTH_KHR: types::GLenum = 0x82E8;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_LIGHTS: types::GLenum = 0x0D31;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_LIST_NESTING: types::GLenum = 0x0B31;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_MODELVIEW_STACK_DEPTH: types::GLenum = 0x0D36;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_NAME_LENGTH: types::GLenum = 0x92F6;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_NAME_STACK_DEPTH: types::GLenum = 0x0D37;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_NUM_ACTIVE_VARIABLES: types::GLenum = 0x92F7;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_PIXEL_MAP_TABLE: types::GLenum = 0x0D34;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_PROGRAM_TEXEL_OFFSET: types::GLenum = 0x8905;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_PROGRAM_TEXTURE_GATHER_OFFSET: types::GLenum = 0x8E5F;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_PROJECTION_STACK_DEPTH: types::GLenum = 0x0D38;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_RECTANGLE_TEXTURE_SIZE: types::GLenum = 0x84F8;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_RECTANGLE_TEXTURE_SIZE_ARB: types::GLenum = 0x84F8;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_RENDERBUFFER_SIZE: types::GLenum = 0x84E8;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_SAMPLES: types::GLenum = 0x8D57;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_SAMPLE_MASK_WORDS: types::GLenum = 0x8E59;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_SERVER_WAIT_TIMEOUT: types::GLenum = 0x9111;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_SHADER_PIXEL_LOCAL_STORAGE_FAST_SIZE_EXT: types::GLenum = 0x8F63;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_SHADER_PIXEL_LOCAL_STORAGE_SIZE_EXT: types::GLenum = 0x8F67;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_SHADER_STORAGE_BLOCK_SIZE: types::GLenum = 0x90DE;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_SHADER_STORAGE_BUFFER_BINDINGS: types::GLenum = 0x90DD;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_TESS_CONTROL_SHADER_STORAGE_BLOCKS: types::GLenum = 0x90D8;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_TESS_EVALUATION_SHADER_STORAGE_BLOCKS: types::GLenum = 0x90D9;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_TEXTURE_BUFFER_SIZE: types::GLenum = 0x8C2B;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_TEXTURE_COORDS: types::GLenum = 0x8871;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_TEXTURE_IMAGE_UNITS: types::GLenum = 0x8872;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_TEXTURE_LOD_BIAS: types::GLenum = 0x84FD;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_TEXTURE_MAX_ANISOTROPY_EXT: types::GLenum = 0x84FF;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_TEXTURE_SIZE: types::GLenum = 0x0D33;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_TEXTURE_STACK_DEPTH: types::GLenum = 0x0D39;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_TEXTURE_UNITS: types::GLenum = 0x84E2;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_TRANSFORM_FEEDBACK_INTERLEAVED_COMPONENTS: types::GLenum = 0x8C8A;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_TRANSFORM_FEEDBACK_SEPARATE_ATTRIBS: types::GLenum = 0x8C8B;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_TRANSFORM_FEEDBACK_SEPARATE_COMPONENTS: types::GLenum = 0x8C80;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_UNIFORM_BLOCK_SIZE: types::GLenum = 0x8A30;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_UNIFORM_BUFFER_BINDINGS: types::GLenum = 0x8A2F;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_UNIFORM_LOCATIONS: types::GLenum = 0x826E;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VARYING_COMPONENTS: types::GLenum = 0x8B4B;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VARYING_FLOATS: types::GLenum = 0x8B4B;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VARYING_VECTORS: types::GLenum = 0x8DFC;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VERTEX_ATOMIC_COUNTERS: types::GLenum = 0x92D2;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VERTEX_ATOMIC_COUNTER_BUFFERS: types::GLenum = 0x92CC;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VERTEX_ATTRIBS: types::GLenum = 0x8869;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VERTEX_ATTRIB_BINDINGS: types::GLenum = 0x82DA;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VERTEX_ATTRIB_RELATIVE_OFFSET: types::GLenum = 0x82D9;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VERTEX_ATTRIB_STRIDE: types::GLenum = 0x82E5;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VERTEX_IMAGE_UNIFORMS: types::GLenum = 0x90CA;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VERTEX_OUTPUT_COMPONENTS: types::GLenum = 0x9122;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VERTEX_SHADER_STORAGE_BLOCKS: types::GLenum = 0x90D6;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VERTEX_TEXTURE_IMAGE_UNITS: types::GLenum = 0x8B4C;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VERTEX_UNIFORM_BLOCKS: types::GLenum = 0x8A2B;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VERTEX_UNIFORM_COMPONENTS: types::GLenum = 0x8B4A;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VERTEX_UNIFORM_VECTORS: types::GLenum = 0x8DFB;
#[allow(dead_code, non_upper_case_globals)]
pub const MAX_VIEWPORT_DIMS: types::GLenum = 0x0D3A;
#[allow(dead_code, non_upper_case_globals)]
pub const MEDIUM_FLOAT: types::GLenum = 0x8DF1;
#[allow(dead_code, non_upper_case_globals)]
pub const MEDIUM_INT: types::GLenum = 0x8DF4;
#[allow(dead_code, non_upper_case_globals)]
pub const MIN: types::GLenum = 0x8007;
#[allow(dead_code, non_upper_case_globals)]
pub const MINOR_VERSION: types::GLenum = 0x821C;
#[allow(dead_code, non_upper_case_globals)]
pub const MIN_PROGRAM_TEXEL_OFFSET: types::GLenum = 0x8904;
#[allow(dead_code, non_upper_case_globals)]
pub const MIN_PROGRAM_TEXTURE_GATHER_OFFSET: types::GLenum = 0x8E5E;
#[allow(dead_code, non_upper_case_globals)]
pub const MIRRORED_REPEAT: types::GLenum = 0x8370;
#[allow(dead_code, non_upper_case_globals)]
pub const MODELVIEW: types::GLenum = 0x1700;
#[allow(dead_code, non_upper_case_globals)]
pub const MODELVIEW_MATRIX: types::GLenum = 0x0BA6;
#[allow(dead_code, non_upper_case_globals)]
pub const MODELVIEW_STACK_DEPTH: types::GLenum = 0x0BA3;
#[allow(dead_code, non_upper_case_globals)]
pub const MODULATE: types::GLenum = 0x2100;
#[allow(dead_code, non_upper_case_globals)]
pub const MULT: types::GLenum = 0x0103;
#[allow(dead_code, non_upper_case_globals)]
pub const MULTIPLY_KHR: types::GLenum = 0x9294;
#[allow(dead_code, non_upper_case_globals)]
pub const MULTISAMPLE: types::GLenum = 0x809D;
#[allow(dead_code, non_upper_case_globals)]
pub const MULTISAMPLE_BIT: types::GLenum = 0x20000000;
#[allow(dead_code, non_upper_case_globals)]
pub const MULTISAMPLE_BUFFER_BIT0_QCOM: types::GLenum = 0x01000000;
#[allow(dead_code, non_upper_case_globals)]
pub const MULTISAMPLE_BUFFER_BIT1_QCOM: types::GLenum = 0x02000000;
#[allow(dead_code, non_upper_case_globals)]
pub const MULTISAMPLE_BUFFER_BIT2_QCOM: types::GLenum = 0x04000000;
#[allow(dead_code, non_upper_case_globals)]
pub const MULTISAMPLE_BUFFER_BIT3_QCOM: types::GLenum = 0x08000000;
#[allow(dead_code, non_upper_case_globals)]
pub const MULTISAMPLE_BUFFER_BIT4_QCOM: types::GLenum = 0x10000000;
#[allow(dead_code, non_upper_case_globals)]
pub const MULTISAMPLE_BUFFER_BIT5_QCOM: types::GLenum = 0x20000000;
#[allow(dead_code, non_upper_case_globals)]
pub const MULTISAMPLE_BUFFER_BIT6_QCOM: types::GLenum = 0x40000000;
#[allow(dead_code, non_upper_case_globals)]
pub const MULTISAMPLE_BUFFER_BIT7_QCOM: types::GLenum = 0x80000000;
#[allow(dead_code, non_upper_case_globals)]
pub const N3F_V3F: types::GLenum = 0x2A25;
#[allow(dead_code, non_upper_case_globals)]
pub const NAME_LENGTH: types::GLenum = 0x92F9;
#[allow(dead_code, non_upper_case_globals)]
pub const NAME_STACK_DEPTH: types::GLenum = 0x0D70;
#[allow(dead_code, non_upper_case_globals)]
pub const NAND: types::GLenum = 0x150E;
#[allow(dead_code, non_upper_case_globals)]
pub const NEAREST: types::GLenum = 0x2600;
#[allow(dead_code, non_upper_case_globals)]
pub const NEAREST_MIPMAP_LINEAR: types::GLenum = 0x2702;
#[allow(dead_code, non_upper_case_globals)]
pub const NEAREST_MIPMAP_NEAREST: types::GLenum = 0x2700;
#[allow(dead_code, non_upper_case_globals)]
pub const NEVER: types::GLenum = 0x0200;
#[allow(dead_code, non_upper_case_globals)]
pub const NICEST: types::GLenum = 0x1102;
#[allow(dead_code, non_upper_case_globals)]
pub const NONE: types::GLenum = 0;
#[allow(dead_code, non_upper_case_globals)]
pub const NOOP: types::GLenum = 0x1505;
#[allow(dead_code, non_upper_case_globals)]
pub const NOR: types::GLenum = 0x1508;
#[allow(dead_code, non_upper_case_globals)]
pub const NORMALIZE: types::GLenum = 0x0BA1;
#[allow(dead_code, non_upper_case_globals)]
pub const NORMAL_ARRAY: types::GLenum = 0x8075;
#[allow(dead_code, non_upper_case_globals)]
pub const NORMAL_ARRAY_BUFFER_BINDING: types::GLenum = 0x8897;
#[allow(dead_code, non_upper_case_globals)]
pub const NORMAL_ARRAY_POINTER: types::GLenum = 0x808F;
#[allow(dead_code, non_upper_case_globals)]
pub const NORMAL_ARRAY_STRIDE: types::GLenum = 0x807F;
#[allow(dead_code, non_upper_case_globals)]
pub const NORMAL_ARRAY_TYPE: types::GLenum = 0x807E;
#[allow(dead_code, non_upper_case_globals)]
pub const NORMAL_MAP: types::GLenum = 0x8511;
#[allow(dead_code, non_upper_case_globals)]
pub const NOTEQUAL: types::GLenum = 0x0205;
#[allow(dead_code, non_upper_case_globals)]
pub const NO_ERROR: types::GLenum = 0;
#[allow(dead_code, non_upper_case_globals)]
pub const NUM_ACTIVE_VARIABLES: types::GLenum = 0x9304;
#[allow(dead_code, non_upper_case_globals)]
pub const NUM_COMPRESSED_TEXTURE_FORMATS: types::GLenum = 0x86A2;
#[allow(dead_code, non_upper_case_globals)]
pub const NUM_EXTENSIONS: types::GLenum = 0x821D;
#[allow(dead_code, non_upper_case_globals)]
pub const NUM_PROGRAM_BINARY_FORMATS: types::GLenum = 0x87FE;
#[allow(dead_code, non_upper_case_globals)]
pub const NUM_SAMPLE_COUNTS: types::GLenum = 0x9380;
#[allow(dead_code, non_upper_case_globals)]
pub const NUM_SHADER_BINARY_FORMATS: types::GLenum = 0x8DF9;
#[allow(dead_code, non_upper_case_globals)]
pub const OBJECT_LINEAR: types::GLenum = 0x2401;
#[allow(dead_code, non_upper_case_globals)]
pub const OBJECT_PLANE: types::GLenum = 0x2501;
#[allow(dead_code, non_upper_case_globals)]
pub const OBJECT_TYPE: types::GLenum = 0x9112;
#[allow(dead_code, non_upper_case_globals)]
pub const OFFSET: types::GLenum = 0x92FC;
#[allow(dead_code, non_upper_case_globals)]
pub const ONE: types::GLenum = 1;
#[allow(dead_code, non_upper_case_globals)]
pub const ONE_MINUS_CONSTANT_ALPHA: types::GLenum = 0x8004;
#[allow(dead_code, non_upper_case_globals)]
pub const ONE_MINUS_CONSTANT_COLOR: types::GLenum = 0x8002;
#[allow(dead_code, non_upper_case_globals)]
pub const ONE_MINUS_DST_ALPHA: types::GLenum = 0x0305;
#[allow(dead_code, non_upper_case_globals)]
pub const ONE_MINUS_DST_COLOR: types::GLenum = 0x0307;
#[allow(dead_code, non_upper_case_globals)]
pub const ONE_MINUS_SRC1_ALPHA: types::GLenum = 0x88FB;
#[allow(dead_code, non_upper_case_globals)]
pub const ONE_MINUS_SRC1_COLOR: types::GLenum = 0x88FA;
#[allow(dead_code, non_upper_case_globals)]
pub const ONE_MINUS_SRC_ALPHA: types::GLenum = 0x0303;
#[allow(dead_code, non_upper_case_globals)]
pub const ONE_MINUS_SRC_COLOR: types::GLenum = 0x0301;
#[allow(dead_code, non_upper_case_globals)]
pub const OPERAND0_ALPHA: types::GLenum = 0x8598;
#[allow(dead_code, non_upper_case_globals)]
pub const OPERAND0_RGB: types::GLenum = 0x8590;
#[allow(dead_code, non_upper_case_globals)]
pub const OPERAND1_ALPHA: types::GLenum = 0x8599;
#[allow(dead_code, non_upper_case_globals)]
pub const OPERAND1_RGB: types::GLenum = 0x8591;
#[allow(dead_code, non_upper_case_globals)]
pub const OPERAND2_ALPHA: types::GLenum = 0x859A;
#[allow(dead_code, non_upper_case_globals)]
pub const OPERAND2_RGB: types::GLenum = 0x8592;
#[allow(dead_code, non_upper_case_globals)]
pub const OR: types::GLenum = 0x1507;
#[allow(dead_code, non_upper_case_globals)]
pub const ORDER: types::GLenum = 0x0A01;
#[allow(dead_code, non_upper_case_globals)]
pub const OR_INVERTED: types::GLenum = 0x150D;
#[allow(dead_code, non_upper_case_globals)]
pub const OR_REVERSE: types::GLenum = 0x150B;
#[allow(dead_code, non_upper_case_globals)]
pub const OUT_OF_MEMORY: types::GLenum = 0x0505;
#[allow(dead_code, non_upper_case_globals)]
pub const OVERLAY_KHR: types::GLenum = 0x9296;
#[allow(dead_code, non_upper_case_globals)]
pub const PACK_ALIGNMENT: types::GLenum = 0x0D05;
#[allow(dead_code, non_upper_case_globals)]
pub const PACK_IMAGE_HEIGHT: types::GLenum = 0x806C;
#[allow(dead_code, non_upper_case_globals)]
pub const PACK_LSB_FIRST: types::GLenum = 0x0D01;
#[allow(dead_code, non_upper_case_globals)]
pub const PACK_ROW_LENGTH: types::GLenum = 0x0D02;
#[allow(dead_code, non_upper_case_globals)]
pub const PACK_SKIP_IMAGES: types::GLenum = 0x806B;
#[allow(dead_code, non_upper_case_globals)]
pub const PACK_SKIP_PIXELS: types::GLenum = 0x0D04;
#[allow(dead_code, non_upper_case_globals)]
pub const PACK_SKIP_ROWS: types::GLenum = 0x0D03;
#[allow(dead_code, non_upper_case_globals)]
pub const PACK_SWAP_BYTES: types::GLenum = 0x0D00;
#[allow(dead_code, non_upper_case_globals)]
pub const PASS_THROUGH_TOKEN: types::GLenum = 0x0700;
#[allow(dead_code, non_upper_case_globals)]
pub const PERSPECTIVE_CORRECTION_HINT: types::GLenum = 0x0C50;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_BUFFER_BARRIER_BIT: types::GLenum = 0x00000080;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_A_TO_A: types::GLenum = 0x0C79;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_A_TO_A_SIZE: types::GLenum = 0x0CB9;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_B_TO_B: types::GLenum = 0x0C78;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_B_TO_B_SIZE: types::GLenum = 0x0CB8;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_G_TO_G: types::GLenum = 0x0C77;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_G_TO_G_SIZE: types::GLenum = 0x0CB7;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_I_TO_A: types::GLenum = 0x0C75;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_I_TO_A_SIZE: types::GLenum = 0x0CB5;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_I_TO_B: types::GLenum = 0x0C74;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_I_TO_B_SIZE: types::GLenum = 0x0CB4;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_I_TO_G: types::GLenum = 0x0C73;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_I_TO_G_SIZE: types::GLenum = 0x0CB3;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_I_TO_I: types::GLenum = 0x0C70;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_I_TO_I_SIZE: types::GLenum = 0x0CB0;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_I_TO_R: types::GLenum = 0x0C72;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_I_TO_R_SIZE: types::GLenum = 0x0CB2;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_R_TO_R: types::GLenum = 0x0C76;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_R_TO_R_SIZE: types::GLenum = 0x0CB6;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_S_TO_S: types::GLenum = 0x0C71;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MAP_S_TO_S_SIZE: types::GLenum = 0x0CB1;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_MODE_BIT: types::GLenum = 0x00000020;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_PACK_BUFFER: types::GLenum = 0x88EB;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_PACK_BUFFER_BINDING: types::GLenum = 0x88ED;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_UNPACK_BUFFER: types::GLenum = 0x88EC;
#[allow(dead_code, non_upper_case_globals)]
pub const PIXEL_UNPACK_BUFFER_BINDING: types::GLenum = 0x88EF;
#[allow(dead_code, non_upper_case_globals)]
pub const POINT: types::GLenum = 0x1B00;
#[allow(dead_code, non_upper_case_globals)]
pub const POINTS: types::GLenum = 0x0000;
#[allow(dead_code, non_upper_case_globals)]
pub const POINT_BIT: types::GLenum = 0x00000002;
#[allow(dead_code, non_upper_case_globals)]
pub const POINT_DISTANCE_ATTENUATION: types::GLenum = 0x8129;
#[allow(dead_code, non_upper_case_globals)]
pub const POINT_FADE_THRESHOLD_SIZE: types::GLenum = 0x8128;
#[allow(dead_code, non_upper_case_globals)]
pub const POINT_SIZE: types::GLenum = 0x0B11;
#[allow(dead_code, non_upper_case_globals)]
pub const POINT_SIZE_GRANULARITY: types::GLenum = 0x0B13;
#[allow(dead_code, non_upper_case_globals)]
pub const POINT_SIZE_MAX: types::GLenum = 0x8127;
#[allow(dead_code, non_upper_case_globals)]
pub const POINT_SIZE_MIN: types::GLenum = 0x8126;
#[allow(dead_code, non_upper_case_globals)]
pub const POINT_SIZE_RANGE: types::GLenum = 0x0B12;
#[allow(dead_code, non_upper_case_globals)]
pub const POINT_SMOOTH: types::GLenum = 0x0B10;
#[allow(dead_code, non_upper_case_globals)]
pub const POINT_SMOOTH_HINT: types::GLenum = 0x0C51;
#[allow(dead_code, non_upper_case_globals)]
pub const POINT_SPRITE: types::GLenum = 0x8861;
#[allow(dead_code, non_upper_case_globals)]
pub const POINT_SPRITE_COORD_ORIGIN: types::GLenum = 0x8CA0;
#[allow(dead_code, non_upper_case_globals)]
pub const POINT_TOKEN: types::GLenum = 0x0701;
#[allow(dead_code, non_upper_case_globals)]
pub const POLYGON: types::GLenum = 0x0009;
#[allow(dead_code, non_upper_case_globals)]
pub const POLYGON_BIT: types::GLenum = 0x00000008;
#[allow(dead_code, non_upper_case_globals)]
pub const POLYGON_MODE: types::GLenum = 0x0B40;
#[allow(dead_code, non_upper_case_globals)]
pub const POLYGON_OFFSET_FACTOR: types::GLenum = 0x8038;
#[allow(dead_code, non_upper_case_globals)]
pub const POLYGON_OFFSET_FILL: types::GLenum = 0x8037;
#[allow(dead_code, non_upper_case_globals)]
pub const POLYGON_OFFSET_LINE: types::GLenum = 0x2A02;
#[allow(dead_code, non_upper_case_globals)]
pub const POLYGON_OFFSET_POINT: types::GLenum = 0x2A01;
#[allow(dead_code, non_upper_case_globals)]
pub const POLYGON_OFFSET_UNITS: types::GLenum = 0x2A00;
#[allow(dead_code, non_upper_case_globals)]
pub const POLYGON_SMOOTH: types::GLenum = 0x0B41;
#[allow(dead_code, non_upper_case_globals)]
pub const POLYGON_SMOOTH_HINT: types::GLenum = 0x0C53;
#[allow(dead_code, non_upper_case_globals)]
pub const POLYGON_STIPPLE: types::GLenum = 0x0B42;
#[allow(dead_code, non_upper_case_globals)]
pub const POLYGON_STIPPLE_BIT: types::GLenum = 0x00000010;
#[allow(dead_code, non_upper_case_globals)]
pub const POLYGON_TOKEN: types::GLenum = 0x0703;
#[allow(dead_code, non_upper_case_globals)]
pub const POSITION: types::GLenum = 0x1203;
#[allow(dead_code, non_upper_case_globals)]
pub const PREVIOUS: types::GLenum = 0x8578;
#[allow(dead_code, non_upper_case_globals)]
pub const PRIMARY_COLOR: types::GLenum = 0x8577;
#[allow(dead_code, non_upper_case_globals)]
pub const PRIMITIVES_GENERATED: types::GLenum = 0x8C87;
#[allow(dead_code, non_upper_case_globals)]
pub const PRIMITIVE_RESTART: types::GLenum = 0x8F9D;
#[allow(dead_code, non_upper_case_globals)]
pub const PRIMITIVE_RESTART_FIXED_INDEX: types::GLenum = 0x8D69;
#[allow(dead_code, non_upper_case_globals)]
pub const PRIMITIVE_RESTART_INDEX: types::GLenum = 0x8F9E;
#[allow(dead_code, non_upper_case_globals)]
pub const PROGRAM: types::GLenum = 0x82E2;
#[allow(dead_code, non_upper_case_globals)]
pub const PROGRAM_BINARY_FORMATS: types::GLenum = 0x87FF;
#[allow(dead_code, non_upper_case_globals)]
pub const PROGRAM_BINARY_LENGTH: types::GLenum = 0x8741;
#[allow(dead_code, non_upper_case_globals)]
pub const PROGRAM_BINARY_RETRIEVABLE_HINT: types::GLenum = 0x8257;
#[allow(dead_code, non_upper_case_globals)]
pub const PROGRAM_INPUT: types::GLenum = 0x92E3;
#[allow(dead_code, non_upper_case_globals)]
pub const PROGRAM_KHR: types::GLenum = 0x82E2;
#[allow(dead_code, non_upper_case_globals)]
pub const PROGRAM_OUTPUT: types::GLenum = 0x92E4;
#[allow(dead_code, non_upper_case_globals)]
pub const PROGRAM_PIPELINE: types::GLenum = 0x82E4;
#[allow(dead_code, non_upper_case_globals)]
pub const PROGRAM_PIPELINE_BINDING: types::GLenum = 0x825A;
#[allow(dead_code, non_upper_case_globals)]
pub const PROGRAM_PIPELINE_KHR: types::GLenum = 0x82E4;
#[allow(dead_code, non_upper_case_globals)]
pub const PROGRAM_POINT_SIZE: types::GLenum = 0x8642;
#[allow(dead_code, non_upper_case_globals)]
pub const PROGRAM_SEPARABLE: types::GLenum = 0x8258;
#[allow(dead_code, non_upper_case_globals)]
pub const PROJECTION: types::GLenum = 0x1701;
#[allow(dead_code, non_upper_case_globals)]
pub const PROJECTION_MATRIX: types::GLenum = 0x0BA7;
#[allow(dead_code, non_upper_case_globals)]
pub const PROJECTION_STACK_DEPTH: types::GLenum = 0x0BA4;
#[allow(dead_code, non_upper_case_globals)]
pub const PROVOKING_VERTEX: types::GLenum = 0x8E4F;
#[allow(dead_code, non_upper_case_globals)]
pub const PROXY_TEXTURE_1D: types::GLenum = 0x8063;
#[allow(dead_code, non_upper_case_globals)]
pub const PROXY_TEXTURE_1D_ARRAY: types::GLenum = 0x8C19;
#[allow(dead_code, non_upper_case_globals)]
pub const PROXY_TEXTURE_2D: types::GLenum = 0x8064;
#[allow(dead_code, non_upper_case_globals)]
pub const PROXY_TEXTURE_2D_ARRAY: types::GLenum = 0x8C1B;
#[allow(dead_code, non_upper_case_globals)]
pub const PROXY_TEXTURE_2D_MULTISAMPLE: types::GLenum = 0x9101;
#[allow(dead_code, non_upper_case_globals)]
pub const PROXY_TEXTURE_2D_MULTISAMPLE_ARRAY: types::GLenum = 0x9103;
#[allow(dead_code, non_upper_case_globals)]
pub const PROXY_TEXTURE_3D: types::GLenum = 0x8070;
#[allow(dead_code, non_upper_case_globals)]
pub const PROXY_TEXTURE_CUBE_MAP: types::GLenum = 0x851B;
#[allow(dead_code, non_upper_case_globals)]
pub const PROXY_TEXTURE_RECTANGLE: types::GLenum = 0x84F7;
#[allow(dead_code, non_upper_case_globals)]
pub const PROXY_TEXTURE_RECTANGLE_ARB: types::GLenum = 0x84F7;
#[allow(dead_code, non_upper_case_globals)]
pub const Q: types::GLenum = 0x2003;
#[allow(dead_code, non_upper_case_globals)]
pub const QUADRATIC_ATTENUATION: types::GLenum = 0x1209;
#[allow(dead_code, non_upper_case_globals)]
pub const QUADS: types::GLenum = 0x0007;
#[allow(dead_code, non_upper_case_globals)]
pub const QUADS_FOLLOW_PROVOKING_VERTEX_CONVENTION: types::GLenum = 0x8E4C;
#[allow(dead_code, non_upper_case_globals)]
pub const QUAD_STRIP: types::GLenum = 0x0008;
#[allow(dead_code, non_upper_case_globals)]
pub const QUERY: types::GLenum = 0x82E3;
#[allow(dead_code, non_upper_case_globals)]
pub const QUERY_BY_REGION_NO_WAIT: types::GLenum = 0x8E16;
#[allow(dead_code, non_upper_case_globals)]
pub const QUERY_BY_REGION_WAIT: types::GLenum = 0x8E15;
#[allow(dead_code, non_upper_case_globals)]
pub const QUERY_COUNTER_BITS: types::GLenum = 0x8864;
#[allow(dead_code, non_upper_case_globals)]
pub const QUERY_COUNTER_BITS_EXT: types::GLenum = 0x8864;
#[allow(dead_code, non_upper_case_globals)]
pub const QUERY_KHR: types::GLenum = 0x82E3;
#[allow(dead_code, non_upper_case_globals)]
pub const QUERY_NO_WAIT: types::GLenum = 0x8E14;
#[allow(dead_code, non_upper_case_globals)]
pub const QUERY_RESULT: types::GLenum = 0x8866;
#[allow(dead_code, non_upper_case_globals)]
pub const QUERY_RESULT_AVAILABLE: types::GLenum = 0x8867;
#[allow(dead_code, non_upper_case_globals)]
pub const QUERY_RESULT_AVAILABLE_EXT: types::GLenum = 0x8867;
#[allow(dead_code, non_upper_case_globals)]
pub const QUERY_RESULT_EXT: types::GLenum = 0x8866;
#[allow(dead_code, non_upper_case_globals)]
pub const QUERY_WAIT: types::GLenum = 0x8E13;
#[allow(dead_code, non_upper_case_globals)]
pub const R: types::GLenum = 0x2002;
#[allow(dead_code, non_upper_case_globals)]
pub const R11F_G11F_B10F: types::GLenum = 0x8C3A;
#[allow(dead_code, non_upper_case_globals)]
pub const R16: types::GLenum = 0x822A;
#[allow(dead_code, non_upper_case_globals)]
pub const R16F: types::GLenum = 0x822D;
#[allow(dead_code, non_upper_case_globals)]
pub const R16F_EXT: types::GLenum = 0x822D;
#[allow(dead_code, non_upper_case_globals)]
pub const R16I: types::GLenum = 0x8233;
#[allow(dead_code, non_upper_case_globals)]
pub const R16UI: types::GLenum = 0x8234;
#[allow(dead_code, non_upper_case_globals)]
pub const R16_SNORM: types::GLenum = 0x8F98;
#[allow(dead_code, non_upper_case_globals)]
pub const R32F: types::GLenum = 0x822E;
#[allow(dead_code, non_upper_case_globals)]
pub const R32F_EXT: types::GLenum = 0x822E;
#[allow(dead_code, non_upper_case_globals)]
pub const R32I: types::GLenum = 0x8235;
#[allow(dead_code, non_upper_case_globals)]
pub const R32UI: types::GLenum = 0x8236;
#[allow(dead_code, non_upper_case_globals)]
pub const R3_G3_B2: types::GLenum = 0x2A10;
#[allow(dead_code, non_upper_case_globals)]
pub const R8: types::GLenum = 0x8229;
#[allow(dead_code, non_upper_case_globals)]
pub const R8I: types::GLenum = 0x8231;
#[allow(dead_code, non_upper_case_globals)]
pub const R8UI: types::GLenum = 0x8232;
#[allow(dead_code, non_upper_case_globals)]
pub const R8_EXT: types::GLenum = 0x8229;
#[allow(dead_code, non_upper_case_globals)]
pub const R8_SNORM: types::GLenum = 0x8F94;
#[allow(dead_code, non_upper_case_globals)]
pub const RASTERIZER_DISCARD: types::GLenum = 0x8C89;
#[allow(dead_code, non_upper_case_globals)]
pub const READ_BUFFER: types::GLenum = 0x0C02;
#[allow(dead_code, non_upper_case_globals)]
pub const READ_FRAMEBUFFER: types::GLenum = 0x8CA8;
#[allow(dead_code, non_upper_case_globals)]
pub const READ_FRAMEBUFFER_BINDING: types::GLenum = 0x8CAA;
#[allow(dead_code, non_upper_case_globals)]
pub const READ_ONLY: types::GLenum = 0x88B8;
#[allow(dead_code, non_upper_case_globals)]
pub const READ_WRITE: types::GLenum = 0x88BA;
#[allow(dead_code, non_upper_case_globals)]
pub const RED: types::GLenum = 0x1903;
#[allow(dead_code, non_upper_case_globals)]
pub const RED_BIAS: types::GLenum = 0x0D15;
#[allow(dead_code, non_upper_case_globals)]
pub const RED_BITS: types::GLenum = 0x0D52;
#[allow(dead_code, non_upper_case_globals)]
pub const RED_INTEGER: types::GLenum = 0x8D94;
#[allow(dead_code, non_upper_case_globals)]
pub const RED_SCALE: types::GLenum = 0x0D14;
#[allow(dead_code, non_upper_case_globals)]
pub const REFERENCED_BY_COMPUTE_SHADER: types::GLenum = 0x930B;
#[allow(dead_code, non_upper_case_globals)]
pub const REFERENCED_BY_FRAGMENT_SHADER: types::GLenum = 0x930A;
#[allow(dead_code, non_upper_case_globals)]
pub const REFERENCED_BY_VERTEX_SHADER: types::GLenum = 0x9306;
#[allow(dead_code, non_upper_case_globals)]
pub const REFLECTION_MAP: types::GLenum = 0x8512;
#[allow(dead_code, non_upper_case_globals)]
pub const RENDER: types::GLenum = 0x1C00;
#[allow(dead_code, non_upper_case_globals)]
pub const RENDERBUFFER: types::GLenum = 0x8D41;
#[allow(dead_code, non_upper_case_globals)]
pub const RENDERBUFFER_ALPHA_SIZE: types::GLenum = 0x8D53;
#[allow(dead_code, non_upper_case_globals)]
pub const RENDERBUFFER_BINDING: types::GLenum = 0x8CA7;
#[allow(dead_code, non_upper_case_globals)]
pub const RENDERBUFFER_BLUE_SIZE: types::GLenum = 0x8D52;
#[allow(dead_code, non_upper_case_globals)]
pub const RENDERBUFFER_DEPTH_SIZE: types::GLenum = 0x8D54;
#[allow(dead_code, non_upper_case_globals)]
pub const RENDERBUFFER_GREEN_SIZE: types::GLenum = 0x8D51;
#[allow(dead_code, non_upper_case_globals)]
pub const RENDERBUFFER_HEIGHT: types::GLenum = 0x8D43;
#[allow(dead_code, non_upper_case_globals)]
pub const RENDERBUFFER_INTERNAL_FORMAT: types::GLenum = 0x8D44;
#[allow(dead_code, non_upper_case_globals)]
pub const RENDERBUFFER_RED_SIZE: types::GLenum = 0x8D50;
#[allow(dead_code, non_upper_case_globals)]
pub const RENDERBUFFER_SAMPLES: types::GLenum = 0x8CAB;
#[allow(dead_code, non_upper_case_globals)]
pub const RENDERBUFFER_STENCIL_SIZE: types::GLenum = 0x8D55;
#[allow(dead_code, non_upper_case_globals)]
pub const RENDERBUFFER_WIDTH: types::GLenum = 0x8D42;
#[allow(dead_code, non_upper_case_globals)]
pub const RENDERER: types::GLenum = 0x1F01;
#[allow(dead_code, non_upper_case_globals)]
pub const RENDER_MODE: types::GLenum = 0x0C40;
#[allow(dead_code, non_upper_case_globals)]
pub const REPEAT: types::GLenum = 0x2901;
#[allow(dead_code, non_upper_case_globals)]
pub const REPLACE: types::GLenum = 0x1E01;
#[allow(dead_code, non_upper_case_globals)]
pub const REQUIRED_TEXTURE_IMAGE_UNITS_OES: types::GLenum = 0x8D68;
#[allow(dead_code, non_upper_case_globals)]
pub const RESCALE_NORMAL: types::GLenum = 0x803A;
#[allow(dead_code, non_upper_case_globals)]
pub const RETURN: types::GLenum = 0x0102;
#[allow(dead_code, non_upper_case_globals)]
pub const RG: types::GLenum = 0x8227;
#[allow(dead_code, non_upper_case_globals)]
pub const RG16: types::GLenum = 0x822C;
#[allow(dead_code, non_upper_case_globals)]
pub const RG16F: types::GLenum = 0x822F;
#[allow(dead_code, non_upper_case_globals)]
pub const RG16F_EXT: types::GLenum = 0x822F;
#[allow(dead_code, non_upper_case_globals)]
pub const RG16I: types::GLenum = 0x8239;
#[allow(dead_code, non_upper_case_globals)]
pub const RG16UI: types::GLenum = 0x823A;
#[allow(dead_code, non_upper_case_globals)]
pub const RG16_SNORM: types::GLenum = 0x8F99;
#[allow(dead_code, non_upper_case_globals)]
pub const RG32F: types::GLenum = 0x8230;
#[allow(dead_code, non_upper_case_globals)]
pub const RG32F_EXT: types::GLenum = 0x8230;
#[allow(dead_code, non_upper_case_globals)]
pub const RG32I: types::GLenum = 0x823B;
#[allow(dead_code, non_upper_case_globals)]
pub const RG32UI: types::GLenum = 0x823C;
#[allow(dead_code, non_upper_case_globals)]
pub const RG8: types::GLenum = 0x822B;
#[allow(dead_code, non_upper_case_globals)]
pub const RG8I: types::GLenum = 0x8237;
#[allow(dead_code, non_upper_case_globals)]
pub const RG8UI: types::GLenum = 0x8238;
#[allow(dead_code, non_upper_case_globals)]
pub const RG8_EXT: types::GLenum = 0x822B;
#[allow(dead_code, non_upper_case_globals)]
pub const RG8_SNORM: types::GLenum = 0x8F95;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB: types::GLenum = 0x1907;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB10: types::GLenum = 0x8052;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB10_A2: types::GLenum = 0x8059;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB10_A2UI: types::GLenum = 0x906F;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB10_A2_EXT: types::GLenum = 0x8059;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB10_EXT: types::GLenum = 0x8052;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB12: types::GLenum = 0x8053;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB16: types::GLenum = 0x8054;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB16F: types::GLenum = 0x881B;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB16F_EXT: types::GLenum = 0x881B;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB16I: types::GLenum = 0x8D89;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB16UI: types::GLenum = 0x8D77;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB16_SNORM: types::GLenum = 0x8F9A;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB32F: types::GLenum = 0x8815;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB32F_EXT: types::GLenum = 0x8815;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB32I: types::GLenum = 0x8D83;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB32UI: types::GLenum = 0x8D71;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB4: types::GLenum = 0x804F;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB5: types::GLenum = 0x8050;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB565: types::GLenum = 0x8D62;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB5_A1: types::GLenum = 0x8057;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB8: types::GLenum = 0x8051;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB8I: types::GLenum = 0x8D8F;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB8UI: types::GLenum = 0x8D7D;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB8_SNORM: types::GLenum = 0x8F96;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB9_E5: types::GLenum = 0x8C3D;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA: types::GLenum = 0x1908;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA12: types::GLenum = 0x805A;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA16: types::GLenum = 0x805B;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA16F: types::GLenum = 0x881A;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA16F_EXT: types::GLenum = 0x881A;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA16I: types::GLenum = 0x8D88;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA16UI: types::GLenum = 0x8D76;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA16_SNORM: types::GLenum = 0x8F9B;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA2: types::GLenum = 0x8055;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA32F: types::GLenum = 0x8814;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA32F_EXT: types::GLenum = 0x8814;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA32I: types::GLenum = 0x8D82;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA32UI: types::GLenum = 0x8D70;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA4: types::GLenum = 0x8056;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA8: types::GLenum = 0x8058;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA8I: types::GLenum = 0x8D8E;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA8UI: types::GLenum = 0x8D7C;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA8_SNORM: types::GLenum = 0x8F97;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA_INTEGER: types::GLenum = 0x8D99;
#[allow(dead_code, non_upper_case_globals)]
pub const RGBA_MODE: types::GLenum = 0x0C31;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB_INTEGER: types::GLenum = 0x8D98;
#[allow(dead_code, non_upper_case_globals)]
pub const RGB_SCALE: types::GLenum = 0x8573;
#[allow(dead_code, non_upper_case_globals)]
pub const RG_INTEGER: types::GLenum = 0x8228;
#[allow(dead_code, non_upper_case_globals)]
pub const RIGHT: types::GLenum = 0x0407;
#[allow(dead_code, non_upper_case_globals)]
pub const S: types::GLenum = 0x2000;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER: types::GLenum = 0x82E6;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_1D: types::GLenum = 0x8B5D;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_1D_ARRAY: types::GLenum = 0x8DC0;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_1D_ARRAY_SHADOW: types::GLenum = 0x8DC3;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_1D_SHADOW: types::GLenum = 0x8B61;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_2D: types::GLenum = 0x8B5E;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_2D_ARRAY: types::GLenum = 0x8DC1;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_2D_ARRAY_SHADOW: types::GLenum = 0x8DC4;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_2D_MULTISAMPLE: types::GLenum = 0x9108;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_2D_MULTISAMPLE_ARRAY: types::GLenum = 0x910B;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_2D_RECT: types::GLenum = 0x8B63;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_2D_RECT_SHADOW: types::GLenum = 0x8B64;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_2D_SHADOW: types::GLenum = 0x8B62;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_3D: types::GLenum = 0x8B5F;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_BINDING: types::GLenum = 0x8919;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_BUFFER: types::GLenum = 0x8DC2;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_CUBE: types::GLenum = 0x8B60;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_CUBE_SHADOW: types::GLenum = 0x8DC5;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_EXTERNAL_OES: types::GLenum = 0x8D66;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLER_KHR: types::GLenum = 0x82E6;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLES: types::GLenum = 0x80A9;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLES_PASSED: types::GLenum = 0x8914;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLE_ALPHA_TO_COVERAGE: types::GLenum = 0x809E;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLE_ALPHA_TO_ONE: types::GLenum = 0x809F;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLE_BUFFERS: types::GLenum = 0x80A8;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLE_COVERAGE: types::GLenum = 0x80A0;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLE_COVERAGE_INVERT: types::GLenum = 0x80AB;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLE_COVERAGE_VALUE: types::GLenum = 0x80AA;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLE_MASK: types::GLenum = 0x8E51;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLE_MASK_VALUE: types::GLenum = 0x8E52;
#[allow(dead_code, non_upper_case_globals)]
pub const SAMPLE_POSITION: types::GLenum = 0x8E50;
#[allow(dead_code, non_upper_case_globals)]
pub const SCISSOR_BIT: types::GLenum = 0x00080000;
#[allow(dead_code, non_upper_case_globals)]
pub const SCISSOR_BOX: types::GLenum = 0x0C10;
#[allow(dead_code, non_upper_case_globals)]
pub const SCISSOR_TEST: types::GLenum = 0x0C11;
#[allow(dead_code, non_upper_case_globals)]
pub const SCREEN_KHR: types::GLenum = 0x9295;
#[allow(dead_code, non_upper_case_globals)]
pub const SECONDARY_COLOR_ARRAY: types::GLenum = 0x845E;
#[allow(dead_code, non_upper_case_globals)]
pub const SECONDARY_COLOR_ARRAY_BUFFER_BINDING: types::GLenum = 0x889C;
#[allow(dead_code, non_upper_case_globals)]
pub const SECONDARY_COLOR_ARRAY_POINTER: types::GLenum = 0x845D;
#[allow(dead_code, non_upper_case_globals)]
pub const SECONDARY_COLOR_ARRAY_SIZE: types::GLenum = 0x845A;
#[allow(dead_code, non_upper_case_globals)]
pub const SECONDARY_COLOR_ARRAY_STRIDE: types::GLenum = 0x845C;
#[allow(dead_code, non_upper_case_globals)]
pub const SECONDARY_COLOR_ARRAY_TYPE: types::GLenum = 0x845B;
#[allow(dead_code, non_upper_case_globals)]
pub const SELECT: types::GLenum = 0x1C02;
#[allow(dead_code, non_upper_case_globals)]
pub const SELECTION_BUFFER_POINTER: types::GLenum = 0x0DF3;
#[allow(dead_code, non_upper_case_globals)]
pub const SELECTION_BUFFER_SIZE: types::GLenum = 0x0DF4;
#[allow(dead_code, non_upper_case_globals)]
pub const SEPARATE_ATTRIBS: types::GLenum = 0x8C8D;
#[allow(dead_code, non_upper_case_globals)]
pub const SEPARATE_SPECULAR_COLOR: types::GLenum = 0x81FA;
#[allow(dead_code, non_upper_case_globals)]
pub const SET: types::GLenum = 0x150F;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADER: types::GLenum = 0x82E1;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADER_BINARY_FORMATS: types::GLenum = 0x8DF8;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADER_COMPILER: types::GLenum = 0x8DFA;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADER_IMAGE_ACCESS_BARRIER_BIT: types::GLenum = 0x00000020;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADER_KHR: types::GLenum = 0x82E1;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADER_PIXEL_LOCAL_STORAGE_EXT: types::GLenum = 0x8F64;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADER_SOURCE_LENGTH: types::GLenum = 0x8B88;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADER_STORAGE_BARRIER_BIT: types::GLenum = 0x00002000;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADER_STORAGE_BLOCK: types::GLenum = 0x92E6;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADER_STORAGE_BUFFER: types::GLenum = 0x90D2;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADER_STORAGE_BUFFER_BINDING: types::GLenum = 0x90D3;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADER_STORAGE_BUFFER_OFFSET_ALIGNMENT: types::GLenum = 0x90DF;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADER_STORAGE_BUFFER_SIZE: types::GLenum = 0x90D5;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADER_STORAGE_BUFFER_START: types::GLenum = 0x90D4;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADER_TYPE: types::GLenum = 0x8B4F;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADE_MODEL: types::GLenum = 0x0B54;
#[allow(dead_code, non_upper_case_globals)]
pub const SHADING_LANGUAGE_VERSION: types::GLenum = 0x8B8C;
#[allow(dead_code, non_upper_case_globals)]
pub const SHININESS: types::GLenum = 0x1601;
#[allow(dead_code, non_upper_case_globals)]
pub const SHORT: types::GLenum = 0x1402;
#[allow(dead_code, non_upper_case_globals)]
pub const SIGNALED: types::GLenum = 0x9119;
#[allow(dead_code, non_upper_case_globals)]
pub const SIGNED_NORMALIZED: types::GLenum = 0x8F9C;
#[allow(dead_code, non_upper_case_globals)]
pub const SINGLE_COLOR: types::GLenum = 0x81F9;
#[allow(dead_code, non_upper_case_globals)]
pub const SLUMINANCE: types::GLenum = 0x8C46;
#[allow(dead_code, non_upper_case_globals)]
pub const SLUMINANCE8: types::GLenum = 0x8C47;
#[allow(dead_code, non_upper_case_globals)]
pub const SLUMINANCE8_ALPHA8: types::GLenum = 0x8C45;
#[allow(dead_code, non_upper_case_globals)]
pub const SLUMINANCE_ALPHA: types::GLenum = 0x8C44;
#[allow(dead_code, non_upper_case_globals)]
pub const SMOOTH: types::GLenum = 0x1D01;
#[allow(dead_code, non_upper_case_globals)]
pub const SMOOTH_LINE_WIDTH_GRANULARITY: types::GLenum = 0x0B23;
#[allow(dead_code, non_upper_case_globals)]
pub const SMOOTH_LINE_WIDTH_RANGE: types::GLenum = 0x0B22;
#[allow(dead_code, non_upper_case_globals)]
pub const SMOOTH_POINT_SIZE_GRANULARITY: types::GLenum = 0x0B13;
#[allow(dead_code, non_upper_case_globals)]
pub const SMOOTH_POINT_SIZE_RANGE: types::GLenum = 0x0B12;
#[allow(dead_code, non_upper_case_globals)]
pub const SOFTLIGHT_KHR: types::GLenum = 0x929C;
#[allow(dead_code, non_upper_case_globals)]
pub const SOURCE0_ALPHA: types::GLenum = 0x8588;
#[allow(dead_code, non_upper_case_globals)]
pub const SOURCE0_RGB: types::GLenum = 0x8580;
#[allow(dead_code, non_upper_case_globals)]
pub const SOURCE1_ALPHA: types::GLenum = 0x8589;
#[allow(dead_code, non_upper_case_globals)]
pub const SOURCE1_RGB: types::GLenum = 0x8581;
#[allow(dead_code, non_upper_case_globals)]
pub const SOURCE2_ALPHA: types::GLenum = 0x858A;
#[allow(dead_code, non_upper_case_globals)]
pub const SOURCE2_RGB: types::GLenum = 0x8582;
#[allow(dead_code, non_upper_case_globals)]
pub const SPECULAR: types::GLenum = 0x1202;
#[allow(dead_code, non_upper_case_globals)]
pub const SPHERE_MAP: types::GLenum = 0x2402;
#[allow(dead_code, non_upper_case_globals)]
pub const SPOT_CUTOFF: types::GLenum = 0x1206;
#[allow(dead_code, non_upper_case_globals)]
pub const SPOT_DIRECTION: types::GLenum = 0x1204;
#[allow(dead_code, non_upper_case_globals)]
pub const SPOT_EXPONENT: types::GLenum = 0x1205;
#[allow(dead_code, non_upper_case_globals)]
pub const SRC0_ALPHA: types::GLenum = 0x8588;
#[allow(dead_code, non_upper_case_globals)]
pub const SRC0_RGB: types::GLenum = 0x8580;
#[allow(dead_code, non_upper_case_globals)]
pub const SRC1_ALPHA: types::GLenum = 0x8589;
#[allow(dead_code, non_upper_case_globals)]
pub const SRC1_COLOR: types::GLenum = 0x88F9;
#[allow(dead_code, non_upper_case_globals)]
pub const SRC1_RGB: types::GLenum = 0x8581;
#[allow(dead_code, non_upper_case_globals)]
pub const SRC2_ALPHA: types::GLenum = 0x858A;
#[allow(dead_code, non_upper_case_globals)]
pub const SRC2_RGB: types::GLenum = 0x8582;
#[allow(dead_code, non_upper_case_globals)]
pub const SRC_ALPHA: types::GLenum = 0x0302;
#[allow(dead_code, non_upper_case_globals)]
pub const SRC_ALPHA_SATURATE: types::GLenum = 0x0308;
#[allow(dead_code, non_upper_case_globals)]
pub const SRC_COLOR: types::GLenum = 0x0300;
#[allow(dead_code, non_upper_case_globals)]
pub const SRGB: types::GLenum = 0x8C40;
#[allow(dead_code, non_upper_case_globals)]
pub const SRGB8: types::GLenum = 0x8C41;
#[allow(dead_code, non_upper_case_globals)]
pub const SRGB8_ALPHA8: types::GLenum = 0x8C43;
#[allow(dead_code, non_upper_case_globals)]
pub const SRGB_ALPHA: types::GLenum = 0x8C42;
#[allow(dead_code, non_upper_case_globals)]
pub const STACK_OVERFLOW: types::GLenum = 0x0503;
#[allow(dead_code, non_upper_case_globals)]
pub const STACK_OVERFLOW_KHR: types::GLenum = 0x0503;
#[allow(dead_code, non_upper_case_globals)]
pub const STACK_UNDERFLOW: types::GLenum = 0x0504;
#[allow(dead_code, non_upper_case_globals)]
pub const STACK_UNDERFLOW_KHR: types::GLenum = 0x0504;
#[allow(dead_code, non_upper_case_globals)]
pub const STATIC_COPY: types::GLenum = 0x88E6;
#[allow(dead_code, non_upper_case_globals)]
pub const STATIC_DRAW: types::GLenum = 0x88E4;
#[allow(dead_code, non_upper_case_globals)]
pub const STATIC_READ: types::GLenum = 0x88E5;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL: types::GLenum = 0x1802;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_ATTACHMENT: types::GLenum = 0x8D20;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BACK_FAIL: types::GLenum = 0x8801;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BACK_FUNC: types::GLenum = 0x8800;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BACK_PASS_DEPTH_FAIL: types::GLenum = 0x8802;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BACK_PASS_DEPTH_PASS: types::GLenum = 0x8803;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BACK_REF: types::GLenum = 0x8CA3;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BACK_VALUE_MASK: types::GLenum = 0x8CA4;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BACK_WRITEMASK: types::GLenum = 0x8CA5;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BITS: types::GLenum = 0x0D57;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BUFFER_BIT: types::GLenum = 0x00000400;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BUFFER_BIT0_QCOM: types::GLenum = 0x00010000;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BUFFER_BIT1_QCOM: types::GLenum = 0x00020000;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BUFFER_BIT2_QCOM: types::GLenum = 0x00040000;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BUFFER_BIT3_QCOM: types::GLenum = 0x00080000;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BUFFER_BIT4_QCOM: types::GLenum = 0x00100000;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BUFFER_BIT5_QCOM: types::GLenum = 0x00200000;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BUFFER_BIT6_QCOM: types::GLenum = 0x00400000;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_BUFFER_BIT7_QCOM: types::GLenum = 0x00800000;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_CLEAR_VALUE: types::GLenum = 0x0B91;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_FAIL: types::GLenum = 0x0B94;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_FUNC: types::GLenum = 0x0B92;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_INDEX: types::GLenum = 0x1901;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_INDEX1: types::GLenum = 0x8D46;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_INDEX16: types::GLenum = 0x8D49;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_INDEX4: types::GLenum = 0x8D47;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_INDEX8: types::GLenum = 0x8D48;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_PASS_DEPTH_FAIL: types::GLenum = 0x0B95;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_PASS_DEPTH_PASS: types::GLenum = 0x0B96;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_REF: types::GLenum = 0x0B97;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_TEST: types::GLenum = 0x0B90;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_VALUE_MASK: types::GLenum = 0x0B93;
#[allow(dead_code, non_upper_case_globals)]
pub const STENCIL_WRITEMASK: types::GLenum = 0x0B98;
#[allow(dead_code, non_upper_case_globals)]
pub const STEREO: types::GLenum = 0x0C33;
#[allow(dead_code, non_upper_case_globals)]
pub const STORAGE_CACHED_APPLE: types::GLenum = 0x85BE;
#[allow(dead_code, non_upper_case_globals)]
pub const STORAGE_PRIVATE_APPLE: types::GLenum = 0x85BD;
#[allow(dead_code, non_upper_case_globals)]
pub const STORAGE_SHARED_APPLE: types::GLenum = 0x85BF;
#[allow(dead_code, non_upper_case_globals)]
pub const STREAM_COPY: types::GLenum = 0x88E2;
#[allow(dead_code, non_upper_case_globals)]
pub const STREAM_DRAW: types::GLenum = 0x88E0;
#[allow(dead_code, non_upper_case_globals)]
pub const STREAM_READ: types::GLenum = 0x88E1;
#[allow(dead_code, non_upper_case_globals)]
pub const SUBPIXEL_BITS: types::GLenum = 0x0D50;
#[allow(dead_code, non_upper_case_globals)]
pub const SUBTRACT: types::GLenum = 0x84E7;
#[allow(dead_code, non_upper_case_globals)]
pub const SYNC_CONDITION: types::GLenum = 0x9113;
#[allow(dead_code, non_upper_case_globals)]
pub const SYNC_FENCE: types::GLenum = 0x9116;
#[allow(dead_code, non_upper_case_globals)]
pub const SYNC_FLAGS: types::GLenum = 0x9115;
#[allow(dead_code, non_upper_case_globals)]
pub const SYNC_FLUSH_COMMANDS_BIT: types::GLenum = 0x00000001;
#[allow(dead_code, non_upper_case_globals)]
pub const SYNC_GPU_COMMANDS_COMPLETE: types::GLenum = 0x9117;
#[allow(dead_code, non_upper_case_globals)]
pub const SYNC_STATUS: types::GLenum = 0x9114;
#[allow(dead_code, non_upper_case_globals)]
pub const T: types::GLenum = 0x2001;
#[allow(dead_code, non_upper_case_globals)]
pub const T2F_C3F_V3F: types::GLenum = 0x2A2A;
#[allow(dead_code, non_upper_case_globals)]
pub const T2F_C4F_N3F_V3F: types::GLenum = 0x2A2C;
#[allow(dead_code, non_upper_case_globals)]
pub const T2F_C4UB_V3F: types::GLenum = 0x2A29;
#[allow(dead_code, non_upper_case_globals)]
pub const T2F_N3F_V3F: types::GLenum = 0x2A2B;
#[allow(dead_code, non_upper_case_globals)]
pub const T2F_V3F: types::GLenum = 0x2A27;
#[allow(dead_code, non_upper_case_globals)]
pub const T4F_C4F_N3F_V4F: types::GLenum = 0x2A2D;
#[allow(dead_code, non_upper_case_globals)]
pub const T4F_V4F: types::GLenum = 0x2A28;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE: types::GLenum = 0x1702;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE0: types::GLenum = 0x84C0;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE1: types::GLenum = 0x84C1;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE10: types::GLenum = 0x84CA;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE11: types::GLenum = 0x84CB;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE12: types::GLenum = 0x84CC;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE13: types::GLenum = 0x84CD;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE14: types::GLenum = 0x84CE;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE15: types::GLenum = 0x84CF;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE16: types::GLenum = 0x84D0;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE17: types::GLenum = 0x84D1;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE18: types::GLenum = 0x84D2;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE19: types::GLenum = 0x84D3;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE2: types::GLenum = 0x84C2;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE20: types::GLenum = 0x84D4;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE21: types::GLenum = 0x84D5;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE22: types::GLenum = 0x84D6;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE23: types::GLenum = 0x84D7;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE24: types::GLenum = 0x84D8;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE25: types::GLenum = 0x84D9;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE26: types::GLenum = 0x84DA;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE27: types::GLenum = 0x84DB;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE28: types::GLenum = 0x84DC;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE29: types::GLenum = 0x84DD;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE3: types::GLenum = 0x84C3;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE30: types::GLenum = 0x84DE;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE31: types::GLenum = 0x84DF;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE4: types::GLenum = 0x84C4;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE5: types::GLenum = 0x84C5;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE6: types::GLenum = 0x84C6;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE7: types::GLenum = 0x84C7;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE8: types::GLenum = 0x84C8;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE9: types::GLenum = 0x84C9;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_1D: types::GLenum = 0x0DE0;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_1D_ARRAY: types::GLenum = 0x8C18;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_2D: types::GLenum = 0x0DE1;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_2D_ARRAY: types::GLenum = 0x8C1A;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_2D_MULTISAMPLE: types::GLenum = 0x9100;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_2D_MULTISAMPLE_ARRAY: types::GLenum = 0x9102;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_3D: types::GLenum = 0x806F;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_ALPHA_SIZE: types::GLenum = 0x805F;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_ALPHA_TYPE: types::GLenum = 0x8C13;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BASE_LEVEL: types::GLenum = 0x813C;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BINDING_1D: types::GLenum = 0x8068;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BINDING_1D_ARRAY: types::GLenum = 0x8C1C;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BINDING_2D: types::GLenum = 0x8069;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BINDING_2D_ARRAY: types::GLenum = 0x8C1D;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BINDING_2D_MULTISAMPLE: types::GLenum = 0x9104;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BINDING_2D_MULTISAMPLE_ARRAY: types::GLenum = 0x9105;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BINDING_3D: types::GLenum = 0x806A;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BINDING_BUFFER: types::GLenum = 0x8C2C;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BINDING_CUBE_MAP: types::GLenum = 0x8514;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BINDING_EXTERNAL_OES: types::GLenum = 0x8D67;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BINDING_RECTANGLE: types::GLenum = 0x84F6;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BINDING_RECTANGLE_ARB: types::GLenum = 0x84F6;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BIT: types::GLenum = 0x00040000;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BLUE_SIZE: types::GLenum = 0x805E;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BLUE_TYPE: types::GLenum = 0x8C12;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BORDER: types::GLenum = 0x1005;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BORDER_COLOR: types::GLenum = 0x1004;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BUFFER: types::GLenum = 0x8C2A;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_BUFFER_DATA_STORE_BINDING: types::GLenum = 0x8C2D;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_COMPARE_FUNC: types::GLenum = 0x884D;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_COMPARE_MODE: types::GLenum = 0x884C;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_COMPONENTS: types::GLenum = 0x1003;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_COMPRESSED: types::GLenum = 0x86A1;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_COMPRESSED_IMAGE_SIZE: types::GLenum = 0x86A0;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_COMPRESSION_HINT: types::GLenum = 0x84EF;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_COORD_ARRAY: types::GLenum = 0x8078;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_COORD_ARRAY_BUFFER_BINDING: types::GLenum = 0x889A;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_COORD_ARRAY_POINTER: types::GLenum = 0x8092;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_COORD_ARRAY_SIZE: types::GLenum = 0x8088;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_COORD_ARRAY_STRIDE: types::GLenum = 0x808A;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_COORD_ARRAY_TYPE: types::GLenum = 0x8089;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_CUBE_MAP: types::GLenum = 0x8513;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_CUBE_MAP_NEGATIVE_X: types::GLenum = 0x8516;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_CUBE_MAP_NEGATIVE_Y: types::GLenum = 0x8518;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_CUBE_MAP_NEGATIVE_Z: types::GLenum = 0x851A;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_CUBE_MAP_POSITIVE_X: types::GLenum = 0x8515;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_CUBE_MAP_POSITIVE_Y: types::GLenum = 0x8517;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_CUBE_MAP_POSITIVE_Z: types::GLenum = 0x8519;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_CUBE_MAP_SEAMLESS: types::GLenum = 0x884F;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_DEPTH: types::GLenum = 0x8071;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_DEPTH_SIZE: types::GLenum = 0x884A;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_DEPTH_TYPE: types::GLenum = 0x8C16;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_ENV: types::GLenum = 0x2300;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_ENV_COLOR: types::GLenum = 0x2201;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_ENV_MODE: types::GLenum = 0x2200;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_EXTERNAL_OES: types::GLenum = 0x8D65;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_FETCH_BARRIER_BIT: types::GLenum = 0x00000008;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_FILTER_CONTROL: types::GLenum = 0x8500;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_FIXED_SAMPLE_LOCATIONS: types::GLenum = 0x9107;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_GEN_MODE: types::GLenum = 0x2500;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_GEN_Q: types::GLenum = 0x0C63;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_GEN_R: types::GLenum = 0x0C62;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_GEN_S: types::GLenum = 0x0C60;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_GEN_T: types::GLenum = 0x0C61;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_GREEN_SIZE: types::GLenum = 0x805D;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_GREEN_TYPE: types::GLenum = 0x8C11;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_HEIGHT: types::GLenum = 0x1001;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_IMMUTABLE_FORMAT: types::GLenum = 0x912F;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_IMMUTABLE_FORMAT_EXT: types::GLenum = 0x912F;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_IMMUTABLE_LEVELS: types::GLenum = 0x82DF;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_INTENSITY_SIZE: types::GLenum = 0x8061;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_INTENSITY_TYPE: types::GLenum = 0x8C15;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_INTERNAL_FORMAT: types::GLenum = 0x1003;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_LOD_BIAS: types::GLenum = 0x8501;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_LUMINANCE_SIZE: types::GLenum = 0x8060;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_LUMINANCE_TYPE: types::GLenum = 0x8C14;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_MAG_FILTER: types::GLenum = 0x2800;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_MATRIX: types::GLenum = 0x0BA8;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_MAX_ANISOTROPY_EXT: types::GLenum = 0x84FE;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_MAX_LEVEL: types::GLenum = 0x813D;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_MAX_LOD: types::GLenum = 0x813B;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_MIN_FILTER: types::GLenum = 0x2801;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_MIN_LOD: types::GLenum = 0x813A;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_PRIORITY: types::GLenum = 0x8066;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_RANGE_LENGTH_APPLE: types::GLenum = 0x85B7;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_RANGE_POINTER_APPLE: types::GLenum = 0x85B8;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_RECTANGLE: types::GLenum = 0x84F5;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_RECTANGLE_ARB: types::GLenum = 0x84F5;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_RED_SIZE: types::GLenum = 0x805C;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_RED_TYPE: types::GLenum = 0x8C10;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_RESIDENT: types::GLenum = 0x8067;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_SAMPLES: types::GLenum = 0x9106;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_SHARED_SIZE: types::GLenum = 0x8C3F;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_STACK_DEPTH: types::GLenum = 0x0BA5;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_STENCIL_SIZE: types::GLenum = 0x88F1;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_STORAGE_HINT_APPLE: types::GLenum = 0x85BC;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_SWIZZLE_A: types::GLenum = 0x8E45;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_SWIZZLE_B: types::GLenum = 0x8E44;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_SWIZZLE_G: types::GLenum = 0x8E43;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_SWIZZLE_R: types::GLenum = 0x8E42;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_SWIZZLE_RGBA: types::GLenum = 0x8E46;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_UPDATE_BARRIER_BIT: types::GLenum = 0x00000100;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_USAGE_ANGLE: types::GLenum = 0x93A2;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_WIDTH: types::GLenum = 0x1000;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_WRAP_R: types::GLenum = 0x8072;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_WRAP_S: types::GLenum = 0x2802;
#[allow(dead_code, non_upper_case_globals)]
pub const TEXTURE_WRAP_T: types::GLenum = 0x2803;
#[allow(dead_code, non_upper_case_globals)]
pub const TIMEOUT_EXPIRED: types::GLenum = 0x911B;
#[allow(dead_code, non_upper_case_globals)]
pub const TIMEOUT_IGNORED: types::GLuint64 = 0xFFFFFFFFFFFFFFFF;
#[allow(dead_code, non_upper_case_globals)]
pub const TIMESTAMP: types::GLenum = 0x8E28;
#[allow(dead_code, non_upper_case_globals)]
pub const TIMESTAMP_EXT: types::GLenum = 0x8E28;
#[allow(dead_code, non_upper_case_globals)]
pub const TIME_ELAPSED: types::GLenum = 0x88BF;
#[allow(dead_code, non_upper_case_globals)]
pub const TIME_ELAPSED_EXT: types::GLenum = 0x88BF;
#[allow(dead_code, non_upper_case_globals)]
pub const TOP_LEVEL_ARRAY_SIZE: types::GLenum = 0x930C;
#[allow(dead_code, non_upper_case_globals)]
pub const TOP_LEVEL_ARRAY_STRIDE: types::GLenum = 0x930D;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSFORM_BIT: types::GLenum = 0x00001000;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSFORM_FEEDBACK: types::GLenum = 0x8E22;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSFORM_FEEDBACK_ACTIVE: types::GLenum = 0x8E24;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSFORM_FEEDBACK_BARRIER_BIT: types::GLenum = 0x00000800;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSFORM_FEEDBACK_BINDING: types::GLenum = 0x8E25;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSFORM_FEEDBACK_BUFFER: types::GLenum = 0x8C8E;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSFORM_FEEDBACK_BUFFER_BINDING: types::GLenum = 0x8C8F;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSFORM_FEEDBACK_BUFFER_MODE: types::GLenum = 0x8C7F;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSFORM_FEEDBACK_BUFFER_SIZE: types::GLenum = 0x8C85;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSFORM_FEEDBACK_BUFFER_START: types::GLenum = 0x8C84;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSFORM_FEEDBACK_PAUSED: types::GLenum = 0x8E23;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSFORM_FEEDBACK_PRIMITIVES_WRITTEN: types::GLenum = 0x8C88;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSFORM_FEEDBACK_VARYING: types::GLenum = 0x92F4;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSFORM_FEEDBACK_VARYINGS: types::GLenum = 0x8C83;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSFORM_FEEDBACK_VARYING_MAX_LENGTH: types::GLenum = 0x8C76;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSPOSE_COLOR_MATRIX: types::GLenum = 0x84E6;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSPOSE_MODELVIEW_MATRIX: types::GLenum = 0x84E3;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSPOSE_PROJECTION_MATRIX: types::GLenum = 0x84E4;
#[allow(dead_code, non_upper_case_globals)]
pub const TRANSPOSE_TEXTURE_MATRIX: types::GLenum = 0x84E5;
#[allow(dead_code, non_upper_case_globals)]
pub const TRIANGLES: types::GLenum = 0x0004;
#[allow(dead_code, non_upper_case_globals)]
pub const TRIANGLES_ADJACENCY: types::GLenum = 0x000C;
#[allow(dead_code, non_upper_case_globals)]
pub const TRIANGLE_FAN: types::GLenum = 0x0006;
#[allow(dead_code, non_upper_case_globals)]
pub const TRIANGLE_STRIP: types::GLenum = 0x0005;
#[allow(dead_code, non_upper_case_globals)]
pub const TRIANGLE_STRIP_ADJACENCY: types::GLenum = 0x000D;
#[allow(dead_code, non_upper_case_globals)]
pub const TRUE: types::GLboolean = 1;
#[allow(dead_code, non_upper_case_globals)]
pub const TYPE: types::GLenum = 0x92FA;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM: types::GLenum = 0x92E1;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_ARRAY_STRIDE: types::GLenum = 0x8A3C;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BARRIER_BIT: types::GLenum = 0x00000004;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BLOCK: types::GLenum = 0x92E2;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BLOCK_ACTIVE_UNIFORMS: types::GLenum = 0x8A42;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BLOCK_ACTIVE_UNIFORM_INDICES: types::GLenum = 0x8A43;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BLOCK_BINDING: types::GLenum = 0x8A3F;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BLOCK_DATA_SIZE: types::GLenum = 0x8A40;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BLOCK_INDEX: types::GLenum = 0x8A3A;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BLOCK_NAME_LENGTH: types::GLenum = 0x8A41;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BLOCK_REFERENCED_BY_FRAGMENT_SHADER: types::GLenum = 0x8A46;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BLOCK_REFERENCED_BY_GEOMETRY_SHADER: types::GLenum = 0x8A45;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BLOCK_REFERENCED_BY_VERTEX_SHADER: types::GLenum = 0x8A44;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BUFFER: types::GLenum = 0x8A11;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BUFFER_BINDING: types::GLenum = 0x8A28;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BUFFER_OFFSET_ALIGNMENT: types::GLenum = 0x8A34;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BUFFER_SIZE: types::GLenum = 0x8A2A;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_BUFFER_START: types::GLenum = 0x8A29;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_IS_ROW_MAJOR: types::GLenum = 0x8A3E;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_MATRIX_STRIDE: types::GLenum = 0x8A3D;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_NAME_LENGTH: types::GLenum = 0x8A39;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_OFFSET: types::GLenum = 0x8A3B;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_SIZE: types::GLenum = 0x8A38;
#[allow(dead_code, non_upper_case_globals)]
pub const UNIFORM_TYPE: types::GLenum = 0x8A37;
#[allow(dead_code, non_upper_case_globals)]
pub const UNPACK_ALIGNMENT: types::GLenum = 0x0CF5;
#[allow(dead_code, non_upper_case_globals)]
pub const UNPACK_CLIENT_STORAGE_APPLE: types::GLenum = 0x85B2;
#[allow(dead_code, non_upper_case_globals)]
pub const UNPACK_IMAGE_HEIGHT: types::GLenum = 0x806E;
#[allow(dead_code, non_upper_case_globals)]
pub const UNPACK_LSB_FIRST: types::GLenum = 0x0CF1;
#[allow(dead_code, non_upper_case_globals)]
pub const UNPACK_ROW_LENGTH: types::GLenum = 0x0CF2;
#[allow(dead_code, non_upper_case_globals)]
pub const UNPACK_SKIP_IMAGES: types::GLenum = 0x806D;
#[allow(dead_code, non_upper_case_globals)]
pub const UNPACK_SKIP_PIXELS: types::GLenum = 0x0CF4;
#[allow(dead_code, non_upper_case_globals)]
pub const UNPACK_SKIP_ROWS: types::GLenum = 0x0CF3;
#[allow(dead_code, non_upper_case_globals)]
pub const UNPACK_SWAP_BYTES: types::GLenum = 0x0CF0;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNALED: types::GLenum = 0x9118;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_BYTE: types::GLenum = 0x1401;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_BYTE_2_3_3_REV: types::GLenum = 0x8362;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_BYTE_3_3_2: types::GLenum = 0x8032;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT: types::GLenum = 0x1405;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_10F_11F_11F_REV: types::GLenum = 0x8C3B;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_10_10_10_2: types::GLenum = 0x8036;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_24_8: types::GLenum = 0x84FA;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_2_10_10_10_REV: types::GLenum = 0x8368;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_5_9_9_9_REV: types::GLenum = 0x8C3E;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_8_8_8_8: types::GLenum = 0x8035;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_8_8_8_8_REV: types::GLenum = 0x8367;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_ATOMIC_COUNTER: types::GLenum = 0x92DB;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_IMAGE_2D: types::GLenum = 0x9063;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_IMAGE_2D_ARRAY: types::GLenum = 0x9069;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_IMAGE_3D: types::GLenum = 0x9064;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_IMAGE_CUBE: types::GLenum = 0x9066;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_SAMPLER_1D: types::GLenum = 0x8DD1;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_SAMPLER_1D_ARRAY: types::GLenum = 0x8DD6;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_SAMPLER_2D: types::GLenum = 0x8DD2;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_SAMPLER_2D_ARRAY: types::GLenum = 0x8DD7;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_SAMPLER_2D_MULTISAMPLE: types::GLenum = 0x910A;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_SAMPLER_2D_MULTISAMPLE_ARRAY: types::GLenum = 0x910D;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_SAMPLER_2D_RECT: types::GLenum = 0x8DD5;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_SAMPLER_3D: types::GLenum = 0x8DD3;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_SAMPLER_BUFFER: types::GLenum = 0x8DD8;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_SAMPLER_CUBE: types::GLenum = 0x8DD4;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_VEC2: types::GLenum = 0x8DC6;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_VEC3: types::GLenum = 0x8DC7;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_INT_VEC4: types::GLenum = 0x8DC8;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_NORMALIZED: types::GLenum = 0x8C17;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_SHORT: types::GLenum = 0x1403;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_SHORT_1_5_5_5_REV: types::GLenum = 0x8366;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_SHORT_4_4_4_4: types::GLenum = 0x8033;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_SHORT_4_4_4_4_REV: types::GLenum = 0x8365;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_SHORT_5_5_5_1: types::GLenum = 0x8034;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_SHORT_5_6_5: types::GLenum = 0x8363;
#[allow(dead_code, non_upper_case_globals)]
pub const UNSIGNED_SHORT_5_6_5_REV: types::GLenum = 0x8364;
#[allow(dead_code, non_upper_case_globals)]
pub const UPPER_LEFT: types::GLenum = 0x8CA2;
#[allow(dead_code, non_upper_case_globals)]
pub const V2F: types::GLenum = 0x2A20;
#[allow(dead_code, non_upper_case_globals)]
pub const V3F: types::GLenum = 0x2A21;
#[allow(dead_code, non_upper_case_globals)]
pub const VALIDATE_STATUS: types::GLenum = 0x8B83;
#[allow(dead_code, non_upper_case_globals)]
pub const VENDOR: types::GLenum = 0x1F00;
#[allow(dead_code, non_upper_case_globals)]
pub const VERSION: types::GLenum = 0x1F02;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ARRAY: types::GLenum = 0x8074;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ARRAY_BINDING: types::GLenum = 0x85B5;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ARRAY_BINDING_APPLE: types::GLenum = 0x85B5;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ARRAY_BUFFER_BINDING: types::GLenum = 0x8896;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ARRAY_KHR: types::GLenum = 0x8074;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ARRAY_POINTER: types::GLenum = 0x808E;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ARRAY_SIZE: types::GLenum = 0x807A;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ARRAY_STRIDE: types::GLenum = 0x807C;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ARRAY_TYPE: types::GLenum = 0x807B;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ATTRIB_ARRAY_BARRIER_BIT: types::GLenum = 0x00000001;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ATTRIB_ARRAY_BUFFER_BINDING: types::GLenum = 0x889F;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ATTRIB_ARRAY_DIVISOR: types::GLenum = 0x88FE;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ATTRIB_ARRAY_ENABLED: types::GLenum = 0x8622;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ATTRIB_ARRAY_INTEGER: types::GLenum = 0x88FD;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ATTRIB_ARRAY_NORMALIZED: types::GLenum = 0x886A;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ATTRIB_ARRAY_POINTER: types::GLenum = 0x8645;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ATTRIB_ARRAY_SIZE: types::GLenum = 0x8623;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ATTRIB_ARRAY_STRIDE: types::GLenum = 0x8624;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ATTRIB_ARRAY_TYPE: types::GLenum = 0x8625;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ATTRIB_BINDING: types::GLenum = 0x82D4;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_ATTRIB_RELATIVE_OFFSET: types::GLenum = 0x82D5;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_BINDING_BUFFER: types::GLenum = 0x8F4F;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_BINDING_DIVISOR: types::GLenum = 0x82D6;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_BINDING_OFFSET: types::GLenum = 0x82D7;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_BINDING_STRIDE: types::GLenum = 0x82D8;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_PROGRAM_POINT_SIZE: types::GLenum = 0x8642;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_PROGRAM_TWO_SIDE: types::GLenum = 0x8643;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_SHADER: types::GLenum = 0x8B31;
#[allow(dead_code, non_upper_case_globals)]
pub const VERTEX_SHADER_BIT: types::GLenum = 0x00000001;
#[allow(dead_code, non_upper_case_globals)]
pub const VIEWPORT: types::GLenum = 0x0BA2;
#[allow(dead_code, non_upper_case_globals)]
pub const VIEWPORT_BIT: types::GLenum = 0x00000800;
#[allow(dead_code, non_upper_case_globals)]
pub const WAIT_FAILED: types::GLenum = 0x911D;
#[allow(dead_code, non_upper_case_globals)]
pub const WEIGHT_ARRAY_BUFFER_BINDING: types::GLenum = 0x889E;
#[allow(dead_code, non_upper_case_globals)]
pub const WRITE_ONLY: types::GLenum = 0x88B9;
#[allow(dead_code, non_upper_case_globals)]
pub const XOR: types::GLenum = 0x1506;
#[allow(dead_code, non_upper_case_globals)]
pub const ZERO: types::GLenum = 0;
#[allow(dead_code, non_upper_case_globals)]
pub const ZOOM_X: types::GLenum = 0x0D16;
#[allow(dead_code, non_upper_case_globals)]
pub const ZOOM_Y: types::GLenum = 0x0D17;
