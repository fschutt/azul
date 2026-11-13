//! Share links of an encrypted remote: azul-storage's shares (`sharing::share`), made on a
//! device with its key and opened the way a visitor without an account opens them.
//!
//! A link names one share: a manifest in the bucket (`.azlin/shares/<id>.azs`) sealed with
//! a share key that only the link carries, listing each shared file's object and its file
//! key wrapped by that share key. The drive key never goes into it. A share is a snapshot:
//! a file added to a shared folder later is not in it.
//!
//! The stand-in's share server is this module's list of links: it answers a visit unless the
//! link expired (`drive_share_expire` lets every link's expiry pass) or the provider took it
//! down (`drive_tamper` `takedown_link`). Neither deletes anything; revoking a share (a key
//! rotation does it for every share) deletes its manifest, so the link opens nothing either.

use std::sync::{Mutex, MutexGuard, PoisonError};

use super::{need, str_arg, with_harness, Answer, Args};
use crate::DriveState;

/// The host the stand-in's links name: a domain of their own, never the app's.
pub(crate) const LINK_HOST: &str = "share.e2e.invalid";

/// One link made in this scenario.
#[derive(Clone, Debug)]
struct Link {
    /// The scenario's name for it (`name`, else the path shared).
    name: String,
    remote: String,
    /// The path shared, as the device named it.
    path: String,
    /// The share's id (hex) and its key as the link's fragment carries it.
    id: String,
    key: String,
    /// The server answers no visit any more.
    expired: bool,
    taken_down: bool,
    /// Whether it was made with an expiry (`drive_share_expire` lets only those pass).
    expires: bool,
}

impl Link {
    fn url(&self) -> String {
        format!("https://{LINK_HOST}/s/{}#k={}", self.id, self.key)
    }
}

static LINKS: Mutex<Vec<Link>> = Mutex::new(Vec::new());

// Lock order: the harness first, then the links.
fn links() -> MutexGuard<'static, Vec<Link>> {
    LINKS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Forgets every link (the scenario ended).
pub(crate) fn forget() {
    links().clear();
}

/// The link `which` names: a link's name, its URL, or the path it shares (the last one made).
fn find(which: &str) -> Result<Link, String> {
    links()
        .iter()
        .rev()
        .find(|l| l.name == which || l.url() == which || l.path == which)
        .cloned()
        .ok_or_else(|| format!("no share link '{which}' (drive_share_link makes one)"))
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `drive_share_expire`: every link of the remote that was made with an expiry has passed
/// it. The server answers their visits no more; the files and the manifests stay.
pub(crate) fn expire(args: &Args) -> Answer {
    let remote = with_harness(|h| Ok(h.remote_of(args)?.name.clone()))?;
    let mut expired = 0;
    for link in links().iter_mut().filter(|l| l.remote == remote && l.expires) {
        link.expired = true;
        expired += 1;
    }
    Ok(serde_json::json!({ "expired": expired }))
}

/// `drive_tamper` `takedown_link` (`path`, or `link`): the provider takes that one link
/// down, as after an abuse report. Nothing else changes: no file, no other link.
pub(crate) fn takedown(args: &Args) -> Answer {
    let remote = with_harness(|h| Ok(h.remote_of(args)?.name.clone()))?;
    let which = str_arg(args, "link").map_or_else(|| need(args, "path"), Ok)?;
    let mut all = links();
    let mut taken = 0;
    for link in all
        .iter_mut()
        .filter(|l| l.remote == remote && (l.name == which || l.path == which || l.url() == which))
    {
        link.taken_down = true;
        taken += 1;
    }
    if taken == 0 {
        return Err(format!("no share link of '{which}' on remote '{remote}'"));
    }
    Ok(serde_json::json!({ "taken_down": taken }))
}

/// What a visit of a link found: whether the server answered, why not, and each file of
/// the share with whether it read back as the share names it.
#[derive(Debug, Default)]
struct Visit {
    available: bool,
    why: String,
    files: serde_json::Map<String, serde_json::Value>,
    unreadable: usize,
}

impl Visit {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "available": self.available,
            "why": self.why,
            "files": self.files,
            "unreadable": self.unreadable,
        })
    }

    /// Every file of the share read back.
    fn readable(&self) -> bool {
        self.available && self.unreadable == 0
    }
}

#[cfg(feature = "encryption")]
mod shares {
    use std::sync::Arc;

    use azul_storage::{
        crypto::share::{share_key_from_text, share_key_text},
        encrypted::open_encrypted,
        sharing, Drive,
    };

    use super::{links, now_secs, Link, Visit};
    use crate::e2e::{c_keys, need, remote::FaultDrive, str_arg, with_harness, Answer, Args};

    /// `drive_share_link`: the device shares `path` (a file, or a folder and all in it) as a
    /// link, as AzDrive's Share > Copy link on an encrypted drive: its answer is the `link`,
    /// the `host` it names, the share's `id` and how many `files` it opens.
    pub(crate) fn share_link(args: &Args) -> Answer {
        let (device, remote, keyring, path, is_folder) = with_harness(|h| {
            let d = h.device(args)?;
            let remote = h
                .remotes
                .get(&d.remote)
                .cloned()
                .ok_or_else(|| format!("no remote '{}'", d.remote))?;
            let path = need(args, "path")?.trim_matches('/').to_string();
            let is_folder = d.root.join(&path).is_dir();
            Ok((d.name.clone(), remote, c_keys::keyring_of(h, &d.name)?, path, is_folder))
        })?;
        if !remote.encrypted {
            return Err(format!(
                "remote '{}' is plain: a share link is an encrypted drive's (its files' keys \
                 wrapped for the link)",
                remote.name
            ));
        }
        let bucket: Arc<dyn Drive> = Arc::new(FaultDrive {
            remote: remote.clone(),
            device: device.clone(),
        });
        // The device's own key opens the drive; the index is read anew (no copy is kept).
        let provider = crate::encryption::meta_provider(None);
        let drive = open_encrypted(bucket, keyring.as_ref(), &remote.drive_name, &provider)
            .map_err(|e| format!("{device} cannot open the drive: {e}"))?;
        let key = if is_folder {
            format!("{}{path}/", remote.prefix)
        } else {
            format!("{}{path}", remote.prefix)
        };
        let expires_s = args.get("expires_s").and_then(serde_json::Value::as_u64);
        let made = sharing::share(&drive, &[key.as_str()], None, expires_s.map(|s| now_secs() + s), None)
            .map_err(|e| format!("the share of {path}: {e}"))?;
        let link = Link {
            name: str_arg(args, "name").unwrap_or(&path).to_string(),
            remote: remote.name.clone(),
            path,
            id: made.id.to_hex(),
            key: share_key_text(&made.key).to_string(),
            expired: false,
            taken_down: false,
            expires: expires_s.is_some(),
        };
        let url = link.url();
        let id = link.id.clone();
        links().push(link);
        Ok(serde_json::json!({
            "link": url,
            "host": super::LINK_HOST,
            "id": id,
            "files": made.manifest.entries.len(),
        }))
    }

    /// A visit of `link` as someone without an account: through the stand-in's share server
    /// (its own connection to the bucket, which a fault on every device reaches too, one on a
    /// single device does not).
    pub(super) fn visit(link: &Link) -> Result<Visit, String> {
        let mut visit = Visit::default();
        if link.taken_down || link.expired {
            visit.why = String::from(if link.taken_down { "taken down" } else { "expired" });
            return Ok(visit);
        }
        let remote = with_harness(|h| {
            h.remotes
                .get(&link.remote)
                .cloned()
                .ok_or_else(|| format!("no remote '{}'", link.remote))
        })?;
        let bucket = FaultDrive {
            remote,
            device: String::from("(share server)"),
        };
        let key = share_key_from_text(&link.key).ok_or("the link's key does not read")?;
        let manifest = match sharing::open_share(&bucket, &key) {
            Ok(manifest) => manifest,
            Err(e) => {
                visit.why = format!("the share opens nothing: {e}");
                return Ok(visit);
            }
        };
        if manifest.expires.is_some_and(|at| at < now_secs()) {
            visit.why = String::from("expired");
            return Ok(visit);
        }
        visit.available = true;
        for entry in &manifest.entries {
            let read = sharing::read_shared_entry(&bucket, entry, &key);
            let file = match &read {
                Ok(plain) => serde_json::json!({ "size": plain.len(), "readable": true }),
                Err(e) => {
                    visit.unreadable += 1;
                    serde_json::json!({ "size": entry.size, "readable": false, "error": e.to_string() })
                }
            };
            visit.files.insert(entry.name.clone(), file);
        }
        Ok(visit)
    }
}

#[cfg(feature = "encryption")]
pub(crate) use shares::share_link;

#[cfg(not(feature = "encryption"))]
pub(crate) fn share_link(_: &Args) -> Answer {
    Err(String::from("this AzDrive was built without encryption"))
}

#[cfg(feature = "encryption")]
fn visit(link: &Link) -> Result<Visit, String> {
    shares::visit(link)
}

#[cfg(not(feature = "encryption"))]
fn visit(_: &Link) -> Result<Visit, String> {
    Err(String::from("this AzDrive was built without encryption"))
}

/// `drive_open_link`: a visit of the link `link` (its name, URL or path): `available`, `why`
/// not, `files` (`{name: {size, readable, error}}`) and how many are `unreadable`.
pub(crate) fn open_link(args: &Args) -> Answer {
    let link = find(need(args, "link")?)?;
    Ok(visit(&link)?.to_json())
}

/// Whether `args` asks for one of the link checks (`share_link`).
pub(crate) fn is_link_check(args: &Args) -> bool {
    str_arg(args, "check") == Some("share_link")
}

/// `drive_check` `share_link` (`path` or `link`, `readable`): whether a visit of the link
/// reads every file it shares (`readable: true`) or opens nothing at all (`false`).
pub(crate) fn check(_s: &mut DriveState, args: &Args) -> Answer {
    let which = str_arg(args, "link").map_or_else(|| need(args, "path"), Ok)?;
    let wanted = args
        .get("readable")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let found = visit(&find(which)?)?;
    let verdict = if wanted {
        if found.readable() {
            Ok(())
        } else if found.available {
            Err(format!("{} of its files do not read", found.unreadable))
        } else {
            Err(format!("the link opens nothing: {}", found.why))
        }
    } else if found.available {
        Err(String::from("the link still opens its files"))
    } else {
        Ok(())
    };
    Ok(match verdict {
        Ok(()) => serde_json::json!({ "ok": true, "detail": "" }),
        Err(detail) => serde_json::json!({
            "ok": false, "success": false, "detail": detail, "error": format!("share_link: {detail}"),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(name: &str, path: &str, expires: bool) -> Link {
        Link {
            name: name.to_string(),
            remote: String::from("r"),
            path: path.to_string(),
            id: format!("{:0>32}", name.len()),
            key: String::from("00"),
            expired: false,
            taken_down: false,
            expires,
        }
    }

    /// A link is found by its name, its URL or the path it shares; the URL names the share
    /// server's own host, and the key rides only in the fragment.
    #[test]
    fn a_link_is_found_by_name_url_or_path_and_its_key_is_only_in_the_fragment() {
        forget();
        links().push(link("l", "big", false));
        let url = find("l").expect("by name").url();
        assert!(url.starts_with(&format!("https://{LINK_HOST}/s/")), "{url}");
        let (before, fragment) = url.split_once('#').expect("a fragment");
        assert!(!before.contains("k="), "the key never reaches the server: {url}");
        assert_eq!(fragment, "k=00");
        assert_eq!(find(&url).expect("by URL").name, "l");
        assert_eq!(find("big").expect("by path").name, "l");
        assert!(find("other").is_err());
        forget();
    }

    /// An expired or taken-down link is not visited at all: the server answers nothing,
    /// whatever the bucket holds.
    #[cfg(feature = "encryption")]
    #[test]
    fn an_expired_or_taken_down_link_opens_nothing() {
        let mut gone = link("a", "a.txt", true);
        gone.expired = true;
        let visit = visit(&gone).expect("a visit");
        assert!(!visit.available && !visit.readable());
        assert_eq!(visit.why, "expired");
        let mut down = link("b", "b.txt", false);
        down.taken_down = true;
        assert_eq!(super::visit(&down).expect("a visit").why, "taken down");
    }
}
