//! Source-text invariants of the Apple keyring and biometric backends: the code that talks to the Keychain and to
//! LocalAuthentication only compiles on an Apple (and an Apple host here is not a phone), so the shapes that decide
//! WHEN a sheet may show, what a store may weaken and what a probe may read are pinned as text, the way
//! `ios_shell_invariants` pins the shell. The decisions that can run anywhere (the OSStatus / LAError tables) are
//! tested as behaviour in `apple_logic_test.rs`.
//!
//! Each pin names what went wrong, or what the model forbids, when it does not hold.

const KEYRING_APPLE_RS: &str = include_str!("apple.rs");
const BIOMETRIC_APPLE_RS: &str = include_str!("../biometric/apple.rs");

/// The body of the first fn declared by `decl`, from its opening brace to the matching closing one.
fn body<'a>(source: &'a str, decl: &str) -> &'a str {
    let start = source
        .find(decl)
        .unwrap_or_else(|| panic!("`{decl}` not found - was it renamed?"));
    let open = start + source[start..].find('{').expect("a fn body");
    let mut depth = 0_usize;
    for (i, c) in source[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[open..=open + i];
                }
            }
            _ => {}
        }
    }
    &source[open..]
}

fn at(haystack: &str, needle: &str, what: &str) {
    assert!(haystack.contains(needle), "{what}: `{needle}` is missing");
}

fn not_at(haystack: &str, needle: &str, what: &str) {
    assert!(!haystack.contains(needle), "{what}: `{needle}` must not be there");
}

/// `first` appears in `haystack`, and `then` appears after it.
fn in_order(haystack: &str, first: &str, then: &str, what: &str) {
    let a = haystack
        .find(first)
        .unwrap_or_else(|| panic!("{what}: `{first}` is missing"));
    assert!(
        haystack[a + first.len()..].contains(then),
        "{what}: `{then}` must come after `{first}`"
    );
}

/// The code of `source` without its comments (line comments, doc comments): what a ban on a call has to look at, or a
/// comment that explains the ban would trip it.
fn code_of(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A panic in a phone app is an abort. A failed Keychain or LocalAuthentication call is an answer
/// (`KeyringResult::Error`, `Unavailable`...), never a panic.
#[test]
fn the_apple_keyring_and_biometric_do_not_unwrap() {
    for (file, source) in [("keyring/apple.rs", KEYRING_APPLE_RS), ("biometric/apple.rs", BIOMETRIC_APPLE_RS)] {
        for (n, line) in code_of(source).lines().enumerate() {
            assert!(
                !line.contains(".unwrap()") && !line.contains(".expect("),
                "{file}:{}: `{}` - a panic aborts a phone app; answer an error instead",
                n + 1,
                line.trim()
            );
        }
    }
}

/// The sheet shows for `Unlock` and for nothing else: the one place that hands an `LAContext` to a search is
/// `unlock`, every other read skips the items that want a user check (so a `Get`, a quiet op and a probe can never
/// pop a Face ID sheet in the middle of a frame).
#[test]
fn only_unlock_may_show_the_user_check_sheet() {
    let code = code_of(KEYRING_APPLE_RS);
    assert_eq!(
        code.matches(".authentication_context(").count(),
        1,
        "exactly one search carries an LAContext (the one of `unlock`)"
    );
    at(body(&code, "fn unlock("), ".authentication_context(", "unlock");
    // `Get` is a read of an unprotected item that skips the protected ones.
    let get = body(&code, "fn get(");
    not_at(get, "LAContext", "get");
    not_at(get, "authentication_context", "get");
    let read = body(&code, "fn read_unprotected(");
    at(read, ".skip_authenticated_items(true)", "read_unprotected");
    not_at(read, "LAContext", "read_unprotected");
    // The ops of `handle`: only the Unlock arm leaves the quiet switch off.
    let handle = body(&code, "pub(super) fn handle(");
    at(handle, "KeyringRequest::Get { key } => in_turn(quiet, || get(", "handle");
    at(handle, "KeyringRequest::Unlock { key, prompt } => in_turn(false, || unlock(", "handle");
    // A protected item answers `WouldPrompt` before any read when the interaction is not the user's.
    let unlock = body(&code, "fn unlock(");
    in_order(unlock, "interaction.user_check()", "LAContext::new()", "unlock");
}

/// A protected store is made from a user's action only and is never silently weaker: no sheet-less creation of an
/// item whose level the device cannot check (no passcode: `NotEnrolled`), and the access control is the
/// passcode-set-this-device-only accessibility, so an item can neither leave the device nor outlive the passcode.
#[test]
fn a_protected_store_is_never_a_weaker_one() {
    let code = code_of(KEYRING_APPLE_RS);
    let store = body(&code, "fn store(");
    // The user's action first, then the question to the device, then the access control, then the item.
    in_order(store, "interaction.user_check()", "device_state_for(level)", "store");
    in_order(store, "device_state_for(level)", "SecAccessControl::create_with_protection(", "store");
    in_order(store, "SecAccessControl::create_with_protection(", "set_generic_password_options(", "store");
    at(store, "ProtectionMode::AccessibleWhenPasscodeSetThisDeviceOnly", "store");
    at(store, "KeyringResult::NotEnrolled", "store");
    // The access control is replaced, never edited in place: the old item goes first.
    in_order(store, "delete_item(service, key)", "set_generic_password_options(", "store");
    // The meta record is written after the item (never a record of an item that was not made) and the item goes when
    // its record cannot be written (never an item an app cannot tell from a plain one).
    in_order(store, "set_generic_password_options(", "write_meta(", "store");
    at(store, "let _ = delete_item(service, key);", "store");
    // An unprotected store drops a stale meta record: it would make the plain item read as a protected one. And an item
    // that was protected is deleted first (a delete never asks for the check, an update of a bound item can).
    in_order(store, "set_generic_password(service, key, secret)", "delete_item(&meta_service(service), key)", "store");
    in_order(store, "read_meta(service, key)", "set_generic_password(service, key, secret)", "store");
    // Only iOS binds items: macOS needs the keychain-access-groups entitlement an unsigned build lacks.
    at(body(&code, "const fn can_bind_items("), "cfg!(target_os = \"ios\")", "can_bind_items");
}

/// A probe says what a read would do from the item's ATTRIBUTES (readable without the user check) and its record:
/// it never asks for the secret, so it can never show a sheet. `Invalidated` comes from the record outliving the item.
#[test]
fn the_probes_read_attributes_and_never_a_secret() {
    let code = code_of(KEYRING_APPLE_RS);
    let state = body(&code, "fn item_state(");
    at(state, ".load_attributes(true)", "item_state");
    not_at(state, ".load_data(true)", "item_state");
    let needs = body(&code, "pub(super) fn needs_prompt(");
    not_at(needs, ".load_data(true)", "needs_prompt");
    not_at(needs, "get_generic_password", "needs_prompt");
    at(needs, "read_meta(service, key)", "needs_prompt");
    at(needs, "item_state(service, key)", "needs_prompt");
    // `support` asks the device (canEvaluatePolicy), never evaluates a policy.
    let support = body(&code, "pub(super) fn support(");
    at(support, "device_state_for(level)", "support");
    not_at(&code, "evaluatePolicy", "the keyring never evaluates a policy itself");
    let device = body(&code, "fn device_state_for(");
    at(device, "canEvaluatePolicy_error", "device_state_for");
}

/// The item and its record live and die together: a delete removes both (a record without its item would read as an
/// invalidated one for ever) and a move of the service moves both.
#[test]
fn the_meta_record_follows_its_item() {
    let code = code_of(KEYRING_APPLE_RS);
    let delete = body(&code, "fn delete(");
    at(delete, "delete_item(service, key)", "delete");
    at(delete, "delete_item(&meta_service(service), key)", "delete");
    let moved = body(&code, "pub(super) fn move_entry(");
    at(moved, "move_one(key, from, to)", "move_entry");
    at(moved, "move_one(key, &meta_service(from), &meta_service(to))", "move_entry");
    // Moving changes the service in place: the secret is never read, so the move can never show a sheet.
    let one = body(&code, "fn move_one(");
    at(one, "update_item(", "move_one");
    not_at(one, "load_data", "move_one");
}

/// The LocalAuthentication sheet is never given an empty reason (`NSInvalidArgumentException`: an exception through
/// Rust frames is an abort), says which cancel button the app wanted, and shows no "Enter Password" button on a
/// biometrics-only prompt; every code `evaluatePolicy` answers goes through the one table.
#[test]
fn the_biometric_sheet_is_built_the_way_the_model_says() {
    let code = code_of(BIOMETRIC_APPLE_RS);
    let request = body(&code, "pub fn request(");
    at(request, "prompt_reason_or_default(", "request");
    at(request, "setLocalizedCancelTitle", "request");
    at(request, "setLocalizedFallbackTitle", "request");
    in_order(request, "prompt_reason_or_default(", "LAContext::new()", "request");
    let map = body(&code, "fn map_la_error(");
    at(map, "gate_outcome(", "map_la_error");
    // The keyring's unlock builds its sheet the same way.
    let keyring = code_of(KEYRING_APPLE_RS);
    let unlock = body(&keyring, "fn unlock(");
    at(unlock, "prompt_reason_or_default(", "unlock");
    at(unlock, "setLocalizedCancelTitle", "unlock");
}
