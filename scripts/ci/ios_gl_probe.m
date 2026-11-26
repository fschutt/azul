// ios_gl_probe.m: what the Simulator's (or a phone's) OpenGL ES really is, for the GPU path of the iOS shell.
//
// A command-line tool for the iphonesimulator SDK, run INSIDE the simulator with `xcrun simctl spawn <udid> <tool>`
// (scripts/ci/ios_gl_probe.sh builds and runs it). It creates an EAGLContext (ES 3.0, then ES 2.0), prints the
// strings and limits WebRender decides its capabilities from, tests the render-target formats WebRender draws into,
// binds a renderbuffer to a CAEAGLLayer the way the shell does, clears it and reads a pixel back, presents it,
// times 120 frames of clear + present, compiles a `#version 300 es` program, and asks the dynamic loader for every
// GL function the engine's loader table names (a file of names given as argv[1], one per line).
// It also asks Metal for its default device: the runner's answer to "is there a GPU at all".
//
// Plain text on stdout, one fact per line, prefixed `probe:`. No UIKit, no window: nothing here needs an app.
#import <Foundation/Foundation.h>
#import <QuartzCore/QuartzCore.h>
#import <OpenGLES/EAGL.h>
#import <OpenGLES/ES3/gl.h>
#import <OpenGLES/ES3/glext.h>
#import <Metal/Metal.h>
#include <dlfcn.h>
#include <mach/mach_time.h>
#include <sys/utsname.h>

#define AZ_GL_BGRA_EXT 0x80E1
#define AZ_GL_BGRA8_EXT 0x93A1
#define AZ_GL_RGBA32F 0x8814
#define AZ_GL_RGBA16F 0x881A

static void line(NSString *fmt, ...) NS_FORMAT_FUNCTION(1, 2);
static void line(NSString *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    NSString *s = [[NSString alloc] initWithFormat:fmt arguments:ap];
    va_end(ap);
    fputs([[@"probe: " stringByAppendingString:[s stringByAppendingString:@"\n"]] UTF8String], stdout);
    fflush(stdout);
}

static double now_ms(void) {
    static mach_timebase_info_data_t tb;
    if (tb.denom == 0) mach_timebase_info(&tb);
    return (double)mach_absolute_time() * tb.numer / tb.denom / 1e6;
}

static const char *status_name(GLenum s) {
    switch (s) {
        case GL_FRAMEBUFFER_COMPLETE: return "complete";
        case GL_FRAMEBUFFER_INCOMPLETE_ATTACHMENT: return "incomplete-attachment";
        case GL_FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT: return "missing-attachment";
        case GL_FRAMEBUFFER_UNSUPPORTED: return "unsupported";
        case GL_FRAMEBUFFER_INCOMPLETE_MULTISAMPLE: return "incomplete-multisample";
        default: return "other";
    }
}

static void print_wrapped(NSString *label, NSArray<NSString *> *items) {
    NSMutableString *cur = [NSMutableString string];
    for (NSString *it in items) {
        if (cur.length + it.length + 1 > 110) {
            line(@"%@ %@", label, cur);
            cur = [NSMutableString string];
        }
        if (cur.length) [cur appendString:@" "];
        [cur appendString:it];
    }
    if (cur.length) line(@"%@ %@", label, cur);
}

// a render target of this format: does an FBO with it attached come out complete?
static void test_target(const char *name, GLint internal, GLenum format, GLenum type) {
    GLuint tex = 0, fbo = 0;
    while (glGetError() != GL_NO_ERROR) {}
    glGenTextures(1, &tex);
    glBindTexture(GL_TEXTURE_2D, tex);
    glTexImage2D(GL_TEXTURE_2D, 0, internal, 8, 8, 0, format, type, NULL);
    GLenum upload_err = glGetError();
    glGenFramebuffers(1, &fbo);
    glBindFramebuffer(GL_FRAMEBUFFER, fbo);
    glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex, 0);
    GLenum st = glCheckFramebufferStatus(GL_FRAMEBUFFER);
    line(@"target %s: glTexImage2D error 0x%04x, framebuffer %s (0x%04x)", name, upload_err, status_name(st), st);
    glBindFramebuffer(GL_FRAMEBUFFER, 0);
    glDeleteFramebuffers(1, &fbo);
    glDeleteTextures(1, &tex);
    while (glGetError() != GL_NO_ERROR) {}
}

static void test_storage(void) {
    while (glGetError() != GL_NO_ERROR) {}
    GLuint tex = 0;
    glGenTextures(1, &tex);
    glBindTexture(GL_TEXTURE_2D, tex);
    glTexStorage2D(GL_TEXTURE_2D, 1, GL_RGBA8, 16, 16);
    line(@"glTexStorage2D RGBA8: error 0x%04x", glGetError());
    glTexSubImage2D(GL_TEXTURE_2D, 0, 0, 0, 4, 4, AZ_GL_BGRA_EXT, GL_UNSIGNED_BYTE, (uint8_t[64]){0});
    line(@"glTexSubImage2D with GL_BGRA_EXT into an RGBA8 storage: error 0x%04x", glGetError());
    GLuint arr = 0;
    glGenTextures(1, &arr);
    glBindTexture(GL_TEXTURE_2D_ARRAY, arr);
    glTexStorage3D(GL_TEXTURE_2D_ARRAY, 1, GL_RGBA8, 16, 16, 4);
    line(@"glTexStorage3D 2D array RGBA8 16x16x4: error 0x%04x", glGetError());
    GLuint t2 = 0;
    glGenTextures(1, &t2);
    glBindTexture(GL_TEXTURE_2D, t2);
    glTexStorage2D(GL_TEXTURE_2D, 1, AZ_GL_BGRA8_EXT, 16, 16);
    line(@"glTexStorage2D BGRA8_EXT: error 0x%04x", glGetError());
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_SWIZZLE_R, GL_BLUE);
    line(@"texture swizzle (GL_TEXTURE_SWIZZLE_R): error 0x%04x", glGetError());
    glDeleteTextures(1, &tex);
    glDeleteTextures(1, &arr);
    glDeleteTextures(1, &t2);
    while (glGetError() != GL_NO_ERROR) {}
}

static void test_shader(void) {
    const char *vs =
        "#version 300 es\n"
        "precision highp float; precision highp int; precision highp sampler2D; precision highp sampler2DArray;\n"
        "in vec2 aPos; in vec4 aData; flat out ivec2 vI; out vec4 vC;\n"
        "uniform mat4 uTransform; uniform sampler2D sGpuCache;\n"
        "void main() { vI = ivec2(aData.xy); vC = texelFetch(sGpuCache, ivec2(0, 0), 0) + aData;\n"
        "  gl_Position = uTransform * vec4(aPos, 0.0, 1.0); }\n";
    const char *fs =
        "#version 300 es\n"
        "precision highp float; precision highp int; precision highp sampler2DArray;\n"
        "flat in ivec2 vI; in vec4 vC; out vec4 oFragColor; uniform sampler2DArray sColor0;\n"
        "void main() { oFragColor = vC + texelFetch(sColor0, ivec3(vI, 0), 0); }\n";
    GLuint v = glCreateShader(GL_VERTEX_SHADER), f = glCreateShader(GL_FRAGMENT_SHADER);
    glShaderSource(v, 1, &vs, NULL);
    glCompileShader(v);
    glShaderSource(f, 1, &fs, NULL);
    glCompileShader(f);
    GLint okv = 0, okf = 0;
    glGetShaderiv(v, GL_COMPILE_STATUS, &okv);
    glGetShaderiv(f, GL_COMPILE_STATUS, &okf);
    GLuint p = glCreateProgram();
    glAttachShader(p, v);
    glAttachShader(p, f);
    glLinkProgram(p);
    GLint okp = 0;
    glGetProgramiv(p, GL_LINK_STATUS, &okp);
    line(@"#version 300 es program (texelFetch, flat ivec2, sampler2DArray): vertex %d fragment %d link %d", okv, okf, okp);
    if (!okp) {
        char log[512] = {0};
        glGetProgramInfoLog(p, sizeof log - 1, NULL, log);
        line(@"link log: %s", log);
    }
    GLint fmts = 0;
    glGetIntegerv(GL_NUM_PROGRAM_BINARY_FORMATS, &fmts);
    line(@"program binary formats: %d (error 0x%04x)", fmts, glGetError());
    const char *bad = "#version 150\nvoid main() { gl_Position = vec4(0.0); }\n";
    GLuint b = glCreateShader(GL_VERTEX_SHADER);
    glShaderSource(b, 1, &bad, NULL);
    glCompileShader(b);
    GLint okb = 1;
    glGetShaderiv(b, GL_COMPILE_STATUS, &okb);
    line(@"#version 150 (desktop GLSL) compiles: %d (expected 0 on an ES context)", okb);
    glDeleteProgram(p);
    glDeleteShader(v);
    glDeleteShader(f);
    glDeleteShader(b);
}

static void probe_context(EAGLRenderingAPI api, NSArray<NSString *> *symbols) {
    double t0 = now_ms();
    EAGLContext *ctx = [[EAGLContext alloc] initWithAPI:api];
    double created = now_ms() - t0;
    line(@"EAGLContext api=%d: %@ (%.1f ms)", (int)api, ctx ? @"created" : @"FAILED (nil)", created);
    if (!ctx) return;
    BOOL cur = [EAGLContext setCurrentContext:ctx];
    line(@"setCurrentContext: %d", cur);
    const char *s;
    s = (const char *)glGetString(GL_VENDOR);
    line(@"GL_VENDOR = %s", s ? s : "(null)");
    s = (const char *)glGetString(GL_RENDERER);
    line(@"GL_RENDERER = %s", s ? s : "(null)");
    s = (const char *)glGetString(GL_VERSION);
    line(@"GL_VERSION = %s", s ? s : "(null)");
    s = (const char *)glGetString(GL_SHADING_LANGUAGE_VERSION);
    line(@"GL_SHADING_LANGUAGE_VERSION = %s", s ? s : "(null)");

    NSMutableArray<NSString *> *exts = [NSMutableArray array];
    if (api == kEAGLRenderingAPIOpenGLES3) {
        GLint n = 0;
        glGetIntegerv(GL_NUM_EXTENSIONS, &n);
        for (GLint i = 0; i < n; i++) {
            const char *e = (const char *)glGetStringi(GL_EXTENSIONS, (GLuint)i);
            if (e) [exts addObject:[NSString stringWithUTF8String:e]];
        }
    } else {
        const char *e = (const char *)glGetString(GL_EXTENSIONS);
        if (e) [exts addObjectsFromArray:[[NSString stringWithUTF8String:e] componentsSeparatedByString:@" "]];
    }
    [exts sortUsingSelector:@selector(compare:)];
    line(@"extensions (%lu):", (unsigned long)exts.count);
    print_wrapped(@"  ext", exts);
    // the ones WebRender's device layer branches on
    NSArray *wanted = @[ @"GL_EXT_color_buffer_float", @"GL_EXT_color_buffer_half_float", @"GL_EXT_texture_format_BGRA8888",
                         @"GL_APPLE_texture_format_BGRA8888", @"GL_EXT_texture_storage", @"GL_EXT_buffer_storage",
                         @"GL_EXT_copy_image", @"GL_KHR_blend_equation_advanced", @"GL_EXT_blend_func_extended",
                         @"GL_OES_EGL_image_external_essl3", @"GL_EXT_debug_marker", @"GL_KHR_debug",
                         @"GL_QCOM_tiled_rendering", @"GL_OES_get_program_binary", @"GL_EXT_disjoint_timer_query",
                         @"GL_APPLE_clip_distance", @"GL_EXT_shader_framebuffer_fetch", @"GL_EXT_map_buffer_range",
                         @"GL_OES_texture_float_linear", @"GL_OES_texture_half_float_linear", @"GL_EXT_texture_filter_anisotropic" ];
    for (NSString *w in wanted) line(@"  webrender wants %@: %@", w, [exts containsObject:w] ? @"present" : @"absent");

    GLint v = 0;
#define LIM(name) glGetIntegerv(name, &v); line(@"  %s = %d", #name, v);
    LIM(GL_MAX_TEXTURE_SIZE) LIM(GL_MAX_RENDERBUFFER_SIZE) LIM(GL_MAX_TEXTURE_IMAGE_UNITS)
    LIM(GL_MAX_VERTEX_TEXTURE_IMAGE_UNITS) LIM(GL_MAX_COMBINED_TEXTURE_IMAGE_UNITS) LIM(GL_MAX_VERTEX_ATTRIBS)
    LIM(GL_MAX_VARYING_VECTORS) LIM(GL_MAX_VERTEX_UNIFORM_VECTORS) LIM(GL_MAX_FRAGMENT_UNIFORM_VECTORS)
    if (api == kEAGLRenderingAPIOpenGLES3) {
        LIM(GL_MAX_ARRAY_TEXTURE_LAYERS) LIM(GL_MAX_3D_TEXTURE_SIZE) LIM(GL_MAX_COLOR_ATTACHMENTS)
        LIM(GL_MAX_DRAW_BUFFERS) LIM(GL_MAX_UNIFORM_BLOCK_SIZE) LIM(GL_MAX_ELEMENTS_VERTICES)
        LIM(GL_MAX_SAMPLES)
    }
#undef LIM

    if (api == kEAGLRenderingAPIOpenGLES3) {
        test_target("RGBA8", GL_RGBA8, GL_RGBA, GL_UNSIGNED_BYTE);
        test_target("BGRA (GL_BGRA_EXT upload into GL_RGBA)", GL_RGBA, AZ_GL_BGRA_EXT, GL_UNSIGNED_BYTE);
        test_target("R8", GL_R8, GL_RED, GL_UNSIGNED_BYTE);
        test_target("RG8", GL_RG8, GL_RG, GL_UNSIGNED_BYTE);
        test_target("RGBA16F", AZ_GL_RGBA16F, GL_RGBA, GL_HALF_FLOAT);
        test_target("RGBA32F", AZ_GL_RGBA32F, GL_RGBA, GL_FLOAT);
        test_target("R16F", GL_R16F, GL_RED, GL_HALF_FLOAT);
        test_target("R32F", GL_R32F, GL_RED, GL_FLOAT);
        test_target("RGB10_A2", GL_RGB10_A2, GL_RGBA, GL_UNSIGNED_INT_2_10_10_10_REV);
        test_storage();
        test_shader();
    }

    // the shell's way to a surface: a CAEAGLLayer's storage as the colour renderbuffer of an FBO
    CAEAGLLayer *layer = [CAEAGLLayer layer];
    layer.frame = CGRectMake(0, 0, 390, 844);
    layer.contentsScale = 3.0;
    layer.opaque = YES;
    layer.drawableProperties = @{ kEAGLDrawablePropertyRetainedBacking: @NO, kEAGLDrawablePropertyColorFormat: kEAGLColorFormatRGBA8 };
    GLuint fbo = 0, rb = 0;
    glGenFramebuffers(1, &fbo);
    glBindFramebuffer(GL_FRAMEBUFFER, fbo);
    glGenRenderbuffers(1, &rb);
    glBindRenderbuffer(GL_RENDERBUFFER, rb);
    double t1 = now_ms();
    BOOL stored = [ctx renderbufferStorage:GL_RENDERBUFFER fromDrawable:layer];
    line(@"renderbufferStorage:fromDrawable: %d (%.1f ms)", stored, now_ms() - t1);
    glFramebufferRenderbuffer(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_RENDERBUFFER, rb);
    GLint rw = 0, rh = 0, rf = 0;
    glGetRenderbufferParameteriv(GL_RENDERBUFFER, GL_RENDERBUFFER_WIDTH, &rw);
    glGetRenderbufferParameteriv(GL_RENDERBUFFER, GL_RENDERBUFFER_HEIGHT, &rh);
    glGetRenderbufferParameteriv(GL_RENDERBUFFER, GL_RENDERBUFFER_INTERNAL_FORMAT, &rf);
    GLenum st = glCheckFramebufferStatus(GL_FRAMEBUFFER);
    line(@"layer storage 390x844 @3x: renderbuffer %dx%d internal format 0x%04x, framebuffer %s", rw, rh, rf, status_name(st));
    glViewport(0, 0, rw, rh);
    glClearColor(0.2f, 0.4f, 0.6f, 1.0f);
    glClear(GL_COLOR_BUFFER_BIT);
    uint8_t px[4] = {0, 0, 0, 0};
    glReadPixels(10, 10, 1, 1, GL_RGBA, GL_UNSIGNED_BYTE, px);
    line(@"cleared to (51,102,153,255), read back (%d,%d,%d,%d), glGetError 0x%04x", px[0], px[1], px[2], px[3], glGetError());
    BOOL presented = [ctx presentRenderbuffer:GL_RENDERBUFFER];
    line(@"presentRenderbuffer: %d", presented);

    // 120 frames of clear + present: what one empty frame costs here
    double start = now_ms();
    double worst = 0;
    for (int i = 0; i < 120; i++) {
        double f0 = now_ms();
        glClearColor((float)(i % 10) / 10.0f, 0.4f, 0.6f, 1.0f);
        glClear(GL_COLOR_BUFFER_BIT);
        [ctx presentRenderbuffer:GL_RENDERBUFFER];
        double d = now_ms() - f0;
        if (d > worst) worst = d;
    }
    double total = now_ms() - start;
    line(@"120 x (clear + presentRenderbuffer): %.1f ms in all, %.2f ms a frame, worst %.2f ms", total, total / 120.0, worst);
    glFinish();

    if (symbols.count && api == kEAGLRenderingAPIOpenGLES3) {
        void *h = dlopen("/System/Library/Frameworks/OpenGLES.framework/OpenGLES", RTLD_NOW);
        line(@"dlopen(OpenGLES.framework/OpenGLES): %s", h ? "ok" : dlerror());
        NSMutableArray<NSString *> *missing = [NSMutableArray array];
        NSUInteger found = 0;
        for (NSString *name in symbols) {
            void *p = h ? dlsym(h, name.UTF8String) : NULL;
            if (p) found++; else [missing addObject:name];
        }
        line(@"the engine's GL loader table: %lu names, %lu resolve through dlsym on the framework handle, %lu do not",
             (unsigned long)symbols.count, (unsigned long)found, (unsigned long)missing.count);
        // the functions WebRender's device layer calls on an ES 3.0 context (names it must find)
        NSArray *needed = @[ @"glActiveTexture", @"glAttachShader", @"glBindAttribLocation", @"glBindBuffer", @"glBindBufferBase",
                             @"glBindBufferRange", @"glBindFramebuffer", @"glBindRenderbuffer", @"glBindTexture", @"glBindVertexArray",
                             @"glBlendEquation", @"glBlendEquationSeparate", @"glBlendFunc", @"glBlendFuncSeparate", @"glBlitFramebuffer",
                             @"glBufferData", @"glBufferSubData", @"glCheckFramebufferStatus", @"glClear", @"glClearBufferfv",
                             @"glClearColor", @"glClearDepthf", @"glClientWaitSync", @"glColorMask", @"glCompileShader",
                             @"glCopyBufferSubData", @"glCopyTexSubImage2D", @"glCreateProgram", @"glCreateShader", @"glCullFace",
                             @"glDeleteBuffers", @"glDeleteFramebuffers", @"glDeleteProgram", @"glDeleteRenderbuffers", @"glDeleteShader",
                             @"glDeleteSync", @"glDeleteTextures", @"glDeleteVertexArrays", @"glDepthFunc", @"glDepthMask", @"glDisable",
                             @"glDisableVertexAttribArray", @"glDrawArrays", @"glDrawArraysInstanced", @"glDrawElements",
                             @"glDrawElementsInstanced", @"glEnable", @"glEnableVertexAttribArray", @"glFenceSync", @"glFinish",
                             @"glFlush", @"glFlushMappedBufferRange", @"glFramebufferRenderbuffer", @"glFramebufferTexture2D",
                             @"glFramebufferTextureLayer", @"glGenBuffers", @"glGenFramebuffers", @"glGenRenderbuffers", @"glGenTextures",
                             @"glGenVertexArrays", @"glGetActiveUniform", @"glGetAttribLocation", @"glGetError", @"glGetFloatv",
                             @"glGetIntegerv", @"glGetProgramBinary", @"glGetProgramInfoLog", @"glGetProgramiv", @"glGetShaderInfoLog",
                             @"glGetShaderiv", @"glGetString", @"glGetStringi", @"glGetUniformBlockIndex", @"glGetUniformLocation",
                             @"glInvalidateFramebuffer", @"glLinkProgram", @"glMapBufferRange", @"glPixelStorei", @"glProgramBinary",
                             @"glReadPixels", @"glRenderbufferStorage", @"glScissor", @"glShaderSource", @"glTexImage2D", @"glTexImage3D",
                             @"glTexParameteri", @"glTexStorage2D", @"glTexStorage3D", @"glTexSubImage2D", @"glTexSubImage3D",
                             @"glUniform1i", @"glUniform1iv", @"glUniform4fv", @"glUniformBlockBinding", @"glUniformMatrix4fv",
                             @"glUnmapBuffer", @"glUseProgram", @"glVertexAttribDivisor", @"glVertexAttribIPointer",
                             @"glVertexAttribPointer", @"glViewport", @"glBufferStorage", @"glBufferStorageEXT", @"glCopyImageSubData",
                             @"glCopyImageSubDataEXT", @"glBlendBarrierKHR", @"glPushDebugGroupKHR", @"glInsertEventMarkerEXT",
                             @"glGenQueries", @"glBeginQuery", @"glEndQuery", @"glGetQueryObjectuiv", @"glDebugMessageControlKHR" ];
        NSMutableArray<NSString *> *needed_missing = [NSMutableArray array];
        for (NSString *n in needed) if (!(h && dlsym(h, n.UTF8String))) [needed_missing addObject:n];
        line(@"of %lu functions WebRender's device layer names, missing from the framework: %@", (unsigned long)needed.count,
             needed_missing.count ? [needed_missing componentsJoinedByString:@" "] : @"(none)");
        // every loader-table name that does not resolve (the desktop-only ones are expected)
        print_wrapped(@"  unresolved", missing);
    }
    glBindFramebuffer(GL_FRAMEBUFFER, 0);
    glDeleteRenderbuffers(1, &rb);
    glDeleteFramebuffers(1, &fbo);
    [EAGLContext setCurrentContext:nil];
}

int main(int argc, char **argv) {
    @autoreleasepool {
        struct utsname u;
        uname(&u);
        line(@"uname: %s %s %s", u.sysname, u.release, u.machine);
        for (NSString *k in @[ @"SIMULATOR_RUNTIME_VERSION", @"SIMULATOR_MODEL_IDENTIFIER", @"SIMULATOR_DEVICE_NAME",
                               @"SIMULATOR_ROOT", @"SIMULATOR_HOST_HOME" ]) {
            const char *v = getenv(k.UTF8String);
            line(@"env %@ = %s", k, v ? v : "(unset)");
        }
        id<MTLDevice> mtl = MTLCreateSystemDefaultDevice();
        line(@"Metal default device: %@", mtl ? mtl.name : @"nil (no Metal device)");
        if (mtl) {
            line(@"  Metal: registryID=%llu apple7=%d apple6=%d apple4=%d maxBufferLength=%lu", (unsigned long long)mtl.registryID,
                 (int)[mtl supportsFamily:MTLGPUFamilyApple7], (int)[mtl supportsFamily:MTLGPUFamilyApple6],
                 (int)[mtl supportsFamily:MTLGPUFamilyApple4], (unsigned long)mtl.maxBufferLength);
        }
        NSArray<NSString *> *symbols = @[];
        if (argc > 1) {
            NSString *all = [NSString stringWithContentsOfFile:[NSString stringWithUTF8String:argv[1]] encoding:NSUTF8StringEncoding error:nil];
            NSMutableArray<NSString *> *names = [NSMutableArray array];
            for (NSString *l in [all componentsSeparatedByString:@"\n"]) if (l.length) [names addObject:l];
            symbols = names;
        }
        probe_context(kEAGLRenderingAPIOpenGLES3, symbols);
        probe_context(kEAGLRenderingAPIOpenGLES2, @[]);
        line(@"done");
    }
    return 0;
}
