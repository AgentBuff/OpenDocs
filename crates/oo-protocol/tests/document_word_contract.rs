//! Public protocol contracts for Word/document semantic commands.

use oo_protocol::{
    ArtifactCapability, ArtifactCapabilityStatus, ArtifactCommandCapability,
    ArtifactCommandEnvelope, ArtifactTransportCapability, CapabilityCatalog, CommandRecord,
    TransactionOrigin, CAPABILITY_CONTRACT_VERSION, CURRENT_PROTOCOL_VERSION,
};
use oo_schema::ArtifactKind;

const WORD_COMMANDS: &[&str] = &[
    "document.insertBlock",
    "document.insertQuote",
    "document.insertTodo",
    "document.insertLink",
    "document.insertDivider",
    "document.setBlockPresentation",
    "document.patchInlineRange",
    "document.convertBlock",
    "document.replaceBlockText",
    "document.setTodoChecked",
    "document.convertToLink",
    "document.setLinkTarget",
    "document.setPageSetup",
];

fn catalog() -> CapabilityCatalog {
    CapabilityCatalog {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        contract_version: CAPABILITY_CONTRACT_VERSION,
        transport: ArtifactTransportCapability {
            snapshot_endpoint: "/api/artifacts/{artifactId}/snapshot".into(),
            transaction_endpoint: "/api/artifacts/{artifactId}/transactions".into(),
            revision_header: "If-Match".into(),
            idempotency_header: "x-transaction-id".into(),
        },
        artifacts: vec![ArtifactCapability {
            kind: ArtifactKind::Document,
            namespace: "document".into(),
            status: ArtifactCapabilityStatus::Stable,
            commands: WORD_COMMANDS
                .iter()
                .map(|type_id| ArtifactCommandCapability {
                    type_id: (*type_id).into(),
                    scope: "document".into(),
                    requires_revision: true,
                    supports_idempotency: true,
                })
                .collect(),
        }],
    }
}

#[test]
fn word_commands_are_discoverable_and_round_trip() {
    let catalog = catalog();
    catalog.validate().expect("word capability catalog");
    let encoded = serde_json::to_string(&catalog).expect("serialize catalog");
    let decoded: CapabilityCatalog = serde_json::from_str(&encoded).expect("decode catalog");
    let ids: Vec<_> = decoded.artifacts[0]
        .commands
        .iter()
        .map(|command| command.type_id.as_str())
        .collect();
    assert_eq!(ids, WORD_COMMANDS);
}

#[test]
fn narrow_insert_commands_are_not_ui_operations() {
    let envelope = ArtifactCommandEnvelope {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        transaction_id: "tx-word-1".into(),
        intent_id: "insert-divider".into(),
        artifact_id: "doc-1".into(),
        actor_id: "agent-1".into(),
        base_revision: 7,
        origin: TransactionOrigin::Local,
        commands: vec![CommandRecord {
            command_id: "cmd-divider".into(),
            type_id: "document.insertDivider".into(),
            payload: serde_json::json!({
                "type": "insertDivider",
                "blockId": "divider-1",
                "index": 1,
            }),
        }],
    };
    envelope.validate().expect("semantic command envelope");
    let encoded = serde_json::to_value(&envelope).expect("serialize envelope");
    assert_eq!(encoded["commands"][0]["typeId"], "document.insertDivider");
    assert!(encoded["commands"][0].get("operationId").is_none());
}
