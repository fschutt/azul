//! Android camera capture backend via the NDK Camera2 C API (`ndk-sys` raw FFI;
//! the safe `ndk` crate doesn't wrap camera). The flow is the standard NDK
//! preview: open the default device, attach an `AImageReader` (YUV_420_888),
//! create a capture session, and repeat a preview request. State callbacks are
//! no-ops (None) - for a simple preview the calls return usable handles and the
//! callbacks only signal async state/errors. `read` pulls the latest image and
//! converts YUV_420_888 -> RGBA into the seam. Mirrors libv4l2 (linux) / nokhwa
//! (windows) / AVFoundation (apple) into the same capture seam.
//!
//! The camera is a runtime permission, and a phone never shows a test pattern in the place of a camera: `open`
//! asks ONCE (`permission::android::ensure`: the dialog appears when the tile is mounted, not at launch, and a
//! refusal is remembered), waits for the answer, and when there is no camera to open - refused, absent, in use - it
//! answers [`NOT_OPENED`], a source that delivers nothing, so the tile keeps its placeholder. The app learns why
//! from `get_permission_status(Camera)`; the log line says it too.

use std::{os::raw::c_char, ptr};

use azul_layout::managers::permission::Capability;

use ndk_sys::{
    ACameraCaptureSession, ACameraCaptureSession_close, ACameraCaptureSession_setRepeatingRequest,
    ACameraCaptureSession_stateCallbacks, ACameraDevice, ACameraDevice_StateCallbacks,
    ACameraDevice_close, ACameraDevice_createCaptureRequest, ACameraDevice_createCaptureSession,
    ACameraDevice_request_template, ACameraIdList, ACameraManager, ACameraManager_create,
    ACameraManager_delete, ACameraManager_deleteCameraIdList, ACameraManager_getCameraIdList,
    ACameraManager_openCamera, ACameraOutputTarget, ACameraOutputTarget_create, ACaptureRequest,
    ACaptureRequest_addTarget, ACaptureRequest_free, ACaptureSessionOutput,
    ACaptureSessionOutputContainer, ACaptureSessionOutputContainer_add,
    ACaptureSessionOutputContainer_create, ACaptureSessionOutputContainer_free,
    ACaptureSessionOutput_create, ACaptureSessionOutput_free, ACameraOutputTarget_free, AImage,
    AImageReader, AImageReader_acquireLatestImage,
    AImageReader_delete, AImageReader_getWindow, AImageReader_new, AImage_delete,
    AImage_getPlaneData, AImage_getPlanePixelStride, AImage_getPlaneRowStride,
};

const AIMAGE_FORMAT_YUV_420_888: i32 = 0x23;

/// Live capture state behind the seam's `u64` handle (worker-thread-local).
struct AndroidCam {
    manager: *mut ACameraManager,
    device: *mut ACameraDevice,
    session: *mut ACameraCaptureSession,
    reader: *mut AImageReader,
    request: *mut ACaptureRequest,
    target: *mut ACameraOutputTarget,
    container: *mut ACaptureSessionOutputContainer,
    output: *mut ACaptureSessionOutput,
    width: u32,
    height: u32,
}

/// The handle of a camera that was not opened. Not `0`, which would make the worker put its colour-cycle TEST PATTERN
/// in the place of the camera: reads of this handle end at once, so the tile keeps its placeholder.
const NOT_OPENED: u64 = 1;

/// Open camera `index` at the request's size: ask for the permission first (once, waiting for the answer), then
/// [`open_device`]. A camera that is not opened is [`NOT_OPENED`], never `0`.
pub fn open(request: &azul_layout::widgets::capture_common::CaptureRequest) -> u64 {
    let state = crate::desktop::extra::permission::android::ensure(
        Capability::Camera,
        crate::desktop::extra::permission::android::DIALOG_PATIENCE,
    );
    if !state.is_granted() {
        crate::plog_warn!(
            "[camera] not opened: the camera permission is {:?} (the tile stays empty; the app reads the state with get_permission_status(Camera) and offers the settings)",
            state
        );
        return NOT_OPENED;
    }
    match open_device(request) {
        0 => {
            crate::plog_warn!(
                "[camera] the camera could not be opened (absent, in use by another app, or the Camera2 session failed): the tile stays empty, no test pattern stands in for it"
            );
            NOT_OPENED
        }
        handle => handle,
    }
}

/// Open camera `index` (default if out of range) at `width` x `height`,
/// YUV_420_888. Returns a boxed handle, or `0` on failure.
fn open_device(request: &azul_layout::widgets::capture_common::CaptureRequest) -> u64 {
    let index = request.index;
    let width = if request.width == 0 {
        640
    } else {
        request.width
    };
    let height = if request.height == 0 {
        480
    } else {
        request.height
    };
    unsafe {
        let manager = ACameraManager_create();
        if manager.is_null() {
            return 0;
        }
        let mut id_list: *mut ACameraIdList = ptr::null_mut();
        ACameraManager_getCameraIdList(manager, &mut id_list);
        if id_list.is_null() {
            ACameraManager_delete(manager);
            return 0;
        }
        let n = (*id_list).numCameras;
        if n <= 0 {
            ACameraManager_deleteCameraIdList(id_list);
            ACameraManager_delete(manager);
            return 0;
        }
        let idx = (index as i32).clamp(0, n - 1) as isize;
        let camera_id: *const c_char = *(*id_list).cameraIds.offset(idx);

        let mut dev_cb = ACameraDevice_StateCallbacks {
            context: ptr::null_mut(),
            onDisconnected: None,
            onError: None,
        };
        let mut device: *mut ACameraDevice = ptr::null_mut();
        ACameraManager_openCamera(manager, camera_id, &mut dev_cb, &mut device);
        ACameraManager_deleteCameraIdList(id_list);
        if device.is_null() {
            ACameraManager_delete(manager);
            return 0;
        }

        let mut reader: *mut AImageReader = ptr::null_mut();
        AImageReader_new(
            width as i32,
            height as i32,
            AIMAGE_FORMAT_YUV_420_888,
            2,
            &mut reader,
        );
        if reader.is_null() {
            ACameraDevice_close(device);
            ACameraManager_delete(manager);
            return 0;
        }
        let mut window = ptr::null_mut();
        AImageReader_getWindow(reader, &mut window);

        let mut request: *mut ACaptureRequest = ptr::null_mut();
        ACameraDevice_createCaptureRequest(
            device,
            ACameraDevice_request_template::TEMPLATE_PREVIEW,
            &mut request,
        );
        let mut target: *mut ACameraOutputTarget = ptr::null_mut();
        ACameraOutputTarget_create(window, &mut target);
        ACaptureRequest_addTarget(request, target);

        let mut container: *mut ACaptureSessionOutputContainer = ptr::null_mut();
        ACaptureSessionOutputContainer_create(&mut container);
        let mut output: *mut ACaptureSessionOutput = ptr::null_mut();
        ACaptureSessionOutput_create(window, &mut output);
        ACaptureSessionOutputContainer_add(container, output);

        let sess_cb = ACameraCaptureSession_stateCallbacks {
            context: ptr::null_mut(),
            onClosed: None,
            onReady: None,
            onActive: None,
        };
        let mut session: *mut ACameraCaptureSession = ptr::null_mut();
        ACameraDevice_createCaptureSession(device, container, &sess_cb, &mut session);
        if session.is_null() {
            ACameraDevice_close(device);
            AImageReader_delete(reader);
            ACameraManager_delete(manager);
            return 0;
        }
        ACameraCaptureSession_setRepeatingRequest(
            session,
            ptr::null_mut(),
            1,
            &mut request,
            ptr::null_mut(),
        );

        Box::into_raw(Box::new(AndroidCam {
            manager,
            device,
            session,
            reader,
            request,
            target,
            container,
            output,
            width,
            height,
        })) as u64
    }
}

/// Acquire the latest image, convert YUV_420_888 -> RGBA into `out`. `Frame`
/// on success; `Idle` when no frame is ready yet (the worker polls again —
/// this used to return the end-of-stream signal, which stopped the worker on
/// the first early read), `Ended` on a broken image.
pub fn read(handle: u64, out: &mut Vec<u8>) -> azul_layout::widgets::capture_common::CaptureRead {
    use azul_layout::widgets::capture_common::CaptureRead;
    if handle <= NOT_OPENED {
        return CaptureRead::Ended;
    }
    let cam = match unsafe { (handle as *mut AndroidCam).as_mut() } {
        Some(c) => c,
        None => return CaptureRead::Ended,
    };
    unsafe {
        let mut image: *mut AImage = ptr::null_mut();
        AImageReader_acquireLatestImage(cam.reader, &mut image);
        if image.is_null() {
            // Not ready: yield briefly so the worker does not spin.
            std::thread::sleep(std::time::Duration::from_millis(5));
            return CaptureRead::Idle;
        }
        let (w, h) = (cam.width as usize, cam.height as usize);
        let mut y_ptr: *mut u8 = ptr::null_mut();
        let mut y_len = 0i32;
        let mut u_ptr: *mut u8 = ptr::null_mut();
        let mut u_len = 0i32;
        let mut v_ptr: *mut u8 = ptr::null_mut();
        let mut v_len = 0i32;
        AImage_getPlaneData(image, 0, &mut y_ptr, &mut y_len);
        AImage_getPlaneData(image, 1, &mut u_ptr, &mut u_len);
        AImage_getPlaneData(image, 2, &mut v_ptr, &mut v_len);
        let mut y_row = 0i32;
        let mut u_row = 0i32;
        let mut v_row = 0i32;
        let mut u_pix = 1i32;
        let mut v_pix = 1i32;
        AImage_getPlaneRowStride(image, 0, &mut y_row);
        AImage_getPlaneRowStride(image, 1, &mut u_row);
        AImage_getPlaneRowStride(image, 2, &mut v_row);
        AImage_getPlanePixelStride(image, 1, &mut u_pix);
        AImage_getPlanePixelStride(image, 2, &mut v_pix);
        if y_ptr.is_null() || u_ptr.is_null() || v_ptr.is_null() {
            AImage_delete(image);
            return CaptureRead::Ended;
        }
        out.clear();
        out.resize(w * h * 4, 0);
        for j in 0..h {
            for i in 0..w {
                let yy = *y_ptr.add(j * y_row as usize + i) as f32;
                let uo = (j / 2) * u_row as usize + (i / 2) * u_pix as usize;
                let vo = (j / 2) * v_row as usize + (i / 2) * v_pix as usize;
                let uu = *u_ptr.add(uo) as f32 - 128.0;
                let vv = *v_ptr.add(vo) as f32 - 128.0;
                let o = (j * w + i) * 4;
                out[o] = (yy + 1.402 * vv).clamp(0.0, 255.0) as u8;
                out[o + 1] = (yy - 0.344 * uu - 0.714 * vv).clamp(0.0, 255.0) as u8;
                out[o + 2] = (yy + 1.772 * uu).clamp(0.0, 255.0) as u8;
                out[o + 3] = 255;
            }
        }
        AImage_delete(image);
        CaptureRead::Frame {
            width: cam.width,
            height: cam.height,
        }
    }
}

/// Stop + free everything (drops the boxed `AndroidCam`).
pub fn close(handle: u64) {
    if handle <= NOT_OPENED {
        return;
    }
    let cam = unsafe { Box::from_raw(handle as *mut AndroidCam) };
    unsafe {
        if !cam.session.is_null() {
            ACameraCaptureSession_close(cam.session);
        }
        if !cam.device.is_null() {
            ACameraDevice_close(cam.device);
        }
        if !cam.reader.is_null() {
            AImageReader_delete(cam.reader);
        }
        if !cam.manager.is_null() {
            ACameraManager_delete(cam.manager);
        }
        // After the session (it holds them): the request, its target and the session's output container were never freed.
        if !cam.request.is_null() {
            ACaptureRequest_free(cam.request);
        }
        if !cam.target.is_null() {
            ACameraOutputTarget_free(cam.target);
        }
        if !cam.container.is_null() {
            ACaptureSessionOutputContainer_free(cam.container);
        }
        if !cam.output.is_null() {
            ACaptureSessionOutput_free(cam.output);
        }
    }
}
