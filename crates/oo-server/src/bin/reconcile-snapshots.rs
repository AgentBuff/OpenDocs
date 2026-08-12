//! 离线登记已有 canonical Artifact snapshot。
//!
//! 迁移工具只会读取和登记经过 schema 校验、且版本不高于当前文档 revision 的对象，
//! 不会删除任何文件。完成备份并检查输出后，才可以设置 `OO_ENABLE_BLOB_GC=1` 启动服务。

use std::sync::Arc;

use chrono::Utc;
use oo_schema::{ArtifactEnvelope, ArtifactPayload};
use oo_server::db;
use oo_server::store::{BlobStore, LocalFsStore};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("OO_DATA_DIR").ok())
        .unwrap_or_else(|| "./data".into());
    let database_url = format!("sqlite://{data_dir}/open-office.db");
    let pool = db::connect(&database_url).await?;
    let store = Arc::new(LocalFsStore::new(format!("{data_dir}/blobs")).await?);
    let mut registered = 0usize;
    let mut skipped = 0usize;

    for key in store.list("").await? {
        let Some((document_id, version)) = parse_snapshot_key(&key) else {
            continue;
        };
        let Some(meta) = db::get_artifact(&pool, document_id).await? else {
            skipped += 1;
            continue;
        };
        if version > meta.version {
            // 未来版本通常是崩溃后未提交的对象，不能登记为历史。
            skipped += 1;
            continue;
        }
        let bytes = store.get(&key).await?;
        let artifact: ArtifactEnvelope = match serde_json::from_slice(&bytes) {
            Ok(artifact) => artifact,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if artifact.artifact_id != document_id
            || artifact.revision != u64::try_from(version).unwrap_or(0)
            || !matches!(artifact.payload, ArtifactPayload::Document(_))
            || artifact.validate().is_err()
        {
            skipped += 1;
            continue;
        }
        let created_at = store.modified_at(&key).await.unwrap_or_else(|_| Utc::now());
        if db::register_artifact_snapshot(&pool, document_id, version, &key, created_at).await? {
            registered += 1;
        }
    }

    println!("已登记 {registered} 个 snapshot，跳过 {skipped} 个未通过校验的对象");
    Ok(())
}

fn parse_snapshot_key(key: &str) -> Option<(&str, i64)> {
    let mut parts = key.split('/');
    let document_id = parts.next()?;
    if parts.next()? != "artifacts" {
        return None;
    }
    let filename = parts.next()?;
    if parts.next().is_some() || !filename.ends_with(".json") {
        return None;
    }
    let version = filename.strip_suffix(".json")?.parse().ok()?;
    (version > 0).then_some((document_id, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_canonical_snapshot_keys_are_parsed() {
        assert_eq!(parse_snapshot_key("doc/artifacts/3.json"), Some(("doc", 3)));
        assert_eq!(parse_snapshot_key("doc/source.docx"), None);
        assert_eq!(parse_snapshot_key("doc/artifacts/0.json"), None);
        assert_eq!(parse_snapshot_key("doc/artifacts/3.json/extra"), None);
    }
}
