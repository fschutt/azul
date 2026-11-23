//! The switches of the widgets that the environment sets (group L5 of the env ledger), and the
//! one place of the crate that names their variables.
//!
//! Nothing here reads the process: every constructor takes a `&dyn EnvSource`
//! (`azul_css::env`), and `Default` is the behaviour of an EMPTY environment. The value is a small
//! `Copy` struct: a widget that has no window to ask (a render callback, a worker thread) captures
//! the part it needs from `LayoutWindow::settings` when it is mounted.

use azul_css::env::{EnvSource, NoEnv};

/// What the environment asks of the widgets: which alternative builds and traces they run.
#[allow(clippy::struct_excessive_bools)] // one independent switch per variable
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WidgetSettings {
    /// `AZ_COLOR_PICKER_CHECKER=cells`: a checkerboard is built as a div per cell, as every
    /// checkerboard was built before; the default is one node whose background draws the cells.
    pub color_picker_cells: bool,
    /// `AZ_MAP_DEBUG` is set: the map widget's debug log.
    pub map_debug: bool,
    /// `AZ_MAP_STATS` is set, not blank and not `0`: the map prints its counters on stdout.
    pub map_stats: bool,
    /// `AZ_NODEGRAPH_PAN=nodes`: a pan of the node graph is carried by every node box and wire,
    /// as before; the default carries it on the two containers.
    pub node_graph_pan_on_nodes: bool,
    /// `AZ_VIDEO_FRAMELOG` is set: log the video widget's renders and decode steps.
    pub video_framelog: bool,
}

impl WidgetSettings {
    /// What the environment `env` asks for; an empty environment asks for the shipping builds
    /// and no trace.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            color_picker_cells: env
                .var("AZ_COLOR_PICKER_CHECKER")
                .is_some_and(|v| v == "cells"),
            map_debug: env.is_set("AZ_MAP_DEBUG"),
            map_stats: env
                .var("AZ_MAP_STATS")
                .is_some_and(|v| !v.trim().is_empty() && v.trim() != "0"),
            node_graph_pan_on_nodes: env
                .var("AZ_NODEGRAPH_PAN")
                .is_some_and(|v| v == "nodes"),
            video_framelog: env.var("AZ_VIDEO_FRAMELOG").is_some(),
        }
    }
}

impl Default for WidgetSettings {
    /// The behaviour of an empty environment.
    fn default() -> Self {
        Self::from_lookup(&NoEnv)
    }
}

#[cfg(test)]
#[path = "widgets_test.rs"]
mod tests;
