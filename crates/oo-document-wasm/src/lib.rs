//! `oo-document` 的浏览器薄绑定。
//!
//! 这个 crate 不实现任何 Document 业务逻辑，也不维护另一份文档模型。它只负责把
//! canonical `DocumentEngine` 的 JSON 边界暴露给 JavaScript：快照加载时接收完整的
//! `SnapshotEnvelope`，高频编辑时接收 `DocumentCommandBatch` 并返回 `ChangeSet`，只有
//! 明确调用 `readSnapshot` 才重新序列化完整快照。

use oo_document::{ChangeSet, DocumentCommandBatch, DocumentEngine};
use oo_protocol::{SnapshotEnvelope, CURRENT_PROTOCOL_VERSION};
use oo_schema::{ArtifactEnvelope, ArtifactKind, ArtifactPayload};
use wasm_bindgen::prelude::*;

/// 浏览器里的 Document engine 会话。
///
/// `DocumentSession` 只持有一个 Rust engine；React、DOM 和 renderer 不应从这里复制或
/// 直接修改模型。高频路径的返回值是 `ChangeSet`，而不是完整 snapshot。
#[wasm_bindgen]
pub struct DocumentSession {
    artifact_id: String,
    schema_version: u16,
    protocol_version: u16,
    engine: DocumentEngine,
    last_change: Option<ChangeSet>,
}

/// 从 canonical `SnapshotEnvelope` 创建浏览器会话。
///
/// 该函数在生成的 JS 模块中导出为 `loadSnapshot`。输入必须是服务端 Artifact 快照，
/// 不接受旧 Paragraph、Canvas 或自定义兼容格式。
#[wasm_bindgen(js_name = loadSnapshot)]
pub fn load_snapshot(snapshot_json: &str) -> Result<DocumentSession, JsValue> {
    let snapshot: SnapshotEnvelope =
        serde_json::from_str(snapshot_json).map_err(|error| js_error("快照 JSON 无效", error))?;
    snapshot
        .validate()
        .map_err(|error| js_error("快照校验失败", error))?;
    let SnapshotEnvelope {
        protocol_version,
        artifact,
    } = snapshot;
    if protocol_version > CURRENT_PROTOCOL_VERSION {
        return Err(JsValue::from_str("快照 protocol 版本高于当前 engine"));
    }
    let ArtifactEnvelope {
        format: _,
        schema_version,
        artifact_id,
        revision,
        kind,
        payload,
    } = artifact;
    if kind != ArtifactKind::Document {
        return Err(JsValue::from_str(
            "DocumentSession 只接受 document Artifact",
        ));
    }
    let ArtifactPayload::Document(model) = payload else {
        return Err(JsValue::from_str("Artifact payload 不是 DocumentModel"));
    };
    let engine = DocumentEngine::new(model, revision)
        .map_err(|error| js_error("创建 DocumentEngine 失败", error))?;
    Ok(DocumentSession {
        artifact_id,
        schema_version,
        protocol_version,
        engine,
        last_change: None,
    })
}

#[wasm_bindgen]
impl DocumentSession {
    /// 应用一批 canonical `DocumentCommandBatch`，返回增量 `ChangeSet` JSON。
    ///
    /// 这个方法不会返回完整文档。调用方按 `changedBlocks` 读取受影响 block，完整快照
    /// 只应在保存、导出或显式调试时通过 `readSnapshot` 获取。
    pub fn dispatch(&mut self, transaction_json: &str) -> Result<String, JsValue> {
        let transaction: DocumentCommandBatch = serde_json::from_str(transaction_json)
            .map_err(|error| js_error("DocumentCommandBatch JSON 无效", error))?;
        let change = self
            .engine
            .execute(transaction)
            .map_err(|error| js_error("DocumentCommandBatch 执行失败", error))?;
        let json = serde_json::to_string(&change)
            .map_err(|error| js_error("ChangeSet 序列化失败", error))?;
        self.last_change = Some(change);
        Ok(json)
    }

    /// 撤销最近一次成功事务。撤销历史由 canonical Rust engine 持有，前端不再复制逆操作。
    pub fn undo(&mut self) -> Result<String, JsValue> {
        let change = self
            .engine
            .undo()
            .map_err(|error| js_error("DocumentEngine 撤销失败", error))?;
        let json = serde_json::to_string(&change)
            .map_err(|error| js_error("ChangeSet 序列化失败", error))?;
        self.last_change = Some(change);
        Ok(json)
    }

    /// 重做最近一次撤销事务。重做也返回增量 ChangeSet，不产生完整 snapshot。
    pub fn redo(&mut self) -> Result<String, JsValue> {
        let change = self
            .engine
            .redo()
            .map_err(|error| js_error("DocumentEngine 重做失败", error))?;
        let json = serde_json::to_string(&change)
            .map_err(|error| js_error("ChangeSet 序列化失败", error))?;
        self.last_change = Some(change);
        Ok(json)
    }

    #[wasm_bindgen(js_name = canUndo)]
    pub fn can_undo(&self) -> bool {
        self.engine.journal().can_undo()
    }

    #[wasm_bindgen(js_name = canRedo)]
    pub fn can_redo(&self) -> bool {
        self.engine.journal().can_redo()
    }

    /// 读取一个已提交 block 的 canonical JSON。
    #[wasm_bindgen(js_name = readBlock)]
    pub fn read_block(&self, block_id: &str) -> Result<String, JsValue> {
        let block = self
            .engine
            .read_block(block_id)
            .map_err(|error| js_error("读取 block 失败", error))?;
        serde_json::to_string(block).map_err(|error| js_error("Block 序列化失败", error))
    }

    /// Reads a ChangeSet's invalidated blocks in one JSON crossing.
    ///
    /// The input is a JSON string because wasm-bindgen's generated boundary is string based. The
    /// returned array preserves the requested BlockId order and contains no document snapshot.
    #[wasm_bindgen(js_name = readBlocks)]
    pub fn read_blocks(&self, block_ids_json: &str) -> Result<String, JsValue> {
        let block_ids: Vec<String> = serde_json::from_str(block_ids_json)
            .map_err(|error| js_error("blockId 列表 JSON 无效", error))?;
        let blocks = self
            .engine
            .read_blocks(&block_ids)
            .map_err(|error| js_error("批量读取 block 失败", error))?;
        serde_json::to_string(&blocks).map_err(|error| js_error("Block 列表序列化失败", error))
    }

    /// 读取最近一次成功事务的增量 ChangeSet；没有事务时返回 `null`。
    #[wasm_bindgen(js_name = readChangeSet)]
    pub fn read_change_set(&self) -> Result<Option<String>, JsValue> {
        self.last_change
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| js_error("ChangeSet 序列化失败", error))
    }

    /// 显式读取完整 canonical snapshot。此调用会产生一次完整模型序列化，不属于输入
    /// 热路径。
    #[wasm_bindgen(js_name = readSnapshot)]
    pub fn read_snapshot(&self) -> Result<String, JsValue> {
        let snapshot = SnapshotEnvelope {
            protocol_version: self.protocol_version,
            artifact: ArtifactEnvelope {
                format: "open-office-artifact".into(),
                schema_version: self.schema_version,
                artifact_id: self.artifact_id.clone(),
                revision: self.engine.revision(),
                kind: ArtifactKind::Document,
                payload: ArtifactPayload::Document(self.engine.model().clone()),
            },
        };
        serde_json::to_string(&snapshot).map_err(|error| js_error("Snapshot 序列化失败", error))
    }

    /// 当前 engine revision；前端事务必须以此 revision 作为 baseRevision。
    pub fn revision(&self) -> u64 {
        self.engine.revision()
    }
}

fn js_error(context: &str, error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&format!("{context}：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oo_schema::{ArtifactPayload, DocumentModel};

    fn snapshot() -> String {
        let mut artifact = ArtifactEnvelope::new(
            "doc-wasm",
            ArtifactPayload::Document(DocumentModel::empty()),
        );
        artifact.revision = 7;
        serde_json::to_string(&SnapshotEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            artifact,
        })
        .unwrap()
    }

    #[test]
    fn load_dispatch_and_read_paths_use_canonical_engine() {
        let mut session = load_snapshot(&snapshot()).unwrap();
        let block = session.read_block("block-1").unwrap();
        assert!(block.contains("block-1"));
        let blocks = session.read_blocks(r#"["block-1"]"#).unwrap();
        assert!(blocks.contains("block-1"));
        let result = session
            .dispatch(
                &serde_json::json!({
                    "baseRevision": 7,
                    "commands": [{
                        "type": "replaceBlockText",
                        "blockId": "block-1",
                        "content": {"text": "hello", "runs": []}
                    }]
                })
                .to_string(),
            )
            .unwrap();
        assert!(result.contains("changedBlocks"));
        assert_eq!(session.revision(), 8);
        assert!(session.can_undo());
        assert!(!session.can_redo());
        let undone = session.undo().unwrap();
        assert!(undone.contains("changedBlocks"));
        assert_eq!(session.revision(), 9);
        assert!(!session.can_undo());
        assert!(session.can_redo());
        let redone = session.redo().unwrap();
        assert!(redone.contains("changedBlocks"));
        assert_eq!(session.revision(), 10);
        assert!(session.read_change_set().unwrap().is_some());
        let snapshot: SnapshotEnvelope =
            serde_json::from_str(&session.read_snapshot().unwrap()).unwrap();
        assert_eq!(snapshot.artifact.revision, 10);
    }

    #[test]
    fn load_snapshot_rejects_an_older_schema_version_at_the_wasm_boundary() {
        let mut value: SnapshotEnvelope = serde_json::from_str(&snapshot()).unwrap();
        let current = value.artifact.schema_version;
        assert!(current > 0, "fixture must use a positive schema version");
        value.artifact.schema_version = current - 1;
        assert!(matches!(
            value.validate(),
            Err(oo_protocol::ProtocolValidationError::Schema(
                oo_schema::SchemaValidationError::UnsupportedSchemaVersion(_)
            ))
        ));
    }

    #[test]
    fn dispatch_rejects_retired_generic_block_update_without_advancing_revision() {
        let result: Result<DocumentCommandBatch, _> = serde_json::from_value(serde_json::json!({
            "baseRevision": 7,
            "commands": [{
                "type": "updateBlock",
            "blockId": "block-1",
                "patch": {"content": {"text": "legacy", "runs": []}}
            }]
        }));
        assert!(
            result.is_err(),
            "WASM DTO must not expose generic block update"
        );
    }
}
