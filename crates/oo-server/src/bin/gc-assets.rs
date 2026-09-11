//! Offline garbage collector for verified Artifact assets.
//!
//! It is intentionally opt-in and grace-period based. Run it while writers are
//! stopped; normal request handling never guesses that a freshly uploaded,
//! currently unreferenced asset is abandoned.

use std::sync::Arc;

use chrono::{Duration, Utc};
use oo_server::db;
use oo_server::store::{BlobStore, LocalFsStore};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("OO_ENABLE_ASSET_GC").as_deref() != Ok("1") {
        return Err("refusing asset GC; set OO_ENABLE_ASSET_GC=1 after stopping writers".into());
    }
    let data_dir = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("OO_DATA_DIR").ok())
        .unwrap_or_else(|| "./data".into());
    let grace_hours = std::env::args()
        .nth(2)
        .map(|value| value.parse::<i64>())
        .transpose()?
        .unwrap_or(24);
    if !(1..=24 * 365).contains(&grace_hours) {
        return Err("grace hours must be between 1 and 8760".into());
    }

    let database_url = format!("sqlite://{data_dir}/open-office.db");
    let pool = db::connect(&database_url).await?;
    let store: Arc<dyn BlobStore> = Arc::new(LocalFsStore::new(format!("{data_dir}/blobs")).await?);
    let cutoff = Utc::now() - Duration::hours(grace_hours);
    let candidates = db::list_unreferenced_artifact_assets_before(&pool, cutoff, 10_000).await?;
    let mut deleted = 0_usize;
    for asset in candidates {
        if !db::delete_artifact_asset(&pool, &asset.artifact_id, &asset.asset_id).await? {
            continue;
        }
        if let Err(error) = store.delete(&asset.object_key).await {
            eprintln!(
                "asset metadata removed but blob cleanup failed for {}: {error}",
                asset.object_key
            );
            continue;
        }
        deleted += 1;
    }
    println!("deleted {deleted} unreferenced assets older than {grace_hours} hours");
    Ok(())
}
