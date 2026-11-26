//! The screenshot run: the simulator puts itself in each scene in turn, lets it settle, takes a picture with the CPU
//! renderer (the phone alone, at 2x), reads the boxes the layout engine made and runs the layout check on them. It
//! writes `<prefix>-<scene>.png` for each scene and says on stderr what the check found; the process ends with a failure
//! code if it found anything. Nothing here touches the screen: it runs headless as well as in a window.

use std::path::PathBuf;

use azul::{
    dom::{DomId, DomNodeId},
    prelude::*,
    str::String as AzString,
};

use super::check::{check, describe, NodeBox};
use super::{scenes, Extras};
use crate::layout::Zone;
use crate::ui::Root;

/// What a step of the run did.
pub enum Step {
    /// Still going; whether the screens must be built again.
    Working(bool),
    /// Done, with the process's exit code.
    Finished(i32),
}

/// The scenes to shoot and where the run stands.
pub struct Player {
    dir: PathBuf,
    prefix: String,
    names: Vec<String>,
    index: usize,
    booted: Option<u64>,
    reset_at: Option<u64>,
    since: Option<u64>,
    faults: usize,
    /// Take one picture of the whole window as it is, to this file (the simulator with its panel), then finish.
    window_file: Option<PathBuf>,
}

/// How long the window gets to come up before the first scene.
const BOOT_MS: u64 = 900;

impl Player {
    /// A run over `names`, pictures into `dir` as `<prefix>-<scene>.png`.
    #[must_use]
    pub fn new(dir: PathBuf, prefix: String, names: Vec<String>) -> Player {
        Player { dir, prefix, names, index: 0, booted: None, reset_at: None, since: None, faults: 0, window_file: None }
    }

    /// One picture of the window as it is.
    #[must_use]
    pub fn window(file: PathBuf) -> Player {
        Player { dir: PathBuf::new(), prefix: String::new(), names: Vec::new(), index: 0, booted: None, reset_at: None, since: None, faults: 0, window_file: Some(file) }
    }

    /// Does what is due at `now`.
    pub fn step(&mut self, root: &mut Root, extras: &Extras, now: u64, info: &mut CallbackInfo) -> Step {
        let booted = *self.booted.get_or_insert(now);
        if now.saturating_sub(booted) < BOOT_MS {
            return Step::Working(false);
        }
        if let Some(file) = self.window_file.clone() {
            if now.saturating_sub(booted) < BOOT_MS + 800 {
                return Step::Working(false);
            }
            return Step::Finished(save(info, &file));
        }
        let Some(name) = self.names.get(self.index).cloned() else {
            return Step::Finished(self.finish());
        };
        let since = match self.since {
            Some(since) => since,
            None => {
                let reset_at = match self.reset_at {
                    Some(at) => at,
                    None => {
                        scenes::reset(root, extras);
                        self.reset_at = Some(now);
                        return Step::Working(true);
                    }
                };
                if now.saturating_sub(reset_at) < scenes::RESET_MS {
                    return Step::Working(false);
                }
                self.reset_at = None;
                if !scenes::setup(&name, root, extras, now) {
                    eprintln!("[shots] no scene named {name}");
                    self.faults += 1;
                    self.index += 1;
                    return Step::Working(false);
                }
                self.since = Some(now);
                return Step::Working(true);
            }
        };
        if now.saturating_sub(since) < scenes::SETTLE_MS {
            return Step::Working(false);
        }
        let file = self.dir.join(format!("{}-{name}.png", self.prefix));
        if save(info, &file) != 0 {
            self.faults += 1;
        }
        let nodes = collect(info);
        let found = check(&nodes, &extras.env());
        if found.is_empty() {
            eprintln!("[shots] CHECK {name}: ok ({} boxes)", nodes.len());
        } else {
            self.faults += found.len();
            eprintln!("[shots] CHECK {name}: {} faults", found.len());
            for line in describe(&found, &nodes) {
                eprintln!("[shots]   {line}");
            }
        }
        self.index += 1;
        self.since = None;
        Step::Working(false)
    }

    fn finish(&self) -> i32 {
        eprintln!("[shots] {} scenes, {} faults", self.names.len(), self.faults);
        i32::from(self.faults > 0)
    }
}

/// Takes the picture of the window's DOM and writes it. 0 when it is written.
fn save(info: &CallbackInfo, file: &PathBuf) -> i32 {
    match info.take_screenshot(DomId { inner: 0 }).into_result() {
        Ok(png) => match std::fs::write(file, png.as_slice()) {
            Ok(()) => {
                eprintln!("[shots] SHOT {}", file.display());
                0
            }
            Err(e) => {
                eprintln!("[shots] cannot write {}: {e}", file.display());
                2
            }
        },
        Err(e) => {
            eprintln!("[shots] the screenshot failed: {}", e.as_str());
            2
        }
    }
}

/// Every box of the phone's DOM, in order, with the index of its parent.
fn collect(info: &CallbackInfo) -> Vec<NodeBox> {
    let dom = DomId { inner: 0 };
    let start = info.get_node_id_by_id_attribute(dom, AzString::from(crate::ui::chrome::PHONE_ID));
    let mut out = Vec::new();
    if start.into_raw() != 0 {
        visit(info, DomNodeId { dom, node: start }, None, &mut out, 0);
    }
    out
}

fn visit(info: &CallbackInfo, node: DomNodeId, parent: Option<usize>, out: &mut Vec<NodeBox>, depth: usize) {
    if depth > 200 || out.len() > 30_000 {
        return;
    }
    let index = out.len();
    let rect = info
        .get_node_rect(node)
        .into_option()
        .map(|r| Zone { x: r.origin.x, y: r.origin.y, w: r.size.width, h: r.size.height })
        .unwrap_or_default();
    out.push(NodeBox {
        index,
        parent,
        tag: info.get_node_tag_name(node).into_option().map(|s| s.as_str().to_string()).unwrap_or_default(),
        id: info.get_node_id(node).into_option().map(|s| s.as_str().to_string()).unwrap_or_default(),
        classes: info.get_node_classes(node).as_ref().iter().map(|c| c.as_str().to_string()).collect(),
        text: info.get_node_text_content(node).into_option().map(|s| s.as_str().to_string()).unwrap_or_default(),
        rect,
    });
    let mut child = info.get_first_child(node).into_option();
    while let Some(c) = child {
        visit(info, c, Some(index), out, depth + 1);
        child = info.get_next_sibling(c).into_option();
    }
}
