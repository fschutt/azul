//! Windows camera capture backend.
//!
//! Two implementations behind the `capture_common` seam (open/read/close):
//!
//! - **default** → Media Foundation: a source reader on the chosen video capture device, with
//!   MF's own video processing decoding MJPG/NV12/YUY2 to RGB32. `mfplat`, `mf` and `mfreadwrite`
//!   are loaded at runtime, so an N edition without the Media Feature Pack still starts (the
//!   widget keeps its test pattern).
//! - **`camera-native` feature ON** → the `nokhwa` backend with RGBA decode. nokhwa's `decoding`
//!   feature pulls `mozjpeg-sys`, whose build script C-compiles libjpeg-turbo.

#[cfg(feature = "camera-native")]
mod native {
    use azul_layout::widgets::capture_common::{CaptureRead, CaptureRequest};
    use nokhwa::{
        pixel_format::RgbAFormat,
        utils::{
            CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType,
            Resolution,
        },
        Camera,
    };

    /// Live capture state behind the seam's `u64` handle (worker-thread-local).
    struct NokhwaCam {
        camera: Camera,
    }

    /// Open camera `index` at the format closest to the requested size + fps
    /// (nokhwa negotiates; a zero size asks for the highest frame rate the
    /// device offers). Returns a boxed handle, or `0` on failure (worker falls
    /// back to the test pattern).
    pub fn open(request: &CaptureRequest) -> u64 {
        let index = request.index;
        let wanted = if request.width > 0 && request.height > 0 {
            RequestedFormatType::Closest(CameraFormat::new(
                Resolution::new(request.width, request.height),
                FrameFormat::MJPEG,
                request.fps_or(30),
            ))
        } else {
            RequestedFormatType::AbsoluteHighestFrameRate
        };
        let format = RequestedFormat::new::<RgbAFormat>(wanted);
        let mut camera = match Camera::new(CameraIndex::Index(index), format) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        if camera.open_stream().is_err() {
            return 0;
        }
        Box::into_raw(Box::new(NokhwaCam { camera })) as u64
    }

    /// Capture + decode the next frame to tightly-packed RGBA8 into `out`.
    /// `Frame` on success; a frame that failed to arrive is `Idle` (the
    /// worker keeps polling), a frame that failed to DECODE is `Ended`.
    pub fn read(handle: u64, out: &mut Vec<u8>) -> CaptureRead {
        let cam = match unsafe { (handle as *mut NokhwaCam).as_mut() } {
            Some(c) => c,
            None => return CaptureRead::Ended,
        };
        let frame = match cam.camera.frame() {
            Ok(f) => f,
            Err(_) => return CaptureRead::Idle,
        };
        let img = match frame.decode_image::<RgbAFormat>() {
            Ok(i) => i,
            Err(_) => return CaptureRead::Ended,
        };
        let (w, h) = (img.width(), img.height());
        out.clear();
        out.extend_from_slice(img.as_raw());
        CaptureRead::Frame {
            width: w,
            height: h,
        }
    }

    /// Stop streaming + free the capture (drops the boxed `NokhwaCam`).
    pub fn close(handle: u64) {
        if handle != 0 {
            unsafe {
                drop(Box::from_raw(handle as *mut NokhwaCam));
            }
        }
    }
}

#[cfg(feature = "camera-native")]
pub use native::{close, open, read};

#[cfg(not(feature = "camera-native"))]
mod media_foundation {
    use core::ffi::c_void;

    use azul_layout::widgets::capture_common::{CaptureRead, CaptureRequest};
    use windows::{
        core::{Interface, GUID, HRESULT, PWSTR},
        Win32::{
            Foundation::E_ACCESSDENIED,
            Media::MediaFoundation::{
                IMF2DBuffer, IMFActivate, IMFAttributes, IMFMediaSource, IMFMediaType, IMFSample,
                IMFSourceReader, MFMediaType_Video, MFVideoFormat_RGB32,
                MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID, MF_LOW_LATENCY,
                MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
                MF_MT_SUBTYPE, MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED,
                MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR,
                MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING,
                MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION,
            },
            System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED},
        },
    };

    const STREAM: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

    /// The flat Media Foundation entry points; everything else is COM vtable calls.
    struct MfApi {
        startup: unsafe extern "system" fn(u32, u32) -> HRESULT,
        shutdown: unsafe extern "system" fn() -> HRESULT,
        create_attributes: unsafe extern "system" fn(*mut *mut c_void, u32) -> HRESULT,
        create_media_type: unsafe extern "system" fn(*mut *mut c_void) -> HRESULT,
        enum_device_sources:
            unsafe extern "system" fn(*mut c_void, *mut *mut *mut c_void, *mut u32) -> HRESULT,
        create_source_reader:
            unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> HRESULT,
    }

    fn api() -> Result<&'static MfApi, String> {
        static API: std::sync::OnceLock<Result<MfApi, String>> = std::sync::OnceLock::new();
        API.get_or_init(|| unsafe {
            let load = |name: &str| -> Result<&'static libloading::Library, String> {
                libloading::Library::new(name)
                    .map(|lib| &*Box::leak(Box::new(lib)))
                    .map_err(|e| format!("{name} unavailable (no Media Feature Pack?): {e}"))
            };
            let mfplat = load("mfplat.dll")?;
            let mf = load("mf.dll")?;
            let mfreadwrite = load("mfreadwrite.dll")?;
            let sym = |lib: &'static libloading::Library, name: &str| -> Result<*const c_void, String> {
                let mut z = name.as_bytes().to_vec();
                z.push(0);
                lib.get::<*const c_void>(&z).map(|s| *s).map_err(|e| format!("{name}: {e}"))
            };
            Ok(MfApi {
                startup: core::mem::transmute(sym(mfplat, "MFStartup")?),
                shutdown: core::mem::transmute(sym(mfplat, "MFShutdown")?),
                create_attributes: core::mem::transmute(sym(mfplat, "MFCreateAttributes")?),
                create_media_type: core::mem::transmute(sym(mfplat, "MFCreateMediaType")?),
                enum_device_sources: core::mem::transmute(sym(mf, "MFEnumDeviceSources")?),
                create_source_reader: core::mem::transmute(sym(
                    mfreadwrite,
                    "MFCreateSourceReaderFromMediaSource",
                )?),
            })
        })
        .as_ref()
        .map_err(Clone::clone)
    }

    struct MfCamera {
        api: &'static MfApi,
        reader: IMFSourceReader,
        source: IMFMediaSource,
        width: u32,
        height: u32,
        /// Bytes per row as MF reports it; negative for bottom-up RGB.
        stride: i32,
    }

    impl Drop for MfCamera {
        fn drop(&mut self) {
            unsafe {
                self.source.Shutdown().ok();
                let _ = (self.api.shutdown)();
            }
        }
    }

    fn describe(hr: HRESULT) -> String {
        if hr == E_ACCESSDENIED {
            "access denied (Settings > Privacy & security > Camera)".to_string()
        } else {
            format!("{:#010x} {}", hr.0 as u32, hr.message())
        }
    }

    unsafe fn attributes(api: &MfApi, capacity: u32) -> Result<IMFAttributes, String> {
        unsafe {
            let mut raw = core::ptr::null_mut();
            (api.create_attributes)(&mut raw, capacity)
                .ok()
                .map_err(|e| describe(e.code()))?;
            Ok(IMFAttributes::from_raw(raw))
        }
    }

    /// Every video capture device, in the order Windows lists them.
    unsafe fn devices(api: &MfApi) -> Result<Vec<IMFActivate>, String> {
        unsafe {
            let filter = attributes(api, 1)?;
            filter
                .SetGUID(
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                )
                .map_err(|e| describe(e.code()))?;
            let mut array: *mut *mut c_void = core::ptr::null_mut();
            let mut count = 0u32;
            (api.enum_device_sources)(filter.as_raw(), &mut array, &mut count)
                .ok()
                .map_err(|e| describe(e.code()))?;
            let list = (0..count as usize)
                .map(|i| IMFActivate::from_raw(*array.add(i)))
                .collect();
            CoTaskMemFree(Some(array as *const c_void));
            Ok(list)
        }
    }

    unsafe fn friendly_name(device: &IMFActivate) -> String {
        unsafe {
            let mut name = PWSTR::null();
            let mut len = 0u32;
            if device
                .GetAllocatedString(&MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, &mut name, &mut len)
                .is_err()
            {
                return String::from("camera");
            }
            let text = name.to_string().unwrap_or_default();
            CoTaskMemFree(Some(name.0 as *const c_void));
            text
        }
    }

    fn packed_pair(ty: &IMFMediaType, key: &GUID) -> Option<(u32, u32)> {
        let v = unsafe { ty.GetUINT64(key) }.ok()?;
        Some(((v >> 32) as u32, v as u32))
    }

    /// The native mode closest to the request: the smallest one covering the wanted size, at the
    /// wanted frame rate if the device has it.
    unsafe fn pick_native_type(
        reader: &IMFSourceReader,
        request: &CaptureRequest,
    ) -> Option<IMFMediaType> {
        unsafe {
            let (want_w, want_h) = if request.width > 0 && request.height > 0 {
                (request.width, request.height)
            } else {
                (1280, 720)
            };
            let want_fps = request.fps_or(30);
            let mut best: Option<((bool, u64, u32), IMFMediaType)> = None;
            for i in 0.. {
                let Ok(ty) = reader.GetNativeMediaType(STREAM, i) else {
                    break;
                };
                let Some((w, h)) = packed_pair(&ty, &MF_MT_FRAME_SIZE) else {
                    continue;
                };
                let fps = packed_pair(&ty, &MF_MT_FRAME_RATE)
                    .map(|(num, den)| if den == 0 { 0 } else { num / den })
                    .unwrap_or(0);
                let covers = w >= want_w && h >= want_h;
                let area = u64::from(w) * u64::from(h);
                let score = (
                    !covers,
                    if covers { area } else { u64::MAX - area },
                    fps.abs_diff(want_fps),
                );
                if best.as_ref().map_or(true, |(b, _)| score < *b) {
                    best = Some((score, ty));
                }
            }
            best.map(|(_, ty)| ty)
        }
    }

    impl MfCamera {
        unsafe fn open(request: &CaptureRequest) -> Result<(Self, String), String> {
            unsafe {
                let api = api()?;
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                (api.startup)(MF_VERSION, 0)
                    .ok()
                    .map_err(|e| describe(e.code()))?;
                // MFShutdown balances MFStartup on every error path; MfCamera's Drop does it after.
                let result = Self::open_started(api, request);
                if result.is_err() {
                    let _ = (api.shutdown)();
                }
                result
            }
        }

        unsafe fn open_started(
            api: &'static MfApi,
            request: &CaptureRequest,
        ) -> Result<(Self, String), String> {
            unsafe {
                let devices = devices(api)?;
                let device = devices
                    .get(request.index as usize)
                    .or(devices.first())
                    .ok_or("no video capture device")?;
                let name = friendly_name(device);
                let source: IMFMediaSource = device.ActivateObject().map_err(|e| describe(e.code()))?;
                match Self::configure(api, &source, request) {
                    Ok((reader, (width, height, stride))) => Ok((
                        MfCamera {
                            api,
                            reader,
                            source,
                            width,
                            height,
                            stride,
                        },
                        name,
                    )),
                    Err(e) => {
                        source.Shutdown().ok();
                        Err(e)
                    }
                }
            }
        }

        /// A source reader on `source` delivering RGB32 at the native mode closest to the request.
        unsafe fn configure(
            api: &MfApi,
            source: &IMFMediaSource,
            request: &CaptureRequest,
        ) -> Result<(IMFSourceReader, (u32, u32, i32)), String> {
            unsafe {
                let config = attributes(api, 2)?;
                config.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1).ok();
                config.SetUINT32(&MF_LOW_LATENCY, 1).ok();
                let mut raw = core::ptr::null_mut();
                (api.create_source_reader)(source.as_raw(), config.as_raw(), &mut raw)
                    .ok()
                    .map_err(|e| describe(e.code()))?;
                let reader = IMFSourceReader::from_raw(raw);
                if let Some(native) = pick_native_type(&reader, request) {
                    reader.SetCurrentMediaType(STREAM, None, &native).ok();
                }
                let mut raw = core::ptr::null_mut();
                (api.create_media_type)(&mut raw)
                    .ok()
                    .map_err(|e| describe(e.code()))?;
                let output = IMFMediaType::from_raw(raw);
                output
                    .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                    .map_err(|e| describe(e.code()))?;
                output
                    .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                    .map_err(|e| describe(e.code()))?;
                reader
                    .SetCurrentMediaType(STREAM, None, &output)
                    .map_err(|e| format!("no RGB32 conversion: {}", describe(e.code())))?;
                let format = current_format(&reader)?;
                Ok((reader, format))
            }
        }

        unsafe fn read(&mut self, out: &mut Vec<u8>) -> CaptureRead {
            unsafe {
                let mut flags = 0u32;
                let mut sample: Option<IMFSample> = None;
                if self
                    .reader
                    .ReadSample(STREAM, 0, None, Some(&mut flags), None, Some(&mut sample))
                    .is_err()
                {
                    return CaptureRead::Ended;
                }
                let flags = flags as i32;
                if flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 | MF_SOURCE_READERF_ERROR.0) != 0 {
                    return CaptureRead::Ended;
                }
                if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 != 0 {
                    match current_format(&self.reader) {
                        Ok((w, h, stride)) => (self.width, self.height, self.stride) = (w, h, stride),
                        Err(_) => return CaptureRead::Ended,
                    }
                }
                let Some(sample) = sample else {
                    return CaptureRead::Idle;
                };
                let Ok(buffer) = sample.ConvertToContiguousBuffer() else {
                    return CaptureRead::Idle;
                };
                let (w, h) = (self.width as usize, self.height as usize);
                out.resize(w * h * 4, 0);
                if let Ok(planar) = buffer.cast::<IMF2DBuffer>() {
                    let mut scan0 = core::ptr::null_mut();
                    let mut pitch = 0i32;
                    if planar.Lock2D(&mut scan0, &mut pitch).is_err() {
                        return CaptureRead::Idle;
                    }
                    copy_bgrx_rows(scan0, pitch as isize, w, h, out);
                    planar.Unlock2D().ok();
                } else {
                    let mut base = core::ptr::null_mut();
                    let mut len = 0u32;
                    if buffer.Lock(&mut base, None, Some(&mut len)).is_err() {
                        return CaptureRead::Idle;
                    }
                    if (len as usize) >= w * h * 4 {
                        let pitch = self.stride as isize;
                        let scan0 = if pitch < 0 {
                            base.add((h - 1) * w * 4)
                        } else {
                            base
                        };
                        copy_bgrx_rows(scan0, pitch, w, h, out);
                    }
                    buffer.Unlock().ok();
                }
                CaptureRead::Frame {
                    width: self.width,
                    height: self.height,
                }
            }
        }
    }

    /// Width, height and stride of the reader's current output type.
    unsafe fn current_format(reader: &IMFSourceReader) -> Result<(u32, u32, i32), String> {
        unsafe {
            let current = reader
                .GetCurrentMediaType(STREAM)
                .map_err(|e| describe(e.code()))?;
            let (w, h) = packed_pair(&current, &MF_MT_FRAME_SIZE).ok_or("no frame size")?;
            let stride = current
                .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                .map(|s| s as i32)
                .unwrap_or(-(w as i32 * 4));
            Ok((w, h, stride))
        }
    }

    /// RGB32 rows (B, G, R, X) from `scan0`, `pitch` bytes apart (negative walks up), to RGBA.
    unsafe fn copy_bgrx_rows(scan0: *const u8, pitch: isize, w: usize, h: usize, out: &mut [u8]) {
        unsafe {
            for y in 0..h {
                let src = core::slice::from_raw_parts(scan0.offset(pitch * y as isize), w * 4);
                let dst = &mut out[y * w * 4..(y + 1) * w * 4];
                for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                    d[0] = s[2];
                    d[1] = s[1];
                    d[2] = s[0];
                    d[3] = 255;
                }
            }
        }
    }

    pub fn open(request: &CaptureRequest) -> u64 {
        match unsafe { MfCamera::open(request) } {
            Ok((camera, name)) => {
                crate::plog_info!(
                    "[camera] Media Foundation: {name} at {}x{}",
                    camera.width,
                    camera.height
                );
                Box::into_raw(Box::new(camera)) as u64
            }
            Err(e) => {
                crate::plog_warn!("[camera] Media Foundation could not open a camera: {e}");
                0
            }
        }
    }

    pub fn read(handle: u64, out: &mut Vec<u8>) -> CaptureRead {
        match unsafe { (handle as *mut MfCamera).as_mut() } {
            Some(camera) => unsafe { camera.read(out) },
            None => CaptureRead::Ended,
        }
    }

    pub fn close(handle: u64) {
        if handle != 0 {
            unsafe { drop(Box::from_raw(handle as *mut MfCamera)) };
        }
    }
}

#[cfg(not(feature = "camera-native"))]
pub use media_foundation::{close, open, read};
