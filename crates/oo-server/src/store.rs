//! 二进制内容的存储抽象。
//!
//! 当前只有本地文件系统实现，够单机开发用。抽象成 trait 是为了后面换成 S3 /
//! MinIO 时，改动只发生在这个文件里——服务层拿到的始终是 `Arc<dyn BlobStore>`。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("对象 {0} 不存在")]
    NotFound(String),

    #[error("存储读写失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("非法的对象键：{0}")]
    InvalidKey(String),

    #[error("对象 {0} 校验和不匹配：期望 {1}，实际 {2}")]
    ChecksumMismatch(String, String, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobDigest {
    pub checksum: String,
    pub size: u64,
}

pub fn checksum(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

pub async fn put_verified(
    store: &dyn BlobStore,
    key: &str,
    bytes: &[u8],
) -> Result<BlobDigest, StoreError> {
    store.put(key, bytes).await?;
    Ok(BlobDigest {
        checksum: checksum(bytes),
        size: bytes.len() as u64,
    })
}

pub async fn get_verified(
    store: &dyn BlobStore,
    key: &str,
    expected_checksum: &str,
) -> Result<Vec<u8>, StoreError> {
    let bytes = store.get(key).await?;
    let actual = checksum(&bytes);
    if !expected_checksum.is_empty() && actual != expected_checksum {
        return Err(StoreError::ChecksumMismatch(
            key.into(),
            expected_checksum.into(),
            actual,
        ));
    }
    Ok(bytes)
}

#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError>;
    async fn get(&self, key: &str) -> Result<Vec<u8>, StoreError>;
    async fn delete(&self, key: &str) -> Result<(), StoreError>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError>;
}

/// 删除数据库没有引用的对象。启动时调用一次即可回收崩溃或版本竞争留下的孤儿文件；
/// 引用集合由数据库事务生成，存储层不自行猜测哪些 snapshot 仍有业务意义。
pub async fn remove_unreferenced(
    store: &dyn BlobStore,
    referenced: &HashSet<String>,
) -> Result<usize, StoreError> {
    let mut removed = 0;
    for key in store.list("").await? {
        if !referenced.contains(&key) {
            store.delete(&key).await?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// 把对象存成本地目录下的文件。
pub struct LocalFsStore {
    root: PathBuf,
}

impl LocalFsStore {
    pub async fn new(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        tokio::fs::create_dir_all(&root).await?;
        Ok(Self { root })
    }

    /// 把对象键解析成路径。
    ///
    /// 键会直接拼进路径，因此必须挡住 `..` 和绝对路径——否则一个精心构造的键就能
    /// 读写存储目录之外的文件。
    fn path_for(&self, key: &str) -> Result<PathBuf, StoreError> {
        if key.is_empty()
            || key.contains("..")
            || key.starts_with('/')
            || key.contains('\\')
            || Path::new(key).is_absolute()
        {
            return Err(StoreError::InvalidKey(key.to_string()));
        }
        Ok(self.root.join(key))
    }

    /// 离线历史登记使用文件 mtime 作为近似创建时间；在线写入路径仍以数据库事务时间为准。
    pub async fn modified_at(&self, key: &str) -> Result<DateTime<Utc>, StoreError> {
        let modified = tokio::fs::metadata(self.path_for(key)?).await?.modified()?;
        Ok(DateTime::<Utc>::from(modified))
    }
}

#[async_trait]
impl BlobStore for LocalFsStore {
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let path = self.path_for(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        // 写临时文件后在同一目录内 rename，避免进程崩溃留下被数据库指针引用的半文件。
        // 临时键不通过 path_for 解析，目标仍然已经过 key 校验且位于 store root 下。
        let temp_path = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
        let mut temp_file = match tokio::fs::File::create(&temp_path).await {
            Ok(file) => file,
            Err(error) => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(error.into());
            }
        };
        use tokio::io::AsyncWriteExt;
        if let Err(error) = temp_file.write_all(bytes).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(error.into());
        }
        if let Err(error) = temp_file.sync_all().await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(error.into());
        }
        drop(temp_file);
        if let Err(error) = tokio::fs::rename(&temp_path, &path).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(error.into());
        }
        // Persist the directory entry as well as file bytes where the platform
        // supports directory fsync. A failure is surfaced instead of reporting
        // a successful pointer commit that may disappear after a crash.
        if let Some(parent) = path.parent() {
            if let Ok(directory) = std::fs::File::open(parent) {
                directory.sync_all()?;
            }
        }
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, StoreError> {
        let path = self.path_for(key)?;
        match tokio::fs::read(&path).await {
            Ok(bytes) => Ok(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(StoreError::NotFound(key.to_string()))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        let path = self.path_for(key)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            // 删除不存在的对象视作成功，让删除操作可以安全重试。
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        if prefix.contains("..") || prefix.starts_with('/') || prefix.contains('\\') {
            return Err(StoreError::InvalidKey(prefix.to_string()));
        }
        let mut pending = vec![self.root.clone()];
        let mut keys = Vec::new();
        while let Some(directory) = pending.pop() {
            let mut entries = tokio::fs::read_dir(&directory).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let file_type = entry.file_type().await?;
                if file_type.is_dir() {
                    pending.push(path);
                    continue;
                }
                let key = path
                    .strip_prefix(&self.root)
                    .map_err(|error| StoreError::Io(std::io::Error::other(error)))?
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/");
                if key.starts_with(prefix) {
                    keys.push(key);
                }
            }
        }
        keys.sort();
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn temp_store() -> (LocalFsStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("oo-store-{}", uuid::Uuid::new_v4()));
        let store = LocalFsStore::new(&dir).await.unwrap();
        (store, dir)
    }

    #[tokio::test]
    async fn put_then_get_roundtrips() {
        let (store, dir) = temp_store().await;
        store.put("docs/a.bin", b"hello").await.unwrap();
        assert_eq!(store.get("docs/a.bin").await.unwrap(), b"hello");
        tokio::fs::remove_dir_all(dir).await.ok();
    }

    #[tokio::test]
    async fn overwrite_is_atomic_and_leaves_no_temp_files() {
        let (store, dir) = temp_store().await;
        store.put("docs/a.bin", b"first").await.unwrap();
        store.put("docs/a.bin", b"second").await.unwrap();
        assert_eq!(store.get("docs/a.bin").await.unwrap(), b"second");

        let mut entries = tokio::fs::read_dir(dir.join("docs")).await.unwrap();
        let mut names = Vec::new();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            names.push(entry.file_name());
        }
        assert_eq!(names, vec![std::ffi::OsString::from("a.bin")]);
        tokio::fs::remove_dir_all(dir).await.ok();
    }

    #[tokio::test]
    async fn missing_object_reports_not_found() {
        let (store, dir) = temp_store().await;
        let err = store.get("nope").await.unwrap_err();
        assert!(matches!(err, StoreError::NotFound(_)));
        tokio::fs::remove_dir_all(dir).await.ok();
    }

    #[tokio::test]
    async fn delete_is_idempotent() {
        let (store, dir) = temp_store().await;
        store.put("x", b"1").await.unwrap();
        store.delete("x").await.unwrap();
        store.delete("x").await.unwrap();
        tokio::fs::remove_dir_all(dir).await.ok();
    }

    #[tokio::test]
    async fn path_traversal_keys_are_rejected() {
        let (store, dir) = temp_store().await;
        for key in ["../escape", "/etc/passwd", "a/../../b", ""] {
            assert!(
                matches!(store.put(key, b"x").await, Err(StoreError::InvalidKey(_))),
                "键 {key:?} 本应被拒绝"
            );
        }
        tokio::fs::remove_dir_all(dir).await.ok();
    }

    #[tokio::test]
    async fn list_and_reconcile_remove_only_unreferenced_objects() {
        let (store, dir) = temp_store().await;
        store.put("docs/keep", b"keep").await.unwrap();
        store.put("docs/orphan", b"orphan").await.unwrap();
        let listed = store.list("docs/").await.unwrap();
        assert_eq!(listed, vec!["docs/keep", "docs/orphan"]);
        let referenced = HashSet::from([String::from("docs/keep")]);
        assert_eq!(remove_unreferenced(&store, &referenced).await.unwrap(), 1);
        assert_eq!(store.get("docs/keep").await.unwrap(), b"keep");
        assert!(matches!(
            store.get("docs/orphan").await,
            Err(StoreError::NotFound(_))
        ));
        tokio::fs::remove_dir_all(dir).await.ok();
    }
}
