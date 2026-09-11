//! Canonical `oo-mindmap` browser binding.
//!
//! This crate is deliberately transport-only: it parses the versioned snapshot, delegates every
//! command/layout/history operation to `MindmapEngine`, and serializes typed results. It contains
//! no second graph model and no renderer-owned domain state.

use oo_mindmap::{
    update_projection, MindmapCommandBatch, MindmapEngine, MindmapLayoutOptions, MindmapProjection,
    MindmapTheme,
};
use oo_protocol::{Invalidation, SnapshotEnvelope, CURRENT_PROTOCOL_VERSION};
use oo_schema::{ArtifactEnvelope, ArtifactKind, ArtifactPayload};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct MindmapSession {
    artifact_id: String,
    schema_version: u16,
    protocol_version: u16,
    engine: MindmapEngine,
}

#[wasm_bindgen(js_name = loadSnapshot)]
pub fn load_snapshot(snapshot_json: &str) -> Result<MindmapSession, JsValue> {
    let snapshot: SnapshotEnvelope = serde_json::from_str(snapshot_json)
        .map_err(|error| js_error("Mindmap snapshot JSON 无效", error))?;
    snapshot
        .validate()
        .map_err(|error| js_error("Mindmap snapshot 校验失败", error))?;
    if snapshot.protocol_version > CURRENT_PROTOCOL_VERSION {
        return Err(JsValue::from_str(
            "快照 protocol 版本高于当前 Mindmap engine",
        ));
    }
    let ArtifactEnvelope {
        schema_version,
        artifact_id,
        revision,
        kind,
        payload,
        ..
    } = snapshot.artifact;
    if kind != ArtifactKind::Mindmap {
        return Err(JsValue::from_str("MindmapSession 只接受 mindmap Artifact"));
    }
    let ArtifactPayload::Mindmap(model) = payload else {
        return Err(JsValue::from_str("Artifact payload 不是 MindmapModel"));
    };
    let engine = MindmapEngine::new(model, revision)
        .map_err(|error| js_error("创建 MindmapEngine 失败", error))?;
    Ok(MindmapSession {
        artifact_id,
        schema_version,
        protocol_version: snapshot.protocol_version,
        engine,
    })
}

#[wasm_bindgen(js_name = updateProjection)]
pub fn update_projection_json(
    previous_json: &str,
    snapshot_json: &str,
    invalidation_json: &str,
    theme: &str,
) -> Result<String, JsValue> {
    let session = load_snapshot(snapshot_json)?;
    let previous: MindmapProjection = serde_json::from_str(previous_json)
        .map_err(|error| js_error("previous Mindmap projection 无效", error))?;
    let invalidation: Invalidation = serde_json::from_str(invalidation_json)
        .map_err(|error| js_error("Mindmap invalidation 无效", error))?;
    let update = update_projection(
        &previous,
        session.engine.model(),
        &invalidation,
        MindmapLayoutOptions::default(),
        parse_theme(theme)?,
    )
    .map_err(|error| js_error("Mindmap 增量 projection 失败", error))?;
    serde_json::to_string(&update)
        .map_err(|error| js_error("Mindmap 增量 projection 序列化失败", error))
}

#[wasm_bindgen]
impl MindmapSession {
    pub fn dispatch(&mut self, batch_json: &str) -> Result<String, JsValue> {
        let batch: MindmapCommandBatch = serde_json::from_str(batch_json)
            .map_err(|error| js_error("MindmapCommandBatch JSON 无效", error))?;
        let change = self
            .engine
            .execute(batch)
            .map_err(|error| js_error("Mindmap command 执行失败", error))?;
        serde_json::to_string(&change)
            .map_err(|error| js_error("Mindmap ChangeSet 序列化失败", error))
    }

    pub fn undo(&mut self) -> Result<String, JsValue> {
        let change = self
            .engine
            .undo(self.engine.revision())
            .map_err(|error| js_error("Mindmap 撤销失败", error))?;
        serde_json::to_string(&change)
            .map_err(|error| js_error("Mindmap ChangeSet 序列化失败", error))
    }

    pub fn redo(&mut self) -> Result<String, JsValue> {
        let change = self
            .engine
            .redo(self.engine.revision())
            .map_err(|error| js_error("Mindmap 重做失败", error))?;
        serde_json::to_string(&change)
            .map_err(|error| js_error("Mindmap ChangeSet 序列化失败", error))
    }

    #[wasm_bindgen(js_name = canUndo)]
    pub fn can_undo(&self) -> bool {
        self.engine.can_undo()
    }

    #[wasm_bindgen(js_name = canRedo)]
    pub fn can_redo(&self) -> bool {
        self.engine.can_redo()
    }

    pub fn projection(&self, theme: &str) -> Result<String, JsValue> {
        let projection = MindmapProjection::build(
            self.engine.model(),
            MindmapLayoutOptions::default(),
            parse_theme(theme)?,
        )
        .map_err(|error| js_error("Mindmap projection 失败", error))?;
        serde_json::to_string(&projection)
            .map_err(|error| js_error("Mindmap projection 序列化失败", error))
    }

    #[wasm_bindgen(js_name = projectionWithMeasurements)]
    pub fn projection_with_measurements(
        &self,
        theme: &str,
        measurements: &str,
    ) -> Result<String, JsValue> {
        let measurements: std::collections::HashMap<String, oo_mindmap::MindmapNodeMeasurement> =
            serde_json::from_str(measurements)
                .map_err(|error| js_error("Mindmap measurements 无效", error))?;
        let projection = MindmapProjection::build_with_measurements(
            self.engine.model(),
            MindmapLayoutOptions::default(),
            parse_theme(theme)?,
            &measurements,
        )
        .map_err(|error| js_error("Mindmap projection 失败", error))?;
        serde_json::to_string(&projection)
            .map_err(|error| js_error("Mindmap projection 序列化失败", error))
    }

    #[wasm_bindgen(js_name = readSnapshot)]
    pub fn read_snapshot(&self) -> Result<String, JsValue> {
        let snapshot = SnapshotEnvelope {
            protocol_version: self.protocol_version,
            artifact: ArtifactEnvelope {
                format: "open-office-artifact".into(),
                schema_version: self.schema_version,
                artifact_id: self.artifact_id.clone(),
                revision: self.engine.revision(),
                kind: ArtifactKind::Mindmap,
                payload: ArtifactPayload::Mindmap(self.engine.model().clone()),
            },
        };
        serde_json::to_string(&snapshot)
            .map_err(|error| js_error("Mindmap snapshot 序列化失败", error))
    }

    pub fn revision(&self) -> u64 {
        self.engine.revision()
    }
}

fn parse_theme(theme: &str) -> Result<MindmapTheme, JsValue> {
    match theme {
        "light" => Ok(MindmapTheme::Light),
        "dark" => Ok(MindmapTheme::Dark),
        "highContrast" | "high-contrast" => Ok(MindmapTheme::HighContrast),
        _ => Err(JsValue::from_str("Mindmap theme 无效")),
    }
}

fn js_error(context: &str, error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&format!("{context}：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oo_schema::{MindmapModel, CURRENT_SCHEMA_VERSION};

    fn snapshot() -> String {
        let mut artifact = ArtifactEnvelope::new(
            "mindmap-wasm",
            ArtifactPayload::Mindmap(MindmapModel::default()),
        );
        artifact.schema_version = CURRENT_SCHEMA_VERSION;
        artifact.revision = 3;
        serde_json::to_string(&SnapshotEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            artifact,
        })
        .unwrap()
    }

    #[test]
    fn binding_delegates_snapshot_and_projection_to_canonical_engine() {
        let session = load_snapshot(&snapshot()).unwrap();
        assert_eq!(session.revision(), 3);
        assert!(session.projection("light").unwrap().contains("\"layout\""));
        let roundtrip: SnapshotEnvelope =
            serde_json::from_str(&session.read_snapshot().unwrap()).unwrap();
        assert_eq!(roundtrip.artifact.revision, 3);
    }

    #[test]
    fn binding_exposes_canonical_incremental_projection_without_a_second_model() {
        let session = load_snapshot(&snapshot()).unwrap();
        let previous = session.projection("light").unwrap();
        let update = update_projection_json(
            &previous,
            &snapshot(),
            r#"{"changedEntities":[{"entityType":"mindmap.edge","entityId":"edge"}],"changedContainers":[],"structureChanged":false}"#,
            "light",
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&update).unwrap();
        assert_eq!(value["recomputedLayoutNodes"], 0);
        assert_eq!(
            value["projection"]["layout"]["nodes"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
}
