//! The watcher: a hint that wakes a pass soon after something in the folder changed, so
//! edits travel in seconds instead of at the next poll. It is never the truth: a pass always
//! scans the whole folder (the base's hashes make that cheap), and a full pass also runs at
//! start, after the computer slept, after the watcher lost events or failed, and every 15
//! minutes ([`RescanClock`]). Changes made while the app was closed are found by the start
//! pass.
//!
//! The backends: FSEvents on macOS, inotify on Linux (one watch per folder, new folders added
//! as they appear), and on other systems a light poll of the folders' times.

use std::{
    collections::BTreeSet,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant, SystemTime},
};

/// How often a full pass runs even when the watcher heard nothing.
pub const FULL_RESCAN: Duration = Duration::from_secs(15 * 60);
/// A gap between the wall clock and the monotonic clock past this means the computer slept.
pub const SLEEP_GAP: Duration = Duration::from_secs(30);

/// What the watcher heard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wake {
    /// Something changed in these folders (keys relative to the root; `""` is the root).
    Changed(BTreeSet<String>),
    /// The watcher cannot say what changed (it lost events, a folder moved, it failed): the
    /// next pass is a full one, and why.
    Rescan(String),
}

/// A running watcher of one folder; dropping it stops it.
pub struct Watcher {
    events: mpsc::Receiver<Wake>,
    stop: Arc<AtomicBool>,
    /// The backend's own handle (the FSEvents stream); dropped with the watcher.
    _backend: Option<Box<dyn Send>>,
}

impl Watcher {
    /// Starts watching `root`. A system without a watcher falls back to a poll.
    ///
    /// # Errors
    ///
    /// When the folder cannot be watched at all (it is gone).
    pub fn start(root: &Path) -> std::io::Result<Watcher> {
        std::fs::metadata(root)?;
        let (sender, events) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let backend = imp::start(root, sender, Arc::clone(&stop))?;
        Ok(Watcher {
            events,
            stop,
            _backend: backend,
        })
    }

    /// Waits up to `timeout` for the first wake, then gathers whatever else arrives within
    /// `quiet` after it (a save is several events), as one wake. `None`: nothing happened.
    #[must_use]
    pub fn wait(&self, timeout: Duration, quiet: Duration) -> Option<Wake> {
        let first = match self.events.recv_timeout(timeout) {
            Ok(wake) => wake,
            Err(mpsc::RecvTimeoutError::Timeout) => return None,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Some(Wake::Rescan(String::from("the watcher stopped")))
            }
        };
        let mut wake = first;
        let until = Instant::now() + quiet;
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            match self.events.recv_timeout(left) {
                Ok(next) => wake = merge(wake, next),
                Err(_) => break,
            }
        }
        Some(wake)
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Two wakes as one: a rescan wins, folders add up.
#[must_use]
pub fn merge(a: Wake, b: Wake) -> Wake {
    match (a, b) {
        (Wake::Rescan(why), _) | (_, Wake::Rescan(why)) => Wake::Rescan(why),
        (Wake::Changed(mut x), Wake::Changed(y)) => {
            x.extend(y);
            Wake::Changed(x)
        }
    }
}

/// The folder key of a changed `path` under `root` (`""` for the root itself), unless it
/// lies in the engine's own bookkeeping (`.azlin/`), which never wakes a pass.
#[must_use]
pub fn folder_key(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if parts.first().is_some_and(|p| p == ".azlin") {
        return None;
    }
    // The folder that holds the changed entry.
    Some(parts[..parts.len().saturating_sub(1)].join("/"))
}

/// When a full pass is due, whatever the watcher says.
#[derive(Clone, Debug)]
pub struct RescanClock {
    started: bool,
    last_full: Option<Instant>,
    /// The two clocks at the last look: a wall clock that ran ahead of the monotonic one
    /// slept in between (macOS's monotonic clock stops in sleep).
    wall: SystemTime,
    mono: Instant,
    pub interval: Duration,
}

impl Default for RescanClock {
    fn default() -> Self {
        RescanClock {
            started: false,
            last_full: None,
            wall: SystemTime::now(),
            mono: Instant::now(),
            interval: FULL_RESCAN,
        }
    }
}

impl RescanClock {
    /// Why a full pass is due now (`start`, `wake`, `interval`), or `None`. `wall` and `mono`
    /// are now on both clocks.
    pub fn due(&mut self, wall: SystemTime, mono: Instant) -> Option<&'static str> {
        let slept = {
            let wall_gap = wall.duration_since(self.wall).unwrap_or(Duration::ZERO);
            let mono_gap = mono.saturating_duration_since(self.mono);
            wall_gap.saturating_sub(mono_gap) > SLEEP_GAP
        };
        self.wall = wall;
        self.mono = mono;
        if !self.started {
            return Some("start");
        }
        if slept {
            return Some("wake");
        }
        match self.last_full {
            Some(at) if mono.saturating_duration_since(at) < self.interval => None,
            _ => Some("interval"),
        }
    }

    /// A full pass ran at `mono`.
    pub fn ran(&mut self, mono: Instant) {
        self.started = true;
        self.last_full = Some(mono);
    }
}

#[cfg(target_os = "macos")]
mod imp {
    //! FSEvents: one stream on the root, delivered on a dispatch queue of its own.

    use std::{
        collections::BTreeSet,
        ffi::{c_char, c_void, CStr, CString},
        os::unix::ffi::OsStrExt,
        path::{Path, PathBuf},
        sync::{atomic::AtomicBool, mpsc, Arc, Mutex},
    };

    use super::{folder_key, Wake};

    type CFIndex = isize;
    type FSEventStreamRef = *mut c_void;
    type Callback = extern "C" fn(
        FSEventStreamRef,
        *mut c_void,
        usize,
        *mut c_void,
        *const u32,
        *const u64,
    );

    #[repr(C)]
    struct FSEventStreamContext {
        version: CFIndex,
        info: *mut c_void,
        retain: *const c_void,
        release: *const c_void,
        copy_description: *const c_void,
    }

    const UTF8: u32 = 0x0800_0100;
    const SINCE_NOW: u64 = u64::MAX;
    const NO_DEFER: u32 = 0x02;
    const WATCH_ROOT: u32 = 0x04;
    const FILE_EVENTS: u32 = 0x10;
    const MUST_SCAN_SUBDIRS: u32 = 0x01;
    const USER_DROPPED: u32 = 0x02;
    const KERNEL_DROPPED: u32 = 0x04;
    const ROOT_CHANGED: u32 = 0x20;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFTypeArrayCallBacks: c_void;
        fn CFStringCreateWithCString(a: *const c_void, s: *const c_char, e: u32) -> *const c_void;
        fn CFArrayCreate(
            a: *const c_void,
            values: *const *const c_void,
            n: CFIndex,
            callbacks: *const c_void,
        ) -> *const c_void;
        fn CFRelease(cf: *const c_void);
    }

    #[link(name = "CoreServices", kind = "framework")]
    extern "C" {
        fn FSEventStreamCreate(
            a: *const c_void,
            callback: Callback,
            context: *mut FSEventStreamContext,
            paths: *const c_void,
            since: u64,
            latency: f64,
            flags: u32,
        ) -> FSEventStreamRef;
        fn FSEventStreamSetDispatchQueue(stream: FSEventStreamRef, queue: *mut c_void);
        fn FSEventStreamStart(stream: FSEventStreamRef) -> u8;
        fn FSEventStreamStop(stream: FSEventStreamRef);
        fn FSEventStreamInvalidate(stream: FSEventStreamRef);
        fn FSEventStreamRelease(stream: FSEventStreamRef);
    }

    extern "C" {
        fn dispatch_queue_create(label: *const c_char, attr: *const c_void) -> *mut c_void;
        fn dispatch_release(object: *mut c_void);
    }

    /// What the callback reaches through the stream's `info`.
    struct Shared {
        root: PathBuf,
        sender: Mutex<mpsc::Sender<Wake>>,
    }

    /// The stream; stopping and releasing it on drop.
    struct Stream {
        stream: FSEventStreamRef,
        queue: *mut c_void,
        /// Freed after the stream: the callback reads it until then.
        shared: *mut Shared,
    }

    // SAFETY: the stream and the queue are only touched again on drop, from whichever thread
    // drops the watcher; FSEvents' calls are thread-safe for a stream used this way.
    unsafe impl Send for Stream {}

    impl Drop for Stream {
        fn drop(&mut self) {
            // SAFETY: the stream was created and started by `start`; after Invalidate no
            // callback runs, so the shared state can go.
            unsafe {
                FSEventStreamStop(self.stream);
                FSEventStreamInvalidate(self.stream);
                FSEventStreamRelease(self.stream);
                dispatch_release(self.queue);
                drop(Box::from_raw(self.shared));
            }
        }
    }

    extern "C" fn heard(
        _stream: FSEventStreamRef,
        info: *mut c_void,
        n: usize,
        paths: *mut c_void,
        flags: *const u32,
        _ids: *const u64,
    ) {
        // SAFETY: `info` is the `Shared` the stream was made with, alive until it is
        // invalidated; without kFSEventStreamCreateFlagUseCFTypes `paths` is an array of `n`
        // C strings and `flags` of `n` flags.
        let shared = unsafe { &*(info as *const Shared) };
        let paths = paths as *const *const c_char;
        let mut folders = BTreeSet::new();
        let mut rescan = None;
        for i in 0..n {
            let (path, flag) = unsafe { (CStr::from_ptr(*paths.add(i)), *flags.add(i)) };
            if flag & (MUST_SCAN_SUBDIRS | USER_DROPPED | KERNEL_DROPPED) != 0 {
                rescan = Some("the system dropped events");
            }
            if flag & ROOT_CHANGED != 0 {
                rescan = Some("the folder itself moved or was removed");
            }
            let path = Path::new(std::ffi::OsStr::from_bytes(path.to_bytes()));
            if let Some(key) = folder_key(&shared.root, path) {
                folders.insert(key);
            }
        }
        let wake = match rescan {
            Some(why) => Wake::Rescan(why.to_string()),
            None if folders.is_empty() => return,
            None => Wake::Changed(folders),
        };
        if let Ok(sender) = shared.sender.lock() {
            let _ = sender.send(wake);
        }
    }

    pub fn start(
        root: &Path,
        sender: mpsc::Sender<Wake>,
        _stop: Arc<AtomicBool>,
    ) -> std::io::Result<Option<Box<dyn Send>>> {
        // FSEvents reports paths with symbolic links resolved (/private/var for /var).
        let root = std::fs::canonicalize(root)?;
        let c_root = CString::new(root.as_os_str().as_bytes())
            .map_err(|_| std::io::Error::other("a NUL in the folder's path"))?;
        let shared = Box::into_raw(Box::new(Shared {
            root,
            sender: Mutex::new(sender),
        }));
        let mut context = FSEventStreamContext {
            version: 0,
            info: shared.cast(),
            retain: std::ptr::null(),
            release: std::ptr::null(),
            copy_description: std::ptr::null(),
        };
        // SAFETY: plain CoreFoundation and FSEvents calls with valid arguments; every object
        // made here is released here or by `Stream`'s drop.
        unsafe {
            let path = CFStringCreateWithCString(std::ptr::null(), c_root.as_ptr(), UTF8);
            let values = [path];
            let array = CFArrayCreate(
                std::ptr::null(),
                values.as_ptr(),
                1,
                std::ptr::addr_of!(kCFTypeArrayCallBacks),
            );
            let stream = FSEventStreamCreate(
                std::ptr::null(),
                heard,
                &mut context,
                array,
                SINCE_NOW,
                0.2,
                NO_DEFER | WATCH_ROOT | FILE_EVENTS,
            );
            CFRelease(array);
            CFRelease(path);
            if stream.is_null() {
                drop(Box::from_raw(shared));
                return Err(std::io::Error::other("FSEvents refused the folder"));
            }
            let queue = dispatch_queue_create(c"io.azlin.sync.watcher".as_ptr(), std::ptr::null());
            FSEventStreamSetDispatchQueue(stream, queue);
            let handle = Stream {
                stream,
                queue,
                shared,
            };
            if FSEventStreamStart(stream) == 0 {
                return Err(std::io::Error::other("FSEvents did not start"));
            }
            Ok(Some(Box::new(handle)))
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    //! inotify: a watch on every folder, a thread reading the events.

    use std::{
        collections::{BTreeMap, BTreeSet},
        ffi::{CString, OsStr},
        os::unix::ffi::OsStrExt,
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc,
        },
    };

    use super::Wake;

    const MASK: u32 = libc::IN_CREATE
        | libc::IN_DELETE
        | libc::IN_MODIFY
        | libc::IN_CLOSE_WRITE
        | libc::IN_MOVED_FROM
        | libc::IN_MOVED_TO
        | libc::IN_ATTRIB
        | libc::IN_DELETE_SELF
        | libc::IN_MOVE_SELF
        | libc::IN_DONT_FOLLOW
        | libc::IN_ONLYDIR;

    struct Inotify {
        fd: i32,
        /// Watch descriptor -> the folder's key ("" is the root).
        watches: BTreeMap<i32, String>,
        root: PathBuf,
    }

    impl Inotify {
        /// Watches `key` and every folder below it (never through a link, never `.azlin`).
        fn add_tree(&mut self, key: &str) {
            let path = if key.is_empty() {
                self.root.clone()
            } else {
                self.root.join(key)
            };
            let Ok(c) = CString::new(path.as_os_str().as_bytes()) else {
                return;
            };
            // SAFETY: a valid descriptor and a NUL-terminated path.
            let wd = unsafe { libc::inotify_add_watch(self.fd, c.as_ptr(), MASK) };
            if wd >= 0 {
                self.watches.insert(wd, key.to_string());
            }
            let Ok(entries) = std::fs::read_dir(&path) else {
                return;
            };
            for entry in entries.flatten() {
                let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
                let name = entry.file_name().to_string_lossy().into_owned();
                if is_dir && !(key.is_empty() && name == ".azlin") {
                    let child = if key.is_empty() { name } else { format!("{key}/{name}") };
                    self.add_tree(&child);
                }
            }
        }
    }

    impl Drop for Inotify {
        fn drop(&mut self) {
            // SAFETY: the descriptor is ours.
            unsafe { libc::close(self.fd) };
        }
    }

    pub fn start(
        root: &Path,
        sender: mpsc::Sender<Wake>,
        stop: Arc<AtomicBool>,
    ) -> std::io::Result<Option<Box<dyn Send>>> {
        // SAFETY: no arguments but flags.
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut inotify = Inotify {
            fd,
            watches: BTreeMap::new(),
            root: root.to_path_buf(),
        };
        inotify.add_tree("");
        std::thread::Builder::new()
            .name(String::from("azcloud-sync-watcher"))
            .spawn(move || run(inotify, &sender, &stop))?;
        Ok(None)
    }

    fn run(mut inotify: Inotify, sender: &mpsc::Sender<Wake>, stop: &AtomicBool) {
        let mut buf = vec![0u8; 64 * 1024];
        while !stop.load(Ordering::SeqCst) {
            let mut poll = libc::pollfd {
                fd: inotify.fd,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: one valid pollfd.
            let ready = unsafe { libc::poll(&mut poll, 1, 500) };
            if ready <= 0 {
                continue;
            }
            // SAFETY: reads into a buffer of its own length.
            let n = unsafe { libc::read(inotify.fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                continue;
            }
            let mut folders = BTreeSet::new();
            let mut rescan = None;
            let mut new_dirs = Vec::new();
            let mut at = 0usize;
            let n = n as usize;
            let head = std::mem::size_of::<libc::inotify_event>();
            while at + head <= n {
                // SAFETY: the kernel wrote whole events; read_unaligned copes with the buffer's
                // alignment.
                let event: libc::inotify_event =
                    unsafe { std::ptr::read_unaligned(buf.as_ptr().add(at).cast()) };
                let name_len = event.len as usize;
                let name = &buf[at + head..(at + head + name_len).min(n)];
                let name = OsStr::from_bytes(name.split(|b| *b == 0).next().unwrap_or(&[]));
                at += head + name_len;
                if event.mask & libc::IN_Q_OVERFLOW != 0 {
                    rescan = Some("inotify's queue overflowed");
                    continue;
                }
                let Some(folder) = inotify.watches.get(&event.wd).cloned() else {
                    continue;
                };
                if event.mask & (libc::IN_DELETE_SELF | libc::IN_MOVE_SELF) != 0 && folder.is_empty() {
                    rescan = Some("the folder itself moved or was removed");
                }
                if folder.is_empty() && name == ".azlin" {
                    continue;
                }
                if event.mask & libc::IN_ISDIR != 0
                    && event.mask & (libc::IN_CREATE | libc::IN_MOVED_TO) != 0
                {
                    let name = name.to_string_lossy();
                    new_dirs.push(if folder.is_empty() {
                        name.into_owned()
                    } else {
                        format!("{folder}/{name}")
                    });
                }
                if event.mask & libc::IN_IGNORED != 0 {
                    inotify.watches.remove(&event.wd);
                }
                folders.insert(folder);
            }
            for dir in new_dirs {
                // A folder made with files in it already: its files are found by the pass.
                inotify.add_tree(&dir);
            }
            let wake = match rescan {
                Some(why) => Wake::Rescan(why.to_string()),
                None if folders.is_empty() => continue,
                None => Wake::Changed(folders),
            };
            if sender.send(wake).is_err() {
                return;
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod imp {
    //! A poll: every two seconds the folders' modification times and the files' sizes and
    //! times are compared with the last look (a file changed in place changes its folder's
    //! listing only on some systems, so files are looked at too).

    use std::{
        collections::{BTreeMap, BTreeSet},
        path::Path,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc,
        },
        time::{Duration, SystemTime},
    };

    use super::Wake;

    type Look = BTreeMap<String, (u64, Option<SystemTime>)>;

    fn look(root: &Path, key: &str, out: &mut Look) {
        let dir = if key.is_empty() {
            root.to_path_buf()
        } else {
            root.join(key)
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if key.is_empty() && name == ".azlin" {
                continue;
            }
            let child = if key.is_empty() { name } else { format!("{key}/{name}") };
            let Ok(meta) = entry.metadata() else { continue };
            out.insert(child.clone(), (meta.len(), meta.modified().ok()));
            if meta.is_dir() {
                look(root, &child, out);
            }
        }
    }

    fn folder_of(key: &str) -> String {
        key.rfind('/').map_or_else(String::new, |i| key[..i].to_string())
    }

    pub fn start(
        root: &Path,
        sender: mpsc::Sender<Wake>,
        stop: Arc<AtomicBool>,
    ) -> std::io::Result<Option<Box<dyn Send>>> {
        let root = root.to_path_buf();
        std::thread::Builder::new()
            .name(String::from("azcloud-sync-watcher"))
            .spawn(move || {
                let mut last = Look::new();
                look(&root, "", &mut last);
                while !stop.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_secs(2));
                    let mut now = Look::new();
                    look(&root, "", &mut now);
                    let changed: BTreeSet<String> = now
                        .iter()
                        .filter(|(k, v)| last.get(*k) != Some(v))
                        .map(|(k, _)| folder_of(k))
                        .chain(last.keys().filter(|k| !now.contains_key(*k)).map(|k| folder_of(k)))
                        .collect();
                    last = now;
                    if !changed.is_empty() && sender.send(Wake::Changed(changed)).is_err() {
                        return;
                    }
                }
            })?;
        Ok(None)
    }
}
