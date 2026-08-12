//! 服务入口。
//!
//! 配置全部来自环境变量，默认值让 `cargo run -p oo-server` 开箱可用：
//! - `OO_BIND`：监听地址，默认 `127.0.0.1:8787`
//! - `OO_DATA_DIR`：数据目录，默认 `./data`
//! - `OO_DATABASE_URL`：SQLite 连接串，默认 `sqlite://<数据目录>/open-office.db`
//! - `RUST_LOG`：日志级别，默认 `info`

use std::sync::Arc;

use oo_server::store::{remove_unreferenced, LocalFsStore};
use oo_server::{build_router, db, AppState};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let bind = std::env::var("OO_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let data_dir = std::env::var("OO_DATA_DIR").unwrap_or_else(|_| "./data".into());
    let database_url = std::env::var("OO_DATABASE_URL")
        .unwrap_or_else(|_| format!("sqlite://{data_dir}/open-office.db"));

    tokio::fs::create_dir_all(&data_dir).await?;

    let pool = db::connect(&database_url).await?;
    let store = Arc::new(LocalFsStore::new(format!("{data_dir}/blobs")).await?);
    // SQL migration 无法读取文件系统里的旧历史对象，默认不猜测对象生命周期。
    // 完成离线历史登记/备份演练后，以 OO_ENABLE_BLOB_GC=1 显式开启回收。
    if matches!(
        std::env::var("OO_ENABLE_BLOB_GC").as_deref(),
        Ok("1") | Ok("true")
    ) {
        let referenced = db::referenced_blob_keys(&pool).await?;
        let removed = remove_unreferenced(store.as_ref(), &referenced).await?;
        tracing::info!(removed, "已完成未引用 Blob 回收");
    } else {
        tracing::info!("未执行未引用 Blob 回收；完成离线历史登记后设置 OO_ENABLE_BLOB_GC=1");
    }
    let app = build_router(AppState {
        pool,
        store,
        write_lock: Arc::new(tokio::sync::Mutex::new(())),
        presence: Arc::new(tokio::sync::Mutex::new(
            oo_server::presence::PresenceStore::default(),
        )),
    });

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, %data_dir, "open-office API 已启动");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        tracing::error!(error = %e, "无法监听终止信号");
    }
    tracing::info!("正在关闭……");
}
