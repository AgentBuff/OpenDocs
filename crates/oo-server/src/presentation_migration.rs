//! Filesystem staging for the offline Presentation v4 → v5 compiler.
//!
//! Staging is deliberately outside BlobStore. Before P1-C, replacing a v4 online snapshot with a
//! v5 Deck would make the running service unreadable and reintroduce a compatibility branch. A
//! fully validated run becomes visible only when this module atomically swaps `current.json`.

use std::path::{Path, PathBuf};

use oo_schema::presentation_migration::{
    StagedPresentationV5, PRESENTATION_V5_STAGE_FORMAT, PRESENTATION_V5_TARGET_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PreparedPresentationStage {
    pub source_key: String,
    pub staged: StagedPresentationV5,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationStageRunManifest {
    pub format: String,
    pub run_id: String,
    pub target_schema_version: u16,
    pub artifacts: Vec<PresentationStageArtifact>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationStageArtifact {
    pub source_key: String,
    pub artifact_id: String,
    pub source_revision: u64,
    pub deck_path: String,
    pub report_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationStagePointer {
    pub format: String,
    pub run_id: String,
    pub target_schema_version: u16,
}

#[derive(Debug, thiserror::Error)]
pub enum PresentationStageError {
    #[error("Presentation migration 不能为空")]
    EmptyRun,
    #[error("迁移候选 {artifact_id} 无效：{reason}")]
    InvalidCandidate { artifact_id: String, reason: String },
    #[error("staging 文件系统错误：{0}")]
    Io(#[from] std::io::Error),
    #[error("staging JSON 序列化错误：{0}")]
    Serialize(#[from] serde_json::Error),
}

/// Create a complete, immutable staging run and atomically activate it.
///
/// All candidates are validated before any filesystem mutation. Files are first written under a
/// private `.staging/<run-id>` directory. Only after every Deck, report and manifest is durable is
/// that directory renamed into `runs/<run-id>` and `current.json` replaced atomically. If a write
/// fails, the previous `current.json` remains active; an unreferenced staging/run directory is
/// safe to inspect or remove later and is never read by online code.
pub async fn activate_presentation_stage_run(
    output_root: impl AsRef<Path>,
    candidates: &[PreparedPresentationStage],
) -> Result<PresentationStageRunManifest, PresentationStageError> {
    if candidates.is_empty() {
        return Err(PresentationStageError::EmptyRun);
    }
    for candidate in candidates {
        validate_candidate(&candidate.staged)?;
    }

    let output_root = output_root.as_ref();
    let run_id = Uuid::new_v4().to_string();
    let staging_dir = output_root.join(".staging").join(&run_id);
    let run_dir = output_root.join("runs").join(&run_id);
    tokio::fs::create_dir_all(staging_dir.join("decks")).await?;
    tokio::fs::create_dir_all(staging_dir.join("reports")).await?;

    let mut artifacts = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let deck_path = format!("decks/{index:05}.deck.json");
        let report_path = format!("reports/{index:05}.report.json");
        write_json_atomic(&staging_dir.join(&deck_path), &candidate.staged).await?;
        write_json_atomic(&staging_dir.join(&report_path), &candidate.staged.report).await?;
        artifacts.push(PresentationStageArtifact {
            source_key: candidate.source_key.clone(),
            artifact_id: candidate.staged.artifact_id.clone(),
            source_revision: candidate.staged.source_revision,
            deck_path,
            report_path,
        });
    }
    let manifest = PresentationStageRunManifest {
        format: PRESENTATION_V5_STAGE_FORMAT.into(),
        run_id: run_id.clone(),
        target_schema_version: PRESENTATION_V5_TARGET_SCHEMA_VERSION,
        artifacts,
    };
    write_json_atomic(&staging_dir.join("manifest.json"), &manifest).await?;
    sync_directory(&staging_dir)?;

    let runs_dir = output_root.join("runs");
    tokio::fs::create_dir_all(&runs_dir).await?;
    tokio::fs::rename(&staging_dir, &run_dir).await?;
    sync_directory(&runs_dir)?;

    let current_path = output_root.join("current.json");
    if let Ok(previous) = tokio::fs::read(&current_path).await {
        let backup_path = output_root
            .join("backups")
            .join(format!("current-before-{run_id}.json"));
        write_bytes_atomic(&backup_path, &previous).await?;
    }
    write_json_atomic(
        &current_path,
        &PresentationStagePointer {
            format: PRESENTATION_V5_STAGE_FORMAT.into(),
            run_id,
            target_schema_version: PRESENTATION_V5_TARGET_SCHEMA_VERSION,
        },
    )
    .await?;
    sync_directory(output_root)?;
    Ok(manifest)
}

fn validate_candidate(candidate: &StagedPresentationV5) -> Result<(), PresentationStageError> {
    if candidate.format != PRESENTATION_V5_STAGE_FORMAT {
        return Err(invalid_candidate(candidate, "format 不匹配"));
    }
    if candidate.schema_version != PRESENTATION_V5_TARGET_SCHEMA_VERSION
        || candidate.source_schema_version != 4
    {
        return Err(invalid_candidate(candidate, "schema version 不匹配"));
    }
    candidate
        .deck
        .validate()
        .map_err(|error| invalid_candidate(candidate, error.to_string()))
}

fn invalid_candidate(
    candidate: &StagedPresentationV5,
    reason: impl Into<String>,
) -> PresentationStageError {
    PresentationStageError::InvalidCandidate {
        artifact_id: candidate.artifact_id.clone(),
        reason: reason.into(),
    }
}

async fn write_json_atomic<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), PresentationStageError> {
    let bytes = serde_json::to_vec_pretty(value)?;
    write_bytes_atomic(path, &bytes).await
}

async fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), PresentationStageError> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("staging path 没有父目录"))?;
    tokio::fs::create_dir_all(parent).await?;
    let temporary = parent.join(format!(".{}.tmp-{}", file_name(path), Uuid::new_v4()));
    let mut file = tokio::fs::File::create(&temporary).await?;
    use tokio::io::AsyncWriteExt;
    if let Err(error) = file.write_all(bytes).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error.into());
    }
    if let Err(error) = file.sync_all().await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error.into());
    }
    drop(file);
    if let Err(error) = tokio::fs::rename(&temporary, path).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error.into());
    }
    sync_directory(parent)?;
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("stage")
        .to_owned()
}

fn sync_directory(path: &Path) -> Result<(), PresentationStageError> {
    std::fs::File::open(path)?.sync_all()?;
    Ok(())
}

pub fn stage_run_dir(output_root: impl AsRef<Path>, run_id: &str) -> PathBuf {
    output_root.as_ref().join("runs").join(run_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oo_schema::presentation_migration::migrate_presentation_artifact_v4_to_v5;

    fn candidate(revision: u64) -> PreparedPresentationStage {
        let mut staged = migrate_presentation_artifact_v4_to_v5(
            serde_json::from_str(include_str!(
                "../../../fixtures/presentation/migration/v4/known-and-unknown.json"
            ))
            .unwrap(),
        )
        .unwrap();
        staged.source_revision = revision;
        PreparedPresentationStage {
            source_key: format!("legacy-presentation-1/artifacts/{revision}.json"),
            staged,
        }
    }

    fn temporary_output_root() -> PathBuf {
        std::env::temp_dir().join(format!("oo-presentation-stage-{}", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn activation_publishes_only_complete_runs_and_backs_up_previous_pointer() {
        let root = temporary_output_root();
        let first = activate_presentation_stage_run(&root, &[candidate(17)])
            .await
            .unwrap();
        let first_pointer: PresentationStagePointer =
            serde_json::from_slice(&tokio::fs::read(root.join("current.json")).await.unwrap())
                .unwrap();
        assert_eq!(first_pointer.run_id, first.run_id);
        assert!(stage_run_dir(&root, &first.run_id)
            .join("manifest.json")
            .is_file());

        let second = activate_presentation_stage_run(&root, &[candidate(18)])
            .await
            .unwrap();
        let second_pointer: PresentationStagePointer =
            serde_json::from_slice(&tokio::fs::read(root.join("current.json")).await.unwrap())
                .unwrap();
        assert_eq!(second_pointer.run_id, second.run_id);
        let backup = root
            .join("backups")
            .join(format!("current-before-{}.json", second.run_id));
        let backed_up: PresentationStagePointer =
            serde_json::from_slice(&tokio::fs::read(backup).await.unwrap()).unwrap();
        assert_eq!(backed_up.run_id, first.run_id);
        let _ = tokio::fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn invalid_candidate_cannot_create_or_replace_current_pointer() {
        let root = temporary_output_root();
        let mut invalid = candidate(17);
        invalid.staged.schema_version = 4;
        assert!(matches!(
            activate_presentation_stage_run(&root, &[invalid]).await,
            Err(PresentationStageError::InvalidCandidate { .. })
        ));
        assert!(!root.join("current.json").exists());
        assert!(!root.join("runs").exists());
    }
}
