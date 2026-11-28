/*
 * ctl.c: the native half of the COLD-START LAB's control app (scripts/startup-control/build.sh).
 *
 * The control is the real AzulActivity (the NativeActivity subclass every Azul APK ships) over this library instead of
 * libazul: no engine, no layout, no fonts. It does to the window what the Rust shell (dll/src/desktop/shell2/android) does
 * to it, with the costs of the layout replaced by sleeps you can set, so that what the SYSTEM does with a NativeActivity's
 * window at a cold start can be told apart from what the ENGINE adds:
 *
 *   window created   ANativeWindow_getWidth/Height, as `InitWindow` reads them
 *   (optional)       a flat frame of one colour at once, before any layout (debug.azctl.flat)
 *   p1 ms            "the first layout"
 *   first frame      ANativeWindow_setBuffersGeometry(w, h, RGBA8888), ANativeWindow_lock(NULL), a copy of every row,
 *                    ANativeWindow_unlockAndPost: the exact calls of `render_frame`; then
 *                    AzulActivity.firstFramePresented(activity), as `signal_first_frame` does
 *   p2 ms            "the Resize pass the insets force"
 *   second frame     the same page, `shift` px lower (the status bar's insets): the jump
 *   afterwards       a RedrawNeeded or a new surface is answered with the last frame, whole (the shell's E47 path)
 *
 * Knobs, system properties (adb shell setprop debug.azctl.<name> <value>), read at the surface:
 *   p1 (400)     ms from the surface to the first frame        p2 (300)    ms from the first to the second frame, 0 = no second frame
 *   flat (-1)    0xRRGGBB: post a flat frame of that colour the moment the surface is there, -1 = none
 *   geom (1)     call setBuffersGeometry before the first post   shift (128) rows the second frame is lower
 *   sig (0)      ms between the first post and firstFramePresented
 *   dark (0)     1: the pictures are dark (the night mode's), 0: light
 * Every step logs `[azctl] ...` with CLOCK_MONOTONIC milliseconds.
 */
#include <android/log.h>
#include <android/native_activity.h>
#include <android/native_window.h>
#include <jni.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/system_properties.h>
#include <time.h>

#define TAG "azctl"
#define LOGI(...) __android_log_print(ANDROID_LOG_INFO, TAG, __VA_ARGS__)

static double now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec * 1000.0 + (double)ts.tv_nsec / 1.0e6;
}

static void sleep_ms(long ms) {
    if (ms <= 0) {
        return;
    }
    struct timespec ts = {.tv_sec = ms / 1000, .tv_nsec = (ms % 1000) * 1000000L};
    nanosleep(&ts, NULL);
}

static long prop(const char *name, long dflt) {
    char key[PROP_NAME_MAX];
    char val[PROP_VALUE_MAX];
    memset(val, 0, sizeof val);
    snprintf(key, sizeof key, "debug.azctl.%s", name);
    if (__system_property_get(key, val) > 0 && val[0] != 0) {
        return strtol(val, NULL, 0);
    }
    return dflt;
}

static uint32_t rgba(uint32_t r, uint32_t g, uint32_t b) {
    return (0xFFu << 24) | (b << 16) | (g << 8) | r;
}

struct ctl {
    ANativeActivity *activity;
    pthread_mutex_t lock;
    pthread_cond_t cond;
    pthread_t thread;
    ANativeWindow *window; /* the surface now, or NULL; guarded by `lock` */
    int generation;        /* bumped at every new surface */
    int redraw;            /* a RedrawNeeded arrived */
    int quit;
    int geometry_set;      /* setBuffersGeometry was told this surface's frame size */
};

static void rect(uint32_t *px, int w, int h, int x0, int y0, int rw, int rh, uint32_t colour) {
    for (int y = y0 < 0 ? 0 : y0; y < y0 + rh && y < h; y++) {
        for (int x = x0 < 0 ? 0 : x0; x < x0 + rw && x < w; x++) {
            px[(size_t)y * (size_t)w + (size_t)x] = colour;
        }
    }
}

/* A page that is not one colour (cards on a cream ground under a header bar), `offset` rows lower: what the second frame is. */
static uint32_t *paint_page(int w, int h, int offset, int dark) {
    uint32_t *px = malloc((size_t)w * (size_t)h * 4);
    if (!px) {
        return NULL;
    }
    uint32_t ground = dark ? rgba(0x12, 0x14, 0x16) : rgba(0xEE, 0xE9, 0xDC);
    uint32_t card = dark ? rgba(0x24, 0x28, 0x2c) : rgba(0xFF, 0xFF, 0xFF);
    uint32_t head = rgba(0x1F, 0x5A, 0x6B);
    uint32_t bar = dark ? rgba(0x70, 0x78, 0x80) : rgba(0xB8, 0xB4, 0xA8);
    for (size_t i = 0; i < (size_t)w * (size_t)h; i++) {
        px[i] = ground;
    }
    rect(px, w, h, 0, offset, w, 96, head);
    for (int i = 0; i < 6; i++) {
        int y = offset + 140 + i * 360;
        rect(px, w, h, 40, y, w - 80, 320, card);
        rect(px, w, h, 80, y + 40, w - 400, 40, bar);
        rect(px, w, h, 80, y + 120, w - 200, 24, bar);
    }
    return px;
}

/* One present, the shell's way. Returns 0 when a frame was posted. */
static int post(struct ctl *c, const char *what, const uint32_t *px, int w, int h, int use_geometry) {
    double t0 = now_ms();
    pthread_mutex_lock(&c->lock);
    ANativeWindow *win = c->window;
    if (!win) {
        pthread_mutex_unlock(&c->lock);
        LOGI("[azctl] post %s: no surface", what);
        return -1;
    }
    int geometry = 0;
    if (use_geometry && !c->geometry_set) {
        geometry = ANativeWindow_setBuffersGeometry(win, w, h, WINDOW_FORMAT_RGBA_8888);
        c->geometry_set = 1;
    }
    double t1 = now_ms();
    ANativeWindow_Buffer buf;
    int locked = ANativeWindow_lock(win, &buf, NULL);
    double t2 = now_ms();
    if (locked != 0) {
        pthread_mutex_unlock(&c->lock);
        LOGI("[azctl] post %s: lock failed %d", what, locked);
        return -1;
    }
    int rows = h < buf.height ? h : buf.height;
    int cols = w < buf.width ? w : buf.width;
    for (int y = 0; y < rows; y++) {
        memcpy((uint8_t *)buf.bits + (size_t)y * (size_t)buf.stride * 4, px + (size_t)y * (size_t)w, (size_t)cols * 4);
    }
    double t3 = now_ms();
    ANativeWindow_unlockAndPost(win);
    double t4 = now_ms();
    pthread_mutex_unlock(&c->lock);
    LOGI("[azctl] POSTED %s at %.1f ms (frame %dx%d, buffer %dx%d stride %d format %d; geometry %d %.1f ms, lock %.1f ms, copy %.1f ms, post %.1f ms)",
         what, t4, w, h, buf.width, buf.height, buf.stride, buf.format, geometry, t1 - t0, t2 - t1, t3 - t2, t4 - t3);
    return 0;
}

static void signal_first_frame(struct ctl *c, JNIEnv *env) {
    jobject activity = c->activity->clazz;
    jclass cls = (*env)->GetObjectClass(env, activity);
    jmethodID m = (*env)->GetStaticMethodID(env, cls, "firstFramePresented", "(Landroid/app/Activity;)V");
    if (!m) {
        (*env)->ExceptionClear(env);
        LOGI("[azctl] AzulActivity has no firstFramePresented");
        return;
    }
    (*env)->CallStaticVoidMethod(env, cls, m, activity);
    if ((*env)->ExceptionCheck(env)) {
        (*env)->ExceptionClear(env);
    }
    LOGI("[azctl] firstFramePresented called at %.1f ms", now_ms());
}

static int surface_size(struct ctl *c, int *w, int *h) {
    pthread_mutex_lock(&c->lock);
    int ok = c->window != NULL;
    if (ok) {
        *w = ANativeWindow_getWidth(c->window);
        *h = ANativeWindow_getHeight(c->window);
    }
    pthread_mutex_unlock(&c->lock);
    return ok ? 0 : -1;
}

/* The pages of the last surface: what a new surface of the same size gets at once (the shell's retained frame, E47). */
struct retained {
    uint32_t *page1;
    uint32_t *page2;
    const uint32_t *last;
    int w, h;
};

static void redraw_loop(struct ctl *c, int generation, const uint32_t **last, int w, int h, int geom) {
    /* what the shell does with a RedrawNeeded: the last frame, whole; until the surface changes or the activity ends */
    for (;;) {
        pthread_mutex_lock(&c->lock);
        while (!c->quit && c->generation == generation && !c->redraw) {
            pthread_cond_wait(&c->cond, &c->lock);
        }
        int again = c->redraw;
        c->redraw = 0;
        int done = c->quit || c->generation != generation;
        pthread_mutex_unlock(&c->lock);
        if (done) {
            break;
        }
        if (again) {
            post(c, "redraw", *last, w, h, geom);
        }
    }
}

static void run_surface(struct ctl *c, JNIEnv *env, int generation, struct retained *keep) {
    int w = 0, h = 0;
    if (surface_size(c, &w, &h) != 0 || w <= 0 || h <= 0) {
        return;
    }
    long flat = prop("flat", -1);
    long p1 = prop("p1", 400);
    long p2 = prop("p2", 300);
    long geom = prop("geom", 1);
    long sig = prop("sig", 0);
    long shift = prop("shift", 128);
    long dark = prop("dark", 0);
    LOGI("[azctl] surface %dx%d gen %d at %.1f ms: flat=%ld p1=%ld p2=%ld geom=%ld sig=%ld shift=%ld dark=%ld", w, h, generation, now_ms(), flat, p1, p2, geom, sig, shift, dark);
    if (keep->last && keep->w == w && keep->h == h) {
        /* home and back: the frame that was on the screen goes on the new surface at once, before anything else */
        post(c, "retained", keep->last, w, h, (int)geom);
        redraw_loop(c, generation, &keep->last, w, h, (int)geom);
        return;
    }
    if (flat >= 0) {
        uint32_t *one = malloc((size_t)w * (size_t)h * 4);
        if (one) {
            uint32_t colour = rgba((uint32_t)(flat >> 16) & 0xFF, (uint32_t)(flat >> 8) & 0xFF, (uint32_t)flat & 0xFF);
            for (size_t i = 0; i < (size_t)w * (size_t)h; i++) {
                one[i] = colour;
            }
            post(c, "flat", one, w, h, (int)geom);
            free(one);
        }
    }
    keep->page1 = paint_page(w, h, 0, (int)dark);
    keep->page2 = p2 > 0 ? paint_page(w, h, (int)shift, (int)dark) : NULL;
    keep->w = w;
    keep->h = h;
    if (!keep->page1) {
        return;
    }
    sleep_ms(p1);
    keep->last = keep->page1;
    if (post(c, "page1", keep->page1, w, h, (int)geom) == 0) {
        sleep_ms(sig);
        signal_first_frame(c, env);
    }
    if (keep->page2) {
        sleep_ms(p2);
        if (post(c, "page2", keep->page2, w, h, (int)geom) == 0) {
            keep->last = keep->page2;
        }
    }
    redraw_loop(c, generation, &keep->last, w, h, (int)geom);
}

static void *worker(void *arg) {
    struct ctl *c = arg;
    JNIEnv *env = NULL;
    JavaVM *vm = c->activity->vm;
    (*vm)->AttachCurrentThread(vm, &env, NULL);
    int last = 0;
    struct retained keep;
    memset(&keep, 0, sizeof keep);
    for (;;) {
        pthread_mutex_lock(&c->lock);
        while (!c->quit && (c->window == NULL || c->generation == last)) {
            pthread_cond_wait(&c->cond, &c->lock);
        }
        int quit = c->quit;
        last = c->generation;
        pthread_mutex_unlock(&c->lock);
        if (quit) {
            break;
        }
        run_surface(c, env, last, &keep);
    }
    (*vm)->DetachCurrentThread(vm);
    return NULL;
}

static void on_window_created(ANativeActivity *a, ANativeWindow *w) {
    struct ctl *c = a->instance;
    pthread_mutex_lock(&c->lock);
    c->window = w;
    c->generation++;
    c->geometry_set = 0;
    c->redraw = 0;
    LOGI("[azctl] InitWindow %dx%d format %d gen %d at %.1f ms", ANativeWindow_getWidth(w), ANativeWindow_getHeight(w), ANativeWindow_getFormat(w), c->generation, now_ms());
    pthread_cond_broadcast(&c->cond);
    pthread_mutex_unlock(&c->lock);
}

static void on_window_destroyed(ANativeActivity *a, ANativeWindow *w) {
    struct ctl *c = a->instance;
    (void)w;
    pthread_mutex_lock(&c->lock); /* waits for a present in flight: the NDK's rule, not to touch the window after this returns */
    c->window = NULL;
    LOGI("[azctl] TerminateWindow at %.1f ms", now_ms());
    pthread_cond_broadcast(&c->cond);
    pthread_mutex_unlock(&c->lock);
}

static void on_window_resized(ANativeActivity *a, ANativeWindow *w) {
    LOGI("[azctl] WindowResized %dx%d at %.1f ms", ANativeWindow_getWidth(w), ANativeWindow_getHeight(w), now_ms());
    (void)a;
}

static void on_window_redraw_needed(ANativeActivity *a, ANativeWindow *w) {
    struct ctl *c = a->instance;
    (void)w;
    LOGI("[azctl] RedrawNeeded at %.1f ms", now_ms());
    pthread_mutex_lock(&c->lock);
    c->redraw = 1;
    pthread_cond_broadcast(&c->cond);
    pthread_mutex_unlock(&c->lock);
}

static void on_content_rect(ANativeActivity *a, const ARect *r) {
    LOGI("[azctl] ContentRectChanged %d,%d - %d,%d at %.1f ms", r->left, r->top, r->right, r->bottom, now_ms());
    (void)a;
}

static void on_destroy(ANativeActivity *a) {
    struct ctl *c = a->instance;
    pthread_mutex_lock(&c->lock);
    c->quit = 1;
    pthread_cond_broadcast(&c->cond);
    pthread_mutex_unlock(&c->lock);
    pthread_join(c->thread, NULL);
    LOGI("[azctl] Destroy at %.1f ms", now_ms());
}

__attribute__((visibility("default"))) void ANativeActivity_onCreate(ANativeActivity *activity, void *saved, size_t saved_size) {
    (void)saved;
    (void)saved_size;
    struct ctl *c = calloc(1, sizeof *c);
    c->activity = activity;
    pthread_mutex_init(&c->lock, NULL);
    pthread_cond_init(&c->cond, NULL);
    activity->instance = c;
    activity->callbacks->onNativeWindowCreated = on_window_created;
    activity->callbacks->onNativeWindowDestroyed = on_window_destroyed;
    activity->callbacks->onNativeWindowResized = on_window_resized;
    activity->callbacks->onNativeWindowRedrawNeeded = on_window_redraw_needed;
    activity->callbacks->onContentRectChanged = on_content_rect;
    activity->callbacks->onDestroy = on_destroy;
    LOGI("[azctl] ANativeActivity_onCreate at %.1f ms", now_ms());
    pthread_create(&c->thread, NULL, worker, c);
}
