//! Offline-only compiler from a persisted v4 Presentation Artifact to a staged v5 Deck.
//!
//! The result is intentionally not [`crate::ArtifactEnvelope`]. Until the destructive v5
//! cutover, an online envelope can only contain the v4 payload and accepting this target output
//! there would create a dual-read path. Operational tooling writes this type to an isolated
//! staging run; P1-C is responsible for turning an approved run into a real v5 envelope.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    presentation_v5::{migrate_legacy_v4, Deck, LegacyMigrationReport},
    ArtifactEnvelope, LegacyPresentationModel, SchemaValidationError,
};

pub const PRESENTATION_V5_STAGE_FORMAT: &str = "open-office-presentation-v5-stage";
pub const PRESENTATION_V5_TARGET_SCHEMA_VERSION: u16 = 5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StagedPresentationV5 {
    pub format: String,
    pub schema_version: u16,
    pub source_schema_version: u16,
    pub artifact_id: String,
    pub source_revision: u64,
    pub deck: Deck,
    pub report: LegacyMigrationReport,
}

#[derive(Debug, thiserror::Error)]
pub enum PresentationV4MigrationError {
    #[error("迁移输入必须是 Artifact object")]
    InvalidEnvelope,
    #[error("仅支持 Presentation schema v4，实际为 {0}")]
    UnsupportedSourceVersion(u64),
    #[error("迁移输入格式不是 open-office-artifact")]
    InvalidFormat,
    #[error("迁移输入不是 Presentation Artifact")]
    NotPresentation,
    #[error("迁移输入缺少非空 artifactId")]
    MissingArtifactId,
    #[error("旧 Presentation payload 无法反序列化：{0}")]
    DeserializeLegacy(#[source] serde_json::Error),
    #[error("旧 Presentation payload 无效：{0}")]
    InvalidLegacy(#[source] SchemaValidationError),
    #[error("迁移结果无效：{0}")]
    InvalidTarget(#[source] SchemaValidationError),
}

/// Compile one raw v4 Presentation Artifact into an immutable v5 staging candidate.
///
/// `raw` is consumed so callers can read directly from a blob without building a second legacy
/// runtime model. The function validates both source and target before returning. It does not
/// access the filesystem and has no online side effects.
pub fn migrate_presentation_artifact_v4_to_v5(
    raw: Value,
) -> Result<StagedPresentationV5, PresentationV4MigrationError> {
    let object = raw
        .as_object()
        .ok_or(PresentationV4MigrationError::InvalidEnvelope)?;
    if object.get("format").and_then(Value::as_str) != Some("open-office-artifact") {
        return Err(PresentationV4MigrationError::InvalidFormat);
    }
    let version = object
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .ok_or(PresentationV4MigrationError::InvalidEnvelope)?;
    if version != 4 {
        return Err(PresentationV4MigrationError::UnsupportedSourceVersion(
            version,
        ));
    }
    let artifact_id = object
        .get("artifactId")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or(PresentationV4MigrationError::MissingArtifactId)?
        .to_owned();
    let source_revision = object
        .get("revision")
        .and_then(Value::as_u64)
        .ok_or(PresentationV4MigrationError::InvalidEnvelope)?;
    if object.get("kind").and_then(Value::as_str) != Some("presentation") {
        return Err(PresentationV4MigrationError::NotPresentation);
    }
    let payload = object
        .get("payload")
        .and_then(Value::as_object)
        .ok_or(PresentationV4MigrationError::InvalidEnvelope)?;
    if payload.get("kind").and_then(Value::as_str) != Some("presentation") {
        return Err(PresentationV4MigrationError::NotPresentation);
    }
    let legacy: LegacyPresentationModel = serde_json::from_value(
        payload
            .get("data")
            .cloned()
            .ok_or(PresentationV4MigrationError::InvalidEnvelope)?,
    )
    .map_err(PresentationV4MigrationError::DeserializeLegacy)?;
    legacy
        .validate()
        .map_err(PresentationV4MigrationError::InvalidLegacy)?;
    let (deck, report) =
        migrate_legacy_v4(&legacy).map_err(PresentationV4MigrationError::InvalidTarget)?;
    deck.validate()
        .map_err(PresentationV4MigrationError::InvalidTarget)?;
    Ok(StagedPresentationV5 {
        format: PRESENTATION_V5_STAGE_FORMAT.into(),
        schema_version: PRESENTATION_V5_TARGET_SCHEMA_VERSION,
        source_schema_version: 4,
        artifact_id,
        source_revision,
        deck,
        report,
    })
}

/// Turn an already audited staging candidate into the one and only online v5 envelope.
///
/// This deliberately accepts `StagedPresentationV5`, never raw v4 JSON. Storage tooling must
/// first create and review a complete staging run, then atomically replace the old snapshot with
/// the returned envelope while the API is stopped. Keeping this operation separate prevents an
/// HTTP reader from becoming a v4/v5 compatibility parser.
pub fn materialize_staged_presentation_v5(
    staged: StagedPresentationV5,
) -> Result<ArtifactEnvelope, PresentationV4MigrationError> {
    if staged.format != PRESENTATION_V5_STAGE_FORMAT
        || staged.schema_version != PRESENTATION_V5_TARGET_SCHEMA_VERSION
        || staged.source_schema_version != 4
    {
        return Err(PresentationV4MigrationError::InvalidEnvelope);
    }
    staged
        .deck
        .validate()
        .map_err(PresentationV4MigrationError::InvalidTarget)?;
    let mut envelope = ArtifactEnvelope::new(
        staged.artifact_id,
        crate::ArtifactPayload::Presentation(staged.deck),
    );
    envelope.revision = staged.source_revision;
    envelope
        .validate()
        .map_err(PresentationV4MigrationError::InvalidTarget)?;
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_shared_v4_fixture_to_self_describing_stage() {
        let staged = migrate_presentation_artifact_v4_to_v5(
            serde_json::from_str(include_str!(
                "../../../fixtures/presentation/migration/v4/known-and-unknown.json"
            ))
            .expect("fixture JSON"),
        )
        .expect("fixture must migrate");
        assert_eq!(staged.schema_version, 5);
        assert_eq!(staged.source_schema_version, 4);
        assert_eq!(staged.artifact_id, "legacy-presentation-1");
        assert!(staged
            .report
            .preserved_extensions
            .contains(&"custom-1".into()));
        staged.deck.validate().expect("staged deck must validate");
    }

    #[test]
    fn rejects_non_presentation_before_compiling() {
        let raw = serde_json::json!({
            "format": "open-office-artifact",
            "schemaVersion": 4,
            "artifactId": "doc-1",
            "revision": 0,
            "kind": "document",
            "payload": { "kind": "document", "data": {} }
        });
        assert!(matches!(
            migrate_presentation_artifact_v4_to_v5(raw),
            Err(PresentationV4MigrationError::NotPresentation)
        ));
    }

    #[test]
    fn rejects_invalid_legacy_hierarchy_without_staging_output() {
        let raw = serde_json::json!({
            "format": "open-office-artifact",
            "schemaVersion": 4,
            "artifactId": "bad-1",
            "revision": 0,
            "kind": "presentation",
            "payload": {
                "kind": "presentation",
                "data": {
                    "slides": [{
                        "id": "slide-1",
                        "elements": [{
                            "id": "shape-1", "typeId": "shape",
                            "transform": { "x": 0, "y": 0, "width": 10, "height": 10 },
                            "children": ["missing"]
                        }]
                    }]
                }
            }
        });
        assert!(matches!(
            migrate_presentation_artifact_v4_to_v5(raw),
            Err(PresentationV4MigrationError::InvalidLegacy(_))
        ));
    }

    #[test]
    fn materializes_a_staged_deck_as_v5_without_runtime_legacy_payload() {
        let staged = migrate_presentation_artifact_v4_to_v5(
            serde_json::from_str(include_str!(
                "../../../fixtures/presentation/migration/v4/known-and-unknown.json"
            ))
            .unwrap(),
        )
        .unwrap();
        let artifact = materialize_staged_presentation_v5(staged).unwrap();
        assert_eq!(artifact.schema_version, crate::CURRENT_SCHEMA_VERSION);
        assert!(matches!(
            artifact.payload,
            crate::ArtifactPayload::Presentation(_)
        ));
    }
}
