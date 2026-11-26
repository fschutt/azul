//! The layout check: what the screens may not do, read from the boxes the layout engine made. Every tappable thing carries
//! the class `az-target`, every line of text `az-text`; the check finds a target that is smaller than a finger, two that
//! overlap, one in the system's bars, a line of text that runs out of its box (clipped or over a neighbour), a pad too
//! small to use, and a quick button that is missing. It is pure: the simulator reads the boxes from a real window (a
//! screenshot run) and gives them here; the tests give it boxes made by hand.

use crate::env::Env;
use crate::layout::{zones, Zone, MIN_TARGET};

/// A node of the laid-out DOM.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeBox {
    /// Its place in the list the nodes came in.
    pub index: usize,
    /// The place of its parent in that list.
    pub parent: Option<usize>,
    pub tag: String,
    /// Its `id` attribute (empty: none).
    pub id: String,
    pub classes: Vec<String>,
    /// The words it holds, if it is a text.
    pub text: String,
    pub rect: Zone,
}

impl NodeBox {
    /// Whether it has the class.
    #[must_use]
    pub fn has(&self, class: &str) -> bool {
        self.classes.iter().any(|c| c == class)
    }
}

/// What a screen did that it may not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: &'static str,
    pub node: usize,
    pub detail: String,
}

/// How far a box may stick out before it counts, in pixels (the engine rounds).
pub const TOLERANCE: f32 = 1.0;

/// The faults of these boxes for a phone like `env`.
#[must_use]
pub fn check(nodes: &[NodeBox], env: &Env) -> Vec<Violation> {
    let safe = zones(env).safe;
    let at: std::collections::HashMap<usize, usize> = nodes.iter().enumerate().map(|(position, n)| (n.index, position)).collect();
    let parent_of = |position: usize| nodes[position].parent.and_then(|p| at.get(&p).copied());
    let has_ancestor = |position: usize, class: &str| {
        let mut current = parent_of(position);
        let mut steps = 0;
        while let Some(p) = current {
            if nodes[p].has(class) {
                return true;
            }
            steps += 1;
            if steps > 256 {
                return false;
            }
            current = parent_of(p);
        }
        false
    };
    let inside = |r: &Zone| r.x >= safe.x - TOLERANCE && r.y >= safe.y - TOLERANCE && r.right() <= safe.right() + TOLERANCE && r.bottom() <= safe.bottom() + TOLERANCE;
    let mut out = Vec::new();

    // the keys
    let targets: Vec<usize> = (0..nodes.len()).filter(|i| nodes[*i].has("az-target")).collect();
    for &i in &targets {
        let n = &nodes[i];
        if n.rect.w < MIN_TARGET - 0.5 || n.rect.h < MIN_TARGET - 0.5 {
            out.push(Violation { rule: "target-size", node: n.index, detail: format!("{:.0} x {:.0} px, a finger needs {MIN_TARGET:.0} x {MIN_TARGET:.0}", n.rect.w, n.rect.h) });
        }
        if !n.has("az-row") && !has_ancestor(i, "az-scroll") && !inside(&n.rect) {
            out.push(Violation { rule: "target-outside-safe-area", node: n.index, detail: format!("at ({:.0}, {:.0}) {:.0} x {:.0} px, outside the safe area", n.rect.x, n.rect.y, n.rect.w, n.rect.h) });
        }
    }
    // what of a box can be seen: a row of a list is cut off by the list's own box
    let visible = |position: usize| -> Zone {
        let mut rect = nodes[position].rect;
        let mut current = parent_of(position);
        let mut steps = 0;
        while let Some(p) = current {
            if nodes[p].has("az-scroll") {
                let clip = &nodes[p].rect;
                let (x, y) = (rect.x.max(clip.x), rect.y.max(clip.y));
                rect = Zone { x, y, w: (rect.right().min(clip.right()) - x).max(0.0), h: (rect.bottom().min(clip.bottom()) - y).max(0.0) };
            }
            steps += 1;
            if steps > 256 {
                break;
            }
            current = parent_of(p);
        }
        rect
    };
    // the keys in the order of their top edge, so each is only compared with the ones that start above its bottom
    let mut seen: Vec<(usize, Zone)> = targets.iter().map(|&i| (i, visible(i))).filter(|(_, r)| r.w > 0.0 && r.h > 0.0).collect();
    seen.sort_by(|a, b| a.1.y.total_cmp(&b.1.y));
    for k in 0..seen.len() {
        let (i, a) = (seen[k].0, seen[k].1);
        for &(j, b) in &seen[k + 1..] {
            if b.y >= a.bottom() - TOLERANCE {
                break;
            }
            let overlap_w = a.right().min(b.right()) - a.x.max(b.x);
            let overlap_h = a.bottom().min(b.bottom()) - a.y.max(b.y);
            if overlap_w > TOLERANCE && overlap_h > TOLERANCE {
                let (first, second) = if nodes[i].index < nodes[j].index { (i, j) } else { (j, i) };
                out.push(Violation {
                    rule: "target-overlap",
                    node: nodes[second].index,
                    detail: format!("overlaps node {} by {overlap_w:.0} x {overlap_h:.0} px", nodes[first].index),
                });
            }
        }
    }

    // the words
    for i in 0..nodes.len() {
        let n = &nodes[i];
        if !n.has("az-text") || n.rect.w <= 0.0 || n.rect.h <= 0.0 {
            continue;
        }
        if !n.has("az-clip") {
            if let Some(p) = parent_of(i).filter(|p| !nodes[*p].has("az-scroll")) {
                let parent = &nodes[p].rect;
                let r = &n.rect;
                if r.right() > parent.right() + TOLERANCE || r.bottom() > parent.bottom() + TOLERANCE || r.x < parent.x - TOLERANCE || r.y < parent.y - TOLERANCE {
                    out.push(Violation {
                        rule: "text-overflow",
                        node: n.index,
                        detail: format!(
                            "\"{}\" is {:.0} x {:.0} px at ({:.0}, {:.0}) and runs out of its box, {:.0} x {:.0} px at ({:.0}, {:.0})",
                            n.text, r.w, r.h, r.x, r.y, parent.w, parent.h, parent.x, parent.y
                        ),
                    });
                }
            }
        }
        if !has_ancestor(i, "az-row") && !has_ancestor(i, "az-scroll") && !n.has("az-row") && !inside(&n.rect) {
            out.push(Violation { rule: "text-outside-safe-area", node: n.index, detail: format!("\"{}\" at ({:.0}, {:.0}) is outside the safe area", n.text, n.rect.x, n.rect.y) });
        }
    }

    // the pad and the four quick buttons
    let pad = nodes.iter().find(|n| n.id == "pad");
    if let Some(pad) = pad {
        if pad.rect.w < 200.0 || pad.rect.h < 180.0 {
            out.push(Violation { rule: "pad-size", node: pad.index, detail: format!("the pad is {:.0} x {:.0} px, it needs 200 x 180", pad.rect.w, pad.rect.h) });
        }
        let quick: Vec<&NodeBox> = nodes.iter().filter(|n| n.has("az-quick")).collect();
        if (!env.landscape() && quick.len() != 4) || (env.landscape() && !quick.is_empty() && quick.len() != 4) {
            out.push(Violation { rule: "quick-buttons", node: pad.index, detail: format!("{} of the 4 quick buttons are there", quick.len()) });
        }
        for button in quick {
            if button.rect.w < 48.0 || button.rect.h < 48.0 {
                out.push(Violation { rule: "quick-buttons", node: button.index, detail: format!("a quick button is {:.0} x {:.0} px, it needs 48 x 48", button.rect.w, button.rect.h) });
            }
        }
    }
    out
}

/// The violations as lines of words.
#[must_use]
pub fn describe(violations: &[Violation], nodes: &[NodeBox]) -> Vec<String> {
    violations
        .iter()
        .map(|v| {
            let place = nodes.iter().find(|n| n.index == v.node).map_or(String::new(), |n| {
                let id = if n.id.is_empty() { String::new() } else { format!("#{}", n.id) };
                format!(" <{}{id}>", n.tag)
            });
            format!("{}: node {}{place}: {}", v.rule, v.node, v.detail)
        })
        .collect()
}

#[cfg(test)]
#[path = "check_test.rs"]
mod check_test;
