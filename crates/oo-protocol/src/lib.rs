//! 跨客户端、服务端和协同适配器的稳定协议。
//!
//! 协议只知道 Artifact、revision 和可版本化的 command/mutation/event record，不知道
//! Yjs/Yrs、DOM、Canvas 或 React。具体 Artifact engine 负责校验对应 capability 的 payload。

use std::collections::HashSet;

use oo_schema::{ArtifactEnvelope, ArtifactKind, SchemaValidationError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CURRENT_PROTOCOL_VERSION: u16 = 1;

/// Version of the machine-readable capability contract exposed by the server.
///
/// This is deliberately separate from `CURRENT_PROTOCOL_VERSION`: a client may
/// understand the transaction envelope while learning new capabilities without
/// requiring a protocol migration.
pub const CAPABILITY_CONTRACT_VERSION: u16 = 1;
pub const CAPABILITIES_PATH: &str = "/api/capabilities";
/// Version of the read-only Artifact projection response.
/// Version two adds the strictly read-only Presentation deck/slide/node
/// projections. It remains independent from the write protocol, whose typed
/// Presentation commands are discovered through the capability catalog.
pub const PROJECTION_CONTRACT_VERSION: u16 = 2;

/// Server-authoritative history command type.
pub const DOCUMENT_HISTORY_TYPE_ID: &str = "document.history";
/// Server-authoritative Presentation history command type.
///
/// The payload is only an undo/redo intent. The server reconstructs the
/// typed Presentation engine journal from the durable semantic transaction
/// record; clients never send inverse scene mutations or snapshots.
pub const PRESENTATION_HISTORY_TYPE_ID: &str = "presentation.history";

/// The write/read boundary that every Artifact client can use after discovering
/// the capability catalog. Paths are URI templates rather than a second RPC
/// surface, so agents, SDKs and MCP adapters all speak the same REST contract.
#[derive(schemars::JsonSchema, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactTransportCapability {
    pub snapshot_endpoint: String,
    pub transaction_endpoint: String,
    pub revision_header: String,
    pub idempotency_header: String,
}

#[derive(schemars::JsonSchema, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArtifactCapabilityStatus {
    Stable,
    Planned,
}

/// A semantic command accepted by an Artifact engine. `scope` allows a client
/// to discover table-specific commands without pretending that tables are a
/// second Artifact model (their namespace remains `document.table`).
#[derive(schemars::JsonSchema, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactCommandCapability {
    pub type_id: String,
    pub scope: String,
    pub requires_revision: bool,
    pub supports_idempotency: bool,
}

#[derive(schemars::JsonSchema, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactCapability {
    pub kind: ArtifactKind,
    pub namespace: String,
    pub status: ArtifactCapabilityStatus,
    /// Empty for planned engines. An empty list is intentional: it is not a
    /// promise that an unimplemented command exists.
    pub commands: Vec<ArtifactCommandCapability>,
}

/// Versioned discovery response for agent/SDK/MCP clients.
#[derive(schemars::JsonSchema, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityCatalog {
    pub protocol_version: u16,
    pub contract_version: u16,
    pub transport: ArtifactTransportCapability,
    pub artifacts: Vec<ArtifactCapability>,
}

/// Stable projection names are intentionally small. They describe read models
/// and never become a second writable document model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArtifactProjectionKind {
    Outline,
    Block,
    /// Compact, deck-level Presentation facts such as page geometry and counts.
    Presentation,
    /// Paginated Presentation slide outline.
    PresentationOutline,
    /// A single Presentation slide, with explicitly requested optional sections.
    PresentationSlide,
    /// A single slide-scoped Presentation node.
    PresentationNode,
    Mindmap,
    Whiteboard,
}

/// Common envelope for bounded read projections. `data` is an opaque, additive
/// projection payload so future Artifact engines can evolve their read model
/// without changing the transaction protocol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionEnvelope {
    pub protocol_version: u16,
    pub contract_version: u16,
    pub artifact_id: String,
    pub revision: u64,
    pub projection: ArtifactProjectionKind,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub truncated: bool,
}

impl CapabilityCatalog {
    pub fn validate(&self) -> Result<(), ProtocolValidationError> {
        validate_protocol_version(self.protocol_version)?;
        if self.contract_version == 0 || self.contract_version > CAPABILITY_CONTRACT_VERSION {
            return Err(
                ProtocolValidationError::UnsupportedCapabilityContractVersion(
                    self.contract_version,
                ),
            );
        }
        if self.transport.snapshot_endpoint.trim().is_empty()
            || self.transport.transaction_endpoint.trim().is_empty()
            || self.transport.revision_header.trim().is_empty()
            || self.transport.idempotency_header.trim().is_empty()
        {
            return Err(ProtocolValidationError::InvalidCapabilityTransport);
        }
        let mut kinds = Vec::new();
        let mut namespaces = HashSet::new();
        for artifact in &self.artifacts {
            if artifact.namespace.trim().is_empty()
                || !namespaces.insert(artifact.namespace.as_str())
            {
                return Err(ProtocolValidationError::DuplicateCapabilityNamespace(
                    artifact.namespace.clone(),
                ));
            }
            if kinds.contains(&artifact.kind) {
                return Err(ProtocolValidationError::DuplicateCapabilityKind(
                    artifact.kind,
                ));
            }
            kinds.push(artifact.kind);
            let mut commands = HashSet::new();
            for command in &artifact.commands {
                if command.type_id.trim().is_empty() || command.scope.trim().is_empty() {
                    return Err(ProtocolValidationError::InvalidCapabilityCommand);
                }
                if !commands.insert(command.type_id.as_str()) {
                    return Err(ProtocolValidationError::DuplicateCapabilityCommand(
                        command.type_id.clone(),
                    ));
                }
            }
            if artifact.status == ArtifactCapabilityStatus::Planned && !artifact.commands.is_empty()
            {
                return Err(ProtocolValidationError::PlannedCapabilityHasCommands(
                    artifact.namespace.clone(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotEnvelope {
    pub protocol_version: u16,
    pub artifact: ArtifactEnvelope,
}

impl SnapshotEnvelope {
    pub fn validate(&self) -> Result<(), ProtocolValidationError> {
        validate_protocol_version(self.protocol_version)?;
        self.artifact.validate()?;
        Ok(())
    }
}

/// 一次用户意图或协同更新的网络边界。
///
/// 这里表达的是语义 command，不是 Document engine 的内部 operation。不同 Artifact
/// 通过 `type_id` 扩展能力，服务端必须在进入领域 engine 前做对应的 command 校验。
#[derive(schemars::JsonSchema, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactCommandEnvelope {
    pub protocol_version: u16,
    pub transaction_id: String,
    pub intent_id: String,
    pub artifact_id: String,
    pub actor_id: String,
    pub base_revision: u64,
    pub origin: TransactionOrigin,
    pub commands: Vec<CommandRecord>,
}

impl ArtifactCommandEnvelope {
    pub fn validate(&self) -> Result<(), ProtocolValidationError> {
        validate_protocol_version(self.protocol_version)?;
        for (name, value) in [
            ("transactionId", &self.transaction_id),
            ("intentId", &self.intent_id),
            ("artifactId", &self.artifact_id),
            ("actorId", &self.actor_id),
        ] {
            if value.trim().is_empty() {
                return Err(ProtocolValidationError::EmptyId(name));
            }
        }
        if self.commands.is_empty() {
            return Err(ProtocolValidationError::EmptyCommands);
        }
        let mut command_ids = HashSet::new();
        for command in &self.commands {
            command.validate()?;
            if !command_ids.insert(&command.command_id) {
                return Err(ProtocolValidationError::DuplicateCommandId(
                    command.command_id.clone(),
                ));
            }
        }
        Ok(())
    }

    pub fn context(&self) -> CommandContext {
        CommandContext {
            artifact_id: self.artifact_id.clone(),
            transaction_id: self.transaction_id.clone(),
            intent_id: self.intent_id.clone(),
            actor_id: self.actor_id.clone(),
            base_revision: self.base_revision,
            origin: self.origin,
        }
    }
}

/// Command handler 所需的不可变上下文。领域层不读取 HTTP、DOM 或 React 状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandContext {
    pub artifact_id: String,
    pub transaction_id: String,
    pub intent_id: String,
    pub actor_id: String,
    pub base_revision: u64,
    pub origin: TransactionOrigin,
}

/// Command 是用户意图；payload 由对应 Artifact capability 负责校验。
#[derive(schemars::JsonSchema, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRecord {
    pub command_id: String,
    pub type_id: String,
    pub payload: Value,
}

impl CommandRecord {
    fn validate(&self) -> Result<(), ProtocolValidationError> {
        if self.command_id.trim().is_empty() {
            return Err(ProtocolValidationError::EmptyId("commandId"));
        }
        if self.type_id.trim().is_empty() {
            return Err(ProtocolValidationError::EmptyId("typeId"));
        }
        if self.payload.is_null() {
            return Err(ProtocolValidationError::NullPayload(self.type_id.clone()));
        }
        Ok(())
    }
}

/// Operation 表示不进入 Artifact snapshot 的视图/协同状态，例如选区和滚动位置。
/// 它与 CommandRecord 有意使用不同的 ID 字段，避免把临时状态误当成持久化意图。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRecord {
    pub operation_id: String,
    pub type_id: String,
    pub payload: Value,
}

impl OperationRecord {
    pub fn validate(&self) -> Result<(), ProtocolValidationError> {
        if self.operation_id.trim().is_empty() {
            return Err(ProtocolValidationError::EmptyId("operationId"));
        }
        if self.type_id.trim().is_empty() {
            return Err(ProtocolValidationError::EmptyId("typeId"));
        }
        if self.payload.is_null() {
            return Err(ProtocolValidationError::NullPayload(self.type_id.clone()));
        }
        Ok(())
    }
}

/// 跨 Artifact 的最小持久化变更记录。具体 payload 由 Mutation registry 解释。
#[derive(schemars::JsonSchema, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationRecord {
    pub type_id: String,
    pub payload: Value,
}

/// 提交后的领域事实。事件只能在事务成功提交后产生。
#[derive(schemars::JsonSchema, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainEventRecord {
    pub event_id: String,
    pub type_id: String,
    pub payload: Value,
}

#[derive(schemars::JsonSchema, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityRef {
    pub entity_type: String,
    pub entity_id: String,
}

/// 只表达增量失效范围，不携带完整 snapshot。
#[derive(schemars::JsonSchema, Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Invalidation {
    pub changed_entities: Vec<EntityRef>,
    pub changed_containers: Vec<EntityRef>,
    pub structure_changed: bool,
}

/// 所有 Artifact 写入的唯一提交返回值。
#[derive(schemars::JsonSchema, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitResult {
    pub protocol_version: u16,
    pub artifact_id: String,
    pub transaction_id: String,
    pub revision: u64,
    pub invalidation: Invalidation,
    pub mutations: Vec<MutationRecord>,
    pub events: Vec<DomainEventRecord>,
}

/// 浏览器保存队列的事务单元；不能再把一批 command 拆成无边界 operation。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingTransaction {
    pub sequence: u64,
    pub envelope: ArtifactCommandEnvelope,
}

#[derive(schemars::JsonSchema, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransactionOrigin {
    Local,
    Remote,
    Undo,
    Redo,
    Import,
    System,
}

/// A history request is deliberately tiny: the server owns the mutation journal and resolves the
/// target snapshot. Clients never send inverse model patches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryAction {
    Undo,
    Redo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentHistoryOperation {
    pub action: HistoryAction,
}

impl DocumentHistoryOperation {
    pub fn command_record(&self, command_id: impl Into<String>) -> CommandRecord {
        CommandRecord {
            command_id: command_id.into(),
            type_id: DOCUMENT_HISTORY_TYPE_ID.to_string(),
            payload: serde_json::to_value(self).expect("history operation is serializable"),
        }
    }

    pub fn from_record(record: &CommandRecord) -> Result<Self, serde_json::Error> {
        serde_json::from_value(record.payload.clone())
    }
}

/// Server-authoritative undo/redo intent for a Presentation Artifact.
///
/// Kept separate from `DocumentHistoryOperation` so a discovered command is
/// always scoped to the owning artifact engine rather than relying on a
/// cross-kind compatibility alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationHistoryOperation {
    pub action: HistoryAction,
}

impl PresentationHistoryOperation {
    pub fn command_record(&self, command_id: impl Into<String>) -> CommandRecord {
        CommandRecord {
            command_id: command_id.into(),
            type_id: PRESENTATION_HISTORY_TYPE_ID.to_string(),
            payload: serde_json::to_value(self).expect("presentation history is serializable"),
        }
    }

    pub fn from_record(record: &CommandRecord) -> Result<Self, serde_json::Error> {
        serde_json::from_value(record.payload.clone())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolValidationError {
    #[error("不支持的 protocol 版本：{0}")]
    UnsupportedProtocolVersion(u16),
    #[error("不支持的 capability contract 版本：{0}")]
    UnsupportedCapabilityContractVersion(u16),
    #[error("capability transport 不能为空")]
    InvalidCapabilityTransport,
    #[error("capability namespace 重复或为空：{0}")]
    DuplicateCapabilityNamespace(String),
    #[error("capability kind 重复：{0:?}")]
    DuplicateCapabilityKind(ArtifactKind),
    #[error("capability command 无效")]
    InvalidCapabilityCommand,
    #[error("capability command 重复：{0}")]
    DuplicateCapabilityCommand(String),
    #[error("planned capability 不得声明 command：{0}")]
    PlannedCapabilityHasCommands(String),
    #[error("{0} 不能为空")]
    EmptyId(&'static str),
    #[error("事务不能没有 command")]
    EmptyCommands,
    #[error("commandId 重复：{0}")]
    DuplicateCommandId(String),
    #[error("command {0} 的 payload 不能为 null")]
    NullPayload(String),
    #[error("schema 校验失败：{0}")]
    Schema(#[from] SchemaValidationError),
}

fn validate_protocol_version(version: u16) -> Result<(), ProtocolValidationError> {
    if version == 0 || version > CURRENT_PROTOCOL_VERSION {
        return Err(ProtocolValidationError::UnsupportedProtocolVersion(version));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oo_schema::{ArtifactPayload, DocumentModel};

    #[test]
    fn command_envelope_round_trips_with_context() {
        let envelope = ArtifactCommandEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            transaction_id: "tx-1".into(),
            intent_id: "intent-1".into(),
            artifact_id: "doc-1".into(),
            actor_id: "user-1".into(),
            base_revision: 4,
            origin: TransactionOrigin::Local,
            commands: vec![CommandRecord {
                command_id: "cmd-1".into(),
                type_id: "document.insertBlock".into(),
                payload: serde_json::json!({"blockId":"p-1"}),
            }],
        };
        envelope.validate().unwrap();
        assert_eq!(envelope.context().intent_id, "intent-1");
        let json = serde_json::to_string(&envelope).unwrap();
        let back: ArtifactCommandEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope, back);
    }

    #[test]
    fn snapshot_validates_the_nested_artifact() {
        let snapshot = SnapshotEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            artifact: ArtifactEnvelope::new(
                "doc-1",
                ArtifactPayload::Document(DocumentModel::default()),
            ),
        };
        snapshot.validate().unwrap();
    }

    #[test]
    fn snapshot_rejects_an_older_artifact_schema_at_the_protocol_boundary() {
        let mut snapshot = SnapshotEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            artifact: ArtifactEnvelope::new(
                "doc-1",
                ArtifactPayload::Document(DocumentModel::default()),
            ),
        };
        assert!(snapshot.artifact.schema_version > 0);
        snapshot.artifact.schema_version -= 1;

        assert!(matches!(
            snapshot.validate(),
            Err(ProtocolValidationError::Schema(
                oo_schema::SchemaValidationError::UnsupportedSchemaVersion(_)
            ))
        ));
    }

    #[test]
    fn snapshot_rejects_the_retired_attrs_and_payload_block_shape() {
        let snapshot = SnapshotEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            artifact: ArtifactEnvelope::new(
                "doc-legacy-shape",
                ArtifactPayload::Document(DocumentModel::empty()),
            ),
        };
        let mut wire = serde_json::to_value(snapshot).unwrap();
        let block = &mut wire["artifact"]["payload"]["data"]["blocks"][0];
        let object = block.as_object_mut().expect("empty document block object");
        object.remove("presentation");
        object.remove("data");
        object.insert("attrs".into(), serde_json::json!({"align":"left"}));
        object.insert(
            "payload".into(),
            serde_json::json!({"type":"none","data":null}),
        );

        let decoded: Result<SnapshotEnvelope, _> = serde_json::from_value(wire);
        assert!(decoded.is_err(), "旧 attrs/payload 不得穿过 protocol 边界");
    }

    #[test]
    fn command_envelope_rejects_duplicate_command_ids() {
        let envelope = ArtifactCommandEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            transaction_id: "tx-1".into(),
            intent_id: "intent-1".into(),
            artifact_id: "doc-1".into(),
            actor_id: "user-1".into(),
            base_revision: 0,
            origin: TransactionOrigin::Local,
            commands: vec![
                CommandRecord {
                    command_id: "cmd-1".into(),
                    type_id: "document.insertBlock".into(),
                    payload: serde_json::json!({"x":1}),
                },
                CommandRecord {
                    command_id: "cmd-1".into(),
                    type_id: "document.insertBlock".into(),
                    payload: serde_json::json!({"x":2}),
                },
            ],
        };
        assert!(matches!(
            envelope.validate(),
            Err(ProtocolValidationError::DuplicateCommandId(id)) if id == "cmd-1"
        ));
    }

    #[test]
    fn history_command_round_trips_as_a_protocol_record() {
        let operation = DocumentHistoryOperation {
            action: HistoryAction::Undo,
        };
        let record = operation.command_record("history-1");
        assert_eq!(record.type_id, DOCUMENT_HISTORY_TYPE_ID);
        let json = serde_json::to_string(&record).unwrap();
        let back: CommandRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(
            DocumentHistoryOperation::from_record(&back).unwrap(),
            operation
        );

        let presentation_operation = PresentationHistoryOperation {
            action: HistoryAction::Redo,
        };
        let record = presentation_operation.command_record("presentation-history-1");
        assert_eq!(record.type_id, PRESENTATION_HISTORY_TYPE_ID);
        assert_eq!(
            PresentationHistoryOperation::from_record(&record).unwrap(),
            presentation_operation
        );
    }

    #[test]
    fn operation_is_not_a_command() {
        let operation = OperationRecord {
            operation_id: "selection-1".into(),
            type_id: "document.setSelection".into(),
            payload: serde_json::json!({"blockId":"p-1"}),
        };
        operation.validate().unwrap();
        let json = serde_json::to_value(&operation).unwrap();
        assert!(json.get("commandId").is_none());
        assert_eq!(json["operationId"], "selection-1");
    }

    #[test]
    fn unknown_command_capabilities_round_trip_without_protocol_changes() {
        let envelope = ArtifactCommandEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            transaction_id: "tx-future".into(),
            intent_id: "intent-future".into(),
            artifact_id: "doc-1".into(),
            actor_id: "user-1".into(),
            base_revision: 9,
            origin: TransactionOrigin::Local,
            commands: vec![CommandRecord {
                command_id: "cmd-future".into(),
                type_id: "document.futureBlockCapability".into(),
                payload: serde_json::json!({
                    "block": {"type": "future.databaseView", "config": {"columns": ["name"]}},
                    "opaque": [1, true, null]
                }),
            }],
        };
        envelope.validate().unwrap();
        let encoded = serde_json::to_string(&envelope).unwrap();
        let decoded: ArtifactCommandEnvelope = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, envelope);
        assert_eq!(
            decoded.commands[0].type_id,
            "document.futureBlockCapability"
        );
        assert_eq!(decoded.commands[0].payload["opaque"][1], true);
    }

    #[test]
    fn protocol_rejects_zero_and_future_versions_at_the_boundary() {
        let mut snapshot = SnapshotEnvelope {
            protocol_version: 0,
            artifact: ArtifactEnvelope::new(
                "doc-1",
                ArtifactPayload::Document(DocumentModel::default()),
            ),
        };
        assert!(matches!(
            snapshot.validate(),
            Err(ProtocolValidationError::UnsupportedProtocolVersion(0))
        ));

        snapshot.protocol_version = CURRENT_PROTOCOL_VERSION + 1;
        assert!(matches!(
            snapshot.validate(),
            Err(ProtocolValidationError::UnsupportedProtocolVersion(version))
                if version == CURRENT_PROTOCOL_VERSION + 1
        ));

        let mut command = ArtifactCommandEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION + 1,
            transaction_id: "tx-1".into(),
            intent_id: "intent-1".into(),
            artifact_id: "doc-1".into(),
            actor_id: "user-1".into(),
            base_revision: 0,
            origin: TransactionOrigin::Local,
            commands: vec![CommandRecord {
                command_id: "cmd-1".into(),
                type_id: "document.insertBlock".into(),
                payload: serde_json::json!({"x": 1}),
            }],
        };
        assert!(matches!(
            command.validate(),
            Err(ProtocolValidationError::UnsupportedProtocolVersion(version))
                if version == CURRENT_PROTOCOL_VERSION + 1
        ));
        command.protocol_version = CURRENT_PROTOCOL_VERSION;
        command.commands[0].payload = Value::Null;
        assert!(matches!(
            command.validate(),
            Err(ProtocolValidationError::NullPayload(type_id)) if type_id == "document.insertBlock"
        ));
    }

    #[test]
    fn commit_result_round_trips_typed_invalidation_mutations_and_events() {
        let result = CommitResult {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            artifact_id: "doc-1".into(),
            transaction_id: "tx-1".into(),
            revision: 10,
            invalidation: Invalidation {
                changed_entities: vec![EntityRef {
                    entity_type: "document.block".into(),
                    entity_id: "p-1".into(),
                }],
                changed_containers: Vec::new(),
                structure_changed: false,
            },
            mutations: vec![MutationRecord {
                type_id: "document.replaceBlockText".into(),
                payload: serde_json::json!({"blockId":"p-1"}),
            }],
            events: vec![
                DomainEventRecord {
                    event_id: "event-1".into(),
                    type_id: "document.blockChanged".into(),
                    payload: serde_json::json!({"blockId":"p-1"}),
                },
                DomainEventRecord {
                    event_id: "event-future".into(),
                    type_id: "future.databaseChanged".into(),
                    // Unknown event payloads remain opaque protocol values. The
                    // boundary must not narrow a future event to object-only JSON.
                    payload: serde_json::json!(["opaque", 7, true, null]),
                },
            ],
        };
        let encoded = serde_json::to_string(&result).unwrap();
        let decoded: CommitResult = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, result);
        assert_eq!(decoded.invalidation.changed_entities[0].entity_id, "p-1");
        assert_eq!(decoded.events[1].type_id, "future.databaseChanged");
        assert_eq!(
            decoded.events[1].payload,
            serde_json::json!(["opaque", 7, true, null])
        );
    }

    #[test]
    fn capability_catalog_round_trips_and_rejects_planned_commands() {
        let catalog = CapabilityCatalog {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            contract_version: CAPABILITY_CONTRACT_VERSION,
            transport: ArtifactTransportCapability {
                snapshot_endpoint: "/api/artifacts/{artifactId}/snapshot".into(),
                transaction_endpoint: "/api/artifacts/{artifactId}/transactions".into(),
                revision_header: "If-Match".into(),
                idempotency_header: "x-transaction-id".into(),
            },
            artifacts: vec![
                ArtifactCapability {
                    kind: ArtifactKind::Document,
                    namespace: "document".into(),
                    status: ArtifactCapabilityStatus::Stable,
                    commands: vec![ArtifactCommandCapability {
                        type_id: "document.insertTableRow".into(),
                        scope: "document.table".into(),
                        requires_revision: true,
                        supports_idempotency: true,
                    }],
                },
                ArtifactCapability {
                    kind: ArtifactKind::Spreadsheet,
                    namespace: "spreadsheet".into(),
                    status: ArtifactCapabilityStatus::Planned,
                    commands: Vec::new(),
                },
            ],
        };
        catalog.validate().unwrap();
        let json = serde_json::to_string(&catalog).unwrap();
        let decoded: CapabilityCatalog = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, catalog);

        let mut invalid = catalog;
        invalid.artifacts[1]
            .commands
            .push(ArtifactCommandCapability {
                type_id: "spreadsheet.fakeCommand".into(),
                scope: "spreadsheet".into(),
                requires_revision: true,
                supports_idempotency: true,
            });
        assert!(matches!(
            invalid.validate(),
            Err(ProtocolValidationError::PlannedCapabilityHasCommands(namespace))
                if namespace == "spreadsheet"
        ));
    }

    #[test]
    fn projection_envelope_round_trips_with_an_opaque_cursor() {
        let envelope = ProjectionEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            contract_version: PROJECTION_CONTRACT_VERSION,
            artifact_id: "doc-1".into(),
            revision: 7,
            projection: ArtifactProjectionKind::Block,
            data: serde_json::json!({
                "blockId": "block-1",
                "refs": [{"artifactId": "doc-1", "blockId": "block-1"}]
            }),
            cursor: Some("opaque:7:0".into()),
            next_cursor: Some("opaque:7:1".into()),
            truncated: true,
        };
        let encoded = serde_json::to_string(&envelope).unwrap();
        let decoded: ProjectionEnvelope = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, envelope);
        assert_eq!(decoded.projection, ArtifactProjectionKind::Block);
    }
}

/// 机器可读协议形状的单源出口（ADR-0010）。
///
/// schema 直接由 Rust 类型派生，是后续 OpenAPI components 与 SDK 类型生成
/// 的唯一真相。任何 wire 兼容性破坏都会在这里改变形状，由 golden 快照
/// 测试拦截；`$defs` 引用保证同名类型全局唯一。
pub fn generate_contract_schemas() -> serde_json::Map<String, serde_json::Value> {
    macro_rules! schema_of {
        ($map:ident, $($t:ty),+ $(,)?) => {$(
            $map.insert(
                stringify!($t).rsplit("::").next().unwrap().to_string(),
                serde_json::to_value(schemars::schema_for!($t))
                    .expect("contract schema must serialize"),
            );
        )+};
    }
    let mut schemas = serde_json::Map::new();
    schema_of!(
        schemas,
        ArtifactCommandEnvelope,
        CommandRecord,
        TransactionOrigin,
        CommitResult,
        MutationRecord,
        DomainEventRecord,
        EntityRef,
        Invalidation,
        CapabilityCatalog,
        ArtifactCapability,
        ArtifactCommandCapability,
        ArtifactTransportCapability,
        ArtifactCapabilityStatus,
    );
    schemas
}
