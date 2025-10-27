//! EGL context management for X11 and OpenGL function loading.

use super::dlopen::{Egl, Xlib};
use super::defines::*;
use crate::desktop::shell2::common::WindowError;
use std::rc::Rc;
use std::mem;
use gl_context_loader::GenericGlContext;
use std::ffi::{c_void, CString};

/// Holds the EGL display, context, and surface for an X11 window.
#[derive(Default)]
pub struct GlContext {
    pub egl: Rc<Egl>,
    pub egl_display: EGLDisplay,
    pub egl_context: EGLContext,
    pub egl_surface: EGLSurface,
}

/// Wrapper to get access to the GL function pointers
pub struct GlFunctions {
    _opengl_lib_handle: Option<super::dlopen::Library>,
    pub functions: Rc<GenericGlContext>,
}

impl std::fmt::Debug for GlFunctions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GlFunctions {{ ... }}")
    }
}

impl GlContext {
    /// Creates a new EGL context for the given X11 display and window.
    pub fn new(
        xlib: &Rc<Xlib>,
        egl: &Rc<Egl>,
        display: *mut Display,
        window: Window,
    ) -> Result<Self, WindowError> {

        let egl_display = unsafe { (egl.eglGetDisplay)(display as EGLNativeDisplayType) };
        if egl_display.is_null() {
            return Err(WindowError::PlatformError("eglGetDisplay failed".into()));
        }

        let mut major = 0;
        let mut minor = 0;
        if unsafe { (egl.eglInitialize)(egl_display, &mut major, &mut minor) } == 0 {
            return Err(WindowError::PlatformError("eglInitialize failed".into()));
        }

        if unsafe { (egl.eglBindAPI)(EGL_OPENGL_API) } == 0 {
            return Err(WindowError::ContextCreationFailed);
        }

        let config_attribs = [
            EGL_RED_SIZE, 8,
            EGL_GREEN_SIZE, 8,
            EGL_BLUE_SIZE, 8,
            EGL_ALPHA_SIZE, 8,
            EGL_DEPTH_SIZE, 24,
            EGL_STENCIL_SIZE, 8,
            EGL_SURFACE_TYPE, EGL_WINDOW_BIT,
            EGL_RENDERABLE_TYPE, EGL_OPENGL_BIT,
            EGL_NONE,
        ];

        let mut config = std::ptr::null_mut();
        let mut num_config = 0;
        if unsafe { (egl.eglChooseConfig)(egl_display, config_attribs.as_ptr(), &mut config, 1, &mut num_config) } == 0 || num_config == 0 {
            return Err(WindowError::ContextCreationFailed);
        }

        let egl_surface = unsafe { (egl.eglCreateWindowSurface)(egl_display, config, window as EGLNativeWindowType, std::ptr::null()) };
        if egl_surface.is_null() {
            return Err(WindowError::PlatformError("eglCreateWindowSurface failed".into()));
        }

        let context_attribs = [
            EGL_CONTEXT_MAJOR_VERSION, 3,
            EGL_CONTEXT_MINOR_VERSION, 2,
            EGL_CONTEXT_OPENGL_PROFILE_MASK, EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT,
            EGL_NONE
        ];
        let egl_context = unsafe { (egl.eglCreateContext)(egl_display, config, std::ptr::null_mut(), context_attribs.as_ptr()) };
        if egl_context.is_null() {
            return Err(WindowError::ContextCreationFailed);
        }

        Ok(Self {
            egl: egl.clone(),
            egl_display,
            egl_context,
            egl_surface,
        })
    }

    /// Makes the OpenGL context current on the calling thread.
    pub fn make_current(&self) {
        unsafe {
            (self.egl.eglMakeCurrent)(self.egl_display, self.egl_surface, self.egl_surface, self.egl_context);
        }
    }

    /// Swaps the front and back buffers.
    pub fn swap_buffers(&self) -> Result<(), WindowError> {
        if unsafe { (self.egl.eglSwapBuffers)(self.egl_display, self.egl_surface) } == 0 {
            Err(WindowError::PlatformError("eglSwapBuffers failed".into()))
        } else {
            Ok(())
        }
    }
}

impl GlFunctions {
    /// Allocates and loads the OpenGL function pointers via eglGetProcAddress.
    pub fn initialize(egl: &Rc<Egl>) -> Result<Self, String> {
        let opengl_lib = super::dlopen::Library::load("libGL.so.1").ok();
        let context: GenericGlContext = unsafe { mem::zeroed() };
        let mut funcs = Self {
            _opengl_lib_handle: opengl_lib,
            functions: Rc::new(context),
        };
        funcs.load(egl);
        Ok(funcs)
    }

    /// Loads the OpenGL function pointers. This must be called after the context is made current.
    pub fn load(&mut self, egl: &Rc<Egl>) {
        fn get_func(egl: &Egl, s: &str, lib: &Option<super::dlopen::Library>) -> *mut c_void {
            let symbol_name = CString::new(s).unwrap();
            let addr = (egl.eglGetProcAddress)(symbol_name.as_ptr());
            if !addr.is_null() {
                return addr as *mut _;
            }
            if let Some(lib) = lib {
                if let Some(addr) = lib.get(s) {
                    return addr as *mut _;
                }
            }
            std::ptr::null_mut()
        }

        macro_rules! load { ($($name:ident),*) => { GenericGlContext { $($name: get_func(egl, stringify!($name), &self._opengl_lib_handle),)* } }; }
        self.functions = Rc::new(load! {
            glAccum, glActiveTexture, glAlphaFunc, glAreTexturesResident, glArrayElement,
            glAttachShader, glBegin, glBeginConditionalRender, glBeginQuery, glBeginTransformFeedback,
            glBindAttribLocation, glBindBuffer, glBindBufferBase, glBindBufferRange,
            glBindFragDataLocation, glBindFragDataLocationIndexed, glBindFramebuffer,
            glBindRenderbuffer, glBindSampler, glBindTexture, glBindVertexArray,
            glBindVertexArrayAPPLE, glBitmap, glBlendBarrierKHR, glBlendColor, glBlendEquation,
            glBlendEquationSeparate, glBlendFunc, glBlendFuncSeparate, glBlitFramebuffer,
            glBufferData, glBufferStorage, glBufferSubData, glCallList, glCallLists,
            glCheckFramebufferStatus, glClampColor, glClear, glClearAccum, glClearBufferfi,
            glClearBufferfv, glClearBufferiv, glClearBufferuiv, glClearColor, glClearDepth,
            glClearIndex, glClearStencil, glClientActiveTexture, glClientWaitSync, glClipPlane,
            glColor3b, glColor3bv, glColor3d, glColor3dv, glColor3f, glColor3fv, glColor3i,
            glColor3iv, glColor3s, glColor3sv, glColor3ub, glColor3ubv, glColor3ui, glColor3uiv,
            glColor3us, glColor3usv, glColor4b, glColor4bv, glColor4d, glColor4dv, glColor4f,
            glColor4fv, glColor4i, glColor4iv, glColor4s, glColor4sv, glColor4ub, glColor4ubv,
            glColor4ui, glColor4uiv, glColor4us, glColor4usv, glColorMask, glColorMaski,
            glColorMaterial, glColorP3ui, glColorP3uiv, glColorP4ui, glColorP4uiv,
            glColorPointer, glCompileShader, glCompressedTexImage1D, glCompressedTexImage2D,
            glCompressedTexImage3D, glCompressedTexSubImage1D, glCompressedTexSubImage2D,
            glCompressedTexSubImage3D, glCopyBufferSubData, glCopyImageSubData, glCopyPixels,
            glCopyTexImage1D, glCopyTexImage2D, glCopyTexSubImage1D, glCopyTexSubImage2D,
            glCopyTexSubImage3D, glCreateProgram, glCreateShader, glCullFace,
            glDebugMessageCallback, glDebugMessageCallbackKHR, glDebugMessageControl,
            glDebugMessageControlKHR, glDebugMessageInsert, glDebugMessageInsertKHR,
            glDeleteBuffers, glDeleteFencesAPPLE, glDeleteFramebuffers, glDeleteLists,
            glDeleteProgram, glDeleteQueries, glDeleteRenderbuffers, glDeleteSamplers,
            glDeleteShader, glDeleteSync, glDeleteTextures, glDeleteVertexArrays,
            glDeleteVertexArraysAPPLE, glDepthFunc, glDepthMask, glDepthRange, glDetachShader,
            glDisable, glDisableClientState, glDisableVertexAttribArray, glDisablei, glEnd,
            glEndConditionalRender, glEndList, glEndQuery, glEndTransformFeedback,
            glEvalCoord1d, glEvalCoord1dv, glEvalCoord1f, glEvalCoord1fv, glEvalCoord2d,
            glEvalCoord2dv, glEvalCoord2f, glEvalCoord2fv, glEvalMesh1, glEvalMesh2,
            glEvalPoint1, glEvalPoint2, glFeedbackBuffer, glFenceSync, glFinish,
            glFinishFenceAPPLE, glFinishObjectAPPLE, glFlush, glFlushMappedBufferRange,
            glFogCoordPointer, glFogCoordd, glFogCoorddv, glFogCoordf, glFogCoordfv, glFogf,
            glFogfv, glFogi, glFogiv, glFramebufferRenderbuffer, glFramebufferTexture,
            glFramebufferTexture1D, glFramebufferTexture2D, glFramebufferTexture3D,
            glFramebufferTextureLayer, glFrontFace, glFrustum, glGenBuffers, glGenFencesAPPLE,
            glGenFramebuffers, glGenLists, glGenQueries, glGenRenderbuffers, glGenSamplers,
            glGenTextures, glGenVertexArrays, glGenVertexArraysAPPLE, glGenerateMipmap,
            glGetActiveAttrib, glGetActiveUniform, glGetActiveUniformBlockName,
            glGetActiveUniformBlockiv, glGetActiveUniformName, glGetActiveUniformsiv,
            glGetAttachedShaders, glGetAttribLocation, glGetBooleani_v, glGetBooleanv,
            glGetBufferParameteri64v, glGetBufferParameteriv, glGetBufferPointerv,
            glGetBufferSubData, glGetClipPlane, glGetCompressedTexImage, glGetDebugMessageLog,
            glGetDebugMessageLogKHR, glGetDoublev, glGetError, glGetFloatv, glGetFragDataIndex,
            glGetFragDataLocation, glGetFramebufferAttachmentParameteriv, glGetInteger64i_v,
            glGetInteger64v, glGetIntegeri_v, glGetIntegerv, glGetLightfv, glGetLightiv,
            glGetMapdv, glGetMapfv, glGetMapiv, glGetMaterialfv, glGetMaterialiv,
            glGetMultisamplefv, glGetObjectLabel, glGetObjectLabelKHR, glGetObjectPtrLabel,
            glGetObjectPtrLabelKHR, glGetPixelMapfv, glGetPixelMapuiv, glGetPixelMapusv,
            glGetPointerv, glGetPointervKHR, glGetPolygonStipple, glGetProgramBinary,
            glGetProgramInfoLog, glGetProgramiv, glGetQueryObjecti64v, glGetQueryObjectiv,
            glGetQueryObjectui64v, glGetQueryObjectuiv, glGetQueryiv,
            glGetRenderbufferParameteriv, glGetSamplerParameterIiv, glGetSamplerParameterIuiv,
            glGetSamplerParameterfv, glGetSamplerParameteriv, glGetShaderInfoLog,
            glGetShaderSource, glGetShaderiv, glGetString, glGetStringi, glGetSynciv,
            glGetTexEnvfv, glGetTexEnviv, glGetTexGendv, glGetTexGenfv, glGetTexGeniv,
            glGetTexImage, glGetTexLevelParameterfv, glGetTexLevelParameteriv,
            glGetTexParameterIiv, glGetTexParameterIuiv, glGetTexParameterPointervAPPLE,
            glGetTexParameterfv, glGetTexParameteriv, glGetTransformFeedbackVarying,
            glGetUniformBlockIndex, glGetUniformIndices, glGetUniformLocation, glGetUniformfv,
            glGetUniformiv, glGetUniformuiv, glGetVertexAttribIiv, glGetVertexAttribIuiv,
            glGetVertexAttribPointerv, glGetVertexAttribdv, glGetVertexAttribfv,
            glGetVertexAttribiv, glHint, glIndexMask, glIndexPointer, glIndexd, glIndexdv,
            glIndexf, glIndexfv, glIndexi, glIndexiv, glIndexs, glIndexsv, glIndexub, glIndexubv,
            glInitNames, glInsertEventMarkerEXT, glInterleavedArrays, glInvalidateBufferData,
            glInvalidateBufferSubData, glInvalidateFramebuffer, glInvalidateSubFramebuffer,
            glInvalidateTexImage, glInvalidateTexSubImage, glIsBuffer, glIsEnabled,
            glIsEnabledi, glIsFenceAPPLE, glIsFramebuffer, glIsList, glIsProgram, glIsQuery,
            glIsRenderbuffer, glIsSampler, glIsShader, glIsSync, glIsTexture,
            glIsVertexArray, glIsVertexArrayAPPLE, glLightModelf, glLightModelfv,
            glLightModeli, glLightModeliv, glLightf, glLightfv, glLighti, glLightiv,
            glLineStipple, glLineWidth, glLinkProgram, glListBase, glLoadIdentity,
            glLoadMatrixd, glLoadMatrixf, glLoadName, glLoadTransposeMatrixd,
            glLoadTransposeMatrixf, glLogicOp, glMap1d, glMap1f, glMap2d, glMap2f,
            glMapBuffer, glMapBufferRange, glMapGrid1d, glMapGrid1f, glMapGrid2d, glMapGrid2f,
            glMaterialf, glMaterialfv, glMateriali, glMaterialiv, glMatrixMode,
            glMultMatrixd, glMultMatrixf, glMultTransposeMatrixd, glMultTransposeMatrixf,
            glMultiDrawArrays, glMultiDrawElements, glMultiDrawElementsBaseVertex,
            glMultiTexCoord1d, glMultiTexCoord1dv, glMultiTexCoord1f, glMultiTexCoord1fv,
            glMultiTexCoord1i, glMultiTexCoord1iv, glMultiTexCoord1s, glMultiTexCoord1sv,
            glMultiTexCoord2d, glMultiTexCoord2dv, glMultiTexCoord2f, glMultiTexCoord2fv,
            glMultiTexCoord2i, glMultiTexCoord2iv, glMultiTexCoord2s, glMultiTexCoord2sv,
            glMultiTexCoord3d, glMultiTexCoord3dv, glMultiTexCoord3f, glMultiTexCoord3fv,
            glMultiTexCoord3i, glMultiTexCoord3iv, glMultiTexCoord3s, glMultiTexCoord3sv,
            glMultiTexCoord4d, glMultiTexCoord4dv, glMultiTexCoord4f, glMultiTexCoord4fv,
            glMultiTexCoord4i, glMultiTexCoord4iv, glMultiTexCoord4s, glMultiTexCoord4sv,
            glMultiTexCoordP1ui, glMultiTexCoordP1uiv, glMultiTexCoordP2ui,
            glMultiTexCoordP2uiv, glMultiTexCoordP3ui, glMultiTexCoordP3uiv,
            glMultiTexCoordP4ui, glMultiTexCoordP4uiv, glNewList, glNormal3b, glNormal3bv,
            glNormal3d, glNormal3dv, glNormal3f, glNormal3fv, glNormal3i, glNormal3iv,
            glNormal3s, glNormal3sv, glNormalP3ui, glNormalP3uiv, glNormalPointer,
            glObjectLabel, glObjectLabelKHR, glObjectPtrLabel, glObjectPtrLabelKHR, glOrtho,
            glPassThrough, glPixelMapfv, glPixelMapuiv, glPixelMapusv, glPixelStoref,
            glPixelStorei, glPixelTransferf, glPixelTransferi, glPixelZoom,
            glPointParameterf, glPointParameterfv, glPointParameteri, glPointParameteriv,
            glPointSize, glPolygonMode, glPolygonOffset, glPolygonStipple, glPopAttrib,
            glPopClientAttrib, glPopDebugGroup, glPopDebugGroupKHR, glPopGroupMarkerEXT,
            glPopMatrix, glPopName, glPrimitiveRestartIndex, glPrioritizeTextures,
            glProgramBinary, glProgramParameteri, glProvokingVertex, glPushAttrib,
            glPushClientAttrib, glPushDebugGroup, glPushDebugGroupKHR, glPushGroupMarkerEXT,
            glPushMatrix, glPushName, glQueryCounter, glRasterPos2d, glRasterPos2dv,
            glRasterPos2f, glRasterPos2fv, glRasterPos2i, glRasterPos2iv, glRasterPos2s,
            glRasterPos2sv, glRasterPos3d, glRasterPos3dv, glRasterPos3f, glRasterPos3fv,
            glRasterPos3i, glRasterPos3iv, glRasterPos3s, glRasterPos3sv, glRasterPos4d,
            glRasterPos4dv, glRasterPos4f, glRasterPos4fv, glRasterPos4i, glRasterPos4iv,
            glRasterPos4s, glRasterPos4sv, glReadBuffer, glReadPixels, glRectd, glRectdv,
            glRectf, glRectfv, glRecti, glRectiv, glRects, glRectsv, glRenderMode,
            glRenderbufferStorage, glRenderbufferStorageMultisample, glRotated, glRotatef,
            glSampleCoverage, glSampleMaski, glSamplerParameterIiv, glSamplerParameterIuiv,
            glSamplerParameterf, glSamplerParameterfv, glSamplerParameteri,
            glSamplerParameteriv, glScaled, glScalef, glScissor, glSecondaryColor3b,
            glSecondaryColor3bv, glSecondaryColor3d, glSecondaryColor3dv,
            glSecondaryColor3f, glSecondaryColor3fv, glSecondaryColor3i,
            glSecondaryColor3iv, glSecondaryColor3s, glSecondaryColor3sv,
            glSecondaryColor3ub, glSecondaryColor3ubv, glSecondaryColor3ui,
            glSecondaryColor3uiv, glSecondaryColor3us, glSecondaryColor3usv,
            glSecondaryColorP3ui, glSecondaryColorP3uiv, glSecondaryColorPointer,
            glSelectBuffer, glSetFenceAPPLE, glShadeModel, glShaderSource,
            glShaderStorageBlockBinding, glStencilFunc, glStencilFuncSeparate,
            glStencilMask, glStencilMaskSeparate, glStencilOp, glStencilOpSeparate,
            glTestFenceAPPLE, glTestObjectAPPLE, glTexBuffer, glTexCoord1d, glTexCoord1dv,
            glTexCoord1f, glTexCoord1fv, glTexCoord1i, glTexCoord1iv, glTexCoord1s,
            glTexCoord1sv, glTexCoord2d, glTexCoord2dv, glTexCoord2f, glTexCoord2fv,
            glTexCoord2i, glTexCoord2iv, glTexCoord2s, glTexCoord2sv, glTexCoord3d,
            glTexCoord3dv, glTexCoord3f, glTexCoord3fv, glTexCoord3i, glTexCoord3iv,
            glTexCoord3s, glTexCoord3sv, glTexCoord4d, glTexCoord4dv, glTexCoord4f,
            glTexCoord4fv, glTexCoord4i, glTexCoord4iv, glTexCoord4s, glTexCoord4sv,
            glTexCoordP1ui, glTexCoordP1uiv, glTexCoordP2ui, glTexCoordP2uiv,
            glTexCoordP3ui, glTexCoordP3uiv, glTexCoordP4ui, glTexCoordP4uiv,
            glTexCoordPointer, glTexEnvf, glTexEnvfv, glTexEnvi, glTexEnviv, glTexGend,
            glTexGendv, glTexGenf, glTexGenfv, glTexGeni, glTexGeniv, glTexImage1D,
            glTexImage2D, glTexImage2DMultisample, glTexImage3D, glTexImage3DMultisample,
            glTexParameterIiv, glTexParameterIuiv, glTexParameterf, glTexParameterfv,
            glTexParameteri, glTexParameteriv, glTexStorage1D, glTexStorage2D,
            glTexStorage3D, glTexSubImage1D, glTexSubImage2D, glTexSubImage3D,
            glTextureRangeAPPLE, glTransformFeedbackVaryings, glTranslated, glTranslatef,
            glUniform1f, glUniform1fv, glUniform1i, glUniform1iv, glUniform1ui,
            glUniform1uiv, glUniform2f, glUniform2fv, glUniform2i, glUniform2iv,
            glUniform2ui, glUniform2uiv, glUniform3f, glUniform3fv, glUniform3i,
            glUniform3iv, glUniform3ui, glUniform3uiv, glUniform4f, glUniform4fv,
            glUniform4i, glUniform4iv, glUniform4ui, glUniform4uiv, glUniformBlockBinding,
            glUniformMatrix2fv, glUniformMatrix2x3fv, glUniformMatrix2x4fv,
            glUniformMatrix3fv, glUniformMatrix3x2fv, glUniformMatrix3x4fv,
            glUniformMatrix4fv, glUniformMatrix4x2fv, glUniformMatrix4x3fv, glUnmapBuffer,
            glUseProgram, glValidateProgram, glVertex2d, glVertex2dv, glVertex2f,
            glVertex2fv, glVertex2i, glVertex2iv, glVertex2s, glVertex2sv, glVertex3d,
            glVertex3dv, glVertex3f, glVertex3fv, glVertex3i, glVertex3iv, glVertex3s,
            glVertex3sv, glVertex4d, glVertex4dv, glVertex4f, glVertex4fv, glVertex4i,
            glVertex4iv, glVertex4s, glVertex4sv, glVertexAttrib1d, glVertexAttrib1dv,
            glVertexAttrib1f, glVertexAttrib1fv, glVertexAttrib1s, glVertexAttrib1sv,
            glVertexAttrib2d, glVertexAttrib2dv, glVertexAttrib2f, glVertexAttrib2fv,
            glVertexAttrib2s, glVertexAttrib2sv, glVertexAttrib3d, glVertexAttrib3dv,
            glVertexAttrib3f, glVertexAttrib3fv, glVertexAttrib3s, glVertexAttrib3sv,
            glVertexAttrib4Nbv, glVertexAttrib4Niv, glVertexAttrib4Nsv,
            glVertexAttrib4Nub, glVertexAttrib4Nubv, glVertexAttrib4Nuiv,
            glVertexAttrib4Nusv, glVertexAttrib4bv, glVertexAttrib4d, glVertexAttrib4dv,
            glVertexAttrib4f, glVertexAttrib4fv, glVertexAttrib4iv, glVertexAttrib4s,
            glVertexAttrib4sv, glVertexAttrib4ubv, glVertexAttrib4uiv,
            glVertexAttrib4usv, glVertexAttribDivisor, glVertexAttribI1i,
            glVertexAttribI1iv, glVertexAttribI1ui, glVertexAttribI1uiv,
            glVertexAttribI2i, glVertexAttribI2iv, glVertexAttribI2ui,
            glVertexAttribI2uiv, glVertexAttribI3i, glVertexAttribI3iv,
            glVertexAttribI3ui, glVertexAttribI3uiv, glVertexAttribI4bv,
            glVertexAttribI4i, glVertexAttribI4iv, glVertexAttribI4sv,
            glVertexAttribI4ubv, glVertexAttribI4ui, glVertexAttribI4uiv,
            glVertexAttribI4usv, glVertexAttribIPointer, glVertexAttribP1ui,
            glVertexAttribP1uiv, glVertexAttribP2ui, glVertexAttribP2uiv,
            glVertexAttribP3ui, glVertexAttribP3uiv, glVertexAttribP4ui,
            glVertexAttribP4uiv, glVertexAttribPointer, glVertexP2ui, glVertexP2uiv,
            glVertexP3ui, glVertexP3uiv, glVertexP4ui, glVertexP4uiv, glVertexPointer,
            glViewport, glWaitSync, glWindowPos2d, glWindowPos2dv, glWindowPos2f,
            glWindowPos2fv, glWindowPos2i, glWindowPos2iv, glWindowPos2s, glWindowPos2sv,
            glWindowPos3d, glWindowPos3dv, glWindowPos3f, glWindowPos3fv, glWindowPos3i,
            glWindowPos3iv, glWindowPos3s, glWindowPos3sv, glStartTilingQCOM, glEndTilingQCOM
        });
    }
}
