//! The permission flow of the Android shell, pinned in source (the JVM and a device cannot run where the tests
//! run; the rules that can are tested as pure code in `extra/permission/android_table_test.rs`). What the audit of
//! 2026-11-28 found (ledger rows AN-3, AN-4, AN-5 of briefs/mobile_capabilities_ledger_2026-11-28.md):
//!
//! * the permission manager was NEVER told what the system says: `probe_status` had no caller, so a permission that
//!   was granted by an earlier run, in the settings or with `pm grant` read NotDetermined for ever (AN-3);
//! * two requests in flight: the system cancels the second one unasked (empty result arrays), and the glue turned
//!   that into a DENIAL, remembered for good (AN-4);
//! * a location subscription made in the same layout pass as the permission request found no permission, returned
//!   silently, and nothing started it after the grant: no fix until the app was started a second time (AN-5).
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_permission_flow_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Java source without its comments.
fn java_code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Rust source without its comments and without whitespace (rustfmt breaks the calls over lines).
fn rust_code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<String>()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// The body of a method or function of `src` starting at `signature` (up to its matching brace).
fn body<'a>(src: &'a str, signature: &str) -> &'a str {
    let start = src
        .find(signature)
        .unwrap_or_else(|| panic!("no `{signature}` in the source"));
    let open = start + src[start..].find('{').unwrap();
    let mut depth = 0;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..open + i + 1];
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces after `{signature}`");
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// ONE permission dialog at a time: a request made while another dialog is on the screen is cancelled by the
/// system without being shown (its result arrays are empty). A camera tile and a microphone tile mounted in the
/// same frame, or a location probe and a first notification, are the ordinary case. The requests wait for each other,
/// in the order they were made, and the next one starts when the answer to the one before it has arrived.
///
/// Mutation: `request` calling `activity.requestPermissions(permissions, requestCode)` itself (the state of 46d6ad5ffd).
#[test]
fn permission_requests_wait_for_the_dialog_of_the_one_before() {
    let java = java_code("scripts/android/AzulPermissions.java");
    assert_eq!(
        count(&java, ".requestPermissions("),
        1,
        "exactly one call site of Activity.requestPermissions: the queue's"
    );
    let request = body(&java, "public static void request(");
    assert!(!request.contains(".requestPermissions("), "request() queues, it does not show a dialog itself");
    assert!(request.contains("waiting.add("), "the request waits in the queue");
    assert!(request.contains("startNext(activity)"), "and the queue is run");
    assert!(request.contains("runOnUiThread"), "the queue is touched on the UI thread only: no lock");

    let next = body(&java, "private static void startNext(");
    let guard = next.find("inFlight == null").expect("a dialog starts only while none is on the screen");
    let show = next.find(".requestPermissions(").expect("startNext shows the dialog");
    let mark = next.find("inFlight = next").expect("the request in flight is remembered");
    assert!(guard < show && show < mark, "check, then show, then remember: in that order");

    let proxy = body(&java, "public static boolean onRequestPermissionsResultProxy(");
    let cleared = proxy.find("inFlight = null").expect("the answer frees the screen");
    let started = proxy.find("startNext(activity)").expect("and the next request starts");
    assert!(cleared < started, "the next dialog starts after the answer to the one before it");
    assert!(
        proxy.contains("done.requestCode != requestCode"),
        "an answer that is not the request in flight is not ours (and is not forwarded as one)"
    );
}

/// A request the system DROPPED (empty result arrays: the user pressed Home, the activity was paused, the framework
/// refused to start it) is not a "no": nothing is remembered as a denial, and the manager goes back to what the
/// system says, instead of being told Denied for a question nobody answered.
///
/// Mutation: the empty-array branch calling `nativeOnPermissionResult(requestCode, false, false)` (the state of
/// 46d6ad5ffd: the request was answered Denied, and the diff path never asked again in that process).
#[test]
fn a_request_the_system_dropped_is_not_a_denial() {
    let java = java_code("scripts/android/AzulPermissions.java");
    let proxy = body(&java, "public static boolean onRequestPermissionsResultProxy(");
    let dropped_at = proxy.find("if (dropped) {").expect("the empty-array case has its own branch");
    let else_at = proxy[dropped_at..].find("} else {").expect("and the answered case the other") + dropped_at;
    let dropped_branch = &proxy[dropped_at..else_at];
    let answered_branch = &proxy[else_at..];
    assert!(dropped_branch.contains("nativeOnPermissionCancelled(requestCode)"));
    assert!(!dropped_branch.contains("nativeOnPermissionResult("), "a dropped request has no result");
    assert!(!dropped_branch.contains("remember("), "and is not remembered as a refusal");
    assert!(answered_branch.contains("remember(activity, permissions, grantResults)"));
    assert!(answered_branch.contains("nativeOnPermissionResult(requestCode, all, any)"));
    let next = body(&java, "private static void startNext(");
    assert!(
        next.contains("nativeOnPermissionCancelled(next.requestCode)") && !next.contains("nativeOnPermissionResult("),
        "a request the framework refuses to start is dropped too, not denied"
    );

    let rust = rust_code("dll/src/desktop/extra/permission/android.rs");
    let cancelled = body(
        &rust,
        "pubunsafeextern\"system\"fnJava_com_azul_permission_AzulPermissions_nativeOnPermissionCancelled(",
    );
    assert!(cancelled.contains("park(capability,probe_status(capability))"), "back to what the system says");
    assert!(!cancelled.contains("Denied"), "never a denial");
    assert!(cancelled.contains("pending.remove("), "the waiting worker is released");
}

/// The manager is told what the system says when a capability is subscribed: a permission granted by an earlier run,
/// in the settings or with `pm grant`, reads Granted at once, a remembered refusal reads Denied (the app can offer the
/// settings); it used to read NotDetermined for ever because `probe_status` had no caller. An unknown
/// (NotDetermined) is not parked: it never replaces a known state.
///
/// Mutation: `handle_event` without the `probe_status` seed (the state of 46d6ad5ffd).
#[test]
fn a_subscribe_tells_the_manager_what_the_system_says() {
    let rust = rust_code("dll/src/desktop/extra/permission/android.rs");
    let handle = body(&rust, "pubfnhandle_event(event:&PermissionDiffEvent){");
    let seed = handle.find("letprobed=probe_status(capability)").expect("the system is asked first");
    let park = handle.find("park(capability,probed)").expect("and the manager is told");
    let ask = handle.find("request_with(capability,false)").expect("then a dialog may follow");
    assert!(seed < park && park < ask);
    assert!(
        handle.contains("probed!=PermissionState::NotDetermined"),
        "an unknown never replaces a known state"
    );
}

/// A dialog is marked Requested BEFORE it is made (its answer, a refusal to start included, comes after it in the
/// queue), a second ask for the same capability joins the first instead of making another, and a request that
/// could not be started puts the manager back to what the system says instead of leaving it Requested.
///
/// Mutation: `park(capability, Requested)` after `request_permissions` (a refused start was overwritten by Requested:
/// the manager waited for an answer for ever, and the 100 ms pump timer ran for ever with it).
#[test]
fn a_dialog_is_marked_requested_first_and_never_made_twice() {
    let rust = rust_code("dll/src/desktop/extra/permission/android.rs");
    let with = body(&rust, "fnrequest_with(capability:Capability,reask:bool)->bool{");
    let joined = with.find("ifrequest_pending(capability){returntrue;}").expect("a dialog on its way is joined");
    let probe = with.find("probe_permissions(capability,perms)").expect("the state decides whether to ask");
    let requested = with.find("park(capability,PermissionState::Requested)").expect("the manager reads Requested");
    let made = with.find("request_permissions(perms,request_code)").expect("the dialog is requested");
    assert!(joined < probe && probe < requested && requested < made);
    let failed = &with[made..];
    assert!(
        failed.contains("park(capability,probe_status(capability))"),
        "a request that could not be started does not leave the manager at Requested"
    );
}

/// A worker thread that needs a permission (the camera tile, the microphone tile, the recogniser) goes through
/// `ensure`: granted -> on, refused -> stop and never ask again from there, undecided -> ask ONCE and wait for
/// the answer. It waits for the request to be answered or dropped, not for a guess of a time.
///
/// Mutation: `ensure` without the `request_pending` wait (the worker opened the device while the dialog was up).
#[test]
fn a_worker_asks_once_and_waits_for_the_answer() {
    let rust = rust_code("dll/src/desktop/extra/permission/android.rs");
    let ensure = body(
        &rust,
        "pubfnensure(capability:Capability,patience:std::time::Duration)->PermissionState{",
    );
    let gate = ensure.find("gate(state)!=Gate::Ask").expect("only an undecided state asks");
    let ask = ensure.find("request_with(capability,false)").expect("the one ask, never a re-ask");
    let wait = ensure.find("whilerequest_pending(capability)").expect("it waits for the answer");
    let probe = ensure.rfind("probe_status(capability)").expect("and reports what the system says then");
    assert!(gate < ask && ask < wait && wait < probe);
    assert!(
        ensure.contains("started.elapsed()<patience"),
        "a dialog nobody answers does not hold the worker for ever"
    );
}

/// A location subscription made before its permission (the probe node and the permission request come from the
/// same layout pass: the dialog is still up when `subscribe` runs) is REMEMBERED, and starts when the answer arrives.
/// A refusal is told to the engine as an error. An answer about another capability changes nothing.
///
/// Mutation: `subscribe` returning without storing `waitingHandle` (the state of 46d6ad5ffd: no fix until the second start).
#[test]
fn a_location_subscription_made_before_the_answer_starts_at_the_grant() {
    let java = java_code("scripts/android/AzulGeolocation.java");
    let subscribe = body(&java, "public static synchronized void subscribe(");
    let none = subscribe.find("if (!fine && !coarse) {").expect("the no-permission case");
    let remembered = subscribe[none..].find("waitingHandle = handle;").expect("the wish is kept") + none;
    let ret = subscribe[none..].find("return;").expect("and the call returns") + none;
    assert!(remembered < ret, "stored before it returns");
    assert!(subscribe.contains("waitingHandle = 0;"), "a started subscription is no longer waiting");

    let answered = body(&java, "public static synchronized void onPermissionAnswered(");
    assert!(answered.contains("subscribe(activity, handle, waitingHighAccuracy, waitingMinIntervalMs)"));
    assert!(answered.contains("!asksForLocation(requested)"), "an answer about the camera starts nothing");
    assert!(answered.contains("nativeOnLocationError(handle, ERROR_PERMISSION_DENIED)"), "a refusal is told to the engine");
    assert!(answered.contains("else if (!dropped)"), "a dropped request is not a refusal");

    let release = body(&java, "public static synchronized void release(");
    assert!(release.contains("waitingHandle = 0"), "a released subscription does not start later");

    let permissions = java_code("scripts/android/AzulPermissions.java");
    let proxy = body(&permissions, "public static boolean onRequestPermissionsResultProxy(");
    assert!(
        proxy.contains("AzulGeolocation.onPermissionAnswered(activity, done.permissions, dropped)"),
        "the answer reaches the waiting subscription"
    );

    let rust = rust_code("dll/src/desktop/extra/geolocation/android.rs");
    let error = body(
        &rust,
        "pubunsafeextern\"system\"fnJava_com_azul_geolocation_AzulGeolocation_nativeOnLocationError(",
    );
    assert!(error.contains("services.geolocation.push_error("), "the engine's channel gets the error");
    assert!(
        error.contains("handleasu64")
            && error.contains("SUBSCRIPTION_HANDLE.load(Ordering::Relaxed)"),
        "an error of a subscription that was released is dropped, like a late fix"
    );
}

/// A camera tile, a microphone tile and the speech recogniser need a permission nobody asked for, and a phone never
/// puts a stand-in in the place of the real thing: each asks ONCE, when it is mounted / when the app listens (not at
/// launch), waits for the answer, and a "no" opens NOTHING (a handle whose reads end at once), never `0`: `0` made the
/// widget show its colour-cycle test pattern / feed a 440 Hz test tone, and the app was told nothing (ledger AN-6).
///
/// Mutation: `open` returning `0` for a refusal (the state of 46d6ad5ffd: a refused camera showed a changing colour).
#[test]
fn camera_microphone_and_speech_ask_once_and_never_stand_in_for_a_refusal() {
    let camera = rust_code("dll/src/desktop/extra/camera/android.rs");
    let open = body(
        &camera,
        "pubfnopen(request:&azul_layout::widgets::capture_common::CaptureRequest)->u64{",
    );
    let ask = open.find("permission::android::ensure(Capability::Camera").expect("the tile asks for the camera");
    let device = open.find("open_device(request)").expect("and only then opens it");
    assert!(ask < device, "ask first, open after");
    assert!(count(open, "returnNOT_OPENED") >= 1 && count(open, "=>{") >= 1 && open.contains("NOT_OPENED"));
    assert!(!open.contains("return0"), "a camera that is not opened is never the handle 0 (the test pattern)");
    assert!(!open.contains("=>0"), "and a failed device open is not 0 either");
    let read = body(&camera, "pubfnread(handle:u64,out:&mutVec<u8>)->azul_layout::widgets::capture_common::CaptureRead{");
    assert!(read.contains("ifhandle<=NOT_OPENED{returnCaptureRead::Ended;}"), "the empty camera delivers nothing");
    let close = body(&camera, "pubfnclose(handle:u64){");
    assert!(close.contains("ifhandle<=NOT_OPENED{return;}"), "and frees nothing");
    for free in [
        "ACaptureRequest_free(cam.request)",
        "ACameraOutputTarget_free(cam.target)",
        "ACaptureSessionOutputContainer_free(cam.container)",
        "ACaptureSessionOutput_free(cam.output)",
    ] {
        assert!(close.contains(free), "the camera objects were leaked per open: {free}");
    }

    let mic = rust_code("dll/src/desktop/extra/audio/aaudio.rs");
    let open = body(&mic, "pubfnmic_open(rate:u32,channels:u16)->u64{");
    let ask = open.find("permission::android::ensure(Capability::Microphone").expect("the tile asks for the microphone");
    let stream = open.find("open_stream(rate,channels,AAUDIO_DIRECTION_INPUT)").expect("and only then opens the stream");
    assert!(ask < stream);
    assert!(!open.contains("return0") && !open.contains("=>0"), "never the handle 0 (the 440 Hz test tone)");
    assert!(count(open, "NOT_OPENED") >= 2, "a refusal and a failed stream both open nothing");
    let read = body(&mic, "pubfnmic_read(handle:u64,out:&mutVec<f32>)->u32{");
    assert!(read.contains("ifhandle<=NOT_OPENED{return0;}"));
    assert!(read.contains("Patience::new()") && read.contains("Action::Retry=>{}") && read.contains("Action::Reopen=>"));
    let close = body(&mic, "pubfnmic_close(handle:u64){");
    assert!(close.contains("ifhandle>NOT_OPENED{") && close.contains("!m.stream.is_null()"));

    let speech = rust_code("dll/src/desktop/extra/speech/android.rs");
    let listen = body(&speech, "pubfnlisten(language:&str,timeout_ms:u32)->SpeechHeard{");
    let ask = listen.find("permission::android::ensure(Capability::Microphone").expect("listening asks for the microphone");
    let handle = listen.find("NEXT_HANDLE.fetch_add").expect("before it starts a listening");
    assert!(ask < handle);
    assert!(listen.contains("HeardStatus::Denied"), "a refusal is an answer: Denied");
}

/// A location listener went on in the background (nothing paused it): battery for an app nobody looks at, the system's location
/// indicator lit, fixes with no window to go to (O-10 of the review). `onPause` stops the updates and keeps the subscription,
/// `onResume` starts it again with the options it had; a released subscription stays released.
///
/// Mutation: `AzulGeolocation.pause()` removed from `onPause` (the state of 6ce827b284).
#[test]
fn location_updates_stop_with_the_activity_and_come_back_with_it() {
    let activity = java_code("scripts/android/AzulActivity.java");
    let pause = body(&activity, "protected void onPause()");
    assert!(pause.contains("AzulGeolocation.pause();"), "the updates stop when the activity pauses");
    let resume = body(&activity, "protected void onResume()");
    assert!(resume.contains("AzulGeolocation.resume(this);"), "and start again when it is back");

    let java = java_code("scripts/android/AzulGeolocation.java");
    let paused = body(&java, "public static synchronized void pause()");
    assert!(paused.contains("manager.removeUpdates(listener)") && !paused.contains("liveHandle = 0"), "the subscription stays");
    let resumed = body(&java, "public static synchronized void resume(Activity activity)");
    assert!(resumed.contains("subscribe(activity, liveHandle, liveHighAccuracy, liveMinIntervalMs)"), "with the options it had");
    let release = body(&java, "public static synchronized void release(long handle)");
    assert!(release.contains("liveHandle = 0;"), "a released subscription does not come back");
}
