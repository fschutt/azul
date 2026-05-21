//! Android camera capture backend via the NDK Camera2 C API (`ndk-sys` raw FFI;
//! the safe `ndk` crate doesn't wrap camera). The flow is the standard NDK
//! preview: open the default device, attach an `AImageReader` (YUV_420_888),
//! create a capture session, and repeat a preview request. State callbacks are
//! no-ops (None) - for a simple preview the calls return usable handles and the
//! callbacks only signal async state/errors. `read` pulls the latest image and
//! converts YUV_420_888 -> RGBA into the seam. Mirrors rscam (linux) / nokhwa
//! (windows) / AVFoundation (apple) into the same capture seam.

use std::os::raw::c_char;
use std::ptr;

use ndk_sys::{
    ACameraCaptureSession, ACameraCaptureSession_close, ACameraCaptureSession_setRepeatingRequest,
    ACameraCaptureSession_stateCallbacks, ACameraDevice, ACameraDevice_StateCallbacks,
    ACameraDevice_close, ACameraDevice_createCaptureRequest, ACameraDevice_createCaptureSession,
    ACameraDevice_request_template, ACameraIdList, ACameraManager,
    ACameraManager_create, ACameraManager_delete, ACameraManager_deleteCameraIdList,
    ACameraManager_getCameraIdList, ACameraManager_openCamera, ACameraOutputTarget,
    ACameraOutputTarget_create, ACaptureRequest, ACaptureRequest_addTarget,
    ACaptureSessionOutput, ACaptureSessionOutput_create, ACaptureSessionOutputContainer,
    ACaptureSessionOutputContainer_add, ACaptureSessionOutputContainer_create, AImage,
    AImage_delete, AImage_getPlaneData, AImage_getPlanePixelStride, AImage_getPlaneRowStride,
    AImageReader, AImageReader_acquireLatestImage, AImageReader_delete, AImageReader_getWindow,
    AImageReader_new,
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

/// Open camera `index` (default if out of range) at `width` x `height`,
/// YUV_420_888. Returns a boxed handle, or `0` on failure (test-pattern fallback).
pub fn open(index: u32, width: u32, height: u32) -> u64 {
    let width = if width == 0 { 640 } else { width };
    let height = if height == 0 { 480 } else { height };
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
        ACameraCaptureSession_setRepeatingRequest(session, ptr::null_mut(), 1, &mut request, ptr::null_mut());

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

/// Acquire the latest image, convert YUV_420_888 -> RGBA into `out`. Returns
/// `(width, height)`, or `(0, 0)` if no frame is ready yet (worker retries).
pub fn read(handle: u64, out: &mut Vec<u8>) -> (u32, u32) {
    let cam = match unsafe { (handle as *mut AndroidCam).as_mut() } {
        Some(c) => c,
        None => return (0, 0),
    };
    unsafe {
        let mut image: *mut AImage = ptr::null_mut();
        AImageReader_acquireLatestImage(cam.reader, &mut image);
        if image.is_null() {
            return (0, 0);
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
            return (0, 0);
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
        (cam.width, cam.height)
    }
}

/// Stop + free everything (drops the boxed `AndroidCam`).
pub fn close(handle: u64) {
    if handle == 0 {
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
        let _ = (cam.request, cam.target, cam.container, cam.output);
    }
}
