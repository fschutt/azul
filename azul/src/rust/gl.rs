    #![allow(dead_code, unused_imports)]
    //! OpenGl helper types (`Texture`, `GlContext`, etc.)
    use crate::dll::*;
    use std::ffi::c_void;
    impl Refstr {
        fn as_str(&self) -> &str { unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.ptr, self.len)) } }
    }

    impl From<&str> for Refstr {
        fn from(s: &str) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl RefstrVecRef {
        fn as_slice(&self) -> &[Refstr] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl From<&[Refstr]> for RefstrVecRef {
        fn from(s: &[Refstr]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl From<&mut [GLint64]> for GLint64VecRefMut {
        fn from(s: &mut [GLint64]) -> Self {
            Self { ptr: s.as_mut_ptr(), len: s.len() }
        }
    }

    impl GLint64VecRefMut {
        fn as_mut_slice(&mut self) -> &mut [GLint64] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) } }
    }

    impl From<&mut [GLfloat]> for GLfloatVecRefMut {
        fn from(s: &mut [GLfloat]) -> Self {
            Self { ptr: s.as_mut_ptr(), len: s.len() }
        }
    }

    impl GLfloatVecRefMut {
        fn as_mut_slice(&mut self) -> &mut [GLfloat] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) } }
    }

    impl From<&mut [GLint]> for GLintVecRefMut {
        fn from(s: &mut [GLint]) -> Self {
            Self { ptr: s.as_mut_ptr(), len: s.len() }
        }
    }

    impl GLintVecRefMut {
        fn as_mut_slice(&mut self) -> &mut [GLint] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) } }
    }

    impl From<&[GLuint]> for GLuintVecRef {
        fn from(s: &[GLuint]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl GLuintVecRef {
        fn as_slice(&self) -> &[GLuint] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl From<&[GLenum]> for GLenumVecRef {
        fn from(s: &[GLenum]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl GLenumVecRef {
        fn as_slice(&self) -> &[GLenum] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl From<&[u8]> for U8VecRef {
        fn from(s: &[u8]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl U8VecRef {
        fn as_slice(&self) -> &[u8] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl PartialOrd for U8VecRef {
        fn partial_cmp(&self, rhs: &Self) -> Option<std::cmp::Ordering> {
            self.as_slice().partial_cmp(rhs.as_slice())
        }
    }

    impl Ord for U8VecRef {
        fn cmp(&self, rhs: &Self) -> std::cmp::Ordering {
            self.as_slice().cmp(rhs.as_slice())
        }
    }

    impl PartialEq for U8VecRef {
        fn eq(&self, rhs: &Self) -> bool {
            self.as_slice().eq(rhs.as_slice())
        }
    }

    impl Eq for U8VecRef { }

    impl std::hash::Hash for U8VecRef {
        fn hash<H>(&self, state: &mut H) where H: std::hash::Hasher {
            self.as_slice().hash(state)
        }
    }

    impl From<&[f32]> for F32VecRef {
        fn from(s: &[f32]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl F32VecRef {
        fn as_slice(&self) -> &[f32] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl From<&[i32]> for I32VecRef {
        fn from(s: &[i32]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl I32VecRef {
        fn as_slice(&self) -> &[i32] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl From<&mut [GLboolean]> for GLbooleanVecRefMut {
        fn from(s: &mut [GLboolean]) -> Self {
            Self { ptr: s.as_mut_ptr(), len: s.len() }
        }
    }

    impl GLbooleanVecRefMut {
        fn as_mut_slice(&mut self) -> &mut [GLboolean] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) } }
    }

    impl From<&mut [u8]> for U8VecRefMut {
        fn from(s: &mut [u8]) -> Self {
            Self { ptr: s.as_mut_ptr(), len: s.len() }
        }
    }

    impl U8VecRefMut {
        fn as_mut_slice(&mut self) -> &mut [u8] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) } }
    }

    pub type GLenum = std::os::raw::c_uint;
    pub type GLboolean = std::os::raw::c_uchar;
    pub type GLbitfield = std::os::raw::c_uint;
    pub type GLvoid = std::os::raw::c_void;
    pub type GLbyte = std::os::raw::c_char;
    pub type GLshort = std::os::raw::c_short;
    pub type GLint = std::os::raw::c_int;
    pub type GLclampx = std::os::raw::c_int;
    pub type GLubyte = std::os::raw::c_uchar;
    pub type GLushort = std::os::raw::c_ushort;
    pub type GLuint = std::os::raw::c_uint;
    pub type GLsizei = std::os::raw::c_int;
    pub type GLfloat = std::os::raw::c_float;
    pub type GLclampf = std::os::raw::c_float;
    pub type GLdouble = std::os::raw::c_double;
    pub type GLclampd = std::os::raw::c_double;
    pub type GLeglImageOES = *const std::os::raw::c_void;
    pub type GLchar = std::os::raw::c_char;
    pub type GLcharARB = std::os::raw::c_char;

    #[cfg(target_os = "macos")]
    pub type GLhandleARB = *const std::os::raw::c_void;
    #[cfg(not(target_os = "macos"))]
    pub type GLhandleARB = std::os::raw::c_uint;

    pub type GLhalfARB = std::os::raw::c_ushort;
    pub type GLhalf = std::os::raw::c_ushort;

    // Must be 32 bits
    pub type GLfixed = GLint;
    pub type GLintptr = isize;
    pub type GLsizeiptr = isize;
    pub type GLint64 = i64;
    pub type GLuint64 = u64;
    pub type GLintptrARB = isize;
    pub type GLsizeiptrARB = isize;
    pub type GLint64EXT = i64;
    pub type GLuint64EXT = u64;

    pub type GLDEBUGPROC = Option<extern "system" fn(source: GLenum, gltype: GLenum, id: GLuint, severity: GLenum, length: GLsizei, message: *const GLchar, userParam: *mut std::os::raw::c_void)>;
    pub type GLDEBUGPROCARB = Option<extern "system" fn(source: GLenum, gltype: GLenum, id: GLuint, severity: GLenum, length: GLsizei, message: *const GLchar, userParam: *mut std::os::raw::c_void)>;
    pub type GLDEBUGPROCKHR = Option<extern "system" fn(source: GLenum, gltype: GLenum, id: GLuint, severity: GLenum, length: GLsizei, message: *const GLchar, userParam: *mut std::os::raw::c_void)>;

    // Vendor extension types
    pub type GLDEBUGPROCAMD = Option<extern "system" fn(id: GLuint, category: GLenum, severity: GLenum, length: GLsizei, message: *const GLchar, userParam: *mut std::os::raw::c_void)>;
    pub type GLhalfNV = std::os::raw::c_ushort;
    pub type GLvdpauSurfaceNV = GLintptr;

    pub const ACCUM: GLenum = 0x0100;
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
    pub const FALSE: GLboolean = 0;
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
    #[allow(non_upper_case_globals)]
    pub const FLOAT_MAT2x3: GLenum = 0x8B65;
    #[allow(non_upper_case_globals)]
    pub const FLOAT_MAT2x4: GLenum = 0x8B66;
    pub const FLOAT_MAT3: GLenum = 0x8B5B;
    #[allow(non_upper_case_globals)]
    pub const FLOAT_MAT3x2: GLenum = 0x8B67;
    #[allow(non_upper_case_globals)]
    pub const FLOAT_MAT3x4: GLenum = 0x8B68;
    pub const FLOAT_MAT4: GLenum = 0x8B5C;
    #[allow(non_upper_case_globals)]
    pub const FLOAT_MAT4x2: GLenum = 0x8B69;
    #[allow(non_upper_case_globals)]
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
    pub const INVALID_INDEX: GLuint = 0xFFFFFFFF;
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
    pub const TIMEOUT_IGNORED: GLuint64 = 0xFFFFFFFFFFFFFFFF;
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
    pub const TRUE: GLboolean = 1;
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

    use crate::vec::{GLuintVec, StringVec};
    use crate::option::OptionU8VecRef;


    /// `GlShaderPrecisionFormatReturn` struct
    #[doc(inline)] pub use crate::dll::AzGlShaderPrecisionFormatReturn as GlShaderPrecisionFormatReturn;

    impl Clone for GlShaderPrecisionFormatReturn { fn clone(&self) -> Self { *self } }
    impl Copy for GlShaderPrecisionFormatReturn { }


    /// `VertexAttributeType` struct
    #[doc(inline)] pub use crate::dll::AzVertexAttributeType as VertexAttributeType;

    impl Clone for VertexAttributeType { fn clone(&self) -> Self { *self } }
    impl Copy for VertexAttributeType { }


    /// `VertexAttribute` struct
    #[doc(inline)] pub use crate::dll::AzVertexAttribute as VertexAttribute;

    impl Clone for VertexAttribute { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_vertex_attribute_deep_copy)(self) } }
    impl Drop for VertexAttribute { fn drop(&mut self) { (crate::dll::get_azul_dll().az_vertex_attribute_delete)(self); } }


    /// `VertexLayout` struct
    #[doc(inline)] pub use crate::dll::AzVertexLayout as VertexLayout;

    impl Clone for VertexLayout { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_vertex_layout_deep_copy)(self) } }
    impl Drop for VertexLayout { fn drop(&mut self) { (crate::dll::get_azul_dll().az_vertex_layout_delete)(self); } }


    /// `VertexArrayObject` struct
    #[doc(inline)] pub use crate::dll::AzVertexArrayObject as VertexArrayObject;

    impl Drop for VertexArrayObject { fn drop(&mut self) { (crate::dll::get_azul_dll().az_vertex_array_object_delete)(self); } }


    /// `IndexBufferFormat` struct
    #[doc(inline)] pub use crate::dll::AzIndexBufferFormat as IndexBufferFormat;

    impl Clone for IndexBufferFormat { fn clone(&self) -> Self { *self } }
    impl Copy for IndexBufferFormat { }


    /// `VertexBuffer` struct
    #[doc(inline)] pub use crate::dll::AzVertexBuffer as VertexBuffer;

    impl Drop for VertexBuffer { fn drop(&mut self) { (crate::dll::get_azul_dll().az_vertex_buffer_delete)(self); } }


    /// `GlType` struct
    #[doc(inline)] pub use crate::dll::AzGlType as GlType;

    impl Clone for GlType { fn clone(&self) -> Self { *self } }
    impl Copy for GlType { }


    /// `DebugMessage` struct
    #[doc(inline)] pub use crate::dll::AzDebugMessage as DebugMessage;

    impl Clone for DebugMessage { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_debug_message_deep_copy)(self) } }
    impl Drop for DebugMessage { fn drop(&mut self) { (crate::dll::get_azul_dll().az_debug_message_delete)(self); } }


    /// C-ABI stable reexport of `&[u8]`
    #[doc(inline)] pub use crate::dll::AzU8VecRef as U8VecRef;

    impl Drop for U8VecRef { fn drop(&mut self) { (crate::dll::get_azul_dll().az_u8_vec_ref_delete)(self); } }


    /// C-ABI stable reexport of `&mut [u8]`
    #[doc(inline)] pub use crate::dll::AzU8VecRefMut as U8VecRefMut;

    impl Drop for U8VecRefMut { fn drop(&mut self) { (crate::dll::get_azul_dll().az_u8_vec_ref_mut_delete)(self); } }


    /// C-ABI stable reexport of `&[f32]`
    #[doc(inline)] pub use crate::dll::AzF32VecRef as F32VecRef;

    impl Drop for F32VecRef { fn drop(&mut self) { (crate::dll::get_azul_dll().az_f32_vec_ref_delete)(self); } }


    /// C-ABI stable reexport of `&[i32]`
    #[doc(inline)] pub use crate::dll::AzI32VecRef as I32VecRef;

    impl Drop for I32VecRef { fn drop(&mut self) { (crate::dll::get_azul_dll().az_i32_vec_ref_delete)(self); } }


    /// C-ABI stable reexport of `&[GLuint]` aka `&[u32]`
    #[doc(inline)] pub use crate::dll::AzGLuintVecRef as GLuintVecRef;

    impl Drop for GLuintVecRef { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_luint_vec_ref_delete)(self); } }


    /// C-ABI stable reexport of `&[GLenum]` aka `&[u32]`
    #[doc(inline)] pub use crate::dll::AzGLenumVecRef as GLenumVecRef;

    impl Drop for GLenumVecRef { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lenum_vec_ref_delete)(self); } }


    /// C-ABI stable reexport of `&mut [GLint]` aka `&mut [i32]`
    #[doc(inline)] pub use crate::dll::AzGLintVecRefMut as GLintVecRefMut;

    impl Drop for GLintVecRefMut { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lint_vec_ref_mut_delete)(self); } }


    /// C-ABI stable reexport of `&mut [GLint64]` aka `&mut [i64]`
    #[doc(inline)] pub use crate::dll::AzGLint64VecRefMut as GLint64VecRefMut;

    impl Drop for GLint64VecRefMut { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lint64_vec_ref_mut_delete)(self); } }


    /// C-ABI stable reexport of `&mut [GLboolean]` aka `&mut [u8]`
    #[doc(inline)] pub use crate::dll::AzGLbooleanVecRefMut as GLbooleanVecRefMut;

    impl Drop for GLbooleanVecRefMut { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lboolean_vec_ref_mut_delete)(self); } }


    /// C-ABI stable reexport of `&mut [GLfloat]` aka `&mut [f32]`
    #[doc(inline)] pub use crate::dll::AzGLfloatVecRefMut as GLfloatVecRefMut;

    impl Drop for GLfloatVecRefMut { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lfloat_vec_ref_mut_delete)(self); } }


    /// C-ABI stable reexport of `&[Refstr]` aka `&mut [&str]`
    #[doc(inline)] pub use crate::dll::AzRefstrVecRef as RefstrVecRef;

    impl Drop for RefstrVecRef { fn drop(&mut self) { (crate::dll::get_azul_dll().az_refstr_vec_ref_delete)(self); } }


    /// C-ABI stable reexport of `&str`
    #[doc(inline)] pub use crate::dll::AzRefstr as Refstr;

    impl Drop for Refstr { fn drop(&mut self) { (crate::dll::get_azul_dll().az_refstr_delete)(self); } }


    /// C-ABI stable reexport of `(U8Vec, u32)`
    #[doc(inline)] pub use crate::dll::AzGetProgramBinaryReturn as GetProgramBinaryReturn;

    impl Clone for GetProgramBinaryReturn { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_get_program_binary_return_deep_copy)(self) } }
    impl Drop for GetProgramBinaryReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_get_program_binary_return_delete)(self); } }


    /// C-ABI stable reexport of `(i32, u32, AzString)`
    #[doc(inline)] pub use crate::dll::AzGetActiveAttribReturn as GetActiveAttribReturn;

    impl Clone for GetActiveAttribReturn { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_get_active_attrib_return_deep_copy)(self) } }
    impl Drop for GetActiveAttribReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_get_active_attrib_return_delete)(self); } }


    /// C-ABI stable reexport of `*const gleam::gl::GLsync`
    #[doc(inline)] pub use crate::dll::AzGLsyncPtr as GLsyncPtr;

    impl Drop for GLsyncPtr { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lsync_ptr_delete)(self); } }


    /// C-ABI stable reexport of `(i32, u32, AzString)`
    #[doc(inline)] pub use crate::dll::AzGetActiveUniformReturn as GetActiveUniformReturn;

    impl Clone for GetActiveUniformReturn { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_get_active_uniform_return_deep_copy)(self) } }
    impl Drop for GetActiveUniformReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_get_active_uniform_return_delete)(self); } }


    /// `GlContextPtr` struct
    #[doc(inline)] pub use crate::dll::AzGlContextPtr as GlContextPtr;

    impl GlContextPtr {
        /// Calls the `GlContextPtr::get_type` function.
        pub fn get_type(&self)  -> crate::gl::GlType { (crate::dll::get_azul_dll().az_gl_context_ptr_get_type)(self) }
        /// Calls the `GlContextPtr::buffer_data_untyped` function.
        pub fn buffer_data_untyped(&self, target: u32, size: isize, data: *const c_void, usage: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_buffer_data_untyped)(self, target, size, data, usage) }
        /// Calls the `GlContextPtr::buffer_sub_data_untyped` function.
        pub fn buffer_sub_data_untyped(&self, target: u32, offset: isize, size: isize, data: *const c_void)  { (crate::dll::get_azul_dll().az_gl_context_ptr_buffer_sub_data_untyped)(self, target, offset, size, data) }
        /// Calls the `GlContextPtr::map_buffer` function.
        pub fn map_buffer(&self, target: u32, access: u32)  -> *mut c_void { (crate::dll::get_azul_dll().az_gl_context_ptr_map_buffer)(self, target, access) }
        /// Calls the `GlContextPtr::map_buffer_range` function.
        pub fn map_buffer_range(&self, target: u32, offset: isize, length: isize, access: u32)  -> *mut c_void { (crate::dll::get_azul_dll().az_gl_context_ptr_map_buffer_range)(self, target, offset, length, access) }
        /// Calls the `GlContextPtr::unmap_buffer` function.
        pub fn unmap_buffer(&self, target: u32)  -> u8 { (crate::dll::get_azul_dll().az_gl_context_ptr_unmap_buffer)(self, target) }
        /// Calls the `GlContextPtr::tex_buffer` function.
        pub fn tex_buffer(&self, target: u32, internal_format: u32, buffer: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_buffer)(self, target, internal_format, buffer) }
        /// Calls the `GlContextPtr::shader_source` function.
        pub fn shader_source(&self, shader: u32, strings: StringVec)  { (crate::dll::get_azul_dll().az_gl_context_ptr_shader_source)(self, shader, strings) }
        /// Calls the `GlContextPtr::read_buffer` function.
        pub fn read_buffer(&self, mode: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_read_buffer)(self, mode) }
        /// Calls the `GlContextPtr::read_pixels_into_buffer` function.
        pub fn read_pixels_into_buffer(&self, x: i32, y: i32, width: i32, height: i32, format: u32, pixel_type: u32, dst_buffer: U8VecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_read_pixels_into_buffer)(self, x, y, width, height, format, pixel_type, dst_buffer) }
        /// Calls the `GlContextPtr::read_pixels` function.
        pub fn read_pixels(&self, x: i32, y: i32, width: i32, height: i32, format: u32, pixel_type: u32)  -> crate::vec::U8Vec { (crate::dll::get_azul_dll().az_gl_context_ptr_read_pixels)(self, x, y, width, height, format, pixel_type) }
        /// Calls the `GlContextPtr::read_pixels_into_pbo` function.
        pub fn read_pixels_into_pbo(&self, x: i32, y: i32, width: i32, height: i32, format: u32, pixel_type: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_read_pixels_into_pbo)(self, x, y, width, height, format, pixel_type) }
        /// Calls the `GlContextPtr::sample_coverage` function.
        pub fn sample_coverage(&self, value: f32, invert: bool)  { (crate::dll::get_azul_dll().az_gl_context_ptr_sample_coverage)(self, value, invert) }
        /// Calls the `GlContextPtr::polygon_offset` function.
        pub fn polygon_offset(&self, factor: f32, units: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_polygon_offset)(self, factor, units) }
        /// Calls the `GlContextPtr::pixel_store_i` function.
        pub fn pixel_store_i(&self, name: u32, param: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_pixel_store_i)(self, name, param) }
        /// Calls the `GlContextPtr::gen_buffers` function.
        pub fn gen_buffers(&self, n: i32)  -> crate::vec::GLuintVec { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_buffers)(self, n) }
        /// Calls the `GlContextPtr::gen_renderbuffers` function.
        pub fn gen_renderbuffers(&self, n: i32)  -> crate::vec::GLuintVec { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_renderbuffers)(self, n) }
        /// Calls the `GlContextPtr::gen_framebuffers` function.
        pub fn gen_framebuffers(&self, n: i32)  -> crate::vec::GLuintVec { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_framebuffers)(self, n) }
        /// Calls the `GlContextPtr::gen_textures` function.
        pub fn gen_textures(&self, n: i32)  -> crate::vec::GLuintVec { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_textures)(self, n) }
        /// Calls the `GlContextPtr::gen_vertex_arrays` function.
        pub fn gen_vertex_arrays(&self, n: i32)  -> crate::vec::GLuintVec { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_vertex_arrays)(self, n) }
        /// Calls the `GlContextPtr::gen_queries` function.
        pub fn gen_queries(&self, n: i32)  -> crate::vec::GLuintVec { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_queries)(self, n) }
        /// Calls the `GlContextPtr::begin_query` function.
        pub fn begin_query(&self, target: u32, id: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_begin_query)(self, target, id) }
        /// Calls the `GlContextPtr::end_query` function.
        pub fn end_query(&self, target: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_end_query)(self, target) }
        /// Calls the `GlContextPtr::query_counter` function.
        pub fn query_counter(&self, id: u32, target: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_query_counter)(self, id, target) }
        /// Calls the `GlContextPtr::get_query_object_iv` function.
        pub fn get_query_object_iv(&self, id: u32, pname: u32)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_query_object_iv)(self, id, pname) }
        /// Calls the `GlContextPtr::get_query_object_uiv` function.
        pub fn get_query_object_uiv(&self, id: u32, pname: u32)  -> u32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_query_object_uiv)(self, id, pname) }
        /// Calls the `GlContextPtr::get_query_object_i64v` function.
        pub fn get_query_object_i64v(&self, id: u32, pname: u32)  -> i64 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_query_object_i64v)(self, id, pname) }
        /// Calls the `GlContextPtr::get_query_object_ui64v` function.
        pub fn get_query_object_ui64v(&self, id: u32, pname: u32)  -> u64 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_query_object_ui64v)(self, id, pname) }
        /// Calls the `GlContextPtr::delete_queries` function.
        pub fn delete_queries(&self, queries: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_queries)(self, queries) }
        /// Calls the `GlContextPtr::delete_vertex_arrays` function.
        pub fn delete_vertex_arrays(&self, vertex_arrays: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_vertex_arrays)(self, vertex_arrays) }
        /// Calls the `GlContextPtr::delete_buffers` function.
        pub fn delete_buffers(&self, buffers: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_buffers)(self, buffers) }
        /// Calls the `GlContextPtr::delete_renderbuffers` function.
        pub fn delete_renderbuffers(&self, renderbuffers: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_renderbuffers)(self, renderbuffers) }
        /// Calls the `GlContextPtr::delete_framebuffers` function.
        pub fn delete_framebuffers(&self, framebuffers: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_framebuffers)(self, framebuffers) }
        /// Calls the `GlContextPtr::delete_textures` function.
        pub fn delete_textures(&self, textures: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_textures)(self, textures) }
        /// Calls the `GlContextPtr::framebuffer_renderbuffer` function.
        pub fn framebuffer_renderbuffer(&self, target: u32, attachment: u32, renderbuffertarget: u32, renderbuffer: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_framebuffer_renderbuffer)(self, target, attachment, renderbuffertarget, renderbuffer) }
        /// Calls the `GlContextPtr::renderbuffer_storage` function.
        pub fn renderbuffer_storage(&self, target: u32, internalformat: u32, width: i32, height: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_renderbuffer_storage)(self, target, internalformat, width, height) }
        /// Calls the `GlContextPtr::depth_func` function.
        pub fn depth_func(&self, func: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_depth_func)(self, func) }
        /// Calls the `GlContextPtr::active_texture` function.
        pub fn active_texture(&self, texture: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_active_texture)(self, texture) }
        /// Calls the `GlContextPtr::attach_shader` function.
        pub fn attach_shader(&self, program: u32, shader: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_attach_shader)(self, program, shader) }
        /// Calls the `GlContextPtr::bind_attrib_location` function.
        pub fn bind_attrib_location(&self, program: u32, index: u32, name: Refstr)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_attrib_location)(self, program, index, name) }
        /// Calls the `GlContextPtr::get_uniform_iv` function.
        pub fn get_uniform_iv(&self, program: u32, location: i32, result: GLintVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_uniform_iv)(self, program, location, result) }
        /// Calls the `GlContextPtr::get_uniform_fv` function.
        pub fn get_uniform_fv(&self, program: u32, location: i32, result: GLfloatVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_uniform_fv)(self, program, location, result) }
        /// Calls the `GlContextPtr::get_uniform_block_index` function.
        pub fn get_uniform_block_index(&self, program: u32, name: Refstr)  -> u32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_uniform_block_index)(self, program, name) }
        /// Calls the `GlContextPtr::get_uniform_indices` function.
        pub fn get_uniform_indices(&self, program: u32, names: RefstrVecRef)  -> crate::vec::GLuintVec { (crate::dll::get_azul_dll().az_gl_context_ptr_get_uniform_indices)(self, program, names) }
        /// Calls the `GlContextPtr::bind_buffer_base` function.
        pub fn bind_buffer_base(&self, target: u32, index: u32, buffer: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_buffer_base)(self, target, index, buffer) }
        /// Calls the `GlContextPtr::bind_buffer_range` function.
        pub fn bind_buffer_range(&self, target: u32, index: u32, buffer: u32, offset: isize, size: isize)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_buffer_range)(self, target, index, buffer, offset, size) }
        /// Calls the `GlContextPtr::uniform_block_binding` function.
        pub fn uniform_block_binding(&self, program: u32, uniform_block_index: u32, uniform_block_binding: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_block_binding)(self, program, uniform_block_index, uniform_block_binding) }
        /// Calls the `GlContextPtr::bind_buffer` function.
        pub fn bind_buffer(&self, target: u32, buffer: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_buffer)(self, target, buffer) }
        /// Calls the `GlContextPtr::bind_vertex_array` function.
        pub fn bind_vertex_array(&self, vao: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_vertex_array)(self, vao) }
        /// Calls the `GlContextPtr::bind_renderbuffer` function.
        pub fn bind_renderbuffer(&self, target: u32, renderbuffer: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_renderbuffer)(self, target, renderbuffer) }
        /// Calls the `GlContextPtr::bind_framebuffer` function.
        pub fn bind_framebuffer(&self, target: u32, framebuffer: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_framebuffer)(self, target, framebuffer) }
        /// Calls the `GlContextPtr::bind_texture` function.
        pub fn bind_texture(&self, target: u32, texture: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_texture)(self, target, texture) }
        /// Calls the `GlContextPtr::draw_buffers` function.
        pub fn draw_buffers(&self, bufs: GLenumVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_draw_buffers)(self, bufs) }
        /// Calls the `GlContextPtr::tex_image_2d` function.
        pub fn tex_image_2d(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, border: i32, format: u32, ty: u32, opt_data: OptionU8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_image_2d)(self, target, level, internal_format, width, height, border, format, ty, opt_data) }
        /// Calls the `GlContextPtr::compressed_tex_image_2d` function.
        pub fn compressed_tex_image_2d(&self, target: u32, level: i32, internal_format: u32, width: i32, height: i32, border: i32, data: U8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_compressed_tex_image_2d)(self, target, level, internal_format, width, height, border, data) }
        /// Calls the `GlContextPtr::compressed_tex_sub_image_2d` function.
        pub fn compressed_tex_sub_image_2d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, width: i32, height: i32, format: u32, data: U8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_compressed_tex_sub_image_2d)(self, target, level, xoffset, yoffset, width, height, format, data) }
        /// Calls the `GlContextPtr::tex_image_3d` function.
        pub fn tex_image_3d(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, depth: i32, border: i32, format: u32, ty: u32, opt_data: OptionU8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_image_3d)(self, target, level, internal_format, width, height, depth, border, format, ty, opt_data) }
        /// Calls the `GlContextPtr::copy_tex_image_2d` function.
        pub fn copy_tex_image_2d(&self, target: u32, level: i32, internal_format: u32, x: i32, y: i32, width: i32, height: i32, border: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_tex_image_2d)(self, target, level, internal_format, x, y, width, height, border) }
        /// Calls the `GlContextPtr::copy_tex_sub_image_2d` function.
        pub fn copy_tex_sub_image_2d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, x: i32, y: i32, width: i32, height: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_tex_sub_image_2d)(self, target, level, xoffset, yoffset, x, y, width, height) }
        /// Calls the `GlContextPtr::copy_tex_sub_image_3d` function.
        pub fn copy_tex_sub_image_3d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, zoffset: i32, x: i32, y: i32, width: i32, height: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_tex_sub_image_3d)(self, target, level, xoffset, yoffset, zoffset, x, y, width, height) }
        /// Calls the `GlContextPtr::tex_sub_image_2d` function.
        pub fn tex_sub_image_2d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, width: i32, height: i32, format: u32, ty: u32, data: U8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_sub_image_2d)(self, target, level, xoffset, yoffset, width, height, format, ty, data) }
        /// Calls the `GlContextPtr::tex_sub_image_2d_pbo` function.
        pub fn tex_sub_image_2d_pbo(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, width: i32, height: i32, format: u32, ty: u32, offset: usize)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_sub_image_2d_pbo)(self, target, level, xoffset, yoffset, width, height, format, ty, offset) }
        /// Calls the `GlContextPtr::tex_sub_image_3d` function.
        pub fn tex_sub_image_3d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, zoffset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, data: U8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_sub_image_3d)(self, target, level, xoffset, yoffset, zoffset, width, height, depth, format, ty, data) }
        /// Calls the `GlContextPtr::tex_sub_image_3d_pbo` function.
        pub fn tex_sub_image_3d_pbo(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, zoffset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, offset: usize)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_sub_image_3d_pbo)(self, target, level, xoffset, yoffset, zoffset, width, height, depth, format, ty, offset) }
        /// Calls the `GlContextPtr::tex_storage_2d` function.
        pub fn tex_storage_2d(&self, target: u32, levels: i32, internal_format: u32, width: i32, height: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_storage_2d)(self, target, levels, internal_format, width, height) }
        /// Calls the `GlContextPtr::tex_storage_3d` function.
        pub fn tex_storage_3d(&self, target: u32, levels: i32, internal_format: u32, width: i32, height: i32, depth: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_storage_3d)(self, target, levels, internal_format, width, height, depth) }
        /// Calls the `GlContextPtr::get_tex_image_into_buffer` function.
        pub fn get_tex_image_into_buffer(&self, target: u32, level: i32, format: u32, ty: u32, output: U8VecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_tex_image_into_buffer)(self, target, level, format, ty, output) }
        /// Calls the `GlContextPtr::copy_image_sub_data` function.
        pub fn copy_image_sub_data(&self, src_name: u32, src_target: u32, src_level: i32, src_x: i32, src_y: i32, src_z: i32, dst_name: u32, dst_target: u32, dst_level: i32, dst_x: i32, dst_y: i32, dst_z: i32, src_width: i32, src_height: i32, src_depth: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_image_sub_data)(self, src_name, src_target, src_level, src_x, src_y, src_z, dst_name, dst_target, dst_level, dst_x, dst_y, dst_z, src_width, src_height, src_depth) }
        /// Calls the `GlContextPtr::invalidate_framebuffer` function.
        pub fn invalidate_framebuffer(&self, target: u32, attachments: GLenumVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_invalidate_framebuffer)(self, target, attachments) }
        /// Calls the `GlContextPtr::invalidate_sub_framebuffer` function.
        pub fn invalidate_sub_framebuffer(&self, target: u32, attachments: GLenumVecRef, xoffset: i32, yoffset: i32, width: i32, height: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_invalidate_sub_framebuffer)(self, target, attachments, xoffset, yoffset, width, height) }
        /// Calls the `GlContextPtr::get_integer_v` function.
        pub fn get_integer_v(&self, name: u32, result: GLintVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_integer_v)(self, name, result) }
        /// Calls the `GlContextPtr::get_integer_64v` function.
        pub fn get_integer_64v(&self, name: u32, result: GLint64VecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_integer_64v)(self, name, result) }
        /// Calls the `GlContextPtr::get_integer_iv` function.
        pub fn get_integer_iv(&self, name: u32, index: u32, result: GLintVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_integer_iv)(self, name, index, result) }
        /// Calls the `GlContextPtr::get_integer_64iv` function.
        pub fn get_integer_64iv(&self, name: u32, index: u32, result: GLint64VecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_integer_64iv)(self, name, index, result) }
        /// Calls the `GlContextPtr::get_boolean_v` function.
        pub fn get_boolean_v(&self, name: u32, result: GLbooleanVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_boolean_v)(self, name, result) }
        /// Calls the `GlContextPtr::get_float_v` function.
        pub fn get_float_v(&self, name: u32, result: GLfloatVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_float_v)(self, name, result) }
        /// Calls the `GlContextPtr::get_framebuffer_attachment_parameter_iv` function.
        pub fn get_framebuffer_attachment_parameter_iv(&self, target: u32, attachment: u32, pname: u32)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_framebuffer_attachment_parameter_iv)(self, target, attachment, pname) }
        /// Calls the `GlContextPtr::get_renderbuffer_parameter_iv` function.
        pub fn get_renderbuffer_parameter_iv(&self, target: u32, pname: u32)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_renderbuffer_parameter_iv)(self, target, pname) }
        /// Calls the `GlContextPtr::get_tex_parameter_iv` function.
        pub fn get_tex_parameter_iv(&self, target: u32, name: u32)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_tex_parameter_iv)(self, target, name) }
        /// Calls the `GlContextPtr::get_tex_parameter_fv` function.
        pub fn get_tex_parameter_fv(&self, target: u32, name: u32)  -> f32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_tex_parameter_fv)(self, target, name) }
        /// Calls the `GlContextPtr::tex_parameter_i` function.
        pub fn tex_parameter_i(&self, target: u32, pname: u32, param: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_parameter_i)(self, target, pname, param) }
        /// Calls the `GlContextPtr::tex_parameter_f` function.
        pub fn tex_parameter_f(&self, target: u32, pname: u32, param: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_parameter_f)(self, target, pname, param) }
        /// Calls the `GlContextPtr::framebuffer_texture_2d` function.
        pub fn framebuffer_texture_2d(&self, target: u32, attachment: u32, textarget: u32, texture: u32, level: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_framebuffer_texture_2d)(self, target, attachment, textarget, texture, level) }
        /// Calls the `GlContextPtr::framebuffer_texture_layer` function.
        pub fn framebuffer_texture_layer(&self, target: u32, attachment: u32, texture: u32, level: i32, layer: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_framebuffer_texture_layer)(self, target, attachment, texture, level, layer) }
        /// Calls the `GlContextPtr::blit_framebuffer` function.
        pub fn blit_framebuffer(&self, src_x0: i32, src_y0: i32, src_x1: i32, src_y1: i32, dst_x0: i32, dst_y0: i32, dst_x1: i32, dst_y1: i32, mask: u32, filter: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_blit_framebuffer)(self, src_x0, src_y0, src_x1, src_y1, dst_x0, dst_y0, dst_x1, dst_y1, mask, filter) }
        /// Calls the `GlContextPtr::vertex_attrib_4f` function.
        pub fn vertex_attrib_4f(&self, index: u32, x: f32, y: f32, z: f32, w: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_vertex_attrib_4f)(self, index, x, y, z, w) }
        /// Calls the `GlContextPtr::vertex_attrib_pointer_f32` function.
        pub fn vertex_attrib_pointer_f32(&self, index: u32, size: i32, normalized: bool, stride: i32, offset: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_vertex_attrib_pointer_f32)(self, index, size, normalized, stride, offset) }
        /// Calls the `GlContextPtr::vertex_attrib_pointer` function.
        pub fn vertex_attrib_pointer(&self, index: u32, size: i32, type_: u32, normalized: bool, stride: i32, offset: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_vertex_attrib_pointer)(self, index, size, type_, normalized, stride, offset) }
        /// Calls the `GlContextPtr::vertex_attrib_i_pointer` function.
        pub fn vertex_attrib_i_pointer(&self, index: u32, size: i32, type_: u32, stride: i32, offset: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_vertex_attrib_i_pointer)(self, index, size, type_, stride, offset) }
        /// Calls the `GlContextPtr::vertex_attrib_divisor` function.
        pub fn vertex_attrib_divisor(&self, index: u32, divisor: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_vertex_attrib_divisor)(self, index, divisor) }
        /// Calls the `GlContextPtr::viewport` function.
        pub fn viewport(&self, x: i32, y: i32, width: i32, height: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_viewport)(self, x, y, width, height) }
        /// Calls the `GlContextPtr::scissor` function.
        pub fn scissor(&self, x: i32, y: i32, width: i32, height: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_scissor)(self, x, y, width, height) }
        /// Calls the `GlContextPtr::line_width` function.
        pub fn line_width(&self, width: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_line_width)(self, width) }
        /// Calls the `GlContextPtr::use_program` function.
        pub fn use_program(&self, program: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_use_program)(self, program) }
        /// Calls the `GlContextPtr::validate_program` function.
        pub fn validate_program(&self, program: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_validate_program)(self, program) }
        /// Calls the `GlContextPtr::draw_arrays` function.
        pub fn draw_arrays(&self, mode: u32, first: i32, count: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_draw_arrays)(self, mode, first, count) }
        /// Calls the `GlContextPtr::draw_arrays_instanced` function.
        pub fn draw_arrays_instanced(&self, mode: u32, first: i32, count: i32, primcount: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_draw_arrays_instanced)(self, mode, first, count, primcount) }
        /// Calls the `GlContextPtr::draw_elements` function.
        pub fn draw_elements(&self, mode: u32, count: i32, element_type: u32, indices_offset: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_draw_elements)(self, mode, count, element_type, indices_offset) }
        /// Calls the `GlContextPtr::draw_elements_instanced` function.
        pub fn draw_elements_instanced(&self, mode: u32, count: i32, element_type: u32, indices_offset: u32, primcount: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_draw_elements_instanced)(self, mode, count, element_type, indices_offset, primcount) }
        /// Calls the `GlContextPtr::blend_color` function.
        pub fn blend_color(&self, r: f32, g: f32, b: f32, a: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_blend_color)(self, r, g, b, a) }
        /// Calls the `GlContextPtr::blend_func` function.
        pub fn blend_func(&self, sfactor: u32, dfactor: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_blend_func)(self, sfactor, dfactor) }
        /// Calls the `GlContextPtr::blend_func_separate` function.
        pub fn blend_func_separate(&self, src_rgb: u32, dest_rgb: u32, src_alpha: u32, dest_alpha: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_blend_func_separate)(self, src_rgb, dest_rgb, src_alpha, dest_alpha) }
        /// Calls the `GlContextPtr::blend_equation` function.
        pub fn blend_equation(&self, mode: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_blend_equation)(self, mode) }
        /// Calls the `GlContextPtr::blend_equation_separate` function.
        pub fn blend_equation_separate(&self, mode_rgb: u32, mode_alpha: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_blend_equation_separate)(self, mode_rgb, mode_alpha) }
        /// Calls the `GlContextPtr::color_mask` function.
        pub fn color_mask(&self, r: bool, g: bool, b: bool, a: bool)  { (crate::dll::get_azul_dll().az_gl_context_ptr_color_mask)(self, r, g, b, a) }
        /// Calls the `GlContextPtr::cull_face` function.
        pub fn cull_face(&self, mode: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_cull_face)(self, mode) }
        /// Calls the `GlContextPtr::front_face` function.
        pub fn front_face(&self, mode: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_front_face)(self, mode) }
        /// Calls the `GlContextPtr::enable` function.
        pub fn enable(&self, cap: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_enable)(self, cap) }
        /// Calls the `GlContextPtr::disable` function.
        pub fn disable(&self, cap: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_disable)(self, cap) }
        /// Calls the `GlContextPtr::hint` function.
        pub fn hint(&self, param_name: u32, param_val: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_hint)(self, param_name, param_val) }
        /// Calls the `GlContextPtr::is_enabled` function.
        pub fn is_enabled(&self, cap: u32)  -> u8 { (crate::dll::get_azul_dll().az_gl_context_ptr_is_enabled)(self, cap) }
        /// Calls the `GlContextPtr::is_shader` function.
        pub fn is_shader(&self, shader: u32)  -> u8 { (crate::dll::get_azul_dll().az_gl_context_ptr_is_shader)(self, shader) }
        /// Calls the `GlContextPtr::is_texture` function.
        pub fn is_texture(&self, texture: u32)  -> u8 { (crate::dll::get_azul_dll().az_gl_context_ptr_is_texture)(self, texture) }
        /// Calls the `GlContextPtr::is_framebuffer` function.
        pub fn is_framebuffer(&self, framebuffer: u32)  -> u8 { (crate::dll::get_azul_dll().az_gl_context_ptr_is_framebuffer)(self, framebuffer) }
        /// Calls the `GlContextPtr::is_renderbuffer` function.
        pub fn is_renderbuffer(&self, renderbuffer: u32)  -> u8 { (crate::dll::get_azul_dll().az_gl_context_ptr_is_renderbuffer)(self, renderbuffer) }
        /// Calls the `GlContextPtr::check_frame_buffer_status` function.
        pub fn check_frame_buffer_status(&self, target: u32)  -> u32 { (crate::dll::get_azul_dll().az_gl_context_ptr_check_frame_buffer_status)(self, target) }
        /// Calls the `GlContextPtr::enable_vertex_attrib_array` function.
        pub fn enable_vertex_attrib_array(&self, index: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_enable_vertex_attrib_array)(self, index) }
        /// Calls the `GlContextPtr::disable_vertex_attrib_array` function.
        pub fn disable_vertex_attrib_array(&self, index: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_disable_vertex_attrib_array)(self, index) }
        /// Calls the `GlContextPtr::uniform_1f` function.
        pub fn uniform_1f(&self, location: i32, v0: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_1f)(self, location, v0) }
        /// Calls the `GlContextPtr::uniform_1fv` function.
        pub fn uniform_1fv(&self, location: i32, values: F32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_1fv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_1i` function.
        pub fn uniform_1i(&self, location: i32, v0: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_1i)(self, location, v0) }
        /// Calls the `GlContextPtr::uniform_1iv` function.
        pub fn uniform_1iv(&self, location: i32, values: I32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_1iv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_1ui` function.
        pub fn uniform_1ui(&self, location: i32, v0: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_1ui)(self, location, v0) }
        /// Calls the `GlContextPtr::uniform_2f` function.
        pub fn uniform_2f(&self, location: i32, v0: f32, v1: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_2f)(self, location, v0, v1) }
        /// Calls the `GlContextPtr::uniform_2fv` function.
        pub fn uniform_2fv(&self, location: i32, values: F32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_2fv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_2i` function.
        pub fn uniform_2i(&self, location: i32, v0: i32, v1: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_2i)(self, location, v0, v1) }
        /// Calls the `GlContextPtr::uniform_2iv` function.
        pub fn uniform_2iv(&self, location: i32, values: I32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_2iv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_2ui` function.
        pub fn uniform_2ui(&self, location: i32, v0: u32, v1: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_2ui)(self, location, v0, v1) }
        /// Calls the `GlContextPtr::uniform_3f` function.
        pub fn uniform_3f(&self, location: i32, v0: f32, v1: f32, v2: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_3f)(self, location, v0, v1, v2) }
        /// Calls the `GlContextPtr::uniform_3fv` function.
        pub fn uniform_3fv(&self, location: i32, values: F32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_3fv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_3i` function.
        pub fn uniform_3i(&self, location: i32, v0: i32, v1: i32, v2: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_3i)(self, location, v0, v1, v2) }
        /// Calls the `GlContextPtr::uniform_3iv` function.
        pub fn uniform_3iv(&self, location: i32, values: I32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_3iv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_3ui` function.
        pub fn uniform_3ui(&self, location: i32, v0: u32, v1: u32, v2: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_3ui)(self, location, v0, v1, v2) }
        /// Calls the `GlContextPtr::uniform_4f` function.
        pub fn uniform_4f(&self, location: i32, x: f32, y: f32, z: f32, w: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_4f)(self, location, x, y, z, w) }
        /// Calls the `GlContextPtr::uniform_4i` function.
        pub fn uniform_4i(&self, location: i32, x: i32, y: i32, z: i32, w: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_4i)(self, location, x, y, z, w) }
        /// Calls the `GlContextPtr::uniform_4iv` function.
        pub fn uniform_4iv(&self, location: i32, values: I32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_4iv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_4ui` function.
        pub fn uniform_4ui(&self, location: i32, x: u32, y: u32, z: u32, w: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_4ui)(self, location, x, y, z, w) }
        /// Calls the `GlContextPtr::uniform_4fv` function.
        pub fn uniform_4fv(&self, location: i32, values: F32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_4fv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_matrix_2fv` function.
        pub fn uniform_matrix_2fv(&self, location: i32, transpose: bool, value: F32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_matrix_2fv)(self, location, transpose, value) }
        /// Calls the `GlContextPtr::uniform_matrix_3fv` function.
        pub fn uniform_matrix_3fv(&self, location: i32, transpose: bool, value: F32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_matrix_3fv)(self, location, transpose, value) }
        /// Calls the `GlContextPtr::uniform_matrix_4fv` function.
        pub fn uniform_matrix_4fv(&self, location: i32, transpose: bool, value: F32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_matrix_4fv)(self, location, transpose, value) }
        /// Calls the `GlContextPtr::depth_mask` function.
        pub fn depth_mask(&self, flag: bool)  { (crate::dll::get_azul_dll().az_gl_context_ptr_depth_mask)(self, flag) }
        /// Calls the `GlContextPtr::depth_range` function.
        pub fn depth_range(&self, near: f64, far: f64)  { (crate::dll::get_azul_dll().az_gl_context_ptr_depth_range)(self, near, far) }
        /// Calls the `GlContextPtr::get_active_attrib` function.
        pub fn get_active_attrib(&self, program: u32, index: u32)  -> crate::gl::GetActiveAttribReturn { (crate::dll::get_azul_dll().az_gl_context_ptr_get_active_attrib)(self, program, index) }
        /// Calls the `GlContextPtr::get_active_uniform` function.
        pub fn get_active_uniform(&self, program: u32, index: u32)  -> crate::gl::GetActiveUniformReturn { (crate::dll::get_azul_dll().az_gl_context_ptr_get_active_uniform)(self, program, index) }
        /// Calls the `GlContextPtr::get_active_uniforms_iv` function.
        pub fn get_active_uniforms_iv(&self, program: u32, indices: GLuintVec, pname: u32)  -> crate::vec::GLintVec { (crate::dll::get_azul_dll().az_gl_context_ptr_get_active_uniforms_iv)(self, program, indices, pname) }
        /// Calls the `GlContextPtr::get_active_uniform_block_i` function.
        pub fn get_active_uniform_block_i(&self, program: u32, index: u32, pname: u32)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_active_uniform_block_i)(self, program, index, pname) }
        /// Calls the `GlContextPtr::get_active_uniform_block_iv` function.
        pub fn get_active_uniform_block_iv(&self, program: u32, index: u32, pname: u32)  -> crate::vec::GLintVec { (crate::dll::get_azul_dll().az_gl_context_ptr_get_active_uniform_block_iv)(self, program, index, pname) }
        /// Calls the `GlContextPtr::get_active_uniform_block_name` function.
        pub fn get_active_uniform_block_name(&self, program: u32, index: u32)  -> crate::str::String { (crate::dll::get_azul_dll().az_gl_context_ptr_get_active_uniform_block_name)(self, program, index) }
        /// Calls the `GlContextPtr::get_attrib_location` function.
        pub fn get_attrib_location(&self, program: u32, name: Refstr)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_attrib_location)(self, program, name) }
        /// Calls the `GlContextPtr::get_frag_data_location` function.
        pub fn get_frag_data_location(&self, program: u32, name: Refstr)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_frag_data_location)(self, program, name) }
        /// Calls the `GlContextPtr::get_uniform_location` function.
        pub fn get_uniform_location(&self, program: u32, name: Refstr)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_uniform_location)(self, program, name) }
        /// Calls the `GlContextPtr::get_program_info_log` function.
        pub fn get_program_info_log(&self, program: u32)  -> crate::str::String { (crate::dll::get_azul_dll().az_gl_context_ptr_get_program_info_log)(self, program) }
        /// Calls the `GlContextPtr::get_program_iv` function.
        pub fn get_program_iv(&self, program: u32, pname: u32, result: GLintVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_program_iv)(self, program, pname, result) }
        /// Calls the `GlContextPtr::get_program_binary` function.
        pub fn get_program_binary(&self, program: u32)  -> crate::gl::GetProgramBinaryReturn { (crate::dll::get_azul_dll().az_gl_context_ptr_get_program_binary)(self, program) }
        /// Calls the `GlContextPtr::program_binary` function.
        pub fn program_binary(&self, program: u32, format: u32, binary: U8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_program_binary)(self, program, format, binary) }
        /// Calls the `GlContextPtr::program_parameter_i` function.
        pub fn program_parameter_i(&self, program: u32, pname: u32, value: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_program_parameter_i)(self, program, pname, value) }
        /// Calls the `GlContextPtr::get_vertex_attrib_iv` function.
        pub fn get_vertex_attrib_iv(&self, index: u32, pname: u32, result: GLintVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_vertex_attrib_iv)(self, index, pname, result) }
        /// Calls the `GlContextPtr::get_vertex_attrib_fv` function.
        pub fn get_vertex_attrib_fv(&self, index: u32, pname: u32, result: GLfloatVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_vertex_attrib_fv)(self, index, pname, result) }
        /// Calls the `GlContextPtr::get_vertex_attrib_pointer_v` function.
        pub fn get_vertex_attrib_pointer_v(&self, index: u32, pname: u32)  -> isize { (crate::dll::get_azul_dll().az_gl_context_ptr_get_vertex_attrib_pointer_v)(self, index, pname) }
        /// Calls the `GlContextPtr::get_buffer_parameter_iv` function.
        pub fn get_buffer_parameter_iv(&self, target: u32, pname: u32)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_buffer_parameter_iv)(self, target, pname) }
        /// Calls the `GlContextPtr::get_shader_info_log` function.
        pub fn get_shader_info_log(&self, shader: u32)  -> crate::str::String { (crate::dll::get_azul_dll().az_gl_context_ptr_get_shader_info_log)(self, shader) }
        /// Calls the `GlContextPtr::get_string` function.
        pub fn get_string(&self, which: u32)  -> crate::str::String { (crate::dll::get_azul_dll().az_gl_context_ptr_get_string)(self, which) }
        /// Calls the `GlContextPtr::get_string_i` function.
        pub fn get_string_i(&self, which: u32, index: u32)  -> crate::str::String { (crate::dll::get_azul_dll().az_gl_context_ptr_get_string_i)(self, which, index) }
        /// Calls the `GlContextPtr::get_shader_iv` function.
        pub fn get_shader_iv(&self, shader: u32, pname: u32, result: GLintVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_shader_iv)(self, shader, pname, result) }
        /// Calls the `GlContextPtr::get_shader_precision_format` function.
        pub fn get_shader_precision_format(&self, shader_type: u32, precision_type: u32)  -> crate::gl::GlShaderPrecisionFormatReturn { (crate::dll::get_azul_dll().az_gl_context_ptr_get_shader_precision_format)(self, shader_type, precision_type) }
        /// Calls the `GlContextPtr::compile_shader` function.
        pub fn compile_shader(&self, shader: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_compile_shader)(self, shader) }
        /// Calls the `GlContextPtr::create_program` function.
        pub fn create_program(&self)  -> u32 { (crate::dll::get_azul_dll().az_gl_context_ptr_create_program)(self) }
        /// Calls the `GlContextPtr::delete_program` function.
        pub fn delete_program(&self, program: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_program)(self, program) }
        /// Calls the `GlContextPtr::create_shader` function.
        pub fn create_shader(&self, shader_type: u32)  -> u32 { (crate::dll::get_azul_dll().az_gl_context_ptr_create_shader)(self, shader_type) }
        /// Calls the `GlContextPtr::delete_shader` function.
        pub fn delete_shader(&self, shader: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_shader)(self, shader) }
        /// Calls the `GlContextPtr::detach_shader` function.
        pub fn detach_shader(&self, program: u32, shader: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_detach_shader)(self, program, shader) }
        /// Calls the `GlContextPtr::link_program` function.
        pub fn link_program(&self, program: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_link_program)(self, program) }
        /// Calls the `GlContextPtr::clear_color` function.
        pub fn clear_color(&self, r: f32, g: f32, b: f32, a: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_clear_color)(self, r, g, b, a) }
        /// Calls the `GlContextPtr::clear` function.
        pub fn clear(&self, buffer_mask: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_clear)(self, buffer_mask) }
        /// Calls the `GlContextPtr::clear_depth` function.
        pub fn clear_depth(&self, depth: f64)  { (crate::dll::get_azul_dll().az_gl_context_ptr_clear_depth)(self, depth) }
        /// Calls the `GlContextPtr::clear_stencil` function.
        pub fn clear_stencil(&self, s: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_clear_stencil)(self, s) }
        /// Calls the `GlContextPtr::flush` function.
        pub fn flush(&self)  { (crate::dll::get_azul_dll().az_gl_context_ptr_flush)(self) }
        /// Calls the `GlContextPtr::finish` function.
        pub fn finish(&self)  { (crate::dll::get_azul_dll().az_gl_context_ptr_finish)(self) }
        /// Calls the `GlContextPtr::get_error` function.
        pub fn get_error(&self)  -> u32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_error)(self) }
        /// Calls the `GlContextPtr::stencil_mask` function.
        pub fn stencil_mask(&self, mask: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_stencil_mask)(self, mask) }
        /// Calls the `GlContextPtr::stencil_mask_separate` function.
        pub fn stencil_mask_separate(&self, face: u32, mask: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_stencil_mask_separate)(self, face, mask) }
        /// Calls the `GlContextPtr::stencil_func` function.
        pub fn stencil_func(&self, func: u32, ref_: i32, mask: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_stencil_func)(self, func, ref_, mask) }
        /// Calls the `GlContextPtr::stencil_func_separate` function.
        pub fn stencil_func_separate(&self, face: u32, func: u32, ref_: i32, mask: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_stencil_func_separate)(self, face, func, ref_, mask) }
        /// Calls the `GlContextPtr::stencil_op` function.
        pub fn stencil_op(&self, sfail: u32, dpfail: u32, dppass: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_stencil_op)(self, sfail, dpfail, dppass) }
        /// Calls the `GlContextPtr::stencil_op_separate` function.
        pub fn stencil_op_separate(&self, face: u32, sfail: u32, dpfail: u32, dppass: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_stencil_op_separate)(self, face, sfail, dpfail, dppass) }
        /// Calls the `GlContextPtr::egl_image_target_texture2d_oes` function.
        pub fn egl_image_target_texture2d_oes(&self, target: u32, image: *const c_void)  { (crate::dll::get_azul_dll().az_gl_context_ptr_egl_image_target_texture2d_oes)(self, target, image) }
        /// Calls the `GlContextPtr::generate_mipmap` function.
        pub fn generate_mipmap(&self, target: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_generate_mipmap)(self, target) }
        /// Calls the `GlContextPtr::insert_event_marker_ext` function.
        pub fn insert_event_marker_ext(&self, message: Refstr)  { (crate::dll::get_azul_dll().az_gl_context_ptr_insert_event_marker_ext)(self, message) }
        /// Calls the `GlContextPtr::push_group_marker_ext` function.
        pub fn push_group_marker_ext(&self, message: Refstr)  { (crate::dll::get_azul_dll().az_gl_context_ptr_push_group_marker_ext)(self, message) }
        /// Calls the `GlContextPtr::pop_group_marker_ext` function.
        pub fn pop_group_marker_ext(&self)  { (crate::dll::get_azul_dll().az_gl_context_ptr_pop_group_marker_ext)(self) }
        /// Calls the `GlContextPtr::debug_message_insert_khr` function.
        pub fn debug_message_insert_khr(&self, source: u32, type_: u32, id: u32, severity: u32, message: Refstr)  { (crate::dll::get_azul_dll().az_gl_context_ptr_debug_message_insert_khr)(self, source, type_, id, severity, message) }
        /// Calls the `GlContextPtr::push_debug_group_khr` function.
        pub fn push_debug_group_khr(&self, source: u32, id: u32, message: Refstr)  { (crate::dll::get_azul_dll().az_gl_context_ptr_push_debug_group_khr)(self, source, id, message) }
        /// Calls the `GlContextPtr::pop_debug_group_khr` function.
        pub fn pop_debug_group_khr(&self)  { (crate::dll::get_azul_dll().az_gl_context_ptr_pop_debug_group_khr)(self) }
        /// Calls the `GlContextPtr::fence_sync` function.
        pub fn fence_sync(&self, condition: u32, flags: u32)  -> crate::gl::GLsyncPtr { (crate::dll::get_azul_dll().az_gl_context_ptr_fence_sync)(self, condition, flags) }
        /// Calls the `GlContextPtr::client_wait_sync` function.
        pub fn client_wait_sync(&self, sync: GLsyncPtr, flags: u32, timeout: u64)  { (crate::dll::get_azul_dll().az_gl_context_ptr_client_wait_sync)(self, sync, flags, timeout) }
        /// Calls the `GlContextPtr::wait_sync` function.
        pub fn wait_sync(&self, sync: GLsyncPtr, flags: u32, timeout: u64)  { (crate::dll::get_azul_dll().az_gl_context_ptr_wait_sync)(self, sync, flags, timeout) }
        /// Calls the `GlContextPtr::delete_sync` function.
        pub fn delete_sync(&self, sync: GLsyncPtr)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_sync)(self, sync) }
        /// Calls the `GlContextPtr::texture_range_apple` function.
        pub fn texture_range_apple(&self, target: u32, data: U8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_texture_range_apple)(self, target, data) }
        /// Calls the `GlContextPtr::gen_fences_apple` function.
        pub fn gen_fences_apple(&self, n: i32)  -> crate::vec::GLuintVec { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_fences_apple)(self, n) }
        /// Calls the `GlContextPtr::delete_fences_apple` function.
        pub fn delete_fences_apple(&self, fences: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_fences_apple)(self, fences) }
        /// Calls the `GlContextPtr::set_fence_apple` function.
        pub fn set_fence_apple(&self, fence: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_set_fence_apple)(self, fence) }
        /// Calls the `GlContextPtr::finish_fence_apple` function.
        pub fn finish_fence_apple(&self, fence: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_finish_fence_apple)(self, fence) }
        /// Calls the `GlContextPtr::test_fence_apple` function.
        pub fn test_fence_apple(&self, fence: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_test_fence_apple)(self, fence) }
        /// Calls the `GlContextPtr::test_object_apple` function.
        pub fn test_object_apple(&self, object: u32, name: u32)  -> u8 { (crate::dll::get_azul_dll().az_gl_context_ptr_test_object_apple)(self, object, name) }
        /// Calls the `GlContextPtr::finish_object_apple` function.
        pub fn finish_object_apple(&self, object: u32, name: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_finish_object_apple)(self, object, name) }
        /// Calls the `GlContextPtr::get_frag_data_index` function.
        pub fn get_frag_data_index(&self, program: u32, name: Refstr)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_frag_data_index)(self, program, name) }
        /// Calls the `GlContextPtr::blend_barrier_khr` function.
        pub fn blend_barrier_khr(&self)  { (crate::dll::get_azul_dll().az_gl_context_ptr_blend_barrier_khr)(self) }
        /// Calls the `GlContextPtr::bind_frag_data_location_indexed` function.
        pub fn bind_frag_data_location_indexed(&self, program: u32, color_number: u32, index: u32, name: Refstr)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_frag_data_location_indexed)(self, program, color_number, index, name) }
        /// Calls the `GlContextPtr::get_debug_messages` function.
        pub fn get_debug_messages(&self)  -> crate::vec::DebugMessageVec { (crate::dll::get_azul_dll().az_gl_context_ptr_get_debug_messages)(self) }
        /// Calls the `GlContextPtr::provoking_vertex_angle` function.
        pub fn provoking_vertex_angle(&self, mode: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_provoking_vertex_angle)(self, mode) }
        /// Calls the `GlContextPtr::gen_vertex_arrays_apple` function.
        pub fn gen_vertex_arrays_apple(&self, n: i32)  -> crate::vec::GLuintVec { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_vertex_arrays_apple)(self, n) }
        /// Calls the `GlContextPtr::bind_vertex_array_apple` function.
        pub fn bind_vertex_array_apple(&self, vao: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_vertex_array_apple)(self, vao) }
        /// Calls the `GlContextPtr::delete_vertex_arrays_apple` function.
        pub fn delete_vertex_arrays_apple(&self, vertex_arrays: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_vertex_arrays_apple)(self, vertex_arrays) }
        /// Calls the `GlContextPtr::copy_texture_chromium` function.
        pub fn copy_texture_chromium(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, internal_format: i32, dest_type: u32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_texture_chromium)(self, source_id, source_level, dest_target, dest_id, dest_level, internal_format, dest_type, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) }
        /// Calls the `GlContextPtr::copy_sub_texture_chromium` function.
        pub fn copy_sub_texture_chromium(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, x_offset: i32, y_offset: i32, x: i32, y: i32, width: i32, height: i32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_sub_texture_chromium)(self, source_id, source_level, dest_target, dest_id, dest_level, x_offset, y_offset, x, y, width, height, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) }
        /// Calls the `GlContextPtr::egl_image_target_renderbuffer_storage_oes` function.
        pub fn egl_image_target_renderbuffer_storage_oes(&self, target: u32, image: *const c_void)  { (crate::dll::get_azul_dll().az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes)(self, target, image) }
        /// Calls the `GlContextPtr::copy_texture_3d_angle` function.
        pub fn copy_texture_3d_angle(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, internal_format: i32, dest_type: u32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_texture_3d_angle)(self, source_id, source_level, dest_target, dest_id, dest_level, internal_format, dest_type, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) }
        /// Calls the `GlContextPtr::copy_sub_texture_3d_angle` function.
        pub fn copy_sub_texture_3d_angle(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, x_offset: i32, y_offset: i32, z_offset: i32, x: i32, y: i32, z: i32, width: i32, height: i32, depth: i32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_sub_texture_3d_angle)(self, source_id, source_level, dest_target, dest_id, dest_level, x_offset, y_offset, z_offset, x, y, z, width, height, depth, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) }
    }

    impl Clone for GlContextPtr { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_gl_context_ptr_deep_copy)(self) } }
    impl Drop for GlContextPtr { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gl_context_ptr_delete)(self); } }


    /// `Texture` struct
    #[doc(inline)] pub use crate::dll::AzTexture as Texture;

    impl Drop for Texture { fn drop(&mut self) { (crate::dll::get_azul_dll().az_texture_delete)(self); } }


    /// `TextureFlags` struct
    #[doc(inline)] pub use crate::dll::AzTextureFlags as TextureFlags;

    impl TextureFlags {
        /// Default texture flags (not opaque, not a video texture)
        pub fn default() -> Self { (crate::dll::get_azul_dll().az_texture_flags_default)() }
    }

    impl Clone for TextureFlags { fn clone(&self) -> Self { *self } }
    impl Copy for TextureFlags { }
