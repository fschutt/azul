#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]

extern crate azul_core;
extern crate azul_css;
#[cfg(feature = "text_layout")]
pub extern crate azul_text_layout as text_layout;

#[cfg(test)]
mod layout_test;
mod layout_solver;

pub use azul_core::{
    callbacks::PipelineId,
    id_tree::{NodeHierarchy, NodeDataContainer},
    app_resources::AppResources,
    ui_solver::LayoutResult,
    dom::NodeData,
    styled_dom::StyledDom,
};
pub use azul_css::{LayoutSize, LayoutPoint, LayoutRect};

pub use layout_solver::do_the_layout;