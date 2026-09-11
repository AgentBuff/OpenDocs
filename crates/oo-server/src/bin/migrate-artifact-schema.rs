//! Offline schema v1/v2 → current Artifact snapshot migration.
//!
//! Usage: `cargo run -p oo-server --bin migrate-artifact-schema -- <data-dir> --apply`
//! Without `--apply` the command only reports candidates. Before every write it copies the
//! original JSON outside BlobStore to `<data-dir>/schema-backup/`, then atomically replaces
//! the snapshot through LocalFsStore. The running API must be stopped while this tool executes.

use std::path::PathBuf;
use std::sync::Arc;

use oo_schema::{
    migrate_artifact_v1_to_v3, migrate_artifact_v2_to_v3, migrate_artifact_v3_to_v4,
    migrate_artifact_v4_to_v5, migrate_artifact_v5_to_v6, migrate_artifact_v6_to_v7,
    migrate_artifact_v7_to_v8,
};
use oo_server::store::{BlobStore, LocalFsStore};
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut data_dir = std::env::var("OO_DATA_DIR").unwrap_or_else(|_| "./data".into());
    let mut apply = false;
    let mut artifact_id = None;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--apply" {
            apply = true;
        } else if argument == "--artifact-id" {
            artifact_id = Some(
                arguments
                    .next()
                    .ok_or("--artifact-id 需要提供 artifact id")?,
            );
        } else if argument == "--help" || argument == "-h" {
            println!("用法：migrate-artifact-schema [data-dir] [--artifact-id <id>] --apply");
            return Ok(());
        } else {
            data_dir = argument;
        }
    }

    let store = Arc::new(LocalFsStore::new(format!("{data_dir}/blobs")).await?);
    let backup_root = PathBuf::from(&data_dir).join("schema-backup");
    let mut migrated = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    for key in store.list("").await? {
        if !key.ends_with(".json") {
            skipped += 1;
            continue;
        }
        if let Some(artifact_id) = artifact_id.as_deref() {
            let expected_prefix = format!("{artifact_id}/artifacts/");
            if !key.starts_with(&expected_prefix) {
                skipped += 1;
                continue;
            }
        }
        let bytes = store.get(&key).await?;
        let raw: Value = match serde_json::from_slice(&bytes) {
            Ok(raw) => raw,
            Err(error) => {
                eprintln!("跳过 {key}：不是 JSON Artifact（{error}）");
                skipped += 1;
                continue;
            }
        };
        let version = raw.get("schemaVersion").and_then(Value::as_u64);
        if !matches!(version, Some(1..=7)) {
            skipped += 1;
            continue;
        }
        let migrated_result = match version {
            Some(1) => migrate_artifact_v1_to_v3(raw),
            Some(2) => migrate_artifact_v2_to_v3(raw),
            Some(3) => migrate_artifact_v3_to_v4(raw),
            Some(4) => migrate_artifact_v4_to_v5(raw),
            Some(5) => migrate_artifact_v5_to_v6(raw),
            Some(6) => migrate_artifact_v6_to_v7(raw),
            Some(7) => migrate_artifact_v7_to_v8(raw),
            _ => unreachable!(),
        };
        let migrated_artifact = match migrated_result {
            Ok(artifact) => artifact,
            Err(error) => {
                eprintln!("迁移失败 {key}：{error}");
                failed += 1;
                continue;
            }
        };
        if apply {
            let backup_path = backup_root.join(&key);
            if let Some(parent) = backup_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&backup_path, &bytes).await?;
            store
                .put(&key, &serde_json::to_vec_pretty(&migrated_artifact)?)
                .await?;
        }
        migrated += 1;
    }

    if !apply {
        println!("检查完成：可迁移 {migrated} 个、跳过 {skipped} 个、失败 {failed} 个；传入 --apply 才会写入。");
    } else {
        println!("迁移完成：已迁移 {migrated} 个、跳过 {skipped} 个、失败 {failed} 个；原始文件位于 {}/schema-backup。", data_dir);
    }
    if failed > 0 {
        return Err("存在迁移失败的 snapshot，未完成切换".into());
    }
    Ok(())
}
