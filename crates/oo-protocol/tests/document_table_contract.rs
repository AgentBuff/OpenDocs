//! Public protocol contracts for Document table commands.

use oo_protocol::{
    ArtifactCapability, ArtifactCapabilityStatus, ArtifactCommandCapability,
    ArtifactCommandEnvelope, ArtifactFeatureCapabilities, ArtifactTransportCapability,
    CapabilityCatalog, CommandRecord, OperationRecord, TransactionOrigin,
    CAPABILITY_CONTRACT_VERSION, CURRENT_PROTOCOL_VERSION,
};
use oo_schema::ArtifactKind;

const TABLE_COMMANDS: &[&str] = &[
    "document.replaceTableCellText",
    "document.patchTableCellInlineRange",
    "document.formatTableCells",
    "document.setTableBorders",
    "document.applyTableBorderPreset",
    "document.insertTableRow",
    "document.insertTableColumn",
    "document.deleteTableRow",
    "document.deleteTableColumn",
    "document.setTableColumnWidth",
    "document.setTableRowHeight",
    "document.mergeTableCells",
    "document.splitTableCells",
];

#[test]
fn table_commands_are_discoverable_as_stable_semantic_type_ids() {
    let catalog = CapabilityCatalog {
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
            features: ArtifactFeatureCapabilities {
                edit: ArtifactCapabilityStatus::Stable,
                history: ArtifactCapabilityStatus::Stable,
                projection: ArtifactCapabilityStatus::Stable,
                import: ArtifactCapabilityStatus::Stable,
                export: ArtifactCapabilityStatus::Preview,
                assets: ArtifactCapabilityStatus::Stable,
                presence: ArtifactCapabilityStatus::Planned,
            },
            commands: TABLE_COMMANDS
                .iter()
                .map(|type_id| ArtifactCommandCapability {
                    type_id: (*type_id).into(),
                    scope: "document.table".into(),
                    requires_revision: true,
                    supports_idempotency: true,
                })
                .collect(),
        }],
    };
    catalog.validate().expect("table capability catalog");

    let encoded = serde_json::to_string(&catalog).expect("serialize capability catalog");
    let decoded: CapabilityCatalog =
        serde_json::from_str(&encoded).expect("decode capability catalog");
    let commands = &decoded.artifacts[0].commands;
    assert_eq!(commands.len(), TABLE_COMMANDS.len());
    assert!(commands
        .iter()
        .all(|command| command.scope == "document.table"));
    assert!(!commands
        .iter()
        .any(|command| command.type_id == "document.replaceBlockText"));
}

#[test]
fn selection_operations_stay_outside_persistent_commands() {
    let operation = OperationRecord {
        operation_id: "selection-1".into(),
        type_id: "document.tableSelection".into(),
        payload: serde_json::json!({
            "blockId": "table-1",
            "kind": "range",
            "startRowId": "r-1",
            "endRowId": "r-2",
            "startColumnId": "c-1",
            "endColumnId": "c-2"
        }),
    };
    operation.validate().expect("selection operation");
    let json = serde_json::to_value(&operation).expect("serialize operation");
    assert!(json.get("commandId").is_none());
    assert_eq!(json["typeId"], "document.tableSelection");

    let command = ArtifactCommandEnvelope {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        transaction_id: "tx-table-1".into(),
        intent_id: "merge-table".into(),
        artifact_id: "doc-1".into(),
        actor_id: "agent-1".into(),
        base_revision: 4,
        origin: TransactionOrigin::Local,
        commands: vec![CommandRecord {
            command_id: "merge-1".into(),
            type_id: "document.mergeTableCells".into(),
            payload: serde_json::json!({
                "type": "mergeTableCells",
                "blockId": "table-1",
                "range": {
                    "startRowId": "r-1",
                    "endRowId": "r-2",
                    "startColumnId": "c-1",
                    "endColumnId": "c-2"
                }
            }),
        }],
    };
    command.validate().expect("merge command");
    assert_ne!(operation.operation_id, command.commands[0].command_id);
}
