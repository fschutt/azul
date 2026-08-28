#[allow(unused_imports)]
pub use super::*;
#[cfg(test)]
mod audit_tests {
    use super::*;

    // AUDIT: FFI slice/str accessors must return an empty slice (not form a
    // slice over a null/dangling ptr, which is UB) when handed a null or
    // zero-length descriptor from C.
    #[test]
    fn refstr_null_and_empty_is_empty_str() {
        let null = Refstr {
            ptr: core::ptr::null(),
            len: 0,
        };
        assert_eq!(null.as_str(), "");

        // null ptr but nonzero len (garbage from FFI) must also not deref.
        let null_lenful = Refstr {
            ptr: core::ptr::null(),
            len: 5,
        };
        assert_eq!(null_lenful.as_str(), "");

        let s = "hello";
        let good: Refstr = s.into();
        assert_eq!(good.as_str(), "hello");
    }

    #[test]
    fn refstr_vec_ref_null_is_empty_slice() {
        let null = RefstrVecRef {
            ptr: core::ptr::null(),
            len: 3,
        };
        assert!(null.as_slice().is_empty());
    }

    #[test]
    fn u8_vec_ref_null_is_empty_slice() {
        let null = U8VecRef {
            ptr: core::ptr::null(),
            len: 8,
        };
        assert!(null.as_slice().is_empty());

        let data = [1u8, 2, 3];
        let good: U8VecRef = (&data[..]).into();
        assert_eq!(good.as_slice(), &[1, 2, 3]);
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact f32 compares are the point: these assert bit-exact round-trips / lossy-cast values
mod autotest_generated {
    use super::*;

    // ---------------------------------------------------------------------
    // Test scaffolding: a "null" GL context.
    //
    // Every field of `GenericGlContext` is a `*mut c_void` function pointer
    // (780 of them, no other field types), and every gl-context-loader method
    // null-checks its pointer and returns a default (0 / empty Vec / "") rather
    // than calling through. So an all-zero context is a SAFE no-op GL driver:
    // it lets us drive azul's whole wrapper surface off-GPU and assert how the
    // wrappers behave when the driver hands back degenerate values -- which is
    // exactly the "broken driver" path `is_gl_usable()` exists for.
    // ---------------------------------------------------------------------
    fn null_gl() -> Rc<GenericGlContext> {
        // SAFETY: `GenericGlContext` is `repr(C)` and every field is a raw
        // pointer, for which the all-zero bit pattern (null) is valid.
        Rc::new(unsafe { core::mem::zeroed::<GenericGlContext>() })
    }

    fn null_ctx() -> GlContextPtr {
        GlContextPtr::new(RendererType::Software, null_gl())
    }

    fn test_texture(texture_id: GLuint, width: u32, height: u32) -> Texture {
        Texture::create(
            texture_id,
            TextureFlags {
                is_opaque: false,
                is_video_texture: false,
            },
            PhysicalSizeU32 { width, height },
            ColorU {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
            null_ctx(),
            RawImageFormat::RGBA8,
        )
    }

    const ALL_ATTRIB_TYPES: [VertexAttributeType; 5] = [
        VertexAttributeType::Float,
        VertexAttributeType::Double,
        VertexAttributeType::UnsignedByte,
        VertexAttributeType::UnsignedShort,
        VertexAttributeType::UnsignedInt,
    ];

    const ALL_INDEX_FORMATS: [IndexBufferFormat; 6] = [
        IndexBufferFormat::Points,
        IndexBufferFormat::Lines,
        IndexBufferFormat::LineStrip,
        IndexBufferFormat::Triangles,
        IndexBufferFormat::TriangleStrip,
        IndexBufferFormat::TriangleFan,
    ];

    // =====================================================================
    // Refstr / *VecRef / *VecRefMut -- FFI slice+str accessors
    // (parsers: malformed / huge / boundary / unicode)
    // =====================================================================

    #[test]
    fn refstr_roundtrips_unicode_multibyte_and_combining_marks() {
        // Non-ASCII, emoji, and combining marks must survive byte-exactly:
        // `as_str` rebuilds the str via `from_utf8_unchecked` over the raw bytes.
        for s in [
            "\u{1F600}",
            "日本語のテキスト",
            "e\u{0301}\u{0328}combining",
            "\u{0}nul\u{0}embedded\u{0}",
            "\u{FFFD}\u{200B}zero-width",
        ] {
            let r: Refstr = s.into();
            assert_eq!(r.as_str(), s);
            assert_eq!(r.as_str().len(), s.len());
        }
    }

    #[test]
    fn refstr_len_zero_over_nonempty_buffer_is_empty_not_dangling() {
        // len == 0 must short-circuit even when ptr is a perfectly valid buffer.
        let backing = "not empty";
        let r = Refstr {
            ptr: backing.as_ptr(),
            len: 0,
        };
        assert_eq!(r.as_str(), "");
    }

    #[test]
    fn refstr_huge_input_does_not_hang() {
        // 1 MB single-token string: as_str is O(1) (no scan), so this must be instant.
        let huge = "x".repeat(1_000_000);
        let r: Refstr = huge.as_str().into();
        assert_eq!(r.as_str().len(), 1_000_000);
        assert_eq!(r.as_str(), huge.as_str());
    }

    #[test]
    fn refstr_debug_on_null_does_not_panic() {
        let null = Refstr {
            ptr: core::ptr::null(),
            len: usize::MAX,
        };
        // Debug goes through as_str(); a null guard failure here would be UB, not a panic.
        assert_eq!(alloc::format!("{null:?}"), "\"\"");
    }

    #[test]
    fn refstr_clone_preserves_ptr_and_len() {
        let s = "clone me";
        let r: Refstr = s.into();
        let c = r.clone();
        assert_eq!(c.as_str(), s);
        assert!(core::ptr::eq(c.ptr, r.ptr));
        assert_eq!(c.len, r.len);
    }

    #[test]
    fn every_vec_ref_with_null_ptr_and_nonzero_len_is_empty() {
        // The FFI boundary can hand us a null ptr with a garbage (nonzero) len.
        // Forming a slice over null is UB, so every accessor must return empty.
        const GARBAGE_LEN: usize = usize::MAX;

        assert!(RefstrVecRef {
            ptr: core::ptr::null(),
            len: GARBAGE_LEN
        }
        .as_slice()
        .is_empty());
        assert!(GLuintVecRef {
            ptr: core::ptr::null(),
            len: GARBAGE_LEN
        }
        .as_slice()
        .is_empty());
        assert!(GLenumVecRef {
            ptr: core::ptr::null(),
            len: GARBAGE_LEN
        }
        .as_slice()
        .is_empty());
        assert!(U8VecRef {
            ptr: core::ptr::null(),
            len: GARBAGE_LEN
        }
        .as_slice()
        .is_empty());
        assert!(F32VecRef {
            ptr: core::ptr::null(),
            len: GARBAGE_LEN
        }
        .as_slice()
        .is_empty());
        assert!(I32VecRef {
            ptr: core::ptr::null(),
            len: GARBAGE_LEN
        }
        .as_slice()
        .is_empty());

        // ...and the same for the mutable variants, through BOTH accessors.
        let mut m64 = GLint64VecRefMut {
            ptr: core::ptr::null_mut(),
            len: GARBAGE_LEN,
        };
        assert!(m64.as_slice().is_empty());
        assert!(m64.as_mut_slice().is_empty());

        let mut mf = GLfloatVecRefMut {
            ptr: core::ptr::null_mut(),
            len: GARBAGE_LEN,
        };
        assert!(mf.as_slice().is_empty());
        assert!(mf.as_mut_slice().is_empty());

        let mut mi = GLintVecRefMut {
            ptr: core::ptr::null_mut(),
            len: GARBAGE_LEN,
        };
        assert!(mi.as_slice().is_empty());
        assert!(mi.as_mut_slice().is_empty());

        let mut mb = GLbooleanVecRefMut {
            ptr: core::ptr::null_mut(),
            len: GARBAGE_LEN,
        };
        assert!(mb.as_slice().is_empty());
        assert!(mb.as_mut_slice().is_empty());

        let mut mu8 = U8VecRefMut {
            ptr: core::ptr::null_mut(),
            len: GARBAGE_LEN,
        };
        assert!(mu8.as_slice().is_empty());
        assert!(mu8.as_mut_slice().is_empty());
    }

    #[test]
    fn vec_refs_roundtrip_from_slice() {
        let u: [GLuint; 3] = [0, 1, u32::MAX];
        assert_eq!(GLuintVecRef::from(&u[..]).as_slice(), &u[..]);

        let e: [GLenum; 2] = [gl::TRIANGLES, u32::MAX];
        assert_eq!(GLenumVecRef::from(&e[..]).as_slice(), &e[..]);

        let b: [u8; 4] = [0, 127, 128, 255];
        assert_eq!(U8VecRef::from(&b[..]).as_slice(), &b[..]);

        let i: [i32; 3] = [i32::MIN, 0, i32::MAX];
        assert_eq!(I32VecRef::from(&i[..]).as_slice(), &i[..]);

        // Empty (but non-null) slices must also come back empty.
        let empty: [u8; 0] = [];
        assert!(U8VecRef::from(&empty[..]).as_slice().is_empty());
    }

    #[test]
    fn f32_vec_ref_preserves_nan_inf_and_subnormals_bit_exactly() {
        // These are pointer casts, not conversions: NaN payloads and -0.0 must
        // survive bit-for-bit (a NaN-normalizing copy would be a real bug).
        let vals: [f32; 6] = [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -0.0,
            f32::MIN_POSITIVE / 2.0, // subnormal
            f32::MAX,
        ];
        let r = F32VecRef::from(&vals[..]);
        let got = r.as_slice();
        assert_eq!(got.len(), vals.len());
        for (g, v) in got.iter().zip(vals.iter()) {
            assert_eq!(g.to_bits(), v.to_bits());
        }
        assert!(got[0].is_nan());
        assert!(got[3].is_sign_negative());
    }

    #[test]
    fn glint64_vec_ref_mut_handles_boundary_values() {
        let mut vals: [GLint64; 4] = [i64::MIN, -1, 0, i64::MAX];
        let mut r = GLint64VecRefMut::from(&mut vals[..]);
        assert_eq!(r.as_slice(), &[i64::MIN, -1, 0, i64::MAX]);

        // Writes through as_mut_slice must land in the caller's buffer.
        r.as_mut_slice()[0] = i64::MAX;
        r.as_mut_slice()[3] = i64::MIN;
        assert_eq!(vals, [i64::MAX, -1, 0, i64::MIN]);
    }

    #[test]
    fn mut_vec_refs_write_through_to_the_caller_buffer() {
        let mut floats: [GLfloat; 2] = [0.0, 0.0];
        GLfloatVecRefMut::from(&mut floats[..]).as_mut_slice()[1] = f32::NAN;
        assert!(floats[1].is_nan());

        let mut ints: [GLint; 2] = [0, 0];
        GLintVecRefMut::from(&mut ints[..]).as_mut_slice()[0] = i32::MIN;
        assert_eq!(ints[0], i32::MIN);

        let mut bools: [GLboolean; 2] = [0, 0];
        GLbooleanVecRefMut::from(&mut bools[..]).as_mut_slice()[1] = 255;
        assert_eq!(bools[1], 255);

        let mut bytes: [u8; 3] = [1, 2, 3];
        U8VecRefMut::from(&mut bytes[..]).as_mut_slice()[2] = 0;
        assert_eq!(bytes, [1, 2, 0]);
    }

    #[test]
    fn u8_vec_ref_ord_eq_hash_agree_with_the_underlying_slice() {
        use core::hash::{BuildHasher, Hasher};

        use alloc::collections::BTreeSet;

        let a = [1u8, 2, 3];
        let b = [1u8, 2, 4];
        let ra = U8VecRef::from(&a[..]);
        let rb = U8VecRef::from(&b[..]);

        assert_eq!(ra, U8VecRef::from(&a[..]));
        assert!(ra < rb);
        assert_eq!(ra.cmp(&rb), a[..].cmp(&b[..]));

        // A null ref must compare equal to an empty one (both are the empty slice).
        let null = U8VecRef {
            ptr: core::ptr::null(),
            len: 99,
        };
        let empty: [u8; 0] = [];
        assert_eq!(null, U8VecRef::from(&empty[..]));

        // Hash must be consistent with Eq (equal values -> equal hashes).
        fn hash_of(v: &U8VecRef) -> u64 {
            core::hash::BuildHasherDefault::<TestHasher>::default().hash_one(v)
        }
        #[derive(Default)]
        struct TestHasher(u64);
        impl Hasher for TestHasher {
            fn finish(&self) -> u64 {
                self.0
            }
            fn write(&mut self, bytes: &[u8]) {
                for b in bytes {
                    self.0 = self.0.wrapping_mul(31).wrapping_add(u64::from(*b));
                }
            }
        }
        assert_eq!(hash_of(&ra), hash_of(&U8VecRef::from(&a[..])));
        assert_eq!(hash_of(&null), hash_of(&U8VecRef::from(&empty[..])));

        // Ord must be a total order good enough for a BTreeSet.
        let mut set = BTreeSet::new();
        set.insert(ra.clone());
        set.insert(U8VecRef::from(&a[..]));
        set.insert(rb);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn refstr_vec_ref_roundtrips_and_maps_back_to_strs() {
        let strs = ["", "a", "\u{1F600}"];
        let refstrs: Vec<Refstr> = strs.iter().map(|s| Refstr::from(*s)).collect();
        let vec_ref = RefstrVecRef::from(&refstrs[..]);
        let got: Vec<&str> = vec_ref.as_slice().iter().map(Refstr::as_str).collect();
        assert_eq!(got, strs);
    }

    // =====================================================================
    // GLsyncPtr (constructor + round-trip)
    // =====================================================================

    #[test]
    fn glsync_ptr_roundtrips_null_and_nonnull() {
        let null = GLsyncPtr::new(core::ptr::null());
        assert!(null.clone().get().is_null());
        assert_eq!(alloc::format!("{null:?}"), "0x0");

        // A non-null (never dereferenced) sentinel must round-trip identically.
        let sentinel = usize::MAX as *const c_void;
        let p = GLsyncPtr::new(sentinel);
        assert_eq!(p.clone().get() as usize, usize::MAX);
        assert!(p.run_destructor);
    }

    // =====================================================================
    // shader_with_glsl_version (parser: the `#version` line swap)
    // =====================================================================

    #[cfg(feature = "std")]
    #[test]
    fn shader_with_glsl_version_replaces_only_the_first_line() {
        let src = b"#version 150\nbody line 1\nbody line 2";
        let out = shader_with_glsl_version(src, b"#version 300 es\n");
        assert_eq!(out, b"#version 300 es\nbody line 1\nbody line 2".to_vec());
    }

    #[cfg(feature = "std")]
    #[test]
    fn shader_with_glsl_version_empty_src_yields_just_the_version_line() {
        // No newline in src -> body_start = 0 (the map_or default), so the whole
        // (empty) src is kept and only the version line is prepended.
        assert_eq!(
            shader_with_glsl_version(b"", b"#version 150\n"),
            b"#version 150\n".to_vec()
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn shader_with_glsl_version_src_without_newline_is_kept_whole() {
        // A src with NO newline has no first line to strip: body_start stays 0,
        // so the version line is prepended and nothing is lost.
        let out = shader_with_glsl_version(b"void main(){}", b"#version 150\n");
        assert_eq!(out, b"#version 150\nvoid main(){}".to_vec());
    }

    #[cfg(feature = "std")]
    #[test]
    fn shader_with_glsl_version_leading_newline_drops_only_that_newline() {
        let out = shader_with_glsl_version(b"\nrest", b"V\n");
        assert_eq!(out, b"V\nrest".to_vec());
    }

    #[cfg(feature = "std")]
    #[test]
    fn shader_with_glsl_version_empty_version_line_still_strips_first_line() {
        let out = shader_with_glsl_version(b"#version 150\nbody", b"");
        assert_eq!(out, b"body".to_vec());

        // Both empty -> empty, no panic.
        assert!(shader_with_glsl_version(b"", b"").is_empty());
    }

    #[cfg(feature = "std")]
    #[test]
    fn shader_with_glsl_version_is_byte_exact_on_invalid_utf8_and_nul_bytes() {
        // Shader sources are bytes, not str: invalid UTF-8 / NULs must pass
        // through untouched (this is fed straight to glShaderSource).
        let src = b"#version 150\n\xFF\xFE\x00\x80body";
        let out = shader_with_glsl_version(src, b"#version 100\n");
        assert_eq!(out, b"#version 100\n\xFF\xFE\x00\x80body".to_vec());

        // A src that is ONLY invalid bytes with no newline is kept whole.
        let out2 = shader_with_glsl_version(&[0xFF, 0xFE, 0x00], b"V\n");
        assert_eq!(out2, vec![b'V', b'\n', 0xFF, 0xFE, 0x00]);
    }

    #[cfg(feature = "std")]
    #[test]
    fn shader_with_glsl_version_handles_a_1mb_source_without_hanging() {
        let mut src = b"#version 150\n".to_vec();
        src.resize(src.len() + 1_000_000, b'x');
        let out = shader_with_glsl_version(&src, b"#version 300 es\n");
        assert_eq!(out.len(), b"#version 300 es\n".len() + 1_000_000);
        assert!(out.starts_with(b"#version 300 es\n"));
        assert!(out.ends_with(b"xxxx"));
    }

    // =====================================================================
    // glsl_version_candidates (invariants the version probe relies on)
    // =====================================================================

    #[test]
    fn glsl_version_candidates_are_wellformed_and_distinct() {
        for gl_type in [GlType::Gl, GlType::Gles] {
            let candidates = glsl_version_candidates(gl_type);
            assert!(
                !candidates.is_empty(),
                "{gl_type:?} must have at least one candidate or the probe can never succeed"
            );
            for c in candidates {
                // GlContextPtr::new parses these back out with
                // `trim().trim_start_matches("#version ")`, which requires
                // valid UTF-8, the exact prefix, and a trailing newline.
                let s = core::str::from_utf8(c).expect("candidate must be valid UTF-8");
                assert!(s.starts_with("#version "), "{s:?}");
                assert!(s.ends_with('\n'), "{s:?}");
                let parsed = s.trim().trim_start_matches("#version ");
                assert!(!parsed.is_empty(), "{s:?} must parse to a nonempty version");
            }
            // No duplicates: a repeated candidate would just re-probe a known failure.
            for (i, a) in candidates.iter().enumerate() {
                for b in candidates.iter().skip(i + 1) {
                    assert_ne!(a, b, "duplicate candidate in {gl_type:?}");
                }
            }
        }

        // The GL and GLES lists must be disjoint (a GLES driver rejects `#version 150`).
        for g in glsl_version_candidates(GlType::Gl) {
            assert!(!glsl_version_candidates(GlType::Gles).contains(g));
        }

        // Documented examples from the API docs: "150" (GL) and "300 es" (GLES).
        let first_gl = core::str::from_utf8(glsl_version_candidates(GlType::Gl)[0]).unwrap();
        assert_eq!(first_gl.trim().trim_start_matches("#version "), "150");
        let first_es = core::str::from_utf8(glsl_version_candidates(GlType::Gles)[0]).unwrap();
        assert_eq!(first_es.trim().trim_start_matches("#version "), "300 es");
    }

    #[test]
    fn gl_type_from_context_gl_type_is_total() {
        assert_eq!(GlType::from(GlContextGlType::Gl), GlType::Gl);
        assert_eq!(GlType::from(GlContextGlType::GlEs), GlType::Gles);
    }

    // =====================================================================
    // GlContextPtr -- construction + the documented "unusable driver" fallback
    // =====================================================================

    #[test]
    fn gl_context_ptr_software_is_never_usable_and_has_no_shaders() {
        // Documented: "Always `false` for a Software context (which never
        // compiles these shaders)".
        let ctx = GlContextPtr::new(RendererType::Software, null_gl());
        assert!(!ctx.is_gl_usable());
        assert_eq!(ctx.get_svg_shader(), 0);
        assert_eq!(ctx.get_brush_shader(), 0);
        assert_eq!(ctx.get_fxaa_shader(), 0);
        assert_eq!(ctx.get_usable_glsl_version().as_str(), "");
        assert_eq!(ctx.renderer_type, RendererType::Software);
    }

    #[test]
    fn gl_context_ptr_hardware_with_broken_driver_falls_back_instead_of_panicking() {
        // THE contract `is_gl_usable()` exists for: context creation "succeeds"
        // but the driver compiles nothing. The probe must try every candidate
        // version, fail them all, and leave every program id at 0 -- so the
        // caller can fall back to CPU rendering -- rather than panicking or
        // handing out a garbage program id.
        let ctx = GlContextPtr::new(RendererType::Hardware, null_gl());
        assert!(
            !ctx.is_gl_usable(),
            "a driver that compiles nothing must report is_gl_usable() == false"
        );
        assert_eq!(ctx.get_svg_shader(), 0);
        assert_eq!(ctx.get_brush_shader(), 0);
        assert_eq!(ctx.get_fxaa_shader(), 0);
        assert_eq!(
            ctx.get_usable_glsl_version().as_str(),
            "",
            "glsl_version must be empty when the context is unusable"
        );
    }

    #[test]
    fn gl_context_ptr_get_type_defaults_to_desktop_gl_on_an_empty_version_string() {
        // get_type() sniffs the GL_VERSION string for "OpenGL ES"; a driver that
        // returns nothing must deterministically fall out as desktop Gl.
        assert_eq!(null_ctx().get_type(), GlType::Gl);
    }

    #[test]
    fn gl_context_ptr_clone_is_eq_but_distinct_contexts_are_not() {
        // Eq/Ord are identity-based (`as_usize` = the inner Rc address), so a
        // clone must compare equal and two independently created contexts must not.
        let a = null_ctx();
        let clone = a.clone();
        assert_eq!(a, clone);
        assert_eq!(a.cmp(&clone), core::cmp::Ordering::Equal);
        assert_eq!(a.partial_cmp(&clone), Some(core::cmp::Ordering::Equal));

        let b = null_ctx();
        assert_ne!(a, b);
        // Ord must be antisymmetric and consistent with PartialOrd.
        assert_eq!(a.cmp(&b), a.partial_cmp(&b).unwrap());
        assert_eq!(a.cmp(&b).reverse(), b.cmp(&a));

        // The clone must point at the same underlying GL context.
        assert!(Rc::ptr_eq(a.get(), clone.get()));
        assert!(!Rc::ptr_eq(a.get(), b.get()));
    }

    #[test]
    fn gl_context_ptr_gen_family_returns_empty_for_every_n_including_negatives() {
        // A driver that generates nothing returns an EMPTY list. Callers index
        // into this ([0] / .get(0).unwrap()), so the wrappers must at least not
        // panic on the way out -- and must not choke on a negative/extreme n.
        let ctx = null_ctx();
        for n in [0, 1, -1, i32::MIN, i32::MAX] {
            assert!(ctx.gen_buffers(n).as_slice().is_empty(), "gen_buffers({n})");
            assert!(
                ctx.gen_textures(n).as_slice().is_empty(),
                "gen_textures({n})"
            );
            assert!(
                ctx.gen_framebuffers(n).as_slice().is_empty(),
                "gen_framebuffers({n})"
            );
            assert!(
                ctx.gen_renderbuffers(n).as_slice().is_empty(),
                "gen_renderbuffers({n})"
            );
            assert!(
                ctx.gen_vertex_arrays(n).as_slice().is_empty(),
                "gen_vertex_arrays({n})"
            );
            assert!(ctx.gen_queries(n).as_slice().is_empty(), "gen_queries({n})");
        }
    }

    #[test]
    fn gl_context_ptr_integer_extremes_do_not_panic() {
        // Offsets/sizes/limits at the boundary of their integer types. These are
        // passed straight through to GL, so the wrapper must not do arithmetic
        // that overflows on the way.
        let ctx = null_ctx();
        let data = [0u8; 4];
        let void_ptr = || GlVoidPtrConst {
            ptr: data.as_ptr().cast(),
            run_destructor: false,
        };

        for offset in [0isize, -1, isize::MIN, isize::MAX] {
            for size in [0isize, -1, isize::MIN, isize::MAX] {
                ctx.buffer_sub_data_untyped(gl::ARRAY_BUFFER, offset, size, void_ptr());
                let _ = ctx.map_buffer_range(gl::ARRAY_BUFFER, offset, size, 0);
            }
        }
        ctx.buffer_data_untyped(gl::ARRAY_BUFFER, isize::MIN, void_ptr(), gl::STATIC_DRAW);

        // usize offset at the extreme (reinterpreted as a byte offset pointer).
        for offset in [0usize, 1, usize::MAX] {
            ctx.tex_sub_image_2d_pbo(
                gl::TEXTURE_2D,
                0,
                0,
                0,
                1,
                1,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                offset,
            );
        }

        ctx.pixel_store_i(gl::PACK_ALIGNMENT, i32::MIN);
        ctx.pixel_store_i(gl::PACK_ALIGNMENT, i32::MAX);
        let _ = ctx.unmap_buffer(gl::ARRAY_BUFFER);
    }

    #[test]
    fn gl_context_ptr_float_extremes_do_not_panic() {
        // NaN / inf / subnormal floats must pass through the f32 wrappers untouched.
        let ctx = null_ctx();
        for v in [
            0.0,
            1.0,
            -1.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MIN,
            f32::MAX,
        ] {
            ctx.sample_coverage(v, false);
            ctx.sample_coverage(v, true);
            ctx.polygon_offset(v, v);
        }
    }

    #[test]
    fn gl_context_ptr_shader_source_handles_empty_unicode_and_embedded_nuls() {
        // shader_source NUL-terminates each string itself; an already-embedded
        // NUL or multibyte UTF-8 must not panic on the way through.
        let ctx = null_ctx();
        let strings: StringVec = vec![
            AzString::from(String::new()),
            AzString::from("\u{1F600} emoji".to_string()),
            AzString::from("has\u{0}nul".to_string()),
            AzString::from("x".repeat(100_000)),
        ]
        .into();
        ctx.shader_source(0, strings);

        // An empty list of sources must also be fine.
        ctx.shader_source(u32::MAX, Vec::<AzString>::new().into());
    }

    #[test]
    fn gl_context_ptr_read_pixels_sizes_the_buffer_from_the_dimensions() {
        // NOTE: only SAFE (small, positive) dimensions are exercised here.
        // Negative dimensions are a live hazard in this path -- see the report:
        // gl-context-loader's read_pixels does
        //   `vec![0; width as usize * height as usize * bit_depth]`
        // BEFORE its null-pointer check, so a negative width sign-extends to
        // ~usize::MAX and the multiply overflows (debug) / requests an
        // exabyte-scale allocation (release). Deliberately not provoked.
        let ctx = null_ctx();
        let px = ctx.read_pixels(0, 0, 2, 3, gl::RGBA, gl::UNSIGNED_BYTE);
        assert_eq!(
            px.len(),
            2 * 3 * 4,
            "RGBA/UNSIGNED_BYTE = 4 bytes per pixel"
        );

        // A zero-size read is the degenerate-but-safe case.
        assert_eq!(
            ctx.read_pixels(0, 0, 0, 0, gl::RGBA, gl::UNSIGNED_BYTE)
                .len(),
            0
        );
    }

    #[test]
    fn gl_context_ptr_read_pixels_into_buffer_accepts_null_and_undersized_targets() {
        // The destination is caller-owned here (no allocation from the dims), so
        // a null / empty / undersized target must simply no-op.
        let ctx = null_ctx();
        ctx.read_pixels_into_buffer(
            0,
            0,
            4,
            4,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            U8VecRefMut {
                ptr: core::ptr::null_mut(),
                len: 64,
            },
        );

        let mut small = [0u8; 1];
        ctx.read_pixels_into_buffer(
            0,
            0,
            4,
            4,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            (&mut small[..]).into(),
        );
        assert_eq!(small, [0u8; 1]);
    }

    #[test]
    fn gl_context_ptr_uniform_getters_accept_null_result_buffers() {
        let ctx = null_ctx();
        ctx.get_uniform_iv(
            0,
            -1,
            GLintVecRefMut {
                ptr: core::ptr::null_mut(),
                len: 16,
            },
        );
        ctx.get_uniform_fv(
            0,
            i32::MIN,
            GLfloatVecRefMut {
                ptr: core::ptr::null_mut(),
                len: 16,
            },
        );

        // ...and real buffers, at extreme locations.
        let mut ints = [0i32; 2];
        ctx.get_uniform_iv(u32::MAX, i32::MAX, (&mut ints[..]).into());
        let mut floats = [0f32; 2];
        ctx.get_uniform_fv(u32::MAX, i32::MAX, (&mut floats[..]).into());
    }

    #[test]
    fn gl_context_ptr_get_uniform_indices_handles_empty_and_null_name_lists() {
        let ctx = null_ctx();
        let empty: &[Refstr] = &[];
        assert!(ctx
            .get_uniform_indices(0, empty.into())
            .as_slice()
            .is_empty());
        assert!(ctx
            .get_uniform_indices(
                0,
                RefstrVecRef {
                    ptr: core::ptr::null(),
                    len: 4,
                },
            )
            .as_slice()
            .is_empty());
    }

    #[test]
    fn gl_context_ptr_delete_family_accepts_empty_and_null_id_lists() {
        // Deleting nothing must be a no-op, not an out-of-bounds read.
        let ctx = null_ctx();
        let empty: &[GLuint] = &[];
        ctx.delete_buffers(empty.into());
        ctx.delete_textures(empty.into());
        ctx.delete_framebuffers(empty.into());
        ctx.delete_renderbuffers(empty.into());
        ctx.delete_vertex_arrays(empty.into());
        ctx.delete_queries(empty.into());

        let null = || GLuintVecRef {
            ptr: core::ptr::null(),
            len: 7,
        };
        ctx.delete_buffers(null());
        ctx.delete_textures(null());
        ctx.delete_framebuffers(null());
        ctx.delete_renderbuffers(null());
        ctx.delete_vertex_arrays(null());
        ctx.delete_queries(null());

        // A real (bogus) id list, incl. 0 and u32::MAX, must also be fine.
        let ids: &[GLuint] = &[0, 1, u32::MAX];
        ctx.delete_buffers(ids.into());
        ctx.delete_textures(ids.into());
    }

    #[test]
    fn gl_context_ptr_draw_buffers_accepts_an_empty_list() {
        let ctx = null_ctx();
        let empty: &[GLenum] = &[];
        ctx.draw_buffers(empty.into());
        ctx.draw_buffers(GLenumVecRef {
            ptr: core::ptr::null(),
            len: 3,
        });
    }

    // =====================================================================
    // VertexAttributeType / VertexAttribute / VertexLayout (numeric + invariants)
    // =====================================================================

    #[test]
    fn vertex_attribute_type_mem_size_matches_the_rust_type_it_names() {
        assert_eq!(
            VertexAttributeType::Float.get_mem_size(),
            core::mem::size_of::<f32>()
        );
        assert_eq!(
            VertexAttributeType::Double.get_mem_size(),
            core::mem::size_of::<f64>()
        );
        assert_eq!(
            VertexAttributeType::UnsignedByte.get_mem_size(),
            core::mem::size_of::<u8>()
        );
        assert_eq!(
            VertexAttributeType::UnsignedShort.get_mem_size(),
            core::mem::size_of::<u16>()
        );
        assert_eq!(
            VertexAttributeType::UnsignedInt.get_mem_size(),
            core::mem::size_of::<u32>()
        );

        // Every size must be nonzero -- a 0 would make get_stride() collapse to 0
        // and silently produce a degenerate vertex layout.
        for t in ALL_ATTRIB_TYPES {
            assert!(t.get_mem_size() > 0, "{t:?}");
        }
    }

    #[test]
    fn vertex_attribute_type_gl_ids_are_distinct_and_nonzero() {
        for (i, a) in ALL_ATTRIB_TYPES.iter().enumerate() {
            assert_ne!(a.get_gl_id(), 0, "{a:?} maps to the GL 'no type' id 0");
            for b in ALL_ATTRIB_TYPES.iter().skip(i + 1) {
                assert_ne!(
                    a.get_gl_id(),
                    b.get_gl_id(),
                    "{a:?} and {b:?} share a GL id -- one of them would upload as the wrong type"
                );
            }
        }
        assert_eq!(VertexAttributeType::Float.get_gl_id(), gl::FLOAT);
        assert_eq!(
            VertexAttributeType::UnsignedByte.get_gl_id(),
            gl::UNSIGNED_BYTE
        );
    }

    #[test]
    fn vertex_attribute_get_stride_at_zero_and_at_the_overflow_boundary() {
        let attr = |ty, item_count| VertexAttribute {
            va_name: AzString::from_const_str("vAttrXY"),
            layout_location: OptionUsize::None,
            attribute_type: ty,
            item_count,
        };

        // Zero items -> zero stride.
        for t in ALL_ATTRIB_TYPES {
            assert_eq!(attr(t, 0).get_stride(), 0, "{t:?}");
        }

        // Exact multiplication for representative counts.
        assert_eq!(attr(VertexAttributeType::Float, 2).get_stride(), 8);
        assert_eq!(attr(VertexAttributeType::Double, 4).get_stride(), 32);
        assert_eq!(attr(VertexAttributeType::UnsignedByte, 3).get_stride(), 3);

        // The LARGEST item_count that does not overflow `mem_size * item_count`.
        // (NOTE: get_stride() is a plain `*`, so an item_count above this bound
        // overflow-panics in debug / wraps in release -- see the report.)
        for t in ALL_ATTRIB_TYPES {
            let max_items = usize::MAX / t.get_mem_size();
            assert_eq!(
                attr(t, max_items).get_stride(),
                max_items * t.get_mem_size(),
                "{t:?} at the overflow boundary"
            );
        }
    }

    #[test]
    fn vertex_layout_stride_is_the_sum_of_its_fields() {
        let attr = |name: &str, ty, item_count| VertexAttribute {
            va_name: AzString::from(name.to_string()),
            layout_location: OptionUsize::None,
            attribute_type: ty,
            item_count,
        };

        // Empty layout: zero fields, zero total stride.
        let empty = VertexLayout {
            fields: VertexAttributeVec::from_const_slice(&[]),
        };
        assert_eq!(
            empty
                .fields
                .iter()
                .map(VertexAttribute::get_stride)
                .sum::<usize>(),
            0
        );

        // vec2<f32> + vec4<u8> = 8 + 4 = 12 bytes per vertex.
        let layout = VertexLayout {
            fields: vec![
                attr("vAttrXY", VertexAttributeType::Float, 2),
                attr("vColor", VertexAttributeType::UnsignedByte, 4),
            ]
            .into(),
        };
        let total: usize = layout.fields.iter().map(VertexAttribute::get_stride).sum();
        assert_eq!(total, 12);

        // Equality/Hash are structural, so an identical layout compares equal.
        assert_eq!(layout, layout.clone());
    }

    #[test]
    fn index_buffer_format_gl_ids_are_distinct() {
        // A collision here would silently draw the wrong primitive.
        for (i, a) in ALL_INDEX_FORMATS.iter().enumerate() {
            for b in ALL_INDEX_FORMATS.iter().skip(i + 1) {
                assert_ne!(a.get_gl_id(), b.get_gl_id(), "{a:?} vs {b:?}");
            }
        }
        assert_eq!(IndexBufferFormat::Points.get_gl_id(), gl::POINTS);
        assert_eq!(
            IndexBufferFormat::TriangleStrip.get_gl_id(),
            gl::TRIANGLE_STRIP
        );
    }

    // =====================================================================
    // Uniform / UniformType (NaN semantics feed GlShader::draw's dedupe)
    // =====================================================================

    #[test]
    fn uniform_type_nan_never_equals_itself_so_draw_always_reuploads_it() {
        // GlShader::draw skips re-setting a uniform when
        // `current_uniforms[i] != Some(uniform.uniform_type)`. With a NaN payload
        // that comparison is ALWAYS unequal, so a NaN uniform is re-uploaded on
        // every buffer -- correct (never stale), just not deduped. Pin it.
        let nan = UniformType::Float(f32::NAN);
        assert_ne!(nan, UniformType::Float(f32::NAN));
        assert_ne!(Some(nan), Some(nan));

        let nan_vec = UniformType::FloatVec4([f32::NAN, 0.0, 0.0, 0.0]);
        assert_ne!(nan_vec, nan_vec);

        // The flip side: +0.0 and -0.0 compare EQUAL, so a -0.0 -> +0.0 change is
        // deduped away and never re-uploaded. Harmless for a uniform, but pinned
        // so a change in semantics is visible.
        assert_eq!(UniformType::Float(0.0), UniformType::Float(-0.0));

        // Integer uniforms have no such wrinkle: they dedupe exactly.
        assert_eq!(UniformType::Int(i32::MIN), UniformType::Int(i32::MIN));
        assert_ne!(UniformType::Int(0), UniformType::UnsignedInt(0));
    }

    #[test]
    fn uniform_type_matrix_transpose_flag_is_part_of_its_identity() {
        let m = [0.0f32; 4];
        assert_ne!(
            UniformType::Matrix2 {
                transpose: false,
                matrix: m
            },
            UniformType::Matrix2 {
                transpose: true,
                matrix: m
            },
        );
        // Same payload, different arity -> different uniforms.
        assert_ne!(
            UniformType::FloatVec2([1.0, 2.0]),
            UniformType::IntVec2([1, 2])
        );
    }

    #[test]
    fn uniform_create_preserves_empty_unicode_and_huge_names() {
        let huge = "n".repeat(100_000);
        for name in ["", "u_color", "\u{1F600}", "e\u{0301}", huge.as_str()] {
            let u = Uniform::create(name.to_string(), UniformType::Int(0));
            assert_eq!(u.uniform_name.as_str(), name);
        }
    }

    // =====================================================================
    // GlShader -- the no-shader-compiler error path
    // =====================================================================

    #[test]
    fn gl_shader_new_reports_no_shader_compiler_instead_of_panicking() {
        // A driver that reports no shader compiler (GL_SHADER_COMPILER == FALSE)
        // must produce a clean Err -- for ANY source, including empty, garbage,
        // unicode and very large ones.
        let ctx = null_ctx();
        let huge = "x".repeat(200_000);
        for (vert, frag) in [
            ("", ""),
            ("   \t\n  ", "\n\n"),
            ("not glsl at all ;;;{{{", "\u{1F600}"),
            ("void main(){}", "void main(){}"),
            (huge.as_str(), huge.as_str()),
        ] {
            let err = GlShader::new(&ctx, vert, frag).unwrap_err();
            assert_eq!(err, GlShaderCreateError::NoShaderCompiler);
        }
    }

    #[test]
    fn shader_error_types_format_without_panicking_on_unicode_and_extremes() {
        let vert = VertexShaderCompileError {
            error_id: i32::MIN,
            info_log: AzString::from("\u{1F600} log".to_string()),
        };
        let frag = FragmentShaderCompileError {
            error_id: i32::MAX,
            info_log: AzString::from(String::new()),
        };
        let link = GlShaderLinkError {
            error_id: -1,
            info_log: AzString::from("multi\nline\0log".to_string()),
        };

        assert!(alloc::format!("{vert}").contains("-2147483648"));
        assert!(alloc::format!("{vert}").contains("\u{1F600}"));
        assert!(alloc::format!("{frag}").contains("2147483647"));

        let compile = GlShaderCompileError::Vertex(vert);
        assert!(alloc::format!("{compile}").contains("Failed to compile vertex shader"));
        // Debug delegates to Display -- must agree, and must not recurse.
        assert_eq!(alloc::format!("{compile:?}"), alloc::format!("{compile}"));

        let frag_err = GlShaderCompileError::Fragment(frag);
        assert!(alloc::format!("{frag_err}").contains("Failed to compile fragment shader"));

        let create = GlShaderCreateError::Link(link);
        assert!(alloc::format!("{create}").contains("Shader linking error"));
        assert_eq!(alloc::format!("{create:?}"), alloc::format!("{create}"));
        assert!(alloc::format!("{}", GlShaderCreateError::NoShaderCompiler)
            .contains("doesn't include a shader compiler"));
    }

    // =====================================================================
    // Texture -- getters, refcounting, and the degenerate-driver paths
    // =====================================================================

    #[test]
    fn texture_descriptor_mirrors_the_texture_including_extreme_sizes() {
        let tex = test_texture(42, 640, 480);
        let d = tex.get_descriptor();
        assert_eq!(d.width, 640);
        assert_eq!(d.height, 480);
        assert_eq!(d.format, RawImageFormat::RGBA8);
        assert_eq!(d.offset, 0);
        assert!(!d.flags.is_opaque);
        assert!(!d.flags.allow_mipmaps, "textures map 1:1, never mipmapped");

        // Zero-size and u32::MAX-size must not panic or wrap in the descriptor.
        let zero = test_texture(1, 0, 0);
        assert_eq!(zero.get_descriptor().width, 0);
        assert_eq!(zero.get_descriptor().height, 0);

        let huge = test_texture(1, u32::MAX, u32::MAX);
        assert_eq!(huge.get_descriptor().width, u32::MAX as usize);
        assert_eq!(huge.get_descriptor().height, u32::MAX as usize);

        // is_opaque must be carried through from the flags, not hardcoded.
        let opaque = Texture::create(
            7,
            TextureFlags {
                is_opaque: true,
                is_video_texture: true,
            },
            PhysicalSizeU32 {
                width: 2,
                height: 2,
            },
            ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
            null_ctx(),
            RawImageFormat::BGRA8,
        );
        assert!(opaque.get_descriptor().flags.is_opaque);
        assert_eq!(opaque.get_descriptor().format, RawImageFormat::BGRA8);
    }

    #[test]
    fn texture_clone_and_drop_share_one_refcount_without_double_freeing() {
        // Texture is refcounted through a raw Box<AtomicUsize>; a miscount here
        // is a double-free (or a leaked GL texture). Exercise fan-out + drop.
        let tex = test_texture(9, 4, 4);
        let clones: Vec<Texture> = (0..16).map(|_| tex.clone()).collect();
        for c in &clones {
            assert_eq!(c.texture_id, 9);
            assert_eq!(c.size, tex.size);
            assert_eq!(c.format, tex.format);
            assert!(c.run_destructor);
        }
        // Equality/Hash are by texture id.
        assert_eq!(clones[0], tex);
        assert_eq!(clones[0], clones[1]);
        assert_ne!(tex, test_texture(10, 4, 4));

        drop(clones); // 16 drops...
        drop(tex); // ...then the last one actually frees.
    }

    #[test]
    fn texture_display_and_debug_render_id_and_size() {
        let tex = test_texture(3, 16, 32);
        assert_eq!(alloc::format!("{tex}"), "Texture { id: 3, 16x32 }");
        // Debug delegates to Display.
        assert_eq!(alloc::format!("{tex:?}"), alloc::format!("{tex}"));
    }

    #[test]
    fn texture_paint_stroke_is_a_noop_when_gl_is_unusable() {
        // Documented: "No-op if the GL context is unusable". With no brush shader
        // (program id 0) this must bail out before touching GL -- including for
        // NaN / infinite / negative radii and coordinates, which must not produce
        // a huge dab loop. (`!(radius > 0.0)` is written that way precisely so it
        // also rejects NaN.)
        let mut tex = test_texture(5, 8, 8);
        assert_eq!(tex.gl_context.get_brush_shader(), 0);

        for radius in [1.0, 0.0, -1.0, f32::NAN, f32::INFINITY, f32::MAX] {
            let mut brush = Brush::new(
                ColorU {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                radius,
            );
            brush.hardness = f32::NAN;
            brush.flow = f32::INFINITY;
            brush.spacing = 0.0; // would be a divide-by-zero step without the .max(0.01)
            tex.paint_stroke(f32::NAN, f32::NEG_INFINITY, f32::MAX, -0.0, brush);
            tex.paint_dot(f32::NAN, f32::NAN, brush);
        }

        // A zero-sized texture is the other early-out (tw/th <= 0).
        let mut zero = test_texture(6, 0, 0);
        zero.paint_dot(
            0.0,
            0.0,
            Brush::new(
                ColorU {
                    r: 1,
                    g: 1,
                    b: 1,
                    a: 1,
                },
                4.0,
            ),
        );
    }

    #[test]
    fn texture_copy_to_raw_image_returns_a_null_image_on_every_degenerate_input() {
        // The `w <= 0 || h <= 0` guard is load-bearing: size is a u32 but is cast
        // to i32 for glReadPixels, so a width >= 2^31 goes NEGATIVE. Without the
        // guard that reaches gl-context-loader's
        //   `vec![0; width as usize * height as usize * bit_depth]`
        // which sign-extends the negative width to ~usize::MAX -> overflow panic
        // (debug) / exabyte allocation (release). Prove the guard holds.
        let is_null_image = |img: &RawImage| img.width == 0 && img.height == 0;

        // texture id 0 -> null image
        assert!(is_null_image(&test_texture(0, 16, 16).copy_to_raw_image()));
        // zero size -> null image
        assert!(is_null_image(&test_texture(1, 0, 0).copy_to_raw_image()));
        assert!(is_null_image(&test_texture(1, 16, 0).copy_to_raw_image()));
        assert!(is_null_image(&test_texture(1, 0, 16).copy_to_raw_image()));

        // u32::MAX / 2^31 both cast to a NEGATIVE i32 and MUST be caught by the guard.
        assert!(is_null_image(
            &test_texture(1, u32::MAX, u32::MAX).copy_to_raw_image()
        ));
        assert!(is_null_image(
            &test_texture(1, 1 << 31, 1 << 31).copy_to_raw_image()
        ));

        // Valid id + valid size, but the driver hands back no framebuffer
        // (`fbo == 0`) -> still a clean null image, no unwrap panic.
        assert!(is_null_image(&test_texture(1, 4, 4).copy_to_raw_image()));
    }

    #[test]
    #[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
    fn texture_clear_panics_when_the_driver_allocates_no_framebuffer() {
        // DOCUMENTED panic: "Panics if no framebuffer/depthbuffer was allocated
        // (the GL object lists are empty)." A driver that generates nothing makes
        // gen_framebuffers(1) return an EMPTY list, so `.get(0).unwrap()` panics.
        // Pinned as documented behavior -- note that the sibling paths
        // (paint_stroke / copy_to_raw_image) degrade gracefully instead.
        test_texture(1, 4, 4).clear();
    }

    // =====================================================================
    // VertexArrayObject / VertexBuffer
    // =====================================================================

    #[test]
    fn vertex_array_object_clone_and_drop_share_one_refcount() {
        let layout = VertexLayout {
            fields: VertexAttributeVec::from_const_slice(&[]),
        };
        let vao = VertexArrayObject::new(layout, 77, null_ctx());
        assert_eq!(vao.vao_id, 77);
        assert!(vao.run_destructor);

        let clones: Vec<VertexArrayObject> = (0..8).map(|_| vao.clone()).collect();
        assert!(clones.iter().all(|c| c.vao_id == 77));
        assert_eq!(clones[0], vao);
        drop(clones);
        drop(vao);
    }

    #[test]
    fn vertex_buffer_new_raw_keeps_its_fields_and_refcounts_its_clones() {
        let vao = VertexArrayObject::new(
            VertexLayout {
                fields: VertexAttributeVec::from_const_slice(&[]),
            },
            1,
            null_ctx(),
        );
        let vb = VertexBuffer::new_raw(11, 300, vao, 22, 40, IndexBufferFormat::TriangleStrip);

        assert_eq!(vb.vertex_buffer_id, 11);
        assert_eq!(vb.vertex_buffer_len, 300);
        assert_eq!(vb.index_buffer_id, 22);
        assert_eq!(vb.index_buffer_len, 40);
        assert_eq!(vb.index_buffer_format, IndexBufferFormat::TriangleStrip);
        assert_eq!(
            alloc::format!("{vb}"),
            "VertexBuffer { buffer: 11 (length: 300) }"
        );

        // Eq/Hash are by vertex_buffer_id.
        let clones: Vec<VertexBuffer> = (0..8).map(|_| vb.clone()).collect();
        assert_eq!(clones[0], vb);
        drop(clones);
        drop(vb);

        // A zero-length buffer is degenerate but legal.
        let empty_vao = VertexArrayObject::new(
            VertexLayout {
                fields: VertexAttributeVec::from_const_slice(&[]),
            },
            0,
            null_ctx(),
        );
        let empty = VertexBuffer::new_raw(0, 0, empty_vao, 0, 0, IndexBufferFormat::Points);
        assert_eq!(empty.vertex_buffer_len, 0);
    }

    #[allow(dead_code)] // the field only exists to give the vertex a realistic size/layout
    struct TestVertex {
        _xy: [f32; 2],
    }

    impl VertexLayoutDescription for TestVertex {
        fn get_description() -> VertexLayout {
            VertexLayout {
                fields: vec![VertexAttribute {
                    va_name: AzString::from_const_str("vAttrXY"),
                    layout_location: OptionUsize::None,
                    attribute_type: VertexAttributeType::Float,
                    item_count: 2,
                }]
                .into(),
            }
        }
    }

    #[test]
    #[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
    fn vertex_buffer_new_panics_when_the_driver_allocates_no_vao() {
        // DOCUMENTED panic: "Panics if the GL driver failed to create the
        // vertex-array/buffer objects (the returned id lists are empty)."
        let verts = [TestVertex { _xy: [0.0, 0.0] }];
        let _ = VertexBuffer::new(
            null_ctx(),
            0,
            &verts[..],
            &[0u32],
            IndexBufferFormat::Triangles,
        );
    }

    // =====================================================================
    // The process-global GL texture table.
    //
    // ACTIVE_GL_TEXTURES is a `static mut` with no lock (GL is single-threaded
    // by design), and cargo test runs #[test]s on parallel threads. So ALL of
    // its coverage lives in this ONE test -- splitting it up would race the
    // table against itself. No other test in this crate touches it.
    // =====================================================================

    #[test]
    fn gl_texture_cache_insert_lookup_and_eviction_lifecycle() {
        let doc = DocumentId {
            namespace_id: crate::resources::IdNamespace(7),
            id: 1,
        };
        let other_doc = DocumentId {
            namespace_id: crate::resources::IdNamespace(7),
            id: 2,
        };

        // --- Empty / uninitialized table: every accessor must degrade, not panic.
        gl_textures_clear_opengl_cache();
        let stale = ExternalImageId { inner: u64::MAX };
        assert!(get_opengl_texture(&stale).is_none());
        assert!(
            remove_single_texture_from_active_gl_textures(&doc, &Epoch::from(0), &stale).is_none()
        );
        gl_textures_remove_epochs_from_pipeline(&doc, Epoch::from(0));
        gl_textures_remove_active_pipeline(&doc);
        gl_textures_clear_opengl_cache(); // idempotent

        // --- Insert into an UNINITIALIZED table.
        // NOTE: the doc comment on insert_into_active_gl_textures claims it
        // "Panics if the global active-GL-texture table has not been
        // initialized" -- it does not; it initializes the table itself. The doc
        // is stale (see the report). Pin the real behavior.
        let id5 = insert_into_active_gl_textures(doc, Epoch::from(5), test_texture(11, 4, 8));
        let id7 = insert_into_active_gl_textures(doc, Epoch::from(7), test_texture(22, 16, 32));
        assert_ne!(id5, id7, "each insert must mint a unique ExternalImageId");

        // --- Lookup: id -> (texture id, (w, h) as f32).
        assert_eq!(get_opengl_texture(&id5), Some((11, (4.0, 8.0))));
        assert_eq!(get_opengl_texture(&id7), Some((22, (16.0, 32.0))));
        assert!(get_opengl_texture(&stale).is_none());

        // A u32::MAX-sized texture goes through a lossy u32 -> f32 cast:
        // u32::MAX (2^32 - 1) has no exact f32, so it rounds UP to 2^32.
        let id_huge =
            insert_into_active_gl_textures(doc, Epoch::from(5), test_texture(33, u32::MAX, 1));
        assert_eq!(
            get_opengl_texture(&id_huge),
            Some((33, (4_294_967_296.0, 1.0)))
        );

        // --- Epoch eviction is STRICTLY older-than: epoch 5 goes, epoch 7 stays.
        gl_textures_remove_epochs_from_pipeline(&doc, Epoch::from(7));
        assert!(
            get_opengl_texture(&id5).is_none(),
            "epoch 5 < 7 must be evicted"
        );
        assert!(
            get_opengl_texture(&id_huge).is_none(),
            "epoch 5 < 7 must be evicted"
        );
        assert_eq!(
            get_opengl_texture(&id7),
            Some((22, (16.0, 32.0))),
            "epoch 7 is NOT < 7, so it must survive"
        );

        // Evicting for an unknown document must be a no-op, not a panic.
        gl_textures_remove_epochs_from_pipeline(&other_doc, Epoch::from(u32::MAX));
        assert_eq!(get_opengl_texture(&id7), Some((22, (16.0, 32.0))));

        // --- remove_single: Some(()) when the (doc, epoch) path exists...
        assert_eq!(
            remove_single_texture_from_active_gl_textures(&doc, &Epoch::from(7), &id7),
            Some(())
        );
        assert!(get_opengl_texture(&id7).is_none());
        // ...including a second removal of an already-gone image (the path is
        // still there, so it reports Some(()) rather than None)...
        assert_eq!(
            remove_single_texture_from_active_gl_textures(&doc, &Epoch::from(7), &id7),
            Some(())
        );
        // ...but None when the document or the epoch is unknown.
        assert!(
            remove_single_texture_from_active_gl_textures(&other_doc, &Epoch::from(7), &id7)
                .is_none()
        );
        assert!(
            remove_single_texture_from_active_gl_textures(&doc, &Epoch::from(u32::MAX), &id7)
                .is_none()
        );

        // --- remove_active_pipeline drops the whole document.
        let id_a = insert_into_active_gl_textures(doc, Epoch::from(1), test_texture(44, 2, 2));
        let id_b =
            insert_into_active_gl_textures(other_doc, Epoch::from(1), test_texture(55, 2, 2));
        assert!(get_opengl_texture(&id_a).is_some());
        gl_textures_remove_active_pipeline(&doc);
        assert!(get_opengl_texture(&id_a).is_none(), "doc was removed");
        assert!(
            get_opengl_texture(&id_b).is_some(),
            "other_doc must be untouched"
        );

        // --- clear drops everything.
        gl_textures_clear_opengl_cache();
        assert!(get_opengl_texture(&id_b).is_none());

        // Leave the table clean for anything that runs after us.
        gl_textures_clear_opengl_cache();
    }
}
