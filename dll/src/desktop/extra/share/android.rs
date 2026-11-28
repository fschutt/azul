//! Android: the system's chooser for `CallbackInfo::share`, built and started from Rust through JNI.
//!
//! No Java helper of ours is involved except the provider that serves the shared files
//! (`scripts/android/AzulShareProvider.java`, a `ContentProvider` the manifest names): everything else is framework
//! calls - `Intent`, `ClipData`, `Uri`, `startActivity` - made here, where they can be read, tested against the
//! pure parts (`azul_layout::managers::share*`) and later generated into a class by the Rust DEX writer.
//!
//! What happens, on the thread the callback runs on:
//!
//! 1. the files of the plan are COPIED into `<cache>/azul-share/<token>/` ([`ShareArea::stage`]): the receiver gets
//!    exactly these files under the names the app gave them, never the app's own folder;
//! 2. each copy gets the URI `content://<package>.azul.share/<token>/<encoded name>` ([`content_uri`]), which the
//!    provider of the app serves read-only;
//! 3. the intent: `ACTION_SEND` (one file or only text), `ACTION_SEND_MULTIPLE` (several files) or, for "open with",
//!    `ACTION_VIEW` of the one file; the URIs are in `EXTRA_STREAM` AND in a `ClipData` (the chooser copies the clip data
//!    and the grant flag of its target intent, which is what lets the chosen app read them), `FLAG_GRANT_READ_URI_PERMISSION`,
//!    `EXTRA_TEXT` / `EXTRA_SUBJECT` / `EXTRA_TITLE` where the request has a text or a title;
//! 4. `Intent.createChooser` and `startActivity`, whose exception (no app can take it, the system refused) is the
//!    `Failed` reason, with the staged copies removed again.
//!
//! `Context.startActivity` may be called from any thread (`AzulUrl.open` does the same from the loop thread).

use std::path::PathBuf;

use azul_core::share::ShareStatus;
use azul_layout::managers::{
    share::SharePlan,
    share_area::{content_uri, ShareArea, StagedShare},
};
use jni::{
    errors::Error,
    objects::{JObject, JString, JValue},
    JNIEnv,
};

use crate::desktop::extra::file_picker::android::with_env;

/// The authority suffix of the app's share provider: `<package>` + this. The manifest's `<provider>` names the same
/// (`@PACKAGE@.azul.share`); `dll/tests/android_share_open_contract.rs` pins both.
const AUTHORITY_SUFFIX: &str = ".azul.share";
/// The folder of the staged shares under the app's cache directory; `AzulShareProvider` serves exactly this one.
const AREA_FOLDER: &str = "azul-share";

const ACTION_SEND: &str = "android.intent.action.SEND";
const ACTION_SEND_MULTIPLE: &str = "android.intent.action.SEND_MULTIPLE";
const ACTION_VIEW: &str = "android.intent.action.VIEW";
const EXTRA_STREAM: &str = "android.intent.extra.STREAM";
const EXTRA_TEXT: &str = "android.intent.extra.TEXT";
const EXTRA_SUBJECT: &str = "android.intent.extra.SUBJECT";
const EXTRA_TITLE: &str = "android.intent.extra.TITLE";
/// `Intent.FLAG_GRANT_READ_URI_PERMISSION`.
const FLAG_GRANT_READ_URI_PERMISSION: i32 = 1;

/// Show the chooser for `plan`, or say why not.
pub(super) fn share(plan: &SharePlan) -> ShareStatus {
    match with_env(|env, activity| start(env, &activity, plan)) {
        Ok(status) => status,
        Err(why) => ShareStatus::Failed(why.into()),
    }
}

/// The whole share on the calling thread's `env`. `Err` is a JNI failure the caller reports as `Failed`; a Java exception is
/// turned into the status here, with its text.
fn start(env: &mut JNIEnv<'_>, activity: &JObject<'_>, plan: &SharePlan) -> Result<ShareStatus, Error> {
    let package = string_of(env, activity, "getPackageName")?;
    let mut staged: Option<StagedShare> = None;
    if plan.files().next().is_some() {
        let cache = {
            let dir = env
                .call_method(activity, "getCacheDir", "()Ljava/io/File;", &[])?
                .l()?;
            let path = env
                .call_method(&dir, "getAbsolutePath", "()Ljava/lang/String;", &[])?
                .l()?;
            let text: String = env.get_string(&JString::from(path))?.into();
            text
        };
        let area = ShareArea::new(PathBuf::from(cache).join(AREA_FOLDER));
        match area.stage(plan, &plan.limits) {
            Ok(done) => staged = Some(done),
            Err(status) => return Ok(status),
        }
    }
    let authority = format!("{package}{AUTHORITY_SUFFIX}");
    match build_and_start(env, activity, plan, staged.as_ref(), &authority) {
        Ok(()) => Ok(ShareStatus::Shown),
        Err(error) => {
            // what was copied for a sheet that never came up is not left behind
            if let Some(done) = &staged {
                drop(std::fs::remove_dir_all(&done.dir));
            }
            let why = take_exception(env).unwrap_or_else(|| format!("{error:?}"));
            Ok(ShareStatus::Failed(describe_failure(&why).into()))
        }
    }
}

/// `activity.<getter>()` as a Rust string.
fn string_of(env: &mut JNIEnv<'_>, object: &JObject<'_>, getter: &str) -> Result<String, Error> {
    let value = env
        .call_method(object, getter, "()Ljava/lang/String;", &[])?
        .l()?;
    let text: String = env.get_string(&JString::from(value))?.into();
    Ok(text)
}

/// A `Uri` of `text` (`Uri.parse`).
fn parse_uri<'a>(env: &mut JNIEnv<'a>, text: &str) -> Result<JObject<'a>, Error> {
    let text = env.new_string(text)?;
    let uri = env
        .call_static_method(
            "android/net/Uri",
            "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&text)],
        )?
        .l()?;
    env.delete_local_ref(text)?;
    Ok(uri)
}

/// `intent.putExtra(name, text)` for a `CharSequence` extra.
fn put_text_extra(env: &mut JNIEnv<'_>, intent: &JObject<'_>, name: &str, text: &str) -> Result<(), Error> {
    env.with_local_frame(8, |env| {
        let key = env.new_string(name)?;
        let value = env.new_string(text)?;
        env.call_method(
            intent,
            "putExtra",
            "(Ljava/lang/String;Ljava/lang/CharSequence;)Landroid/content/Intent;",
            &[JValue::Object(&key), JValue::Object(&value)],
        )?;
        Ok(())
    })
}

/// The intent of the plan, the chooser around it and `startActivity`.
fn build_and_start(
    env: &mut JNIEnv<'_>,
    activity: &JObject<'_>,
    plan: &SharePlan,
    staged: Option<&StagedShare>,
    authority: &str,
) -> Result<(), Error> {
    let files = staged.map_or(&[][..], |s| s.files.as_slice());
    let action = if plan.open_with {
        ACTION_VIEW
    } else if files.len() > 1 {
        ACTION_SEND_MULTIPLE
    } else {
        ACTION_SEND
    };
    let mime = plan.mime();

    let intent = {
        let action = env.new_string(action)?;
        env.new_object(
            "android/content/Intent",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&action)],
        )?
    };

    // the files: the first URI types and carries the intent, the rest join it
    if let Some(staged) = staged {
        let list = if files.len() > 1 {
            Some(env.new_object("java/util/ArrayList", "()V", &[])?)
        } else {
            None
        };
        let mut first: Option<JObject<'_>> = None;
        let mut clip: Option<JObject<'_>> = None;
        for (i, file) in files.iter().enumerate() {
            let text = content_uri(authority, &staged.token, &file.name);
            if i == 0 {
                let uri = parse_uri(env, &text)?;
                let label = env.new_string("azul-share")?;
                let data = env
                    .call_static_method(
                        "android/content/ClipData",
                        "newRawUri",
                        "(Ljava/lang/CharSequence;Landroid/net/Uri;)Landroid/content/ClipData;",
                        &[JValue::Object(&label), JValue::Object(&uri)],
                    )?
                    .l()?;
                env.delete_local_ref(label)?;
                if let Some(list) = &list {
                    env.call_method(list, "add", "(Ljava/lang/Object;)Z", &[JValue::Object(&uri)])?;
                }
                first = Some(uri);
                clip = Some(data);
            } else if let (Some(clip), Some(list)) = (&clip, &list) {
                env.with_local_frame(8, |env| {
                    let uri = parse_uri(env, &text)?;
                    let item = env.new_object(
                        "android/content/ClipData$Item",
                        "(Landroid/net/Uri;)V",
                        &[JValue::Object(&uri)],
                    )?;
                    env.call_method(
                        clip,
                        "addItem",
                        "(Landroid/content/ClipData$Item;)V",
                        &[JValue::Object(&item)],
                    )?;
                    env.call_method(list, "add", "(Ljava/lang/Object;)Z", &[JValue::Object(&uri)])?;
                    Ok::<(), Error>(())
                })?;
            }
        }
        if let (Some(first), Some(clip)) = (&first, &clip) {
            if plan.open_with {
                let kind = env.new_string(&mime)?;
                env.call_method(
                    &intent,
                    "setDataAndType",
                    "(Landroid/net/Uri;Ljava/lang/String;)Landroid/content/Intent;",
                    &[JValue::Object(first), JValue::Object(&kind)],
                )?;
                env.delete_local_ref(kind)?;
            } else {
                let kind = env.new_string(&mime)?;
                env.call_method(
                    &intent,
                    "setType",
                    "(Ljava/lang/String;)Landroid/content/Intent;",
                    &[JValue::Object(&kind)],
                )?;
                env.delete_local_ref(kind)?;
                let key = env.new_string(EXTRA_STREAM)?;
                if let Some(list) = &list {
                    env.call_method(
                        &intent,
                        "putParcelableArrayListExtra",
                        "(Ljava/lang/String;Ljava/util/ArrayList;)Landroid/content/Intent;",
                        &[JValue::Object(&key), JValue::Object(list)],
                    )?;
                } else {
                    env.call_method(
                        &intent,
                        "putExtra",
                        "(Ljava/lang/String;Landroid/os/Parcelable;)Landroid/content/Intent;",
                        &[JValue::Object(&key), JValue::Object(first)],
                    )?;
                }
                env.delete_local_ref(key)?;
            }
            env.call_method(
                &intent,
                "setClipData",
                "(Landroid/content/ClipData;)V",
                &[JValue::Object(clip)],
            )?;
            env.call_method(
                &intent,
                "addFlags",
                "(I)Landroid/content/Intent;",
                &[JValue::Int(FLAG_GRANT_READ_URI_PERMISSION)],
            )?;
        }
    } else {
        let kind = env.new_string("text/plain")?;
        env.call_method(
            &intent,
            "setType",
            "(Ljava/lang/String;)Landroid/content/Intent;",
            &[JValue::Object(&kind)],
        )?;
        env.delete_local_ref(kind)?;
    }

    // texts and links ride along as the text of the share (one file and a line of text is a normal share); several
    // files take no text: the extra of a multiple share is a list of its own
    let texts = plan.texts();
    if !texts.is_empty() && files.len() <= 1 && !plan.open_with {
        put_text_extra(env, &intent, EXTRA_TEXT, &texts.join("\n"))?;
    }
    if !plan.title.is_empty() && !plan.open_with {
        put_text_extra(env, &intent, EXTRA_SUBJECT, &plan.title)?;
        put_text_extra(env, &intent, EXTRA_TITLE, &plan.title)?;
    }

    // the chooser: the system's list of apps for the intent (for "open with": the apps that can open the file)
    let title = if plan.title.is_empty() {
        None
    } else {
        Some(env.new_string(&plan.title)?)
    };
    let null = JObject::null();
    let title_arg = title.as_ref().map_or(&null, |t| &**t);
    let chooser = env
        .call_static_method(
            "android/content/Intent",
            "createChooser",
            "(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;",
            &[JValue::Object(&intent), JValue::Object(title_arg)],
        )?
        .l()?;
    env.call_method(
        activity,
        "startActivity",
        "(Landroid/content/Intent;)V",
        &[JValue::Object(&chooser)],
    )?;
    Ok(())
}

/// The text of the exception a failed call left pending (cleared here), or `None` when none is pending.
fn take_exception(env: &mut JNIEnv<'_>) -> Option<String> {
    if !env.exception_check().unwrap_or(false) {
        return None;
    }
    let throwable = env.exception_occurred().ok()?;
    let _ = env.exception_clear();
    let text = env
        .call_method(&throwable, "toString", "()Ljava/lang/String;", &[])
        .ok()?
        .l()
        .ok()?;
    let text: String = env.get_string(&JString::from(text)).ok()?.into();
    Some(text)
}

/// The words for the user of a failure of the chooser: an app that cannot be found is what a person meets; everything else
/// is said with the system's own text.
fn describe_failure(exception: &str) -> String {
    if exception.contains("ActivityNotFoundException") {
        "no app on this phone can receive this".to_string()
    } else if exception.contains("SecurityException") {
        format!("the system refused to share this ({exception})")
    } else {
        format!("the share sheet could not be shown ({exception})")
    }
}
