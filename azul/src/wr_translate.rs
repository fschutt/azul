use webrender::api::{
    LayoutRect as WrLayoutRect,
    HitTestItem as WrHitTestItem,
    FontKey as WrFontKey,
    FontInstanceKey as WrFontInstanceKey,
    ImageKey as WrImageKey,
    IdNamespace as WrIdNamespace,
    PipelineId as WrPipelineId,
};
use azul_core::{
    callbacks::{HidpiAdjustedBounds, HitTestItem, PipelineId},
    window::{LogicalPosition, LogicalSize},
    app_resources::{FontKey, Au, FontInstanceKey, ImageKey, IdNamespace},
};
use app_units::Au as WrAu;

#[inline(always)]
pub(crate) fn translate_wr_hittest_item(input: WrHitTestItem) -> HitTestItem {
    HitTestItem {
        pipeline: PipelineId(input.pipeline.0, input.pipeline.1),
        tag: input.tag,
        point_in_viewport: LogicalPosition::new(input.point_in_viewport.x, input.point_in_viewport.y),
        point_relative_to_item: LogicalPosition::new(input.point_relative_to_item.x, input.point_relative_to_item.y),
    }
}

#[inline(always)]
pub(crate) fn hidpi_rect_from_bounds(bounds: WrLayoutRect, hidpi_factor: f32, winit_hidpi_factor: f32) -> HidpiAdjustedBounds {
    let logical_size = LogicalSize::new(bounds.size.width, bounds.size.height);
    HidpiAdjustedBounds {
        logical_size,
        hidpi_factor,
        winit_hidpi_factor,
    }
}

#[inline(always)]
fn translate_id_namespace(ns: IdNamespace) -> WrIdNamespace {
    WrIdNamespace(ns.0)
}

#[inline(always)]
pub(crate) fn translate_font_key(font_key: FontKey) -> WrFontKey {
    WrFontKey(translate_id_namespace(font_key.namespace), font_key.key)
}

#[inline(always)]
pub(crate) fn translate_font_instance_key(font_instance_key: FontInstanceKey) -> WrFontInstanceKey {
    WrFontInstanceKey(translate_id_namespace(font_instance_key.namespace), font_instance_key.key)
}

#[inline(always)]
pub(crate) fn translate_image_key(image_key: ImageKey) -> WrImageKey {
    WrImageKey(translate_id_namespace(image_key.namespace), image_key.key)
}

#[inline(always)]
pub(crate) fn translate_pipeline_id(pipeline_id: PipelineId) -> WrPipelineId {
    WrPipelineId(pipeline_id.0, pipeline_id.1)
}

#[inline(always)]
pub(crate) fn translate_au(au: Au) -> WrAu {
    WrAu(au.0)
}

