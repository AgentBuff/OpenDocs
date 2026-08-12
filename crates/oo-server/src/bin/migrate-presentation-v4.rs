//! Offline v4 Presentation → v5 staging compiler.
//!
//! Usage: `cargo run -p oo-server --bin migrate-presentation-v4 -- <data-dir> --apply`.
//! It never changes `data-dir/blobs`: before P1-C those files are live v4 Artifact snapshots.
//! `--apply` creates a separately versioned, atomically activated staging run instead.

use std::sync::Arc;

use oo_schema::presentation_migration::{
    materialize_staged_presentation_v5, migrate_presentation_artifact_v4_to_v5,
};
use oo_server::presentation_migration::{
    activate_presentation_stage_run, PreparedPresentationStage,
};
use oo_server::store::{BlobStore, LocalFsStore};
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = Options::parse()?;
    let store = Arc::new(LocalFsStore::new(format!("{}/blobs", options.data_dir)).await?);
    let mut candidates = Vec::new();
    let mut skipped = 0usize;
    let mut failures = Vec::new();
    let mut loss_count = 0usize;
    let mut extension_count = 0usize;

    for key in store.list("").await? {
        if !key.ends_with(".json") {
            skipped += 1;
            continue;
        }
        let bytes = store.get(&key).await?;
        let raw: Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if !looks_like_v4_presentation(&raw) {
            skipped += 1;
            continue;
        }
        if let Some(artifact_id) = options.artifact_id.as_deref() {
            if raw.get("artifactId").and_then(Value::as_str) != Some(artifact_id) {
                skipped += 1;
                continue;
            }
        }
        match migrate_presentation_artifact_v4_to_v5(raw) {
            Ok(staged) => {
                loss_count += staged.report.losses.len();
                extension_count += staged.report.preserved_extensions.len();
                candidates.push(PreparedPresentationStage {
                    source_key: key,
                    staged,
                });
            }
            Err(error) => failures.push(format!("{key}: {error}")),
        }
    }

    if !failures.is_empty() {
        for failure in &failures {
            eprintln!("迁移失败：{failure}");
        }
        return Err(format!(
            "发现 {} 个无法迁移的 v4 Presentation；未写入 staging run",
            failures.len()
        )
        .into());
    }

    if !options.apply {
        println!(
            "演练完成：候选 {} 个、跳过 {} 个、Extension 保留 {} 个、明确 loss {} 项；传入 --apply 才会写入独立 staging run。",
            candidates.len(), skipped, extension_count, loss_count
        );
        return Ok(());
    }
    let manifest = activate_presentation_stage_run(&options.output_dir, &candidates).await?;
    if options.materialize {
        materialize_staged_candidates(&store, &options.data_dir, &candidates).await?;
    }
    println!(
        "staging run 已激活：{}（{} 个 Artifact、{} 个 Extension、{} 项 loss）。{}",
        manifest.run_id,
        manifest.artifacts.len(),
        extension_count,
        loss_count,
        if options.materialize {
            "已从该已审计 run 原子替换对应 online snapshots。"
        } else {
            "在线 blobs 未改动；传入 --materialize 才会从该已审计 run 写入。"
        },
    );
    Ok(())
}

fn looks_like_v4_presentation(raw: &Value) -> bool {
    raw.get("schemaVersion").and_then(Value::as_u64) == Some(4)
        && raw.get("kind").and_then(Value::as_str) == Some("presentation")
        && raw
            .get("payload")
            .and_then(Value::as_object)
            .and_then(|payload| payload.get("kind"))
            .and_then(Value::as_str)
            == Some("presentation")
}

/// Materialize only candidates just compiled into the active staging run.
/// Originals are backed up before writing; a failed write restores any
/// already-updated blobs, so this tool never creates an online dual-read path.
async fn materialize_staged_candidates(
    store: &Arc<LocalFsStore>,
    data_dir: &str,
    candidates: &[PreparedPresentationStage],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut originals = Vec::with_capacity(candidates.len());
    let backup_root = std::path::Path::new(data_dir).join("schema-backup");
    for candidate in candidates {
        let source = store.get(&candidate.source_key).await?;
        let backup_path = backup_root.join(&candidate.source_key);
        if let Some(parent) = backup_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&backup_path, &source).await?;
        originals.push((candidate.source_key.clone(), source));
    }

    for (index, candidate) in candidates.iter().enumerate() {
        let artifact = materialize_staged_presentation_v5(candidate.staged.clone())?;
        let bytes = serde_json::to_vec_pretty(&artifact)?;
        if let Err(error) = store.put(&candidate.source_key, &bytes).await {
            for (key, original) in originals.iter().take(index + 1) {
                let _ = store.put(key, original).await;
            }
            return Err(error.into());
        }
    }
    Ok(())
}

struct Options {
    data_dir: String,
    output_dir: String,
    artifact_id: Option<String>,
    apply: bool,
    materialize: bool,
}

impl Options {
    fn parse() -> Result<Self, Box<dyn std::error::Error>> {
        let mut data_dir = std::env::var("OO_DATA_DIR").unwrap_or_else(|_| "./data".into());
        let mut output_dir = None;
        let mut artifact_id = None;
        let mut apply = false;
        let mut materialize = false;
        let mut arguments = std::env::args().skip(1);
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--apply" => apply = true,
                "--materialize" => materialize = true,
                "--artifact-id" => {
                    artifact_id = Some(
                        arguments
                            .next()
                            .ok_or("--artifact-id 需要提供 artifact id")?,
                    );
                }
                "--output-dir" => {
                    output_dir = Some(arguments.next().ok_or("--output-dir 需要目录")?);
                }
                "--help" | "-h" => {
                    println!(
                        "用法：migrate-presentation-v4 [data-dir] [--artifact-id <id>] [--output-dir <dir>] --apply [--materialize]"
                    );
                    std::process::exit(0);
                }
                value if value.starts_with('-') => {
                    return Err(format!("未知参数：{value}").into());
                }
                value => data_dir = value.to_owned(),
            }
        }
        if materialize && !apply {
            return Err("--materialize 必须与 --apply 一起使用".into());
        }
        let output_dir = output_dir.unwrap_or_else(|| format!("{data_dir}/presentation-v5-stage"));
        Ok(Self {
            data_dir,
            output_dir,
            artifact_id,
            apply,
            materialize,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_only_v4_presentation_envelopes() {
        assert!(looks_like_v4_presentation(&serde_json::json!({
            "schemaVersion": 4,
            "kind": "presentation",
            "payload": { "kind": "presentation" }
        })));
        assert!(!looks_like_v4_presentation(&serde_json::json!({
            "schemaVersion": 5,
            "kind": "presentation",
            "payload": { "kind": "presentation" }
        })));
    }
}
