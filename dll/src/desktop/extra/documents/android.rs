//! Android: `Documents` over the storage access framework, through JNI.
//!
//! A locator on this platform is one of two things: a PATH (the app's own storage, the device's shared storage once the person
//! gave "All files access") handled by the desktop's backend over `std::fs`, or a `content://` document URI inside a folder the
//! person granted (`ACTION_OPEN_DOCUMENT_TREE`, kept with `takePersistableUriPermission`), handled here with `ContentResolver` and
//! `DocumentsContract`. The persisted grants ARE the list of places: `ContentResolver.getPersistedUriPermissions()`, no second
//! file.
//!
//! No Java of ours is involved: every call is a framework call made from Rust, where it can be read, and the decisions in it
//! (a row's flags, a provider's exception, the shape of a locator) are the pure functions of `azul_layout::documents_saf`, which
//! have host tests. The framework's constants (column names, flag bits, the folder type) are READ from
//! `DocumentsContract.Document` at run time, never copied.
//!
//! # Threads and time
//!
//! A provider is another app: a cloud provider answers in seconds or never, and a binder call cannot be cancelled. Every call
//! therefore runs on a worker thread of its own with a deadline ([`CallGuard`]: 20 s, then `TimedOut`; a provider that holds eight
//! late calls is refused at once). The worker attaches itself to the VM ([`with_env`]) and holds no reference of the caller.
//!
//! # What SAF cannot do, and what is done about it
//!
//! * A provider renames instead of failing when a name is taken ("a (1).txt"): creating checks the folder's names first (`Exists`).
//! * A write is in place (`"wt"`: truncate and write); there is no atomic replace across providers. The file is as it was only when
//!   the write never started: the app keeps its working copy until the write came back.
//! * A write ends with `ParcelFileDescriptor.close()`, not with a raw `close`: a provider that checks the data when the
//!   descriptor closes (a reliable pipe to a cloud drive) reports its error from there.
//! * A descriptor of a cloud document may be a pipe: a ranged read of a pipe reads and drops what comes before the range (up to a
//!   limit), and says so when the range is too far in.

use std::{fs::File, io::Write, mem::ManuallyDrop, os::fd::FromRawFd};

use azul_core::documents::{AllFilesAccess, DocumentEntry, DocumentError, Place, PlaceKind};
use azul_css::{AzString, OptionU64};
use azul_layout::{
    documents::{
        cap_listing, io_error, read_range, sort_listing, CallGuard, DocumentBackend, LocalDocuments, ReadSeek, MAX_ENTRIES,
    },
    documents_saf::{classify_failure, entry_from_row, readable_name, DocumentRow, FlagBits, Locator},
    mime,
};
use jni::{
    errors::Error,
    objects::{JObject, JObjectArray, JString, JValue},
    JNIEnv,
};

use crate::desktop::extra::file_picker::android::with_env;

const DOCUMENTS_CONTRACT: &str = "android/provider/DocumentsContract";
const DOCUMENT_CLASS: &str = "android/provider/DocumentsContract$Document";
/// `ContentResolver.query(uri, projection, selection, selectionArgs, sortOrder)`.
const RESOLVER_QUERY: &str =
    "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;";
/// `Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION`: what releasing a persisted grant is given.
const GRANT_FLAGS: i32 = 1 | 2;
/// How many persisted grants are listed as places (the system keeps at most 512).
const MAX_PLACES: i32 = 512;
/// How long the name of a place may take to ask for: a screen waits for it, so a hung provider shows under its URI's own words.
const PLACE_NAME_DEADLINE: std::time::Duration = std::time::Duration::from_secs(4);

/// Why a JNI step of a call failed.
#[derive(Debug)]
enum Failure {
    /// The framework threw: `Throwable.toString()` of the exception.
    Java(String),
    /// JNI itself said no (no exception pending).
    Jni(String),
    /// The backend already decided what the app is told.
    Decided(DocumentError),
}

impl Failure {
    fn into_error(self, what: &str) -> DocumentError {
        match self {
            Self::Java(throwable) => classify_failure(&throwable, what),
            Self::Jni(why) => DocumentError::Failed(AzString::from(format!("{what}: {why}"))),
            Self::Decided(error) => error,
        }
    }
}

/// The text of the exception a failed call left pending (cleared here), or `None` when none is pending.
fn take_exception(env: &mut JNIEnv<'_>) -> Option<String> {
    if !env.exception_check().unwrap_or(false) {
        return None;
    }
    let throwable = env.exception_occurred().ok()?;
    // cleared BEFORE the next JNI call: `toString` must not run with the exception pending
    let _ = env.exception_clear();
    let text = env.call_method(&throwable, "toString", "()Ljava/lang/String;", &[]).ok()?.l().ok()?;
    let text: String = env.get_string(&JString::from(text)).ok()?.into();
    Some(text)
}

/// A JNI result as a [`Failure`]: the exception a failed call left pending is taken and cleared here, because the next JNI call on
/// this thread must not run with one pending.
trait Jx<T> {
    fn jx(self, env: &mut JNIEnv<'_>) -> Result<T, Failure>;
}

impl<T> Jx<T> for Result<T, Error> {
    fn jx(self, env: &mut JNIEnv<'_>) -> Result<T, Failure> {
        self.map_err(|error| match take_exception(env) {
            Some(text) => Failure::Java(text),
            None => Failure::Jni(format!("{error}")),
        })
    }
}

fn decided<T>(error: DocumentError) -> Result<T, Failure> {
    Err(Failure::Decided(error))
}

fn text_of(env: &mut JNIEnv<'_>, object: JObject<'_>) -> Result<String, Failure> {
    env.get_string(&JString::from(object)).map(String::from).jx(env)
}

/// `object.<getter>()` as a `String` (`None` for a `null`).
fn string_of(env: &mut JNIEnv<'_>, object: &JObject<'_>, getter: &str) -> Result<Option<String>, Failure> {
    let value = env.call_method(object, getter, "()Ljava/lang/String;", &[]).and_then(|v| v.l()).jx(env)?;
    if value.is_null() {
        return Ok(None);
    }
    text_of(env, value).map(Some)
}

fn static_string(env: &mut JNIEnv<'_>, class: &str, field: &str) -> Result<String, Failure> {
    let value = env.get_static_field(class, field, "Ljava/lang/String;").and_then(|v| v.l()).jx(env)?;
    text_of(env, value)
}

fn static_int(env: &mut JNIEnv<'_>, class: &str, field: &str) -> Result<i32, Failure> {
    env.get_static_field(class, field, "I").and_then(|v| v.i()).jx(env)
}

fn parse_uri<'a>(env: &mut JNIEnv<'a>, text: &str) -> Result<JObject<'a>, Failure> {
    let text = env.new_string(text).jx(env)?;
    env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&text)])
        .and_then(|v| v.l())
        .jx(env)
}

fn resolver<'a>(env: &mut JNIEnv<'a>, activity: &JObject<'_>) -> Result<JObject<'a>, Failure> {
    env.call_method(activity, "getContentResolver", "()Landroid/content/ContentResolver;", &[])
        .and_then(|v| v.l())
        .jx(env)
}

fn string_array<'a>(env: &mut JNIEnv<'a>, items: &[String]) -> Result<JObjectArray<'a>, Failure> {
    let class = env.find_class("java/lang/String").jx(env)?;
    let null = JObject::null();
    let length = i32::try_from(items.len()).unwrap_or(i32::MAX);
    let array = env.new_object_array(length, &class, &null).jx(env)?;
    for (index, item) in (0..length).zip(items) {
        let text = env.new_string(item).jx(env)?;
        env.set_object_array_element(&array, index, &text).jx(env)?;
        let _ = env.delete_local_ref(text);
    }
    Ok(array)
}

/// The framework's own names and numbers, read from `DocumentsContract.Document` for this call.
struct Constants {
    /// `COLUMN_DOCUMENT_ID`, `COLUMN_DISPLAY_NAME`, `COLUMN_MIME_TYPE`, `COLUMN_SIZE`, `COLUMN_LAST_MODIFIED`, `COLUMN_FLAGS`.
    columns: Vec<String>,
    /// `MIME_TYPE_DIR`.
    dir_mime: String,
    bits: FlagBits,
}

fn constants(env: &mut JNIEnv<'_>) -> Result<Constants, Failure> {
    let mut columns = Vec::with_capacity(6);
    for field in [
        "COLUMN_DOCUMENT_ID",
        "COLUMN_DISPLAY_NAME",
        "COLUMN_MIME_TYPE",
        "COLUMN_SIZE",
        "COLUMN_LAST_MODIFIED",
        "COLUMN_FLAGS",
    ] {
        columns.push(static_string(env, DOCUMENT_CLASS, field)?);
    }
    Ok(Constants {
        columns,
        dir_mime: static_string(env, DOCUMENT_CLASS, "MIME_TYPE_DIR")?,
        bits: FlagBits {
            write: static_int(env, DOCUMENT_CLASS, "FLAG_SUPPORTS_WRITE")?,
            delete: static_int(env, DOCUMENT_CLASS, "FLAG_SUPPORTS_DELETE")?,
            create: static_int(env, DOCUMENT_CLASS, "FLAG_DIR_SUPPORTS_CREATE")?,
            rename: static_int(env, DOCUMENT_CLASS, "FLAG_SUPPORTS_RENAME")?,
        },
    })
}

// ---- cursors ----

/// `resolver.query(uri, projection, null, null, null)`; `None` when the provider answers with no cursor (the document is not there,
/// or the provider's process is gone).
fn query<'a>(
    env: &mut JNIEnv<'a>,
    resolver: &JObject<'_>,
    uri: &JObject<'_>,
    projection: &[String],
) -> Result<Option<JObject<'a>>, Failure> {
    let columns = string_array(env, projection)?;
    let null = JObject::null();
    let cursor = env
        .call_method(
            resolver,
            "query",
            RESOLVER_QUERY,
            &[
                JValue::Object(uri),
                JValue::Object(&columns),
                JValue::Object(&null),
                JValue::Object(&null),
                JValue::Object(&null),
            ],
        )
        .and_then(|v| v.l())
        .jx(env)?;
    Ok(if cursor.is_null() { None } else { Some(cursor) })
}

fn close_cursor(env: &mut JNIEnv<'_>, cursor: &JObject<'_>) {
    let _ = env.call_method(cursor, "close", "()V", &[]);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}

/// The indexes of the six columns of a cursor (a provider may answer with more columns, or in another order, than asked for).
struct Columns {
    id: i32,
    name: i32,
    mime: i32,
    size: i32,
    modified: i32,
    flags: i32,
}

fn column(env: &mut JNIEnv<'_>, cursor: &JObject<'_>, name: &str) -> Result<i32, Failure> {
    let key = env.new_string(name).jx(env)?;
    let index = env
        .call_method(cursor, "getColumnIndex", "(Ljava/lang/String;)I", &[JValue::Object(&key)])
        .and_then(|v| v.i())
        .jx(env);
    let _ = env.delete_local_ref(key);
    index
}

fn columns_of(env: &mut JNIEnv<'_>, cursor: &JObject<'_>, constants: &Constants) -> Result<Columns, Failure> {
    Ok(Columns {
        id: column(env, cursor, &constants.columns[0])?,
        name: column(env, cursor, &constants.columns[1])?,
        mime: column(env, cursor, &constants.columns[2])?,
        size: column(env, cursor, &constants.columns[3])?,
        modified: column(env, cursor, &constants.columns[4])?,
        flags: column(env, cursor, &constants.columns[5])?,
    })
}

fn cell_text(env: &mut JNIEnv<'_>, cursor: &JObject<'_>, index: i32) -> Result<Option<String>, Failure> {
    if index < 0 {
        return Ok(None);
    }
    let value = env
        .call_method(cursor, "getString", "(I)Ljava/lang/String;", &[JValue::Int(index)])
        .and_then(|v| v.l())
        .jx(env)?;
    if value.is_null() {
        return Ok(None);
    }
    text_of(env, value).map(Some)
}

fn cell_long(env: &mut JNIEnv<'_>, cursor: &JObject<'_>, index: i32) -> Result<Option<i64>, Failure> {
    if index < 0 {
        return Ok(None);
    }
    // `getLong` of a NULL cell answers 0, which is a size: the null is asked for first
    let null = env.call_method(cursor, "isNull", "(I)Z", &[JValue::Int(index)]).and_then(|v| v.z()).jx(env)?;
    if null {
        return Ok(None);
    }
    env.call_method(cursor, "getLong", "(I)J", &[JValue::Int(index)]).and_then(|v| v.j()).jx(env).map(Some)
}

fn cell_int(env: &mut JNIEnv<'_>, cursor: &JObject<'_>, index: i32) -> Result<i32, Failure> {
    if index < 0 {
        return Ok(0);
    }
    env.call_method(cursor, "getInt", "(I)I", &[JValue::Int(index)]).and_then(|v| v.i()).jx(env)
}

fn read_row(env: &mut JNIEnv<'_>, cursor: &JObject<'_>, columns: &Columns) -> Result<DocumentRow, Failure> {
    Ok(DocumentRow {
        document_id: cell_text(env, cursor, columns.id)?.unwrap_or_default(),
        display_name: cell_text(env, cursor, columns.name)?,
        mime: cell_text(env, cursor, columns.mime)?,
        size: cell_long(env, cursor, columns.size)?,
        modified_ms: cell_long(env, cursor, columns.modified)?,
        flags: cell_int(env, cursor, columns.flags)?,
    })
}

// ---- the operations (each runs on a worker with a deadline, see `AndroidDocuments::java`) ----

fn not_a_locator<T>(locator: &str) -> Result<T, Failure> {
    decided(DocumentError::Unsupported(AzString::from(format!(
        "{locator} is not a document of a folder the person granted (a content://.../tree/... URI)"
    ))))
}

fn first_entry(
    env: &mut JNIEnv<'_>,
    cursor: &JObject<'_>,
    constants: &Constants,
    uri: String,
    what: &str,
) -> Result<DocumentEntry, Failure> {
    let columns = columns_of(env, cursor, constants)?;
    let moved = env.call_method(cursor, "moveToFirst", "()Z", &[]).and_then(|v| v.z()).jx(env)?;
    if !moved {
        return decided(DocumentError::NotFound(AzString::from(what)));
    }
    let row = read_row(env, cursor, &columns)?;
    Ok(entry_from_row(uri, &row, &constants.dir_mime, constants.bits))
}

fn stat(env: &mut JNIEnv<'_>, activity: &JObject<'_>, locator: &str, what: &str) -> Result<DocumentEntry, Failure> {
    let Some(parsed) = Locator::parse(locator) else {
        return not_a_locator(locator);
    };
    let document = parsed.document_uri();
    let resolver = resolver(env, activity)?;
    let uri = parse_uri(env, &document)?;
    let constants = constants(env)?;
    let Some(cursor) = query(env, &resolver, &uri, &constants.columns)? else {
        return decided(DocumentError::NotFound(AzString::from(what)));
    };
    let entry = first_entry(env, &cursor, &constants, document, what);
    close_cursor(env, &cursor);
    entry
}

fn read_rows(
    env: &mut JNIEnv<'_>,
    cursor: &JObject<'_>,
    constants: &Constants,
    parsed: &Locator,
) -> Result<Vec<DocumentEntry>, Failure> {
    let columns = columns_of(env, cursor, constants)?;
    let mut entries = Vec::new();
    let mut unreadable = 0_usize;
    loop {
        let more = env.call_method(cursor, "moveToNext", "()Z", &[]).and_then(|v| v.z()).jx(env)?;
        if !more {
            break;
        }
        // the strings of one row are released with it: a folder of twenty thousand entries holds sixteen references, not a million
        let row = env.with_local_frame(16, |env| Ok::<_, Error>(read_row(env, cursor, &columns))).jx(env)?;
        let Ok(row) = row else {
            // a row the provider made unreadable (a size that is not a number): the listing goes on
            unreadable += 1;
            continue;
        };
        if let Some(uri) = parsed.child(&row.document_id) {
            entries.push(entry_from_row(uri, &row, &constants.dir_mime, constants.bits));
        }
        if entries.len() > MAX_ENTRIES {
            break;
        }
    }
    if unreadable > 0 {
        crate::plog_warn!("[documents] {unreadable} row(s) of a folder could not be read and are left out of its listing");
    }
    if cap_listing(&mut entries) {
        crate::plog_warn!("[documents] a folder holds more than {MAX_ENTRIES} entries: its listing is cut there");
    }
    sort_listing(&mut entries);
    Ok(entries)
}

fn list(env: &mut JNIEnv<'_>, activity: &JObject<'_>, locator: &str, what: &str) -> Result<Vec<DocumentEntry>, Failure> {
    let Some(parsed) = Locator::parse(locator) else {
        return not_a_locator(locator);
    };
    let Some(tree) = parsed.tree_uri() else {
        // one picked document has no folder around it
        return decided(DocumentError::NotADirectory(AzString::from(what)));
    };
    let resolver = resolver(env, activity)?;
    let tree = parse_uri(env, &tree)?;
    let document = env.new_string(parsed.document_id()).jx(env)?;
    let children = env
        .call_static_method(
            DOCUMENTS_CONTRACT,
            "buildChildDocumentsUriUsingTree",
            "(Landroid/net/Uri;Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&tree), JValue::Object(&document)],
        )
        .and_then(|v| v.l())
        .jx(env)?;
    let constants = constants(env)?;
    let Some(cursor) = query(env, &resolver, &children, &constants.columns)? else {
        return decided(DocumentError::NotFound(AzString::from(what)));
    };
    let entries = read_rows(env, &cursor, &constants, &parsed);
    close_cursor(env, &cursor);
    entries
}

/// `resolver.openFileDescriptor(uri, mode)`.
fn open_descriptor<'a>(
    env: &mut JNIEnv<'a>,
    resolver: &JObject<'_>,
    uri: &JObject<'_>,
    mode: &str,
    what: &str,
) -> Result<JObject<'a>, Failure> {
    let mode = env.new_string(mode).jx(env)?;
    let descriptor = env
        .call_method(
            resolver,
            "openFileDescriptor",
            "(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;",
            &[JValue::Object(uri), JValue::Object(&mode)],
        )
        .and_then(|v| v.l())
        .jx(env)?;
    if descriptor.is_null() {
        return decided(DocumentError::Failed(AzString::from(format!(
            "{what}: the provider has no file to open (a virtual document)"
        ))));
    }
    Ok(descriptor)
}

/// The document opened for reading, as a `File` that owns its descriptor (`detachFd`).
fn open_file(env: &mut JNIEnv<'_>, activity: &JObject<'_>, locator: &str, what: &str) -> Result<File, Failure> {
    let Some(parsed) = Locator::parse(locator) else {
        return not_a_locator(locator);
    };
    let resolver = resolver(env, activity)?;
    let uri = parse_uri(env, &parsed.document_uri())?;
    let descriptor = open_descriptor(env, &resolver, &uri, "r", what)?;
    let fd = env.call_method(&descriptor, "detachFd", "()I", &[]).and_then(|v| v.i()).jx(env)?;
    if fd < 0 {
        return decided(DocumentError::Failed(AzString::from(format!("{what}: the provider gave no file descriptor"))));
    }
    // SAFETY: `detachFd` hands the ownership of an open descriptor over: this `File` closes it.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn read(
    env: &mut JNIEnv<'_>,
    activity: &JObject<'_>,
    locator: &str,
    what: &str,
    offset: u64,
    len: u64,
) -> Result<Vec<u8>, Failure> {
    let mut file = open_file(env, activity, locator, what)?;
    read_range(&mut file, what, offset, len).map_err(Failure::Decided)
}

fn write_descriptor(fd: i32, bytes: &[u8]) -> std::io::Result<()> {
    // SAFETY: the descriptor belongs to the `ParcelFileDescriptor`, which the caller closes after this returns; `ManuallyDrop`
    // keeps this `File` from closing it first.
    let mut file = ManuallyDrop::new(unsafe { File::from_raw_fd(fd) });
    file.write_all(bytes)?;
    file.flush()
}

fn write(
    env: &mut JNIEnv<'_>,
    activity: &JObject<'_>,
    locator: &str,
    what: &str,
    bytes: &[u8],
) -> Result<DocumentEntry, Failure> {
    let Some(parsed) = Locator::parse(locator) else {
        return not_a_locator(locator);
    };
    let resolver = resolver(env, activity)?;
    let uri = parse_uri(env, &parsed.document_uri())?;
    // "wt": write and TRUNCATE ("w" alone does not truncate from Android 10)
    let descriptor = open_descriptor(env, &resolver, &uri, "wt", what)?;
    let fd = env.call_method(&descriptor, "getFd", "()I", &[]).and_then(|v| v.i()).jx(env)?;
    let written = write_descriptor(fd, bytes);
    // `close()` of the descriptor, not a raw close: the provider of a reliable pipe reports what it found wrong from here
    let closed = env.call_method(&descriptor, "close", "()V", &[]).jx(env);
    written.map_err(|e| Failure::Decided(io_error(what, &e)))?;
    closed?;
    stat(env, activity, locator, what)
}

/// The folder's entries by lower-cased name: the provider renames instead of refusing when a name is taken, so the names are
/// asked for first.
fn name_is_taken(env: &mut JNIEnv<'_>, activity: &JObject<'_>, parent: &str, what: &str, name: &str) -> Result<bool, Failure> {
    let wanted = name.to_lowercase();
    Ok(list(env, activity, parent, what)?.iter().any(|entry| entry.name.as_str().to_lowercase() == wanted))
}

fn create(
    env: &mut JNIEnv<'_>,
    activity: &JObject<'_>,
    parent: &str,
    what: &str,
    name: &str,
    mime_type: Option<&str>,
) -> Result<DocumentEntry, Failure> {
    let Some(folder) = Locator::parse(parent) else {
        return not_a_locator(parent);
    };
    if name_is_taken(env, activity, parent, what, name)? {
        return decided(DocumentError::Exists(AzString::from(name)));
    }
    let constants = constants(env)?;
    let kind = match mime_type {
        None => constants.dir_mime.clone(),
        Some("") => mime::for_file_name(name).to_string(),
        Some(given) => given.to_string(),
    };
    let resolver = resolver(env, activity)?;
    let parent_uri = parse_uri(env, &folder.document_uri())?;
    let kind = env.new_string(kind).jx(env)?;
    let display_name = env.new_string(name).jx(env)?;
    let created = env
        .call_static_method(
            DOCUMENTS_CONTRACT,
            "createDocument",
            "(Landroid/content/ContentResolver;Landroid/net/Uri;Ljava/lang/String;Ljava/lang/String;)Landroid/net/Uri;",
            &[
                JValue::Object(&resolver),
                JValue::Object(&parent_uri),
                JValue::Object(&kind),
                JValue::Object(&display_name),
            ],
        )
        .and_then(|v| v.l())
        .jx(env)?;
    if created.is_null() {
        return decided(DocumentError::Failed(AzString::from(format!("{what}: the provider would not create {name}"))));
    }
    let Some(created) = string_of(env, &created, "toString")? else {
        return decided(DocumentError::Failed(AzString::from(format!("{what}: the provider gave no locator for {name}"))));
    };
    stat(env, activity, &created, name)
}

fn rename(env: &mut JNIEnv<'_>, activity: &JObject<'_>, locator: &str, what: &str, new_name: &str) -> Result<DocumentEntry, Failure> {
    // asked first: a missing document is `NotFound`, a provider that cannot rename says so before anything is tried
    let before = stat(env, activity, locator, what)?;
    if !before.can_rename {
        return decided(DocumentError::Unsupported(AzString::from(format!("{what} cannot be renamed here"))));
    }
    let Some(parsed) = Locator::parse(locator) else {
        return not_a_locator(locator);
    };
    let resolver = resolver(env, activity)?;
    let uri = parse_uri(env, &parsed.document_uri())?;
    let name = env.new_string(new_name).jx(env)?;
    let renamed = env
        .call_static_method(
            DOCUMENTS_CONTRACT,
            "renameDocument",
            "(Landroid/content/ContentResolver;Landroid/net/Uri;Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&resolver), JValue::Object(&uri), JValue::Object(&name)],
        )
        .and_then(|v| v.l())
        .jx(env)?;
    if renamed.is_null() {
        return decided(DocumentError::Failed(AzString::from(format!("{what}: the provider would not rename it to {new_name}"))));
    }
    let Some(renamed) = string_of(env, &renamed, "toString")? else {
        return decided(DocumentError::Failed(AzString::from(format!("{what}: the provider gave no locator for {new_name}"))));
    };
    stat(env, activity, &renamed, new_name)
}

fn delete(env: &mut JNIEnv<'_>, activity: &JObject<'_>, locator: &str, what: &str) -> Result<(), Failure> {
    // a provider answers `false` for a missing document and for one it could not delete alike: the document is asked for first
    stat(env, activity, locator, what)?;
    let Some(parsed) = Locator::parse(locator) else {
        return not_a_locator(locator);
    };
    let resolver = resolver(env, activity)?;
    let uri = parse_uri(env, &parsed.document_uri())?;
    let deleted = env
        .call_static_method(
            DOCUMENTS_CONTRACT,
            "deleteDocument",
            "(Landroid/content/ContentResolver;Landroid/net/Uri;)Z",
            &[JValue::Object(&resolver), JValue::Object(&uri)],
        )
        .and_then(|v| v.z())
        .jx(env)?;
    if deleted {
        Ok(())
    } else {
        decided(DocumentError::Failed(AzString::from(format!("{what} could not be deleted"))))
    }
}

fn forget(env: &mut JNIEnv<'_>, activity: &JObject<'_>, locator: &str) -> Result<(), Failure> {
    let Some(parsed) = Locator::parse(locator) else {
        return not_a_locator(locator);
    };
    // the grant is kept under the URI the person's picker gave: the folder's tree URI, or the document's own
    let granted = parsed.tree_uri().unwrap_or_else(|| parsed.document_uri());
    let resolver = resolver(env, activity)?;
    let uri = parse_uri(env, &granted)?;
    env.call_method(
        &resolver,
        "releasePersistableUriPermission",
        "(Landroid/net/Uri;I)V",
        &[JValue::Object(&uri), JValue::Int(GRANT_FLAGS)],
    )
    .jx(env)
    .map(|_| ())
}

// ---- places and "All files access" ----

/// One persisted grant, as the framework lists it.
struct Grant {
    locator: String,
    writable: bool,
    granted_ms: Option<u64>,
}

/// What the framework says without asking any provider: the app's folder, the grants, the state of "All files access".
struct RawPlaces {
    files_dir: String,
    grants: Vec<Grant>,
    all_files: AllFilesAccess,
    shared_storage: Option<String>,
}

fn declared_permission(env: &mut JNIEnv<'_>, activity: &JObject<'_>, permission: &str) -> Result<bool, Failure> {
    let package = string_of(env, activity, "getPackageName")?.unwrap_or_default();
    let manager = env
        .call_method(activity, "getPackageManager", "()Landroid/content/pm/PackageManager;", &[])
        .and_then(|v| v.l())
        .jx(env)?;
    let flag = static_int(env, "android/content/pm/PackageManager", "GET_PERMISSIONS")?;
    let package = env.new_string(package).jx(env)?;
    let info = env
        .call_method(
            &manager,
            "getPackageInfo",
            "(Ljava/lang/String;I)Landroid/content/pm/PackageInfo;",
            &[JValue::Object(&package), JValue::Int(flag)],
        )
        .and_then(|v| v.l())
        .jx(env)?;
    let requested = env.get_field(&info, "requestedPermissions", "[Ljava/lang/String;").and_then(|v| v.l()).jx(env)?;
    if requested.is_null() {
        return Ok(false);
    }
    let requested = JObjectArray::from(requested);
    let count = env.get_array_length(&requested).jx(env)?;
    for index in 0..count {
        let item = env.get_object_array_element(&requested, index).jx(env)?;
        if !item.is_null() && text_of(env, item)? == permission {
            return Ok(true);
        }
    }
    Ok(false)
}

fn all_files_access(env: &mut JNIEnv<'_>, activity: &JObject<'_>) -> Result<AllFilesAccess, Failure> {
    // `MANAGE_EXTERNAL_STORAGE` exists from Android 11 (API 30); a build that does not declare it has no such thing to ask for
    if static_int(env, "android/os/Build$VERSION", "SDK_INT")? < 30
        || !declared_permission(env, activity, "android.permission.MANAGE_EXTERNAL_STORAGE")?
    {
        return Ok(AllFilesAccess::Unavailable);
    }
    let granted = env
        .call_static_method("android/os/Environment", "isExternalStorageManager", "()Z", &[])
        .and_then(|v| v.z())
        .jx(env)?;
    Ok(if granted { AllFilesAccess::Granted } else { AllFilesAccess::NotGranted })
}

fn request_all_files_access(env: &mut JNIEnv<'_>, activity: &JObject<'_>) -> Result<(), Failure> {
    let package = string_of(env, activity, "getPackageName")?.unwrap_or_default();
    let uri = parse_uri(env, &format!("package:{package}"))?;
    let action = env.new_string("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION").jx(env)?;
    let intent = env
        .new_object(
            "android/content/Intent",
            "(Ljava/lang/String;Landroid/net/Uri;)V",
            &[JValue::Object(&action), JValue::Object(&uri)],
        )
        .jx(env)?;
    env.call_method(activity, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])
        .jx(env)
        .map(|_| ())
}

fn raw_places(env: &mut JNIEnv<'_>, activity: &JObject<'_>) -> Result<RawPlaces, Failure> {
    let files = env.call_method(activity, "getFilesDir", "()Ljava/io/File;", &[]).and_then(|v| v.l()).jx(env)?;
    let files_dir = string_of(env, &files, "getAbsolutePath")?.unwrap_or_default();
    let all_files = all_files_access(env, activity)?;
    let shared_storage = if all_files == AllFilesAccess::Granted {
        let root = env
            .call_static_method("android/os/Environment", "getExternalStorageDirectory", "()Ljava/io/File;", &[])
            .and_then(|v| v.l())
            .jx(env)?;
        string_of(env, &root, "getAbsolutePath")?
    } else {
        None
    };
    let resolver = resolver(env, activity)?;
    let list = env
        .call_method(&resolver, "getPersistedUriPermissions", "()Ljava/util/List;", &[])
        .and_then(|v| v.l())
        .jx(env)?;
    let count = env.call_method(&list, "size", "()I", &[]).and_then(|v| v.i()).jx(env)?.min(MAX_PLACES);
    let mut grants = Vec::new();
    for index in 0..count {
        let grant = env.with_local_frame(16, |env| Ok::<_, Error>(read_grant(env, &list, index))).jx(env)?;
        match grant {
            Ok(Some(grant)) => grants.push(grant),
            Ok(None) => {}
            Err(why) => crate::plog_debug!("[documents] a persisted grant could not be read: {why:?}"),
        }
    }
    Ok(RawPlaces { files_dir, grants, all_files, shared_storage })
}

/// The grant at `index` of the persisted permissions; `None` for one that is not a folder (a single document the person picked).
fn read_grant(env: &mut JNIEnv<'_>, list: &JObject<'_>, index: i32) -> Result<Option<Grant>, Failure> {
    let permission = env
        .call_method(list, "get", "(I)Ljava/lang/Object;", &[JValue::Int(index)])
        .and_then(|v| v.l())
        .jx(env)?;
    let uri = env.call_method(&permission, "getUri", "()Landroid/net/Uri;", &[]).and_then(|v| v.l()).jx(env)?;
    let Some(text) = string_of(env, &uri, "toString")? else {
        return Ok(None);
    };
    let Some(tree @ Locator::Tree { .. }) = Locator::parse(&text) else {
        return Ok(None);
    };
    let writable = env.call_method(&permission, "isWritePermission", "()Z", &[]).and_then(|v| v.z()).jx(env)?;
    let time = env.call_method(&permission, "getPersistedTime", "()J", &[]).and_then(|v| v.j()).jx(env)?;
    Ok(Some(Grant {
        locator: tree.document_uri(),
        writable,
        // `UriPermission.INVALID_TIME` is `Long.MIN_VALUE`: a grant that was never persisted
        granted_ms: u64::try_from(time).ok().filter(|ms| *ms > 0),
    }))
}

// ---- the backend ----

/// `Documents` of an Android phone: paths by the desktop's backend, `content://` documents through the storage framework.
#[derive(Debug)]
pub(super) struct AndroidDocuments {
    /// The backend of every locator that is a path.
    local: LocalDocuments,
    /// The deadline of the calls into providers, and the count of those that outlived it.
    guard: CallGuard,
}

fn is_content(locator: &str) -> bool {
    locator.starts_with("content://")
}

impl AndroidDocuments {
    pub(super) fn new() -> Self {
        Self {
            local: LocalDocuments::default(),
            guard: CallGuard::default(),
        }
    }

    /// Run `call` for the document `locator` on a worker with the guard's deadline: attached to the VM, with the activity, the
    /// locator and the name the person knows it by; a failure of the framework becomes the error the app is told.
    fn java<T: Send + 'static>(
        &self,
        locator: &str,
        call: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>, &str, &str) -> Result<T, Failure> + Send + 'static,
    ) -> Result<T, DocumentError> {
        self.java_within(None, locator, call)
    }

    fn java_within<T: Send + 'static>(
        &self,
        deadline: Option<std::time::Duration>,
        locator: &str,
        call: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>, &str, &str) -> Result<T, Failure> + Send + 'static,
    ) -> Result<T, DocumentError> {
        let locator = locator.to_string();
        let what = readable_name(&locator);
        let label = what.clone();
        let work = move || match with_env(|env, activity| Ok::<_, Error>(call(env, &activity, &locator, &what))) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(failure)) => Err(failure.into_error(&what)),
            Err(why) => Err(DocumentError::Failed(AzString::from(format!("{what}: {why}")))),
        };
        match deadline {
            Some(limit) => self.guard.run_within(limit, &label, work),
            None => self.guard.run(&label, work),
        }
    }
}

impl DocumentBackend for AndroidDocuments {
    fn name(&self) -> &str {
        "android-saf"
    }

    fn places(&self) -> Vec<Place> {
        let raw = match self.java("the places of this phone", |env, activity, _, _| raw_places(env, activity)) {
            Ok(raw) => raw,
            Err(why) => {
                crate::plog_warn!("[documents] the places of this phone could not be listed: {}", why.describe());
                return Vec::new();
            }
        };
        let mut places = vec![Place {
            uri: AzString::from(raw.files_dir),
            name: AzString::from("App storage"),
            writable: true,
            granted_ms: OptionU64::None,
            kind: PlaceKind::AppStorage,
        }];
        let mut granted = Vec::new();
        for grant in raw.grants {
            // the framework knows the URI of a grant, not its name: the provider is asked, and a provider that does not answer in
            // a few seconds shows under the last piece of its URI instead of holding the list back
            let name = self
                .java_within(Some(PLACE_NAME_DEADLINE), &grant.locator, |env, activity, locator, what| {
                    stat(env, activity, locator, what)
                })
                .map_or_else(|_| readable_name(&grant.locator), |entry| entry.name.as_str().to_string());
            granted.push(Place {
                uri: AzString::from(grant.locator),
                name: AzString::from(name),
                writable: grant.writable,
                granted_ms: grant.granted_ms.map_or(OptionU64::None, OptionU64::Some),
                kind: PlaceKind::Granted,
            });
        }
        granted.sort_by_key(|place| place.name.as_str().to_lowercase());
        places.extend(granted);
        if let Some(root) = raw.shared_storage {
            places.push(Place {
                uri: AzString::from(root),
                name: AzString::from("All files"),
                writable: true,
                granted_ms: OptionU64::None,
                kind: PlaceKind::SharedStorage,
            });
        }
        places
    }

    fn forget(&self, locator: &str) -> Result<(), DocumentError> {
        if !is_content(locator) {
            return self.local.forget(locator);
        }
        self.java(locator, |env, activity, locator, _| forget(env, activity, locator))
    }

    fn stat(&self, locator: &str) -> Result<DocumentEntry, DocumentError> {
        if !is_content(locator) {
            return self.local.stat(locator);
        }
        self.java(locator, stat)
    }

    fn children(&self, locator: &str) -> Result<Vec<DocumentEntry>, DocumentError> {
        if !is_content(locator) {
            return self.local.children(locator);
        }
        self.java(locator, list)
    }

    fn read(&self, locator: &str, offset: u64, len: u64) -> Result<Vec<u8>, DocumentError> {
        if !is_content(locator) {
            return self.local.read(locator, offset, len);
        }
        self.java(locator, move |env, activity, locator, what| read(env, activity, locator, what, offset, len))
    }

    fn open_read(&self, locator: &str) -> Result<Box<dyn ReadSeek>, DocumentError> {
        if !is_content(locator) {
            return self.local.open_read(locator);
        }
        self.java(locator, |env, activity, locator, what| {
            open_file(env, activity, locator, what).map(|file| Box::new(file) as Box<dyn ReadSeek>)
        })
    }

    fn write(&self, locator: &str, bytes: &[u8]) -> Result<DocumentEntry, DocumentError> {
        if !is_content(locator) {
            return self.local.write(locator, bytes);
        }
        // the worker may outlive a caller that gave up: it owns its copy of the bytes
        let bytes = bytes.to_vec();
        self.java(locator, move |env, activity, locator, what| write(env, activity, locator, what, &bytes))
    }

    fn create_file(&self, parent: &str, name: &str, mime_type: &str) -> Result<DocumentEntry, DocumentError> {
        if !is_content(parent) {
            return self.local.create_file(parent, name, mime_type);
        }
        let (name, mime_type) = (name.to_string(), mime_type.to_string());
        self.java(parent, move |env, activity, parent, what| {
            create(env, activity, parent, what, &name, Some(&mime_type))
        })
    }

    fn create_dir(&self, parent: &str, name: &str) -> Result<DocumentEntry, DocumentError> {
        if !is_content(parent) {
            return self.local.create_dir(parent, name);
        }
        let name = name.to_string();
        self.java(parent, move |env, activity, parent, what| create(env, activity, parent, what, &name, None))
    }

    fn rename(&self, locator: &str, new_name: &str) -> Result<DocumentEntry, DocumentError> {
        if !is_content(locator) {
            return self.local.rename(locator, new_name);
        }
        let new_name = new_name.to_string();
        self.java(locator, move |env, activity, locator, what| rename(env, activity, locator, what, &new_name))
    }

    fn delete(&self, locator: &str) -> Result<(), DocumentError> {
        if !is_content(locator) {
            return self.local.delete(locator);
        }
        self.java(locator, delete)
    }

    fn all_files_access(&self) -> AllFilesAccess {
        self.java("All files access", |env, activity, _, _| all_files_access(env, activity))
            .unwrap_or(AllFilesAccess::Unavailable)
    }

    fn request_all_files_access(&self) -> bool {
        // the system's page is shown by the activity of the caller's tap: no provider is asked, so no deadline is needed
        match with_env(|env, activity| Ok::<_, Error>(request_all_files_access(env, &activity))) {
            Ok(Ok(())) => true,
            Ok(Err(failure)) => {
                crate::plog_warn!("[documents] the page of \"All files access\" could not be shown: {failure:?}");
                false
            }
            Err(why) => {
                crate::plog_warn!("[documents] the page of \"All files access\" could not be shown: {why}");
                false
            }
        }
    }
}
