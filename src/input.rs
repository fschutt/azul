use webrender::api::{HitTestResult, PipelineId, DocumentId, HitTestFlags, RenderApi, WorldPoint};

pub fn hit_test_ui(api: &RenderApi, document_id: DocumentId, pipeline_id: Option<PipelineId>, point: WorldPoint) -> HitTestResult {
	api.hit_test(document_id, pipeline_id, point, HitTestFlags::FIND_ALL)
}

