//! Offline migration naming/shape contracts.
//!
//! The public migration API exposes only explicit source-to-target steps. Runtime consumers
//! accept v4; the v1 entry point therefore performs the complete v1 -> v4 cutover in one pass.

use oo_schema::{
    migrate_artifact_v1_to_v3, migrate_artifact_v2_to_v3, ArtifactMigrationError,
    CURRENT_SCHEMA_VERSION,
};

fn v1_table_snapshot() -> serde_json::Value {
    serde_json::json!({
        "format": "open-office-artifact",
        "schemaVersion": 1,
        "artifactId": "doc-migration",
        "revision": 7,
        "kind": "document",
        "payload": {
            "kind": "document",
            "data": {
                "root": ["table"],
                "blocks": [{
                    "id": "table",
                    "kind": { "type": "table" },
                    "attrs": {},
                    "content": null,
                    "children": [],
                    "payload": {
                        "type": "table",
                        "data": {
                            "columns": [{"id": "c-1", "width": null}],
                            "rows": [{"id": "r-1", "height": null, "cells": [{
                                "id": "cell-1", "content": {"text": "A", "runs": []}
                            }]}]
                        }
                    }
                }],
                "pageSetup": null
            }
        }
    })
}

#[test]
fn v1_entry_point_is_explicitly_a_complete_v4_cutover() {
    let migrated = migrate_artifact_v1_to_v3(v1_table_snapshot()).expect("v1 migration");
    assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
    let json = serde_json::to_value(migrated).expect("serialize migrated artifact");
    assert_eq!(json["schemaVersion"], CURRENT_SCHEMA_VERSION);
    assert_eq!(
        json["payload"]["data"]["blocks"][0]["data"]["data"]["mergedRanges"],
        serde_json::json!([])
    );
}

#[test]
fn v1_and_v2_entry_points_reject_wrong_source_versions() {
    let v1 = v1_table_snapshot();
    assert!(matches!(
        migrate_artifact_v2_to_v3(v1.clone()),
        Err(ArtifactMigrationError::UnsupportedSourceVersion(1))
    ));
    let mut v2 = v1;
    v2["schemaVersion"] = serde_json::json!(2);
    assert!(matches!(
        migrate_artifact_v1_to_v3(v2),
        Err(ArtifactMigrationError::UnsupportedSourceVersion(2))
    ));
}
