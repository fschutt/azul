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

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{c_char, c_int, c_void, CStr, CString},
    net::IpAddr,
    sync::{atomic::AtomicU64, Arc},
    time::{Duration, Instant},
};

use iroh::{
    address_lookup::{AddressLookup, EndpointData, EndpointInfo, Error as LookupError, Item},
    Endpoint, EndpointId,
};
use n0_future::boxed::BoxStream;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use super::{
    discovery::{Held, LocalDiscovery, LocalLookup},
    dns_wire::{
        assemble, describe_error, instance_fullname, instance_label, ip_from_sockaddr, parse_srv,
        DOMAIN, SERVICE_TYPE,
    },
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
const CLASS_IN: u16 = 1;
const PROTOCOL_IPV4: u32 = 0x01;
const PROTOCOL_IPV6: u32 = 0x02;
const NO_ERROR: ErrorType = 0;

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
        Some(Box::pin(n0_future::stream::unfold(receiver, |mut receiver| async move {
            receiver.recv().await.map(|next| (next, receiver))
        })))
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
    ) -> Result<Option<Held>, String> {
        let services = endpoint.address_lookup().map_err(|e| format!("{e:#}"))?;
        services.add(LocalLookup::new(BonjourLookup, ignored.clone()));
        Ok(None)
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
    /// SRV target to port; host name to its addresses.
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
        let _ = worker
            .sender
            .send(Err(LookupError::from_err("bonjour", std::io::Error::other(reason))));
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
        self.ports.insert(srv.target.clone(), srv.port);
        if !self.asked.insert(srv.target.clone()) {
            return;
        }
        let Ok(host) = CString::new(srv.target) else {
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
        let known = self.hosts.entry(host.to_string()).or_default();
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
    Some(CStr::from_ptr(text).to_string_lossy().into_owned())
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

#[cfg(test)]
#[path = "bonjour_test.rs"]
mod tests;
