// AzVideo (C) - H.264 hardware-decode video player (Big Buck Bunny).
//
// C port of examples/azul-video/src/main.rs, driving the same FFI:
//   1. AzVideoStartupCheck_run()      - probe VK_KHR_video_decode_h264 readiness.
//   2. local sample (fopen/fread) or AzHttpRequestConfig_downloadBytesDefault()
//                                     - obtain the Big Buck Bunny H.264 MP4 bytes.
//   3. AzDecodedVideo_decodeMp4H264() - demux + decode the whole clip to RGBA.
//   4. each AzVideoFrame -> AzImageRef (RawImageFormat::RGBA8); a per-frame Timer
//      advances an <img> through them so the clip actually plays + loops.
//
// Where no Vulkan Video decoder exists, the decode yields no frames and a
// placeholder box stands in (the probe summary text explains why).
//
// NB: the image CSS deliberately omits border-radius / overflow:hidden - a
// rounded clip on an image node clips the image out in cpurender (known bug);
// a plain border is fine. The display box uses the decoded (native) size.
//
// Build (do NOT use the stale -I../../dll path; azul.h now lives in target/codegen):
//   cc -o video.bin video.c -I../../target/codegen -L../../target/release -lazul
// Run:
//   LD_LIBRARY_PATH=../../target/release ./video.bin

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdarg.h>

// Big Buck Bunny H.264/MP4, 360p. Fetched if the local sample is absent.
#define BBB_URL "https://test-videos.co.uk/vids/bigbuckbunny/mp4/h264/360/Big_Buck_Bunny_360_10s_2MB.mp4"
#define LOCAL_SAMPLE "/tmp/video-media-samples/big-buck-bunny-360p.mp4"
// Cap decoded frames held in memory (360p RGBA ~= 0.9 MB each). Plays + loops.
#define MAX_FRAMES 150
#define MAX_STATUS 12
#define STATUS_LEN 256

// Runtime AzString from a C string (copies the bytes; stack buffers are fine).
#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

typedef struct {
    char status[MAX_STATUS][STATUS_LEN]; // pipeline status lines for the panel
    int n_status;
    AzImageRef* frames;                  // decoded frames as renderable images
    size_t n_frames;
    size_t idx;                          // currently displayed frame (wraps to loop)
    uint32_t vw, vh;                     // coded video size (for the display box)
    float fps;
} VideoApp;

void VideoApp_destructor(void* p) {
    VideoApp* s = (VideoApp*)p;
    if (s->frames) {
        for (size_t i = 0; i < s->n_frames; i++) {
            AzImageRef_delete(&s->frames[i]);
        }
        free(s->frames);
        s->frames = NULL;
        s->n_frames = 0;
    }
}
AZ_REFLECT(VideoApp, VideoApp_destructor);

// Forward declarations
AzDom layout(AzRefAny data, AzLayoutCallbackInfo info);
AzUpdate on_startup(AzRefAny data, AzCallbackInfo info);
AzTimerCallbackReturn advance_frame(AzRefAny data, AzTimerCallbackInfo info);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// Copy an AzString into a NUL-terminated C buffer (truncates to fit).
static void az_string_to_cstr(const AzString* s, char* out, size_t outcap) {
    size_t n = s->vec.len;
    if (n >= outcap) n = outcap - 1;
    if (s->vec.ptr && n > 0) memcpy(out, s->vec.ptr, n);
    out[n] = '\0';
}

// printf-style status line: echo to stderr (like the Rust demo) + store for the UI.
static void push_status(VideoApp* m, const char* fmt, ...) {
    char buf[STATUS_LEN];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);
    fprintf(stderr, "[azvideo] %s\n", buf);
    if (m->n_status < MAX_STATUS) {
        strncpy(m->status[m->n_status], buf, STATUS_LEN - 1);
        m->status[m->n_status][STATUS_LEN - 1] = '\0';
        m->n_status++;
    }
}

// Read a whole file into a malloc'd buffer (plain C stdio).
static unsigned char* read_file(const char* path, size_t* out_len) {
    FILE* f = fopen(path, "rb");
    if (!f) return NULL;
    if (fseek(f, 0, SEEK_END) != 0) { fclose(f); return NULL; }
    long sz = ftell(f);
    if (sz <= 0) { fclose(f); return NULL; }
    rewind(f);
    unsigned char* buf = (unsigned char*)malloc((size_t)sz);
    if (!buf) { fclose(f); return NULL; }
    size_t rd = fread(buf, 1, (size_t)sz, f);
    fclose(f);
    if (rd != (size_t)sz) { free(buf); return NULL; }
    *out_len = (size_t)sz;
    return buf;
}

// ---------------------------------------------------------------------------
// Pipeline: decode the clip bytes + wrap each frame as a renderable image.
// ---------------------------------------------------------------------------

static void decode_and_build(VideoApp* m, AzU8VecRef bytes, bool hw_ready) {
    AzOptionDecodedVideo opt = AzDecodedVideo_decodeMp4H264(bytes);

    if (AzOptionDecodedVideo_isSome(&opt)) {
        AzDecodedVideo* dv = &opt.Some.payload; // borrow; freed via opt below
        m->vw = dv->width;
        m->vh = dv->height;
        m->fps = dv->fps;

        size_t total = AzVideoFrameVec_len(&dv->frames);
        push_status(m, "Demuxed H.264: %ux%u @ %.1f fps - %zu access units fed",
                    dv->width, dv->height, dv->fps, dv->access_units_fed);

        AzVideoFrameVecSlice slice = AzVideoFrameVec_asCSlice(&dv->frames);
        size_t cap = slice.len < (size_t)MAX_FRAMES ? slice.len : (size_t)MAX_FRAMES;
        if (cap > 0) {
            m->frames = (AzImageRef*)malloc(cap * sizeof(AzImageRef));
        }
        for (size_t i = 0; i < cap && m->frames; i++) {
            const AzVideoFrame* vf = &slice.ptr[i];
            // Wrap the decoded RGBA8 frame as a renderable image. newRawimage
            // consumes the AzRawImage (pixels + tag) - on None it frees them.
            AzRawImage raw;
            raw.pixels = AzRawImageData_u8(AzU8Vec_copyFromPtr(vf->bytes.ptr, vf->bytes.len));
            raw.width = vf->width;
            raw.height = vf->height;
            raw.premultiplied_alpha = false;
            raw.data_format = AzRawImageFormat_RGBA8;
            raw.tag = AzU8Vec_copyFromBytes((const uint8_t*)"bbb-frame", 0, 9);

            AzOptionImageRef oimg = AzImageRef_newRawimage(raw);
            if (AzOptionImageRef_isSome(&oimg)) {
                m->frames[m->n_frames++] = oimg.Some.payload;
            }
        }

        push_status(m, "Decoded %zu frames (%ux%u) via %s - %s",
                    total, m->vw, m->vh,
                    hw_ready ? "VK Video (GPU decode, CPU copy-back)" : "no HW decoder",
                    m->n_frames == 0 ? "showing placeholder" : "playing");
    } else {
        // None => no Vulkan Video decoder on this system. Handle gracefully.
        push_status(m, "Decode returned None - no H.264 hardware decoder available");
    }

    AzOptionDecodedVideo_delete(&opt); // single free of the decoded clip + frames
}

static void run_pipeline(VideoApp* m) {
    // 1. Hardware-decode capability probe.
    AzVideoStartupCheck check = AzVideoStartupCheck_run();
    bool hw = check.hw_decode_ready;
    char summ[STATUS_LEN];
    az_string_to_cstr(&check.summary, summ, sizeof(summ));
    push_status(m, "VK hardware H.264 decode: %s - %s",
                hw ? "READY" : "not available", summ);
    if (check.detail.vec.len > 0) {
        char det[STATUS_LEN];
        az_string_to_cstr(&check.detail, det, sizeof(det));
        push_status(m, "%s", det);
    }
    // Provision msgbox/autofix (the Rust C-player extra). The API IS exposed
    // (AzVideoStartupCheck_remediate -> AzVideoProvisionOutcome { ok,
    // reboot_required, message }, plus AzMsgBox_new for the dialog), but
    // remediation installs GPU drivers / may require a reboot, and this machine
    // has a documented incident where driver provisioning left the kernel
    // unbootable. So we SURFACE the capability here and leave the destructive
    // call as a guarded TODO rather than auto-running it from a demo.
    if (check.can_remediate || check.needs_reboot) {
        push_status(m, "driver remediation available (can_remediate=%s needs_reboot=%s) - not auto-run; see TODO",
                    check.can_remediate ? "yes" : "no",
                    check.needs_reboot ? "yes" : "no");
        // TODO(provision): on user confirm via an AzMsgBox, call
        //   AzVideoProvisionOutcome out = AzVideoStartupCheck_remediate();
        //   ... read out.ok / out.reboot_required / out.message ...
        //   AzVideoProvisionOutcome_delete(&out);
        // Gated behind a reboot-safety check before shipping (see memory notes).
    }
    AzVideoStartupCheck_delete(&check);

    // 2. Obtain the clip - prefer the local sample (offline / fast).
    size_t len = 0;
    unsigned char* buf = read_file(LOCAL_SAMPLE, &len);
    if (buf) {
        push_status(m, "Loaded local sample: %zu bytes", len);
        AzU8VecRef ref = { .ptr = buf, .len = len };
        decode_and_build(m, ref, hw); // decode copies the input; safe to free after
        free(buf);
        return;
    }

    // URL fallback via the azul http FFI (mirrors http_get in the Rust demo).
    push_status(m, "Local sample missing (%s) - fetching URL", LOCAL_SAMPLE);
    AzResultU8VecHttpError r = AzHttpRequestConfig_downloadBytesDefault(AZ_STR(BBB_URL));
    if (r.Ok.tag == AzResultU8VecHttpError_Tag_Ok) {
        AzU8Vec* body = &r.Ok.payload; // borrow; freed via r below
        push_status(m, "HTTP GET -> %zu bytes", body->len);
        AzU8VecRef ref = { .ptr = body->ptr, .len = body->len };
        decode_and_build(m, ref, hw);
    } else {
        AzString es = AzHttpError_toDbgString(&r.Err.payload);
        char ebuf[STATUS_LEN];
        az_string_to_cstr(&es, ebuf, sizeof(ebuf));
        AzString_delete(&es);
        push_status(m, "HTTP fetch failed: %s", ebuf);
    }
    AzResultU8VecHttpError_delete(&r);
}

// ---------------------------------------------------------------------------
// Callbacks
// ---------------------------------------------------------------------------

// Per-frame Timer: advance to the next decoded frame (wrap to loop) + relayout.
AzTimerCallbackReturn advance_frame(AzRefAny data, AzTimerCallbackInfo info) {
    (void)info;
    VideoAppRefMut d = VideoAppRefMut_create(&data);
    if (!VideoApp_downcastMut(&data, &d)) {
        return AzTimerCallbackReturn_continueUnchanged();
    }
    bool refresh = false;
    if (d.ptr->n_frames > 0) {
        d.ptr->idx = (d.ptr->idx + 1) % d.ptr->n_frames;
        refresh = true;
    }
    VideoAppRefMut_delete(&d);
    return refresh ? AzTimerCallbackReturn_continueAndRefreshDom()
                   : AzTimerCallbackReturn_continueUnchanged();
}

// Window-create: install the playback Timer (only if we have frames to show).
AzUpdate on_startup(AzRefAny data, AzCallbackInfo info) {
    bool has_frames = false;
    uint64_t interval_ms = 40; // ~25 fps default
    VideoAppRef d = VideoAppRef_create(&data);
    if (VideoApp_downcastRef(&data, &d)) {
        has_frames = d.ptr->n_frames > 0;
        if (d.ptr->fps > 0.0f) {
            interval_ms = (uint64_t)(1000.0f / d.ptr->fps);
            if (interval_ms == 0) interval_ms = 1;
        }
        VideoAppRef_delete(&d);
    }
    if (!has_frames) {
        return AzUpdate_DoNothing;
    }

    AzGetSystemTimeCallback time_fn = AzCallbackInfo_getSystemTimeFn(&info);
    AzTimer timer = AzTimer_create(
        AzRefAny_clone(&data),
        (AzTimerCallback){ .cb = advance_frame, .ctx = AzOptionRefAny_none() },
        time_fn);

    AzSystemTimeDiff diff = AzSystemTimeDiff_fromMillis(interval_ms);
    AzDuration interval = { .System = { .tag = AzDuration_Tag_System, .payload = diff } };
    timer = AzTimer_withInterval(timer, interval);

    AzCallbackInfo_addTimer(&info, AzTimerId_unique(), timer);
    return AzUpdate_DoNothing;
}

AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    (void)info;
    VideoAppRef d = VideoAppRef_create(&data);
    if (!VideoApp_downcastRef(&data, &d)) {
        return AzDom_createBody();
    }

    AzDom body = AzDom_createBody();
    AzDom_setCss(&body, AZ_STR(
        "display: flex; flex-direction: column; padding: 16px; background: #0e0e14; "
        "font-family: sans-serif; color: #e6e6f0;"));

    AzDom title = AzDom_createText(AZ_STR("AzVideo (C) - H.264 hardware decode (Big Buck Bunny)"));
    AzDom_setCss(&title, AZ_STR("font-size: 22px; margin-bottom: 10px;"));
    AzDom_addChild(&body, title);

    for (int i = 0; i < d.ptr->n_status; i++) {
        AzDom line = AzDom_createText(AZ_STR(d.ptr->status[i]));
        AzDom_setCss(&line, AZ_STR("font-size: 13px; color: #b8c0d0; margin-bottom: 5px;"));
        AzDom_addChild(&body, line);
    }

    // Sizing for the video box: fit ~520px wide, keep aspect (native fallback).
    uint32_t boxw, boxh;
    if (d.ptr->vw > 0 && d.ptr->vh > 0) {
        float scale = 520.0f / (float)d.ptr->vw;
        boxw = 520;
        boxh = (uint32_t)((float)d.ptr->vh * scale);
    } else {
        boxw = 480;
        boxh = 270;
    }

    if (d.ptr->n_frames > 0) {
        size_t idx = d.ptr->idx;
        char pbuf[64];
        snprintf(pbuf, sizeof(pbuf), "playing - frame %zu/%zu", idx + 1, d.ptr->n_frames);
        AzDom playing = AzDom_createText(AZ_STR(pbuf));
        AzDom_setCss(&playing, AZ_STR("font-size: 12px; color: #7ad17a; margin: 10px 0 5px 0;"));
        AzDom_addChild(&body, playing);

        // Clone the stored frame: createImage consumes the ImageRef.
        AzImageRef img = AzImageRef_clone(&d.ptr->frames[idx]);
        AzDom img_dom = AzDom_createImage(img);
        char css[160];
        // No border-radius / overflow:hidden here (clips the image blank in cpurender).
        snprintf(css, sizeof(css),
                 "width: %upx; height: %upx; flex-shrink: 0; border: 2px solid #2a2a3a;",
                 boxw, boxh);
        AzDom_setCss(&img_dom, AZ_STR(css));
        AzDom_addChild(&body, img_dom);
    } else {
        AzDom note = AzDom_createText(AZ_STR(
            "no decoded frames - no Vulkan Video decode here (see probe summary above)"));
        AzDom_setCss(&note, AZ_STR("font-size: 12px; color: #6a7080; margin: 10px 0 5px 0;"));
        AzDom_addChild(&body, note);

        AzDom placeholder = AzDom_createDiv();
        char css[160];
        snprintf(css, sizeof(css),
                 "width: %upx; height: %upx; background: #16161e; border: 2px solid #2a2a3a;",
                 boxw, boxh);
        AzDom_setCss(&placeholder, AZ_STR(css));
        AzDom_addChild(&body, placeholder);
    }

    VideoAppRef_delete(&d);
    return body;
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

int main(void) {
    fprintf(stderr, "[azvideo] decoding (this can take a few seconds)...\n");

    VideoApp model;
    memset(&model, 0, sizeof(model));
    run_pipeline(&model);

    AzRefAny data = VideoApp_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create((AzLayoutCallbackType)layout);
    window.window_state.title = AZ_STR("AzVideo (C) - Big Buck Bunny");
    window.window_state.size.dimensions.width = 600.0;
    window.window_state.size.dimensions.height = 640.0;

    // Install the playback Timer at window-create (ctx = clone of app data).
    AzCallback startup_cb = {
        .cb = (AzCallbackType)on_startup,
        .ctx = AzOptionRefAny_some(AzRefAny_clone(&data)),
    };
    window.create_callback = AzOptionCallback_some(startup_cb);

    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
