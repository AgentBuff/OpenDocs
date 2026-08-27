//! SQLite 持久化：Artifact 元数据、快照指针、事务和事件投递状态。
//!
//! 正文不进数据库——原始文件和解析后的 Artifact snapshot 都放在 [`crate::store`] 里，
//! 数据库只存元数据和指向对象的键。换成 Postgres 时这里的 SQL 基本不用动。

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use oo_protocol::DomainEventRecord;
pub use oo_schema::ArtifactKind;
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

fn artifact_kind_str(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Document => "document",
        ArtifactKind::Spreadsheet => "spreadsheet",
        ArtifactKind::Presentation => "presentation",
        ArtifactKind::Mindmap => "mindmap",
        ArtifactKind::Whiteboard => "whiteboard",
    }
}

/// Artifact 元数据；正文和原始文件只通过 blob key 引用。
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactMeta {
    pub id: String,
    pub kind: ArtifactKind,
    pub title: String,
    pub owner_id: String,
    /// 原始文件字节数。
    pub size: i64,
    pub version: i64,
    /// 是否被收藏。
    pub starred: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Artifact engine 事务日志。它只记录可重放事务的提交摘要，不把整份 Artifact 快照塞进
/// 日志；正文仍由不可变 snapshot 提供，`transaction_id` 负责网络重试幂等。
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactTransactionRecord {
    pub artifact_id: String,
    pub transaction_id: String,
    pub author_id: String,
    pub base_version: i64,
    pub version: i64,
    pub changed_entities: Vec<String>,
    pub structure_changed: bool,
    pub origin: String,
    /// The semantic command batch is retained for audit/replay. It is never
    /// treated as a second mutable domain model.
    pub commands_json: String,
    pub created_at: DateTime<Utc>,
}

/// Logical history entry.  `before_snapshot_key` and `after_snapshot_key` are
/// private object-store keys; the HTTP API exposes only the resulting revision.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactHistoryRecord {
    pub history_id: i64,
    pub artifact_id: String,
    pub transaction_id: String,
    pub before_snapshot_key: String,
    pub after_snapshot_key: String,
    pub changed_entities: Vec<String>,
    pub structure_changed: bool,
    pub is_undone: bool,
    pub created_at: DateTime<Utc>,
}

/// 不可变 Artifact snapshot 的登记信息。对象键只在服务内部使用，HTTP 层不会暴露存储布局。
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactSnapshot {
    pub artifact_id: String,
    pub version: i64,
    pub snapshot_key: String,
    pub created_at: DateTime<Utc>,
}

/// Blob references for an Artifact. Keys are opaque to HTTP callers and are
/// only consumed by the server's object-store adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactBlobKeys {
    pub source_key: String,
    pub snapshot_key: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactAsset {
    pub artifact_id: String,
    pub asset_id: String,
    pub object_key: String,
    pub content_type: String,
    pub file_name: String,
    pub checksum: String,
    pub size: i64,
    pub ref_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BlobIntegrity {
    pub object_key: String,
    pub artifact_id: String,
    pub object_kind: String,
    pub checksum: String,
    pub size: i64,
    pub verified_at: DateTime<Utc>,
}

/// Durable post-commit event waiting for delivery.  The event payload is an
/// opaque protocol value: the database only owns delivery state and must not
/// interpret domain semantics.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactEventOutboxRecord {
    pub event_id: String,
    pub artifact_id: String,
    pub transaction_id: String,
    pub revision: i64,
    pub event: DomainEventRecord,
    pub status: String,
    pub attempts: i64,
    pub available_at: DateTime<Utc>,
    pub claimed_by: Option<String>,
    pub lease_until: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl ArtifactEventOutboxRecord {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        let event_id: String = row.try_get("event_id")?;
        let event_type_id: String = row.try_get("type_id")?;
        let payload_json: String = row.try_get("payload_json")?;
        let payload = serde_json::from_str(&payload_json)
            .map_err(|error| sqlx::Error::Protocol(format!("无效的领域事件 payload：{error}")))?;
        Ok(Self {
            event_id: event_id.clone(),
            artifact_id: row.try_get("artifact_id")?,
            transaction_id: row.try_get("transaction_id")?,
            revision: row.try_get("revision")?,
            event: DomainEventRecord {
                event_id,
                type_id: event_type_id,
                payload,
            },
            status: row.try_get("status")?,
            attempts: row.try_get("attempts")?,
            available_at: row.try_get("available_at")?,
            claimed_by: row.try_get("claimed_by")?,
            lease_until: row.try_get("lease_until")?,
            delivered_at: row.try_get("delivered_at")?,
            last_error: row.try_get("last_error")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

impl ArtifactTransactionRecord {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        let changed_entities_json: String = row.try_get("changed_entities_json")?;
        let changed_entities = serde_json::from_str(&changed_entities_json)
            .map_err(|error| sqlx::Error::Protocol(format!("无效的事务摘要 JSON：{error}")))?;
        Ok(Self {
            artifact_id: row.try_get("artifact_id")?,
            transaction_id: row.try_get("transaction_id")?,
            author_id: row.try_get("author_id")?,
            base_version: row.try_get("base_version")?,
            version: row.try_get("version")?,
            changed_entities,
            structure_changed: row.try_get::<i64, _>("structure_changed")? != 0,
            origin: row.try_get("origin")?,
            commands_json: row.try_get("commands_json")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

impl ArtifactHistoryRecord {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        let changed_entities_json: String = row.try_get("changed_entities_json")?;
        let changed_entities = serde_json::from_str(&changed_entities_json)
            .map_err(|error| sqlx::Error::Protocol(format!("无效的历史摘要 JSON：{error}")))?;
        Ok(Self {
            history_id: row.try_get("history_id")?,
            artifact_id: row.try_get("artifact_id")?,
            transaction_id: row.try_get("transaction_id")?,
            before_snapshot_key: row.try_get("before_snapshot_key")?,
            after_snapshot_key: row.try_get("after_snapshot_key")?,
            changed_entities,
            structure_changed: row.try_get::<i64, _>("structure_changed")? != 0,
            is_undone: row.try_get::<i64, _>("is_undone")? != 0,
            created_at: row.try_get("created_at")?,
        })
    }
}

impl ArtifactMeta {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        let kind = match row.try_get::<String, _>("kind")?.as_str() {
            "document" => ArtifactKind::Document,
            "spreadsheet" => ArtifactKind::Spreadsheet,
            "presentation" => ArtifactKind::Presentation,
            "mindmap" => ArtifactKind::Mindmap,
            "whiteboard" => ArtifactKind::Whiteboard,
            value => {
                return Err(sqlx::Error::Protocol(format!(
                    "未知 artifact kind：{value}"
                )))
            }
        };
        Ok(Self {
            id: row.try_get("id")?,
            kind,
            title: row.try_get("title")?,
            owner_id: row.try_get("owner_id")?,
            size: row.try_get("size")?,
            version: row.try_get("version")?,
            starred: row.try_get::<i64, _>("starred")? != 0,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

/// 建立连接池并运行版本化迁移。
///
/// `url` 形如 `sqlite://data/open-office.db?mode=rwc`，或 `sqlite::memory:`。
pub async fn connect(url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options: SqliteConnectOptions = url
        .parse::<SqliteConnectOptions>()?
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        // 内存库在连接关闭时就消失了，池子必须至少保住一条连接。
        .min_connections(1)
        .max_connections(8)
        .connect_with(options)
        .await?;

    migrate(&pool).await?;
    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|error| sqlx::Error::Protocol(format!("数据库迁移失败：{error}")))
}

/// 新建文档记录时需要的字段。
pub struct NewArtifact<'a> {
    pub id: &'a str,
    pub kind: ArtifactKind,
    pub title: &'a str,
    pub owner_id: &'a str,
    pub size: i64,
    pub source_key: &'a str,
    pub snapshot_key: &'a str,
    /// Lifecycle facts committed atomically with metadata and revision 1.
    pub events: &'a [DomainEventRecord],
}

pub async fn insert_artifact(
    pool: &SqlitePool,
    doc: NewArtifact<'_>,
) -> Result<ArtifactMeta, sqlx::Error> {
    let now = Utc::now();
    let mut transaction = pool.begin().await?;
    sqlx::query(
        r#"
        INSERT INTO artifacts (id, kind, title, owner_id, size, version, source_key, snapshot_key, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?, ?)
        "#,
    )
    .bind(doc.id)
    .bind(artifact_kind_str(doc.kind))
    .bind(doc.title)
    .bind(doc.owner_id)
    .bind(doc.size)
    .bind(doc.source_key)
    .bind(doc.snapshot_key)
    .bind(now)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO artifact_snapshots (artifact_id, version, snapshot_key, created_at) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(doc.id)
    .bind(1_i64)
    .bind(doc.snapshot_key)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    insert_domain_events(
        &mut transaction,
        doc.id,
        &format!("artifact:create:{}", doc.id),
        1,
        doc.events,
        now,
    )
    .await?;
    transaction.commit().await?;

    Ok(ArtifactMeta {
        id: doc.id.to_string(),
        kind: doc.kind,
        title: doc.title.to_string(),
        owner_id: doc.owner_id.to_string(),
        size: doc.size,
        version: 1,
        starred: false,
        created_at: now,
        updated_at: now,
    })
}

/// 设置或取消收藏。
pub async fn set_artifact_starred(
    pool: &SqlitePool,
    id: &str,
    starred: bool,
) -> Result<ArtifactMeta, sqlx::Error> {
    sqlx::query("UPDATE artifacts SET starred = ? WHERE id = ?")
        .bind(i64::from(starred))
        .bind(id)
        .execute(pool)
        .await?;
    get_artifact(pool, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

/// 重命名。
pub async fn rename_artifact(
    pool: &SqlitePool,
    id: &str,
    title: &str,
) -> Result<ArtifactMeta, sqlx::Error> {
    sqlx::query("UPDATE artifacts SET title = ?, updated_at = ? WHERE id = ?")
        .bind(title)
        .bind(Utc::now())
        .bind(id)
        .execute(pool)
        .await?;
    get_artifact(pool, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

/// 列出某个用户的全部文档，最近更新的排在前面。
pub async fn list_artifacts(
    pool: &SqlitePool,
    owner_id: &str,
) -> Result<Vec<ArtifactMeta>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, kind, title, owner_id, size, version, starred, created_at, updated_at
         FROM artifacts WHERE owner_id = ? ORDER BY updated_at DESC",
    )
    .bind(owner_id)
    .fetch_all(pool)
    .await?;

    rows.iter().map(ArtifactMeta::from_row).collect()
}

pub async fn get_artifact(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<ArtifactMeta>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, kind, title, owner_id, size, version, starred, created_at, updated_at
         FROM artifacts WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    row.as_ref().map(ArtifactMeta::from_row).transpose()
}

/// 取出文档原件与最新 Artifact snapshot 对应的对象键。
pub async fn get_artifact_blob_keys(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<ArtifactBlobKeys>, sqlx::Error> {
    let row = sqlx::query("SELECT source_key, snapshot_key FROM artifacts WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    row.map(|r| {
        Ok(ArtifactBlobKeys {
            source_key: r.try_get("source_key")?,
            snapshot_key: r.try_get("snapshot_key")?,
        })
    })
    .transpose()
}

pub async fn register_blob_integrity(
    pool: &SqlitePool,
    artifact_id: &str,
    object_key: &str,
    object_kind: &str,
    checksum: &str,
    size: i64,
) -> Result<(), sqlx::Error> {
    if !matches!(object_kind, "source" | "snapshot" | "asset") {
        return Err(sqlx::Error::Protocol("未知 Blob kind".into()));
    }
    if size < 0 || checksum.len() != 64 {
        return Err(sqlx::Error::Protocol("Blob checksum/size 无效".into()));
    }
    sqlx::query(
        "INSERT INTO artifact_blob_integrity (object_key, artifact_id, object_kind, checksum, size, verified_at) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(object_key) DO UPDATE SET checksum = excluded.checksum, size = excluded.size, verified_at = excluded.verified_at",
    )
    .bind(object_key)
    .bind(artifact_id)
    .bind(object_kind)
    .bind(checksum)
    .bind(size)
    .bind(Utc::now())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_blob_integrity(
    pool: &SqlitePool,
    object_key: &str,
) -> Result<Option<BlobIntegrity>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT object_key, artifact_id, object_kind, checksum, size, verified_at \
         FROM artifact_blob_integrity WHERE object_key = ?",
    )
    .bind(object_key)
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        Ok(BlobIntegrity {
            object_key: row.try_get("object_key")?,
            artifact_id: row.try_get("artifact_id")?,
            object_kind: row.try_get("object_kind")?,
            checksum: row.try_get("checksum")?,
            size: row.try_get("size")?,
            verified_at: row.try_get("verified_at")?,
        })
    })
    .transpose()
}

pub async fn insert_artifact_asset(
    pool: &SqlitePool,
    asset: &ArtifactAsset,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO artifact_assets \
         (artifact_id, asset_id, object_key, content_type, file_name, checksum, size, ref_count, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&asset.artifact_id)
    .bind(&asset.asset_id)
    .bind(&asset.object_key)
    .bind(&asset.content_type)
    .bind(&asset.file_name)
    .bind(&asset.checksum)
    .bind(asset.size)
    .bind(asset.ref_count)
    .bind(asset.created_at)
    .bind(asset.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

fn asset_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ArtifactAsset, sqlx::Error> {
    Ok(ArtifactAsset {
        artifact_id: row.try_get("artifact_id")?,
        asset_id: row.try_get("asset_id")?,
        object_key: row.try_get("object_key")?,
        content_type: row.try_get("content_type")?,
        file_name: row.try_get("file_name")?,
        checksum: row.try_get("checksum")?,
        size: row.try_get("size")?,
        ref_count: row.try_get("ref_count")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub async fn list_artifact_assets(
    pool: &SqlitePool,
    artifact_id: &str,
) -> Result<Vec<ArtifactAsset>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT artifact_id, asset_id, object_key, content_type, file_name, checksum, size, ref_count, created_at, updated_at \
         FROM artifact_assets WHERE artifact_id = ? ORDER BY created_at, asset_id",
    )
    .bind(artifact_id)
    .fetch_all(pool)
    .await?;
    rows.iter().map(asset_from_row).collect()
}

pub async fn get_artifact_asset(
    pool: &SqlitePool,
    artifact_id: &str,
    asset_id: &str,
) -> Result<Option<ArtifactAsset>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT artifact_id, asset_id, object_key, content_type, file_name, checksum, size, ref_count, created_at, updated_at \
         FROM artifact_assets WHERE artifact_id = ? AND asset_id = ?",
    )
    .bind(artifact_id)
    .bind(asset_id)
    .fetch_optional(pool)
    .await?;
    row.as_ref().map(asset_from_row).transpose()
}

pub async fn set_artifact_asset_references(
    pool: &SqlitePool,
    artifact_id: &str,
    referenced_asset_ids: &[String],
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    set_asset_references_in_transaction(&mut transaction, artifact_id, referenced_asset_ids)
        .await?;
    transaction.commit().await
}

/// Rebuild the asset reference counts while the caller's transaction is open.
///
/// A missing asset id is rejected instead of silently producing a snapshot that
/// cannot be deleted or garbage-collected correctly.  The caller's transaction
/// then rolls back both the document pointer and the reference update.
async fn set_asset_references_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    artifact_id: &str,
    referenced_asset_ids: &[String],
) -> Result<(), sqlx::Error> {
    let now = Utc::now();
    sqlx::query("UPDATE artifact_assets SET ref_count = 0, updated_at = ? WHERE artifact_id = ?")
        .bind(now)
        .bind(artifact_id)
        .execute(&mut **transaction)
        .await?;
    for asset_id in referenced_asset_ids {
        let result = sqlx::query(
            "UPDATE artifact_assets SET ref_count = ref_count + 1, updated_at = ? \
             WHERE artifact_id = ? AND asset_id = ?",
        )
        .bind(now)
        .bind(artifact_id)
        .bind(asset_id)
        .execute(&mut **transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(sqlx::Error::Protocol(format!(
                "artifact {artifact_id} 引用了不存在的 asset {asset_id}"
            )));
        }
    }
    Ok(())
}

pub async fn delete_artifact_asset(
    pool: &SqlitePool,
    artifact_id: &str,
    asset_id: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM artifact_assets WHERE artifact_id = ? AND asset_id = ? AND ref_count = 0",
    )
    .bind(artifact_id)
    .bind(asset_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// 返回文档的全部 snapshot 对象键，删除文档时一并回收历史版本。
pub async fn list_artifact_snapshot_keys(
    pool: &SqlitePool,
    id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT snapshot_key FROM artifact_snapshots WHERE artifact_id = ? ORDER BY version",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    rows.iter().map(|row| row.try_get("snapshot_key")).collect()
}

/// 返回文档的全部 snapshot，最新版本排在前面。
pub async fn list_artifact_snapshots(
    pool: &SqlitePool,
    id: &str,
) -> Result<Vec<ArtifactSnapshot>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT artifact_id, version, snapshot_key, created_at \
         FROM artifact_snapshots WHERE artifact_id = ? ORDER BY version DESC",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(ArtifactSnapshot {
                artifact_id: row.try_get("artifact_id")?,
                version: row.try_get("version")?,
                snapshot_key: row.try_get("snapshot_key")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect()
}

/// 通过文档 id 和版本定位不可变 snapshot；不存在时不泄露存储路径。
pub async fn get_artifact_snapshot(
    pool: &SqlitePool,
    id: &str,
    version: i64,
) -> Result<Option<ArtifactSnapshot>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT artifact_id, version, snapshot_key, created_at \
         FROM artifact_snapshots WHERE artifact_id = ? AND version = ?",
    )
    .bind(id)
    .bind(version)
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        Ok(ArtifactSnapshot {
            artifact_id: row.try_get("artifact_id")?,
            version: row.try_get("version")?,
            snapshot_key: row.try_get("snapshot_key")?,
            created_at: row.try_get("created_at")?,
        })
    })
    .transpose()
}

/// 离线登记一个已经存在且经过上层校验的 snapshot。重复执行安全，不能覆盖已有版本指针。
pub async fn register_artifact_snapshot(
    pool: &SqlitePool,
    artifact_id: &str,
    version: i64,
    snapshot_key: &str,
    created_at: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO artifact_snapshots \
         (artifact_id, version, snapshot_key, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(artifact_id)
    .bind(version)
    .bind(snapshot_key)
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// 返回数据库仍然引用的所有对象键，用于进程启动时清理崩溃留下的孤儿 blob。
pub async fn referenced_blob_keys(pool: &SqlitePool) -> Result<HashSet<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT source_key AS object_key FROM artifacts \
         UNION SELECT snapshot_key AS object_key FROM artifacts \
         UNION SELECT snapshot_key AS object_key FROM artifact_snapshots \
         UNION SELECT object_key AS object_key FROM artifact_assets \
         UNION SELECT object_key AS object_key FROM artifact_blob_integrity",
    )
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| row.try_get::<String, _>("object_key"))
        .collect()
}

/// 读取 Artifact engine 事务日志，用于网络重试幂等。
pub async fn get_artifact_transaction(
    pool: &SqlitePool,
    artifact_id: &str,
    transaction_id: &str,
) -> Result<Option<ArtifactTransactionRecord>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT artifact_id, transaction_id, author_id, base_version, version, changed_entities_json, \
                structure_changed, origin, commands_json, created_at \
         FROM artifact_transactions WHERE artifact_id = ? AND transaction_id = ?",
    )
    .bind(artifact_id)
    .bind(transaction_id)
    .fetch_optional(pool)
    .await?;
    row.as_ref()
        .map(ArtifactTransactionRecord::from_row)
        .transpose()
}

/// Returns the next logical history entry without exposing object-store keys to
/// callers outside the server.  Undo consumes the newest active entry; redo
/// reapplies the oldest entry in the currently undone suffix.
pub async fn get_artifact_history(
    pool: &SqlitePool,
    artifact_id: &str,
    undone: bool,
) -> Result<Option<ArtifactHistoryRecord>, sqlx::Error> {
    let order = if undone { "ASC" } else { "DESC" };
    let query = format!(
        "SELECT history_id, artifact_id, transaction_id, before_snapshot_key, after_snapshot_key, \
                changed_entities_json, structure_changed, is_undone, created_at \
         FROM artifact_history WHERE artifact_id = ? AND is_undone = ? \
         ORDER BY history_id {order} LIMIT 1"
    );
    let row = sqlx::query(&query)
        .bind(artifact_id)
        .bind(i64::from(undone))
        .fetch_optional(pool)
        .await?;
    row.as_ref()
        .map(ArtifactHistoryRecord::from_row)
        .transpose()
}

/// Return the authoritative history affordances for the current Artifact
/// pointer.  The browser must not infer these flags from an optimistic local
/// stack because a refresh or another writer may have changed the branch.
pub async fn artifact_history_state(
    pool: &SqlitePool,
    artifact_id: &str,
) -> Result<(bool, bool), sqlx::Error> {
    let can_undo: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM artifact_history WHERE artifact_id = ? AND is_undone = 0)",
    )
    .bind(artifact_id)
    .fetch_one(pool)
    .await?;
    let can_redo: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM artifact_history WHERE artifact_id = ? AND is_undone = 1)",
    )
    .bind(artifact_id)
    .fetch_one(pool)
    .await?;
    Ok((can_undo != 0, can_redo != 0))
}

/// Insert event facts while the caller's artifact transaction is still open.
/// Any insertion error aborts the transaction, so a committed Artifact can
/// never be observed without its corresponding outbox rows.
async fn insert_domain_events(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    artifact_id: &str,
    transaction_id: &str,
    revision: i64,
    events: &[DomainEventRecord],
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    for event in events {
        let payload_json = serde_json::to_string(&event.payload)
            .map_err(|error| sqlx::Error::Protocol(format!("领域事件序列化失败：{error}")))?;
        sqlx::query(
            "INSERT INTO artifact_event_outbox \
             (event_id, artifact_id, transaction_id, revision, type_id, payload_json, \
              status, attempts, available_at, delivered_at, last_error, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, 'pending', 0, ?, NULL, NULL, ?)",
        )
        .bind(&event.event_id)
        .bind(artifact_id)
        .bind(transaction_id)
        .bind(revision)
        .bind(&event.type_id)
        .bind(payload_json)
        .bind(now)
        .bind(now)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

/// Return unclaimed pending events in deterministic creation order.
///
/// This is an operational read used by diagnostics. Delivery workers must use
/// [`claim_pending_domain_events`] so that two consumers cannot process the
/// same event concurrently.
pub async fn list_pending_domain_events(
    pool: &SqlitePool,
    limit: u32,
) -> Result<Vec<ArtifactEventOutboxRecord>, sqlx::Error> {
    let limit = i64::from(limit.clamp(1, 1_000));
    let rows = sqlx::query(
        "SELECT event_id, artifact_id, transaction_id, revision, type_id, payload_json, \
                status, attempts, available_at, claimed_by, lease_until, delivered_at, last_error, created_at \
         FROM artifact_event_outbox \
         WHERE status = 'pending' AND available_at <= ? \
         ORDER BY created_at ASC, event_id ASC LIMIT ?",
    )
    .bind(Utc::now())
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(ArtifactEventOutboxRecord::from_row)
        .collect()
}

/// Read committed domain facts for one Artifact without exposing outbox
/// delivery state. The cursor is the `(revision, event_id)` tuple so events
/// sharing one revision remain deterministic and replayable.
pub async fn list_artifact_events(
    pool: &SqlitePool,
    artifact_id: &str,
    since_revision: i64,
    cursor: Option<(i64, &str)>,
    limit: u32,
) -> Result<Vec<ArtifactEventOutboxRecord>, sqlx::Error> {
    let limit = i64::from(limit.clamp(1, 1_001));
    let (cursor_revision, cursor_event_id) = cursor.unwrap_or((since_revision, ""));
    let rows = sqlx::query(
        "SELECT event_id, artifact_id, transaction_id, revision, type_id, payload_json, \
                status, attempts, available_at, claimed_by, lease_until, delivered_at, last_error, created_at \
         FROM artifact_event_outbox \
         WHERE artifact_id = ? AND (revision > ? OR (revision = ? AND event_id > ?)) \
         ORDER BY revision ASC, event_id ASC LIMIT ?",
    )
    .bind(artifact_id)
    .bind(cursor_revision)
    .bind(cursor_revision)
    .bind(cursor_event_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(ArtifactEventOutboxRecord::from_row)
        .collect()
}

/// Atomically claim available events for one consumer.
///
/// The update uses SQLite's `RETURNING` clause, so selection and ownership
/// assignment happen as one write. Expired processing leases are first made
/// pending in the same transaction; a crashed consumer therefore cannot strand
/// an event. `attempts` counts claims, not failed callbacks.
pub async fn claim_pending_domain_events(
    pool: &SqlitePool,
    consumer_id: &str,
    limit: u32,
    lease: chrono::Duration,
) -> Result<Vec<ArtifactEventOutboxRecord>, sqlx::Error> {
    if consumer_id.trim().is_empty() {
        return Err(sqlx::Error::Protocol("事件消费者 id 不能为空".into()));
    }
    if lease <= chrono::Duration::zero() {
        return Err(sqlx::Error::Protocol("事件租约必须为正数".into()));
    }
    let limit = i64::from(limit.clamp(1, 1_000));
    let now = Utc::now();
    let lease_until = now + lease;
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "UPDATE artifact_event_outbox SET status = 'pending', claimed_by = NULL, lease_until = NULL \
         WHERE status = 'processing' AND lease_until IS NOT NULL AND lease_until <= ?",
    )
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    let rows = sqlx::query(
        "UPDATE artifact_event_outbox SET status = 'processing', claimed_by = ?, \
                lease_until = ?, attempts = attempts + 1 \
         WHERE event_id IN ( \
             SELECT event_id FROM artifact_event_outbox \
             WHERE status = 'pending' AND available_at <= ? \
             ORDER BY created_at ASC, event_id ASC LIMIT ? \
         ) \
         RETURNING event_id, artifact_id, transaction_id, revision, type_id, payload_json, \
                   status, attempts, available_at, claimed_by, lease_until, delivered_at, last_error, created_at",
    )
    .bind(consumer_id)
    .bind(lease_until)
    .bind(now)
    .bind(limit)
    .fetch_all(&mut *transaction)
    .await?;
    transaction.commit().await?;
    rows.iter()
        .map(ArtifactEventOutboxRecord::from_row)
        .collect()
}

/// Acknowledge a claimed event after its external side effect succeeds.
/// Ownership is checked so a late worker cannot acknowledge a lease it no
/// longer holds. Repeating an acknowledgement is harmless and returns false.
pub async fn ack_domain_event(
    pool: &SqlitePool,
    event_id: &str,
    consumer_id: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE artifact_event_outbox SET status = 'delivered', delivered_at = ?, \
                claimed_by = NULL, lease_until = NULL, last_error = NULL \
         WHERE event_id = ? AND status = 'processing' AND claimed_by = ?",
    )
    .bind(Utc::now())
    .bind(event_id)
    .bind(consumer_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Return a claimed event to pending with an explicit retry time.
///
/// Backoff policy belongs to the worker; this storage primitive only records
/// the next availability and the failure detail. Ownership is required to
/// prevent an expired worker from rescheduling a newer claim.
pub async fn fail_domain_event(
    pool: &SqlitePool,
    event_id: &str,
    consumer_id: &str,
    retry_at: DateTime<Utc>,
    error: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE artifact_event_outbox SET status = 'pending', available_at = ?, \
                claimed_by = NULL, lease_until = NULL, last_error = ? \
         WHERE event_id = ? AND status = 'processing' AND claimed_by = ?",
    )
    .bind(retry_at)
    .bind(error)
    .bind(event_id)
    .bind(consumer_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Number of events still awaiting delivery.  This is intentionally a cheap
/// operational metric and does not expose payloads to the HTTP API.
pub async fn pending_domain_event_count(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM artifact_event_outbox WHERE status != 'delivered'")
        .fetch_one(pool)
        .await
}

/// 原子提交 Artifact engine 的正文指针和事务摘要。
pub struct ArtifactTransactionCommit<'a> {
    pub id: &'a str,
    pub expected_version: i64,
    pub snapshot_key: &'a str,
    pub transaction_id: &'a str,
    pub author_id: &'a str,
    pub changed_entities: &'a [String],
    pub structure_changed: bool,
    pub origin: &'a str,
    pub commands_json: &'a str,
    /// The previous current snapshot, retained only in the private history log.
    pub before_snapshot_key: &'a str,
    /// Domain facts persisted in the same SQLite transaction as the snapshot.
    pub events: &'a [DomainEventRecord],
}

pub async fn commit_artifact_transaction(
    pool: &SqlitePool,
    commit: ArtifactTransactionCommit<'_>,
) -> Result<Option<ArtifactMeta>, sqlx::Error> {
    commit_artifact_transaction_internal(pool, commit, None).await
}

/// Commit a document snapshot and atomically rebuild its asset references.
pub async fn commit_artifact_transaction_with_assets(
    pool: &SqlitePool,
    commit: ArtifactTransactionCommit<'_>,
    referenced_asset_ids: &[String],
) -> Result<Option<ArtifactMeta>, sqlx::Error> {
    commit_artifact_transaction_internal(pool, commit, Some(referenced_asset_ids)).await
}

async fn commit_artifact_transaction_internal(
    pool: &SqlitePool,
    commit: ArtifactTransactionCommit<'_>,
    referenced_asset_ids: Option<&[String]>,
) -> Result<Option<ArtifactMeta>, sqlx::Error> {
    let changed_entities_json = serde_json::to_string(commit.changed_entities)
        .map_err(|error| sqlx::Error::Protocol(format!("事务摘要序列化失败：{error}")))?;
    let next_version = commit
        .expected_version
        .checked_add(1)
        .ok_or_else(|| sqlx::Error::Protocol("文档版本溢出".into()))?;
    let now = Utc::now();
    let mut transaction = pool.begin().await?;
    let result = sqlx::query(
        "UPDATE artifacts SET snapshot_key = ?, version = ?, updated_at = ? \
         WHERE id = ? AND version = ?",
    )
    .bind(commit.snapshot_key)
    .bind(next_version)
    .bind(now)
    .bind(commit.id)
    .bind(commit.expected_version)
    .execute(&mut *transaction)
    .await?;
    if result.rows_affected() == 0 {
        transaction.rollback().await?;
        return Ok(None);
    }
    sqlx::query(
        "INSERT INTO artifact_snapshots (artifact_id, version, snapshot_key, created_at) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(commit.id)
    .bind(next_version)
    .bind(commit.snapshot_key)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO artifact_transactions \
         (artifact_id, transaction_id, author_id, base_version, version, changed_entities_json, structure_changed, origin, commands_json, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(commit.id)
    .bind(commit.transaction_id)
    .bind(commit.author_id)
    .bind(commit.expected_version)
    .bind(next_version)
    .bind(changed_entities_json.as_str())
    .bind(i64::from(commit.structure_changed))
    .bind(commit.origin)
    .bind(commit.commands_json)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    // A normal commit starts a new branch and makes redo entries unreachable.
    // Their immutable snapshots remain available through the snapshot registry.
    sqlx::query("DELETE FROM artifact_history WHERE artifact_id = ? AND is_undone = 1")
        .bind(commit.id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "INSERT INTO artifact_history \
         (artifact_id, transaction_id, before_snapshot_key, after_snapshot_key, changed_entities_json, structure_changed, is_undone, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, 0, ?)",
    )
    .bind(commit.id)
    .bind(commit.transaction_id)
    .bind(commit.before_snapshot_key)
    .bind(commit.snapshot_key)
    .bind(changed_entities_json.as_str())
    .bind(i64::from(commit.structure_changed))
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    if let Some(asset_ids) = referenced_asset_ids {
        set_asset_references_in_transaction(&mut transaction, commit.id, asset_ids).await?;
    }
    insert_domain_events(
        &mut transaction,
        commit.id,
        commit.transaction_id,
        next_version,
        commit.events,
        now,
    )
    .await?;
    transaction.commit().await?;
    get_artifact(pool, commit.id).await
}

/// Commit a server-authoritative undo/redo transition.  The target history row
/// is moved between active and undone states in the same SQLite transaction as
/// the Artifact pointer and transaction idempotency record.
pub struct ArtifactHistoryCommit<'a> {
    pub id: &'a str,
    pub expected_version: i64,
    pub snapshot_key: &'a str,
    pub transaction_id: &'a str,
    pub author_id: &'a str,
    pub changed_entities: &'a [String],
    pub structure_changed: bool,
    pub origin: &'a str,
    pub commands_json: &'a str,
    pub history_id: i64,
    pub expected_undone: bool,
    pub next_undone: bool,
    /// Domain facts persisted atomically with the history transition.
    pub events: &'a [DomainEventRecord],
}

pub async fn commit_artifact_history(
    pool: &SqlitePool,
    commit: ArtifactHistoryCommit<'_>,
) -> Result<Option<ArtifactMeta>, sqlx::Error> {
    commit_artifact_history_internal(pool, commit, None).await
}

/// Commit an undo/redo snapshot and atomically rebuild its asset references.
pub async fn commit_artifact_history_with_assets(
    pool: &SqlitePool,
    commit: ArtifactHistoryCommit<'_>,
    referenced_asset_ids: &[String],
) -> Result<Option<ArtifactMeta>, sqlx::Error> {
    commit_artifact_history_internal(pool, commit, Some(referenced_asset_ids)).await
}

async fn commit_artifact_history_internal(
    pool: &SqlitePool,
    commit: ArtifactHistoryCommit<'_>,
    referenced_asset_ids: Option<&[String]>,
) -> Result<Option<ArtifactMeta>, sqlx::Error> {
    let changed_entities_json = serde_json::to_string(commit.changed_entities)
        .map_err(|error| sqlx::Error::Protocol(format!("事务摘要序列化失败：{error}")))?;
    let next_version = commit
        .expected_version
        .checked_add(1)
        .ok_or_else(|| sqlx::Error::Protocol("文档版本溢出".into()))?;
    let now = Utc::now();
    let mut transaction = pool.begin().await?;
    let result = sqlx::query(
        "UPDATE artifacts SET snapshot_key = ?, version = ?, updated_at = ? \
         WHERE id = ? AND version = ?",
    )
    .bind(commit.snapshot_key)
    .bind(next_version)
    .bind(now)
    .bind(commit.id)
    .bind(commit.expected_version)
    .execute(&mut *transaction)
    .await?;
    if result.rows_affected() == 0 {
        transaction.rollback().await?;
        return Ok(None);
    }
    sqlx::query(
        "INSERT INTO artifact_snapshots (artifact_id, version, snapshot_key, created_at) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(commit.id)
    .bind(next_version)
    .bind(commit.snapshot_key)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO artifact_transactions \
         (artifact_id, transaction_id, author_id, base_version, version, changed_entities_json, structure_changed, origin, commands_json, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(commit.id)
    .bind(commit.transaction_id)
    .bind(commit.author_id)
    .bind(commit.expected_version)
    .bind(next_version)
    .bind(changed_entities_json.as_str())
    .bind(i64::from(commit.structure_changed))
    .bind(commit.origin)
    .bind(commit.commands_json)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    let history_update = sqlx::query(
        "UPDATE artifact_history SET is_undone = ? \
         WHERE history_id = ? AND artifact_id = ? AND is_undone = ?",
    )
    .bind(i64::from(commit.next_undone))
    .bind(commit.history_id)
    .bind(commit.id)
    .bind(i64::from(commit.expected_undone))
    .execute(&mut *transaction)
    .await?;
    if history_update.rows_affected() == 0 {
        transaction.rollback().await?;
        return Ok(None);
    }
    if let Some(asset_ids) = referenced_asset_ids {
        set_asset_references_in_transaction(&mut transaction, commit.id, asset_ids).await?;
    }
    insert_domain_events(
        &mut transaction,
        commit.id,
        commit.transaction_id,
        next_version,
        commit.events,
        now,
    )
    .await?;
    transaction.commit().await?;
    get_artifact(pool, commit.id).await
}

/// 删除记录，返回是否确实删掉了一行。
pub async fn delete_artifact(pool: &SqlitePool, id: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM artifacts WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_event(event_id: &str) -> DomainEventRecord {
        DomainEventRecord {
            event_id: event_id.into(),
            type_id: "document.blockUpdated".into(),
            payload: serde_json::json!({"blockId": "b1"}),
        }
    }

    async fn pool() -> SqlitePool {
        connect("sqlite::memory:").await.unwrap()
    }

    fn new_doc<'a>(id: &'a str, title: &'a str) -> NewArtifact<'a> {
        NewArtifact {
            id,
            kind: ArtifactKind::Document,
            title,
            owner_id: "dev-user",
            size: 42,
            source_key: id,
            snapshot_key: id,
            events: &[],
        }
    }

    #[tokio::test]
    async fn insert_then_get_returns_the_same_metadata() {
        let pool = pool().await;
        let inserted = insert_artifact(&pool, new_doc("d1", "标题")).await.unwrap();
        let fetched = get_artifact(&pool, "d1").await.unwrap().unwrap();
        assert_eq!(inserted.id, fetched.id);
        assert_eq!(fetched.kind, ArtifactKind::Document);
        assert_eq!(fetched.title, "标题");
        assert_eq!(fetched.version, 1);
    }

    #[tokio::test]
    async fn canonical_metadata_uses_artifact_tables_without_document_aliases() {
        let pool = pool().await;
        let tables: Vec<String> = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE '_sqlx_%'",
        )
        .fetch_all(&pool)
        .await
        .unwrap()
        .iter()
        .map(|row| row.try_get("name").unwrap())
        .collect();
        assert!(tables.iter().any(|name| name == "artifacts"));
        assert!(tables.iter().any(|name| name == "artifact_snapshots"));
        assert!(tables.iter().any(|name| name == "artifact_transactions"));
        assert!(tables.iter().any(|name| name == "artifact_history"));
        assert!(tables.iter().any(|name| name == "artifact_event_outbox"));
        assert!(!tables.iter().any(|name| name == "documents"));
        assert!(!tables.iter().any(|name| name == "document_transactions"));
    }

    #[tokio::test]
    async fn list_is_scoped_to_owner_and_sorted_by_recency() {
        let pool = pool().await;
        insert_artifact(&pool, new_doc("d1", "先")).await.unwrap();
        insert_artifact(&pool, new_doc("d2", "后")).await.unwrap();
        insert_artifact(
            &pool,
            NewArtifact {
                owner_id: "someone-else",
                ..new_doc("d3", "别人的")
            },
        )
        .await
        .unwrap();

        let list = list_artifacts(&pool, "dev-user").await.unwrap();
        assert_eq!(list.len(), 2, "不应看到其他用户的文档");
        assert!(list.iter().all(|d| d.owner_id == "dev-user"));
    }

    #[tokio::test]
    async fn missing_document_returns_none() {
        let pool = pool().await;
        assert!(get_artifact(&pool, "nope").await.unwrap().is_none());
        assert!(get_artifact_blob_keys(&pool, "nope")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn delete_reports_whether_a_row_was_removed() {
        let pool = pool().await;
        insert_artifact(&pool, new_doc("d1", "x")).await.unwrap();
        assert!(delete_artifact(&pool, "d1").await.unwrap());
        assert!(
            !delete_artifact(&pool, "d1").await.unwrap(),
            "重复删除应返回 false"
        );
    }

    #[tokio::test]
    async fn transaction_foreign_key_is_enforced_and_cascades_on_delete() {
        let pool = pool().await;
        let missing = sqlx::query(
            "INSERT INTO artifact_transactions \
             (artifact_id, transaction_id, author_id, base_version, version, changed_entities_json, structure_changed, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("missing")
        .bind("tx-1")
        .bind("dev-user")
        .bind(0_i64)
        .bind(1_i64)
        .bind("[]")
        .bind(0_i64)
        .bind(Utc::now())
        .execute(&pool)
        .await;
        assert!(missing.is_err(), "事务不能引用不存在的文档");

        insert_artifact(&pool, new_doc("d1", "x")).await.unwrap();
        sqlx::query(
            "INSERT INTO artifact_transactions \
             (artifact_id, transaction_id, author_id, base_version, version, changed_entities_json, structure_changed, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("d1")
        .bind("tx-1")
        .bind("dev-user")
        .bind(1_i64)
        .bind(2_i64)
        .bind("[]")
        .bind(0_i64)
        .bind(Utc::now())
        .execute(&pool)
        .await
        .unwrap();
        delete_artifact(&pool, "d1").await.unwrap();
        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM artifact_transactions WHERE artifact_id = ?")
                .bind("d1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(remaining, 0, "删除文档应级联清理事务记录");
    }

    #[tokio::test]
    async fn command_journal_uses_only_the_canonical_commands_column() {
        let pool = pool().await;
        insert_artifact(&pool, new_doc("d1", "x")).await.unwrap();
        let command_json =
            r#"[{"commandId":"cmd-1","typeId":"document.replaceBlockText","payload":{}}]"#;
        sqlx::query(
            "INSERT INTO artifact_transactions \
             (artifact_id, transaction_id, author_id, base_version, version, changed_entities_json, structure_changed, origin, commands_json, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("d1")
        .bind("tx-commands")
        .bind("dev-user")
        .bind(1_i64)
        .bind(2_i64)
        .bind("[]")
        .bind(0_i64)
        .bind("local")
        .bind(command_json)
        .bind(Utc::now())
        .execute(&pool)
        .await
        .unwrap();

        let columns: Vec<String> = sqlx::query("PRAGMA table_info(artifact_transactions)")
            .fetch_all(&pool)
            .await
            .unwrap()
            .iter()
            .map(|row| row.try_get("name").unwrap())
            .collect();
        assert!(columns.iter().any(|name| name == "commands_json"));
        let removed_column = ["operations", "json"].join("_");
        assert!(!columns.iter().any(|name| name == &removed_column));

        let transaction = get_artifact_transaction(&pool, "d1", "tx-commands")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(transaction.artifact_id, "d1");
        assert_eq!(transaction.commands_json, command_json);
    }

    #[tokio::test]
    async fn snapshot_registry_tracks_versions_and_referenced_objects() {
        let pool = pool().await;
        insert_artifact(&pool, new_doc("d1", "x")).await.unwrap();
        assert_eq!(
            list_artifact_snapshot_keys(&pool, "d1").await.unwrap(),
            vec!["d1"]
        );
        let first_refs = referenced_blob_keys(&pool).await.unwrap();
        assert!(first_refs.contains("d1"));

        let changed_entities = Vec::new();
        commit_artifact_transaction(
            &pool,
            ArtifactTransactionCommit {
                id: "d1",
                expected_version: 1,
                snapshot_key: "d1-v2",
                transaction_id: "snapshot-registry-1",
                author_id: "dev-user",
                changed_entities: &changed_entities,
                structure_changed: false,
                origin: "system",
                commands_json: "[]",
                before_snapshot_key: "d1",
                events: &[],
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            list_artifact_snapshot_keys(&pool, "d1").await.unwrap(),
            vec!["d1", "d1-v2"]
        );
        assert!(referenced_blob_keys(&pool).await.unwrap().contains("d1-v2"));
    }

    #[tokio::test]
    async fn asset_references_commit_atomically_with_snapshot_pointer() {
        let pool = pool().await;
        insert_artifact(&pool, new_doc("d1", "x")).await.unwrap();
        let now = Utc::now();
        insert_artifact_asset(
            &pool,
            &ArtifactAsset {
                artifact_id: "d1".into(),
                asset_id: "asset-1".into(),
                object_key: "d1/assets/asset-1".into(),
                content_type: "image/png".into(),
                file_name: "image.png".into(),
                checksum: "checksum".into(),
                size: 3,
                ref_count: 0,
                created_at: now,
                updated_at: now,
            },
        )
        .await
        .unwrap();

        let changed_entities = Vec::new();
        let referenced = vec!["asset-1".to_string()];
        commit_artifact_transaction_with_assets(
            &pool,
            ArtifactTransactionCommit {
                id: "d1",
                expected_version: 1,
                snapshot_key: "d1-v2",
                transaction_id: "asset-tx-1",
                author_id: "dev-user",
                changed_entities: &changed_entities,
                structure_changed: true,
                origin: "local",
                commands_json: "[]",
                before_snapshot_key: "d1",
                events: &[],
            },
            &referenced,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            get_artifact_asset(&pool, "d1", "asset-1")
                .await
                .unwrap()
                .unwrap()
                .ref_count,
            1
        );

        let missing = vec!["missing".to_string()];
        assert!(commit_artifact_transaction_with_assets(
            &pool,
            ArtifactTransactionCommit {
                id: "d1",
                expected_version: 2,
                snapshot_key: "d1-v3",
                transaction_id: "asset-tx-2",
                author_id: "dev-user",
                changed_entities: &changed_entities,
                structure_changed: true,
                origin: "local",
                commands_json: "[]",
                before_snapshot_key: "d1-v2",
                events: &[],
            },
            &missing,
        )
        .await
        .is_err());
        assert_eq!(get_artifact(&pool, "d1").await.unwrap().unwrap().version, 2);
        assert_eq!(
            get_artifact_asset(&pool, "d1", "asset-1")
                .await
                .unwrap()
                .unwrap()
                .ref_count,
            1
        );
    }

    #[tokio::test]
    async fn committed_events_are_durable_and_delivery_is_idempotent() {
        let pool = pool().await;
        insert_artifact(&pool, new_doc("d1", "x")).await.unwrap();
        let changed_entities = vec!["b1".to_string()];
        let events = vec![test_event("tx-1:2:0")];
        let committed = commit_artifact_transaction(
            &pool,
            ArtifactTransactionCommit {
                id: "d1",
                expected_version: 1,
                snapshot_key: "d1-v2",
                transaction_id: "tx-1",
                author_id: "dev-user",
                changed_entities: &changed_entities,
                structure_changed: false,
                origin: "local",
                commands_json: "[]",
                before_snapshot_key: "d1",
                events: &events,
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(committed.version, 2);

        let pending = list_pending_domain_events(&pool, 10).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event, events[0]);
        assert_eq!(pending[0].attempts, 0);
        assert_eq!(pending_domain_event_count(&pool).await.unwrap(), 1);

        let claimed =
            claim_pending_domain_events(&pool, "worker-a", 10, chrono::Duration::minutes(1))
                .await
                .unwrap();
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].status, "processing");
        assert_eq!(claimed[0].claimed_by.as_deref(), Some("worker-a"));
        assert_eq!(claimed[0].attempts, 1);
        assert!(list_pending_domain_events(&pool, 10)
            .await
            .unwrap()
            .is_empty());

        let retry_at = Utc::now() - chrono::Duration::seconds(1);
        assert!(fail_domain_event(
            &pool,
            "tx-1:2:0",
            "worker-a",
            retry_at,
            "downstream unavailable",
        )
        .await
        .unwrap());
        let retried = list_pending_domain_events(&pool, 10).await.unwrap();
        assert_eq!(retried[0].attempts, 1);
        assert_eq!(
            retried[0].last_error.as_deref(),
            Some("downstream unavailable")
        );
        let claimed_again =
            claim_pending_domain_events(&pool, "worker-a", 10, chrono::Duration::minutes(1))
                .await
                .unwrap();
        assert_eq!(claimed_again[0].attempts, 2);
        assert!(ack_domain_event(&pool, "tx-1:2:0", "worker-a")
            .await
            .unwrap());
        assert!(!ack_domain_event(&pool, "tx-1:2:0", "worker-a")
            .await
            .unwrap());
        assert!(list_pending_domain_events(&pool, 10)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(pending_domain_event_count(&pool).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn event_claim_is_exclusive_and_expired_lease_is_reclaimed() {
        let pool = pool().await;
        insert_artifact(&pool, new_doc("d1", "x")).await.unwrap();
        let changed_entities = vec!["b1".to_string()];
        let events = vec![test_event("lease-event")];
        commit_artifact_transaction(
            &pool,
            ArtifactTransactionCommit {
                id: "d1",
                expected_version: 1,
                snapshot_key: "d1-v2",
                transaction_id: "lease-tx",
                author_id: "dev-user",
                changed_entities: &changed_entities,
                structure_changed: false,
                origin: "local",
                commands_json: "[]",
                before_snapshot_key: "d1",
                events: &events,
            },
        )
        .await
        .unwrap();

        let first = claim_pending_domain_events(&pool, "worker-a", 1, chrono::Duration::minutes(1))
            .await
            .unwrap();
        assert_eq!(first.len(), 1);
        assert!(
            claim_pending_domain_events(&pool, "worker-b", 1, chrono::Duration::minutes(1),)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(!ack_domain_event(&pool, "lease-event", "worker-b")
            .await
            .unwrap());

        sqlx::query(
            "UPDATE artifact_event_outbox SET lease_until = ? WHERE event_id = 'lease-event'",
        )
        .bind(Utc::now() - chrono::Duration::seconds(1))
        .execute(&pool)
        .await
        .unwrap();
        let reclaimed =
            claim_pending_domain_events(&pool, "worker-b", 1, chrono::Duration::minutes(1))
                .await
                .unwrap();
        assert_eq!(reclaimed.len(), 1);
        assert_eq!(reclaimed[0].claimed_by.as_deref(), Some("worker-b"));
        assert_eq!(reclaimed[0].attempts, 2);
        assert!(ack_domain_event(&pool, "lease-event", "worker-b")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn outbox_insert_failure_rolls_back_snapshot_and_event() {
        let pool = pool().await;
        insert_artifact(&pool, new_doc("d1", "x")).await.unwrap();
        let changed_entities = vec!["b1".to_string()];
        let duplicate = test_event("duplicate");
        let events = vec![duplicate.clone(), duplicate];
        let result = commit_artifact_transaction(
            &pool,
            ArtifactTransactionCommit {
                id: "d1",
                expected_version: 1,
                snapshot_key: "d1-v2",
                transaction_id: "tx-fails",
                author_id: "dev-user",
                changed_entities: &changed_entities,
                structure_changed: false,
                origin: "local",
                commands_json: "[]",
                before_snapshot_key: "d1",
                events: &events,
            },
        )
        .await;
        assert!(result.is_err(), "重复事件 id 必须让整个提交失败");
        assert_eq!(get_artifact(&pool, "d1").await.unwrap().unwrap().version, 1);
        assert_eq!(pending_domain_event_count(&pool).await.unwrap(), 0);
        assert!(get_artifact_transaction(&pool, "d1", "tx-fails")
            .await
            .unwrap()
            .is_none());
    }
}

// --- C4: per-artifact collaborators -----------------------------------------

/// 委派角色。owner 永远记录在 `artifacts.owner_id`，不进入本表。
pub const COLLABORATOR_ROLES: &[&str] = &["editor", "viewer"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collaborator {
    pub user_id: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

pub async fn list_collaborators(
    pool: &SqlitePool,
    artifact_id: &str,
) -> Result<Vec<Collaborator>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT user_id, role, created_at FROM artifact_collaborators \
         WHERE artifact_id = ? ORDER BY created_at, user_id",
    )
    .bind(artifact_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(Collaborator {
                user_id: row.try_get("user_id")?,
                role: row.try_get("role")?,
                created_at: DateTime::parse_from_rfc3339(&row.try_get::<String, _>("created_at")?)
                    .map_err(|error| sqlx::Error::ColumnDecode {
                        index: "created_at".into(),
                        source: Box::new(error),
                    })?
                    .with_timezone(&Utc),
            })
        })
        .collect()
}

/// 授予或更新一个协作者角色；角色合法性由路由层校验。
pub async fn upsert_collaborator(
    pool: &SqlitePool,
    artifact_id: &str,
    user_id: &str,
    role: &str,
) -> Result<(), sqlx::Error> {
    if !COLLABORATOR_ROLES.contains(&role) {
        panic!("upsert_collaborator 仅接受 {COLLABORATOR_ROLES:?}");
    }
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO artifact_collaborators (artifact_id, user_id, role, created_at) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT(artifact_id, user_id) DO UPDATE SET role = excluded.role",
    )
    .bind(artifact_id)
    .bind(user_id)
    .bind(role)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_collaborator(
    pool: &SqlitePool,
    artifact_id: &str,
    user_id: &str,
) -> Result<bool, sqlx::Error> {
    let result =
        sqlx::query("DELETE FROM artifact_collaborators WHERE artifact_id = ? AND user_id = ?")
            .bind(artifact_id)
            .bind(user_id)
            .execute(pool)
            .await?;
    Ok(result.rows_affected() > 0)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollaboratorList {
    pub collaborators: Vec<Collaborator>,
}
