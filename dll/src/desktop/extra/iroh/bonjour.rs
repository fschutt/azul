//! Local-network discovery through the system's Bonjour (`dns_sd`, in libSystem): how an iPhone finds
//! the Mac that announced itself, and the lookup the engine uses on iOS (`cfg(az_iroh_bonjour)`,
//! see build.rs; macOS compiles it for the test that checks it finds what the mDNS lookup announces).
//!
//! Why not the mDNS lookup of every other platform: on iOS an app may send or receive multicast
//! only with the restricted `com.apple.developer.networking.multicast` entitlement, which Apple
//! grants a team on request and a personal (free) team cannot have (TN3179). Bonjour operations on
//! a service type that the app lists in `NSBonjourServices` need only the Local Network permission.
//! So the phone browses `_irohv1._udp`, queries the SRV records of the instance named by the
//! endpoint id, and asks the system for the addresses of their hosts. An iPhone is a client of the
//! Mac, so it announces nothing ([`LocalDiscovery::can_announce`]).
//!
//! The Info.plist of an app that uses an endpoint on iOS needs `NSLocalNetworkUsageDescription`
//! (the text of the permission prompt) and `NSBonjourServices` = `["_irohv1._udp"]`:
//! `AZ_IOS_LOCAL_NETWORK` of scripts/build-ios.sh writes both (doc/guide/en/deploying/mobile.md).
//! Without them the system answers `kDNSServiceErr_PolicyDenied` and the lookup says so in words.
//!
//! One worker thread per question, one `dns_sd` connection shared by the browse, the SRV query and
//! the address queries, and every callback runs on that thread, inside `DNSServiceProcessResult`.
//!
//! Besides the questions there is one WATCH per engine: a thread that browses `_irohv1._udp` for as
//! long as the engine lives and tells the engine's list of nearby endpoints ([`Nearby`]) of every
//! instance that is named by an endpoint id: its SRV records (the ports), the address records of
//! their hosts, and its TXT record (the text it advertises, the `user-data` attribute), so a phone
//! lists the Macs on the network by name. When an instance goes, the list hears that too.

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{c_char, c_int, c_void, CStr, CString},
    net::IpAddr,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use iroh::{
    address_lookup::{
        AddressLookup, EndpointData, EndpointInfo, Error as LookupError, Item, UserData,
    },
    Endpoint, EndpointId,
};
use n0_future::boxed::BoxStream;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use super::{
    discovery::{Held, LocalDiscovery, LocalLookup},
    dns_wire::{
        assemble, describe_error, host_key, instance_fullname, instance_label, ip_from_sockaddr,
        parse_srv, txt_value, DOMAIN, SERVICE_TYPE,
    },
    nearby::{Nearby, MAX_NEARBY_PEERS},
};

/// How long one question may run before it gives up: a Mac that is on the network answers within
/// a second or two; the system's own browse keeps asking at growing intervals.
const GIVE_UP_AFTER: Duration = Duration::from_secs(12);

/// How often a worker looks up from the system's socket to see whether its stream was dropped.
const POLL_MS: c_int = 200;

// ----- the C interface of dns_sd.h (libSystem; no framework to link) -----

type DnsServiceRef = *mut c_void;
type Flags = u32;
type ErrorType = i32;

const FLAGS_MORE_COMING: Flags = 0x1;
const FLAGS_ADD: Flags = 0x2;
const FLAGS_SHARE_CONNECTION: Flags = 0x4000;
const INTERFACE_ANY: u32 = 0;
const TYPE_SRV: u16 = 33;
const TYPE_TXT: u16 = 16;
const CLASS_IN: u16 = 1;
const PROTOCOL_IPV4: u32 = 0x01;
const PROTOCOL_IPV6: u32 = 0x02;
const NO_ERROR: ErrorType = 0;
/// `kDNSServiceErr_PolicyDenied`: the app may not use the local network (permission or Info.plist).
const ERR_POLICY_DENIED: ErrorType = -65570;
/// `kDNSServiceErr_ServiceNotRunning`.
const ERR_NOT_RUNNING: ErrorType = -65563;
/// The most SRV targets one instance may have (an endpoint has a handful of addresses).
const MAX_TARGETS: usize = 16;

type BrowseReply = extern "C" fn(
    DnsServiceRef,
    Flags,
    u32,
    ErrorType,
    *const c_char,
    *const c_char,
    *const c_char,
    *mut c_void,
);
type QueryRecordReply = extern "C" fn(
    DnsServiceRef,
    Flags,
    u32,
    ErrorType,
    *const c_char,
    u16,
    u16,
    u16,
    *const c_void,
    u32,
    *mut c_void,
);
type GetAddrInfoReply = extern "C" fn(
    DnsServiceRef,
    Flags,
    u32,
    ErrorType,
    *const c_char,
    *const c_void,
    u32,
    *mut c_void,
);

extern "C" {
    fn DNSServiceCreateConnection(sd_ref: *mut DnsServiceRef) -> ErrorType;
    fn DNSServiceRefSockFD(sd_ref: DnsServiceRef) -> c_int;
    fn DNSServiceProcessResult(sd_ref: DnsServiceRef) -> ErrorType;
    fn DNSServiceRefDeallocate(sd_ref: DnsServiceRef);
    fn DNSServiceBrowse(
        sd_ref: *mut DnsServiceRef,
        flags: Flags,
        interface_index: u32,
        regtype: *const c_char,
        domain: *const c_char,
        callback: BrowseReply,
        context: *mut c_void,
    ) -> ErrorType;
    fn DNSServiceQueryRecord(
        sd_ref: *mut DnsServiceRef,
        flags: Flags,
        interface_index: u32,
        fullname: *const c_char,
        rrtype: u16,
        rrclass: u16,
        callback: QueryRecordReply,
        context: *mut c_void,
    ) -> ErrorType;
    fn DNSServiceGetAddrInfo(
        sd_ref: *mut DnsServiceRef,
        flags: Flags,
        interface_index: u32,
        protocol: u32,
        hostname: *const c_char,
        callback: GetAddrInfoReply,
        context: *mut c_void,
    ) -> ErrorType;
    fn poll(fds: *mut PollFd, count: u32, timeout_ms: c_int) -> c_int;
}

#[repr(C)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}

const POLL_IN: i16 = 0x0001;

// ----- the lookup -----

/// What the system's Bonjour finds, as an iroh address lookup. It never announces.
#[derive(Debug)]
struct BonjourLookup;

impl AddressLookup for BonjourLookup {
    fn resolve(&self, endpoint_id: EndpointId) -> Option<BoxStream<Result<Item, LookupError>>> {
        let (sender, receiver) = unbounded_channel::<Result<Item, LookupError>>();
        std::thread::Builder::new()
            .name("azul-bonjour".to_string())
            .spawn(move || run_question(endpoint_id, sender))
            .ok()?;
        Some(Box::pin(n0_future::stream::unfold(
            receiver,
            |mut receiver| async move { receiver.recv().await.map(|next| (next, receiver)) },
        )))
    }
}

/// Bonjour on iOS.
pub(super) struct BonjourDiscovery;

impl LocalDiscovery for BonjourDiscovery {
    /// An iPhone is a client: it asks for the Mac and announces nothing.
    fn can_announce(&self) -> bool {
        false
    }

    fn start(
        &self,
        endpoint: &Endpoint,
        _announce: bool,
        ignored: &Arc<AtomicU64>,
        nearby: &Arc<Nearby>,
    ) -> Result<Option<Held>, String> {
        let services = endpoint.address_lookup().map_err(|e| format!("{e:#}"))?;
        services.add(LocalLookup::new(
            BonjourLookup,
            ignored.clone(),
            nearby.clone(),
        ));
        // the watch: who is on the network, with the text they advertise; it stops with the engine
        let stop = Arc::new(AtomicBool::new(false));
        let watching = stop.clone();
        let list = nearby.clone();
        std::thread::Builder::new()
            .name("azul-bonjour-watch".to_string())
            .spawn(move || run_watch(list, &watching))
            .map_err(|e| format!("could not start the Bonjour watch: {e}"))?;
        Ok(Some(Box::new(StopWatch(stop))))
    }
}

/// Stops the watch of an engine when the engine is dropped.
struct StopWatch(Arc<AtomicBool>);

impl Drop for StopWatch {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

// ----- one question: where is endpoint `id` -----

/// The state of one question. It lives in a `Box` that this question's thread leaks to a raw
/// pointer for the system to hand to its callbacks, and takes back when the connection is closed:
/// every access, in the thread's own code and in the callbacks, goes through that one pointer, and a
/// callback runs only inside `DNSServiceProcessResult`, on this thread, never while the thread holds
/// a reference to the state.
struct Worker {
    /// The pointer the system holds as the context of every operation: this worker's own address.
    me: *mut Worker,
    id: EndpointId,
    label: String,
    sender: UnboundedSender<Result<Item, LookupError>>,
    /// The shared connection and the operations on it.
    connection: DnsServiceRef,
    operations: Vec<DnsServiceRef>,
    /// The SRV query was started.
    srv_started: bool,
    /// The host names whose addresses were asked for.
    asked: BTreeSet<String>,
    /// SRV target to port; host name to its addresses (both keyed by [`host_key`]).
    ports: BTreeMap<String, u16>,
    hosts: BTreeMap<String, Vec<IpAddr>>,
    /// The addresses already handed to the stream.
    sent: usize,
    /// What went wrong, once.
    failure: Option<String>,
}

/// Asks the system where `id` is and sends what it learns into `sender`, until the stream behind
/// `sender` is dropped, something fails, or [`GIVE_UP_AFTER`] has passed.
fn run_question(id: EndpointId, sender: UnboundedSender<Result<Item, LookupError>>) {
    let state = Box::into_raw(Box::new(Worker {
        me: std::ptr::null_mut(),
        id,
        label: instance_label(id.as_bytes()),
        sender,
        connection: std::ptr::null_mut(),
        operations: Vec::new(),
        srv_started: false,
        asked: BTreeSet::new(),
        ports: BTreeMap::new(),
        hosts: BTreeMap::new(),
        sent: 0,
        failure: None,
    }));
    // SAFETY: just made, and not shared with anything yet.
    unsafe { (*state).me = state };
    drive(state);
    // SAFETY: `state` was made above; the callbacks stop with `close`, which comes first.
    let failure = unsafe {
        (*state).close();
        (*state).failure.take()
    };
    // SAFETY: made by `Box::into_raw` above, taken back once, no callback can run any more.
    let worker = unsafe { Box::from_raw(state) };
    if let Some(reason) = failure {
        let _ = worker.sender.send(Err(LookupError::from_err(
            "bonjour",
            std::io::Error::other(reason),
        )));
    }
}

/// Starts the browse and pumps the system's answers into the worker behind `state`.
fn drive(state: *mut Worker) {
    // SAFETY (this function): `state` points at the live worker of `run_question`; each `unsafe`
    // block below touches it briefly and holds no reference across a call into the system.
    let mut connection: DnsServiceRef = std::ptr::null_mut();
    // SAFETY: a valid out pointer; on success it holds a connection that `close` deallocates.
    let code = unsafe { DNSServiceCreateConnection(&mut connection) };
    if code != NO_ERROR {
        unsafe { (*state).failure = Some(describe_error(code)) };
        return;
    }
    unsafe { (*state).connection = connection };
    let (Ok(regtype), Ok(domain)) = (CString::new(SERVICE_TYPE), CString::new(DOMAIN)) else {
        return;
    };
    let mut browse: DnsServiceRef = connection;
    // SAFETY: the strings outlive the call, `on_browse` matches DNSServiceBrowseReply, and
    // `state` outlives the connection the system keeps it for.
    let code = unsafe {
        DNSServiceBrowse(
            &mut browse,
            FLAGS_SHARE_CONNECTION,
            INTERFACE_ANY,
            regtype.as_ptr(),
            domain.as_ptr(),
            on_browse,
            state.cast::<c_void>(),
        )
    };
    if code != NO_ERROR {
        unsafe { (*state).failure = Some(describe_error(code)) };
        return;
    }
    unsafe { (*state).operations.push(browse) };

    // SAFETY: the connection is valid until `close`.
    let fd = unsafe { DNSServiceRefSockFD(connection) };
    if fd < 0 {
        unsafe { (*state).failure = Some("the system's Bonjour gave no socket".to_string()) };
        return;
    }
    let started = Instant::now();
    while started.elapsed() < GIVE_UP_AFTER {
        // SAFETY: a brief look at two fields.
        let stop = unsafe { (*state).failure.is_some() || (*state).sender.is_closed() };
        if stop {
            break;
        }
        let mut watched = PollFd {
            fd,
            events: POLL_IN,
            revents: 0,
        };
        // SAFETY: one valid pollfd.
        let ready = unsafe { poll(&mut watched, 1, POLL_MS) };
        if ready > 0 && watched.revents & POLL_IN != 0 {
            // SAFETY: the callbacks run in here, on this thread; no reference to the worker is held.
            let code = unsafe { DNSServiceProcessResult(connection) };
            if code != NO_ERROR {
                unsafe { (*state).failure = Some(describe_error(code)) };
            }
        }
    }
}

impl Worker {
    /// Deallocates the operations, then the connection that carried them.
    fn close(&mut self) {
        for operation in self.operations.drain(..) {
            // SAFETY: each was started on the shared connection and is deallocated once.
            unsafe { DNSServiceRefDeallocate(operation) };
        }
        if !self.connection.is_null() {
            // SAFETY: created in `drive`, deallocated once.
            unsafe { DNSServiceRefDeallocate(self.connection) };
            self.connection = std::ptr::null_mut();
        }
    }

    /// An instance named like the endpoint was found: ask for its SRV records.
    fn instance_found(&mut self) {
        if self.srv_started {
            return;
        }
        self.srv_started = true;
        let Ok(name) = CString::new(instance_fullname(&self.label)) else {
            return;
        };
        let mut query: DnsServiceRef = self.connection;
        let context = self.me.cast::<c_void>();
        // SAFETY: as in `drive`: the context is the worker's own pointer, kept for the callbacks.
        let code = unsafe {
            DNSServiceQueryRecord(
                &mut query,
                FLAGS_SHARE_CONNECTION,
                INTERFACE_ANY,
                name.as_ptr(),
                TYPE_SRV,
                CLASS_IN,
                on_srv,
                context,
            )
        };
        if code == NO_ERROR {
            self.operations.push(query);
        } else {
            self.failure = Some(describe_error(code));
        }
    }

    /// One SRV record of the instance: its port, and ask for the addresses of its host.
    fn srv_found(&mut self, rdata: &[u8]) {
        let Some(srv) = parse_srv(rdata) else {
            return;
        };
        let target = host_key(&srv.target);
        self.ports.insert(target.clone(), srv.port);
        if !self.asked.insert(target.clone()) {
            return;
        }
        let Ok(host) = CString::new(target) else {
            return;
        };
        let mut query: DnsServiceRef = self.connection;
        let context = self.me.cast::<c_void>();
        // SAFETY: as in `instance_found`.
        let code = unsafe {
            DNSServiceGetAddrInfo(
                &mut query,
                FLAGS_SHARE_CONNECTION,
                INTERFACE_ANY,
                PROTOCOL_IPV4 | PROTOCOL_IPV6,
                host.as_ptr(),
                on_address,
                context,
            )
        };
        if code == NO_ERROR {
            self.operations.push(query);
        } else {
            self.failure = Some(describe_error(code));
        }
    }

    /// One address of a host: remember it, and when it adds to what the stream has had, hand the
    /// stream the whole picture (an iroh dial merges the addresses of every item it gets).
    fn address_found(&mut self, host: &str, ip: IpAddr, more_coming: bool) {
        let known = self.hosts.entry(host_key(host)).or_default();
        if !known.contains(&ip) {
            known.push(ip);
        }
        if more_coming {
            return;
        }
        let addrs = assemble(&self.ports, &self.hosts);
        if addrs.len() <= self.sent {
            return;
        }
        self.sent = addrs.len();
        let mut data = EndpointData::default();
        data.add_ip_addrs(addrs);
        let item = Item::new(EndpointInfo::from_parts(self.id, data), "bonjour", None);
        let _ = self.sender.send(Ok(item));
    }
}

/// Reads a C string the system passed to a callback.
///
/// # Safety
/// `text` is null or a NUL-terminated string that lives for the call.
unsafe fn c_text(text: *const c_char) -> Option<String> {
    if text.is_null() {
        return None;
    }
    // SAFETY: not null, and NUL-terminated for the length of the call (the contract above).
    Some(
        unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned(),
    )
}

extern "C" fn on_browse(
    _sd_ref: DnsServiceRef,
    flags: Flags,
    _interface: u32,
    error: ErrorType,
    service_name: *const c_char,
    _regtype: *const c_char,
    _domain: *const c_char,
    context: *mut c_void,
) {
    // SAFETY: `context` is the worker that started the browse, alive while its connection is.
    let worker = unsafe { &mut *context.cast::<Worker>() };
    if error != NO_ERROR {
        worker.failure = Some(describe_error(error));
        return;
    }
    // SAFETY: the system passes a NUL-terminated name.
    let name = unsafe { c_text(service_name) };
    if flags & FLAGS_ADD != 0 && name.as_deref() == Some(worker.label.as_str()) {
        worker.instance_found();
    }
}

extern "C" fn on_srv(
    _sd_ref: DnsServiceRef,
    flags: Flags,
    _interface: u32,
    error: ErrorType,
    _fullname: *const c_char,
    rrtype: u16,
    rrclass: u16,
    rdlen: u16,
    rdata: *const c_void,
    _ttl: u32,
    context: *mut c_void,
) {
    // SAFETY: as in `on_browse`.
    let worker = unsafe { &mut *context.cast::<Worker>() };
    if error != NO_ERROR {
        worker.failure = Some(describe_error(error));
        return;
    }
    if flags & FLAGS_ADD == 0 || rrtype != TYPE_SRV || rrclass != CLASS_IN || rdata.is_null() {
        return;
    }
    // SAFETY: the system passes `rdlen` bytes at `rdata`, valid for the call.
    let bytes = unsafe { std::slice::from_raw_parts(rdata.cast::<u8>(), usize::from(rdlen)) };
    worker.srv_found(bytes);
}

extern "C" fn on_address(
    _sd_ref: DnsServiceRef,
    flags: Flags,
    _interface: u32,
    error: ErrorType,
    hostname: *const c_char,
    address: *const c_void,
    _ttl: u32,
    context: *mut c_void,
) {
    // SAFETY: as in `on_browse`.
    let worker = unsafe { &mut *context.cast::<Worker>() };
    if error != NO_ERROR {
        // an address query that finds nothing is an answer, not a failure of the lookup
        return;
    }
    if flags & FLAGS_ADD == 0 || address.is_null() {
        return;
    }
    // SAFETY: the system passes a NUL-terminated host name.
    let Some(host) = (unsafe { c_text(hostname) }) else {
        return;
    };
    // SAFETY: a `sockaddr` starts with its own length in one byte, and is that long.
    let bytes = unsafe {
        let length = usize::from(*address.cast::<u8>());
        std::slice::from_raw_parts(address.cast::<u8>(), length)
    };
    if let Some(ip) = ip_from_sockaddr(bytes) {
        worker.address_found(&host, ip, flags & FLAGS_MORE_COMING != 0);
    }
}

// ----- the watch: who is on the network, and what they say -----

/// What is known of one announced instance so far.
struct Instance {
    id: EndpointId,
    /// SRV target to port; host name to its addresses; the text it advertises (its TXT record's
    /// `user-data`).
    ports: BTreeMap<String, u16>,
    hosts: BTreeMap<String, Vec<IpAddr>>,
    user_data: Option<String>,
    /// The queries started for it, deallocated when it goes.
    operations: Vec<DnsServiceRef>,
}

/// The state of the watch, owned like [`Worker`]: a `Box` leaked to the raw pointer the system holds
/// as the context of every operation, taken back when the connection is closed.
struct Watcher {
    me: *mut Watcher,
    connection: DnsServiceRef,
    browse: DnsServiceRef,
    nearby: Arc<Nearby>,
    /// The instances by name (the endpoint id as base32).
    instances: BTreeMap<String, Instance>,
    /// Host name to the instance whose SRV record names it.
    owners: BTreeMap<String, String>,
    failure: Option<String>,
}

/// Runs the watch until `stop` is set or the system's Bonjour fails (the list says why).
fn run_watch(nearby: Arc<Nearby>, stop: &AtomicBool) {
    let state = Box::into_raw(Box::new(Watcher {
        me: std::ptr::null_mut(),
        connection: std::ptr::null_mut(),
        browse: std::ptr::null_mut(),
        nearby,
        instances: BTreeMap::new(),
        owners: BTreeMap::new(),
        failure: None,
    }));
    // SAFETY: just made, and not shared with anything yet.
    unsafe { (*state).me = state };
    watch(state, stop);
    // SAFETY: made above; the callbacks stop with `close`, which comes first, and the box is taken
    // back once.
    drop(unsafe {
        (*state).close();
        Box::from_raw(state)
    });
}

fn watch(state: *mut Watcher, stop: &AtomicBool) {
    // SAFETY (this function): `state` is the live watcher of `run_watch`; each `unsafe` block
    // touches it briefly and holds no reference across a call into the system.
    let mut connection: DnsServiceRef = std::ptr::null_mut();
    // SAFETY: a valid out pointer; on success it holds a connection that `close` deallocates.
    let code = unsafe { DNSServiceCreateConnection(&mut connection) };
    if code != NO_ERROR {
        unsafe { (*state).fail(describe_error(code)) };
        return;
    }
    unsafe { (*state).connection = connection };
    let (Ok(regtype), Ok(domain)) = (CString::new(SERVICE_TYPE), CString::new(DOMAIN)) else {
        return;
    };
    let mut browse: DnsServiceRef = connection;
    // SAFETY: the strings outlive the call, `on_watch_browse` matches DNSServiceBrowseReply, and
    // `state` outlives the connection the system keeps it for.
    let code = unsafe {
        DNSServiceBrowse(
            &mut browse,
            FLAGS_SHARE_CONNECTION,
            INTERFACE_ANY,
            regtype.as_ptr(),
            domain.as_ptr(),
            on_watch_browse,
            state.cast::<c_void>(),
        )
    };
    if code != NO_ERROR {
        unsafe { (*state).fail(describe_error(code)) };
        return;
    }
    unsafe { (*state).browse = browse };
    // SAFETY: the connection is valid until `close`.
    let fd = unsafe { DNSServiceRefSockFD(connection) };
    if fd < 0 {
        unsafe { (*state).fail("the system's Bonjour gave no socket".to_string()) };
        return;
    }
    while !stop.load(Ordering::Relaxed) {
        // SAFETY: a brief look at one field.
        if unsafe { (*state).failure.is_some() } {
            break;
        }
        let mut watched = PollFd {
            fd,
            events: POLL_IN,
            revents: 0,
        };
        // SAFETY: one valid pollfd.
        let ready = unsafe { poll(&mut watched, 1, POLL_MS) };
        if ready > 0 && watched.revents & POLL_IN != 0 {
            // SAFETY: the callbacks run in here, on this thread; no reference to the watcher is held.
            let code = unsafe { DNSServiceProcessResult(connection) };
            if code != NO_ERROR {
                unsafe { (*state).fail(describe_error(code)) };
            }
        }
    }
}

impl Watcher {
    /// The system's Bonjour refused or broke: the list keeps the reason for the status.
    fn fail(&mut self, reason: String) {
        self.nearby.set_trouble(reason.clone());
        self.failure.get_or_insert(reason);
    }

    /// Deallocates every query, then the browse, then the connection that carried them.
    fn close(&mut self) {
        for instance in self.instances.values_mut() {
            for operation in instance.operations.drain(..) {
                // SAFETY: each was started on the shared connection and is deallocated once.
                unsafe { DNSServiceRefDeallocate(operation) };
            }
        }
        if !self.browse.is_null() {
            // SAFETY: started in `watch`, deallocated once.
            unsafe { DNSServiceRefDeallocate(self.browse) };
            self.browse = std::ptr::null_mut();
        }
        if !self.connection.is_null() {
            // SAFETY: created in `watch`, deallocated once.
            unsafe { DNSServiceRefDeallocate(self.connection) };
            self.connection = std::ptr::null_mut();
        }
    }

    /// An instance named `label` was announced: if the name is an endpoint id, ask for its
    /// records.
    fn instance_added(&mut self, label: &str) {
        let Ok(id) = EndpointId::from_str(label) else {
            return; // another app's service of the same type
        };
        if self.instances.contains_key(label) || self.instances.len() >= MAX_NEARBY_PEERS {
            return;
        }
        let Ok(name) = CString::new(instance_fullname(label)) else {
            return;
        };
        let mut instance = Instance {
            id,
            ports: BTreeMap::new(),
            hosts: BTreeMap::new(),
            user_data: None,
            operations: Vec::new(),
        };
        let context = self.me.cast::<c_void>();
        let queries: [(u16, QueryRecordReply); 2] =
            [(TYPE_SRV, on_watch_srv), (TYPE_TXT, on_watch_txt)];
        for (kind, callback) in queries {
            let mut query: DnsServiceRef = self.connection;
            // SAFETY: as in `Worker::instance_found`.
            let code = unsafe {
                DNSServiceQueryRecord(
                    &mut query,
                    FLAGS_SHARE_CONNECTION,
                    INTERFACE_ANY,
                    name.as_ptr(),
                    kind,
                    CLASS_IN,
                    callback,
                    context,
                )
            };
            if code == NO_ERROR {
                instance.operations.push(query);
            } else {
                self.fail(describe_error(code));
            }
        }
        self.instances.insert(label.to_string(), instance);
    }

    /// The instance `label` is no longer announced.
    fn instance_removed(&mut self, label: &str) {
        let Some(instance) = self.instances.remove(label) else {
            return;
        };
        for operation in instance.operations {
            // SAFETY: each was started on the shared connection and is deallocated once.
            unsafe { DNSServiceRefDeallocate(operation) };
        }
        self.owners.retain(|_, owner| owner != label);
        self.nearby.gone(&instance.id);
    }

    /// One SRV record of the instance `label`: its port, and ask for the addresses of its host.
    fn srv_found(&mut self, label: &str, rdata: &[u8], added: bool) {
        let Some(srv) = parse_srv(rdata) else {
            return;
        };
        let target = host_key(&srv.target);
        let Some(instance) = self.instances.get_mut(label) else {
            return;
        };
        if !added {
            // the record went, and so does its port: the addresses of the host stay, the query
            // that found them delivers them once and the record may come back
            if instance.ports.get(&target) == Some(&srv.port) {
                instance.ports.remove(&target);
            }
            self.publish(label);
            return;
        }
        if !instance.ports.contains_key(&target) && instance.ports.len() >= MAX_TARGETS {
            return;
        }
        let known = instance.ports.insert(target.clone(), srv.port).is_some();
        let owned = self
            .owners
            .insert(target.clone(), label.to_string())
            .is_some();
        if !known && !owned {
            if let Ok(host) = CString::new(target) {
                let mut query: DnsServiceRef = self.connection;
                let context = self.me.cast::<c_void>();
                // SAFETY: as in `Worker::srv_found`.
                let code = unsafe {
                    DNSServiceGetAddrInfo(
                        &mut query,
                        FLAGS_SHARE_CONNECTION,
                        INTERFACE_ANY,
                        PROTOCOL_IPV4 | PROTOCOL_IPV6,
                        host.as_ptr(),
                        on_watch_address,
                        context,
                    )
                };
                if code == NO_ERROR {
                    if let Some(instance) = self.instances.get_mut(label) {
                        instance.operations.push(query);
                    }
                } else {
                    self.fail(describe_error(code));
                }
            }
        }
        self.publish(label);
    }

    /// The TXT record of the instance `label` came or went: its `user-data` is what it advertises.
    fn txt_found(&mut self, label: &str, rdata: &[u8], added: bool) {
        let value = txt_value(rdata, "user-data");
        let Some(instance) = self.instances.get_mut(label) else {
            return;
        };
        if added {
            instance.user_data = value;
        } else if instance.user_data == value {
            // the old record goes after the new one came: only what it said is forgotten
            instance.user_data = None;
        }
        self.publish(label);
    }

    /// One address of a host.
    fn address_found(&mut self, host: &str, ip: IpAddr, more_coming: bool) {
        let key = host_key(host);
        let Some(label) = self.owners.get(&key).cloned() else {
            return;
        };
        if let Some(instance) = self.instances.get_mut(&label) {
            let known = instance.hosts.entry(key).or_default();
            if !known.contains(&ip) && known.len() < MAX_TARGETS {
                known.push(ip);
            }
        }
        if !more_coming {
            self.publish(&label);
        }
    }

    /// Tells the list what is known of the instance `label`, once it has an address a dial can use.
    fn publish(&self, label: &str) {
        let Some(instance) = self.instances.get(label) else {
            return;
        };
        let addresses = assemble(&instance.ports, &instance.hosts);
        if addresses.is_empty() {
            return;
        }
        let mut data = EndpointData::default();
        data.add_ip_addrs(addresses);
        if let Some(text) = &instance.user_data {
            data.set_user_data(text.parse::<UserData>().ok());
        }
        self.nearby
            .seen(&EndpointInfo::from_parts(instance.id, data));
    }
}

/// The name of the instance a record belongs to: the first label of its full name.
fn instance_of(fullname: *const c_char) -> Option<String> {
    // SAFETY: the system passes a NUL-terminated name.
    let name = unsafe { c_text(fullname) }?;
    name.split('.').next().map(str::to_string)
}

extern "C" fn on_watch_browse(
    _sd_ref: DnsServiceRef,
    flags: Flags,
    _interface: u32,
    error: ErrorType,
    service_name: *const c_char,
    _regtype: *const c_char,
    _domain: *const c_char,
    context: *mut c_void,
) {
    // SAFETY: `context` is the watcher that started the browse, alive while its connection is.
    let watcher = unsafe { &mut *context.cast::<Watcher>() };
    if error != NO_ERROR {
        watcher.fail(describe_error(error));
        return;
    }
    // SAFETY: the system passes a NUL-terminated name.
    let Some(label) = (unsafe { c_text(service_name) }) else {
        return;
    };
    if flags & FLAGS_ADD != 0 {
        watcher.instance_added(&label);
    } else {
        watcher.instance_removed(&label);
    }
}

/// A record of an instance: `rdlen` bytes at `rdata`, added or removed.
fn record_of<'a>(rdata: *const c_void, rdlen: u16) -> Option<&'a [u8]> {
    if rdata.is_null() {
        return None;
    }
    // SAFETY: the system passes `rdlen` bytes at `rdata`, valid for the call.
    Some(unsafe { std::slice::from_raw_parts(rdata.cast::<u8>(), usize::from(rdlen)) })
}

extern "C" fn on_watch_srv(
    _sd_ref: DnsServiceRef,
    flags: Flags,
    _interface: u32,
    error: ErrorType,
    fullname: *const c_char,
    rrtype: u16,
    rrclass: u16,
    rdlen: u16,
    rdata: *const c_void,
    _ttl: u32,
    context: *mut c_void,
) {
    // SAFETY: as in `on_watch_browse`.
    let watcher = unsafe { &mut *context.cast::<Watcher>() };
    if error == ERR_POLICY_DENIED || error == ERR_NOT_RUNNING {
        watcher.fail(describe_error(error));
        return;
    }
    if error != NO_ERROR || rrtype != TYPE_SRV || rrclass != CLASS_IN {
        return;
    }
    if let (Some(label), Some(bytes)) = (instance_of(fullname), record_of(rdata, rdlen)) {
        watcher.srv_found(&label, bytes, flags & FLAGS_ADD != 0);
    }
}

extern "C" fn on_watch_txt(
    _sd_ref: DnsServiceRef,
    flags: Flags,
    _interface: u32,
    error: ErrorType,
    fullname: *const c_char,
    rrtype: u16,
    rrclass: u16,
    rdlen: u16,
    rdata: *const c_void,
    _ttl: u32,
    context: *mut c_void,
) {
    // SAFETY: as in `on_watch_browse`.
    let watcher = unsafe { &mut *context.cast::<Watcher>() };
    if error == ERR_POLICY_DENIED || error == ERR_NOT_RUNNING {
        watcher.fail(describe_error(error));
        return;
    }
    if error != NO_ERROR || rrtype != TYPE_TXT || rrclass != CLASS_IN {
        return;
    }
    if let (Some(label), Some(bytes)) = (instance_of(fullname), record_of(rdata, rdlen)) {
        watcher.txt_found(&label, bytes, flags & FLAGS_ADD != 0);
    }
}

extern "C" fn on_watch_address(
    _sd_ref: DnsServiceRef,
    flags: Flags,
    _interface: u32,
    error: ErrorType,
    hostname: *const c_char,
    address: *const c_void,
    _ttl: u32,
    context: *mut c_void,
) {
    // SAFETY: as in `on_watch_browse`.
    let watcher = unsafe { &mut *context.cast::<Watcher>() };
    if error != NO_ERROR || flags & FLAGS_ADD == 0 || address.is_null() {
        return;
    }
    // SAFETY: the system passes a NUL-terminated host name.
    let Some(host) = (unsafe { c_text(hostname) }) else {
        return;
    };
    // SAFETY: a `sockaddr` starts with its own length in one byte, and is that long.
    let bytes = unsafe {
        let length = usize::from(*address.cast::<u8>());
        std::slice::from_raw_parts(address.cast::<u8>(), length)
    };
    if let Some(ip) = ip_from_sockaddr(bytes) {
        watcher.address_found(&host, ip, flags & FLAGS_MORE_COMING != 0);
    }
}

#[cfg(test)]
#[path = "bonjour_test.rs"]
mod tests;
